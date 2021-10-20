use crate::graphics::{FrameBuffer, MonospaceFont, MonospaceTextBuffer, ScreenBuffer};
use crate::interrupts::{ticks, TIMER_FREQ};
use crate::sync::queue::Queue;
use crate::task;
use alloc::boxed::Box;
use core::convert::TryInto;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};
use log::trace;

mod ansi;
mod kbd;

const OUT_CHUNK_SIZE: usize = 64;

static IN: Queue<Input, 128> = Queue::new();
static OUT: Queue<heapless::String<OUT_CHUNK_SIZE>, 128> = Queue::new();
static OUT_READY: AtomicBool = AtomicBool::new(false);
static RAW_IN: Queue<RawInput, 128> = Queue::new();

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub enum Input {
    Char(char),
    Ctrl(char),
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

pub fn input_queue() -> &'static Queue<Input, 128> {
    &IN
}

#[derive(Debug, Clone, Copy)]
pub struct ConsoleWrite;

impl fmt::Write for ConsoleWrite {
    fn write_str(&mut self, mut s: &str) -> fmt::Result {
        if OUT_READY.load(Ordering::Acquire) {
            while s.len() > 0 {
                let mut i = s.len().min(OUT_CHUNK_SIZE);
                while !s.is_char_boundary(i) {
                    i -= 1;
                }
                let (chunk, next_s) = s.split_at(i);
                OUT.enqueue(chunk.into());
                s = next_s;
            }
        }
        Ok(())
    }
}

pub fn initialize(buf: ScreenBuffer) {
    trace!("INITIALIZING console");
    let buf = Box::into_raw(Box::new(buf)) as u64;
    task::scheduler().add(task::Priority::MAX, handle_output, buf);
    task::scheduler().add(task::Priority::MAX, handle_raw_input, 0);
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum RawInput {
    Kbd(u8),
    Com1(u8),
}

pub fn accept_raw_input(input: RawInput) {
    // Normally this function is called from interrupt handlers,
    // so failure of enqueuing is ignored without blocking.
    let _ = RAW_IN.try_enqueue(input);
}

extern "C" fn handle_output(buf: u64) -> ! {
    const RENDER_FREQ: usize = 30;
    const RENDER_INTERVAL: usize = TIMER_FREQ / RENDER_FREQ;
    const FONT_SIZE: u32 = 14;
    static FONT_NORMAL: &[u8] = include_bytes!("console/Tamzen7x14r.ttf");
    static FONT_BOLD: &[u8] = include_bytes!("console/Tamzen7x14b.ttf");

    let buf = unsafe { Box::from_raw(buf as *mut ScreenBuffer) };
    let format = buf.format();
    let mut buf = MonospaceTextBuffer::new(
        *buf,
        MonospaceFont::new(FONT_SIZE, FONT_NORMAL, FONT_BOLD, format),
    );
    let mut next_render_ticks = 0;

    OUT_READY.store(true, Ordering::SeqCst);

    loop {
        let t = ticks();
        if next_render_ticks <= t {
            buf.render();
            next_render_ticks = ticks() + RENDER_INTERVAL;
        }

        if let Some(out) = OUT.dequeue_timeout(next_render_ticks - t) {
            // TODO: Handle escape sequence
            for c in out.chars() {
                buf.put(c);
            }
        }
    }
}

extern "C" fn handle_raw_input(_: u64) -> ! {
    let mut kbd_decoder = kbd::Decoder::new();
    let mut com1_decoder = ansi::Decoder::new();

    loop {
        let input = RAW_IN.dequeue();
        if let Some(input) = match input {
            RawInput::Kbd(input) => kbd_decoder.add(input),
            RawInput::Com1(0x7f) => Some(Input::Char('\x08')), // DEL -> BS
            RawInput::Com1(0x0d) => Some(Input::Char('\x0A')), // CR  -> LF
            RawInput::Com1(input) if input <= 0x7e => com1_decoder
                .add_char(char::from(input))
                .and_then(|input| input.try_into().ok()),
            _ => {
                trace!("console: Unhandled raw-input: {:?}", input);
                None
            }
        } {
            let _ = IN.try_enqueue(input);
        }
    }
}
