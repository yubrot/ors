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
pub mod console;
pub mod context;
pub mod cpu;
pub mod devices;
pub mod fs;
pub mod graphics;
pub mod interrupts;
pub mod logger;
pub mod paging;
pub mod phys_memory;
pub mod segmentation;
mod shell;
pub mod sync;
pub mod task;
pub mod x64;

use ors_common::frame_buffer::FrameBuffer as RawFrameBuffer;
use ors_common::memory_map::MemoryMap;

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
    task::initialize_scheduler();
    devices::pci::initialize_devices();
    devices::virtio::block::initialize();
    devices::serial::default_port().init();
    console::initialize((*fb).into());
    task::scheduler().add(task::Priority::L1, shell::run, 0);
    drop(cli);

    #[cfg(test)]
    test_main();

    loop {
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
    use log::info;

    info!("RUNNING {} tests", tests.len());
    for test in tests {
        test();
    }

    devices::qemu::exit(devices::qemu::ExitCode::Success);
}
