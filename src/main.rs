#![no_std]
#![no_main]
#![feature(sync_unsafe_cell)]
#![feature(decl_macro)]

mod arch;
mod mem;
mod tty;

use arch::halt;
use core::cell::UnsafeCell;
use limine::BaseRevision;
use limine::firmware_type::FirmwareType;
use limine::request::{
    BootloaderInfoRequest, ExecutableCmdlineRequest, FirmwareTypeRequest, FramebufferRequest,
    ModuleRequest, MpRequest, RequestsEndMarker, RequestsStartMarker, RsdpRequest, SmbiosRequest,
};
use static_cell::StaticCell;
use tty::{FlanTermTTY, print_grouped, println};

#[used]
#[unsafe(link_section = ".limine_requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(4);

#[used]
#[unsafe(link_section = ".limine_requests")]
static BOOTLOADER_INFO_REQUEST: BootloaderInfoRequest = BootloaderInfoRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static CMDLINE_REQUEST: ExecutableCmdlineRequest = ExecutableCmdlineRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static FIRMWARE_TYPE_REQUEST: FirmwareTypeRequest = FirmwareTypeRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static MP_REQUEST: MpRequest = MpRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static MODULE_REQUEST: ModuleRequest = ModuleRequest::new();

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

static FLANTERM_TTY: StaticCell<UnsafeCell<FlanTermTTY>> = StaticCell::new();

fn dump_boot_info() {
    if let Some(res) = BOOTLOADER_INFO_REQUEST.get_response() {
        println!("kmain(): bootloader: {} v{}", res.name(), res.version());
    }

    if let Some(res) = CMDLINE_REQUEST.get_response() {
        println!("kmain(): cmdline: \"{}\"", res.cmdline().to_str().unwrap());
    }

    if let Some(res) = FIRMWARE_TYPE_REQUEST.get_response() {
        println!(
            "kmain(): firmware: {}",
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
    let fb = &FRAMEBUFFER_REQUEST
        .get_response()
        .unwrap()
        .framebuffers()
        .next()
        .unwrap();

    let tty = FlanTermTTY::from_framebuffer(fb);
    let cell_ref = FLANTERM_TTY.init(UnsafeCell::new(tty));

    let tty_mut = unsafe { &mut *cell_ref.get() };

    tty::set_handler(tty_mut);

    println!("kmain(): tty initialized");
    println!("kmain(): framebuffer: {}x{}", fb.width(), fb.height());

    dump_boot_info();
    mem::init();

    halt();
}

#[panic_handler]
fn rust_panic(info: &core::panic::PanicInfo) -> ! {
    print_grouped(|mut printer| {
        printer.println(format_args!("panic: {}", info.message()));
        match info.location() {
            Some(location) => printer.println(format_args!(
                "at {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            )),
            None => printer.println(format_args!("at unknown location")),
        };
    });

    halt()
}
