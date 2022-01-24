use core::fmt;
use derive_new::new;

pub mod virtio;

/// A unit of volume read/write.
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub struct Sector(usize);

impl Sector {
    pub fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub fn index(self) -> usize {
        self.0
    }

    pub fn offset(self, s: usize) -> Self {
        Self(self.0 + s)
    }
}

impl fmt::Display for Sector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Storage area used by the file system.
pub trait Volume {
    fn sector_count(&self) -> usize;
    fn sector_size(&self) -> usize;
    fn read(&self, sector: Sector, buf: &mut [u8]) -> Result<(), VolumeError>;
    fn write(&self, sector: Sector, buf: &[u8]) -> Result<(), VolumeError>;
}

/// Error during volume operations.
#[derive(PartialEq, Eq, Debug, Clone, Copy, new)]
pub struct VolumeError {
    pub sector: Sector,
    pub kind: VolumeErrorKind,
}

impl fmt::Display for VolumeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            VolumeErrorKind::Io => write!(f, "I/O error")?,
            VolumeErrorKind::OutOfRange => write!(f, "Out of range")?,
            VolumeErrorKind::Unknown => write!(f, "Unknown error")?,
        }
        write!(f, " at sector={}", self.sector)
    }
}

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum VolumeErrorKind {
    Io,
    OutOfRange,
    Unknown,
}
