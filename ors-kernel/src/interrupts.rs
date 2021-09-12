use super::global::default_serial_port;
use super::paging::KernelAcpiHandler;
use super::segmentation::DOUBLE_FAULT_IST_INDEX;
use super::x64;
use acpi::AcpiTables;
use log::{error, info, trace};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use spin::{Lazy, Mutex, Once};

static mut IDT: x64::InterruptDescriptorTable = x64::InterruptDescriptorTable::new();
static LAPIC: Once<x64::LApic> = Once::new();

const EXTERNAL_IRQ_OFFSET: u32 = 32; // first 32 entries are reserved by CPU
const IRQ_KBD: u32 = 1; // Keyboard on PS/2 port
const IRQ_COM1: u32 = 4; // First serial port

pub unsafe fn initialize(rsdp: usize) {
    initialize_idt();
    disable_pic_8259();
    initialize_apic(rsdp);
}

pub fn enable() {
    x64::interrupts::enable();
}

pub fn without_interrupts<T>(f: impl FnOnce() -> T) -> T {
    x64::interrupts::without_interrupts(f)
}

unsafe fn initialize_idt() {
    IDT.breakpoint.set_handler_fn(breakpoint_handler);
    IDT.page_fault.set_handler_fn(page_fault_handler);
    IDT.double_fault
        .set_handler_fn(double_fault_handler)
        .set_stack_index(DOUBLE_FAULT_IST_INDEX);
    IDT[(EXTERNAL_IRQ_OFFSET + IRQ_KBD) as usize].set_handler_fn(kbd_handler);
    IDT[(EXTERNAL_IRQ_OFFSET + IRQ_COM1) as usize].set_handler_fn(com1_handler);
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
    let ioapic_id = apic.io_apics.first().unwrap().id;

    let processor_info = info.processor_info.unwrap();
    let bp = processor_info.boot_processor;
    let aps = processor_info.application_processors;

    trace!("{:?}, {:?}, bp = {:?}, aps = {:?}", lapic, ioapic, bp, aps);

    LAPIC.call_once(|| lapic);

    // TODO: Understand the detailed semantics of these setup processes
    // https://wiki.osdev.org/APIC
    // https://github.com/mit-pdos/xv6-public/blob/master/lapic.c#L55
    {
        const ENABLE: u32 = 0x100;
        const MASKED: u32 = 0x10000;
        const BCAST: u32 = 0x80000;
        const INIT: u32 = 0x00500;
        const LEVEL: u32 = 0x08000;
        const DELIVS: u32 = 0x01000;

        // Enable the Local APIC to receive interrupts by configuring the Spurious Interrupt Vector Register.
        lapic.set_svr(ENABLE | 0xFF);

        // TODO: Timer?

        // Disable  logical interrupt lines
        lapic.set_lint0(MASKED);
        lapic.set_lint1(MASKED);

        // Disable performance counter overflow interrupts on machines that provide that interrupt entry.
        if (lapic.ver() >> 16) & 0xFF >= 4 {
            lapic.set_pcint(MASKED);
        }

        // TODO: Error interrupt?

        // Ack any outstanding interrupts
        lapic.set_eoi(0);

        // Send an Init Level De-Assert to synchronise arbitration ID's.
        lapic.set_icrhi(0);
        lapic.set_icrlo(BCAST | INIT | LEVEL);
        while (lapic.icrlo() & DELIVS) != 0 {}

        // Enable interrupts on the APIC (but not on the processor)
        lapic.set_tpr(0);
    }

    // https://github.com/mit-pdos/xv6-public/blob/master/ioapic.c#L49
    {
        // const LEVEL: u64 = 0x00008000; // Level-triggered (vs edge-)
        // const ACTIVELOW: u64 = 0x00002000; // Active low (vs high)
        // const LOGICAL: u64 = 0x00000800; // Destination is CPU id (vs APIC ID)
        const DISABLED: u64 = 0x00010000; // Interrupt disabled

        let max_intr = ioapic.ver() >> 16 & 0xFF;
        if ioapic.apic_id() != ioapic_id {
            panic!("ioapic id mismatch: not a MP");
        }

        // Mark all interrupts edge-triggered, active high, disabled, and not routed to any CPUs.
        for i in 0..max_intr {
            ioapic.set_redirection_table_at(i, DISABLED | (EXTERNAL_IRQ_OFFSET + i) as u64);
        }

        let cpu0 = (bp.local_apic_id as u64) << (24 + 32);
        ioapic.set_redirection_table_at(IRQ_KBD, (EXTERNAL_IRQ_OFFSET + IRQ_KBD) as u64 | cpu0);
        ioapic.set_redirection_table_at(IRQ_COM1, (EXTERNAL_IRQ_OFFSET + IRQ_COM1) as u64 | cpu0);
    }
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: x64::InterruptStackFrame) {
    without_interrupts(|| {
        info!("EXCEPTION: BREAKPOINT");
        info!("{:#?}", stack_frame);
    });
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: x64::InterruptStackFrame,
    error_code: x64::PageFaultErrorCode,
) {
    without_interrupts(|| {
        info!("EXCEPTION: PAGE FAULT");
        info!("Address: {:?}", x64::Cr2::read());
        info!("Error Code: {:?}", error_code);
        info!("{:#?}", stack_frame);
    });

    loop {
        x64::hlt()
    }
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: x64::InterruptStackFrame,
    _error_code: u64,
) -> ! {
    without_interrupts(|| {
        error!("EXCEPTION: DOUBLE FAULT");
        error!("{:#?}", stack_frame);
    });

    loop {
        x64::hlt()
    }
}

static KEYBOARD: Lazy<Mutex<Keyboard<layouts::Jis109Key, ScancodeSet1>>> = Lazy::new(|| {
    Mutex::new(Keyboard::new(
        layouts::Jis109Key,
        ScancodeSet1,
        HandleControl::Ignore,
    ))
});

extern "x86-interrupt" fn kbd_handler(_stack_frame: x64::InterruptStackFrame) {
    without_interrupts(|| {
        let mut keyboard = KEYBOARD.lock();
        if let Ok(Some(e)) = keyboard.add_byte(unsafe { x64::Port::new(0x60).read() }) {
            if let Some(key) = keyboard.process_keyevent(e) {
                match key {
                    DecodedKey::RawKey(key) => info!("KBD: {:?}", key),
                    DecodedKey::Unicode(ch) => info!("KBD: {}", ch),
                }
            }
        }
    });

    unsafe { LAPIC.wait().set_eoi(0) };
}

extern "x86-interrupt" fn com1_handler(_stack_frame: x64::InterruptStackFrame) {
    without_interrupts(|| {
        let input = default_serial_port().receive();
        info!("COM1: {}", char::from(input));
    });
    unsafe { LAPIC.wait().set_eoi(0) };
}

unsafe fn disable_pic_8259() {
    x64::Port::new(0xa1).write(0xffu8);
    x64::Port::new(0x21).write(0xffu8);
}
