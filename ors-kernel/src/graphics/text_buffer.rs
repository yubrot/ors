use super::{Color, FontStyle, FrameBuffer, FrameBufferExt, MonospaceFont, VecBuffer};
use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug)]
pub struct MonospaceTextBuffer<'a, T> {
    lines: VecDeque<Line>,
    buf: T,
    render_diff: RenderDiff,
    font: MonospaceFont<'a>,
    cursor: (usize, usize),
}

impl<'a, T: FrameBuffer> MonospaceTextBuffer<'a, T> {
    pub fn new(buf: T, font: MonospaceFont<'a>) -> Self {
        assert_eq!(buf.format(), font.format());
        let height = buf.height() / font.unit_height() as usize;
        let lines = vec![Line::new(&buf, &font); height].into();
        Self {
            lines,
            buf,
            render_diff: None,
            font,
            cursor: (0, 0),
        }
    }

    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        let (x, y) = self.cursor;
        let y = (y as i32 + dy).clamp(0, self.lines.len() as i32 - 1) as usize;
        let x = (x as i32 + dx).clamp(0, self.lines[y].chars.len() as i32 - 1) as usize;
        self.cursor = (x, y);
    }

    pub fn set_cursor(&mut self, x: Option<u32>, y: Option<u32>) {
        let y = y
            .map(|n| n as usize)
            .unwrap_or(self.cursor.1)
            .clamp(0, self.lines.len() - 1);
        let x = x
            .map(|n| n as usize)
            .unwrap_or(self.cursor.0)
            .clamp(0, self.lines[y].chars.len() - 1);
        self.cursor = (x, y);
    }

    pub fn erase(
        &mut self,
        bg: Color,
        before_cursor_lines: bool,
        before_cursor_chars: bool,
        after_cursor_chars: bool,
        after_cursor_lines: bool,
    ) {
        let (x, y) = self.cursor;
        let mut start = usize::MAX;
        let mut end = 0;
        if before_cursor_lines {
            for (i, l) in self.lines.iter_mut().enumerate().take(y) {
                if l.erase(bg, 0, usize::MAX) {
                    start = start.min(i);
                    end = end.max(i + 1);
                }
            }
        }
        {
            let a = if before_cursor_chars { 0 } else { x };
            let b = if after_cursor_chars { usize::MAX } else { x };
            if self.lines[y].erase(bg, a, b) {
                start = start.min(y);
                end = end.max(y + 1);
            }
        }
        if after_cursor_lines {
            for (i, l) in self.lines.iter_mut().enumerate().skip(y + 1) {
                if l.erase(bg, 0, usize::MAX) {
                    start = start.min(i);
                    end = end.max(i + 1);
                }
            }
        }
        if start < end {
            extend_render_diff(&mut self.render_diff, start, end);
        }
    }

    pub fn next_line(&mut self, bg: Color) {
        let (_, y) = self.cursor;
        if y + 1 >= self.lines.len() {
            let mut first_line = self.lines.pop_front().unwrap(); // remove the first line
            first_line.erase(bg, 0, usize::MAX);
            self.lines.push_back(first_line);
            self.render_diff = Some((0, self.lines.len())); // all lines
            self.cursor = (0, self.lines.len() - 1);
        } else {
            self.cursor = (0, y + 1);
        }
    }

    pub fn put(&mut self, c: char, fg: Color, bg: Color, style: FontStyle) {
        let (x, y) = self.cursor;
        match self.lines[y].put(c, fg, bg, style, x) {
            LinePutResult::LineFeed => self.next_line(bg),
            LinePutResult::Wrapping => {
                self.next_line(bg);
                self.put(c, fg, bg, style);
            }
            LinePutResult::Next(changed, x) => {
                self.cursor = (x, y);
                if changed {
                    extend_render_diff(&mut self.render_diff, y, y + 1);
                }
            }
        }
    }

    pub fn render(&mut self) {
        if let Some((a, b)) = self.render_diff {
            let pad_y =
                (self.buf.height() - self.lines.len() * self.font.unit_height() as usize) as i32;
            for (i, line) in self.lines.iter_mut().enumerate().skip(a).take(b - a) {
                line.render(&mut self.font);
                let pad_x =
                    (self.buf.width() - line.chars.len() * self.font.unit_width() as usize) as i32;
                let ofs_y = (i * self.font.unit_height() as usize) as i32;
                self.buf.blit(pad_x / 2, pad_y / 2 + ofs_y, &line.buf);
            }
            self.render_diff = None;
        }
    }
}

