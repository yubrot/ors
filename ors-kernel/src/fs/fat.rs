//! FAT File System implementation (work in progress)

use super::volume::{Sector, Volume, VolumeError};
use alloc::string::String;
use alloc::vec;
use core::fmt;
use dir_entry::{DirEntry, SfnEntry};
use fat_entry::FatEntry;
use low_level::{Cluster, DirEntries, Root};

mod boot_sector;
mod dir_entry;
mod fat_entry;
mod low_level;

pub use boot_sector::{BootSector, Error as BootSectorError};

// TODO:
// * FAT12/16 Support
// * Handle bpb_num_fats (Currently FAT copies are completely untouched)
// * Handle _bpb_fs_info to reduce FAT traversal
// * Handle _bpb_bk_boot_sec correctly
// * Better error recovering

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
pub struct FileSystem<V> {
    root: Root<V>,
}

impl<V: Volume> FileSystem<V> {
    pub fn new(volume: V) -> Result<Self, Error> {
        Ok(Self {
            root: Root::new(volume)?,
        })
    }

    pub fn boot_sector(&self) -> &BootSector {
        self.root.boot_sector()
    }

    pub fn root_dir(&self) -> Dir<V> {
        let cluster = self.boot_sector().root_dir_cluster();
        Dir::new(&self.root, cluster)
    }
}

#[derive(Debug)]
pub struct Dir<'a, V> {
    root: &'a Root<V>,
    cluster: Cluster,
}

impl<'a, V: Volume> Dir<'a, V> {
    fn new(root: &'a Root<V>, cluster: Cluster) -> Self {
        Self { root, cluster }
    }

    pub fn files(&self) -> DirIter<V> {
        DirIter {
            root: self.root,
            cluster: self.cluster,
            inner: self.root.dir_entries(self.cluster),
        }
    }

    // TODO: create_file(..), create_dir(..)
}

#[derive(Debug)]
pub struct DirIter<'a, V> {
    root: &'a Root<V>,
    cluster: Cluster,
    inner: DirEntries<'a, V>,
}

impl<'a, V: Volume> DirIter<'a, V> {
    fn handle_entry(&mut self, (c, n, entry): (Cluster, usize, DirEntry)) -> Option<File<'a, V>> {
        match entry {
            DirEntry::UnusedTerminal => None,
            DirEntry::Unused => self.next(),
            DirEntry::Lfn(lfn) if lfn.is_last_entry() => {
                let checksum = lfn.checksum();
                let mut order = lfn.order();
                let mut name_buf = vec![0; order * 13];

                order -= 1;
                lfn.put_name_parts_into(&mut name_buf[order * 13..]);

                // Attempt to read `order` LFN entries and a SFN entry.
                // NOTE: In this implementation, broken LFNs are basically ignored by calling `handle_entry` recursively.

                // `order` LFN entries:
                while order != 0 {
                    let next @ (_, _, entry) = self.inner.next()?;
                    match entry {
                        DirEntry::Lfn(lfn)
                            if lfn.checksum() == checksum && lfn.order() == order =>
                        {
                            order -= 1;
                            lfn.put_name_parts_into(&mut name_buf[order * 13..]);
                        }
                        _ => return self.handle_entry(next),
                    }
                }

                // a SFN entry:
                let next @ (_, _, entry) = self.inner.next()?;
                match entry {
                    DirEntry::Sfn(sfn) if sfn.checksum() == checksum => {
                        // LFN is 0x0000-terminated and padded with 0xffff
                        while matches!(name_buf.last(), Some(0xffff)) {
                            name_buf.pop();
                        }
                        if name_buf.pop() != Some(0x0000) {
                            return self.handle_entry(next);
                        }
                        let name = String::from_utf16_lossy(name_buf.as_slice());
                        Some(File::new(self.root, self.cluster, name, sfn, (c, n)))
                    }
                    // failback
                    _ => return self.handle_entry(next),
                }
            }
            DirEntry::Lfn(_) => self.next(),
            DirEntry::Sfn(sfn) if sfn.is_volume_id() => self.next(),
            DirEntry::Sfn(sfn) => {
                let (_, name) = sfn.name();
                Some(File::new(self.root, self.cluster, name, sfn, (c, n)))
            }
        }
    }
}

impl<'a, V: Volume> Iterator for DirIter<'a, V> {
    type Item = File<'a, V>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.inner.next()?;
        self.handle_entry(next)
    }
}

#[derive(Debug)]
pub struct File<'a, V> {
    root: &'a Root<V>,
    dir: Cluster,
    name: String,
    last_entry: SfnEntry,
    entry_location: (Cluster, usize),
}

impl<'a, V: Volume> File<'a, V> {
    fn new(
        root: &'a Root<V>,
        dir: Cluster,
        name: String,
        last_entry: SfnEntry,
        entry_location: (Cluster, usize),
    ) -> Self {
        Self {
            root,
            dir,
            name,
            last_entry,
            entry_location,
        }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn is_read_only(&self) -> bool {
        self.last_entry.is_read_only()
    }

    pub fn is_hidden(&self) -> bool {
        self.last_entry.is_hidden()
    }

    pub fn is_system(&self) -> bool {
        self.last_entry.is_system()
    }

    // TODO: set_is_read_only, set_is_hidden, set_is_system

    pub fn is_dir(&self) -> bool {
        self.last_entry.is_directory()
    }

    pub fn as_dir(&self) -> Option<Dir<V>> {
        if self.is_dir() {
            Some(Dir::new(self.root, self.last_entry.cluster()?))
        } else {
            None
        }
    }

    // TODO: contetnts(), remove(), move()
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
