use super::{
    AddressRange, ByteSize, PMM, PageFrameAllocator, PageSize, VFRange, VirtualPageFrameNumber,
    Wrapper,
};
use crate::arch::paging::{PageFlags, PageTableSet};
use core::alloc::GlobalAlloc;
use log::info;
use spin::{Mutex, Once};
use talc::{OomHandler, Span, Talc, Talck};

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
                .map_page_small(&mut this.pmm, this.limit, phys_frame, &PageFlags::KERNEL_RW);

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
    delegate: Once<Talck<Mutex<()>, BumpHeap>>,
}

unsafe impl GlobalAlloc for GlobalAllocImpl {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let delegate = self.delegate.get().expect("alloc not initialized");

        unsafe { delegate.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        let delegate = self.delegate.get().expect("alloc not initialized");

        unsafe { delegate.dealloc(ptr, layout) };
    }

    unsafe fn realloc(
        &self,
        ptr: *mut u8,
        layout: core::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        let delegate = self.delegate.get().expect("alloc not initialized");

        unsafe { delegate.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL_ALLOC: GlobalAllocImpl = GlobalAllocImpl {
    delegate: Once::new(),
};

pub(super) fn init_malloc(heap_range: VFRange, addr: PageTableSet) {
    info!("mem::init_malloc(): initializing heap");

    GLOBAL_ALLOC.delegate.call_once(|| {
        Talck::new(Talc::new(BumpHeap {
            range: heap_range,
            limit: heap_range.start(),
            pmm: PMM::get(),
            addr,
        }))
    });
}
