use super::{BootSector, BootSectorError, DirEntry, Error, FatEntry, Sector, SliceExt, Volume};
use crate::fs::volume::{BufferedSectorRef, BufferedVolume};
use alloc::vec;
use core::fmt;
use log::trace;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub(super) struct Cluster(usize);

impl Cluster {
    pub(super) fn from_index(index: usize) -> Self {
        Self(index)
    }

    pub(super) fn index(self) -> usize {
        self.0
    }

    pub(super) fn offset(self, s: usize) -> Self {
        Self(self.0 + s)
    }
}

impl fmt::Display for Cluster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug)]
pub(super) struct Root<V> {
    volume: BufferedVolume<V>,
    bs: BootSector,
}

impl<V: Volume> Root<V> {
    pub(super) fn new(volume: V) -> Result<Self, Error> {
        let sector_size = volume.sector_size();
        let mut buf = vec![0; sector_size];

        volume.read(Sector::from_index(0), buf.as_mut())?;
        let bs = BootSector::try_from(buf.as_ref())?;

        if bs.sector_size() != sector_size {
            Err(BootSectorError::Broken("BytsPerSec (mismatch)"))?;
        }
        if volume.sector_count() < bs.total_sector_count() {
            Err(BootSectorError::Broken("TotSec (mismatch)"))?;
        }

        let volume = BufferedVolume::new(volume);
        Ok(Self { volume, bs })
    }

    pub(super) fn commit(&self) -> Result<(), Error> {
        Ok(self.volume.commit()?)
    }

    pub(super) fn boot_sector(&self) -> &BootSector {
        &self.bs
    }

    pub(super) fn fat(&self) -> BufferedFat<V> {
        BufferedFat {
            root: self,
            last: None,
        }
    }

    pub(super) fn cluster(&self, cluster: Cluster) -> BufferedCluster<V> {
        let first_sector = self.bs.cluster_location(cluster);
        BufferedCluster {
            cluster,
            volume: &self.volume,
            first_sector,
            sector_count: self.bs.cluster_size(),
            sector_size: self.bs.sector_size(),
            last: None,
        }
    }

    pub(super) fn dir_entries(&self, cluster: Cluster) -> DirEntries<V> {
        DirEntries {
            root: self,
            cursor: Some((self.cluster(cluster), 0)),
        }
    }
}

#[derive(Debug)]
pub(super) struct BufferedFat<'a, V> {
    root: &'a Root<V>,
    last: Option<BufferedSectorRef<'a>>, // cached to reduce sector search
}

