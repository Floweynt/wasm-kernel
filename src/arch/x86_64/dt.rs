use crate::arch::x86_64::interrupt::irq_handler_entry;
use seq_macro::seq;
use x86::{
    Ring,
    bits64::segmentation::Descriptor64,
    dtables::{DescriptorTablePointer, lgdt, lidt},
    segmentation::{
        BuildDescriptor, CodeSegmentType, DataSegmentType, Descriptor, DescriptorBuilder,
        GateDescriptorBuilder, SegmentDescriptorBuilder, SegmentSelector, load_cs, load_ds,
        load_es, load_fs, load_gs, load_ss,
    },
    task::load_tr,
};

#[repr(C, packed)]
#[derive(Default)]
pub(super) struct InterruptStackTable {
    pub reserved0: u32,
    pub rsp0: usize,
    pub rsp1: usize,
    pub rsp2: usize,
    pub reserved1: u64,
    pub ist1: usize,
    pub ist2: usize,
    pub ist3: usize,
    pub ist4: usize,
    pub ist5: usize,
    pub ist6: usize,
    pub ist7: usize,
    pub reserved2: u64,
    pub reserved3: u16,
    pub io_bp: u16,
}

#[repr(C, packed)]
pub(super) struct GlobalDescriptorTable {
    null: Descriptor,
    cs: Descriptor,
    ds: Descriptor,
    tss: Descriptor64,
}

#[repr(C, packed)]
pub(super) struct InterruptDescriptorTable {
    pub entries: [Descriptor64; 256],
}

impl GlobalDescriptorTable {
    pub const CS: u16 = 1;
    pub const DS: u16 = 2;
    pub const TSS: u16 = 3;

    pub fn new(ist: &InterruptStackTable) -> GlobalDescriptorTable {
        let cs: Descriptor =
            DescriptorBuilder::code_descriptor(0, 0xfffff, CodeSegmentType::ExecuteRead)
                .present()
                .dpl(Ring::Ring0)
                .limit_granularity_4kb()
                .l()
                .finish();

        let ds: Descriptor =
            DescriptorBuilder::data_descriptor(0, 0xfffff, DataSegmentType::ReadWrite)
                .present()
                .dpl(Ring::Ring0)
                .limit_granularity_4kb()
                .l()
                .finish();

        let tss: Descriptor64 = <DescriptorBuilder as GateDescriptorBuilder<u64>>::tss_descriptor(
            &raw const ist as u64,
            size_of::<InterruptStackTable>() as u64,
            true,
        )
        .present()
        .finish();

        GlobalDescriptorTable {
            null: Descriptor::NULL,
            cs,
            ds,
            tss,
        }
    }

    pub unsafe fn load(&self) {
        unsafe {
            lgdt(&DescriptorTablePointer::new(self));
            load_cs(SegmentSelector::new(Self::CS, Ring::Ring0));
            load_ds(SegmentSelector::new(Self::DS, Ring::Ring0));
            load_ss(SegmentSelector::new(Self::DS, Ring::Ring0));
            load_es(SegmentSelector::new(Self::DS, Ring::Ring0));
            load_fs(SegmentSelector::new(Self::DS, Ring::Ring0));
            load_gs(SegmentSelector::new(Self::DS, Ring::Ring0));
            load_tr(SegmentSelector::new(Self::TSS, Ring::Ring0))
        }
    }
}

impl InterruptDescriptorTable {
    fn pack_idt_entry(addr: u64, ist: u8, dpl: Ring) -> Descriptor64 {
        DescriptorBuilder::interrupt_descriptor(
            SegmentSelector::new(GlobalDescriptorTable::CS, Ring::Ring0),
            addr,
        )
        .present()
        .dpl(dpl)
        .ist(ist)
        .finish()
    }

    pub fn new() -> InterruptDescriptorTable {
        let mut entries = [Descriptor64::default(); 256];

        // exception handler
        seq!(N in 0..=21 {
            let address = irq_handler_entry::<N> as *const () as u64;
            // always switch to stack 1
            entries[N] = Self::pack_idt_entry(address, 1, Ring::Ring0);
        });

        // regular irq handlers
        seq!(N in 32..=255 {
            let address = irq_handler_entry::<N> as *const () as u64;
            entries[N] = Self::pack_idt_entry(address, 1, Ring::Ring0);
        });

        InterruptDescriptorTable { entries }
    }

    pub unsafe fn load(&self) {
        unsafe {
            lidt(&DescriptorTablePointer::new(self));
        }
    }
}
