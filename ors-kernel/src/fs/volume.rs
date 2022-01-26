use crate::sync::mutex::{Mutex, MutexGuard};
use crate::sync::spin::Spin;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};
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

    pub const INVALID: Self = Self(usize::MAX);
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

/// A volume with in-memory buffering.
#[derive(Debug)]
pub struct BufferedVolume<V> {
    volume: V,
    sectors: Spin<BufferedSectors>,
}

impl<V> BufferedVolume<V> {
    const EXPECTED_CACHE_SIZE: usize = 8;

    pub fn new(volume: V) -> Self {
        Self {
            volume,
            sectors: Spin::new(BufferedSectors {
                lent: Vec::with_capacity(8),
                cached: VecDeque::with_capacity(Self::EXPECTED_CACHE_SIZE),
            }),
        }
    }
}

impl<V: Volume> BufferedVolume<V> {
    pub fn sector_count(&self) -> usize {
        self.volume.sector_count()
    }

    pub fn sector_size(&self) -> usize {
        self.volume.sector_size()
    }

    pub fn sector(&self, sector: Sector) -> Result<BufferedSectorRef, VolumeError> {
        // NOTE: How can we optimize reading and writing of consecutive sectors?

        let mut sectors = self.sectors.lock();

        if let Some(s) = sectors.lent.iter().find(|s| s.sector() == sector) {
            let r = BufferedSectorRef::new(&self.sectors, s);
            drop(sectors);
            // This is necessary since the first initialize happens after drop(sectors) at (*1)
            r.initialize(&self.volume)?;
            return Ok(r);
        }

        let s = match sectors.cached.iter().position(|s| s.sector() == sector) {
            // Found a cached BufferedSector, use it
            Some(index) => sectors.cached.remove(index).unwrap(),
            // Recycle the least recently used BufferedSector
            None if Self::EXPECTED_CACHE_SIZE <= sectors.cached.len() => {
                let mut s = sectors.cached.pop_back().unwrap();
                // #63292: If UniqueArc is introduced, this unwrap may be removable
                Arc::get_mut(&mut s).unwrap().recycle(sector);
                s
            }
            // Create a new BufferedSector
            None => Arc::new(BufferedSector::new(sector, &self.volume)),
        };
        let r = BufferedSectorRef::new(&self.sectors, &s);
        sectors.lent.push(s);
        drop(sectors); // (*1)

        // This must happen after drop(sectors) to perform (blocking) volume reading/writing
        r.initialize(&self.volume)?;
        Ok(r)
    }

    pub fn commit(&self) -> Result<(), VolumeError> {
        let sectors = self.sectors.lock();
        // This temporary Vec is necessary since the cached sectors must be uniquely owned by BufferedVolume.
        let cached = sectors.cached.iter().map(|s| s.sector).collect::<Vec<_>>();
        drop(sectors);

        for s in cached {
            self.sector(s)?.commit(&self.volume)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
struct BufferedSectors {
    lent: Vec<Arc<BufferedSector>>,        // shared
    cached: VecDeque<Arc<BufferedSector>>, // uniquely owned
}

#[derive(Debug)]
pub struct BufferedSector {
    sector: Sector,
    data: Mutex<BufferedSectorData>,
}

impl BufferedSector {
    fn new(sector: Sector, volume: &impl Volume) -> Self {
        Self {
            sector,
            data: Mutex::new(BufferedSectorData {
                sector: None,
                is_dirty: false,
                bytes: vec![0; volume.sector_size()],
            }),
        }
    }

    fn recycle(&mut self, sector: Sector) {
        self.sector = sector;
    }

    fn initialize(&self, volume: &impl Volume) -> Result<(), VolumeError> {
        self.data.lock().initialize(self.sector, volume)
    }

    fn commit(&self, volume: &impl Volume) -> Result<(), VolumeError> {
        self.data.lock().commit(volume)
    }

    pub fn sector(&self) -> Sector {
        self.sector
    }

    pub fn is_dirty(&self) -> bool {
        self.data.lock().is_dirty
    }

    pub fn mark_as_dirty(&self) {
        self.data.lock().is_dirty = true;
    }

    pub fn bytes(&self) -> MutexGuard<impl DerefMut<Target = [u8]>> {
        self.data.lock()
    }
}

#[derive(Debug)]
struct BufferedSectorData {
    sector: Option<Sector>,
    is_dirty: bool,
    bytes: Vec<u8>,
}

impl BufferedSectorData {
    fn initialize(&mut self, sector: Sector, volume: &impl Volume) -> Result<(), VolumeError> {
        self.commit(volume)?;
        if self.sector != Some(sector) {
            volume.read(sector, self.bytes.as_mut())?;
            self.sector = Some(sector);
        }
        Ok(())
    }

    fn commit(&mut self, volume: &impl Volume) -> Result<(), VolumeError> {
        if self.is_dirty {
            volume.write(self.sector.unwrap(), self.bytes.as_ref())?;
            self.is_dirty = false;
        }
        Ok(())
    }
}

impl Deref for BufferedSectorData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.bytes.as_ref()
    }
}

impl DerefMut for BufferedSectorData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.bytes.as_mut()
    }
}

#[derive(Debug)]
pub struct BufferedSectorRef<'a> {
    sectors: &'a Spin<BufferedSectors>,
    sector: ManuallyDrop<Arc<BufferedSector>>,
}

impl<'a> BufferedSectorRef<'a> {
    fn new(sectors: &'a Spin<BufferedSectors>, sector: &Arc<BufferedSector>) -> Self {
        Self {
            sectors,
            sector: ManuallyDrop::new(Arc::clone(sector)),
        }
    }
}

impl<'a> Clone for BufferedSectorRef<'a> {
    fn clone(&self) -> Self {
        Self::new(self.sectors, &self.sector)
    }
}

impl<'a> Drop for BufferedSectorRef<'a> {
    fn drop(&mut self) {
        let mut sectors = self.sectors.lock();
        let sector = unsafe { ManuallyDrop::take(&mut self.sector) };

        // This is the last owner except sectors.lent
        if Arc::strong_count(&sector) == 2 {
            let index = sectors
                .lent
                .iter()
                .position(|s| s.sector() == sector.sector())
                .unwrap();
            drop(sector); // 2 -> 1

            // Move this BufferedSector from sectors.lent to the front of sectors.cached
            let sector = sectors.lent.swap_remove(index);
            sectors.cached.push_front(sector);
        }
    }
}

impl<'a> Deref for BufferedSectorRef<'a> {
    type Target = BufferedSector;

    fn deref(&self) -> &Self::Target {
        &self.sector
    }
}
