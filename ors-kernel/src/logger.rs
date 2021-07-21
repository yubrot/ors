use super::global;
use super::graphics::Color;
use core::fmt::Write;

pub fn initialize() {
    log::set_logger(&KernelLogger).unwrap();
    log::set_max_level(log::LevelFilter::Info);
}

struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        writeln!(
            unsafe { global::CONSOLE.on(global::BUFFER, 0, 0, Color::WHITE, Color::BLACK) },
            "{}: {}",
            record.level(),
            record.args()
        )
        .unwrap();
    }

    fn flush(&self) {}
}
