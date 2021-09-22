use super::Color;
use alloc::vec;
use alloc::vec::Vec;
use core::slice;
use ors_common::frame_buffer::{FrameBuffer as RawFrameBuffer, PixelFormat as RawPixelFormat};
use spin::{Mutex, MutexGuard, Once};

static SCREEN_BUFFER: Once<Mutex<ScreenBuffer>> = Once::new();

pub fn screen_buffer() -> MutexGuard<'static, ScreenBuffer> {
    SCREEN_BUFFER.wait().lock()
}

pub fn screen_buffer_if_available() -> Option<MutexGuard<'static, ScreenBuffer>> {
    SCREEN_BUFFER.get()?.try_lock()
}

pub fn initialize_screen_buffer(fb: RawFrameBuffer) {
    SCREEN_BUFFER.call_once(move || Mutex::new(fb.into()));
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum FrameBufferFormat {
    Rgbx, // [R, G, B, _, R, G, B, _, ..; stride * height * 4]
    Bgrx, // [B, G, R, _, B, G, R, _, ..; stride * height * 4]
}

impl FrameBufferFormat {
    pub fn encoder(&self) -> fn(Color) -> [u8; 4] {
        match self {
            Self::Rgbx => |c| [c.r, c.g, c.b, 255],
            Self::Bgrx => |c| [c.b, c.g, c.r, 255],
        }
    }

    pub fn decoder(&self) -> fn([u8; 4]) -> Color {
        match self {
            Self::Rgbx => |a| Color::new(a[0], a[1], a[2]),
            Self::Bgrx => |a| Color::new(a[2], a[1], a[0]),
        }
    }
}

impl From<RawPixelFormat> for FrameBufferFormat {
    fn from(f: RawPixelFormat) -> Self {
        match f {
            RawPixelFormat::Rgb => Self::Rgbx,
            RawPixelFormat::Bgr => Self::Bgrx,
        }
    }
}

pub trait FrameBuffer {
    fn bytes(&self) -> &[u8];
    fn bytes_mut(&mut self) -> &mut [u8];
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn stride(&self) -> usize;
    fn format(&self) -> FrameBufferFormat;
}

#[derive(Debug)]
pub struct VecBuffer {
    data: Vec<u8>,
    width: usize,
    height: usize,
    format: FrameBufferFormat,
}

impl VecBuffer {
    pub fn new(width: usize, height: usize, format: FrameBufferFormat) -> Self {
        Self {
            data: vec![0; width * height * 4],
            width,
            height,
            format,
        }
    }
}

impl FrameBuffer for VecBuffer {
    fn bytes(&self) -> &[u8] {
        self.data.as_slice()
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        self.data.as_mut_slice()
    }

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn stride(&self) -> usize {
        self.width
    }

    fn format(&self) -> FrameBufferFormat {
        self.format
    }
}

#[derive(Debug)]
pub struct ScreenBuffer {
    ptr: *mut u8,
    stride: usize,
    width: usize,
    height: usize,
    format: FrameBufferFormat,
}

impl FrameBuffer for ScreenBuffer {
    fn bytes(&self) -> &[u8] {
        unsafe {
            slice::from_raw_parts(
                self.ptr as *const u8,
                (self.stride * self.height * 4) as usize,
            )
        }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr, (self.stride * self.height * 4) as usize) }
    }

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn format(&self) -> FrameBufferFormat {
        self.format
    }

    fn stride(&self) -> usize {
        self.stride
    }
}

impl From<RawFrameBuffer> for ScreenBuffer {
    fn from(fb: RawFrameBuffer) -> Self {
        Self {
            ptr: fb.frame_buffer,
            stride: fb.stride as usize,
            width: fb.resolution.0 as usize,
            height: fb.resolution.1 as usize,
            format: fb.format.into(),
        }
    }
}

unsafe impl Send for ScreenBuffer {}

unsafe impl Sync for ScreenBuffer {}
