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
pub mod graphics;
pub mod interrupts;
pub mod logger;
pub mod paging;
pub mod phys_memory;
pub mod segmentation;
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
    devices::serial::default_port().init();
    console::initialize((*fb).into());
    task::scheduler().add(task::Priority::L1, terminal, 0);
    drop(cli);

    #[cfg(test)]
    test_main();

    loop {
        x64::hlt()
    }
}

extern "C" fn terminal(_: u64) -> ! {
    // TODO: implementation

    kprint!("\x1b[H\x1b[2J");

    loop {
        let input = console::input_queue().dequeue();
        if let console::Input::Char(ch) = input {
            match ch {
                'a' => kprint!("\x1b[A"),
                'A' => kprint!("\x1b[3A"),
                'b' => kprint!("\x1b[B"),
                'B' => kprint!("\x1b[3B"),
                'c' => kprint!("\x1b[C"),
                'C' => kprint!("\x1b[3C"),
                'd' => kprint!("\x1b[D"),
                'D' => kprint!("\x1b[3D"),
                'e' => kprint!("\x1b[E"),
                'E' => kprint!("\x1b[3E"),
                'f' => kprint!("\x1b[F"),
                'F' => kprint!("\x1b[3F"),
                'g' => kprint!("\x1b[G"),
                'G' => kprint!("\x1b[3G"),
                'h' => kprint!("\x1b[H"),
                'H' => kprint!("\x1b[4;8H"),
                'j' => kprint!("\x1b[J"),
                'J' => kprint!("\x1b[1J"),
                'z' => kprint!("\x1b[2J"),
                'k' => kprint!("\x1b[K"),
                'K' => kprint!("\x1b[1K"),
                'x' => kprint!("\x1b[2K"),
                '0' => kprint!("\x1b[m"),
                '1' => kprint!("\x1b[1m"),
                '2' => kprint!("\x1b[22m"),
                '3' => kprint!("\x1b[30m"),
                '4' => kprint!("\x1b[31m"),
                '5' => kprint!("\x1b[37m"),
                '#' => kprint!("\x1b[1;30m"),
                '$' => kprint!("\x1b[1;31m"),
                '%' => kprint!("\x1b[1;37m"),
                '6' => kprint!("\x1b[40m"),
                '7' => kprint!("\x1b[44m"),
                '8' => kprint!("\x1b[47m"),
                '9' => kprint!("\x1b[49m"),
                'p' => kprint!("\x1b[38;5;254m"),
                'q' => kprint!("\x1b[48;5;254m"),
                _ => kprint!("{},", ch),
            }
        } else {
            kprintln!("{:?}", input);
        }
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
