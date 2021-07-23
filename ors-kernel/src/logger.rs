use super::global;
use super::graphics::{Color, ConsoleWriteOptions};
use core::fmt::Write;

pub fn initialize() {
    log::set_logger(&KernelLogger).unwrap();
    log::set_max_level(log::LevelFilter::Info);
}

struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        writeln!(
            global::default_console().writer(
                &mut **global::frame_buffer(),
                ConsoleWriteOptions::new(0, 0, Color::WHITE, Color::BLACK),
            ),
            "{}: {}",
            record.level(),
            record.args()
        )
        .unwrap();
        writeln!(global::default_serial_port(), "{}", record.args()).unwrap();
    }

    fn flush(&self) {}
}
