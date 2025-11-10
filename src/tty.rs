use core::fmt::{self, Error, Result, Write};
use core::ptr;
use flanterm::{
    flanterm_context, flanterm_fb_init, flanterm_flush, flanterm_set_autoflush, flanterm_write,
};
use limine::framebuffer::Framebuffer;
use spin::Mutex;

use crate::arch::InterruptLockGuard;

pub trait TTYHandler: Send + Sync {
    fn putc(&mut self, ch: char);
}

static TTY: Mutex<Option<&'static mut dyn TTYHandler>> = Mutex::new(None);
static PRINT_LOCK: Mutex<()> = Mutex::new(());

pub fn set_handler(handler: &'static mut dyn TTYHandler) {
    let _int_guard = InterruptLockGuard::new();
    let mut data = TTY.lock();
    *data = Some(handler);
}

#[derive(Default)]
struct GlobalPrintWriter;

impl Write for GlobalPrintWriter {
    fn write_str(&mut self, str: &str) -> Result {
        let mut data = TTY.lock();
        let tty = data.as_mut().ok_or(Error::default())?;
        for ch in str.chars() {
            tty.putc(ch);
        }
        Ok(())
    }
}

pub struct UnlockedPrinter;

impl UnlockedPrinter {
    pub fn print(&mut self, args: fmt::Arguments) {
        let mut pw = GlobalPrintWriter::default();
        let _ = pw.write_fmt(args);
    }

    pub fn println(&mut self, args: fmt::Arguments) {
        let mut pw = GlobalPrintWriter::default();
        let _ = pw.write_fmt(format_args!("{}\n", args));
    }
}

pub fn print_grouped<T: FnOnce(UnlockedPrinter)>(func: T) {
    let _int_guard = InterruptLockGuard::new();
    let _guard = PRINT_LOCK.lock();
    let printer = UnlockedPrinter {};
    func(printer);
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _int_guard = InterruptLockGuard::new();
    let _guard = PRINT_LOCK.lock();
    let mut pw = GlobalPrintWriter::default();
    let _ = pw.write_fmt(args);
}

pub macro print {
    ($($arg:tt)*) => ($crate::tty::_print(format_args!($($arg)*))),
}

pub macro println {
    () => ($crate::tty::print!("\n")),
    ($($arg:tt)*) => ($crate::tty::print!("{}\n", format_args!($($arg)*))),
}

pub macro red($text:expr) {
    concat!("\x1b[31m", $text, "\x1b[0m")
}

pub macro green($text:expr) {
    concat!("\x1b[32m", $text, "\x1b[0m")
}

pub macro yellow($text:expr) {
    concat!("\x1b[33m", $text, "\x1b[0m")
}

pub macro blue($text:expr) {
    concat!("\x1b[34m", $text, "\x1b[0m")
}

pub struct FlanTermTTY {
    context: *mut flanterm_context,
}

impl FlanTermTTY {
    pub fn from_framebuffer(fb: &Framebuffer) -> FlanTermTTY {
        let context: *mut flanterm_context;

        unsafe {
            context = flanterm_fb_init(
                None,
                None,
                fb.addr() as *mut u32,
                usize::try_from(fb.width()).unwrap(),
                usize::try_from(fb.height()).unwrap(),
                usize::try_from(fb.pitch()).unwrap(),
                fb.red_mask_size(),
                fb.red_mask_shift(),
                fb.green_mask_size(),
                fb.green_mask_shift(),
                fb.blue_mask_size(),
                fb.blue_mask_shift(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0usize,
                0usize,
                1usize,
                0usize,
                0usize,
                0usize,
            );

            flanterm_set_autoflush(context, false);
        }

        FlanTermTTY { context: context }
    }
}

impl TTYHandler for FlanTermTTY {
    fn putc(&mut self, ch: char) {
        unsafe {
            flanterm_write(self.context, ptr::from_ref(&(ch as i8)), 1);

            if ch == '\n' {
                flanterm_flush(self.context);
            }
        }
    }
}

unsafe impl Send for FlanTermTTY {}
unsafe impl Sync for FlanTermTTY {}
