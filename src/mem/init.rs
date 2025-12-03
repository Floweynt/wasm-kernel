use super::{
    ByteSize, MemoryMapType, MemoryMapView, PageFrameAllocator, PageFrameNumber, PageSize,
    PhysicalAddress, VARange, VirtualAddress, VirtualPageFrameNumber, Wrapper, page_info,
    vpa::{EarlyAllocator, VirtualAllocator},
};
use crate::{
    arch::paging::{PageFlags, PageTableSet, get_higher_half_addr},
    log::ansi::{ANSIFormatter, Color},
    mem::{
        AddressRange, MEMORY_MAP_REQUEST, VFRange, get_hhdm_start, get_kernel_physical_base,
        get_kernel_virtual_base, init_pdt, malloc::init_malloc, vpa,
    },
};
use core::{cell::RefCell, ffi::c_void};
use limine::{memory_map::EntryType, response::MemoryMapResponse};
use log::info;
use spin::Once;

unsafe extern "C" {
    static _marker_kernel_start: c_void;
    static _marker_limine_request_start: c_void;
    static _marker_limine_request_end: c_void;
    static _marker_text_start: c_void;
    static _marker_text_end: c_void;
    static _marker_rodata_start: c_void;
    static _marker_rodata_end: c_void;
    static _marker_data_start: c_void;
    static _marker_data_end: c_void;
    static _marker_kernel_end: c_void;
}

struct EarlyPMMInner {
    index: usize,
    offset: PageSize,
    is_frozen: bool,
}

pub(super) struct EarlyPMM {
    data: RefCell<EarlyPMMInner>,
}

impl PageFrameAllocator for EarlyPMM {
    fn allocate_single_page(&self) -> PageFrameNumber {
        let mut state = self.data.borrow_mut();

        assert!(!state.is_frozen);

        let entries = MemoryMapView::get();

        loop {
            if state.offset < entries.at(state.index).size
                && entries.at(state.index).entry_type == MemoryMapType::Usable
            {
                break;
            }

            state.index += 1;
            state.offset = PageSize::new(0u64);

            if state.index >= entries.len() {
                panic!("EarlyPMM::allocate_page(): out-of-memory")
            }
        }

        let pos = entries.at(state.index).start + state.offset;
        state.offset += PageSize::new(1u64);
        pos
    }
}

impl EarlyPMM {
    pub(super) fn freeze(&self) {
        #[cfg(debug_assertions)]
        {
            self.data.borrow_mut().is_frozen = true;
        }
    }

    pub(super) fn is_used(&self, index: usize, offset: PageSize) -> bool {
        let state = self.data.borrow();
        index < state.index || (index == state.index && offset < state.offset)
    }
}

pub fn dump_memory_info() {
    let mem_map = MEMORY_MAP_REQUEST.get_response().unwrap();

    info!("memory map: ");
    for entries in mem_map.entries() {
        let (str, color) = match entries.entry_type {
            EntryType::USABLE => ("usable", Color::GREEN),
            EntryType::RESERVED => ("reserved", Color::RED),
            EntryType::ACPI_RECLAIMABLE => ("ACPI reclaim", Color::YELLOW),
            EntryType::ACPI_NVS => ("ACPI NVS", Color::BLUE),
            EntryType::BAD_MEMORY => ("bad", Color::RED),
            EntryType::BOOTLOADER_RECLAIMABLE => ("bootloader", Color::YELLOW),
            EntryType::EXECUTABLE_AND_MODULES => ("kernel", Color::CYAN),
            EntryType::FRAMEBUFFER => ("framebuffer", Color::BLUE),
            _ => ("unknown", Color::RED),
        };

        info!(
            "[{:12 }] {:#016x}-{:#016x} len = {:#x}",
            ANSIFormatter::new(&str).color(color),
            entries.base,
            entries.base + entries.length,
            entries.length
        );
    }
}

