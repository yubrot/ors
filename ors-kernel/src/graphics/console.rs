use super::{font, Color, FrameBuffer, FrameBufferExt, ScreenBuffer, VecBuffer};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec;
use core::fmt;
use log::trace;
use spin::{Mutex, MutexGuard, Once};

static SCREEN_CONSOLE: Once<Mutex<Console<ScreenBuffer>>> = Once::new();

pub fn screen_console() -> MutexGuard<'static, Console<ScreenBuffer>> {
    SCREEN_CONSOLE.wait().lock()
}

pub fn screen_console_if_available() -> Option<MutexGuard<'static, Console<ScreenBuffer>>> {
    SCREEN_CONSOLE.get()?.try_lock()
}

pub fn initialize_screen_console(sb: ScreenBuffer) {
    SCREEN_CONSOLE.call_once(move || {
        trace!("INITIALIZING screen console");
        Mutex::new(Console::new(sb, Color::WHITE, Color::BLACK))
    });
}

pub struct Console<T> {
    fb: T,
    fg: Color,
    bg: Color,
    size: (usize, usize),
    char_cache: BTreeMap<char, VecBuffer>,
    rendered_lines: VecDeque<VecBuffer>,
    cursor: (usize, usize),
}

impl<T: FrameBuffer> Console<T> {
    pub fn new(fb: T, fg: Color, bg: Color) -> Self {
        let format = fb.format();
        let size = (
            fb.width() / font::WIDTH as usize,
            fb.height() / font::HEIGHT as usize,
        );
        let line_size = (size.0 * font::WIDTH as usize, font::HEIGHT as usize);
        Self {
            fb,
            fg,
            bg,
            size,
            char_cache: BTreeMap::new(),
            rendered_lines: vec![VecBuffer::new(line_size.0, line_size.1, format); size.1].into(),
            cursor: (0, 0),
        }
    }

    pub fn clear(&mut self) {
        for rendered_line in self.rendered_lines.iter_mut() {
            rendered_line.clear(self.bg);
        }
        self.cursor = (0, 0);
    }

    pub fn render(&mut self) {
        let ox = (self.fb.width() as i32 - self.size.0 as i32 * font::WIDTH as i32) / 2;
        let oy = (self.fb.height() as i32 - self.size.1 as i32 * font::HEIGHT as i32) / 2;
        for (i, rendered_line) in self.rendered_lines.iter().enumerate() {
            self.fb
                .blit(ox, oy + i as i32 * font::HEIGHT as i32, rendered_line);
        }
    }

    pub fn put(&mut self, c: char) {
        // wrapping / line feed
        if c == '\n' || self.cursor.0 >= self.size.0 {
            self.cursor.0 = 0;
            self.cursor.1 += 1;
        }
        // remove the first line
        if self.cursor.1 >= self.size.1 {
            self.cursor.1 = self.size.1 - 1;
            let mut next_line = self.rendered_lines.pop_front().unwrap();
            next_line.clear(self.bg);
            self.rendered_lines.push_back(next_line);
        }
        if c != '\n' {
            let fg = self.fg;
            let bg = self.bg;
            let format = self.fb.format();
            let char_buf = self.char_cache.entry(c).or_insert_with_key(|c| {
                let mut buf = VecBuffer::new(font::WIDTH as usize, font::HEIGHT as usize, format);
                buf.write_char(0, 0, *c, fg, bg);
                buf
            });
            let (x, y) = self.cursor;
            self.rendered_lines[y].blit(x as i32 * font::WIDTH as i32, 0, char_buf);
            self.cursor.0 += 1;
        }
    }
}

impl<T: FrameBuffer> fmt::Write for Console<T> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.put(c);
        }
        Ok(())
    }
}
