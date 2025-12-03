use crate::{
    arch::{
        mp::get_cpu_local_pointer,
        paging::{PageFlags, PageTableSet},
    },
    mem::{AddressRange, ByteDiff, PMM, PageSize, SizeType, VFRange, VirtualAddress, Wrapper, vpa},
};
use alloc::vec::Vec;
use atomic_enum::atomic_enum;
use core::{
    cell::Cell,
    ffi::c_void,
    ops::{Deref, DerefMut},
    ptr,
};
use derive_more::{Debug, Display};
use spin::Once;

extern crate alloc;

#[atomic_enum]
pub enum MpState {
    KInit,
    MPInit,
    // TODO: do we even want to do preemption
    MPPreempt,
}

pub static MP_STATE: AtomicMpState = AtomicMpState::new(MpState::KInit);

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Display)]
#[display("{_0}")]
#[debug("CoreId({_0})")]
pub struct CoreId(pub usize);

// core local stuff

#[repr(C)]
pub struct CoreLocal<T>(T);

pub macro core_local {
    {
        $(
            $(#[$meta:meta])*
            $vis:vis $name:ident : $ty:ty = $init:expr;
        )*
    } => {
        $(
            $(#[$meta])*
            #[unsafe(link_section = ".cpu_local")]
            $vis static $name: crate::mp::CoreLocal<$ty> = crate::mp::CoreLocal::new($init);
        )*
    }
}

unsafe extern "C" {
    static _marker_cpu_local_template_start: c_void;
    static _marker_cpu_local_template_end: c_void;
}

static OFFSET_ARRAY: Once<Vec<u64>> = Once::new();

fn cpu_local_template_region() -> VFRange {
    VFRange::new(
        VirtualAddress::from(&raw const _marker_cpu_local_template_start).frame_aligned(),
        VirtualAddress::from(&raw const _marker_cpu_local_template_end).frame_aligned(),
    )
}

impl<T> CoreLocal<T> {
    pub const fn new(val: T) -> Self {
        Self(val)
    }

    fn offset(&self) -> ByteDiff {
        let self_addr = VirtualAddress::new(self as *const _ as u64);
        let template_range = cpu_local_template_region();

        assert!(template_range.as_va_range().contains(self_addr));

        self_addr - template_range.start().address()
    }

    pub fn addr(&self) -> VirtualAddress {
        get_cpu_local_pointer() + self.offset()
    }
}

// core locals can always be "sent" and "synced" across threads (which is meaningless)
unsafe impl<T> Send for CoreLocal<T> {}
unsafe impl<T> Sync for CoreLocal<T> {}

impl<T> Deref for CoreLocal<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.addr().as_ptr() }
    }
}

impl<T> DerefMut for CoreLocal<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.addr().as_ptr_mut() }
    }
}

pub fn get_cpu_local_offset(core: CoreId) -> VirtualAddress {
    VirtualAddress::from(&raw const OFFSET_ARRAY.get().unwrap()[core.0])
}

pub fn init_cpu_local_table(tables: &PageTableSet, n_cores: usize) {
    let template = cpu_local_template_region();
    let alloc = vpa::get_global_vpa();
    let pmm = PMM::get();

    OFFSET_ARRAY.call_once(|| {
        (0..n_cores)
            .map(|_| {
                let addr = alloc
                    .allocate_backed_padded(
                        &pmm,
                        tables,
                        template.size(),
                        PageSize::new(1),
                        PageFlags::KERNEL_RW,
                    )
                    .expect("failed!")
                    .leak();

                unsafe {
                    ptr::copy_nonoverlapping(
                        template.start().as_ptr::<u8>(),
                        addr.start().as_ptr_mut::<u8>(),
                        template.size().size_bytes() as usize,
                    )
                };
                addr.start().address().value()
            })
            .collect()
    });
}

core_local! {
    pub CORE_ID: Cell<CoreId> = Cell::new(CoreId(0));
}
