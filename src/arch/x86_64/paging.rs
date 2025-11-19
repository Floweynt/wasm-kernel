use super::{
    HIGHER_HALF_VIRTUAL_ADDRESS_BASE_PML4, HIGHER_HALF_VIRTUAL_ADDRESS_BASE_PML5,
    SMALL_PAGE_PAGE_SIZE,
};
use crate::{
    arch::{LARGE_PAGE_PAGE_SIZE, MEDIUM_PAGE_PAGE_SIZE},
    mem::{
        PageFrameAllocator, PageFrameNumber, PageSize, PhysicalAddress, VirtualAddress,
        VirtualPageFrameNumber, Wrapper,
    },
};
use limine::{paging::Mode, request::PagingModeRequest};
use x86::{
    bits64::paging::{
        PAGE_SIZE_ENTRIES, PAddr, PD, PDEntry, PDFlags, PDPT, PDPTEntry, PDPTFlags, PML4,
        PML4Entry, PML4Flags, PT, PTEntry, PTFlags, pd_index, pdpt_index, pml4_index, pt_index,
    },
    controlregs::cr3_write,
};

#[cfg(any(target_arch = "x86_64"))]
#[used]
#[unsafe(link_section = ".limine_requests")]
static PAGING_MODE_REQUEST: PagingModeRequest =
    PagingModeRequest::new().with_mode(Mode::FIVE_LEVEL);

pub fn get_higher_half_addr() -> VirtualAddress {
    if let Some(res) = PAGING_MODE_REQUEST.get_response() {
        if res.mode() == Mode::FIVE_LEVEL {
            return HIGHER_HALF_VIRTUAL_ADDRESS_BASE_PML5;
        }
    }

    return HIGHER_HALF_VIRTUAL_ADDRESS_BASE_PML4;
}

// TODO: this should really be dynamic based on the current paging mode
#[derive(Clone, Copy)]
pub struct PageTableSet {
    pml_addr: PageFrameNumber,
}

trait PageTableEntry: Copy {
    fn create_page_map(addr: PageFrameNumber) -> Self;
    fn address(self) -> PAddr;
    fn present(self) -> bool;
}

macro impl_pte($ident:ident, $flags:ident) {
    impl PageTableEntry for $ident {
        fn create_page_map(addr: PageFrameNumber) -> Self {
            return $ident::new(
                PAddr(addr.address().value()),
                $flags::P | $flags::RW | $flags::US,
            );
        }

        fn address(self) -> PAddr {
            return self.address();
        }

        fn present(self) -> bool {
            return self.is_present();
        }
    }
}

impl_pte!(PML4Entry, PML4Flags);
impl_pte!(PDPTEntry, PDPTFlags);
impl_pte!(PDEntry, PDFlags);
impl_pte!(PTEntry, PTFlags);

// TODO: make this bitflags?
pub struct PageFlags {
    pub write: bool,
    pub user: bool,
    pub execute: bool,
    pub global: bool,
}

impl PageFlags {
    pub const KERNEL_RW: PageFlags = PageFlags {
        write: true,
        user: false,
        execute: false,
        global: true,
    };

    pub const KERNEL_RO: PageFlags = PageFlags {
        write: false,
        user: false,
        execute: false,
        global: true,
    };

    pub const KERNEL_X: PageFlags = PageFlags {
        write: false,
        user: false,
        execute: true,
        global: true,
    };
}

macro tl_flag($expr:expr, $type:ident::$flag_name:ident) {
    if $expr {
        $type::$flag_name
    } else {
        $type::empty()
    }
}

impl PageTableSet {
    pub fn new<T: PageFrameAllocator>(alloc: &mut T) -> PageTableSet {
        PageTableSet {
            pml_addr: alloc.allocate_single_page(),
        }
    }

