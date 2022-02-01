//! FAT File System implementation.

use super::volume::{Sector, Volume, VolumeError};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use dir_entry::{DirEntry, LfnReader, ReadLfnResult, SfnEntry};
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
    FileAlreadyExists,
    InvalidFileName,
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
            Self::FileAlreadyExists => write!(f, "File with the same name already exists"),
            Self::InvalidFileName => write!(f, "Invalid file name"),
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
            dir: self.cluster,
            inner: self.root.dir_entries(self.cluster),
        }
    }

    fn insert_dir_entries(
        &self,
        entries: impl ExactSizeIterator<Item = DirEntry>,
    ) -> Result<(), Error> {
        let required_len = entries.len();
        if required_len == 0 {
            return Ok(());
        }
        let mut writable_start = (self.cluster, 0);
        let mut writable_len = 0;
        for (c, n, entry) in self.root.dir_entries(self.cluster) {
            match entry {
                DirEntry::Unused => {
                    if writable_len == 0 {
                        writable_start = (c, n);
                    }
                    writable_len += 1;
                    if writable_len == required_len {
                        // Found a enough writable space (starting at writable_start) for entries
                        break;
                    }
                }
                DirEntry::UnusedTerminal => {
                    if writable_len == 0 {
                        writable_start = (c, n);
                    }
                    // We don't have a enough space (writable_len != required_len) for entries
                    break;
                }
                _ => writable_len = 0,
            }
        }
        let terminal = (writable_len != required_len).then(|| DirEntry::UnusedTerminal);
        let (c, mut n) = writable_start;
        let mut c = self.root.cluster(c);
        for entry in entries.chain(terminal) {
            if c.dir_entries_count() <= n {
                c = self.root.chained_cluster(c.cluster()).prepare()?;
                n = 0;
            }
            c.write_dir_entry(n, entry)?;
            n += 1;
        }
        Ok(())
    }

    // TODO: create_file(..), create_dir(..)
}

#[derive(Debug)]
pub struct DirIter<'a, V> {
    root: &'a Root<V>,
    dir: Cluster,
    inner: DirEntries<'a, V>,
}

impl<'a, V: Volume> Iterator for DirIter<'a, V> {
    type Item = File<'a, V>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut reader = LfnReader::Init;
        let (mut sc, mut sn, mut entry) = self.inner.next()?;
        let (mut ec, mut en) = (sc, sn);
        let (name, sfn) = loop {
            match reader.read(entry) {
                ReadLfnResult::Meta(DirEntry::UnusedTerminal) => return None,
                ReadLfnResult::Meta(_) => return self.next(),
                ReadLfnResult::Incomplete => (ec, en, entry) = self.inner.next()?,
                ReadLfnResult::Complete(name, sfn) => break (name, sfn),
                ReadLfnResult::Broken(_, e) => (sc, sn, entry) = (ec, en, e),
            }
        };
        Some(File {
            root: self.root,
            dir: self.dir,
            name,
            entry_location: (sc, sn),
            last_entry: (sfn, ec, en),
        })
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

    fn dir(&self) -> Dir<'a, V> {
        Dir {
            root: self.root,
            cluster: self.dir,
        }
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

    // cluster, prepare_cluster, and release_cluster correspond to low_level::ChainedCluster methods

