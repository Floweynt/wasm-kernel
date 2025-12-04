pub mod paging;

mod dt;
mod interrupt;
pub mod mp;
mod serial;
mod unwind;

extern crate alloc;

use crate::mem::ByteSize;
use crate::mem::PageSize;
use crate::mem::PhysicalAddress;
use crate::mem::VirtualAddress;
use crate::mem::Wrapper;
use core::arch::asm;
use core::arch::naked_asm;
use dt::GlobalDescriptorTable;
use dt::InterruptStackTable;
use x86::bits64::paging::PAddr;
use x86::bits64::paging::VAddr;
use x86::bits64::rflags::{self, RFlags};

pub use serial::*;
pub use unwind::*;

pub fn halt() -> ! {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        asm!("cli");
        loop {
            asm!("hlt");
        }
    }
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, len: usize) -> *mut u8 {
    naked_asm!(
        "mov rax, rdi",
        "mov rcx, rdx",
        "shr rcx, 3",
        "rep movsq",
        "mov rcx, rdx",
        "and rcx, 0x7",
        "rep movsb",
        "ret",
    );
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dest: *mut u8, byte: i32, len: usize) -> *mut u8 {
    naked_asm!(
        "mov r11, rdi",
        "mov rcx, rdx",
        "movzx rax, sil",
        "mov r10, 0x0101010101010101",
        "mul r10",
        "mov rdx, rcx",
        "shr rcx, 3",
        "rep stosq",
        "mov rcx, rdx",
        "and rcx, 0x7",
        "rep stosb",
        "mov rax, r11",
        "ret",
    )
}

#[inline(always)]
fn disable_interrupts() {
    unsafe {
        asm!("cli");
    }
}

#[inline(always)]
fn enable_interrupts() {
    unsafe {
        asm!("sti");
    }
}

#[repr(transparent)]
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct IrqState(bool);

impl IrqState {
    #[inline(always)]
    pub fn save() -> IrqState {
        IrqState(rflags::read().contains(RFlags::FLAGS_IF))
    }

    #[inline(always)]
    pub fn restore(self) {
        if self.0 {
            enable_interrupts();
        } else {
            disable_interrupts();
        }
    }
}

#[inline(always)]
pub fn irq_disable() {
    disable_interrupts();
}

pub const HIGHER_HALF_VIRTUAL_ADDRESS_BASE_PML4: VirtualAddress =
    VirtualAddress::new(0xffff800000000000u64);
pub const HIGHER_HALF_VIRTUAL_ADDRESS_BASE_PML5: VirtualAddress =
    VirtualAddress::new(0xff00000000000000u64);

pub const PAGE_SMALL_SIZE: u64 = 4096;
pub const PAGE_MEDIUM_SIZE: u64 = 512 * PAGE_SMALL_SIZE;
pub const PAGE_LARGE_SIZE: u64 = 512 * PAGE_MEDIUM_SIZE;
pub const PAGE_MAX_SIZE: u64 = PAGE_LARGE_SIZE;

pub const SMALL_PAGE_BYTE_SIZE: ByteSize = ByteSize::new(PAGE_SMALL_SIZE);
pub const MEDIUM_PAGE_BYTE_SIZE: ByteSize = ByteSize::new(PAGE_MEDIUM_SIZE);
pub const LARGE_PAGE_BYTE_SIZE: ByteSize = ByteSize::new(PAGE_LARGE_SIZE);

pub const SMALL_PAGE_PAGE_SIZE: PageSize = PageSize::new(1);
pub const MEDIUM_PAGE_PAGE_SIZE: PageSize = PageSize::new(512);
pub const LARGE_PAGE_PAGE_SIZE: PageSize = PageSize::new(512 * 512);

impl From<VirtualAddress> for VAddr {
    fn from(val: VirtualAddress) -> Self {
        // TODO: don't unwrap
        VAddr::from_u64(val.value())
    }
}

impl From<PhysicalAddress> for PAddr {
    fn from(val: PhysicalAddress) -> Self {
        PAddr(val.value())
    }
}

pub fn load_core_local_ptr() -> VirtualAddress {
    let value: u64;
    unsafe {
        asm!(
            "movq %gs:0, {}",
            lateout(reg) value,
            options(nostack, preserves_flags, pure, readonly),
        );
    }

    VirtualAddress::new(value)
}
