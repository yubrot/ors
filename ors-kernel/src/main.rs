#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

pub mod global;
pub mod graphics;
pub mod logger;
pub mod memory_manager;
pub mod page_table;
pub mod pci;
pub mod qemu;
pub mod segmentation;

use log::{error, info};
use ors_common::frame_buffer::FrameBuffer as RawFrameBuffer;
use ors_common::memory_map::MemoryMap;
use x86_64::instructions as asm;

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &RawFrameBuffer, mm: &MemoryMap) {
    unsafe { segmentation::initialize() };
    unsafe { page_table::initialize() };
    global::memory_manager().initialize(mm);
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
    info!("1 + 2 = {}", 1 + 2);

    loop {
        asm::hlt()
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    error!("{}", info);

    #[cfg(test)]
    qemu::exit(qemu::ExitCode::Failure);

    loop {
        asm::hlt()
    }
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    info!("Running {} tests", tests.len());
    for test in tests {
        test();
    }

    qemu::exit(qemu::ExitCode::Success);
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn trivial_test() {
        log::info!("Running trivial test");
        assert_eq!(1 + 1, 2);
    }
}
