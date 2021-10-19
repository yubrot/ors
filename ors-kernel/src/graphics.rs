mod color;
mod console;
mod font;
mod frame_buffer;
mod rect;
mod text_buffer;

pub use color::Color;
pub use console::{
    initialize_screen_console, screen_console, screen_console_if_initialized, Console,
};
pub use frame_buffer::{FrameBuffer, FrameBufferFormat, ScreenBuffer, VecBuffer};
pub use rect::Rect;
pub use text_buffer::TextBuffer;

pub trait FrameBufferExt: FrameBuffer {
    fn rect(&self) -> Rect {
        Rect::new(0, 0, self.width() as u32, self.height() as u32)
    }

    fn pixel_index(&self, x: i32, y: i32) -> Option<usize> {
        if self.rect().contains(x, y) {
            Some((y as usize * self.stride() + x as usize) * 4)
        } else {
            None
        }
    }

    fn read_pixel(&self, x: i32, y: i32) -> Option<Color> {
        let i = self.pixel_index(x, y)?;
        let format = self.format();
        let src = self.bytes();
        let color = format.decoder()([src[i], src[i + 1], src[i + 2], src[i + 3]]);
        Some(color)
    }

    fn write_pixel(&mut self, x: i32, y: i32, color: Color) -> bool {
        if let Some(i) = self.pixel_index(x, y) {
            let format = self.format();
            let dest = self.bytes_mut();
            let color = format.encoder()(color);
            dest[i..i + 4].copy_from_slice(&color);
            true
        } else {
            false
        }
    }

    fn blit(&mut self, x: i32, y: i32, fb: &impl FrameBuffer) {
        if let Some(rect) = self.rect().intersect(fb.rect().offset(x, y)) {
            let oy = (rect.y - y) as usize;
            let ox = (rect.x - x) as usize;
            let src_stride = fb.stride();
            let src = fb.bytes();
            let dest_stride = self.stride();
            let dest = self.bytes_mut();
            let l = rect.w as usize * 4;

            for dy in 0..rect.h as usize {
                let i = ((rect.y as usize + dy) * dest_stride + rect.x as usize) * 4;
                let j = ((oy + dy) * src_stride + ox) * 4;
                dest[i..i + l].copy_from_slice(&src[j..j + l]);
            }
        }
    }

    fn fill_rect(&mut self, rect: Rect, color: Color) {
        if let Some(rect) = self.rect().intersect(rect) {
            let x = rect.x as usize;
            let y = rect.y as usize;
            let w = rect.w as usize;
            let h = rect.h as usize;
            let stride = self.stride();
            let color = self.format().encoder()(color);
            let dest = self.bytes_mut();
            for oy in 0..h {
                let i = ((y + oy) * stride + x) * 4;
                if oy == 0 {
                    const CHUNK: usize = 16;
                    for ox in 0..w.min(CHUNK) {
                        dest[i + ox * 4..i + ox * 4 + 4].copy_from_slice(&color);
                    }
                    if 16 < w {
                        for ox in (1..w / CHUNK).map(|n| n * CHUNK) {
                            let (a, b) = dest.split_at_mut(i + ox * 4);
                            b[0..CHUNK * 4].copy_from_slice(&mut a[i..i + CHUNK * 4]);
                        }
                        let mw = w % CHUNK;
                        let (a, b) = dest.split_at_mut(i + (w - mw) * 4);
                        b[0..mw * 4].copy_from_slice(&mut a[i..i + mw * 4]);
                    }
                } else {
                    let (a, b) = dest.split_at_mut(i);
                    b[0..w * 4].copy_from_slice(&mut a[i - stride * 4..i - (stride - w) * 4]);
                }
            }
        }
    }

    fn clear(&mut self, color: Color) {
        self.fill_rect(self.rect(), color);
    }

    fn write_char(&mut self, x: i32, y: i32, c: char, fg: Color, bg: Color) {
        font::write_ascii(self, x, y, c, fg, bg);
    }

    fn write_string(&mut self, x: i32, y: i32, s: &str, fg: Color, bg: Color) {
        for (i, c) in s.chars().enumerate() {
            self.write_char(x + font::WIDTH as i32 * i as i32, y, c, fg, bg);
        }
    }
}

impl<T: FrameBuffer + ?Sized> FrameBufferExt for T {}
