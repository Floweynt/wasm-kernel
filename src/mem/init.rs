use super::{
    ByteSize, MemoryMapType, MemoryMapView, PageFrameAllocator, PageFrameNumber, PageSize,
    PhysicalAddress, VirtualAddress, get_kernel_size,
};
use crate::{
    arch::paging::{AddressSpace, PageFlags, get_higher_half_addr},
    mem::{
        HHDM_INFO_REQUEST, MEMORY_MAP_REQUEST, get_hhdm_start, get_kernel_physical_base,
        get_kernel_virtual_base,
    },
    tty::{blue, green, println, red, yellow},
};
use limine::{memory_map::EntryType, response::MemoryMapResponse};
use spin::Once;

struct EarlyPMM {
    index: usize,
    offset: PageSize,
}

impl PageFrameAllocator for EarlyPMM {
    fn allocate_single_page(&mut self) -> PageFrameNumber {
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

pub fn dump_memory_info() {
    if let Some(res) = HHDM_INFO_REQUEST.get_response() {
        println!("kmain(): hhdm @ {:#016x}", res.offset());
    }

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

#[derive(Debug)]
pub(super) struct VirtualMemoryLayout {
    pub(super) higher_half_base: VirtualAddress,
    pub(super) hhdm_base: VirtualAddress,
    pub(super) hhdm_end: VirtualAddress,
    pub(super) pfn_start: VirtualAddress,
    pub(super) kernel_base: VirtualAddress,
    pub(super) kernel_end: VirtualAddress,
    pub(super) kernel_phys_base: PhysicalAddress,
}

pub(super) static VM_LAYOUT: Once<VirtualMemoryLayout> = Once::new();

fn init_vm_layout(memory_map: &MemoryMapResponse) -> &'static VirtualMemoryLayout {
    let max_addr = memory_map
        .entries()
        .iter()
        .map(|f| f.base + f.length)
        .max()
        .expect("memory map is empty");

    let hhdm_base_addr = get_hhdm_start();

    let hhdm_end = hhdm_base_addr + ByteSize::new(max_addr);

    VM_LAYOUT.call_once(|| VirtualMemoryLayout {
        higher_half_base: get_higher_half_addr(),
        hhdm_base: hhdm_base_addr,
        hhdm_end: hhdm_end,
        pfn_start: hhdm_end + PageSize::new(32u64), // TODO: don't hardcode this
        kernel_base: get_kernel_virtual_base(),
        kernel_end: get_kernel_virtual_base() + get_kernel_size(),
        kernel_phys_base: get_kernel_physical_base(),
    })
}

/*
fn setup_pfn(page: &VirtualMemoryLayout) {
    let mut pfn_page = None;

    for entry in MemoryMapView::get().iter() {
        entry.start.to_virtual();
    }
}*/

fn transition_paging(pmm: &mut EarlyPMM, layout: &VirtualMemoryLayout) {
    let mut root_space = AddressSpace::new::<EarlyPMM>(pmm);

    for entry in MemoryMapView::get().iter() {
        if let Some(traits) = match entry.entry_type {
            MemoryMapType::Usable
            | MemoryMapType::KernelBinaries
            | MemoryMapType::Framebuffer
            | MemoryMapType::ACPIReclaimable
            | MemoryMapType::BootloaderReclaimable => Some(PageFlags {
                write: true,
                user: false,
                execute: false,
                global: true,
            }),
            MemoryMapType::BadMemory | MemoryMapType::ACPINVS => Some(PageFlags {
                write: false,
                user: false,
                execute: false,
                global: true,
            }),
            _ => None,
        } {
            root_space.map_range(
                pmm,
                entry.start.to_virtual(),
                entry.start,
                entry.size,
                &traits,
            );
        }
    }

    root_space.map_range(
        pmm,
        layout.kernel_base.frame_number(),
        layout.kernel_phys_base.frame_number(),
        get_kernel_size().page_size_roundup(),
        &PageFlags {
            write: true,
            user: false,
            execute: true,
            global: true,
        },
    );

    println!("mem::transition_paging(): preparing to switch pt...");

    unsafe {
        root_space.set_current();
    }

    println!("mem::transition_paging(): done switch pt");
}

pub fn init() {
    let memory_map = MEMORY_MAP_REQUEST
        .get_response()
        .expect("memory map not present");
    let layout = init_vm_layout(memory_map);

    println!("mem::init(): VM layout: {:?}", layout);

    /* TODO check HHDM align
    if layout.hhdm_base != 0 {
        panic!("hhdm is not aligned");
    }*/

    let mut early_pmm = EarlyPMM {
        index: 0,
        offset: PageSize::new(0u64),
    };

    // transition over to our own memory mapping scheme
    transition_paging(&mut early_pmm, &layout);
}