#[derive(Debug, Clone)]
struct Line {
    chars: Vec<Char>,
    buf: VecBuffer,
    render_diff: RenderDiff,
}

impl Line {
    fn new(parent_buf: &impl FrameBuffer, font: &MonospaceFont) -> Self {
        let width = parent_buf.width() / font.unit_width() as usize;
        Self {
            chars: vec![Char::void(); width],
            buf: VecBuffer::new(
                width * font.unit_width() as usize,
                font.unit_height() as usize,
                parent_buf.format(),
            ),
            render_diff: None,
        }
    }

    fn erase(&mut self, bg: Color, a: usize, b: usize /* inclusive */) -> bool {
        let b = b.saturating_add(1);
        let mut start = usize::MAX;
        let mut end = 0;
        for (i, c) in self.chars.iter_mut().enumerate().take(b).skip(a) {
            if c.erase(bg) {
                start = start.min(i);
                end = end.max(i + 1);
            }
        }
        if start < end {
            extend_render_diff(&mut self.render_diff, start, end);
            true
        } else {
            false
        }
    }

    fn put(&mut self, c: char, fg: Color, bg: Color, style: FontStyle, i: usize) -> LinePutResult {
        if c == '\n' {
            LinePutResult::LineFeed
        } else if i >= self.chars.len() {
            LinePutResult::Wrapping
        } else if self.chars[i].update(c, fg, bg, style) {
            extend_render_diff(&mut self.render_diff, i, i + 1);
            LinePutResult::Next(true, i + 1)
        } else {
            LinePutResult::Next(false, i + 1)
        }
    }

    fn render(&mut self, font: &mut MonospaceFont) {
        if let Some((a, b)) = self.render_diff {
            for (i, c) in self.chars.iter().copied().enumerate().take(b).skip(a) {
                let ofs_x = (i * font.unit_width() as usize) as i32;
                c.render_to(&mut self.buf, ofs_x, 0, font);
            }
            self.render_diff = None;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LinePutResult {
    LineFeed,
    Wrapping,
    Next(bool, usize),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
struct Char {
    // Since MonospaceFont caches the rendered glyphs, Char does not hold a VecBuffer.
    value: char,
    fg: Color,
    bg: Color,
    font_style: FontStyle,
}

impl Char {
    const fn new(value: char, fg: Color, bg: Color, font_style: FontStyle) -> Self {
        Self {
            value,
            fg,
            bg,
            font_style,
        }
    }

    const fn void() -> Self {
        Self::new(
            '\0',
            Color::new(255, 255, 255),
            Color::new(0, 0, 0),
            FontStyle::Normal,
        )
    }

    fn erase(&mut self, bg: Color) -> bool {
        self.update(' ', self.fg, bg, self.font_style)
    }

    fn update(&mut self, c: char, fg: Color, bg: Color, style: FontStyle) -> bool {
        let new_self = Self::new(c, fg, bg, style);
        if *self != new_self {
            *self = new_self;
            true
        } else {
            false
        }
    }

    fn render_to(&self, buf: &mut impl FrameBuffer, x: i32, y: i32, font: &mut MonospaceFont) {
        buf.blit(
            x,
            y,
            font.get(self.value, self.fg, self.bg, self.font_style),
        );
    }
}

type RenderDiff = Option<(usize, usize)>;

fn extend_render_diff(a: &mut RenderDiff, start: usize, end: usize) {
    *a = match *a {
        None => Some((start, end)),
        Some((a, b)) => Some((a.min(start), b.max(end))),
    };
}

// Workaround for linker error

#[no_mangle]
#[doc(hidden)]
pub extern "C" fn fminf(x: f32, y: f32) -> f32 {
    libm::fminf(x, y)
}

#[no_mangle]
#[doc(hidden)]
pub extern "C" fn fmaxf(x: f32, y: f32) -> f32 {
    libm::fmaxf(x, y)
}
