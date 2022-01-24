//! FAT File System implementation (work in progress)

use super::volume::{Sector, Volume, VolumeError};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use log::trace;

mod boot_sector;

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

impl<'a, V: Volume> Iterator for DirIter<'a, V> {
    type Item = DirEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let (_, _, entry) = self.inner.next()?;
        match entry {
            RawDirEntry::UnusedTerminal => None,
            RawDirEntry::Unused => self.next(),
            RawDirEntry::VolumeId(_) => self.next(),
            RawDirEntry::Lfn(lfn)
                if (lfn.ord & LfnEntry::LAST_LONG_ENTRY) == LfnEntry::LAST_LONG_ENTRY
                    && matches!(lfn.ord & !LfnEntry::LAST_LONG_ENTRY, 1..=20) =>
            {
                let mut num_lfn = (lfn.ord & !LfnEntry::LAST_LONG_ENTRY) as usize;
                let mut name_buf = vec![0; num_lfn * 13];

                num_lfn -= 1;
                lfn.put_name_parts_into(&mut name_buf[num_lfn * 13..]);
                let checksum = lfn.chksum;

                // Attempt to read `num_lfn` LFN entries and a normal directory entry.
                // NOTE: In this implementation, broken LFNs are basically ignored. When such an LFN
                // entry is encountered, the next (possibly unbroken) LFN entry may also be ignored.

                // `num_lfn` LFN entries:
                while num_lfn != 0 {
                    let (_, _, entry) = self.inner.next()?;
                    match entry {
                        RawDirEntry::Lfn(lfn) if lfn.chksum == checksum => {
                            num_lfn -= 1;
                            lfn.put_name_parts_into(&mut name_buf[num_lfn * 13..]);
                        }
                        // failback
                        RawDirEntry::Normal(sfn) => return Some(sfn.into()),
                        _ => return self.next(),
                    }
                }

                // a normal directory entry:
                let (_, _, entry) = self.inner.next()?;
                match entry {
                    RawDirEntry::Normal(sfn) if sfn.checksum() == checksum => {
                        // LFN is 0x0000-terminated and padded with 0xffff
                        while matches!(name_buf.last(), Some(0xffff)) {
                            name_buf.pop();
                        }
                        if name_buf.pop() == Some(0x0000) {
                            let name = String::from_utf16_lossy(name_buf.as_slice());
                            Some(DirEntry { name, sfn })
                        } else {
                            // failback
                            Some(sfn.into())
                        }
                    }
                    // failback
                    RawDirEntry::Normal(sfn) => Some(sfn.into()),
                    _ => self.next(),
                }
            }
            RawDirEntry::Lfn(_) => {
                // failback
                self.next()
            }
            RawDirEntry::Normal(sfn) => Some(sfn.into()),
        }
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
                match RawDirEntry::try_from(&buf[offset..offset + RawDirEntry::SIZE]) {
                    Ok(entry) => {
                        if !matches!(entry, RawDirEntry::UnusedTerminal) {
                            self.cursor =
                                RawDirCursor::Mid(cluster, buf, offset + RawDirEntry::SIZE);
                        }
                        Some((cluster, offset, entry))
                    }
                    Err(e) => {
                        trace!(
                            "Broken DirEntry at cluster={} offset={}: {}",
                            cluster,
                            offset,
                            e
                        );
                        None
                    }
                }
            }
            RawDirCursor::Mid(cluster, mut buf, _) => {
                match self.fs.read_fat_entry(cluster, buf.as_mut()) {
                    Ok(FatEntry::Used(Some(cluster))) => {
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

/// Deserialized FAT entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum FatEntry {
    Unused,
    Reserved,
    Used(Option<Cluster>),
    Bad,
}

impl From<u32> for FatEntry {
    fn from(value: u32) -> Self {
        match value & 0x0fffffff {
            0 => Self::Unused,
            1 => Self::Reserved,
            0x00000002..=0x0ffffff6 => Self::Used(Some(Cluster::from_index(value as usize))),
            0x0ffffff7 => Self::Bad,
            0x0ffffff8..=0x0fffffff => Self::Used(None),
            0x10000000..=0xffffffff => unreachable!(),
        }
    }
}

/// Deserialized Directory entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum RawDirEntry {
    UnusedTerminal,
    Unused,
    VolumeId(SfnEntry),
    Lfn(LfnEntry),
    Normal(SfnEntry),
}

impl RawDirEntry {
    const SIZE: usize = 32;

    const READ_ONLY: u8 = 0x01;
    const HIDDEN: u8 = 0x02;
    const SYSTEM: u8 = 0x04;
    const VOLUME_ID: u8 = 0x08;
    const DIRECTORY: u8 = 0x10;
    const ARCHIVE: u8 = 0x20;
    const LONG_FILE_NAME: u8 = 0x0f;
    const LONG_FILE_NAME_MASK: u8 = 0x3f;
}

impl TryFrom<&'_ [u8]> for RawDirEntry {
    type Error = &'static str;

    fn try_from(buf: &'_ [u8]) -> Result<Self, Self::Error> {
        if buf.len() != Self::SIZE {
            Err("Directory entry must be 32 bytes long")?;
        }

        let status = buf[0];
        let attr = buf[11];

        if status == 0xe5 {
            Ok(Self::Unused)
        } else if status == 0x00 {
            Ok(Self::UnusedTerminal)
        } else if (attr & Self::LONG_FILE_NAME_MASK) == Self::LONG_FILE_NAME {
            Ok(Self::Lfn(buf.try_into()?))
        } else if (attr & Self::VOLUME_ID) == Self::VOLUME_ID {
            Ok(Self::VolumeId(buf.try_into()?))
        } else {
            Ok(Self::Normal(buf.try_into()?))
        }
    }
}

