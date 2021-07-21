use super::{Buffer, Color};

pub const WIDTH: usize = 8;
pub const HEIGHT: usize = 16;

static ASCII_FONT: &[u8; 4096] = include_bytes!("ascii.bin");

pub fn write_ascii<B: Buffer + ?Sized>(b: &B, x: i32, y: i32, mut c: char, color: Color) {
    if !c.is_ascii() {
        c = '?';
    }

    let offset = HEIGHT * c as u8 as usize;
    let font = &ASCII_FONT[offset..offset + HEIGHT];
    for dy in 0..HEIGHT {
        for dx in 0..WIDTH {
            if ((font[dy] << dx) & 0x80) != 0 {
                b.write_pixel(x + dx as i32, y + dy as i32, color);
            }
        }
    }
}
