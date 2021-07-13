#![no_std]
#![no_main]
#![feature(asm)]

use ors_common::frame_buffer::{FrameBuffer, PixelFormat};
use ors_common::hlt;

#[no_mangle]
pub extern "sysv64" fn kernel_main(fb: &FrameBuffer) {
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
