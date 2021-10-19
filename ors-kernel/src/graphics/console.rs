use super::{Color, FrameBuffer, ScreenBuffer, TextBuffer};
use crate::sync::mutex::{Mutex, MutexGuard};
use core::fmt;
use log::trace;
use spin::Once;

// TODO: Separate writing to the console and drawing to the screen

static SCREEN_CONSOLE: Once<Mutex<Console<ScreenBuffer>>> = Once::new();

pub fn initialize_screen_console(sb: ScreenBuffer) {
    SCREEN_CONSOLE.call_once(move || {
        trace!("INITIALIZING screen console");
        Mutex::new(Console::new(sb, Color::WHITE, Color::BLACK))
    });
}

pub fn screen_console() -> MutexGuard<'static, Console<ScreenBuffer>> {
    SCREEN_CONSOLE
        .get()
        .expect("console::screen_console is called before console::initialize_screen_console")
        .lock()
}

pub fn screen_console_if_initialized() -> Option<MutexGuard<'static, Console<ScreenBuffer>>> {
    Some(SCREEN_CONSOLE.get()?.lock())
}

pub struct Console<T> {
    buf: TextBuffer<T>,
}

impl<T: FrameBuffer> Console<T> {
    pub fn new(fb: T, fg: Color, bg: Color) -> Self {
        Self {
            buf: TextBuffer::new(fb, fg, bg, true),
        }
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn render(&mut self) {
        self.buf.render();
    }

    pub fn put(&mut self, c: char) {
        self.buf.put(c);
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
