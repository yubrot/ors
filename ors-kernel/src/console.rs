use crate::graphics::{Color, ScreenBuffer, TextBuffer};
use crate::interrupts::{ticks, TIMER_FREQ};
use crate::sync::queue::Queue;
use crate::task;
use alloc::boxed::Box;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};
use log::{info, trace};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

static OUT: Queue<heapless::String<64>, 128> = Queue::new();
static OUT_READY: AtomicBool = AtomicBool::new(false);
static RAW_IN: Queue<RawInput, 128> = Queue::new();

#[derive(Debug, Clone, Copy)]
pub struct ConsoleWrite;

impl fmt::Write for ConsoleWrite {
    fn write_str(&mut self, mut s: &str) -> fmt::Result {
        if OUT_READY.load(Ordering::Acquire) {
            while s.len() > 0 {
                let mut i = s.len().min(128);
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

    let buf = unsafe { Box::from_raw(buf as *mut ScreenBuffer) };
    let mut buf = TextBuffer::new(*buf, Color::WHITE, Color::BLACK);
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
    let mut kbd = Keyboard::new(layouts::Jis109Key, ScancodeSet1, HandleControl::Ignore);

    loop {
        let input = RAW_IN.dequeue();
        match input {
            RawInput::Kbd(input) => {
                if let Ok(Some(e)) = kbd.add_byte(input) {
                    if let Some(key) = kbd.process_keyevent(e) {
                        match key {
                            DecodedKey::RawKey(key) => info!("KBD: {:?}", key),
                            DecodedKey::Unicode(ch) => info!("KBD: {}", ch),
                        }
                    }
                }
            }
            RawInput::Com1(input) => {
                info!("COM1: {}", char::from(input));
            }
        }
    }
}
