extern crate alloc;

use super::paging::PageTableSet;
use crate::{
    arch::{
        halt,
        x86_64::{GlobalDescriptorTable, InterruptStackTable},
    },
    mem::VirtualAddress,
    mp::{CORE_ID, CoreId, get_cpu_local_offset, init_cpu_local_table},
};
use core::{arch::asm, sync::atomic::Ordering};
use limine::{mp::Cpu, request::MpRequest};
use log::info;
use x86::{msr::wrmsr, vmx::vmcs::guest::GS_BASE};

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
    let ptr = get_cpu_local_offset(core_id);
    unsafe { wrmsr(GS_BASE, ptr) };
}

pub fn initialize_mp(tables: &PageTableSet) -> ! {
    let response = MP_REQUEST.get_response().expect("mp response not received");

    let n_cores = response.cpus().len();
    info!("x86::initialize_mp(): bootstrapping {} cores", n_cores);

    init_cpu_local_table(&tables, n_cores);

    let core_id: u64 = 1;
    let bsp_id = response.bsp_lapic_id();

    let mut core_self = None;

    for cpu in response.cpus() {
        if bsp_id != cpu.lapic_id {
            cpu.extra.store(core_id, Ordering::SeqCst);
            cpu.goto_address.write(initialize_core);
        } else {
            core_self = Some(cpu);
        }
    }

    unsafe { initialize_core(core_self.expect("limine did not give current CPU in MP response")) };
}

unsafe extern "C" fn initialize_core(cpu: &Cpu) -> ! {
    init_cpu_local_ptr(CoreId(cpu.extra.load(Ordering::SeqCst) as usize));

    info!("hi from core: {}", CORE_ID.0);

    let ist = InterruptStackTable::default();
    let gdt = GlobalDescriptorTable::new(&ist);
    unsafe { gdt.load() };

    halt();
}
