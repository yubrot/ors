#![no_std]
#![no_main]
#![feature(asm)]

#[macro_use]
mod graphics;
mod memory_manager;
mod page_table;
mod segments;

use core::fmt::Write;
use graphics::{BgrFrameBuffer, Buffer, Color, Console, RgbFrameBuffer};
use memory_manager::BitmapMemoryManager;
use ors_common::frame_buffer::{FrameBuffer, PixelFormat};
use ors_common::hlt;
use ors_common::memory_map::MemoryMap;

static mut MEMORY_MANAGER: BitmapMemoryManager = BitmapMemoryManager::new();

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &FrameBuffer, mm: &MemoryMap) {
    unsafe {
        segments::initialize();
        page_table::initialize();
        MEMORY_MANAGER.initialize(mm);
    };

    match fb.format {
        PixelFormat::Rgb => render_example(&RgbFrameBuffer(fb)),
        PixelFormat::Bgr => render_example(&BgrFrameBuffer(fb)),
    }

    loop {
        hlt!()
    }
}

fn render_example(b: &impl Buffer) {
    b.fill_rect(0, 0, b.width(), b.height(), Color::BLACK);
    let mut console = Console::<80, 25>::new();
    let mut c = console.on(b, 10, 10, Color::WHITE, Color::BLACK);
    writeln!(c, "Hello, World!").unwrap();
    writeln!(c, "1 + 2 = {}", 1 + 2).unwrap();
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hlt!()
    }
}
