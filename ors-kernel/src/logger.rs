use crate::devices;
use crate::graphics;
use core::fmt::Write;

pub fn register() {
    log::set_logger(&KernelLogger).unwrap();
    log::set_max_level(log::LevelFilter::Info);
}

struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if let Some(mut sc) = graphics::screen_console_if_initialized() {
            writeln!(sc, "{}: {}", record.level(), record.args()).unwrap();
        }
        writeln!(devices::serial::default_port(), "{}", record.args()).unwrap();
    }

    fn flush(&self) {}
}
