use core::ptr::addr_eq;

use super::{
    ByteSize, MemoryMapType, MemoryMapView, PageFrameAllocator, PageFrameNumber, PageSize,
    PhysicalAddress, VARange, VirtualAddress, VirtualPageFrameNumber, Wrapper, page_info,
    vpa::{AddressSpace, TreeVirtualAllocator, VirtualAllocator},
};
use crate::{
    arch::paging::{PageFlags, PageTableSet, get_higher_half_addr},
    mem::{
        AddressRange, MEMORY_MAP_REQUEST, VFRange, get_hhdm_start, get_kernel_physical_base,
        get_kernel_virtual_base, init_malloc, init_pdt, vpa::EarlyVirtualAllocator,
    },
    tty::{blue, green, println, red, yellow},
};
use limine::{memory_map::EntryType, response::MemoryMapResponse};
use spin::Once;

unsafe extern "C" {
    static _marker_kernel_start: u8;
    static _marker_limine_request_start: u8;
    static _marker_limine_request_end: u8;
    static _marker_text_start: u8;
    static _marker_text_end: u8;
    static _marker_rodata_start: u8;
    static _marker_rodata_end: u8;
    static _marker_data_start: u8;
    static _marker_data_end: u8;
    static _marker_kernel_end: u8;
}

pub(super) struct EarlyPMM {
    index: usize,
    offset: PageSize,
    is_frozen: bool,
}

impl PageFrameAllocator for EarlyPMM {
    fn allocate_single_page(&mut self) -> PageFrameNumber {
        assert!(!self.is_frozen);

        let entries = MemoryMapView::get();

        loop {
            if self.offset < entries.at(self.index).size
                && entries.at(self.index).entry_type == MemoryMapType::Usable
            {
                break;
            }

            self.index += 1;
            self.offset = PageSize::new(0u64);

            if self.index >= entries.len() {
                panic!("EarlyPMM::allocate_page(): out-of-memory")
            }
        }

        let pos = entries.at(self.index).start + self.offset;
        self.offset += PageSize::new(1u64);
        pos
    }
}

impl EarlyPMM {
    pub(super) fn freeze(&mut self) {
        #[cfg(debug_assertions)]
        {
            self.is_frozen = true;
        }
    }

    pub(super) fn is_used(&mut self, index: usize, offset: PageSize) -> bool {
        index < self.index || (index == self.index && offset < self.offset)
    }
}

pub fn dump_memory_info() {
    let mem_map = MEMORY_MAP_REQUEST.get_response().unwrap();

    println!("memory map: ");
    for entries in mem_map.entries() {
        println!(
            "[{}] {:#016x}-{:#016x} len = {:#x}",
            match entries.entry_type {
                EntryType::USABLE => green!("usable      "),
                EntryType::RESERVED => red!("reserved    "),
                EntryType::ACPI_RECLAIMABLE => yellow!("ACPI reclaim"),
                EntryType::ACPI_NVS => blue!("ACPI NVS    "),
                EntryType::BAD_MEMORY => red!("bad         "),
                EntryType::BOOTLOADER_RECLAIMABLE => yellow!("bootloader  "),
                EntryType::EXECUTABLE_AND_MODULES => blue!("kernel      "),
                EntryType::FRAMEBUFFER => blue!("framebuffer "),
                _ => red!("unknown     "),
            },
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

fn allocate_padded(
    alloc: &mut EarlyVirtualAllocator,
    size: PageSize,
    padding: PageSize,
) -> Option<VFRange> {
    alloc
        .allocate(padding + size + padding)
        .map(|f| VFRange::new(f + padding, f + padding + size))
}

fn init_vm_layout(
    memory_map: &MemoryMapResponse,
) -> (&'static VirtualMemoryLayout, EarlyVirtualAllocator) {
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

    let mut allocator = EarlyVirtualAllocator::new(
        get_higher_half_addr().frame_aligned(),
        get_kernel_virtual_base().frame_aligned(),
    );

    allocator
        .reserve_range(VFRange::new(
            hhdm_base.frame_aligned() - padding,
            hhdm_end.frame_aligned() + padding,
        ))
        .expect("mem::init_vm_layout(): failed to reserve range for HHDM");

    let pdt_size =
        (ByteSize::size_of::<page_info::PageState>() * hhdm_size.value()).page_size_roundup();

    let heap_size = PageSize::new(1 << 28);

    let (pdt_base, pdt_end) = allocate_padded(&mut allocator, pdt_size, padding)
        .expect("mem::init_vm_layout(): failed to allocate memory for physical page desc table")
        .tup();

    let (heap_base, heap_end) = allocate_padded(&mut allocator, heap_size, padding)
        .expect("mem::init_vm_layout(): failed to allocate memory for heap")
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
    pmm: &mut EarlyPMM,
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

fn transition_paging(pmm: &mut EarlyPMM, layout: &VirtualMemoryLayout, space: &mut PageTableSet) {
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

    println!("mem::transition_paging(): kernel segment layout:");
    println!("  limine_requests r-- {}-{}", limine_start, limine_end);
    println!("  text            r-x {}-{}", text_start, text_end);
    println!("  rodata          r-- {}-{}", rodata_start, rodata_end);
    println!("  data            rw- {}-{}", data_start, data_end);

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

    println!("mem::transition_paging(): preparing to switch pt...");

    unsafe {
        space.set_current();
    }

    println!("mem::transition_paging(): done switch pt");
}

pub fn init() -> AddressSpace<TreeVirtualAllocator> {
    let memory_map = MEMORY_MAP_REQUEST
        .get_response()
        .expect("memory map not present");

    let (layout, early_allocator) = init_vm_layout(memory_map);

    println!("mem::init(): VM layout:");
    println!("  base: {}", layout.higher_half_base);
    println!(
        "  HHDM: {}-{} ({} pages)",
        layout.hhdm_base, layout.hhdm_end, layout.hhdm_size
    );
    println!("  PDT: {}-{}", layout.pdt_base, layout.pdt_end);
    println!(
        "  heap: {}-{}",
        layout.heap_base.address(),
        layout.heap_end.address()
    );
    println!(
        "  kernel: {}-{} -> phys:{}",
        layout.kernel_base, layout.kernel_end, layout.kernel_phys_base
    );

    /* TODO check HHDM align
    if layout.hhdm_base != 0 {
        panic!("hhdm is not aligned");
    }*/

    let mut early_pmm = EarlyPMM {
        index: 0,
        offset: PageSize::new(0u64),
        is_frozen: false,
    };

    // transition over to our own memory mapping scheme

    let mut root_space = PageTableSet::new::<EarlyPMM>(&mut early_pmm);

    transition_paging(&mut early_pmm, &layout, &mut root_space);

    init_pdt(
        &mut early_pmm,
        &mut root_space,
        layout.pdt_base,
        layout.hhdm_size,
    );

    init_malloc(VFRange::new(layout.heap_base, layout.heap_end), root_space);

    let tree_allocator = TreeVirtualAllocator::new(early_allocator);

    AddressSpace {
        virtual_alloc: tree_allocator,
        tables: root_space,
    }
}
