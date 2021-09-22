use super::{Color, FrameBuffer, FrameBufferExt, Rect};

pub const WIDTH: u32 = 8;
pub const HEIGHT: u32 = 16;

static ASCII_FONT: &[u8; 4096] = include_bytes!("ascii.bin");

pub fn write_ascii(
    fb: &mut (impl FrameBuffer + ?Sized),
    x: i32,
    y: i32,
    mut c: char,
    fg: Color,
    bg: Color,
) {
    if let Some(rect) = fb.rect().intersect(Rect::new(x, y, WIDTH, HEIGHT)) {
        if !c.is_ascii() {
            c = '?';
        }
        let offset = HEIGHT as usize * c as u8 as usize;
        let font = &ASCII_FONT[offset..offset + HEIGHT as usize];
        let oy = (rect.y - y) as usize;
        let ox = (rect.x - x) as usize;
        let fg = fb.format().encoder()(fg);
        let bg = fb.format().encoder()(bg);
        let stride = fb.stride();
        let dest = fb.bytes_mut();

        for dy in 0..rect.h as usize {
            let y = rect.y as usize + dy;
            for dx in 0..rect.w as usize {
                let x = rect.x as usize + dx;
                let color = if (font[dy + oy] << (dx + ox) & 0x80) != 0 {
                    fg
                } else {
                    bg
                };
                let i = (y * stride + x) * 4;
                dest[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}
