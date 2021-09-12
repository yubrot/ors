use super::paging::KernelAcpiHandler;
use super::segmentation::DOUBLE_FAULT_IST_INDEX;
use super::x64;
use acpi::AcpiTables;
use log::{error, info, trace};

static mut IDT: x64::InterruptDescriptorTable = x64::InterruptDescriptorTable::new();

pub unsafe fn initialize(rsdp: usize) {
    initialize_idt();
    initialize_apic(rsdp);
}

unsafe fn initialize_idt() {
    IDT.breakpoint.set_handler_fn(breakpoint_handler);
    IDT.page_fault.set_handler_fn(page_fault_handler);
    IDT.double_fault
        .set_handler_fn(double_fault_handler)
        .set_stack_index(DOUBLE_FAULT_IST_INDEX);
    IDT.load();
}

unsafe fn initialize_apic(rsdp: usize) {
    // https://wiki.osdev.org/MADT
    let info = AcpiTables::from_rsdp(KernelAcpiHandler, rsdp)
        .unwrap()
        .platform_info()
        .unwrap();

    let apic = match info.interrupt_model {
        acpi::InterruptModel::Apic(apic) => apic,
        _ => panic!("Could not find APIC"),
    };

    let lapic = x64::LApic::new(apic.local_apic_address);
    let ioapic = x64::IoApic::new(apic.io_apics.first().unwrap().address as u64);
    let _ioapic_id = apic.io_apics.first().unwrap().id;

    let processor_info = info.processor_info.unwrap();
    let bp = processor_info.boot_processor;
    let aps = processor_info.application_processors;

    trace!("{:?}, {:?}, bp = {:?}, aps = {:?}", lapic, ioapic, bp, aps);
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
