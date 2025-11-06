use anyhow::{Error, Result, bail};
use cargo_metadata::Message;
use clap::{Parser, Subcommand};
use fatfs::{FatType, FileSystem, FormatVolumeOptions, FsOptions, format_volume};
use fscommon::StreamSlice;
use gptman::{GPT, GPTPartitionEntry};
use reqwest::blocking;
use std::env::current_dir;
use std::fs::{File, create_dir_all, exists, metadata, rename};
use std::io::{BufReader, Write, copy};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;
use uuid::Uuid;

const LIMINE_URL: &str =
    "https://github.com/limine-bootloader/limine/raw/refs/heads/v10.x-binary/BOOTX64.EFI";
const KERNEL_IMG_PATH: &str = "kernel.img";
const LIMINE_CONF: &str = "limine.conf";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Image,
    Qemu {
        #[arg(long)]
        kvm: bool,
        #[arg(short = 'j', long, default_value_t = 1)]
        cores: u8,
        #[arg(short, long, default_value_t = 4)]
        mem: u8,
    },
    Gdb {
        #[arg(long)]
        kvm: bool,
    },
}

fn cache_dir() -> Result<PathBuf> {
    let root = current_dir()?.join("buildtool-cache");
    create_dir_all(&root)?;
    Ok(root)
}

fn resources_dir() -> Result<PathBuf> {
    let root = current_dir()?.join("resources");
    Ok(root)
}

fn run_dir() -> Result<PathBuf> {
    let root = current_dir()?.join("run");
    create_dir_all(&root)?;
    Ok(root)
}

fn download_limine() -> Result<PathBuf> {
    let root = cache_dir()?;
    let limine_path = root.join("limine.efi");

    if !limine_path.exists() {
        let response = blocking::get(LIMINE_URL)?;
        let mut dest = File::create(&limine_path)?;
        let content = response.bytes()?;
        copy(&mut content.as_ref(), &mut dest)?;
    }

    Ok(limine_path)
}

fn build_kernel() -> Result<PathBuf> {
    let mut cmd = Command::new("cargo")
        .args(&["build", "--message-format=json", "--target", "x86_64-unknown-none"])
        .env("RUSTFLAGS", "-C relocation-model=static")
        .stdout(Stdio::piped())
        .spawn()?;

    let stdout = cmd.stdout.take().expect("Failed to capture cargo stdout");
    let reader = BufReader::new(stdout);

    for message in Message::parse_stream(reader) {
        match message? {
            // TODO: check package
            Message::CompilerArtifact(artifact) => {
                if let Some(executable) = artifact.executable {
                    println!("kernel image path: {}", executable);
                    return Ok(PathBuf::from(executable));
                }
            }
            Message::CompilerMessage(message) => {
                println!("{}", message)
            }
            _ => {}
        }
    }

    bail!("kernel binary not found")
}

