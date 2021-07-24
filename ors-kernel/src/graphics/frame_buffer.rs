use super::{font, Color};
use core::{mem, ptr};
use ors_common::frame_buffer::{FrameBuffer as RawFrameBuffer, PixelFormat as RawPixelFormat};

const FRAME_BUFFER_PAYLOAD_SIZE: usize = mem::size_of::<RgbFrameBuffer>();
static_assertions::assert_eq_size!(RgbFrameBuffer, BgrFrameBuffer);

pub struct FrameBufferPayload([u8; FRAME_BUFFER_PAYLOAD_SIZE]);

impl FrameBufferPayload {
    pub const fn new() -> Self {
        Self([0; FRAME_BUFFER_PAYLOAD_SIZE])
    }
}

pub unsafe fn prepare_frame_buffer(
    fb: RawFrameBuffer,
    payload: &mut FrameBufferPayload,
) -> &mut (dyn FrameBuffer + Send + Sync) {
    match fb.format {
        RawPixelFormat::Rgb => {
            let p = &mut payload.0[0] as *mut u8 as *mut RgbFrameBuffer;
            ptr::write(p, RgbFrameBuffer(fb));
            &mut *p
        }
        RawPixelFormat::Bgr => {
            let p = &mut payload.0[0] as *mut u8 as *mut BgrFrameBuffer;
            ptr::write(p, BgrFrameBuffer(fb));
            &mut *p
        }
    }
}

pub trait FrameBuffer {
    fn width(&self) -> i32;
    fn height(&self) -> i32;
    fn write_pixel(&mut self, x: i32, y: i32, color: Color);

    fn write_char(&mut self, x: i32, y: i32, c: char, color: Color) {
        font::write_ascii(self, x, y, c, color);
    }

    fn write_string(&mut self, x: i32, y: i32, s: &str, color: Color) {
        for (i, c) in s.chars().enumerate() {
            self.write_char(x + (font::WIDTH * i) as i32, y, c, color);
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        for dx in 0..w {
            for dy in 0..h {
                self.write_pixel(x + dx, y + dy, color);
            }
        }
    }

    fn clear(&mut self, color: Color) {
        self.fill_rect(0, 0, self.width(), self.height(), color);
    }
}

impl FrameBuffer for () {
    fn width(&self) -> i32 {
        0
    }

    fn height(&self) -> i32 {
        0
    }

    fn write_pixel(&mut self, _x: i32, _y: i32, _color: Color) {}
}

struct RgbFrameBuffer(pub ors_common::frame_buffer::FrameBuffer);

unsafe impl Send for RgbFrameBuffer {}

unsafe impl Sync for RgbFrameBuffer {}

impl FrameBuffer for RgbFrameBuffer {
    fn width(&self) -> i32 {
        self.0.resolution.0 as i32
    }

    fn height(&self) -> i32 {
        self.0.resolution.1 as i32
    }

    fn write_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || self.width() <= x || y < 0 || self.height() <= y {
            return;
        }
        unsafe {
            let offset = (4 * (self.0.stride * y as u32 + x as u32)) as usize;
            *self.0.frame_buffer.add(offset) = color.r;
            *self.0.frame_buffer.add(offset + 1) = color.g;
            *self.0.frame_buffer.add(offset + 2) = color.b;
        }
    }
}

struct BgrFrameBuffer(pub ors_common::frame_buffer::FrameBuffer);

unsafe impl Send for BgrFrameBuffer {}

unsafe impl Sync for BgrFrameBuffer {}

impl FrameBuffer for BgrFrameBuffer {
    fn width(&self) -> i32 {
        self.0.resolution.0 as i32
    }

    fn height(&self) -> i32 {
        self.0.resolution.1 as i32
    }

    fn write_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || self.width() <= x || y < 0 || self.height() <= y {
            return;
        }
        unsafe {
            let offset = (4 * (self.0.stride * y as u32 + x as u32)) as usize;
            *self.0.frame_buffer.add(offset) = color.b;
            *self.0.frame_buffer.add(offset + 1) = color.g;
            *self.0.frame_buffer.add(offset + 2) = color.r;
        }
    }
}
