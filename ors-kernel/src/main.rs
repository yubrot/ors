#![no_std]
#![no_main]
#![feature(asm)]

#[macro_use]
mod graphics;
mod global;
mod logger;
mod memory_manager;
mod page_table;
mod panic_handler;
mod segments;

use core::{mem, ptr};
use graphics::{BgrFrameBuffer, Buffer, Color, RgbFrameBuffer};
use log::info;
use ors_common::frame_buffer::{FrameBuffer, PixelFormat};
use ors_common::hlt;
use ors_common::memory_map::MemoryMap;

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &FrameBuffer, mm: &MemoryMap) {
    unsafe {
        segments::initialize();
        page_table::initialize();
        global::MEMORY_MANAGER.initialize(mm);
        global::BUFFER = prepare_buffer(*fb);
        global::BUFFER.fill_rect(
            0,
            0,
            global::BUFFER.width(),
            global::BUFFER.height(),
            Color::BLACK,
        );
    };

    logger::initialize();

    info!("Hello, World!");
    info!("1 + 2 = {}", 1 + 2);

    loop {
        hlt!()
    }
}

unsafe fn prepare_buffer(fb: FrameBuffer) -> &'static dyn Buffer {
    const PAYLOAD_SIZE: usize = mem::size_of::<RgbFrameBuffer>();
    static mut SCREEN_BUFFER_PAYLOAD: [u8; PAYLOAD_SIZE] = [0; PAYLOAD_SIZE];
    match fb.format {
        PixelFormat::Rgb => {
            let p = &mut SCREEN_BUFFER_PAYLOAD[0] as *mut u8 as *mut RgbFrameBuffer;
            ptr::write(p, RgbFrameBuffer(fb));
            &mut *p
        }
        PixelFormat::Bgr => {
            let p = &mut SCREEN_BUFFER_PAYLOAD[0] as *mut u8 as *mut BgrFrameBuffer;
            ptr::write(p, BgrFrameBuffer(fb));
            &mut *p
        }
    }
}
