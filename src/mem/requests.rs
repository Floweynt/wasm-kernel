use limine::{
    memory_map::{self, EntryType},
    request::{ExecutableAddressRequest, HhdmRequest, MemoryMapRequest},
    response::MemoryMapResponse,
};

use crate::mem::{ByteSize, PhysicalAddress};

use super::{PageFrameNumber, PageSize, VirtualAddress};

#[used]
#[unsafe(link_section = ".limine_requests")]
pub(super) static HHDM_INFO_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
pub(super) static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".limine_requests")]
pub(super) static KERNEL_ADDRESS_REQUEST: ExecutableAddressRequest =
    ExecutableAddressRequest::new();

pub fn get_hhdm_start() -> VirtualAddress {
    let response = HHDM_INFO_REQUEST
        .get_response()
        .expect("hhdm info response not received");

    VirtualAddress::new(response.offset())
}

pub fn get_kernel_physical_base() -> PhysicalAddress {
    PhysicalAddress::new(
        KERNEL_ADDRESS_REQUEST
            .get_response()
            .expect("kernel address response not received")
            .physical_base(),
    )
}

pub fn get_kernel_virtual_base() -> VirtualAddress {
    VirtualAddress::new(
        KERNEL_ADDRESS_REQUEST
            .get_response()
            .expect("kernel address response not received")
            .virtual_base(),
    )
}

#[derive(PartialEq, Clone, Copy)]
pub enum MemoryMapType {
    Usable,
    Reserved,
    ACPIReclaimable,
    ACPINVS,
    BadMemory,
    BootloaderReclaimable,
    KernelBinaries,
    Framebuffer,
    Unknown,
}

pub struct MemoryMapEntry {
    pub start: PageFrameNumber,
    pub size: PageSize,
    pub entry_type: MemoryMapType,
}

pub struct MemoryMapView {
    limine_map: &'static MemoryMapResponse,
}

impl MemoryMapView {
    pub fn get() -> MemoryMapView {
        let response = MEMORY_MAP_REQUEST
            .get_response()
            .expect("memory map response not received");

        MemoryMapView {
            limine_map: response,
        }
    }

    fn translate(entry: &memory_map::Entry) -> MemoryMapEntry {
        MemoryMapEntry {
            start: PhysicalAddress::new(entry.base).frame_aligned(),
            size: ByteSize::new(entry.length)
                .try_into()
                .expect("hhdm entry size is not page aligned"),
            entry_type: match entry.entry_type {
                EntryType::USABLE => MemoryMapType::Usable,
                EntryType::RESERVED => MemoryMapType::Reserved,
                EntryType::ACPI_RECLAIMABLE => MemoryMapType::ACPIReclaimable,
                EntryType::ACPI_NVS => MemoryMapType::ACPINVS,
                EntryType::BAD_MEMORY => MemoryMapType::BadMemory,
                EntryType::BOOTLOADER_RECLAIMABLE => MemoryMapType::BootloaderReclaimable,
                EntryType::EXECUTABLE_AND_MODULES => MemoryMapType::KernelBinaries,
                EntryType::FRAMEBUFFER => MemoryMapType::Framebuffer,
                _ => MemoryMapType::Unknown,
            },
        }
    }

    pub fn at(&self, index: usize) -> MemoryMapEntry {
        Self::translate(self.limine_map.entries()[index])
    }

    pub fn len(&self) -> usize {
        self.limine_map.entries().len()
    }

    pub fn iter(&self) -> impl Iterator<Item = MemoryMapEntry> {
        self.limine_map.entries().iter().map(|f| Self::translate(f))
    }
}
