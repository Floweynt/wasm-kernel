extern crate alloc;

use super::{dt::InterruptDescriptorTable, paging::PageTableSet};
use crate::{
    arch::{
        paging::PageFlags,
        x86_64::{GlobalDescriptorTable, InterruptStackTable},
    },
    ksmp,
    mem::{AddressRange, LOCAL_PAGE_TABLE, PMM, PageSize, VirtualAddress, Wrapper, vpa},
    mp::{CORE_ID, CoreId, core_local, get_cpu_local_offset, init_cpu_local_table},
};
use core::{
    arch::{asm, naked_asm},
    sync::atomic::Ordering,
};
use limine::{mp::Cpu, request::MpRequest};
use log::info;
use spin::Once;
use x86::msr::{IA32_GS_BASE, wrmsr};

#[used]
#[unsafe(link_section = ".limine_requests")]
static MP_REQUEST: MpRequest = MpRequest::new();

pub fn get_cpu_local_pointer() -> VirtualAddress {
    let mut val: u64;

    unsafe {
        asm!(
            "movq %gs:0, {}",
            lateout(reg) val,
            options(nostack, preserves_flags, pure, readonly, att_syntax),
        );
    }

    VirtualAddress::new(val)
}

fn init_cpu_local_ptr(core_id: CoreId) {
    let ptr = get_cpu_local_offset(core_id).value();
    unsafe { wrmsr(IA32_GS_BASE, ptr) };
}

static BOOTSTRAP_PT: Once<PageTableSet> = Once::new();

pub fn initialize_mp(tables: &PageTableSet) -> ! {
    let response = MP_REQUEST.get_response().expect("mp response not received");

    let n_cores = response.cpus().len();
    info!("x86::initialize_mp(): bootstrapping {} cores", n_cores);

    init_cpu_local_table(tables, n_cores);

    let mut core_id: u64 = 1;
    let bsp_id = response.bsp_lapic_id();

    let mut core_self = None;

    tables.map_kernel_pages(&PMM::get());

    BOOTSTRAP_PT.call_once(|| *tables);

    for cpu in response.cpus() {
        if bsp_id != cpu.lapic_id {
            cpu.extra.store(core_id, Ordering::SeqCst);
            core_id += 1;
            cpu.goto_address.write(initialize_core);
        } else {
            core_self = Some(cpu);
        }
    }

    unsafe { initialize_core(core_self.expect("limine did not give current CPU in MP response")) };
}

core_local! {
    IST: Once<InterruptStackTable> = Once::new();
    GDT: Once<GlobalDescriptorTable> = Once::new();
    IDT: Once<InterruptDescriptorTable> = Once::new();
}

unsafe extern "C" fn initialize_core(cpu: &Cpu) -> ! {
    fn allocate_sp(size: PageSize, msg: &str) -> u64 {
        vpa::get_global_vpa()
            .allocate_backed_padded(
                &PMM::get(),
                LOCAL_PAGE_TABLE.get().unwrap(),
                size,
                PageSize::new(1),
                PageFlags::KERNEL_RW,
            )
            .expect(msg)
            .leak()
            .as_va_range()
            .end()
            .value()
    }

    let id = CoreId(cpu.extra.load(Ordering::SeqCst) as usize);

    let pt = if id != CoreId(0) {
        // swap page tables for other cores
        let early_pt = BOOTSTRAP_PT.get().unwrap();
        unsafe { early_pt.set_current() };
        let pt = early_pt.duplicate(&PMM::get());
        unsafe { pt.set_current() };
        pt
    } else {
        // since this is core 0, we can inherit the page tables initialized by initialize_mp
        // earlier in kinit
        *BOOTSTRAP_PT.get().unwrap()
    };

    info!("hi from core (early): {}", id.0);

    init_cpu_local_ptr(id);

    CORE_ID.replace(id);
    LOCAL_PAGE_TABLE.call_once(|| pt);

    info!("hi from core: {}", CORE_ID.get());

    let ist = IST.call_once(|| {
        let mut ist = InterruptStackTable::default();

        ist.ist1 = allocate_sp(PageSize::new(32), "failed to allocate IST");
        ist.ist2 = allocate_sp(PageSize::new(32), "failed to allocate IST");
        ist.ist3 = allocate_sp(PageSize::new(32), "failed to allocate IST");
        ist.ist4 = allocate_sp(PageSize::new(32), "failed to allocate IST");
        ist.ist5 = allocate_sp(PageSize::new(32), "failed to allocate IST");
        ist.ist6 = allocate_sp(PageSize::new(32), "failed to allocate IST");
        ist.ist7 = allocate_sp(PageSize::new(32), "failed to allocate IST");

        ist
    });
    let gdt = GDT.call_once(|| GlobalDescriptorTable::new(ist));
    let idt = IDT.call_once(InterruptDescriptorTable::new);

    unsafe { gdt.load() };
    unsafe { idt.load() };

    // we need to re-load the core local, for Reasons
    init_cpu_local_ptr(id);

    // 8MB stack
    unsafe {
        switch_stack_to_ksmp(allocate_sp(
            PageSize::new(2048),
            "failed to allocate kernel smp init stack",
        ))
    };
}

#[unsafe(naked)]
unsafe extern "C" fn switch_stack_to_ksmp(new_sp: u64) -> ! {
    naked_asm!(
        "movq %rdi, %rsp",
        "pushq $0",
        "jmp {}",
        options(att_syntax),
        sym ksmp
    )
}
