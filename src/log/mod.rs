use crate::{arch::UnwindContext, modules::symbols};
use core::fmt::{self, Display, Result};

pub mod ansi;
mod flanterm;
mod init;
mod log;
pub mod options;

pub use init::*;
use rustc_demangle::demangle;

pub trait CharSink: Send + Sync {
    unsafe fn putc(&self, ch: u8);

    unsafe fn flush(&self);
}

pub struct StackTrace(UnwindContext);

impl StackTrace {
    pub fn new(ctx: UnwindContext) -> StackTrace {
        StackTrace(ctx)
    }

    #[inline(always)]
    pub fn current() -> StackTrace {
        Self::new(unsafe { UnwindContext::get() })
    }
}

impl Display for StackTrace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result {
        let StackTrace(mut context) = *self;

        let mut i = 0;
        while unsafe { context.valid() } {
            let addr = unsafe { context.return_address() };
            writeln!(f, "#{}: {:#016x}", i, addr)?;

            let (fn_iter, loc) = symbols::symbolize(addr);

            if let Some(loc) = loc {
                writeln!(
                    f,
                    "  at {}:{}:{}",
                    loc.file.unwrap_or("unk"),
                    loc.row,
                    loc.col
                )?;
            }

            if let Some(mut iter) = fn_iter {
                if let Some(first) = iter.next() {
                    writeln!(f, "  in {:#}", demangle(first.name.unwrap_or("unk")))?;
                }

                for inl in iter.by_ref() {
                    let loc = inl.location;
                    writeln!(
                        f,
                        "    inlined at {}:{}:{}",
                        loc.file.unwrap_or("unk"),
                        loc.row,
                        loc.col
                    )?;
                    writeln!(f, "    into {:#}", demangle(inl.name.unwrap_or("unk")))?;
                }
            }

            i += 1;
            context = unsafe { context.next() };
        }

        Ok(())
    }
}
