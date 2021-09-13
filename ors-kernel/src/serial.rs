use spin::{Mutex, MutexGuard};
pub use uart_16550::SerialPort as Port;

static DEFAULT_PORT: Mutex<Port> = Mutex::new(unsafe { Port::new(0x3f8) });

pub fn default_port() -> MutexGuard<'static, Port> {
    DEFAULT_PORT.lock()
}
