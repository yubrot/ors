use crate::cpu;
use crate::devices;
use crate::paging::KernelAcpiHandler;
use crate::segmentation::DOUBLE_FAULT_IST_INDEX;
use crate::task;
use crate::x64;
use acpi::platform::address::AddressSpace;
use acpi::AcpiTables;
use core::sync::atomic::{AtomicUsize, Ordering};
use heapless::mpmc::Q64 as Queue;
use log::trace;
use spin::Once;

pub static TICKS: AtomicUsize = AtomicUsize::new(0);

pub const TIMER_FREQ: u32 = 250;

#[derive(Debug)]
pub enum Event {
    Kbd(u8),
    Com1(u8),
    Timer,
}

static EVENT_QUEUE: Queue<Event> = Queue::new();

pub fn event_queue() -> &'static Queue<Event> {
    &EVENT_QUEUE
}

pub fn ticks() -> usize {
    TICKS.load(Ordering::SeqCst)
}

pub unsafe fn initialize(rsdp: usize) {
    initialize_idt();
    disable_pic_8259();
    initialize_apic(rsdp);
}

/// Clear Interrupt Flag. Interrupts are disabled while this value is alive.
#[derive(Debug)]
pub struct Cli;

impl Cli {
    pub fn new() -> Self {
        let cli = !x64::interrupts::are_enabled();
        x64::interrupts::disable();
        let mut cpu = cpu::Cpu::current().info().lock();
        if cpu.ncli == 0 {
            cpu.zcli = cli;
        }
        cpu.ncli += 1;
        Self
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        assert!(
            !x64::interrupts::are_enabled(),
            "Inconsistent interrupt flag"
        );
        let mut cpu = cpu::Cpu::current().info().lock();
        cpu.ncli -= 1;
        let sti = cpu.ncli == 0 && !cpu.zcli;
        drop(cpu);
        if sti {
            x64::interrupts::enable();
        }
    }
}

static mut IDT: x64::InterruptDescriptorTable = x64::InterruptDescriptorTable::new();

unsafe fn initialize_idt() {
    IDT.breakpoint
        .set_handler_fn(breakpoint_handler)
        .disable_interrupts(true);
    IDT.page_fault
        .set_handler_fn(page_fault_handler)
        .disable_interrupts(true);
    IDT.double_fault
        .set_handler_fn(double_fault_handler)
        .set_stack_index(DOUBLE_FAULT_IST_INDEX)
        .disable_interrupts(true);
    IDT[(EXTERNAL_IRQ_OFFSET + IRQ_TIMER) as usize]
        .set_handler_fn(timer_handler)
        .disable_interrupts(true);
    IDT[(EXTERNAL_IRQ_OFFSET + IRQ_KBD) as usize]
        .set_handler_fn(kbd_handler)
        .disable_interrupts(true);
    IDT[(EXTERNAL_IRQ_OFFSET + IRQ_COM1) as usize]
        .set_handler_fn(com1_handler)
        .disable_interrupts(true);
    IDT.load();
}

static LAPIC: Once<x64::LApic> = Once::new();

