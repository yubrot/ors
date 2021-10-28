use crate::acpi;
use crate::console;
use crate::cpu::Cpu;
use crate::segmentation::DOUBLE_FAULT_IST_INDEX;
use crate::task;
use crate::x64;
use core::ops::Range;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Lazy;

pub const TIMER_FREQ: usize = 250;

static TICKS: AtomicUsize = AtomicUsize::new(0);

pub fn ticks() -> usize {
    TICKS.load(Ordering::SeqCst)
}

/// Clear Interrupt Flag. Interrupts are disabled while this value is alive.
#[derive(Debug)]
pub struct Cli;

impl Cli {
    pub fn new() -> Self {
        let cli = !x64::interrupts::are_enabled();
        x64::interrupts::disable();
        let mut cpu = Cpu::current().state().lock();
        if cpu.thread_state.ncli == 0 {
            cpu.thread_state.zcli = cli;
        }
        cpu.thread_state.ncli += 1;
        Self
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        assert!(
            !x64::interrupts::are_enabled(),
            "Inconsistent interrupt flag"
        );
        let mut cpu = Cpu::current().state().lock();
        cpu.thread_state.ncli -= 1;
        let sti = cpu.thread_state.ncli == 0 && !cpu.thread_state.zcli;
        drop(cpu);
        if sti {
            x64::interrupts::enable();
        }
    }
}

pub unsafe fn initialize() {
    IDT.load();
    disable_pic_8259();
    initialize_local_apic();
    initialize_io_apic();
}

const PIC_8259_IRQ_OFFSET: u32 = 32; // first 32 entries are reserved by CPU
const IRQ_TIMER: u32 = PIC_8259_IRQ_OFFSET + 0;
const IRQ_KBD: u32 = PIC_8259_IRQ_OFFSET + 1; // Keyboard on PS/2 port
const IRQ_COM1: u32 = PIC_8259_IRQ_OFFSET + 4; // First serial port

const VIRTIO_BLOCK_IRQ_OFFSET: u32 = PIC_8259_IRQ_OFFSET + 16; // next 16 entries are for 8259 PIC interrupts
const IRQ_VIRTIO_BLOCK: Range<u32> = VIRTIO_BLOCK_IRQ_OFFSET..VIRTIO_BLOCK_IRQ_OFFSET + 8;

static IDT: Lazy<x64::InterruptDescriptorTable> = Lazy::new(|| unsafe { prepare_idt() });

unsafe fn prepare_idt() -> x64::InterruptDescriptorTable {
    let mut idt = x64::InterruptDescriptorTable::new();
    idt.breakpoint
        .set_handler_fn(breakpoint_handler)
        .disable_interrupts(true);
    idt.page_fault
        .set_handler_fn(page_fault_handler)
        .disable_interrupts(true);
    idt.double_fault
        .set_handler_fn(double_fault_handler)
        .set_stack_index(DOUBLE_FAULT_IST_INDEX)
        .disable_interrupts(true);
    idt[IRQ_TIMER as usize]
        .set_handler_fn(timer_handler)
        .disable_interrupts(true);
    idt[IRQ_KBD as usize]
        .set_handler_fn(kbd_handler)
        .disable_interrupts(true);
    idt[IRQ_COM1 as usize]
        .set_handler_fn(com1_handler)
        .disable_interrupts(true);

    for (i, irq) in IRQ_VIRTIO_BLOCK.enumerate() {
        idt[irq as usize]
            .set_handler_fn(get_virtio_block_handler(i))
            .disable_interrupts(true);
    }

    idt
}

unsafe fn disable_pic_8259() {
    x64::Port::new(0xa1).write(0xffu8);
    x64::Port::new(0x21).write(0xffu8);
}

static LAPIC: Lazy<x64::LApic> =
    Lazy::new(|| x64::LApic::new(acpi::apic_info().local_apic_address));

