use super::graphics;
use super::pci::Device;
use super::phys_memory;
use heapless::Vec;
use spin::{Mutex, MutexGuard, Once};
use uart_16550::SerialPort;

pub type PhysMemoryManager = phys_memory::BitmapMemoryManager;
pub type FrameBuffer = &'static mut (dyn graphics::FrameBuffer + Send + Sync);
pub type PciDevices = Vec<Device, 32>;
pub type Console = graphics::Console<80, 25>;

static PHYS_MEMORY_MANAGER: Mutex<PhysMemoryManager> = Mutex::new(PhysMemoryManager::new());
static FRAME_BUFFER: Once<Mutex<FrameBuffer>> = Once::new();
static PCI_DEVICES: Once<PciDevices> = Once::new();
static DEFAULT_CONSOLE: Mutex<Console> = Mutex::new(Console::new());
static DEFAULT_SERIAL_PORT: Mutex<SerialPort> = Mutex::new(unsafe { SerialPort::new(0x3f8) });

pub fn phys_memory_manager() -> MutexGuard<'static, PhysMemoryManager> {
    PHYS_MEMORY_MANAGER.lock()
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
