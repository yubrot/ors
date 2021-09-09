use super::segmentation::DOUBLE_FAULT_IST_INDEX;
use log::{error, info};

mod x64 {
    pub use x86_64::instructions::hlt;
    pub use x86_64::registers::control::Cr2;
    pub use x86_64::structures::idt::{
        InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode,
    };
}

static mut IDT: x64::InterruptDescriptorTable = x64::InterruptDescriptorTable::new();

pub unsafe fn initialize() {
    IDT.breakpoint.set_handler_fn(breakpoint_handler);
    IDT.page_fault.set_handler_fn(page_fault_handler);
    IDT.double_fault
        .set_handler_fn(double_fault_handler)
        .set_stack_index(DOUBLE_FAULT_IST_INDEX);
    IDT.load();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: x64::InterruptStackFrame) {
    info!("EXCEPTION: BREAKPOINT");
    info!("{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: x64::InterruptStackFrame,
    error_code: x64::PageFaultErrorCode,
) {
    info!("EXCEPTION: PAGE FAULT");
    info!("Address: {:?}", x64::Cr2::read());
    info!("Error Code: {:?}", error_code);
    info!("{:#?}", stack_frame);

    loop {
        x64::hlt()
    }
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: x64::InterruptStackFrame,
    _error_code: u64,
) -> ! {
    error!("EXCEPTION: DOUBLE FAULT");
    error!("{:#?}", stack_frame);

    loop {
        x64::hlt()
    }
}
