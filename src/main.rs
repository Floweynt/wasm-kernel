#![no_std]
#![no_main]
#![feature(lang_items)]
mod arch;
mod tty;

use arch::halt;
use tty::{FlanTermTTY, TTYHandler};

use limine::BaseRevision;
use limine::request::{FramebufferRequest, RequestsEndMarker, RequestsStartMarker};

#[used]
#[unsafe(link_section = ".limine_requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(4);

#[used]
#[unsafe(link_section = ".limine_requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests_start")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used]
#[unsafe(link_section = ".limine_requests_end")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // All limine requests must also be referenced in a called function, otherwise they may be
    // removed by the linker.
    assert!(BASE_REVISION.is_supported());

    let tty = FlanTermTTY::from_framebuffer(
        &FRAMEBUFFER_REQUEST
            .get_response()
            .unwrap()
            .framebuffers()
            .next()
            .unwrap(),
    );

    tty.putc('h');
    tty.putc('e');
    tty.putc('l');
    tty.putc('l');
    tty.putc('o');
    tty.putc('\n');

    halt();
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    halt()
}

#[lang = "eh_personality"]
extern "C" fn rust_eh_personality() -> ! {
    halt()
}
