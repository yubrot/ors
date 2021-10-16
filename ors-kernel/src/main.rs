#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(const_mut_refs)]
#![feature(maybe_uninit_uninit_array)]
#![feature(maybe_uninit_array_assume_init)]
#![feature(const_fn_fn_ptr_basics)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

#[macro_use]
pub mod print;
pub mod acpi;
pub mod allocator;
pub mod context;
pub mod cpu;
pub mod devices;
pub mod graphics;
pub mod interrupts;
pub mod logger;
pub mod paging;
pub mod phys_memory;
pub mod segmentation;
pub mod sync;
pub mod task;
pub mod x64;

use log::info;
use ors_common::frame_buffer::FrameBuffer as RawFrameBuffer;
use ors_common::memory_map::MemoryMap;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &RawFrameBuffer, mm: &MemoryMap, rsdp: u64) {
    x64::interrupts::enable(); // To ensure that interrupts are enabled by default

    let cli = interrupts::Cli::new();

    logger::register();
    unsafe { segmentation::initialize() };
    unsafe { paging::initialize() };
    unsafe { phys_memory::frame_manager().initialize(mm) };
    unsafe { acpi::initialize(paging::KernelAcpiHandler, rsdp as usize) };
    cpu::initialize();
    unsafe { interrupts::initialize() };
    devices::pci::initialize_devices();
    devices::serial::default_port().init();
    graphics::initialize_screen_console((*fb).into());

    #[cfg(not(test))]
    {
        task::scheduler().add(task::Priority::MAX, task_process_kbd, 0);
        task::scheduler().add(task::Priority::MAX, task_process_com1, 0);
        task::scheduler().add(task::Priority::MAX, task_render, 0);
        task::scheduler().add(task::Priority::L1, task_producer, 1);
        task::scheduler().add(task::Priority::L1, task_consumer, 2);
    }

    drop(cli);

    #[cfg(test)]
    test_main();

    loop {
        x64::hlt()
    }
}

extern "C" fn task_process_kbd(_: u64) -> ! {
    let mut kbd = Keyboard::new(layouts::Jis109Key, ScancodeSet1, HandleControl::Ignore);

    loop {
        let input = interrupts::kbd_queue().dequeue(None);
        if let Ok(Some(e)) = kbd.add_byte(input) {
            if let Some(key) = kbd.process_keyevent(e) {
                match key {
                    DecodedKey::RawKey(key) => info!("KBD: {:?}", key),
                    DecodedKey::Unicode(ch) => info!("KBD: {}", ch),
                }
            }
        }
    }
}

extern "C" fn task_process_com1(_: u64) -> ! {
    loop {
        let input = interrupts::com1_queue().dequeue(None);
        info!("COM1: {}", char::from(input));
    }
}

extern "C" fn task_render(_: u64) -> ! {
    loop {
        graphics::screen_console().render();
        task::scheduler().sleep(interrupts::TIMER_FREQ / 30);
    }
}

static EXAMPLE_QUEUE: sync::queue::Queue<u32, 128> = sync::queue::Queue::new();

extern "C" fn task_producer(_: u64) -> ! {
    loop {
        for i in 0..100 {
            kprintln!();
            for n in 0..i {
                kprint!("+");
                EXAMPLE_QUEUE.enqueue(n, None);
            }
            task::scheduler().sleep(interrupts::TIMER_FREQ);
        }
    }
}

extern "C" fn task_consumer(_: u64) -> ! {
    loop {
        let _ = EXAMPLE_QUEUE.dequeue(None);
        kprint!("-");
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    sprintln!("{}", info);

    #[cfg(test)]
    devices::qemu::exit(devices::qemu::ExitCode::Failure);

    loop {
        x64::hlt()
    }
}

#[global_allocator]
static ALLOCATOR: allocator::KernelAllocator = allocator::KernelAllocator::new();

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("Allocation error: {:?}", layout)
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    info!("RUNNING {} tests", tests.len());
    for test in tests {
        test();
    }

    devices::qemu::exit(devices::qemu::ExitCode::Success);
}
