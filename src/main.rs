#![no_std]
#![no_main]
#![feature(sync_unsafe_cell)]
#![feature(decl_macro)]
#![feature(const_range)]
#![feature(const_trait_impl)]
#![feature(stmt_expr_attributes)]
#![feature(assert_matches)]
#![feature(step_trait)]
#![feature(iter_map_windows)]
#![feature(unsafe_cell_access)]
#![feature(associated_type_defaults)]
#![feature(const_ops)]
#![feature(generic_atomic)]
#![feature(const_default)]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

mod arch;
mod cmdline;
mod log;
mod mem;
mod modules;
mod mp;
mod sync;

use ::log::{info, warn};
use arch::halt;
use arch::mp::initialize_mp;
use cmdline::{get_cmdline_error, get_cmdline_text, parse_kernel_cmdline};
use limine::BaseRevision;
use limine::firmware_type::FirmwareType;
use limine::request::{
    BootloaderInfoRequest, FirmwareTypeRequest, RequestsEndMarker, RequestsStartMarker,
    RsdpRequest, SmbiosRequest,
};
use log::{StackTrace, init_tty};
use modules::load_modules_early;

#[used]
#[unsafe(link_section = ".limine_requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(4);

#[used]
#[unsafe(link_section = ".limine_requests")]
static BOOTLOADER_INFO_REQUEST: BootloaderInfoRequest = BootloaderInfoRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static FIRMWARE_TYPE_REQUEST: FirmwareTypeRequest = FirmwareTypeRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static SMBIOS_REQUEST: SmbiosRequest = SmbiosRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests_start")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

fn dump_boot_info() {
    if let Some(res) = BOOTLOADER_INFO_REQUEST.get_response() {
        info!("bootloader: {} v{}", res.name(), res.version());
    }

    if let Some(res) = get_cmdline_text() {
        info!("cmdline: \"{}\"", res);
    }

    if let Some(err) = get_cmdline_error() {
        match err {
            cmdline::CmdlineError::NoResponse => warn!("no response received for cmdline request"),
            cmdline::CmdlineError::Utf8Error(err) => {
                warn!("failed to convert cmdline to utf8: {}", err)
            }
            cmdline::CmdlineError::ParseError(err) => warn!("failed to parse cmdline: {}", err),
        }
    }

    if let Some(res) = FIRMWARE_TYPE_REQUEST.get_response() {
        info!(
            "firmware: {}",
            match res.firmware_type() {
                FirmwareType::X86_BIOS => "bios",
                FirmwareType::UEFI_32 => "efi_32",
                FirmwareType::UEFI_64 => "efi_64",
                FirmwareType::SBI => "sbi",
                _ => "unknown",
            }
        );
    }

    mem::dump_memory_info();
}

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    parse_kernel_cmdline();
    init_tty();
    load_modules_early();
    dump_boot_info();

    let addr_space = mem::init();

    initialize_mp(&addr_space);
}

pub extern "C" fn ksmp() -> ! {
    info!("hello from ksmp: {}", StackTrace::current());
    info!("i did not halt!");
    halt();
}

#[cfg(not(test))]
#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    use ::log::error;
    use arch::halt;
    use log::StackTrace;

    match info.location() {
        Some(location) => error!(
            "panic: {}\nat {}:{}:{}\n{}",
            info.message(),
            location.file(),
            location.line(),
            location.column(),
            StackTrace::current()
        ),
        None => error!(
            "panic: {}\nat unknown location\n{}",
            info.message(),
            StackTrace::current()
        ),
    };

    halt()
}
