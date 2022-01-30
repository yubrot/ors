//! FAT File System implementation.

use super::volume::{Sector, Volume, VolumeError};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use dir_entry::{DirEntry, SfnEntry};
use fat_entry::FatEntry;
use low_level::{BufferedCluster, Cluster, DirEntries, Root};

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
    Full,
    DirectoryNotEmpty,
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
            Self::Volume(e) => write!(f, "{}", e),
            Self::BootSector(e) => write!(f, "{}", e),
            Self::Full => write!(f, "Full"),
            Self::DirectoryNotEmpty => write!(f, "Directory not empty"),
        }
    }
}

/// Entry point of the FAT File System.
#[derive(Debug)]
pub struct FileSystem<V> {
    root: Root<V>,
}

impl<V: Volume> FileSystem<V> {
    pub fn new(volume: V) -> Result<Self, Error> {
        Ok(Self {
            root: Root::new(volume)?,
        })
    }

    pub fn commit(&self) -> Result<(), Error> {
        self.root.commit()
    }

    pub fn boot_sector(&self) -> &BootSector {
        self.root.boot_sector()
    }

    pub fn root_dir(&self) -> Dir<V> {
        let cluster = self.boot_sector().root_dir_cluster();
        Dir {
            root: &self.root,
            cluster,
        }
    }
}

#[derive(Debug)]
pub struct Dir<'a, V> {
    root: &'a Root<V>,
    cluster: Cluster,
}

impl<'a, V: Volume> Dir<'a, V> {
    pub fn files(&self) -> DirIter<'a, V> {
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
                let next @ (lc, ln, entry) = self.inner.next()?;
                match entry {
                    DirEntry::Sfn(sfn) if sfn.checksum() == checksum => {
                        // LFN is 0x0000-terminated and padded with 0xffff
                        while matches!(name_buf.last(), Some(0xffff)) {
                            name_buf.pop();
                        }
                        if matches!(name_buf.last(), Some(0x0000)) {
                            name_buf.pop();
                        }
                        let name = String::from_utf16_lossy(name_buf.as_slice());
                        Some(File {
                            root: self.root,
                            dir: self.cluster,
                            name,
                            entry_location: (c, n),
                            last_entry: (sfn, lc, ln),
                        })
                    }
                    // failback
                    _ => return self.handle_entry(next),
                }
            }
            DirEntry::Lfn(_) => self.next(),
            DirEntry::Sfn(sfn) if sfn.is_volume_id() => self.next(),
            DirEntry::Sfn(sfn) => {
                let (_, name) = sfn.name();
                Some(File {
                    root: self.root,
                    dir: self.cluster,
                    name,
                    entry_location: (c, n),
                    last_entry: (sfn, c, n),
                })
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
    entry_location: (Cluster, usize),
    last_entry: (SfnEntry, Cluster, usize),
}

impl<'a, V: Volume> File<'a, V> {
    fn write_back(&mut self) -> Result<(), Error> {
        self.last_entry.0.mark_archive();
        let (entry, c, n) = self.last_entry;
        self.root
            .cluster(c)
            .write_dir_entry(n, DirEntry::Sfn(entry))
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn is_read_only(&self) -> bool {
        self.last_entry.0.is_read_only()
    }

    pub fn is_hidden(&self) -> bool {
        self.last_entry.0.is_hidden()
    }

    pub fn is_system(&self) -> bool {
        self.last_entry.0.is_system()
    }

    // TODO: set_is_read_only, set_is_hidden, set_is_system

    pub fn archive(&self) -> bool {
        self.last_entry.0.archive()
    }

    pub fn is_dir(&self) -> bool {
        self.last_entry.0.is_directory()
    }

    pub fn as_dir(&self) -> Option<Dir<'a, V>> {
        if self.is_dir() {
            Some(Dir {
                root: self.root,
                cluster: self.last_entry.0.cluster()?,
            })
        } else {
            None
        }
    }

    pub fn file_size(&self) -> usize {
        self.last_entry.0.file_size()
    }

    fn set_file_size(&mut self, size: usize) -> Result<(), Error> {
        self.last_entry.0.set_file_size(size);
        self.write_back()
    }

    fn cluster(&self) -> Option<Cluster> {
        self.last_entry.0.cluster()
    }

    fn set_cluster(&mut self, cluster: Option<Cluster>) -> Result<(), Error> {
        self.last_entry.0.set_cluster(cluster);
        self.write_back()
    }

    pub fn reader(&self) -> Option<FileReader<V>> {
        if self.is_dir() {
            None
        } else {
            Some(FileReader {
                root: self.root,
                rest_size: self.file_size(),
                cursor: self.cluster().map(|c| (self.root.cluster(c), 0)),
            })
        }
    }

