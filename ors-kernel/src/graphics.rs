mod color;
mod console;
mod font;
mod frame_buffer;

pub use color::Color;
pub use console::{default_console, Console, ConsoleWriteOptions, ConsoleWriter};
pub use frame_buffer::{
    frame_buffer, frame_buffer_if_available, initialize_frame_buffer, FrameBuffer,
};