fn build_image(kernel_elf: &PathBuf) -> Result<()> {
    let cache_dir = cache_dir()?;
    let limine_efi = download_limine()?;
    let limine_cfg = resources_dir()?.join(LIMINE_CONF);
    let output_img = cache_dir.join(KERNEL_IMG_PATH);

    if !exists(&output_img)?
        || metadata(&kernel_elf)?.modified()? > metadata(&output_img)?.modified()?
        || metadata(&limine_efi)?.modified()? > metadata(&output_img)?.modified()?
        || metadata(&limine_cfg)?.modified()? > metadata(&output_img)?.modified()?
    {
        println!("rebuilding image");

        let temp_img_out = NamedTempFile::new_in(cache_dir)?;
        let mut output_file = temp_img_out.as_file();

        output_file.set_len(64 * 1024 * 1024)?;

        let disk_guid = *Uuid::new_v4().as_bytes();
        let sector_size = 512;
        GPT::write_protective_mbr_into(&mut output_file, sector_size)?;
        let mut gpt = GPT::new_from(&mut output_file, sector_size, disk_guid)?;
        let start_lba = gpt.header.first_usable_lba;
        let end_lba = gpt.header.last_usable_lba;

        gpt[1] = GPTPartitionEntry {
            partition_type_guid: *Uuid::parse_str("c12a7328-f81f-11d2-ba4b-00a0c93ec93b")?
                .as_bytes(),
            unique_partition_guid: *Uuid::new_v4().as_bytes(),
            starting_lba: start_lba,
            ending_lba: end_lba,
            attribute_bits: 0,
            partition_name: "EFI System".into(),
        };

        gpt.write_into(&mut output_file)?;

        let mut slice = StreamSlice::new(
            &mut output_file,
            start_lba * sector_size,
            end_lba * sector_size,
        )?;

        format_volume(
            &mut slice,
            FormatVolumeOptions::new().fat_type(FatType::Fat32),
        )?;

        let fs = FileSystem::new(&mut slice, FsOptions::new())?;

        fs.root_dir().create_dir("efi")?;
        fs.root_dir().create_dir("efi/boot")?;

        copy(
            &mut File::open(limine_efi)?,
            &mut fs.root_dir().create_file("efi/boot/bootx64.efi")?,
        )?;
        copy(
            &mut File::open(limine_cfg)?,
            &mut fs.root_dir().create_file(LIMINE_CONF)?,
        )?;
        copy(
            &mut File::open(kernel_elf)?,
            &mut fs.root_dir().create_file("kernel.elf")?,
        )?;

        fs.unmount()?;

        output_file.flush()?;

        rename(temp_img_out.path(), output_img)?;
    }

    Ok(())
}

fn exec<T: std::fmt::Debug + AsRef<std::ffi::OsStr>>(command: &str, args: Vec<T>) -> Result<()> {
    println!("running: {} {:?}", command, args);
    let err = Command::new(command)
        .args(args)
        .current_dir(run_dir()?)
        .exec();
    Err(err.into())
}

fn qemu(kvm: bool, cores: u8, mem_g: u8) -> Result<()> {
    build_image(&build_kernel()?)?;

    let mut args = vec![
        "-bios".to_string(),
        resources_dir()?
            .join("OVMF.fd")
            .canonicalize()?
            .to_str()
            .ok_or(Error::msg("bad ovmf path"))?
            .to_string(),
        "-hda".to_string(),
        cache_dir()?
            .join(KERNEL_IMG_PATH)
            .canonicalize()?
            .to_str()
            .ok_or(Error::msg("bad image path"))?
            .to_string(),
        "-no-reboot".to_string(),
        "-monitor".to_string(),
        "stdio".to_string(),
        "-d".to_string(),
        "int,cpu_reset".to_string(),
        "-D".to_string(),
        "qemu.log".to_string(),
        "-no-shutdown".to_string(),
        "-s".to_string(),
        "-S".to_string(),
        "-M".to_string(),
        "smm=off".to_string(),
        "-m".to_string(),
        format!("{}G", mem_g).to_string(),
        "-smp".to_string(),
        format!("{}", cores),
    ];

    if kvm {
        args.push("-enable-kvm".to_string());
        args.push("-cpu".to_string());
        args.push("host".to_string());
    }

    exec("qemu-system-x86_64", args)
}

fn gdb(kvm: bool) -> Result<()> {
    let kernel_elf = build_kernel()?;
    build_image(&kernel_elf)?;

    let gdb_args;

    if kvm {
        gdb_args = vec!["target remote localhost:1234", "hbreak kinit", "c"]
    } else {
        gdb_args = vec!["target remote localhost:1234", "b kinit", "c"]
    }

    let kernel_elf_abs = kernel_elf.canonicalize()?;
    let file = kernel_elf_abs
        .to_str()
        .ok_or(Error::msg("bad kernel elf path"))?;

    let mut args = vec![file];

    for ent in gdb_args {
        args.push("-ex");
        args.push(ent);
    }

    exec("gdb", args)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Image => build_image(&build_kernel()?)?,
        Commands::Qemu { kvm, cores, mem } => qemu(kvm, cores, mem)?,
        Commands::Gdb { kvm } => gdb(kvm)?,
    }

    Ok(())
}
