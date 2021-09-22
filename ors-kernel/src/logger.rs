#[cfg(not(test))]
use crate::graphics::{self, Color, ConsoleWriteOptions};
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
            // FIXME: If an interrupt occurs during framebuffer processing, the log in that
            // interrupt will not be written to the framebuffer. However, direct writing to the
            // framebuffer will be removed as we continue to improve the graphics implementation.
            #[cfg(not(test))]
            if let (Some(mut fb), Some(mut console)) = (
                graphics::screen_buffer_if_available(),
                graphics::default_console_if_available(),
            ) {
                writeln!(
                    console.writer(
                        &mut *fb,
                        ConsoleWriteOptions::new(0, 0, Color::WHITE, Color::BLACK),
                    ),
                    "{}: {}",
                    record.level(),
                    record.args()
                )
                .unwrap();
            }

            writeln!(serial::default_port(), "{}", record.args()).unwrap();
        });
    }

    fn flush(&self) {}
}