pub(super) struct VirtualMemoryLayout {
    pub(super) higher_half_base: VirtualAddress,
    pub(super) hhdm_base: VirtualAddress,
    pub(super) hhdm_end: VirtualAddress,
    pub(super) hhdm_size: PageSize,
    pub(super) pdt_base: VirtualAddress,
    pub(super) pdt_end: VirtualAddress,
    pub(super) heap_base: VirtualPageFrameNumber,
    pub(super) heap_end: VirtualPageFrameNumber,
    pub(super) kernel_base: VirtualAddress,
    pub(super) kernel_end: VirtualAddress,
    pub(super) kernel_phys_base: PhysicalAddress,
}

pub(super) static VM_LAYOUT: Once<VirtualMemoryLayout> = Once::new();

fn init_vm_layout(
    memory_map: &MemoryMapResponse,
) -> (
    &'static VirtualMemoryLayout,
    VirtualAllocator<EarlyAllocator>,
) {
    let max_addr = memory_map
        .entries()
        .iter()
        .map(|f| f.base + f.length)
        .max()
        .expect("memory map is empty");

    let padding = PageSize::new(32u64);

    let hhdm_base = get_hhdm_start();
    let hhdm_size = ByteSize::new(max_addr).page_size_roundup();
    let hhdm_end = hhdm_base + hhdm_size;

    let kernel_base = get_kernel_virtual_base();
    let kernel_end = kernel_base
        + (VirtualAddress::from(&raw const _marker_kernel_end)
            - VirtualAddress::from(&raw const _marker_kernel_start));

    let allocator = VirtualAllocator::early(
        VFRange::new(
            get_higher_half_addr().frame_aligned(),
            get_kernel_virtual_base().frame_aligned(),
        ),
        &[VFRange::new(
            hhdm_base.frame_aligned() - padding,
            hhdm_end.frame_aligned() + padding,
        )],
    )
    .expect("mem::init_vm_layout(): failed to reserve range for HHDM");

    let pdt_size =
        (ByteSize::size_of::<page_info::PageState>() * hhdm_size.value()).page_size_roundup();

    let heap_size = PageSize::new(1 << 28);

    let (pdt_base, pdt_end) = allocator
        .allocate_padded(pdt_size, padding)
        .expect("mem::init_vm_layout(): failed to allocate memory for physical page desc table")
        .leak()
        .tup();

    let (heap_base, heap_end) = allocator
        .allocate_padded(heap_size, padding)
        .expect("mem::init_vm_layout(): failed to allocate memory for heap")
        .leak()
        .tup();

    let layout = VM_LAYOUT.call_once(|| VirtualMemoryLayout {
        higher_half_base: get_higher_half_addr(),
        hhdm_base,
        hhdm_end,
        hhdm_size,
        pdt_base: pdt_base.address(),
        pdt_end: pdt_end.address(),
        heap_base,
        heap_end,
        kernel_base,
        kernel_end,
        kernel_phys_base: get_kernel_physical_base(),
    });

    (layout, allocator)
}

fn map_kernel_segment(
    pmm: &EarlyPMM,
    address_space: &mut PageTableSet,
    layout: &VirtualMemoryLayout,
    range: VARange,
    flags: PageFlags,
) {
    let (start, end) = range.tup();
    let offset = start - layout.kernel_base;
    let size: ByteSize = (end - start).into();

    address_space.map_range(
        pmm,
        start.frame_aligned(),
        (layout.kernel_phys_base + offset).frame_aligned(),
        size.page_size_roundup(),
        &flags,
    );
}

