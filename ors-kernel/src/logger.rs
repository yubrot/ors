#[cfg(not(test))]
use crate::graphics;
use crate::interrupts;
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
        interrupts::without_interrupts(|| {
            // FIXME: If an interrupt occurs during frame buffer processing, logs in that
            // interrupt will not be written to the frame buffer.
            #[cfg(not(test))]
            if let Some(mut c) = graphics::screen_console_if_available() {
                writeln!(&mut *c, "{}: {}", record.level(), record.args()).unwrap();
            }

            writeln!(serial::default_port(), "{}", record.args()).unwrap();
        });
    }

    fn flush(&self) {}
}
