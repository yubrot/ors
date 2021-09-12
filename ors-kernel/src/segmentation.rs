use super::x64::{self, Segment};

static mut GDT: x64::GlobalDescriptorTable = x64::GlobalDescriptorTable::new();
static mut TSS: x64::TaskStateSegment = x64::TaskStateSegment::new();

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

pub unsafe fn initialize() {
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
    GDT.load();
    x64::CS::set_reg(code_selector);
    x64::SS::set_reg(data_selector);
    x64::load_tss(tss_selector);
}
