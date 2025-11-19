use crate::arch::{InterruptLockGuard, UnwindContext};
use core::fmt::{self, Error, Result, Write};
use spin::Mutex;

mod flanterm;

pub use flanterm::*;

pub trait TTYHandler: Send + Sync {
    fn putc(&mut self, ch: u8);
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
        for ch in str.as_bytes() {
            tty.putc(*ch);
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

    pub fn writer(self) -> impl Write {
        GlobalPrintWriter::default()
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

pub unsafe fn dump_stack<T: Write>(writer: &mut T, mut context: UnwindContext) {
    let mut i = 0;
    while unsafe { context.valid() } {
        let _ = writeln!(writer, "#{}: {:#016x}", i, unsafe {
            context.return_address()
        });
        i += 1;
        context = unsafe { context.next() };
    }
}

// TODO this is really stupid

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

pub struct MultiTTY {
    left: &'static mut dyn TTYHandler,
    right: &'static mut dyn TTYHandler,
}

impl MultiTTY {
    pub fn new(left: &'static mut dyn TTYHandler, right: &'static mut dyn TTYHandler) -> MultiTTY {
        MultiTTY { left, right }
    }
}

impl TTYHandler for MultiTTY {
    fn putc(&mut self, ch: u8) {
        self.left.putc(ch);
        self.right.putc(ch);
    }
}
