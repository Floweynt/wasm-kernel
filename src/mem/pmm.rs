use super::{EarlyPMM, MemoryMapView, PageFrameNumber, PageSize, VirtualAddress};
use crate::{
    arch::{
        PAGE_SMALL_SIZE, SMALL_PAGE_PAGE_SIZE,
        paging::{PageFlags, PageTableSet},
    },
    mem::{ByteSize, MemoryMapType, Wrapper},
};
use core::ptr;
use log::info;
use page_info::PageState;
use spin::{Mutex, Once};
use static_assertions::const_assert;

pub trait PageFrameAllocator {
    fn allocate_single_page(&self) -> PageFrameNumber;

    fn allocate_zeroed_page(&self) -> PageFrameNumber {
        let frame = self.allocate_single_page();

        unsafe {
            ptr::write_bytes(
                frame.to_virtual().as_ptr_mut::<u8>(),
                0,
                PAGE_SMALL_SIZE as usize,
            )
        };

        frame
    }
}

pub mod page_info {
    use crate::mem::PageFrameNumber;

    pub enum PageState {
        Free(Option<PageFrameNumber>),
        Used,
    }

    #[repr(align(64))]
    pub struct Page {
        pub state: PageState,
    }
}

const_assert!(size_of::<page_info::Page>() == 64);

struct PDTData {
    pdt: *mut page_info::Page,
    len: u64,
    // TODO: don't force a global lock on everything
    free_list: Mutex<Option<PageFrameNumber>>,
}

unsafe impl Sync for PDTData {}
unsafe impl Send for PDTData {}

static PDT: Once<PDTData> = Once::new();

fn get_page_info(frame: PageFrameNumber) -> &'static mut page_info::Page {
    let pdt = &mut PDT.get().expect("pdt not initialized");
    let index = (frame - PageFrameNumber::new(0)).value() as u64;
    assert!(index < pdt.len);
    unsafe { &mut *pdt.pdt.add(frame.value() as usize) }
}

pub(super) fn init_pdt(
    pmm: &EarlyPMM,
    address_space: &mut PageTableSet,
    start: VirtualAddress,
    hhdm_size: PageSize,
) {
    assert!(start.is_aligned(SMALL_PAGE_PAGE_SIZE));

    // we need to first populate the pdt by allocating backing pages and setting up the memory
    // mappings, then we can actually populate the table

    info!("mem::init_pdt(): mapping for pdt...");

    let mut prev = None;
    for entry in MemoryMapView::get().iter() {
        for offset in PageSize::new(0)..entry.size {
            let index = (entry.start + offset).value();
            let page_map_requested =
                (start + ByteSize::size_of::<page_info::Page>() * index).frame_containing();

            if prev != Some(page_map_requested) {
                prev = Some(page_map_requested);

                let backing_page = pmm.allocate_single_page();

                address_space.map_page_small(
                    pmm,
                    page_map_requested,
                    backing_page,
                    &PageFlags::KERNEL_RW,
                );
            }
        }
    }

    info!("mem::init_pdt(): built pdt mappings");

    pmm.freeze();

    let pdt = PDT.call_once(|| PDTData {
        pdt: start.as_ptr_mut(),
        len: hhdm_size.value(),
        free_list: Mutex::new(None),
    });

    let mut next_free = None;

    // populate table
    for (index, entry) in MemoryMapView::get().iter().enumerate() {
        // only populate for usable for now
        for offset in PageSize::new(0)..entry.size {
            let frame = entry.start + offset;
            let info = get_page_info(frame);
            *info = if entry.entry_type == MemoryMapType::Usable && !pmm.is_used(index, offset) {
                let result = page_info::Page {
                    state: PageState::Free(next_free),
                };

                next_free = Some(frame);

                result
            } else {
                page_info::Page {
                    state: PageState::Used,
                }
            }
        }
    }

    *pdt.free_list.lock() = next_free;

    info!("mem::init_pdt(): wrote physical page data table");
}

pub struct PMM {
    pdt: &'static PDTData,
}

impl PageFrameAllocator for PMM {
    fn allocate_single_page(&self) -> PageFrameNumber {
        // TODO: maybe use results more
        self.allocate_pages(PageSize::new(1))
            .expect("out of memory")
    }
}

impl PMM {
    pub fn get() -> PMM {
        PMM {
            pdt: PDT.get().expect("pdt not initialized"),
        }
    }

    fn allocate_pages(&self, count: PageSize) -> Option<PageFrameNumber> {
        // TODO
        assert!(count.value() == 1);
        let mut free_list = self.pdt.free_list.lock();

        free_list.inspect(|&free_page_number| {
            let free_page = get_page_info(free_page_number);

            if let page_info::PageState::Free(next) = &free_page.state {
                *free_list = *next;
            } else {
                panic!("free list points to non-free page")
            }
        })
    }
}
