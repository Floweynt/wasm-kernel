#![no_std]
#![no_main]
#![feature(sync_unsafe_cell)]
#![feature(decl_macro)]

mod arch;
mod tty;

use arch::halt;
use core::cell::UnsafeCell;
use core::panic;
use limine::BaseRevision;
use limine::request::{
    FramebufferRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker,
};
use static_cell::StaticCell;
use tty::{FlanTermTTY, print_grouped, println};

#[used]
#[unsafe(link_section = ".limine_requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(4);

#[used]
#[unsafe(link_section = ".limine_requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests_start")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

static FLANTERM_TTY: StaticCell<UnsafeCell<FlanTermTTY>> = StaticCell::new();

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

    println!("hello, world!");
    println!("1");
    println!("2");
    panic!("meow");

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
