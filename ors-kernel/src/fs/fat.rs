//! FAT File System implementation (work in progress)

use super::volume::{Sector, Volume, VolumeError};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use dir_entry::{DirEntry as RawDirEntry, SfnEntry};
use fat_entry::FatEntry;
use log::trace;

mod boot_sector;
mod dir_entry;
mod fat_entry;

pub use boot_sector::{BootSector, Error as BootSectorError};

// TODO:
// * FAT12/16 Support
// * FSINFO support
// * Reduce Volume I/O
// * ...
// * Mark as unsafe?

/// Errors that occur during FAT file system operations.
#[derive(PartialEq, Eq, Debug)]
pub enum Error {
    Volume(VolumeError),
    BootSector(BootSectorError),
}

impl From<VolumeError> for Error {
    fn from(e: VolumeError) -> Self {
        Self::Volume(e)
    }
}

impl From<BootSectorError> for Error {
    fn from(e: BootSectorError) -> Self {
        Self::BootSector(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Volume(e) => write!(f, "{}", e),
            Error::BootSector(e) => write!(f, "{}", e),
        }
    }
}

/// Entry point of the FAT File System.
#[derive(Debug)]
pub struct FileSystem<V> {
    volume: V,
    bs: BootSector,
}

impl<V: Volume> FileSystem<V> {
    pub fn new(volume: V) -> Result<Self, Error> {
        let sector_size = volume.sector_size();
        let mut buf = vec![0; sector_size];

        volume.read(Sector::from_index(0), buf.as_mut())?;
        let bs = BootSector::try_from(buf.as_ref())?;

        if bs.sector_size() != sector_size {
            Err(BootSectorError::Broken("BytsPerSec (mismatch)"))?;
        }
        if volume.sector_count() < bs.total_sector_count() as usize {
            Err(BootSectorError::Broken("TotSec (mismatch)"))?;
        }

        Ok(Self { volume, bs })
    }

    pub fn root_dir(&self) -> Dir<V> {
        Dir {
            fs: self,
            cluster: self.bs.root_dir_cluster(),
        }
    }

    fn read_fat_entry(&self, n: Cluster, buf: &mut [u8]) -> Result<FatEntry, Error> {
        debug_assert!(self.bs.sector_size() <= buf.len());
        let (sector, offset) = self.bs.fat_entry_location(n);
        self.volume
            .read(sector, &mut buf[0..self.bs.sector_size()])?;
        Ok(FatEntry::from(u32::from_le_bytes(
            buf.array::<4>(offset as usize),
        )))
    }

    fn read_data(&self, n: Cluster, buf: &mut [u8]) -> Result<(), Error> {
        debug_assert_eq!(self.bs.cluster_size() * self.bs.sector_size(), buf.len());
        let sector = self.bs.cluster_location(n);
        Ok(self.volume.read(sector, buf)?)
    }
}

#[derive(Debug)]
pub struct Dir<'a, V> {
    fs: &'a FileSystem<V>,
    cluster: Cluster,
}

impl<'a, V: Volume> Dir<'a, V> {
    pub fn entries(&self) -> DirIter<'a, V> {
        DirIter {
            inner: self.raw_entries(),
        }
    }

    fn raw_entries(&self) -> RawDirIter<'a, V> {
        RawDirIter {
            fs: self.fs,
            cursor: RawDirCursor::Init(self.cluster),
        }
    }
}

#[derive(Debug)]
pub struct DirIter<'a, V> {
    inner: RawDirIter<'a, V>,
}

