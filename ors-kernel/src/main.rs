#![no_std]
#![no_main]

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
use graphics::{BgrFrameBuffer, Color, FrameBuffer, RgbFrameBuffer};
use log::info;
use ors_common::frame_buffer::{FrameBuffer as RawFrameBuffer, PixelFormat as RawPixelFormat};
use ors_common::memory_map::MemoryMap;
use x86_64::instructions as asm;

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &RawFrameBuffer, mm: &MemoryMap) {
    unsafe { segments::initialize() };
    unsafe { page_table::initialize() };
    global::memory_manager().initialize(mm);
    global::initialize_frame_buffer(unsafe { prepare_frame_buffer(*fb) });
    global::initialize_devices(pci::Device::scan::<32>().unwrap());
    logger::initialize();
    global::frame_buffer().clear(Color::BLACK);

    info!("Hello, World!");
    info!("1 + 2 = {}", 1 + 2);

    loop {
        asm::hlt()
    }
}

unsafe fn prepare_frame_buffer(fb: RawFrameBuffer) -> &'static mut (dyn FrameBuffer + Send + Sync) {
    static_assertions::assert_eq_size!(RgbFrameBuffer, BgrFrameBuffer);
    const PAYLOAD_SIZE: usize = mem::size_of::<RgbFrameBuffer>();
    static mut PAYLOAD: [u8; PAYLOAD_SIZE] = [0; PAYLOAD_SIZE];
    match fb.format {
        RawPixelFormat::Rgb => {
            let p = &mut PAYLOAD[0] as *mut u8 as *mut RgbFrameBuffer;
            ptr::write(p, RgbFrameBuffer(fb));
            &mut *p
        }
        RawPixelFormat::Bgr => {
            let p = &mut PAYLOAD[0] as *mut u8 as *mut BgrFrameBuffer;
            ptr::write(p, BgrFrameBuffer(fb));
            &mut *p
        }
    }
}
