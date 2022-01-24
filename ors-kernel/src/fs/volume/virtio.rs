mod virtio {
    pub use crate::devices::virtio::block::*;
}
use super::{Volume, VolumeError};
use derive_new::new;

impl From<virtio::Error> for VolumeError {
    fn from(e: virtio::Error) -> Self {
        match e {
            virtio::Error::Io => Self::Io,
            virtio::Error::OutOfRange => Self::OutOfRange,
            _ => Self::Unknown,
        }
    }
}

/// Let the entire VirtIO block as a single volume.
#[derive(Debug, Clone, Copy, new)]
pub struct VirtIOBlockVolume(&'static virtio::Block);

impl Volume for VirtIOBlockVolume {
    fn sector_count(&self) -> usize {
        self.0.capacity() as usize
    }

    fn sector_size(&self) -> usize {
        virtio::Block::SECTOR_SIZE
    }

    fn read(&self, sector: usize, buf: &mut [u8]) -> Result<(), VolumeError> {
        Ok(self.0.read(sector as u64, buf)?)
    }

    fn write(&self, sector: usize, buf: &[u8]) -> Result<(), VolumeError> {
        Ok(self.0.write(sector as u64, buf)?)
    }
}
