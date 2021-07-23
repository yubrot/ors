use super::graphics;
use super::memory_manager::BitmapMemoryManager;
use super::pci::Device;
use heapless::Vec;
use spin::{Mutex, MutexGuard, Once};

pub type Devices = Vec<Device, 32>;
pub type FrameBuffer = &'static mut (dyn graphics::FrameBuffer + Send + Sync);
pub type Console = graphics::Console<80, 25>;

static MEMORY_MANAGER: Mutex<BitmapMemoryManager> = Mutex::new(BitmapMemoryManager::new());
static FRAME_BUFFER: Once<Mutex<FrameBuffer>> = Once::new();
static CONSOLE: Mutex<Console> = Mutex::new(Console::new());
static DEVICES: Once<Devices> = Once::new();

pub fn memory_manager() -> MutexGuard<'static, BitmapMemoryManager> {
    MEMORY_MANAGER.lock()
}

pub fn frame_buffer() -> MutexGuard<'static, FrameBuffer> {
    FRAME_BUFFER.wait().lock()
}

pub fn initialize_frame_buffer(fb: FrameBuffer) {
    FRAME_BUFFER.call_once(move || Mutex::new(fb));
}

pub fn console() -> MutexGuard<'static, Console> {
    CONSOLE.lock()
}

pub fn devices() -> &'static Devices {
    DEVICES.wait()
}

pub fn initialize_devices(devices: Devices) {
    DEVICES.call_once(|| devices);
}