unsafe fn initialize_local_apic() {
    // TODO: Understand the detailed semantics of these setup processes
    // https://wiki.osdev.org/APIC
    // https://github.com/mit-pdos/xv6-public/blob/master/lapic.c#L55
    const ENABLE: u32 = 0x100;
    const X1: u32 = 0b1011; // divide by 1 (Divide Configuration Register)
    const PERIODIC: u32 = 0x20000; // vs ONE_SHOT
    const MASKED: u32 = 0x10000;
    const BCAST: u32 = 0x80000;
    const INIT: u32 = 0x00500;
    const LEVEL: u32 = 0x08000;
    const DELIVS: u32 = 0x01000;

    // Enable the Local APIC to receive interrupts by configuring the Spurious Interrupt Vector Register.
    LAPIC.set_svr(ENABLE | 0xFF);

    // Measure the frequency of the Local APIC Timer
    LAPIC.set_tdcr(X1);
    LAPIC.set_timer(MASKED);
    LAPIC.set_ticr(u32::MAX); // start
    acpi::wait_milliseconds_with_pm_timer(100);
    let measured_lapic_timer_freq = (u32::MAX - LAPIC.tccr()) * 10;
    LAPIC.set_ticr(0); // stop

    // Enable timer interrupts
    LAPIC.set_tdcr(X1);
    LAPIC.set_timer(PERIODIC | IRQ_TIMER);
    LAPIC.set_ticr(measured_lapic_timer_freq / TIMER_FREQ as u32);

    // Disable  logical interrupt lines
    LAPIC.set_lint0(MASKED);
    LAPIC.set_lint1(MASKED);

    // Disable performance counter overflow interrupts on machines that provide that interrupt entry.
    if (LAPIC.ver() >> 16) & 0xFF >= 4 {
        LAPIC.set_pcint(MASKED);
    }

    // TODO: Error interrupt?

    // Ack any outstanding interrupts
    LAPIC.set_eoi(0);

    // Send an Init Level De-Assert to synchronise arbitration ID's.
    LAPIC.set_icrhi(0);
    LAPIC.set_icrlo(BCAST | INIT | LEVEL);
    while (LAPIC.icrlo() & DELIVS) != 0 {}

    // Enable interrupts on the APIC (but not on the processor)
    LAPIC.set_tpr(0);
}

unsafe fn initialize_io_apic() {
    let ioapic = x64::IoApic::new(acpi::apic_info().io_apics.first().unwrap().address as u64);

    // https://wiki.osdev.org/APIC
    // https://github.com/mit-pdos/xv6-public/blob/master/ioapic.c#L49

    // const ACTIVELOW: u64 = 0x00002000; // Active low (vs high)
    // const LOGICAL: u64 = 0x00000800; // Destination is CPU id (vs APIC ID)
    const LEVEL: u64 = 0x00008000; // Level-triggered (vs edge-)
    const DISABLED: u64 = 0x00010000; // Interrupt disabled

    let max_intr = ioapic.ver() >> 16 & 0xFF;

    // Mark all interrupts edge-triggered, active high, disabled, and not routed to any CPUs.
    for i in 0..max_intr {
        ioapic.set_redirection_table_at(i, DISABLED | (PIC_8259_IRQ_OFFSET + i) as u64);
    }

    let bsp = (Cpu::boot_strap().lapic_id().unwrap() as u64) << (24 + 32);
    ioapic.set_redirection_table_at(IRQ_KBD - PIC_8259_IRQ_OFFSET, IRQ_KBD as u64 | bsp | LEVEL);
    ioapic.set_redirection_table_at(
        IRQ_COM1 - PIC_8259_IRQ_OFFSET,
        IRQ_COM1 as u64 | bsp | LEVEL,
    );
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
    TICKS.fetch_add(1, Ordering::SeqCst);
    task::scheduler().elapse();
    unsafe { LAPIC.set_eoi(0) };
    task::scheduler().r#yield();
}

extern "x86-interrupt" fn kbd_handler(_stack_frame: x64::InterruptStackFrame) {
    let v = unsafe { x64::Port::new(0x60).read() };
    console::accept_raw_input(console::RawInput::Kbd(v));
    unsafe { LAPIC.set_eoi(0) };
}

extern "x86-interrupt" fn com1_handler(_stack_frame: x64::InterruptStackFrame) {
    use crate::devices::serial::default_port;

    let v = default_port().receive();
    console::accept_raw_input(console::RawInput::Com1(v));
    unsafe { LAPIC.set_eoi(0) };
}

extern "x86-interrupt" fn virtio_block_handler<const N: usize>(
    _stack_frame: x64::InterruptStackFrame,
) {
    use crate::devices::virtio::block;

    block::list()[N].collect();
    unsafe { LAPIC.set_eoi(0) };
}

fn get_virtio_block_handler(index: usize) -> extern "x86-interrupt" fn(x64::InterruptStackFrame) {
    match index {
        0 => virtio_block_handler::<0>,
        1 => virtio_block_handler::<1>,
        2 => virtio_block_handler::<2>,
        3 => virtio_block_handler::<3>,
        4 => virtio_block_handler::<4>,
        5 => virtio_block_handler::<5>,
        6 => virtio_block_handler::<6>,
        7 => virtio_block_handler::<7>,
        _ => panic!("Unsupported index"),
    }
}

pub fn virtio_block_irq(index: usize) -> Option<u32> {
    if index < IRQ_VIRTIO_BLOCK.len() {
        Some(IRQ_VIRTIO_BLOCK.start + index as u32)
    } else {
        None
    }
}