impl<'a, V: Volume> DirIter<'a, V> {
    fn handle_entry(&mut self, entry: RawDirEntry) -> Option<DirEntry> {
        match entry {
            RawDirEntry::UnusedTerminal => None,
            RawDirEntry::Unused => self.next(),
            RawDirEntry::Lfn(lfn) if lfn.is_last_entry() => {
                let checksum = lfn.checksum();
                let mut order = lfn.order();
                let mut name_buf = vec![0; order * 13];

                order -= 1;
                lfn.put_name_parts_into(&mut name_buf[order * 13..]);

                // Attempt to read `order` LFN entries and a SFN entry.
                // NOTE: In this implementation, broken LFNs are basically ignored by calling `handle_entry` recursively.

                // `order` LFN entries:
                while order != 0 {
                    let (_, _, entry) = self.inner.next()?;
                    match entry {
                        RawDirEntry::Lfn(lfn)
                            if lfn.checksum() == checksum && lfn.order() == order =>
                        {
                            order -= 1;
                            lfn.put_name_parts_into(&mut name_buf[order * 13..]);
                        }
                        entry => return self.handle_entry(entry),
                    }
                }

                // a SFN entry:
                let (_, _, entry) = self.inner.next()?;
                match entry {
                    RawDirEntry::Sfn(sfn) if sfn.checksum() == checksum => {
                        // LFN is 0x0000-terminated and padded with 0xffff
                        while matches!(name_buf.last(), Some(0xffff)) {
                            name_buf.pop();
                        }
                        if name_buf.pop() != Some(0x0000) {
                            return self.handle_entry(entry);
                        }
                        let name = String::from_utf16_lossy(name_buf.as_slice());
                        Some(DirEntry { name, sfn })
                    }
                    // failback
                    entry => return self.handle_entry(entry),
                }
            }
            RawDirEntry::Lfn(_) => self.next(),
            RawDirEntry::Sfn(sfn) if sfn.is_volume_id() => self.next(),
            RawDirEntry::Sfn(sfn) => {
                let (_, name) = sfn.name();
                Some(DirEntry { name, sfn })
            }
        }
    }
}

impl<'a, V: Volume> Iterator for DirIter<'a, V> {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let (_, _, entry) = self.inner.next()?;
        self.handle_entry(entry)
    }
}

#[derive(Debug)]
pub struct DirEntry {
    name: String,
    sfn: SfnEntry,
}

impl DirEntry {
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

impl From<SfnEntry> for DirEntry {
    fn from(sfn: SfnEntry) -> Self {
        let (_, name) = sfn.name();
        DirEntry { name, sfn }
    }
}

#[derive(Debug)]
struct RawDirIter<'a, V> {
    fs: &'a FileSystem<V>,
    cursor: RawDirCursor,
}

impl<'a, V: Volume> Iterator for RawDirIter<'a, V> {
    type Item = (Cluster, usize, RawDirEntry);

    fn next(&mut self) -> Option<Self::Item> {
        match core::mem::replace(&mut self.cursor, RawDirCursor::End) {
            RawDirCursor::Init(cluster) => {
                let buf = vec![0; self.fs.bs.cluster_size() * self.fs.bs.sector_size()];
                self.cursor = RawDirCursor::Start(cluster, buf);
                self.next()
            }
            RawDirCursor::Start(cluster, mut buf) => {
                match self.fs.read_data(cluster, buf.as_mut()) {
                    Ok(()) => {
                        self.cursor = RawDirCursor::Mid(cluster, buf, 0);
                        self.next()
                    }
                    Err(e) => {
                        trace!("Failed to read data at cluster={}: {:?}", cluster, e);
                        None
                    }
                }
            }
            RawDirCursor::Mid(cluster, buf, offset) if offset < buf.len() => {
                let entry = RawDirEntry::from(buf.array::<{ RawDirEntry::SIZE }>(offset));
                if !matches!(entry, RawDirEntry::UnusedTerminal) {
                    self.cursor = RawDirCursor::Mid(cluster, buf, offset + RawDirEntry::SIZE);
                }
                Some((cluster, offset, entry))
            }
            RawDirCursor::Mid(cluster, mut buf, _) => {
                match self.fs.read_fat_entry(cluster, buf.as_mut()) {
                    Ok(FatEntry::UsedChained(cluster)) => {
                        self.cursor = RawDirCursor::Start(cluster, buf);
                        self.next()
                    }
                    Ok(_) => None,
                    Err(e) => {
                        trace!("Failed to read FAT entry of cluster={}: {:?}", cluster, e);
                        None
                    }
                }
            }
            RawDirCursor::End => None,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
enum RawDirCursor {
    Init(Cluster),
    Start(Cluster, Vec<u8>),
    Mid(Cluster, Vec<u8>, usize),
    End,
}

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

trait SliceExt {
    fn array<const N: usize>(&self, offset: usize) -> [u8; N];
    fn copy_from_array<const N: usize>(&mut self, offset: usize, array: [u8; N]);
}

impl SliceExt for [u8] {
    fn array<const N: usize>(&self, offset: usize) -> [u8; N] {
        let mut ret = [0; N];
        ret.copy_from_slice(&self[offset..offset + N]);
        ret
    }

    fn copy_from_array<const N: usize>(&mut self, offset: usize, array: [u8; N]) {
        self[offset..offset + N].copy_from_slice(&array);
    }
}