impl<'a, V: Volume> BufferedFat<'a, V> {
    pub(super) fn entries<'f>(&'f mut self) -> FatEntries<'f, 'a, V> {
        FatEntries {
            fat: self,
            cursor: Some(Cluster(2)),
        }
    }

    fn entry(&mut self, cluster: Cluster) -> Result<(&BufferedSectorRef<'a>, usize), Error> {
        let (sector, offset) = self.root.bs.fat_entry_location(cluster);
        if !matches!(self.last, Some(ref r) if r.sector() == sector) {
            self.last = Some(self.root.volume.sector(sector)?);
        }
        Ok((self.last.as_ref().unwrap(), offset))
    }

    pub(super) fn allocate(&mut self) -> Result<Cluster, Error> {
        // FIXME: This implementation is too slow since it always searches from the start
        for (c, entry) in self.entries() {
            if matches!(entry, FatEntry::Unused) {
                self.write(c, FatEntry::UsedEoc)?;
                return Ok(c);
            }
        }
        Err(Error::Full)
    }

    pub(super) fn release(&mut self, c: Cluster) -> Result<(), Error> {
        let mut next_c = Some(c);
        while let Some(c) = next_c {
            match self.read(c)? {
                FatEntry::UsedChained(c) => next_c = Some(c),
                FatEntry::UsedEoc => next_c = None,
                _ => break,
            }
            self.write(c, FatEntry::Unused)?;
        }
        Ok(())
    }

    pub(super) fn read(&mut self, cluster: Cluster) -> Result<FatEntry, Error> {
        let (sector, offset) = self.entry(cluster)?;
        Ok(u32::from_le_bytes(sector.bytes().array::<4>(offset)).into())
    }

    pub(super) fn write(&mut self, cluster: Cluster, value: FatEntry) -> Result<(), Error> {
        let (sector, offset) = self.entry(cluster)?;
        sector
            .bytes()
            .copy_from_array::<4>(offset, u32::to_le_bytes(value.into()));
        sector.mark_as_dirty();
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct FatEntries<'f, 'a, V> {
    fat: &'f mut BufferedFat<'a, V>,
    cursor: Option<Cluster>,
}

impl<'f, 'a, V: Volume> Iterator for FatEntries<'f, 'a, V> {
    type Item = (Cluster, FatEntry);

    fn next(&mut self) -> Option<Self::Item> {
        let n = core::mem::take(&mut self.cursor)?;
        if self.fat.root.bs.is_cluster_available(n) {
            let entry = self.fat.read(n).trace_err()?;
            self.cursor = Some(n.offset(1));
            Some((n, entry))
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BufferedCluster<'a, V> {
    cluster: Cluster,
    volume: &'a BufferedVolume<V>,
    first_sector: Sector,
    sector_count: usize,
    sector_size: usize,
    last: Option<BufferedSectorRef<'a>>, // cached to reduce sector search
}

impl<'a, V: Volume> BufferedCluster<'a, V> {
    fn sector(&mut self, index: usize) -> Result<&BufferedSectorRef<'a>, Error> {
        debug_assert!(index < self.sector_count);
        let sector = self.first_sector.offset(index);
        if !matches!(self.last, Some(ref r) if r.sector() == sector) {
            self.last = Some(self.volume.sector(sector)?);
        }
        Ok(self.last.as_ref().unwrap())
    }

    fn sector_range(
        &self,
        start: usize,
        end: usize,
    ) -> impl Iterator<Item = (usize, usize, usize)> {
        debug_assert!(start <= end && end <= self.size());
        let ss = start / self.sector_size;
        let es = end / self.sector_size;
        let so = start % self.sector_size;
        let eo = end % self.sector_size;
        let s = self.sector_size;
        (ss..=es).filter_map(move |sector| {
            let i = if sector == ss { so } else { 0 };
            let j = if sector == es { eo } else { s };
            (i < j).then(|| (sector, i, j))
        })
    }

    pub(super) fn cluster(&self) -> Cluster {
        self.cluster
    }

    pub(super) fn size(&self) -> usize {
        self.sector_size * self.sector_count
    }

    pub(super) fn read(&mut self, offset: usize, mut buf: &mut [u8]) -> Result<(), Error> {
        for (sector, i, j) in self.sector_range(offset, offset + buf.len()) {
            let s = self.sector(sector)?;
            buf[0..j - i].copy_from_slice(&s.bytes()[i..j]);
            buf = &mut buf[j - i..];
        }
        Ok(())
    }

    pub(super) fn write(&mut self, offset: usize, mut buf: &[u8]) -> Result<(), Error> {
        for (sector, i, j) in self.sector_range(offset, offset + buf.len()) {
            let s = self.sector(sector)?;
            s.bytes()[i..j].copy_from_slice(&buf[0..j - i]);
            s.mark_as_dirty();
            buf = &buf[j - i..];
        }
        Ok(())
    }

    pub(super) fn dir_entries_count(&self) -> usize {
        self.size() / DirEntry::SIZE
    }

    pub(super) fn read_dir_entry(&mut self, index: usize) -> Result<DirEntry, Error> {
        let mut buf = [0; DirEntry::SIZE];
        self.read(index * DirEntry::SIZE, buf.as_mut())?;
        Ok(buf.into())
    }

    pub(super) fn write_dir_entry(&mut self, index: usize, entry: DirEntry) -> Result<(), Error> {
        let buf: [u8; 32] = entry.into();
        self.write(index * DirEntry::SIZE, buf.as_ref())
    }
}

#[derive(Debug)]
pub(super) struct DirEntries<'a, V> {
    root: &'a Root<V>,
    cursor: Option<(BufferedCluster<'a, V>, usize)>,
}

impl<'a, V: Volume> Iterator for DirEntries<'a, V> {
    type Item = (Cluster, usize, DirEntry);

    fn next(&mut self) -> Option<Self::Item> {
        let (mut c, n) = core::mem::take(&mut self.cursor)?;
        if n < c.dir_entries_count() {
            let cluster = c.cluster;
            let entry = c.read_dir_entry(n).trace_err()?;
            if !matches!(entry, DirEntry::UnusedTerminal) {
                self.cursor = Some((c, n + 1));
            }
            Some((cluster, n, entry))
        } else {
            let fat_entry = self.root.fat().read(c.cluster).trace_err()?;
            self.cursor = Some((self.root.cluster(fat_entry.chain()?), 0));
            self.next()
        }
    }
}

trait ResultExt {
    type Result;
    fn trace_err(self) -> Self::Result;
}

impl<T, E: fmt::Display> ResultExt for Result<T, E> {
    type Result = Option<T>;

    fn trace_err(self) -> Self::Result {
        match self {
            Ok(r) => Some(r),
            Err(e) => {
                trace!("{}", e);
                None
            }
        }
    }
}
