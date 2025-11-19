#![feature(decl_macro)]

use anyhow::{Error, Result};
use cargo_metadata::Message;
use clap::{Parser, Subcommand};
use elf::ElfBytes;
use elf::endian::AnyEndian;
use fatfs::{FatType, FileSystem, FormatVolumeOptions, FsOptions, format_volume};
use fscommon::StreamSlice;
use gptman::{GPT, GPTPartitionEntry};
use reqwest::blocking;
use std::env::current_dir;
use std::fs::{self, File, create_dir_all, exists, metadata, rename, write};
use std::io::{BufReader, Write, copy};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::NamedTempFile;
use uuid::Uuid;

const LIMINE_URL: &str =
    "https://github.com/limine-bootloader/limine/raw/refs/heads/v10.x-binary/BOOTX64.EFI";
const LIMINE_CONF: &str = "limine.conf";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Image {
        #[arg(long)]
        release: bool,
    },
    Qemu {
        #[arg(long)]
        kvm: bool,
        #[arg(short = 'j', long, default_value_t = 1)]
        cores: u8,
        #[arg(short, long, default_value_t = 4)]
        mem: u8,
        #[arg(long)]
        release: bool,
    },
    Gdb {
        #[arg(long)]
        kvm: bool,
        #[arg(long)]
        release: bool,
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

fn build_kernel(release: bool) -> Result<PathBuf> {
    let mut args = vec![
        "build",
        "--message-format=json-render-diagnostics",
        "--target",
        "x86_64-unknown-none",
        "-Zbuild-std=core,alloc",
    ];

    if release {
        args.push("--release");
    }

    let mut cmd = Command::new("cargo")
        .args(args)
        .env(
            "RUSTFLAGS",
            "-C relocation-model=static -C force-frame-pointers=yes",
        )
        .stdout(Stdio::piped())
        .spawn()?;

    let stdout = cmd.stdout.take().expect("Failed to capture cargo stdout");
    let reader = BufReader::new(stdout);

    let mut res = None;

    for message in Message::parse_stream(reader) {
        match message? {
            // TODO: check package
            Message::CompilerArtifact(artifact) => {
                if let Some(executable) = artifact.executable {
                    println!("kernel binary path: {}", executable);
                    res = Some(PathBuf::from(executable));
                }
            }
            _ => {}
        }
    }
    Ok(res.ok_or(Error::msg("failed to locate executable"))?)
}

fn path_to_string(path: &PathBuf) -> Result<String> {
    Ok(path
        .canonicalize()?
        .to_str()
        .ok_or(Error::msg("bad path"))?
        .to_string())
}

fn split_debug_info(elf: &PathBuf) -> Result<(Vec<u8>, Vec<u8>)> {
    let cache = cache_dir()?;
    let tmp_stripped = NamedTempFile::new_in(&cache)?;
    let tmp_debug = NamedTempFile::new_in(&cache)?;

    let elf_abs = path_to_string(&elf)?;

    Command::new("strip")
        .args([
            elf_abs.clone(),
            "-o".to_owned(),
            path_to_string(&tmp_stripped.path().to_path_buf())?,
        ])
        .spawn()?
        .wait()?;

    Command::new("strip")
        .args([
            elf_abs,
            "-o".to_owned(),
            path_to_string(&tmp_debug.path().to_path_buf())?,
            "--only-keep-debug".to_owned(),
        ])
        .spawn()?
        .wait()?;

    Ok((fs::read(tmp_stripped)?, fs::read(tmp_debug)?))
}

fn gen_debug_module(debug_elf: Vec<u8>) -> Result<Vec<u8>> {
    let file = ElfBytes::<AnyEndian>::minimal_parse(&debug_elf)?;

    let table = file.symbol_table()?;

    // TODO: parse symbol table

    todo!()
}

fn build_image(kernel_elf: &PathBuf, release: bool) -> Result<PathBuf> {
    let cache_dir = cache_dir()?;
    let limine_efi = download_limine()?;
    let limine_cfg = resources_dir()?.join(LIMINE_CONF);
    let output_img = cache_dir.join(format!(
        "kernel-{}.img",
        if release { "release" } else { "debug" }
    ));

    if !exists(&output_img)?
        || metadata(&kernel_elf)?.modified()? > metadata(&output_img)?.modified()?
        || metadata(&limine_efi)?.modified()? > metadata(&output_img)?.modified()?
        || metadata(&limine_cfg)?.modified()? > metadata(&output_img)?.modified()?
    {
        println!(
            "rebuilding image: {}",
            output_img
                .to_str()
                .ok_or(Error::msg("could not convert image file"))?
        );

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

        let (elf_data, debug_data) = split_debug_info(kernel_elf)?;

        fs.root_dir()
            .create_file("kernel.elf")?
            .write_all(&elf_data)?;

        fs.unmount()?;

        output_file.flush()?;

        rename(temp_img_out.path(), &output_img)?;
    }

    Ok(output_img)
}

fn exec<T: std::fmt::Debug + AsRef<std::ffi::OsStr>>(command: &str, args: Vec<T>) -> Result<()> {
    println!("running: {} {:?}", command, args);
    let err = Command::new(command)
        .args(args)
        .current_dir(run_dir()?)
        .exec();
    Err(err.into())
}

fn qemu(kvm: bool, cores: u8, mem_g: u8, release: bool) -> Result<()> {
    let path = build_image(&build_kernel(release)?, release)?;

    let mut args = vec![
        "-bios".to_owned(),
        path_to_string(&resources_dir()?.join("OVMF.fd"))?,
        "-hda".to_owned(),
        path_to_string(&path)?,
        "-no-reboot".to_owned(),
        "-monitor".to_owned(),
        "stdio".to_owned(),
        "-d".to_owned(),
        "int,cpu_reset".to_owned(),
        "-D".to_owned(),
        "qemu.log".to_owned(),
        "-no-shutdown".to_owned(),
        "-s".to_owned(),
        "-S".to_owned(),
        "-M".to_owned(),
        "smm=off".to_owned(),
        "-m".to_owned(),
        format!("{}G", mem_g).to_owned(),
        "-smp".to_owned(),
        format!("{}", cores),
        "-vga".to_owned(),
        "std".to_owned(),
        "-serial".to_owned(),
        format!("file:{}/serial.txt", path_to_string(&run_dir()?)?),
    ];

    if kvm {
        args.push("-enable-kvm".to_owned());
        args.push("-cpu".to_owned());
        args.push("host".to_owned());
    }

    exec("qemu-system-x86_64", args)
}

fn gdb(kvm: bool, release: bool) -> Result<()> {
    let kernel_elf = build_kernel(release)?;

    let gdb_args;

    if kvm {
        gdb_args = vec!["target remote localhost:1234", "hbreak kmain", "c"]
    } else {
        gdb_args = vec!["target remote localhost:1234", "b kmain", "c"]
    }

    let kernel_elf_abs = kernel_elf.canonicalize()?;
    let file = kernel_elf_abs
        .to_str()
        .ok_or(Error::msg("bad kernel elf path"))?;

    let mut args = vec![path_to_string(&kernel_elf)?];

    for ent in gdb_args {
        args.push("-ex".to_owned());
        args.push(ent.to_owned());
    }

    exec("gdb", args)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Image { release } => {
            build_image(&build_kernel(release)?, release)?;
        }
        Commands::Qemu {
            kvm,
            cores,
            mem,
            release,
        } => qemu(kvm, cores, mem, release)?,
        Commands::Gdb { kvm, release } => gdb(kvm, release)?,
    }

    Ok(())
}
