use x86::bits64::registers::rbp;

#[derive(Clone, Copy)]
pub struct UnwindContext {
    ptr: *const u64,
}

impl UnwindContext {
    #[inline(always)]
    pub unsafe fn get() -> UnwindContext {
        UnwindContext {
            ptr: rbp() as *const u64,
        }
    }

    pub unsafe fn valid(&self) -> bool {
        (unsafe { self.return_address() }) != 0
    }

    pub unsafe fn return_address(&self) -> u64 {
        unsafe { self.ptr.wrapping_add(1).read() }
    }

    pub unsafe fn next(&self) -> UnwindContext {
        UnwindContext {
            ptr: unsafe { self.ptr.read() } as *const u64,
        }
    }
}
