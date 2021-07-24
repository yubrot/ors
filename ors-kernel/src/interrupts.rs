use log::info;

mod x64 {
    pub use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
}

static mut IDT: x64::InterruptDescriptorTable = x64::InterruptDescriptorTable::new();

pub unsafe fn initialize() {
    IDT.breakpoint.set_handler_fn(breakpoint_handler);
    IDT.load();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: x64::InterruptStackFrame) {
    info!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}
