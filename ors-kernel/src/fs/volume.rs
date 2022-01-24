pub mod virtio;

/// Storage area used by the file system.
pub trait Volume {
    fn sector_count(&self) -> usize;
    fn sector_size(&self) -> usize;
    fn read(&self, sector: usize, buf: &mut [u8]) -> Result<(), VolumeError>;
    fn write(&self, sector: usize, buf: &[u8]) -> Result<(), VolumeError>;
}

/// Error during volume operations.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum VolumeError {
    Io,
    OutOfRange,
    Unknown,
}
