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
pub mod global;
pub mod graphics;
pub mod interrupts;
pub mod logger;
pub mod paging;
pub mod pci;
pub mod phys_memory;
pub mod qemu;
pub mod segmentation;

use log::{error, info};
use ors_common::frame_buffer::FrameBuffer as RawFrameBuffer;
use ors_common::memory_map::MemoryMap;
use x86_64::instructions as x64;

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &RawFrameBuffer, mm: &MemoryMap) {
    unsafe { segmentation::initialize() };
    unsafe { paging::initialize() };
    unsafe { interrupts::initialize() };
    global::frame_manager().initialize(mm);
    global::initialize_frame_buffer(unsafe {
        static mut PAYLOAD: graphics::FrameBufferPayload = graphics::FrameBufferPayload::new();
        graphics::prepare_frame_buffer(*fb, &mut PAYLOAD)
    });
    global::initialize_devices(pci::Device::scan::<32>().unwrap());
    logger::initialize();

    global::frame_buffer().clear(graphics::Color::BLACK);

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
