use crate::arch::UnwindContext;
use core::fmt::{self, Display, Result};

pub mod ansi;
mod flanterm;
mod init;
mod log;
pub mod options;

pub use init::*;

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
            let _ = writeln!(f, "#{}: {:#016x}", i, unsafe { context.return_address() })?;
            i += 1;
            context = unsafe { context.next() };
        }

        Ok(())
    }
}