    pub fn overwriter(&'a mut self) -> Option<FileWriter<'a, V>> {
        if self.is_dir() {
            None
        } else {
            Some(FileWriter {
                file: self,
                total_size: 0,
                cursor: None,
            })
        }
    }

    pub fn appender(&'a mut self) -> Option<FileWriter<'a, V>> {
        if self.is_dir() {
            None
        } else {
            let mut total_size = 0;
            let cursor = if let Some(c) = self.cluster() {
                let mut c = self.root.cluster(c);
                let mut rest_size = self.file_size();
                while c.size() < rest_size {
                    match self.root.fat().read(c.cluster()).map(|f| f.chain()) {
                        Ok(Some(next_c)) => {
                            total_size += c.size();
                            rest_size -= c.size();
                            c = self.root.cluster(next_c);
                        }
                        _ => rest_size = c.size(), // FIXME: How should we handle the broken cluster chain?
                    }
                }
                total_size += rest_size;
                Some((c, rest_size))
            } else {
                None
            };
            Some(FileWriter {
                file: self,
                total_size,
                cursor,
            })
        }
    }

    pub fn remove(self, recursive: bool) -> Result<(), Error> {
        if let Some(dir) = self.as_dir() {
            for file in dir.files() {
                if !matches!(file.name(), "." | "..") {
                    if recursive {
                        file.remove(true)?;
                    } else {
                        Err(Error::DirectoryNotEmpty)?;
                    }
                }
            }
        } else if let Some(c) = self.cluster() {
            self.root.fat().release(c)?;
        }

        let (start_c, start_offset) = self.entry_location;
        let (_, end_c, end_offset) = self.last_entry;
        let mut c = self.root.cluster(start_c);

        loop {
            let i = match c.cluster() == start_c {
                true => start_offset,
                false => 0,
            };
            let j = match c.cluster() == end_c {
                true => end_offset,
                false => c.dir_entries_count() - 1,
            };
            for offset in i..=j {
                c.write_dir_entry(offset, DirEntry::Unused)?;
            }
            if c.cluster() == end_c {
                break;
            }
            match self.root.fat().read(c.cluster())?.chain() {
                Some(next_c) => c = self.root.cluster(next_c),
                None => break, // TODO: How should we handle the broken cluster chain?
            }
        }
        Ok(())
    }

    // TODO: move()
}

#[derive(Debug)]
pub struct FileReader<'a, V> {
    root: &'a Root<V>,
    rest_size: usize,
    cursor: Option<(BufferedCluster<'a, V>, usize)>,
}

impl<'a, V: Volume> FileReader<'a, V> {
    pub fn read(&mut self, mut buf: &mut [u8]) -> Result<usize, Error> {
        let mut total_read = 0;
        while buf.len() != 0 && self.rest_size != 0 {
            let (mut c, offset) = match core::mem::take(&mut self.cursor) {
                Some(cursor) => cursor,
                None => break,
            };
            let l = buf.len().min(self.rest_size).min(c.size() - offset);
            c.read(offset, &mut buf[0..l])?;
            buf = &mut buf[l..];
            total_read += l;
            self.rest_size -= l;

            self.cursor = Some(if l == c.size() - offset {
                match self.root.fat().read(c.cluster())?.chain() {
                    Some(c) => (self.root.cluster(c), 0),
                    None => break,
                }
            } else {
                (c, offset + l)
            });
        }
        Ok(total_read)
    }

    pub fn read_to_end(mut self) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        let mut tmp = [0; 4096];
        while {
            let len = self.read(&mut tmp)?;
            buf.extend_from_slice(&tmp[0..len]);
            0 < len
        } {}
        Ok(buf)
    }
}

#[derive(Debug)]
pub struct FileWriter<'a, V: Volume> {
    file: &'a mut File<'a, V>,
    total_size: usize,
    cursor: Option<(BufferedCluster<'a, V>, usize)>,
}

impl<'a, V: Volume> FileWriter<'a, V> {
    pub fn write(&mut self, mut buf: &[u8]) -> Result<(), Error> {
        while !buf.is_empty() {
            let (mut c, offset) = match core::mem::take(&mut self.cursor) {
                Some((c, offset)) if offset < c.size() => (c, offset),
                Some((c, _)) => {
                    let prev_c = c.cluster();
                    match self.file.root.fat().read(prev_c)?.chain() {
                        Some(c) => (self.file.root.cluster(c), 0), // recycle
                        None => {
                            let c = self.file.root.fat().allocate()?;
                            self.file.root.fat().write(prev_c, c.into())?; // FAT[prev_c] -> c
                            (self.file.root.cluster(c), 0)
                        }
                    }
                }
                None => match self.file.cluster() {
                    Some(c) => (self.file.root.cluster(c), 0), // recycle
                    None => {
                        let c = self.file.root.fat().allocate()?;
                        let _ = self.file.set_cluster(Some(c)); // file.cluster -> c
                        (self.file.root.cluster(c), 0)
                    }
                },
            };
            let l = buf.len().min(c.size() - offset);
            c.write(offset, &buf[0..l])?;
            buf = &buf[l..];
            self.total_size += l;
            self.cursor = Some((c, offset + l));
        }
        Ok(())
    }
}

impl<'a, V: Volume> Drop for FileWriter<'a, V> {
    fn drop(&mut self) {
        match self.cursor {
            Some((ref c, _)) => {
                let last_c = c.cluster();
                if let Ok(FatEntry::UsedChained(c)) = self.file.root.fat().read(last_c) {
                    let _ = self.file.root.fat().write(last_c, FatEntry::UsedEoc); // FAT[last_c] -x-> c
                    let _ = self.file.root.fat().release(c);
                }
            }
            None => {
                if let Some(c) = self.file.cluster() {
                    let _ = self.file.set_cluster(None); // file.cluster -x-> c
                    let _ = self.file.root.fat().release(c);
                }
            }
        }
        let _ = self.file.set_file_size(self.total_size); // TODO: Handle error
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
