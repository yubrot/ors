#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![feature(const_mut_refs)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

pub mod allocator;
pub mod cpu;
pub mod graphics;
pub mod interrupts;
pub mod logger;
pub mod paging;
pub mod pci;
pub mod phys_memory;
pub mod qemu;
pub mod segmentation;
pub mod serial;
pub mod x64;

use log::{error, info};
use ors_common::frame_buffer::FrameBuffer as RawFrameBuffer;
use ors_common::memory_map::MemoryMap;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &RawFrameBuffer, mm: &MemoryMap, rsdp: u64) {
    interrupts::disable();

    logger::initialize();
    unsafe { segmentation::initialize() };
    unsafe { paging::initialize() };
    phys_memory::frame_manager().initialize(mm);
    unsafe { interrupts::initialize(rsdp as usize) };
    pci::initialize_devices();
    serial::default_port().init();

    graphics::initialize_screen_console((*fb).into());

    interrupts::enable();

    #[cfg(test)]
    test_main();

    info!("Hello, World!");

    let mut kbd = Keyboard::new(layouts::Jis109Key, ScancodeSet1, HandleControl::Ignore);
    let mut next_msg = None;
    let mut tick = 0usize;

    loop {
        if let Some(msg) = next_msg
            .take()
            .or_else(|| interrupts::message_queue().dequeue())
        {
            match msg {
                interrupts::Message::Kbd(key) => {
                    if let Ok(Some(e)) = kbd.add_byte(key) {
                        if let Some(key) = kbd.process_keyevent(e) {
                            match key {
                                DecodedKey::RawKey(key) => info!("KBD: {:?}", key),
                                DecodedKey::Unicode(ch) => info!("KBD: {}", ch),
                            }
                        }
                    }
                }
                interrupts::Message::Com1(b) => {
                    info!("COM1: {}", char::from(b))
                }
                interrupts::Message::Timer => {
                    tick += 1;
                    if tick % interrupts::TIMER_FREQ as usize == 0 {
                        info!("COUNT: {}", tick / interrupts::TIMER_FREQ as usize);
                    }
                    graphics::screen_console().render();
                }
            }
        } else {
            interrupts::disable();
            if let Some(msg) = interrupts::message_queue().dequeue() {
                next_msg = Some(msg);
                interrupts::enable();
            } else {
                interrupts::enable_and_hlt();
            }
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    error!("{}", info);

    #[cfg(test)]
    qemu::exit(qemu::ExitCode::Failure);

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

    qemu::exit(qemu::ExitCode::Success);
}
