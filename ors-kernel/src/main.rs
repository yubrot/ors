#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(const_mut_refs)]
#![feature(drain_filter)]
#![feature(maybe_uninit_uninit_array)]
#![feature(maybe_uninit_array_assume_init)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

#[macro_use]
pub mod devices;
pub mod allocator;
pub mod context;
pub mod cpu;
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
    phys_memory::frame_manager().initialize(mm);
    unsafe { interrupts::initialize(rsdp as usize) };
    devices::pci::initialize_devices();
    devices::serial::default_port().init();
    graphics::initialize_screen_console((*fb).into());
    drop(cli);

    #[cfg(test)]
    test_main();

    task::task_manager().add(task::Priority::MIN, task_process_events, 0);
    task::task_manager().add(task::Priority::MIN, task_counter, 1);
    task::task_manager().add(task::Priority::MIN, task_counter, 2);

    loop {
        x64::hlt()
    }
}

extern "C" fn task_process_events(_: u64) -> ! {
    let mut kbd = Keyboard::new(layouts::Jis109Key, ScancodeSet1, HandleControl::Ignore);
    let mut draw = 0;

    loop {
        while let Some(msg) = interrupts::event_queue().dequeue() {
            match msg {
                interrupts::Event::Kbd(key) => {
                    if let Ok(Some(e)) = kbd.add_byte(key) {
                        if let Some(key) = kbd.process_keyevent(e) {
                            match key {
                                DecodedKey::RawKey(key) => info!("KBD: {:?}", key),
                                DecodedKey::Unicode(ch) => info!("KBD: {}", ch),
                            }
                        }
                    }
                }
                interrupts::Event::Com1(b) => {
                    info!("COM1: {}", char::from(b))
                }
                interrupts::Event::Timer => {
                    let next_draw = interrupts::ticks() * 10 / interrupts::TIMER_FREQ as usize;
                    if draw < next_draw {
                        draw = next_draw;
                        graphics::screen_console().render();
                    }
                }
            }
        }
        x64::hlt();
    }
}

extern "C" fn task_counter(i: u64) -> ! {
    let mut t = 0;
    let mut n = 0;
    loop {
        if t + 100 < interrupts::ticks() {
            t += 100;
            n += 1;
            info!("COUNTER[{}]: {}", i, n);
        }
        x64::hlt()
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
