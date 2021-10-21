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
        sprintln!("{}: {}", record.level(), record.args());
    }

    fn flush(&self) {}
}
