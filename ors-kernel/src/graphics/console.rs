use super::{font, Color, FrameBuffer};
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

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    pub fn put(&mut self, c: char) -> UpdateRequirement {
        let (x, y) = self.cursor;

        if c == '\n' {
            if (y + 1) % C == self.init {
                self.buf[self.init] = [' '; R];
                self.cursor = (0, self.init);
                self.init = (self.init + 1) % C;
                UpdateRequirement::Everything
            } else {
                self.cursor = (0, (y + 1) % C);
                UpdateRequirement::None
            }
        } else if x < R {
            self.buf[y][x] = c;
            self.cursor = (x + 1, y);
            UpdateRequirement::At(x, (y + C - self.init) % C)
        } else {
            let a = self.put('\n'); // wrapping line feed
            let b = self.put(c);
            a.merge(b)
        }
    }

    pub fn char_at(&self, x: usize, y: usize) -> char {
        self.buf[(self.init + y) % C][x]
    }

    pub fn writer<'a, B: FrameBuffer + ?Sized>(
        &'a mut self,
        fb: &'a mut B,
        options: ConsoleWriteOptions,
    ) -> ConsoleWriter<'a, B, R, C> {
        ConsoleWriter::new(self, fb, options)
    }
}

pub enum UpdateRequirement {
    None,
    Everything,
    At(usize, usize),
}

impl UpdateRequirement {
    pub fn merge(self, other: Self) -> Self {
        use UpdateRequirement::*;
        match (self, other) {
            (Everything, _) | (_, Everything) => Everything,
            (x @ At(_, _), None) | (None, x @ At(_, _)) => x,
            _ => None,
        }
    }
}

#[derive(new)]
pub struct ConsoleWriter<'a, B: ?Sized, const R: usize, const C: usize> {
    console: &'a mut Console<R, C>,
    fb: &'a mut B,
    options: ConsoleWriteOptions,
}

impl<'a, B: FrameBuffer + ?Sized, const R: usize, const C: usize> ConsoleWriter<'a, B, R, C> {
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.fb.fill_rect(
            self.options.x,
            self.options.y,
            (R * font::WIDTH) as i32,
            (C * font::HEIGHT) as i32,
            self.options.bg,
        );
        self.console.clear();
    }

    pub fn write_char_at(&mut self, x: usize, y: usize) {
        self.fb.fill_rect(
            self.options.x + (x * font::WIDTH) as i32,
            self.options.y + (y * font::HEIGHT) as i32,
            font::WIDTH as i32,
            font::HEIGHT as i32,
            self.options.bg,
        );
        self.fb.write_char(
            self.options.x + (x * font::WIDTH) as i32,
            self.options.y + (y * font::HEIGHT) as i32,
            self.console.char_at(x, y),
            self.options.fg,
        );
    }
}

impl<'a, B: FrameBuffer + ?Sized, const R: usize, const C: usize> fmt::Write
    for ConsoleWriter<'a, B, R, C>
{
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            match self.console.put(c) {
                UpdateRequirement::None => {}
                UpdateRequirement::Everything => {
                    for y in 0..C {
                        for x in 0..R {
                            self.write_char_at(x, y);
                        }
                    }
                }
                UpdateRequirement::At(x, y) => self.write_char_at(x, y),
            }
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, new)]
pub struct ConsoleWriteOptions {
    x: i32,
    y: i32,
    fg: Color,
    bg: Color,
}
