use super::{Cluster, Sector, SliceExt};
use core::fmt;

/// Error while reading boot sector.
#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash)]
pub enum Error {
    SignatureMismatch,
    Broken(&'static str),
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::SignatureMismatch => write!(f, "Boot signature mismatch"),
            Error::Broken(s) => write!(f, "Broken boot sector: {}", s),
            Error::Unsupported(s) => write!(f, "Unsupported feature: {}", s),
        }
    }
}

/// Deserialized boot sector structure.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct BootSector {
    // `bpb_` means that it is part of the BIOS parameter block.
    /// Jump instruction to the bootstrap code. usually 0xEB??90 | 0xE9????
    _jmp_boot: [u8; 3],
    /// Formatter's name. usually MSWIN4.1
    _oem_name: [u8; 8],
    /// Sector size in bytes. It must be same as the volume sector size. 512 | 1024 | 2048 | 4096
    bpb_byts_per_sec: u16,
    /// Cluster size in sectors. It must be power of two. Cluster is an allocation unit of FAT data and consists of contiguous sectors.
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
    _bpb_bk_boot_sec: u16,
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
    pub fn volume_id(&self) -> u32 {
        self.vol_id
    }

    pub fn volume_label(&self) -> [u8; 11] {
        self.vol_lab
    }

    /// Sector size in bytes.
    pub fn sector_size(&self) -> usize {
        self.bpb_byts_per_sec as usize
    }

    /// Total number of sectors.
    pub fn total_sector_count(&self) -> usize {
        debug_assert_eq!(self._bpb_tot_sec_16, 0);
        self.bpb_tot_sec_32 as usize
    }

    /// FAT size in sectors.
    pub fn fat_size(&self) -> usize {
        debug_assert_eq!(self._bpb_fat_sz_16, 0);
        self.bpb_fat_sz_32 as usize
    }

    // A FAT volume consists of
    // Reserved area | FAT area | Root dir area (for FAT12/16) | Data area

    /// Fat area start sector.
    pub fn fat_area_start(&self) -> Sector {
        Sector::from_index(self.bpb_rsvd_sec_cnt as usize)
    }

    /// FAT area size in sectors.
    pub fn fat_area_size(&self) -> usize {
        self.fat_size() * self.bpb_num_fats as usize
    }

    /// Root dir area start sector.
    pub fn root_dir_area_start(&self) -> Sector {
        self.fat_area_start().offset(self.fat_area_size())
    }

    /// Root dir area size in sectors.
    pub fn root_dir_area_size(&self) -> usize {
        debug_assert_eq!(self._bpb_root_ent_cnt, 0);
        0
        // use super::dir_entry::DirEntry;
        // let sector_size = self.sector_size();
        // (DirEntry::SIZE * self._bpb_root_ent_cnt as usize + sector_size - 1) / sector_size
    }

    /// Data area start sector.
    pub fn data_area_start(&self) -> Sector {
        self.root_dir_area_start().offset(self.root_dir_area_size())
    }

    /// Data area size in sectors.
    pub fn data_area_size(&self) -> usize {
        self.total_sector_count() - self.data_area_start().index()
    }

    /// Cluster size in sectors.
    pub fn cluster_size(&self) -> usize {
        self.bpb_sec_per_clus as usize
    }

    /// Number of available clusters.
    pub fn cluster_count(&self) -> usize {
        self.data_area_size() / self.cluster_size()
    }

    pub(super) fn is_cluster_available(&self, n: Cluster) -> bool {
        // Cluster numbers start at 2, thus the maximum cluster number is `cluster_count() + 1`.
        2 <= n.index() && n.index() <= self.cluster_count() + 1
    }

    /// Get the location of the FAT entry corresponding to the given cluster number.
    ///
    /// In FAT32, FAT (File Allocation Table) is an array of 32-bit FAT entries.
    /// Each FAT entry has a 1:1 correspondence with each cluster,
    /// and the value of the FAT entry indicates the status of the corresponding cluster.
    /// Notice that FAT[0] and FAT[1] are reserved, and correspondingly, cluster numbers also start at 2.
    /// It should also be noted that in FAT32, the upper 4 bits of the FAT entry are reserved.
    pub(super) fn fat_entry_location(&self, n: Cluster) -> (Sector, usize) {
        debug_assert!(self.is_cluster_available(n));
        let bytes_offset = n.index() * 4; // 32-bit -> 4bytes
        let sector = self
            .fat_area_start()
            .offset(bytes_offset / self.sector_size());
        let offset = bytes_offset % self.sector_size();
        (sector, offset)
    }

    /// Get the location of the data corresponding to the given cluster number.
    pub(super) fn cluster_location(&self, n: Cluster) -> Sector {
        debug_assert!(self.is_cluster_available(n));
        self.data_area_start()
            .offset((n.index() - 2) * self.cluster_size())
    }

    pub(super) fn root_dir_cluster(&self) -> Cluster {
        Cluster::from_index(self.bpb_root_clus as usize)
    }
}

