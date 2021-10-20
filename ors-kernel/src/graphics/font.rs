use super::{Color, FrameBufferExt, FrameBufferFormat, VecBuffer};
use ab_glyph::{Font, FontRef, ScaleFont};
use alloc::collections::BTreeMap;

#[derive(Debug)]
pub struct MonospaceFont<'a> {
    size: u32,
    normal: FontRef<'a>, // TODO: Use <T: Font> instead of FontRef
    bold: FontRef<'a>,
    format: FrameBufferFormat,
    cache: BTreeMap<CacheKey, VecBuffer>,
}

impl<'a> MonospaceFont<'a> {
    pub fn new(size: u32, normal: &'a [u8], bold: &'a [u8], format: FrameBufferFormat) -> Self {
        Self {
            size,
            normal: FontRef::try_from_slice(normal).unwrap(),
            bold: FontRef::try_from_slice(bold).unwrap(),
            format,
            cache: BTreeMap::new(),
        }
    }

    pub fn unit_width(&self) -> u32 {
        (self.size + 1) / 2
    }

    pub fn unit_height(&self) -> u32 {
        self.size
    }

    pub fn format(&self) -> FrameBufferFormat {
        self.format
    }

    pub fn get(&mut self, ch: char, fg: Color, bg: Color, style: FontStyle) -> &VecBuffer {
        let key = CacheKey { ch, fg, bg, style };
        let Self { size, format, .. } = *self;
        let unit_width = self.unit_width();
        let unit_height = self.unit_height();
        let font = match style {
            FontStyle::Normal => &self.normal,
            FontStyle::Bold => &self.bold,
        }
        .as_scaled(size as f32);
        self.cache.entry(key).or_insert_with(|| {
            let mut glyph = font.scaled_glyph(ch);
            glyph.position = ab_glyph::point(0.0, font.ascent());
            let mut buf = VecBuffer::new(unit_width as usize, unit_height as usize, format);
            buf.clear(bg);
            if let Some(q) = font.outline_glyph(glyph) {
                let min_x = q.px_bounds().min.x as i32;
                let min_y = q.px_bounds().min.y as i32;
                q.draw(|x, y, c| {
                    buf.write_pixel(min_x + x as i32, min_y + y as i32, bg.mix(fg, c));
                });
            }
            buf
        })
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
struct CacheKey {
    ch: char,
    fg: Color,
    bg: Color,
    style: FontStyle,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum FontStyle {
    Normal,
    Bold,
}
