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
mod pci;
mod segments;

use core::{mem, ptr};
use graphics::{BgrFrameBuffer, Buffer, Color, RgbFrameBuffer};
use log::info;
use ors_common::asm;
use ors_common::frame_buffer::{FrameBuffer, PixelFormat};
use ors_common::memory_map::MemoryMap;

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &FrameBuffer, mm: &MemoryMap) {
    unsafe {
        segments::initialize();
        page_table::initialize();
        global::MEMORY_MANAGER.initialize(mm);
        global::BUFFER = prepare_buffer(*fb);
        global::BUFFER.clear(Color::BLACK);
    };

    logger::initialize();

    info!("Hello, World!");
    info!("1 + 2 = {}", 1 + 2);

    for d in pci::Device::scan::<32>().unwrap() {
        info!(
            "{}.{}.{}: vend {:04x}, class {:02x}.{:02x}.{:02x}, head {:02x}",
            d.bus,
            d.device,
            d.function,
            d.vendor_id(),
            d.class_code().base,
            d.class_code().sub,
            d.class_code().interface,
            d.header_type()
        );
    }

    loop {
        asm::hlt()
    }
}

unsafe fn prepare_buffer(fb: FrameBuffer) -> &'static dyn Buffer {
    static_assertions::assert_eq_size!(RgbFrameBuffer, BgrFrameBuffer);
    const PAYLOAD_SIZE: usize = mem::size_of::<RgbFrameBuffer>();
    static mut PAYLOAD: [u8; PAYLOAD_SIZE] = [0; PAYLOAD_SIZE];
    match fb.format {
        PixelFormat::Rgb => {
            let p = &mut PAYLOAD[0] as *mut u8 as *mut RgbFrameBuffer;
            ptr::write(p, RgbFrameBuffer(fb));
            &mut *p
        }
        PixelFormat::Bgr => {
            let p = &mut PAYLOAD[0] as *mut u8 as *mut BgrFrameBuffer;
            ptr::write(p, BgrFrameBuffer(fb));
            &mut *p
        }
    }
}
