use super::graphics::{Buffer, Console};
use super::memory_manager::BitmapMemoryManager;

pub static mut MEMORY_MANAGER: BitmapMemoryManager = BitmapMemoryManager::new();
pub static mut BUFFER: &dyn Buffer = &();
pub static mut CONSOLE: Console<80, 25> = Console::new();
