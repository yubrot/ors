mod virtio {
    pub use crate::devices::virtio::block::*;
}

pub mod fat;

/// Storage area used by the file system.
pub trait Volume {
    fn sector_count(&self) -> usize;
    fn sector_size(&self) -> usize;
    fn read(&self, sector: usize, buf: &mut [u8]) -> Result<(), Error>;
    fn write(&self, sector: usize, buf: &[u8]) -> Result<(), Error>;
}

/// Error during volume operations.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Error {
    Io,
    OutOfRange,
    Unknown,
}

impl From<virtio::Error> for Error {
    fn from(e: virtio::Error) -> Self {
        match e {
            virtio::Error::Io => Self::Io,
            virtio::Error::OutOfRange => Self::OutOfRange,
            _ => Self::Unknown,
        }
    }
}

/// Let the entire VirtIO block as a single volume.
#[derive(Debug, Clone, Copy)]
pub struct VirtIOBlockVolume(&'static virtio::Block);

impl From<&'static virtio::Block> for VirtIOBlockVolume {
    fn from(b: &'static virtio::Block) -> Self {
        VirtIOBlockVolume(b)
    }
}

impl Volume for VirtIOBlockVolume {
    fn sector_count(&self) -> usize {
        self.0.capacity() as usize
    }

    fn sector_size(&self) -> usize {
        virtio::Block::SECTOR_SIZE
    }

    fn read(&self, sector: usize, buf: &mut [u8]) -> Result<(), Error> {
        Ok(self.0.read(sector as u64, buf)?)
    }

    fn write(&self, sector: usize, buf: &[u8]) -> Result<(), Error> {
        Ok(self.0.write(sector as u64, buf)?)
    }
}