fn transition_paging(pmm: &EarlyPMM, layout: &VirtualMemoryLayout, space: &mut PageTableSet) {
    for entry in MemoryMapView::get().iter() {
        if let Some(traits) = match entry.entry_type {
            MemoryMapType::Usable
            | MemoryMapType::KernelBinaries
            | MemoryMapType::Framebuffer
            | MemoryMapType::ACPIReclaimable
            | MemoryMapType::BootloaderReclaimable => Some(PageFlags::KERNEL_RW),
            MemoryMapType::BadMemory | MemoryMapType::ACPINVS => Some(PageFlags::KERNEL_RO),
            _ => None,
        } {
            space.map_range(
                pmm,
                entry.start.to_virtual(),
                entry.start,
                entry.size,
                &traits,
            );
        }
    }

    // kernel segments

    let limine_start = VirtualAddress::from(&raw const _marker_limine_request_start);
    let limine_end = VirtualAddress::from(&raw const _marker_limine_request_end);
    let text_start = VirtualAddress::from(&raw const _marker_text_start);
    let text_end = VirtualAddress::from(&raw const _marker_text_end);
    let rodata_start = VirtualAddress::from(&raw const _marker_rodata_start);
    let rodata_end = VirtualAddress::from(&raw const _marker_rodata_end);
    let data_start = VirtualAddress::from(&raw const _marker_data_start);
    let data_end = VirtualAddress::from(&raw const _marker_data_end);

    info!("mem::transition_paging(): kernel segment layout:");
    info!("  limine_requests r-- {}-{}", limine_start, limine_end);
    info!("  text            r-x {}-{}", text_start, text_end);
    info!("  rodata          r-- {}-{}", rodata_start, rodata_end);
    info!("  data            rw- {}-{}", data_start, data_end);

    map_kernel_segment(
        pmm,
        space,
        layout,
        VARange::new(limine_start, limine_end),
        PageFlags::KERNEL_RO,
    );

    map_kernel_segment(
        pmm,
        space,
        layout,
        VARange::new(text_start, text_end),
        PageFlags::KERNEL_X,
    );

    map_kernel_segment(
        pmm,
        space,
        layout,
        VARange::new(rodata_start, rodata_end),
        PageFlags::KERNEL_RO,
    );

    map_kernel_segment(
        pmm,
        space,
        layout,
        VARange::new(data_start, data_end),
        PageFlags::KERNEL_RW,
    );

    info!("mem::transition_paging(): preparing to switch pt...");

    unsafe {
        space.set_current();
    }

    // make sure that the first-level page tables are allocated
    // this is important because it means that all separate cores will share these tables
    space.map_kernel_pages(pmm);

    info!("mem::transition_paging(): done switch pt");
}

pub fn init() -> PageTableSet {
    let memory_map = MEMORY_MAP_REQUEST
        .get_response()
        .expect("memory map not present");

    let (layout, early_allocator) = init_vm_layout(memory_map);

    info!("mem::init(): VM layout:");
    info!("  base: {}", layout.higher_half_base);
    info!(
        "  HHDM: {}-{} ({} pages)",
        layout.hhdm_base, layout.hhdm_end, layout.hhdm_size
    );
    info!("  PDT: {}-{}", layout.pdt_base, layout.pdt_end);
    info!(
        "  heap: {}-{}",
        layout.heap_base.address(),
        layout.heap_end.address()
    );
    info!(
        "  kernel: {}-{} -> phys:{}",
        layout.kernel_base, layout.kernel_end, layout.kernel_phys_base
    );

    let early_pmm = EarlyPMM {
        data: RefCell::new(EarlyPMMInner {
            index: 0,
            offset: PageSize::new(0),
            is_frozen: false,
        }),
    };

    // transition over to our own memory mapping scheme

    let mut root_space = PageTableSet::new::<EarlyPMM>(&early_pmm);

    transition_paging(&early_pmm, layout, &mut root_space);

    init_pdt(
        &early_pmm,
        &mut root_space,
        layout.pdt_base,
        layout.hhdm_size,
    );

    init_malloc(VFRange::new(layout.heap_base, layout.heap_end), root_space);

    vpa::initialize(VirtualAllocator::tree(early_allocator));

    root_space
}
