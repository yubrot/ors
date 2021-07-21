#[repr(C)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub enum PixelFormat {
    Rgb,
    Bgr,
}

#[repr(C)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub struct FrameBuffer {
    pub frame_buffer: *mut u8,
    pub stride: u32,
    pub resolution: (u32, u32), // (horizontal, vertical)
    pub format: PixelFormat,
}
