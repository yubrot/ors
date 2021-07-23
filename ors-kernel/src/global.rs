use super::graphics;
use super::memory_manager::BitmapMemoryManager;
use super::pci::Device;
use heapless::Vec;
use spin::{Mutex, MutexGuard, Once};
use uart_16550::SerialPort;

pub type FrameBuffer = &'static mut (dyn graphics::FrameBuffer + Send + Sync);
pub type PciDevices = Vec<Device, 32>;
pub type Console = graphics::Console<80, 25>;

static MEMORY_MANAGER: Mutex<BitmapMemoryManager> = Mutex::new(BitmapMemoryManager::new());
static FRAME_BUFFER: Once<Mutex<FrameBuffer>> = Once::new();
static PCI_DEVICES: Once<PciDevices> = Once::new();
static DEFAULT_CONSOLE: Mutex<Console> = Mutex::new(Console::new());
static DEFAULT_SERIAL_PORT: Mutex<SerialPort> = Mutex::new(unsafe { SerialPort::new(0x3f8) });

pub fn memory_manager() -> MutexGuard<'static, BitmapMemoryManager> {
    MEMORY_MANAGER.lock()
}

pub fn frame_buffer() -> MutexGuard<'static, FrameBuffer> {
    FRAME_BUFFER.wait().lock()
}

pub fn initialize_frame_buffer(fb: FrameBuffer) {
    FRAME_BUFFER.call_once(move || Mutex::new(fb));
}

pub fn pci_devices() -> &'static PciDevices {
    PCI_DEVICES.wait()
}

pub fn initialize_devices(devices: PciDevices) {
    PCI_DEVICES.call_once(|| devices);
}

pub fn default_console() -> MutexGuard<'static, Console> {
    DEFAULT_CONSOLE.lock()
}

pub fn default_serial_port() -> MutexGuard<'static, SerialPort> {
    DEFAULT_SERIAL_PORT.lock()
}
