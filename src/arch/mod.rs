#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use self::x86_64::*;

pub struct InterruptLockGuard {
    has_interrupts: bool,
}

impl InterruptLockGuard {
    pub fn new() -> Self {
        let has_int = has_interrupts();
        disable_interrupts();
        InterruptLockGuard {
            has_interrupts: has_int,
        }
    }
}

impl Drop for InterruptLockGuard {
    fn drop(&mut self) {
        if self.has_interrupts {
            enable_interrupts();
        }
    }
}
