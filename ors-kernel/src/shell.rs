//! A rough shell implementation for debugging.

use crate::console::{input_queue, Input};
use crate::devices;
use crate::interrupts::{ticks, TIMER_FREQ};
use crate::phys_memory::frame_manager;
use alloc::string::String;
use core::fmt;

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
                let t = ticks();
                execute_command(&command_buf);
                let t = ticks() - t;
                command_buf.clear();
                cursor = 0;
                kprintln!(
                    "elapsed = {}ms",
                    (t as f64 / TIMER_FREQ as f64 * 1000.0) as u32
                );
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
        "stats" => {
            kprintln!("[phys_memory]");
            {
                let mut graph = [0.0; 100];
                let (total, available) = {
                    let fm = frame_manager();
                    let total = fm.total_frames();
                    let available = fm.available_frames();
                    for i in 0..100 {
                        graph[i] =
                            fm.availability_in_range(i as f64 / 100.0, (i + 1) as f64 / 100.0);
                    }
                    (total, available)
                };
                for a in graph {
                    kprint!("\x1b[48;5;{}m \x1b[0m", 232 + (23.0 * a) as usize);
                }
                kprintln!();
                kprintln!(
                    "{}/{} frames ({}/{})",
                    available,
                    total,
                    PrettySize(available * 4096),
                    PrettySize(total * 4096)
                );
            }
        }
        "lspci" => {
            for d in devices::pci::devices() {
                unsafe {
                    let ty = d.device_type();
                    kprintln!("{:02x}:{:02x}.{:02x} = {{", d.bus, d.device, d.function);
                    kprint!("  vendor_id = {:x}", d.vendor_id());
                    if d.is_vendor_intel() {
                        kprint!(" (intel)");
                    }
                    kprintln!();
                    kprint!("  device_id = {:x}", d.device_id());
                    if d.is_virtio() {
                        kprint!(" (virtio)");
                    }
                    kprintln!();
                    kprintln!(
                        "  device_type = {{ class_code = {:02x}, subclass = {:02x}, interface = {:02x} }}",
                        ty.class_code,
                        ty.subclass,
                        ty.prog_interface
                    );
                    if d.is_virtio() {
                        kprintln!("  subsystem_id = {}", d.subsystem_id());
                    }
                    if let Some(cap) = d.msi_x() {
                        kprintln!("  msi-x = {{}}"); // TODO
                    }
                    kprintln!("}}");
                }
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

struct PrettySize(usize);

impl fmt::Display for PrettySize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 1024 {
            write!(f, "{}B", self.0)
        } else if self.0 < 1024 * 1024 {
            write!(f, "{:.2}KiB", (self.0 as f64) / 1024.0)
        } else if self.0 < 1024 * 1024 * 1024 {
            write!(f, "{:.2}MiB", (self.0 as f64) / (1024.0 * 1024.0))
        } else {
            write!(f, "{:.2}GiB", (self.0 as f64) / (1024.0 * 1024.0 * 1024.0))
        }
    }
}
