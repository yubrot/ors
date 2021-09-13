#[cfg(not(test))]
use crate::graphics::{self, Color, ConsoleWriteOptions};
use crate::serial;
use core::fmt::Write;

pub fn initialize() {
    log::set_logger(&KernelLogger).unwrap();
    #[cfg(test)]
    log::set_max_level(log::LevelFilter::Trace);
    #[cfg(not(test))]
    log::set_max_level(log::LevelFilter::Info);
}

struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        #[cfg(not(test))]
        if let Some(mut fb) = graphics::frame_buffer_if_available() {
            writeln!(
                graphics::default_console().writer(
                    &mut **fb,
                    ConsoleWriteOptions::new(0, 0, Color::WHITE, Color::BLACK),
                ),
                "{}: {}",
                record.level(),
                record.args()
            )
            .unwrap();
        }
        writeln!(serial::default_port(), "{}", record.args()).unwrap();
    }

    fn flush(&self) {}
}
