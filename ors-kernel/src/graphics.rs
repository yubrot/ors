mod color;
mod console;
mod font;

pub use color::Color;
pub use console::Console;

pub trait Buffer {
    fn width(&self) -> i32;
    fn height(&self) -> i32;
    fn write_pixel(&self, x: i32, y: i32, color: Color);

    fn write_char(&self, x: i32, y: i32, c: char, color: Color) {
        font::write_ascii(self, x, y, c, color);
    }

    fn write_string(&self, x: i32, y: i32, s: &str, color: Color) {
        for (i, c) in s.chars().enumerate() {
            self.write_char(x + (font::WIDTH * i) as i32, y, c, color);
        }
    }

    fn fill_rect(&self, x: i32, y: i32, w: i32, h: i32, color: Color) {
        for dx in 0..w {
            for dy in 0..h {
                self.write_pixel(x + dx, y + dy, color);
            }
        }
    }

    fn clear(&self, color: Color) {
        self.fill_rect(0, 0, self.width(), self.height(), color);
    }
}

impl Buffer for () {
    fn width(&self) -> i32 {
        0
    }

    fn height(&self) -> i32 {
        0
    }

    fn write_pixel(&self, _x: i32, _y: i32, _color: Color) {}
}

pub struct RgbFrameBuffer(pub ors_common::frame_buffer::FrameBuffer);

impl Buffer for RgbFrameBuffer {
    fn width(&self) -> i32 {
        self.0.resolution.0 as i32
    }

    fn height(&self) -> i32 {
        self.0.resolution.1 as i32
    }

    fn write_pixel(&self, x: i32, y: i32, color: Color) {
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

pub struct BgrFrameBuffer(pub ors_common::frame_buffer::FrameBuffer);

impl Buffer for BgrFrameBuffer {
    fn width(&self) -> i32 {
        self.0.resolution.0 as i32
    }

    fn height(&self) -> i32 {
        self.0.resolution.1 as i32
    }

    fn write_pixel(&self, x: i32, y: i32, color: Color) {
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
