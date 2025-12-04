use super::{
    AddressRange, ByteSize, PMM, PageFrameAllocator, PageSize, VFRange, VirtualPageFrameNumber,
    Wrapper,
};
use crate::{
    arch::paging::{PageFlags, PageTableSet},
    sync::IntMutex,
};
use core::{
    alloc::{GlobalAlloc, Layout},
    cmp::Ordering,
    ptr::{NonNull, null_mut},
};
use log::info;
use spin::Once;
use talc::{OomHandler, Span, Talc};

struct BumpHeap {
    range: VFRange,
    limit: VirtualPageFrameNumber,
    pmm: PMM,
    addr: PageTableSet,
}

impl OomHandler for BumpHeap {
    fn handle_oom(talc: &mut Talc<Self>, layout: core::alloc::Layout) -> Result<(), ()> {
        let this = &mut talc.oom_handler;
        let base = this.range.start();
        let initial_span = Span::new(base.as_ptr_mut(), this.limit.as_ptr_mut());
        let size = ByteSize::new(layout.pad_to_align().size() as u64).page_size_roundup();

        if base + size > this.range.end() {
            return Err(());
        }

        for _ in 0..size.value() {
            let phys_frame = this.pmm.allocate_single_page();

            this.addr
                .map_page_small(&this.pmm, this.limit, phys_frame, &PageFlags::KERNEL_RW);

            this.limit += PageSize::new(1);
        }

        let final_span = Span::new(base.as_ptr_mut(), this.limit.as_ptr_mut());

        if initial_span.is_empty() {
            unsafe { talc.claim(final_span).expect("initial heap claim failed") };
        } else {
            unsafe { talc.extend(initial_span, final_span) };
        }

        Ok(())
    }
}

// TODO: this should delegate stuff, but im lazy

struct GlobalAllocImpl {
    delegate: Once<IntMutex<Talc<BumpHeap>>>,
}

unsafe impl GlobalAlloc for GlobalAllocImpl {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let mut delegate = self.delegate.get().expect("alloc not initialized").lock();
        unsafe { delegate.malloc(layout).map_or(null_mut(), |nn| nn.as_ptr()) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let mut delegate = self.delegate.get().expect("alloc not initialized").lock();
        unsafe { delegate.free(NonNull::new_unchecked(ptr), layout) };
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        old_layout: core::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        let delegate = self.delegate.get().expect("alloc not initialized");

        let nn_ptr = unsafe { NonNull::new_unchecked(ptr) };

        match new_size.cmp(&old_layout.size()) {
            Ordering::Greater => {
                if let Ok(nn) =
                    unsafe { delegate.lock().grow_in_place(nn_ptr, old_layout, new_size) }
                {
                    return nn.as_ptr();
                }

                let new_layout =
                    unsafe { Layout::from_size_align_unchecked(new_size, old_layout.align()) };

                let mut lock = delegate.lock();
                let allocation = match unsafe { lock.malloc(new_layout) } {
                    Ok(ptr) => ptr,
                    Err(_) => return null_mut(),
                };

                if old_layout.size() > 0x10000 {
                    drop(lock);
                    unsafe {
                        allocation
                            .as_ptr()
                            .copy_from_nonoverlapping(ptr, old_layout.size())
                    };
                    lock = delegate.lock();
                } else {
                    unsafe {
                        allocation
                            .as_ptr()
                            .copy_from_nonoverlapping(ptr, old_layout.size())
                    };
                }

                unsafe { lock.free(nn_ptr, old_layout) };
                allocation.as_ptr()
            }

            Ordering::Less => {
                unsafe {
                    delegate
                        .lock()
                        .shrink(NonNull::new_unchecked(ptr), old_layout, new_size)
                };
                ptr
            }

            Ordering::Equal => ptr,
        }
    }
}

#[global_allocator]
static GLOBAL_ALLOC: GlobalAllocImpl = GlobalAllocImpl {
    delegate: Once::new(),
};

pub(super) fn init_malloc(heap_range: VFRange, addr: PageTableSet) {
    info!("mem::init_malloc(): initializing heap");

    GLOBAL_ALLOC.delegate.call_once(|| {
        IntMutex::new(Talc::new(BumpHeap {
            range: heap_range,
            limit: heap_range.start(),
            pmm: PMM::get(),
            addr,
        }))
    });
}
