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

    pub(super) fn boot_sector(&self) -> &BootSector {
        &self.bs
    }

    pub(super) fn fat_entries(&self) -> FatEntries<V> {
        FatEntries {
            root: self,
            cursor: Some((Cluster(2), None)),
        }
    }

    pub(super) fn fat_entry<'a>(
        &'a self,
        cluster: Cluster,
        prev: Option<BufferedFatEntryRef<'a>>, // used to reduce sector search
    ) -> Result<BufferedFatEntryRef<'a>, Error> {
        let (sector, offset) = self.bs.fat_entry_location(cluster);
        let buf = match prev {
            Some(r) if r.buf.sector() == sector => r.buf,
            _ => self.volume.sector(sector)?,
        };
        Ok(BufferedFatEntryRef { buf, offset })
    }

    pub(super) fn cluster(&self, cluster: Cluster) -> BufferedCluster<V> {
        let first_sector = self.bs.cluster_location(cluster);
        BufferedCluster {
            cluster,
            volume: &self.volume,
            first_sector,
            sector_count: self.bs.cluster_size(),
            sector_size: self.bs.sector_size(),
        }
    }

    pub(super) fn chained_cluster(
        &self,
        cluster: Cluster,
    ) -> Result<Option<BufferedCluster<V>>, Error> {
        let entry = self.fat_entry(cluster, None)?;
        Ok(match entry.read() {
            FatEntry::UsedChained(cluster) => Some(self.cluster(cluster)),
            _ => None,
        })
    }

    pub(super) fn dir_entries(&self, cluster: Cluster) -> DirEntries<V> {
        DirEntries {
            root: self,
            cursor: Some((self.cluster(cluster), 0, None)),
        }
    }
}

#[derive(Debug)]
pub(super) struct BufferedFatEntryRef<'a> {
    buf: BufferedSectorRef<'a>,
    offset: usize,
}

impl<'a> BufferedFatEntryRef<'a> {
    pub(super) fn read(&self) -> FatEntry {
        u32::from_le_bytes(self.buf.bytes().array::<4>(self.offset)).into()
    }

    pub(super) fn write(&self, value: FatEntry) {
        self.buf
            .bytes()
            .copy_from_array::<4>(self.offset, u32::to_le_bytes(value.into()));
        self.buf.mark_as_dirty();
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BufferedCluster<'a, V> {
    cluster: Cluster,
    volume: &'a BufferedVolume<V>,
    first_sector: Sector,
    sector_count: usize,
    sector_size: usize,
}

impl<'a, V: Volume> BufferedCluster<'a, V> {
    fn sector(
        &self,
        index: usize,
        prev: Option<BufferedSectorRef<'a>>, // used to reduce sector search
    ) -> Result<BufferedSectorRef<'a>, Error> {
        debug_assert!(index < self.sector_count);
        let sector = self.first_sector.offset(index);
        match prev {
            Some(r) if r.sector() == sector => Ok(r),
            _ => Ok(self.volume.sector(sector)?),
        }
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

    pub(super) fn size(&self) -> usize {
        self.sector_size * self.sector_count
    }

    pub(super) fn read(
        &self,
        offset: usize,
        mut buf: &mut [u8],
        mut prev: Option<BufferedSectorRef<'a>>,
    ) -> Result<Option<BufferedSectorRef<'a>>, Error> {
        for (sector, i, j) in self.sector_range(offset, offset + buf.len()) {
            let curr = self.sector(sector, prev)?;
            buf[0..j - i].copy_from_slice(&curr.bytes()[i..j]);
            buf = &mut buf[j - i..];
            prev = Some(curr);
        }
        Ok(prev)
    }

    pub(super) fn write(
        &self,
        offset: usize,
        mut buf: &[u8],
        mut prev: Option<BufferedSectorRef<'a>>,
    ) -> Result<Option<BufferedSectorRef<'a>>, Error> {
        for (sector, i, j) in self.sector_range(offset, offset + buf.len()) {
            let curr = self.sector(sector, prev)?;
            curr.bytes()[i..j].copy_from_slice(&buf[0..j - i]);
            buf = &buf[j - i..];
            prev = Some(curr);
        }
        Ok(prev)
    }

    pub(super) fn dir_entries_count(&self) -> usize {
        self.size() / DirEntry::SIZE
    }

    pub(super) fn read_dir_entry(
        &self,
        index: usize,
        prev: Option<BufferedSectorRef<'a>>,
    ) -> Result<(DirEntry, Option<BufferedSectorRef<'a>>), Error> {
        let mut buf = [0; DirEntry::SIZE];
        let curr = self.read(index * DirEntry::SIZE, buf.as_mut(), prev)?;
        Ok((buf.into(), curr))
    }

    pub(super) fn write_dir_entry(
        &self,
        index: usize,
        entry: DirEntry,
        prev: Option<BufferedSectorRef<'a>>,
    ) -> Result<Option<BufferedSectorRef<'a>>, Error> {
        let buf: [u8; 32] = entry.into();
        let curr = self.write(index * DirEntry::SIZE, buf.as_ref(), prev)?;
        Ok(curr)
    }
}

#[derive(Debug)]
pub(super) struct FatEntries<'a, V> {
    root: &'a Root<V>,
    cursor: Option<(Cluster, Option<BufferedFatEntryRef<'a>>)>,
}

impl<'a, V: Volume> Iterator for FatEntries<'a, V> {
    type Item = (Cluster, FatEntry);

    fn next(&mut self) -> Option<Self::Item> {
        let (n, prev) = core::mem::replace(&mut self.cursor, None)?;
        if self.root.bs.is_cluster_available(n) {
            let curr = self.root.fat_entry(n, prev).trace_err()?;
            let entry = curr.read();
            self.cursor = Some((n.offset(1), Some(curr)));
            Some((n, entry))
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub(super) struct DirEntries<'a, V> {
    root: &'a Root<V>,
    cursor: Option<(BufferedCluster<'a, V>, usize, Option<BufferedSectorRef<'a>>)>,
}

impl<'a, V: Volume> Iterator for DirEntries<'a, V> {
    type Item = (Cluster, usize, DirEntry);

    fn next(&mut self) -> Option<Self::Item> {
        let (c, n, prev) = core::mem::replace(&mut self.cursor, None)?;
        if n < c.dir_entries_count() {
            let cluster = c.cluster;
            let (entry, curr) = c.read_dir_entry(n, prev).trace_err()?;
            if !matches!(entry, DirEntry::UnusedTerminal) {
                self.cursor = Some((c, n + 1, curr));
            }
            Some((cluster, n, entry))
        } else {
            let c = self.root.chained_cluster(c.cluster).trace_err()??;
            self.cursor = Some((c, 0, None));
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
