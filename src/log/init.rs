use super::flanterm::FlanTermTTY;
use crate::{arch::SerialCharSink, cmdline::get_cmdline, log::CharSink};
use limine::request::FramebufferRequest;
use log::{LevelFilter, info, set_logger};
use spin::{Once, mutex::Mutex};

use super::log::LogImpl;

#[used]
#[unsafe(link_section = ".limine_requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

static LOGGER: Once<LogImpl> = Once::new();

static SERIAL: Once<SerialCharSink> = Once::new();

static FLANTERM: Once<FlanTermTTY> = Once::new();

pub fn init_tty() {
    let mut serial: Option<&'static dyn CharSink> = None;
    let mut framebuffer: Option<&'static dyn CharSink> = None;

    if get_cmdline().logging.serial.enable {
        serial = Some(SERIAL.call_once(|| SerialCharSink::open(get_cmdline().logging.serial.port)));
    }

    if let Some(res) = FRAMEBUFFER_REQUEST.get_response()
        && let Some(ref fb) = res.framebuffers().next()
    {
        framebuffer = Some(FLANTERM.call_once(|| FlanTermTTY::from_framebuffer(fb)));
    }

    set_logger(LOGGER.call_once(|| LogImpl {
        lock: Mutex::new(()),
        serial,
        framebuffer,
    }))
    .map(|()| log::set_max_level(LevelFilter::Trace))
    .unwrap();

    info!("kmain(): tty initialized");

    if let Some(res) = FRAMEBUFFER_REQUEST.get_response()
        && let Some(ref fb) = res.framebuffers().next()
    {
        info!("kmain(): framebuffer: {}x{}", fb.width(), fb.height());
    }
}
