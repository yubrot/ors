#![no_std]
#![no_main]
#![feature(asm)]

mod memory_manager;
mod page_table;
mod segments;

use core::slice;
use memory_manager::{BitmapMemoryManager, FrameId};
use ors_common::frame_buffer::{FrameBuffer, PixelFormat};
use ors_common::hlt;
use ors_common::memory_map::MemoryMap;

static mut MEMORY_MANAGER: BitmapMemoryManager = BitmapMemoryManager::new();

#[no_mangle]
pub extern "sysv64" fn kernel_main2(fb: &FrameBuffer, mm: &MemoryMap) {
    segments::initialize();
    page_table::initialize();

    unsafe {
        let mut phys_available_end = 0;
        for d in slice::from_raw_parts(mm.descriptors, mm.descriptors_len as usize) {
            let phys_start = d.phys_start as usize;
            let phys_end = d.phys_end as usize;
            if phys_available_end < d.phys_start as usize {
                MEMORY_MANAGER.mark_allocated_in_bytes(
                    FrameId::from_physical_address(phys_available_end),
                    phys_start - phys_available_end,
                );
            }
            phys_available_end = phys_end;
        }
        MEMORY_MANAGER.set_memory_range(
            FrameId::MIN,
            FrameId::from_physical_address(phys_available_end),
        );
    }

    match fb.format {
        PixelFormat::Rgb => render_example::<RgbPixelWriter>(fb),
        PixelFormat::Bgr => render_example::<BgrPixelWriter>(fb),
    }

    loop {
        hlt!()
    }
}

trait PixelWriter {
    fn put_pixel(fb: &FrameBuffer, x: u32, y: u32, color: (u8, u8, u8));
}

enum RgbPixelWriter {}

impl PixelWriter for RgbPixelWriter {
    fn put_pixel(fb: &FrameBuffer, x: u32, y: u32, color: (u8, u8, u8)) {
        unsafe {
            let offset = (4 * (fb.stride * y + x)) as usize;
            *fb.frame_buffer.add(offset) = color.0;
            *fb.frame_buffer.add(offset + 1) = color.1;
            *fb.frame_buffer.add(offset + 2) = color.2;
        }
    }
}

enum BgrPixelWriter {}

impl PixelWriter for BgrPixelWriter {
    fn put_pixel(fb: &FrameBuffer, x: u32, y: u32, color: (u8, u8, u8)) {
        unsafe {
            let offset = (4 * (fb.stride * y + x)) as usize;
            *fb.frame_buffer.add(offset) = color.2;
            *fb.frame_buffer.add(offset + 1) = color.1;
            *fb.frame_buffer.add(offset + 2) = color.0;
        }
    }
}

fn render_example<W: PixelWriter>(fb: &FrameBuffer) {
    for x in 0..fb.resolution.0 {
        for y in 0..fb.resolution.1 {
            W::put_pixel(fb, x, y, (255, 255, 255));
        }
    }

    for x in 50..250 {
        for y in 50..150 {
            W::put_pixel(fb, x, y, (50, 155, 255));
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        hlt!()
    }
}
