use super::{font, Buffer, Color};
use core::fmt;
use derive_new::new;

pub struct Console<const R: usize, const C: usize> {
    init: usize,
    cursor: (usize, usize),
    buf: [[char; R]; C],
}

impl<const R: usize, const C: usize> Console<R, C> {
    pub const fn new() -> Self {
        Self {
            init: 0,
            cursor: (0, 0),
            buf: [[' '; R]; C],
        }
    }

    pub fn clear(&mut self) {
        *self = Self::new();
    }

    pub fn put(&mut self, c: char) -> RenderRequirement {
        let (x, y) = self.cursor;

        if c == '\n' {
            if (y + 1) % C == self.init {
                self.buf[self.init] = [' '; R];
                self.cursor = (0, self.init);
                self.init = (self.init + 1) % C;
                RenderRequirement::Everything
            } else {
                self.cursor = (0, (y + 1) % C);
                RenderRequirement::None
            }
        } else if x < R {
            self.buf[y][x] = c;
            self.cursor = (x + 1, y);
            RenderRequirement::At(x, (y + C - self.init) % C)
        } else {
            let a = self.put('\n'); // wrapping line feed
            let b = self.put(c);
            a.merge(b)
        }
    }

    pub fn char_at(&self, x: usize, y: usize) -> char {
        self.buf[(self.init + y) % C][x]
    }

    pub fn on<'a, B: Buffer + ?Sized>(
        &'a mut self,
        b: &'a B,
        x: i32,
        y: i32,
        fg: Color,
        bg: Color,
    ) -> ConsoleWriter<'a, B, R, C> {
        ConsoleWriter::new(b, self, x, y, fg, bg)
    }
}

pub enum RenderRequirement {
    None,
    Everything,
    At(usize, usize),
}

impl RenderRequirement {
    pub fn merge(self, other: Self) -> Self {
        use RenderRequirement::*;
        match (self, other) {
            (Everything, _) | (_, Everything) => Everything,
            (x @ At(_, _), None) | (None, x @ At(_, _)) => x,
            _ => None,
        }
    }
}

#[derive(new)]
pub struct ConsoleWriter<'a, B: ?Sized, const R: usize, const C: usize> {
    buffer: &'a B,
    console: &'a mut Console<R, C>,
    x: i32,
    y: i32,
    fg: Color,
    bg: Color,
}

impl<'a, B: Buffer + ?Sized, const R: usize, const C: usize> ConsoleWriter<'a, B, R, C> {
    pub fn clear(&mut self) {
        self.buffer.fill_rect(
            self.x,
            self.y,
            (R * font::WIDTH) as i32,
            (C * font::HEIGHT) as i32,
            self.bg,
        );
        self.console.clear();
    }

    pub fn write_char_at(&mut self, x: usize, y: usize) {
        self.buffer.fill_rect(
            self.x + (x * font::WIDTH) as i32,
            self.y + (y * font::HEIGHT) as i32,
            font::WIDTH as i32,
            font::HEIGHT as i32,
            self.bg,
        );
        self.buffer.write_char(
            self.x + (x * font::WIDTH) as i32,
            self.y + (y * font::HEIGHT) as i32,
            self.console.char_at(x, y),
            self.fg,
        );
    }
}

impl<'a, B: Buffer + ?Sized, const R: usize, const C: usize> fmt::Write
    for ConsoleWriter<'a, B, R, C>
{
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            match self.console.put(c) {
                RenderRequirement::None => {}
                RenderRequirement::Everything => {
                    for y in 0..C {
                        for x in 0..R {
                            self.write_char_at(x, y);
                        }
                    }
                }
                RenderRequirement::At(x, y) => self.write_char_at(x, y),
            }
        }

        Ok(())
    }
}
