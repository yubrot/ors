//! FAT File System implementation (work in progress)

use super::volume::{Volume, VolumeError};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use log::trace;

// TODO:
// * FAT12/16 Support
// * FSINFO support
// * Reduce Volume I/O
// * ...
// * Mark as unsafe?

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Error {
    Volume(VolumeError),
    BootSignatureMismatch,
    BrokenBootSector(&'static str),
    Unsupported(&'static str),
}

impl From<VolumeError> for Error {
    fn from(e: VolumeError) -> Self {
        Self::Volume(e)
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

        volume.read(0, buf.as_mut())?;
        let bs = BootSector::try_from(buf.as_ref())?;

        if bs.sector_size() != sector_size {
            Err(Error::BrokenBootSector("BytsPerSec (mismatch)"))?;
        }
        if volume.sector_count() < bs.total_sector_count() as usize {
            Err(Error::BrokenBootSector("TotSec (mismatch)"))?;
        }

        Ok(Self { volume, bs })
    }

    pub fn root_dir(&self) -> Dir<V> {
        Dir {
            fs: self,
            cluster: self.bs.bpb_root_clus,
        }
    }

    fn read_fat_entry(&self, n: u32, buf: &mut [u8]) -> Result<FatEntry, Error> {
        debug_assert!(self.bs.sector_size() <= buf.len());
        let (sector, offset) = self.bs.fat_entry_location(n);
        self.volume
            .read(sector as usize, &mut buf[0..self.bs.sector_size()])?;
        Ok(FatEntry::from(u32::from_le_bytes(
            buf.fixed_slice::<4>(offset as usize),
        )))
    }

    fn read_data(&self, n: u32, buf: &mut [u8]) -> Result<(), Error> {
        debug_assert_eq!(self.bs.cluster_size_in_bytes(), buf.len());
        let sector = self.bs.data_location(n);
        Ok(self.volume.read(sector as usize, buf)?)
    }
}

#[derive(Debug)]
pub struct Dir<'a, V> {
    fs: &'a FileSystem<V>,
    cluster: u32,
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
    type Item = (u32, usize, RawDirEntry);

    fn next(&mut self) -> Option<Self::Item> {
        match core::mem::replace(&mut self.cursor, RawDirCursor::End) {
            RawDirCursor::Init(cluster) => {
                let buf = vec![0; self.fs.bs.cluster_size_in_bytes()];
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
                    Ok(FatEntry::Used {
                        next: Some(cluster),
                    }) => {
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
    Init(u32),
    Start(u32, Vec<u8>),
    Mid(u32, Vec<u8>, usize),
    End,
}

/// Deserialized boot sector structure.
///
/// `bpb_` means that it is part of the BIOS parameter block.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
struct BootSector {
    /// Jump instruction to the bootstrap code. usually 0xEB??90 | 0xE9????
    _jmp_boot: [u8; 3],
    /// Formatter's name. usually MSWIN4.1
    _oem_name: [u8; 8],
    /// Sector size in bytes. It must be same as the volume sector size. 512 | 1024 | 2048 | 4096
    bpb_byts_per_sec: u16,
    /// Cluster size in sectors. It must be power of two. Cluster is an allocation unit of FAT and consists of contiguous sectors.
    bpb_sec_per_clus: u8,
    /// Number of sectors of reserved area. It must not be 0 since it includes this boot sector.
    bpb_rsvd_sec_cnt: u16,
    /// Number of FAT copies. It should be 2.
    bpb_num_fats: u8,
    /// Number of directory entries in the root directory. unused for FAT32.
    _bpb_root_ent_cnt: u16,
    /// Total number of sectors for FAT12/16. 0 for FAT32.
    _bpb_tot_sec_16: u16,
    /// Media type. ignored
    _bpb_media: u8,
    /// FAT size in sectors for FAT12/FAT16. 0 for FAT32.
    _bpb_fat_sz_16: u16,
    /// Track size in sectors. ignored
    _bpb_sec_per_trk: u16,
    /// Number of heads. ignored
    _bpb_num_heads: u16,
    /// Number of hidden sectors before this volume.
    _bpb_hidd_sec: u32,
    /// Total number of sectors for FAT32. 0 for FAT12/16.
    bpb_tot_sec_32: u32,
    // FAT32 only fields ------
    /// FAT size in sectors for FAT32.
    bpb_fat_sz_32: u32,
    _bpb_ext_flags: u16,
    /// File system version. It must be 0x0000.
    _bpb_fs_ver: u16,
    /// Cluster number of the root directory.
    bpb_root_clus: u32,
    /// Sector number of the FSINFO. It must be 1.
    _bpb_fs_info: u16,
    /// Sector number where the boot sector backup is placed. 6 is recommended
    bpb_bk_boot_sec: u16,
    _bpb_reserved: [u8; 12],
    // ------
    /// Drive Number. ignored
    _drv_num: u8,
    _reserved: u8,
    /// Extended boot signature. It must be 0x29.
    _boot_sig: u8,
    /// Volume ID. usually format datetime.
    vol_id: u32,
    /// Volume Label.
    vol_lab: [u8; 11],
    /// FS type name. This field is not used to determine the FAT type.
    _fil_sys_type: [u8; 8],
    // boot_code
    // boot_sign
}

impl BootSector {
    /// Sector size in bytes.
    fn sector_size(&self) -> usize {
        self.bpb_byts_per_sec as usize
    }

    /// Total number of sectors.
    fn total_sector_count(&self) -> u32 {
        debug_assert_eq!(self._bpb_tot_sec_16, 0);
        self.bpb_tot_sec_32
    }

    /// Cluster size in sectors.
    fn cluster_size(&self) -> u32 {
        self.bpb_sec_per_clus as u32
    }

    /// Cluster size in bytes.
    fn cluster_size_in_bytes(&self) -> usize {
        self.sector_size() * self.cluster_size() as usize
    }

    /// FAT size in sectors.
    fn fat_size(&self) -> u32 {
        debug_assert_eq!(self._bpb_fat_sz_16, 0);
        self.bpb_fat_sz_32
    }

    // A FAT volume consists of
    // Reserved area | FAT area | Root dir area (for FAT12/16) | Data area

    /// Fat area start sector.
    fn fat_area_start(&self) -> u32 {
        self.bpb_rsvd_sec_cnt as u32
    }

    /// FAT area size in sectors.
    fn fat_area_size(&self) -> u32 {
        self.fat_size() * self.bpb_num_fats as u32
    }

    /// Root dir area start sector.
    fn root_dir_area_start(&self) -> u32 {
        self.fat_area_start() + self.fat_area_size()
    }

    /// Root dir area size in sectors.
    fn root_dir_area_size(&self) -> u32 {
        debug_assert_eq!(self._bpb_root_ent_cnt, 0);
        0
        // (DirEntry::SIZE as u32 * self._bpb_root_ent_cnt as u32 + self.bpb_byts_per_sec as u32 - 1)
        //     / self.bpb_byts_per_sec as u32
    }

    /// Data area start sector.
    fn data_area_start(&self) -> u32 {
        self.root_dir_area_start() + self.root_dir_area_size()
    }

    /// Data area size in sectors.
    fn data_area_size(&self) -> u32 {
        self.total_sector_count() - self.data_area_start()
    }

    /// Get the location of the FAT entry corresponding to the given cluster number, in sectors and byte-offset.
    ///
    /// In FAT32, FAT is an array of 32-bit FAT entries.
    /// Each FAT entry has a 1:1 correspondence with each cluster,
    /// and the value of the FAT entry indicates the status of the corresponding cluster.
    /// Notice that FAT[0] and FAT[1] are reserved, and correspondingly, the valid cluster number is also 2-origin.
    /// It should also be noted that in FAT32, the upper 4 bits of the FAT entry are reserved.
    fn fat_entry_location(&self, n: u32) -> (u32, u32) {
        let sector = self.fat_area_start() + (n * 4 / self.cluster_size());
        let offset = (n * 4) % self.cluster_size();
        (sector, offset)
    }

    /// Get the location of the data corresponding to the given cluster number, in sectors.
    fn data_location(&self, n: u32) -> u32 {
        debug_assert!(2 <= n, "Cluster number is 2-origin");
        self.data_area_start() + (n - 2) * self.cluster_size()
    }
}

impl TryFrom<&'_ [u8]> for BootSector {
    type Error = Error;

    fn try_from(buf: &'_ [u8]) -> Result<Self, Self::Error> {
        if buf.len() < 512 || !matches!(buf[510..512], [0x55, 0xaa]) {
            Err(Error::BootSignatureMismatch)?;
        }

        let _jmp_boot = buf.fixed_slice::<3>(0);
        let _oem_name = buf.fixed_slice::<8>(3);
        let bpb_byts_per_sec = u16::from_le_bytes(buf.fixed_slice::<2>(11));
        let bpb_sec_per_clus = buf[13];
        let bpb_rsvd_sec_cnt = u16::from_le_bytes(buf.fixed_slice::<2>(14));
        let bpb_num_fats = buf[16];
        let _bpb_root_ent_cnt = u16::from_le_bytes(buf.fixed_slice::<2>(17));
        let _bpb_tot_sec_16 = u16::from_le_bytes(buf.fixed_slice::<2>(19));
        let _bpb_media = buf[21];
        let _bpb_fat_sz_16 = u16::from_le_bytes(buf.fixed_slice::<2>(22));
        let _bpb_sec_per_trk = u16::from_le_bytes(buf.fixed_slice::<2>(24));
        let _bpb_num_heads = u16::from_le_bytes(buf.fixed_slice::<2>(26));
        let _bpb_hidd_sec = u32::from_le_bytes(buf.fixed_slice::<4>(28));
        let bpb_tot_sec_32 = u32::from_le_bytes(buf.fixed_slice::<4>(32));

        if !matches!(_jmp_boot, [0xeb, _, 0x90] | [0xe9, _, _]) {
            Err(Error::BrokenBootSector("JmpBoot"))?;
        }
        if !matches!(bpb_byts_per_sec, 512 | 1024 | 2048 | 4096) {
            Err(Error::BrokenBootSector("BytsPerSec"))?;
        }
        if !bpb_sec_per_clus.is_power_of_two() {
            Err(Error::BrokenBootSector("SecPerClus"))?;
        }
        if _bpb_root_ent_cnt != 0 || _bpb_tot_sec_16 != 0 || _bpb_fat_sz_16 != 0 {
            Err(Error::Unsupported("FAT12/16"))?;
        }

        let bpb_fat_sz_32 = u32::from_le_bytes(buf.fixed_slice::<4>(36));
        let _bpb_ext_flags = u16::from_le_bytes(buf.fixed_slice::<2>(40));
        let _bpb_fs_ver = u16::from_le_bytes(buf.fixed_slice::<2>(42));
        let bpb_root_clus = u32::from_le_bytes(buf.fixed_slice::<4>(44));
        let _bpb_fs_info = u16::from_le_bytes(buf.fixed_slice::<2>(48));
        let bpb_bk_boot_sec = u16::from_le_bytes(buf.fixed_slice::<2>(50));
        let _bpb_reserved = buf.fixed_slice::<12>(52);
        let _drv_num = buf[64];
        let _reserved = buf[65];
        let _boot_sig = buf[66];
        let vol_id = u32::from_le_bytes(buf.fixed_slice::<4>(67));
        let vol_lab = buf.fixed_slice::<11>(71);
        let _fil_sys_type = buf.fixed_slice::<8>(82);

        if _bpb_fs_ver != 0x0000 {
            Err(Error::Unsupported("FSVer"))?;
        }
        if _bpb_fs_info != 1 {
            Err(Error::BrokenBootSector("FSInfo"))?;
        }
        if _boot_sig != 0x29 {
            Err(Error::BrokenBootSector("BootSig"))?;
        }

        Ok(Self {
            _jmp_boot,
            _oem_name,
            bpb_byts_per_sec,
            bpb_sec_per_clus,
            bpb_rsvd_sec_cnt,
            bpb_num_fats,
            _bpb_root_ent_cnt,
            _bpb_tot_sec_16,
            _bpb_media,
            _bpb_fat_sz_16,
            _bpb_sec_per_trk,
            _bpb_num_heads,
            _bpb_hidd_sec,
            bpb_tot_sec_32,
            bpb_fat_sz_32,
            _bpb_ext_flags,
            _bpb_fs_ver,
            bpb_root_clus,
            _bpb_fs_info,
            bpb_bk_boot_sec,
            _bpb_reserved,
            _drv_num,
            _reserved,
            _boot_sig,
            vol_id,
            vol_lab,
            _fil_sys_type,
        })
    }
}

/// Deserialized FAT entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum FatEntry {
    Unused,
    Reserved,
    Used { next: Option<u32> },
    Bad,
}

impl From<u32> for FatEntry {
    fn from(value: u32) -> Self {
        match value & 0x0fffffff {
            0 => Self::Unused,
            1 => Self::Reserved,
            0x00000002..=0x0ffffff6 => Self::Used { next: Some(value) },
            0x0ffffff7 => Self::Bad,
            0x0ffffff8..=0x0fffffff => Self::Used { next: None },
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

        let name = buf.fixed_slice::<11>(0);
        let attr = buf[11];
        let nt_res = buf[12];
        let crt_time_tenth = buf[13];
        let crt_time = u16::from_le_bytes(buf.fixed_slice::<2>(14));
        let crt_date = u16::from_le_bytes(buf.fixed_slice::<2>(16));
        let lst_acc_date = u16::from_le_bytes(buf.fixed_slice::<2>(18));
        let fst_clus_hi = u16::from_le_bytes(buf.fixed_slice::<2>(20));
        let wrt_time = u16::from_le_bytes(buf.fixed_slice::<2>(22));
        let wrt_date = u16::from_le_bytes(buf.fixed_slice::<2>(24));
        let fst_clus_lo = u16::from_le_bytes(buf.fixed_slice::<2>(26));
        let file_size = u32::from_le_bytes(buf.fixed_slice::<4>(28));

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
                0..=4 => self.name1.fixed_slice::<2>(i * 2),
                5..=10 => self.name2.fixed_slice::<2>((i - 5) * 2),
                11..=12 => self.name3.fixed_slice::<2>((i - 11) * 2),
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
        let name1 = buf.fixed_slice::<10>(1);
        let attr = buf[11];
        let ty = buf[12];
        let chksum = buf[13];
        let name2 = buf.fixed_slice::<12>(14);
        let _fst_clus_lo = u16::from_le_bytes(buf.fixed_slice::<2>(26));
        let name3 = buf.fixed_slice::<4>(28);

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

trait SliceExt {
    fn fixed_slice<const N: usize>(&self, offset: usize) -> [u8; N];
}

impl SliceExt for [u8] {
    fn fixed_slice<const N: usize>(&self, offset: usize) -> [u8; N] {
        let mut ret = [0; N];
        ret.copy_from_slice(&self[offset..offset + N]);
        ret
    }
}
