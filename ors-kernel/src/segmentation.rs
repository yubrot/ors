use crate::x64::{self, Segment};
use log::trace;
use spin::Once;

static mut GDT: x64::GlobalDescriptorTable = x64::GlobalDescriptorTable::new();
static mut TSS: x64::TaskStateSegment = x64::TaskStateSegment::new();

static KERNEL_CS: Once<x64::SegmentSelector> = Once::new();
static KERNEL_SS: Once<x64::SegmentSelector> = Once::new();

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

pub fn cs() -> x64::SegmentSelector {
    *KERNEL_CS.wait()
}

pub fn ss() -> x64::SegmentSelector {
    *KERNEL_SS.wait()
}

pub unsafe fn initialize() {
    // TODO: GDT needs to be created for each processor.
    trace!("INITIALIZING segmentation");
    TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 4096 * 5;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
        let stack_start = x64::VirtAddr::from_ptr(&STACK[0]);
        let stack_end = stack_start + STACK_SIZE;
        stack_end
    };
    let code_selector = GDT.add_entry(x64::Descriptor::kernel_code_segment());
    let data_selector = GDT.add_entry(x64::Descriptor::kernel_data_segment());
    let tss_selector = GDT.add_entry(x64::Descriptor::tss_segment(&TSS));
    let null_ss = x64::SegmentSelector::new(0, x64::PrivilegeLevel::Ring0);
    GDT.load();
    x64::DS::set_reg(null_ss);
    x64::ES::set_reg(null_ss);
    x64::FS::set_reg(null_ss);
    x64::GS::set_reg(null_ss);
    x64::CS::set_reg(code_selector);
    x64::SS::set_reg(data_selector);
    x64::load_tss(tss_selector);

    KERNEL_CS.call_once(|| code_selector);
    KERNEL_SS.call_once(|| data_selector);
}
