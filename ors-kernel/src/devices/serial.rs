use crate::sync::spin::{Spin, SpinGuard};
pub use uart_16550::SerialPort as Port;

const DEFAULT_PORT_ADDRESS: u16 = 0x3f8;

static DEFAULT_PORT: Spin<Port> = Spin::new(unsafe { Port::new(DEFAULT_PORT_ADDRESS) });

pub fn default_port() -> SpinGuard<'static, Port> {
    DEFAULT_PORT.lock()
}

/// Default port with no locking mechanism.
/// Used for debugging output in interrupt handlers and panic handlers.
pub fn raw_default_port() -> Port {
    unsafe { Port::new(DEFAULT_PORT_ADDRESS) }
}