const EXTERNAL_IRQ_OFFSET: u32 = 32; // first 32 entries are reserved by CPU
const IRQ_TIMER: u32 = 0;
const IRQ_KBD: u32 = 1; // Keyboard on PS/2 port
const IRQ_COM1: u32 = 4; // First serial port

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

    // https://wiki.osdev.org/ACPI_Timer
    let pm_timer = info.pm_timer.expect("Could not find ACPI PM Timer");
    assert_eq!(pm_timer.base.address_space, AddressSpace::SystemIo); // TODO: MMIO Support
    assert_eq!(pm_timer.base.bit_width, 32);
    let pm_timer_port = x64::Port::<u32>::new(pm_timer.base.address as u16);

    let processor_info = info.processor_info.unwrap();
    let bsp = processor_info.boot_processor;
    let aps = processor_info.application_processors;

    trace!("{:?}, {:?}, bp = {:?}, aps = {:?}", lapic, ioapic, bsp, aps);
    assert_eq!(lapic.apic_id(), bsp.local_apic_id);
    assert_eq!(ioapic.apic_id(), ioapic_id);

    cpu::initialize(
        apic.local_apic_address,
        bsp.local_apic_id,
        aps.iter().map(|ap| ap.local_apic_id),
    );
    LAPIC.call_once(|| lapic);

    // TODO: Understand the detailed semantics of these setup processes
    // https://wiki.osdev.org/APIC
    // https://github.com/mit-pdos/xv6-public/blob/master/lapic.c#L55
    {
        const ENABLE: u32 = 0x100;
        const X1: u32 = 0b1011; // divide by 1 (Divide Configuration Register)
        const PERIODIC: u32 = 0x20000; // vs ONE_SHOT
        const MASKED: u32 = 0x10000;
        const BCAST: u32 = 0x80000;
        const INIT: u32 = 0x00500;
        const LEVEL: u32 = 0x08000;
        const DELIVS: u32 = 0x01000;

        // Enable the Local APIC to receive interrupts by configuring the Spurious Interrupt Vector Register.
        lapic.set_svr(ENABLE | 0xFF);

        // Measure the frequency of the Local APIC Timer
        lapic.set_tdcr(X1);
        lapic.set_timer(MASKED);
        lapic.set_ticr(u32::MAX); // start
        wait_milliseconds_with_pm_timer(pm_timer_port, pm_timer.supports_32bit, 100);
        let measured_lapic_timer_freq = (u32::MAX - lapic.tccr()) * 10;
        lapic.set_ticr(0); // stop

        // Enable timer interrupts
        lapic.set_tdcr(X1);
        lapic.set_timer(PERIODIC | (EXTERNAL_IRQ_OFFSET + IRQ_TIMER));
        lapic.set_ticr(measured_lapic_timer_freq / TIMER_FREQ);

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

        // Mark all interrupts edge-triggered, active high, disabled, and not routed to any CPUs.
        for i in 0..max_intr {
            ioapic.set_redirection_table_at(i, DISABLED | (EXTERNAL_IRQ_OFFSET + i) as u64);
        }

        let cpu0 = (bsp.local_apic_id as u64) << (24 + 32);
        ioapic.set_redirection_table_at(IRQ_KBD, (EXTERNAL_IRQ_OFFSET + IRQ_KBD) as u64 | cpu0);
        ioapic.set_redirection_table_at(IRQ_COM1, (EXTERNAL_IRQ_OFFSET + IRQ_COM1) as u64 | cpu0);
    }
}

unsafe fn disable_pic_8259() {
    x64::Port::new(0xa1).write(0xffu8);
    x64::Port::new(0x21).write(0xffu8);
}

fn wait_milliseconds_with_pm_timer(mut time: x64::Port<u32>, supports_32bit: bool, msec: u32) {
    const PM_TIMER_FREQ: usize = 3579545;
    let start = unsafe { time.read() };
    let mut end = start.wrapping_add((PM_TIMER_FREQ * msec as usize / 1000) as u32);
    if !supports_32bit {
        end &= 0x00ffffff;
    }
    if end < start {
        while unsafe { time.read() } >= start {}
    }
    while unsafe { time.read() } < end {}
}

// Be careful to avoid deadlocks:
// https://matklad.github.io/2020/01/02/spinlocks-considered-harmful.html

extern "x86-interrupt" fn breakpoint_handler(stack_frame: x64::InterruptStackFrame) {
    sprintln!("EXCEPTION: BREAKPOINT");
    sprintln!("{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: x64::InterruptStackFrame,
    error_code: x64::PageFaultErrorCode,
) {
    sprintln!("EXCEPTION: PAGE FAULT");
    sprintln!("Address: {:?}", x64::Cr2::read());
    sprintln!("Error Code: {:?}", error_code);
    sprintln!("{:#?}", stack_frame);

    loop {
        x64::hlt()
    }
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: x64::InterruptStackFrame,
    _error_code: u64,
) -> ! {
    sprintln!("EXCEPTION: DOUBLE FAULT");
    sprintln!("{:#?}", stack_frame);

    loop {
        x64::hlt()
    }
}

extern "x86-interrupt" fn timer_handler(_stack_frame: x64::InterruptStackFrame) {
    let msg = Event::Timer;
    let _ = event_queue().enqueue(msg);
    TICKS.fetch_add(1, Ordering::SeqCst);
    unsafe { LAPIC.wait().set_eoi(0) };
    unsafe { task::task_manager().switch(None) };
}

extern "x86-interrupt" fn kbd_handler(_stack_frame: x64::InterruptStackFrame) {
    let msg = Event::Kbd(unsafe { x64::Port::new(0x60).read() });
    let _ = event_queue().enqueue(msg);
    unsafe { LAPIC.wait().set_eoi(0) };
}

extern "x86-interrupt" fn com1_handler(_stack_frame: x64::InterruptStackFrame) {
    let byte = devices::serial::default_port().receive();
    let _ = event_queue().enqueue(Event::Com1(byte));
    unsafe { LAPIC.wait().set_eoi(0) };
}