    fn cluster(&self) -> Option<BufferedCluster<'a, V>> {
        self.last_entry.0.cluster().map(|c| self.root.cluster(c))
    }

    fn prepare_cluster(&mut self) -> Result<BufferedCluster<'a, V>, Error> {
        match self.last_entry.0.cluster() {
            Some(c) => Ok(self.root.cluster(c)),
            None => {
                let c = self.root.fat().allocate()?;
                self.last_entry.0.set_cluster(Some(c));
                self.write_back()?;
                Ok(self.root.cluster(c))
            }
        }
    }

    fn release_cluster(&mut self) -> Result<(), Error> {
        if let Some(c) = self.last_entry.0.cluster() {
            self.last_entry.0.set_cluster(None);
            self.write_back()?;
            self.root.fat().release(c)?;
        }
        Ok(())
    }

    pub fn reader(&self) -> Option<FileReader<V>> {
        if self.is_dir() {
            None
        } else {
            Some(FileReader {
                root: self.root,
                rest_size: self.file_size(),
                cursor: self.cluster().map(|c| (c, 0)),
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
            // Same as overwriter except the cursor is at the end of self.cluster()
            let mut total_size = 0;
            let cursor = self.cluster().map(|mut c| {
                let mut rest_size = self.file_size();
                while c.size() < rest_size {
                    match self.root.chained_cluster(c.cluster()).get() {
                        Ok(Some(next_c)) => {
                            total_size += c.size();
                            rest_size -= c.size();
                            c = next_c;
                        }
                        _ => rest_size = c.size(), // FIXME: How should we handle the broken cluster chain?
                    }
                }
                total_size += rest_size;
                (c, rest_size)
            });
            Some(FileWriter {
                file: self,
                total_size,
                cursor,
            })
        }
    }

    fn dir_entry_locations(
        &self,
    ) -> impl Iterator<Item = (BufferedCluster<'a, V>, usize, usize)> + 'a {
        let (start_c, start_offset) = self.entry_location;
        let (_, end_c, end_offset) = self.last_entry;
        let mut next_c = Some(self.root.cluster(start_c));
        let root = self.root;
        core::iter::from_fn(move || {
            let c = core::mem::take(&mut next_c)?;
            let i = match c.cluster() == start_c {
                true => start_offset,
                false => 0,
            };
            let j = match c.cluster() == end_c {
                true => end_offset,
                false => c.dir_entries_count() - 1,
            };
            next_c = if c.cluster() == end_c {
                None
            } else {
                root.chained_cluster(c.cluster()).get().ok().flatten()
            };
            Some((c, i, j))
        })
    }

    pub fn remove(mut self, recursive: bool) -> Result<(), Error> {
        if let Some(dir) = self.as_dir() {
            for file in dir.files().filter(|f| !matches!(f.name(), "." | "..")) {
                if recursive {
                    file.remove(true)?;
                } else {
                    Err(Error::DirectoryNotEmpty)?;
                }
            }
        }
        self.release_cluster()?;

        for (mut c, i, j) in self.dir_entry_locations() {
            for offset in i..=j {
                c.write_dir_entry(offset, DirEntry::Unused)?;
            }
        }
        Ok(())
    }

    pub fn mv(self, dir: Option<Dir<'a, V>>, name: Option<&str>) -> Result<(), Error> {
        let (name, dir, entries) = match name {
            Some(name) if name != self.name => {
                let dir = dir.unwrap_or_else(|| self.dir());
                let entries = DirEntry::lfn_sequence(name, self.last_entry.0)
                    .ok_or(Error::InvalidFileName)?;
                (name, dir, entries)
            }
            _ => {
                let dir = match dir {
                    Some(dir) if dir.cluster != self.dir => dir,
                    _ => return Ok(()),
                };
                // Since there is no name change, just move the DirEntry sequence
                let entries = self
                    .dir_entry_locations()
                    .flat_map(|(mut c, i, j)| (i..=j).map(move |offset| c.read_dir_entry(offset)))
                    .collect::<Result<Vec<_>, _>>()?;
                (self.name.as_str(), dir, entries)
            }
        };
        // FIXME: We also need to check SFN name conflict
        if dir.files().any(|f| f.name() == name) {
            Err(Error::FileAlreadyExists)?;
        }
        for (mut c, i, j) in self.dir_entry_locations() {
            for offset in i..=j {
                c.write_dir_entry(offset, DirEntry::Unused)?;
            }
        }
        dir.insert_dir_entries(entries.into_iter())
    }
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

            self.cursor = if l == c.size() - offset {
                self.root
                    .chained_cluster(c.cluster())
                    .get()?
                    .map(|c| (c, 0))
            } else {
                Some((c, offset + l))
            };
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
                Some((c, _)) => (self.file.root.chained_cluster(c.cluster()).prepare()?, 0),
                None => (self.file.prepare_cluster()?, 0),
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
        let _ = match self.cursor {
            Some((ref c, _)) => self.file.root.chained_cluster(c.cluster()).release(),
            None => self.file.release_cluster(),
        };
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
