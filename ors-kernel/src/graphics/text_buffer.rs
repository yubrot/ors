use super::{font, Color, FrameBuffer, FrameBufferExt, FrameBufferFormat, VecBuffer};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec;
use alloc::vec::Vec;
use core::mem;

#[derive(Debug)]
pub struct TextBuffer<T> {
    lines: VecDeque<Line>,
    buf: T,
    render_range: RenderRange,
    ctx: Context,
    cursor: (usize, usize),
}

impl<T: FrameBuffer> TextBuffer<T> {
    pub fn new(buf: T, fg: Color, bg: Color) -> Self {
        let format = buf.format();
        let width = buf.width() / font::WIDTH as usize;
        let height = buf.height() / font::HEIGHT as usize;
        Self {
            buf,
            lines: vec![Line::new(width, format); height].into(),
            render_range: Some((0, height)),
            ctx: Context::new(fg, bg, format),
            cursor: (0, 0),
        }
    }

    pub fn clear(&mut self) {
        self.cursor = (0, 0);
        let mut start = usize::MAX;
        let mut end = 0;
        for (i, l) in self.lines.iter_mut().enumerate() {
            if l.clear() {
                start = start.min(i);
                end = end.max(i + 1);
            }
        }
        if start < end {
            extend_render_range(&mut self.render_range, start, end);
        }
    }

    pub fn next_line(&mut self) {
        let (_, y) = self.cursor;
        if y + 1 >= self.lines.len() {
            let mut first_line = self.lines.pop_front().unwrap(); // remove the first line
            first_line.clear();
            self.lines.push_back(first_line);
            self.render_range = Some((0, self.lines.len())); // all lines
            self.cursor = (0, self.lines.len() - 1);
        } else {
            self.cursor = (0, y + 1);
        }
    }

    pub fn put(&mut self, c: char) {
        let (x, y) = self.cursor;
        match self.lines[y].put(c, x) {
            LinePutResult::LineFeed => self.next_line(),
            LinePutResult::Wrapping(c) => {
                self.next_line();
                self.put(c);
            }
            LinePutResult::Next(changed, x) => {
                self.cursor = (x, y);
                if changed {
                    extend_render_range(&mut self.render_range, y, y + 1);
                }
            }
        }
    }

    pub fn render(&mut self) {
        if let Some((a, b)) = self.render_range {
            let oy = (self.buf.height() as i32 - self.lines.len() as i32 * font::HEIGHT as i32) / 2;
            for (i, l) in self.lines.iter_mut().enumerate().skip(a).take(b - a) {
                l.render(&mut self.ctx);
                let x = (self.buf.width() as i32 - l.chars.len() as i32 * font::WIDTH as i32) / 2;
                let y = oy + i as i32 * font::HEIGHT as i32;
                self.buf.blit(x, y, &l.buf);
            }
            self.render_range = None;
        }
    }
}

#[derive(Debug, Clone)]
struct Line {
    chars: Vec<char>,
    buf: VecBuffer,
    render_range: RenderRange,
}

impl Line {
    fn new(width: usize, format: FrameBufferFormat) -> Self {
        Self {
            chars: vec![' '; width],
            buf: VecBuffer::new(width * font::WIDTH as usize, font::HEIGHT as usize, format),
            render_range: None,
        }
    }

    fn clear(&mut self) -> bool {
        let mut start = usize::MAX;
        let mut end = 0;
        for (i, c) in self.chars.iter_mut().enumerate() {
            if mem::replace(c, ' ') != ' ' {
                start = start.min(i);
                end = end.max(i + 1);
            }
        }
        if start < end {
            extend_render_range(&mut self.render_range, start, end);
            true
        } else {
            false
        }
    }

    fn put(&mut self, c: char, cursor: usize) -> LinePutResult {
        if c == '\n' {
            LinePutResult::LineFeed
        } else if cursor >= self.chars.len() {
            LinePutResult::Wrapping(c)
        } else if mem::replace(&mut self.chars[cursor], c) != c {
            extend_render_range(&mut self.render_range, cursor, cursor + 1);
            LinePutResult::Next(true, cursor + 1)
        } else {
            LinePutResult::Next(false, cursor + 1)
        }
    }

    fn render(&mut self, ctx: &mut Context) {
        if let Some((a, b)) = self.render_range {
            for (i, c) in self.chars.iter().copied().enumerate().skip(a).take(b - a) {
                let char_buf = ctx.char_buf(c);
                let x = i as i32 * font::WIDTH as i32;
                self.buf.blit(x, 0, char_buf);
            }
            self.render_range = None;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LinePutResult {
    LineFeed,
    Wrapping(char),
    Next(bool, usize),
}

#[derive(Debug, Clone)]
struct Context {
    fg: Color,
    bg: Color,
    format: FrameBufferFormat,
    char_cache: BTreeMap<char, VecBuffer>,
}

impl Context {
    fn new(fg: Color, bg: Color, format: FrameBufferFormat) -> Self {
        Self {
            fg,
            bg,
            format,
            char_cache: BTreeMap::new(),
        }
    }

    fn char_buf(&mut self, c: char) -> &VecBuffer {
        let Self { fg, bg, format, .. } = *self;
        self.char_cache.entry(c).or_insert_with_key(|c| {
            let mut buf = VecBuffer::new(font::WIDTH as usize, font::HEIGHT as usize, format);
            buf.write_char(0, 0, *c, fg, bg);
            buf
        })
    }
}

type RenderRange = Option<(usize, usize)>;

fn extend_render_range(a: &mut RenderRange, start: usize, end: usize) {
    *a = match *a {
        None => Some((start, end)),
        Some((a, b)) => Some((a.min(start), b.max(end))),
    };
}
