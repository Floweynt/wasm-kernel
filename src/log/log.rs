use core::fmt::Write;

use super::CharSink;
use crate::{
    cmdline::get_cmdline,
    log::ansi::{ANSIFormatter, Color},
};
use core::fmt::Result;
use log::Log;
use spin::Mutex;

pub struct LogImpl {
    pub(super) lock: Mutex<()>,
    pub(super) serial: Option<&'static dyn CharSink>,
    pub(super) framebuffer: Option<&'static dyn CharSink>,
}

impl Write for &'static dyn CharSink {
    fn write_str(&mut self, s: &str) -> Result {
        for ch in s.bytes() {
            unsafe {
                self.putc(ch);
            }
        }

        Ok(())
    }
}

fn do_write<T: Write>(record: &log::Record, backend: &mut T) {
    if get_cmdline().logging.options.level {
        let _ = match record.level() {
            log::Level::Error => write!(
                backend,
                "{} | ",
                ANSIFormatter::new(&"error").color(Color::RED).bold()
            ),
            log::Level::Warn => write!(
                backend,
                "{} | ",
                ANSIFormatter::new(&"warn").color(Color::YELLOW).bold()
            ),
            log::Level::Info => write!(
                backend,
                "{} | ",
                ANSIFormatter::new(&"info").color(Color::CYAN)
            ),
            log::Level::Debug => write!(backend, "debug | "),
            log::Level::Trace => write!(backend, "{} | ", ANSIFormatter::new(&"trace").italic()),
        };
    }

    if get_cmdline().logging.options.target
        && !record.target().is_empty() {
            let _ = write!(backend, "{} | ", record.target());
        }

    if get_cmdline().logging.options.mod_path
        && let Some(path) = record.module_path() {
            let _ = write!(backend, "{} | ", path);
        }

    if get_cmdline().logging.options.src {
        let _ = write!(
            backend,
            "{}:{} | ",
            record.file().unwrap_or("<unk>"),
            record.line().unwrap_or(0)
        );
    }

    let _ = backend.write_fmt(*record.args());
    let _ = backend.write_char('\n');
}

impl Log for LogImpl {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        // filtering is done per-backend anyway
        true
    }

    fn log(&self, record: &log::Record) {
        let _guard = self.lock.lock();

        if let Some(mut serial) = self.serial {
            do_write(record, &mut serial);
        }

        if let Some(mut framebuffer) = self.framebuffer {
            do_write(record, &mut framebuffer);
        }
    }

    fn flush(&self) {
        let _guard = self.lock.lock();

        if let Some(serial) = self.serial {
            unsafe { serial.flush() };
        }

        if let Some(framebuffer) = self.framebuffer {
            unsafe { framebuffer.flush() };
        }
    }
}
