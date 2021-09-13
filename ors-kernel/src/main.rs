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

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &RawFrameBuffer, mm: &MemoryMap, rsdp: u64) {
    logger::initialize();
    unsafe { segmentation::initialize() };
    unsafe { paging::initialize() };
    phys_memory::frame_manager().initialize(mm);
    unsafe { interrupts::initialize(rsdp as usize) };
    pci::initialize_devices();
    serial::default_port().init();

    interrupts::enable();

    graphics::initialize_frame_buffer(*fb);
    graphics::frame_buffer().clear(graphics::Color::BLACK);

    #[cfg(test)]
    test_main();

    info!("Hello, World!");

    loop {
        x64::hlt()
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
    info!("Running {} tests", tests.len());
    for test in tests {
        test();
    }

    qemu::exit(qemu::ExitCode::Success);
}
