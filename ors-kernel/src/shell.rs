//! A rough shell implementation for debugging.

use crate::console::{input_queue, Input};
use crate::devices;
use alloc::string::String;

static CLEAR: &str = "\x1b[H\x1b[2J";
static INPUT_START: &str = "\x1b[G\x1b[32m$\x1b[0m ";
static INPUT_END: &str = "\x1b[K";
static CURSOR_START: &str = "\x1b[30;47m";
static CURSOR_END: &str = "\x1b[0m";

pub extern "C" fn run(_: u64) -> ! {
    let mut command_buf = String::new();
    let mut cursor = 0;

    kprint!("{}", CLEAR);
    kprintln!("[ors shell]");

    loop {
        kprint!("{}", INPUT_START);
        for (i, c) in command_buf.chars().enumerate() {
            if i == cursor {
                kprint!("{}{}{}", CURSOR_START, c, CURSOR_END);
            } else {
                kprint!("{}", c);
            }
        }
        if cursor == command_buf.chars().count() {
            kprint!("{} {}", CURSOR_START, CURSOR_END);
        }
        kprint!("{}", INPUT_END);

        match input_queue().dequeue() {
            Input::Char('\n') => {
                kprintln!("{}{}{}", INPUT_START, &command_buf, INPUT_END);
                execute_command(&command_buf);
                command_buf.clear();
                cursor = 0;
            }
            Input::Char('\x08' /* BS */) if 0 < cursor => {
                cursor -= 1;
                command_buf.remove(cursor);
            }
            Input::Char('\x7f' /* DEL */) if cursor < command_buf.len() => {
                command_buf.remove(cursor);
            }
            Input::Char(c) if ' ' <= c && c <= '~' => {
                command_buf.insert(cursor, c);
                cursor += 1;
            }
            Input::Home => cursor = 0,
            Input::End => cursor = command_buf.len(),
            Input::ArrowLeft if 0 < cursor => cursor -= 1,
            Input::ArrowRight if cursor < command_buf.len() => cursor += 1,
            _ => {}
        }
    }
}

fn execute_command(command_buf: &str) {
    match command_buf.trim() {
        "clear" => kprint!("{}", CLEAR),
        "lspci" => {
            for d in devices::pci::devices() {
                kprintln!("device({}, {}, {}) = {{", d.bus, d.device, d.function);
                kprintln!("  device_id = {}", d.device_id());
                kprintln!("  vendor_id = {:x}", d.vendor_id());
                kprintln!("}}");
            }
        }
        "color" => {
            fn p(n: i32) {
                kprint!("\x1b[48;5;{}m{:>4}\x1b[0m", n, n);
            }

            for i in 0..16 {
                p(i);
                if i % 8 == 7 {
                    kprintln!();
                }
            }
            kprintln!();

            for i in 0..2 {
                for j in 0..6 {
                    for k in 0..3 {
                        for l in 0..6 {
                            p(16 + l + 36 * k + 6 * j + 108 * i);
                        }
                        kprint!(" ");
                    }
                    kprintln!();
                }
                kprintln!();
            }

            for i in 232..256 {
                p(i);
            }
            kprintln!();
            kprintln!();
        }
        "shutdown" => devices::qemu::exit(devices::qemu::ExitCode::Success),
        "" => {}
        cmd => kprintln!("Unsupported command: {}", cmd),
    }
}