impl TryFrom<&'_ [u8]> for BootSector {
    type Error = Error;

    fn try_from(buf: &'_ [u8]) -> Result<Self, Self::Error> {
        if buf.len() < 512 || !matches!(buf[510..512], [0x55, 0xaa]) {
            Err(Error::SignatureMismatch)?;
        }

        let _jmp_boot = buf.array::<3>(0);
        let _oem_name = buf.array::<8>(3);
        let bpb_byts_per_sec = u16::from_le_bytes(buf.array::<2>(11));
        let bpb_sec_per_clus = buf[13];
        let bpb_rsvd_sec_cnt = u16::from_le_bytes(buf.array::<2>(14));
        let bpb_num_fats = buf[16];
        let _bpb_root_ent_cnt = u16::from_le_bytes(buf.array::<2>(17));
        let _bpb_tot_sec_16 = u16::from_le_bytes(buf.array::<2>(19));
        let _bpb_media = buf[21];
        let _bpb_fat_sz_16 = u16::from_le_bytes(buf.array::<2>(22));
        let _bpb_sec_per_trk = u16::from_le_bytes(buf.array::<2>(24));
        let _bpb_num_heads = u16::from_le_bytes(buf.array::<2>(26));
        let _bpb_hidd_sec = u32::from_le_bytes(buf.array::<4>(28));
        let bpb_tot_sec_32 = u32::from_le_bytes(buf.array::<4>(32));

        if !matches!(_jmp_boot, [0xeb, _, 0x90] | [0xe9, _, _]) {
            Err(Error::Broken("JmpBoot"))?;
        }
        if !matches!(bpb_byts_per_sec, 512 | 1024 | 2048 | 4096) {
            Err(Error::Broken("BytsPerSec"))?;
        }
        if !bpb_sec_per_clus.is_power_of_two() {
            Err(Error::Broken("SecPerClus"))?;
        }
        if bpb_rsvd_sec_cnt == 0 {
            Err(Error::Broken("RsvdSecCnt"))?;
        }
        if _bpb_root_ent_cnt != 0 || _bpb_tot_sec_16 != 0 || _bpb_fat_sz_16 != 0 {
            Err(Error::Unsupported("FAT12/16"))?;
        }

        let bpb_fat_sz_32 = u32::from_le_bytes(buf.array::<4>(36));
        let _bpb_ext_flags = u16::from_le_bytes(buf.array::<2>(40));
        let _bpb_fs_ver = u16::from_le_bytes(buf.array::<2>(42));
        let bpb_root_clus = u32::from_le_bytes(buf.array::<4>(44));
        let _bpb_fs_info = u16::from_le_bytes(buf.array::<2>(48));
        let _bpb_bk_boot_sec = u16::from_le_bytes(buf.array::<2>(50));
        let _bpb_reserved = buf.array::<12>(52);
        let _drv_num = buf[64];
        let _reserved = buf[65];
        let _boot_sig = buf[66];
        let vol_id = u32::from_le_bytes(buf.array::<4>(67));
        let vol_lab = buf.array::<11>(71);
        let _fil_sys_type = buf.array::<8>(82);

        if _bpb_fs_ver != 0x0000 {
            Err(Error::Unsupported("FSVer"))?;
        }
        if _bpb_fs_info != 1 {
            Err(Error::Broken("FSInfo"))?;
        }
        if _boot_sig != 0x29 {
            Err(Error::Broken("BootSig"))?;
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
            _bpb_bk_boot_sec,
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