/// Deserialized Short File Name entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct SfnEntry {
    name: [u8; 11],
    attr: u8,
    nt_res: u8,
    crt_time_tenth: u8,
    crt_time: u16,
    crt_date: u16,
    lst_acc_date: u16,
    fst_clus_hi: u16,
    wrt_time: u16,
    wrt_date: u16,
    fst_clus_lo: u16,
    file_size: u32,
}

impl SfnEntry {
    fn name(&self) -> (bool, String) {
        let mut is_irreversible = false;
        let mut dest = String::with_capacity(12);
        let mut put = |seq: &[u8], is_lower: bool| {
            for c in seq {
                dest.push(match *c {
                    32 => break,
                    65..=90 if is_lower => (*c + 32) as char,
                    33..=126 => *c as char,
                    _ => {
                        // NOTE: This includes both 0xe5 and 0x05
                        is_irreversible = true;
                        '\u{fffd}'
                    }
                });
            }
        };
        put(&self.name[0..8], (self.nt_res & 0x08) == 0x08);
        put(&self.name[8..11], (self.nt_res & 0x10) == 0x10);
        (is_irreversible, dest)
    }

    fn checksum(&self) -> u8 {
        self.name.iter().fold(0u8, |sum, c| {
            (sum >> 1).wrapping_add(sum << 7).wrapping_add(*c)
        })
    }
}

impl TryFrom<&'_ [u8]> for SfnEntry {
    type Error = &'static str;

    fn try_from(buf: &'_ [u8]) -> Result<Self, Self::Error> {
        if buf.len() != RawDirEntry::SIZE {
            Err("Directory entry must be 32 bytes long")?;
        }

        let name = buf.array::<11>(0);
        let attr = buf[11];
        let nt_res = buf[12];
        let crt_time_tenth = buf[13];
        let crt_time = u16::from_le_bytes(buf.array::<2>(14));
        let crt_date = u16::from_le_bytes(buf.array::<2>(16));
        let lst_acc_date = u16::from_le_bytes(buf.array::<2>(18));
        let fst_clus_hi = u16::from_le_bytes(buf.array::<2>(20));
        let wrt_time = u16::from_le_bytes(buf.array::<2>(22));
        let wrt_date = u16::from_le_bytes(buf.array::<2>(24));
        let fst_clus_lo = u16::from_le_bytes(buf.array::<2>(26));
        let file_size = u32::from_le_bytes(buf.array::<4>(28));

        if (attr & 0xc0) != 0 {
            Err("Invalid Attr")?;
        }
        if (attr & RawDirEntry::LONG_FILE_NAME_MASK) == RawDirEntry::LONG_FILE_NAME {
            Err("LFN")?;
        }

        Ok(Self {
            name,
            attr,
            nt_res,
            crt_time_tenth,
            crt_time,
            crt_date,
            lst_acc_date,
            fst_clus_hi,
            wrt_time,
            wrt_date,
            fst_clus_lo,
            file_size,
        })
    }
}

/// Deserialized Long File Name entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct LfnEntry {
    ord: u8,
    name1: [u8; 10],
    attr: u8,
    ty: u8,
    chksum: u8,
    name2: [u8; 12],
    _fst_clus_lo: u16,
    name3: [u8; 4],
}

impl LfnEntry {
    const LAST_LONG_ENTRY: u8 = 0x40;

    fn put_name_parts_into(&self, buf: &mut [u16]) {
        for i in 0..13 {
            buf[i] = u16::from_le_bytes(match i {
                0..=4 => self.name1.array::<2>(i * 2),
                5..=10 => self.name2.array::<2>((i - 5) * 2),
                11..=12 => self.name3.array::<2>((i - 11) * 2),
                _ => unreachable!(),
            });
        }
    }
}

impl TryFrom<&'_ [u8]> for LfnEntry {
    type Error = &'static str;

    fn try_from(buf: &'_ [u8]) -> Result<Self, Self::Error> {
        if buf.len() != RawDirEntry::SIZE {
            Err("Directory entry must be 32 bytes long")?;
        }

        let ord = buf[0];
        let name1 = buf.array::<10>(1);
        let attr = buf[11];
        let ty = buf[12];
        let chksum = buf[13];
        let name2 = buf.array::<12>(14);
        let _fst_clus_lo = u16::from_le_bytes(buf.array::<2>(26));
        let name3 = buf.array::<4>(28);

        if (attr & 0xc0) != 0 {
            Err("Invalid Attr")?;
        }
        if (attr & RawDirEntry::LONG_FILE_NAME_MASK) != RawDirEntry::LONG_FILE_NAME {
            Err("SFN")?;
        }

        Ok(Self {
            ord,
            name1,
            attr,
            ty,
            chksum,
            name2,
            _fst_clus_lo,
            name3,
        })
    }
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
}

impl SliceExt for [u8] {
    fn array<const N: usize>(&self, offset: usize) -> [u8; N] {
        let mut ret = [0; N];
        ret.copy_from_slice(&self[offset..offset + N]);
        ret
    }
}