    fn walk_entry<'a, T: PageFrameAllocator, U: PageTableEntry, P>(
        alloc: &mut T,
        table: &'a mut [U; PAGE_SIZE_ENTRIES],
        index: usize,
    ) -> &'a mut P {
        if !table[index].present() {
            table[index] = U::create_page_map(alloc.allocate_single_page());
        }

        let ptr = PhysicalAddress::new(table[index].address().0)
            .to_virtual()
            .as_ptr_mut();

        unsafe { &mut *ptr }
    }

    // TODO: figure out semantics for overwriting entries

    pub fn map_page_small<T: PageFrameAllocator>(
        &mut self,
        alloc: &mut T,
        virt: VirtualPageFrameNumber,
        phys: PageFrameNumber,
        flags: &PageFlags,
    ) {
        let pml4_ptr = self.pml_addr.address().to_virtual().as_ptr_mut();
        let pml4: &mut PML4 = unsafe { &mut *pml4_ptr };

        let pdpt = Self::walk_entry::<T, _, PDPT>(alloc, pml4, pml4_index(virt.address().into()));
        let pd = Self::walk_entry::<T, _, PD>(alloc, pdpt, pdpt_index(virt.address().into()));
        let pt = Self::walk_entry::<T, _, PT>(alloc, pd, pd_index(virt.address().into()));

        // TODO: we can't be sure XD exists, so maybe we need to check that?
        pt[pt_index(virt.address().into())] = PTEntry::new(
            PAddr(phys.address().value()),
            PTFlags::P
                | tl_flag!(flags.write, PTFlags::RW)
                | tl_flag!(flags.user, PTFlags::US)
                | tl_flag!(!flags.execute, PTFlags::XD)
                | tl_flag!(flags.global, PTFlags::G),
        );
    }

    pub fn map_page_medium<T: PageFrameAllocator>(
        &mut self,
        alloc: &mut T,
        virt: VirtualPageFrameNumber,
        phys: PageFrameNumber,
        flags: &PageFlags,
    ) {
        assert!(virt.is_aligned(MEDIUM_PAGE_PAGE_SIZE));
        assert!(phys.is_aligned(MEDIUM_PAGE_PAGE_SIZE));

        let pml4_ptr = self.pml_addr.address().to_virtual().as_ptr_mut();
        let pml4: &mut PML4 = unsafe { &mut *pml4_ptr };

        let pdpt = Self::walk_entry::<T, _, PDPT>(alloc, pml4, pml4_index(virt.address().into()));
        let pd = Self::walk_entry::<T, _, PD>(alloc, pdpt, pdpt_index(virt.address().into()));

        // TODO: we can't be sure XD exists, so maybe we need to check that?
        pd[pd_index(virt.address().into())] = PDEntry::new(
            PAddr(phys.address().value()),
            PDFlags::P
                | PDFlags::PS
                | tl_flag!(flags.write, PDFlags::RW)
                | tl_flag!(flags.user, PDFlags::US)
                | tl_flag!(!flags.execute, PDFlags::XD)
                | tl_flag!(flags.global, PDFlags::G),
        );
    }

    pub fn map_page_large<T: PageFrameAllocator>(
        &mut self,
        alloc: &mut T,
        virt: VirtualPageFrameNumber,
        phys: PageFrameNumber,
        flags: &PageFlags,
    ) {
        assert!(virt.is_aligned(LARGE_PAGE_PAGE_SIZE));
        assert!(phys.is_aligned(LARGE_PAGE_PAGE_SIZE));

        let pml4_ptr = self.pml_addr.address().to_virtual().as_ptr_mut();
        let pml4: &mut PML4 = unsafe { &mut *pml4_ptr };

        let pdpt = Self::walk_entry::<T, _, PDPT>(alloc, pml4, pml4_index(virt.address().into()));

        // TODO: we can't be sure XD exists, so maybe we need to check that?
        pdpt[pdpt_index(virt.address().into())] = PDPTEntry::new(
            PAddr(phys.address().value()),
            PDPTFlags::P
                | PDPTFlags::PS
                | tl_flag!(flags.write, PDPTFlags::RW)
                | tl_flag!(flags.user, PDPTFlags::US)
                | tl_flag!(!flags.execute, PDPTFlags::XD)
                | tl_flag!(flags.global, PDPTFlags::G),
        );
    }

    pub fn map_range<T: PageFrameAllocator>(
        &mut self,
        alloc: &mut T,
        base: VirtualPageFrameNumber,
        phys: PageFrameNumber,
        size: PageSize,
        flags: &PageFlags,
    ) {
        let mut base = base;
        let end = base + size;
        let mut phys = phys;

        while base < end
            && !(base.is_aligned(MEDIUM_PAGE_PAGE_SIZE) && phys.is_aligned(MEDIUM_PAGE_PAGE_SIZE))
        {
            self.map_page_small(alloc, base, phys, flags);
            base += SMALL_PAGE_PAGE_SIZE;
            phys += SMALL_PAGE_PAGE_SIZE;
        }

        while base + MEDIUM_PAGE_PAGE_SIZE <= end
            && !(base.is_aligned(LARGE_PAGE_PAGE_SIZE) && phys.is_aligned(LARGE_PAGE_PAGE_SIZE))
        {
            self.map_page_medium(alloc, base, phys, flags);
            base += MEDIUM_PAGE_PAGE_SIZE;
            phys += MEDIUM_PAGE_PAGE_SIZE;
        }

        while base + LARGE_PAGE_PAGE_SIZE <= end {
            self.map_page_large(alloc, base, phys, flags);
            base += LARGE_PAGE_PAGE_SIZE;
            phys += LARGE_PAGE_PAGE_SIZE;
        }

        while base + MEDIUM_PAGE_PAGE_SIZE <= end {
            self.map_page_medium(alloc, base, phys, flags);
            base += MEDIUM_PAGE_PAGE_SIZE;
            phys += MEDIUM_PAGE_PAGE_SIZE;
        }

        while base < end {
            self.map_page_small(alloc, base, phys, flags);
            base += SMALL_PAGE_PAGE_SIZE;
            phys += SMALL_PAGE_PAGE_SIZE;
        }
    }

    pub unsafe fn set_current(&self) {
        unsafe {
            cr3_write(self.pml_addr.address().value());
        }
    }
}
