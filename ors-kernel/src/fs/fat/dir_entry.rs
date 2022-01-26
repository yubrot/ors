use super::{Cluster, SliceExt};
use alloc::string::String;

/// Deserialized Directory entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(super) enum DirEntry {
    UnusedTerminal,
    Unused,
    Lfn(LfnEntry),
    Sfn(SfnEntry),
}

impl DirEntry {
    pub(super) const SIZE: usize = 32;

    const READ_ONLY: u8 = 0x01;
    const HIDDEN: u8 = 0x02;
    const SYSTEM: u8 = 0x04;
    const VOLUME_ID: u8 = 0x08;
    const DIRECTORY: u8 = 0x10;
    const ARCHIVE: u8 = 0x20;
    const LONG_FILE_NAME: u8 = 0x0f;
    const LONG_FILE_NAME_MASK: u8 = 0x3f;
}

impl From<[u8; 32]> for DirEntry {
    fn from(buf: [u8; 32]) -> Self {
        let status = buf[0];
        let attr = buf[11];

        if status == 0xe5 {
            Self::Unused
        } else if status == 0x00 {
            Self::UnusedTerminal
        } else if (attr & Self::LONG_FILE_NAME_MASK) == Self::LONG_FILE_NAME {
            Self::Lfn(buf.try_into().expect("LFN"))
        } else {
            Self::Sfn(buf.try_into().expect("SFN"))
        }
    }
}

impl Into<[u8; 32]> for DirEntry {
    fn into(self) -> [u8; 32] {
        match self {
            DirEntry::UnusedTerminal => [0; 32],
            DirEntry::Unused => {
                let mut buf = [0; 32];
                buf[0] = 0xe5;
                buf
            }
            DirEntry::Lfn(lfn) => lfn.into(),
            DirEntry::Sfn(sfn) => sfn.into(),
        }
    }
}

/// Deserialized Short File Name entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(super) struct SfnEntry {
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
    pub(super) fn name(&self) -> (bool, String) {
        let mut is_irreversible = false;
        let mut dest = String::with_capacity(12);
        let mut put = |seq: &[u8], is_lower: bool| {
            for c in seq {
                dest.push(match *c {
                    32 => break,
                    65..=90 if is_lower => (*c + 32) as char,
                    33..=126 => *c as char,
                    _ => {
                        // NOTE: This erases both 0xe5 and 0x05
                        is_irreversible = true;
                        '\u{fffd}'
                    }
                });
            }
        };
        put(&self.name[0..8], (self.nt_res & 0x08) == 0x08);
        if self.name[8] != 32 {
            put(&['.' as u8], false);
        }
        put(&self.name[8..11], (self.nt_res & 0x10) == 0x10);
        (is_irreversible, dest)
    }

    pub(super) fn cluster(&self) -> Option<Cluster> {
        let index = self.fst_clus_lo as usize | ((self.fst_clus_hi as usize) << 16);
        (index != 0).then(|| Cluster::from_index(index))
    }

    pub(super) fn is_read_only(&self) -> bool {
        (self.attr & DirEntry::READ_ONLY) == DirEntry::READ_ONLY
    }

    pub(super) fn is_hidden(&self) -> bool {
        (self.attr & DirEntry::HIDDEN) == DirEntry::HIDDEN
    }

    pub(super) fn is_system(&self) -> bool {
        (self.attr & DirEntry::SYSTEM) == DirEntry::SYSTEM
    }

    pub(super) fn is_volume_id(&self) -> bool {
        (self.attr & DirEntry::VOLUME_ID) == DirEntry::VOLUME_ID
    }

    pub(super) fn is_directory(&self) -> bool {
        (self.attr & DirEntry::DIRECTORY) == DirEntry::DIRECTORY
    }

    pub(super) fn archive(&self) -> bool {
        (self.attr & DirEntry::ARCHIVE) == DirEntry::ARCHIVE
    }

    pub(super) fn mark_archive(&mut self) {
        self.attr |= DirEntry::ARCHIVE;
    }

    pub fn checksum(&self) -> u8 {
        self.name.iter().fold(0u8, |sum, c| {
            (sum >> 1).wrapping_add(sum << 7).wrapping_add(*c)
        })
    }
}

impl TryFrom<[u8; 32]> for SfnEntry {
    type Error = &'static str;

    fn try_from(buf: [u8; 32]) -> Result<Self, Self::Error> {
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

        if (attr & DirEntry::LONG_FILE_NAME_MASK) == DirEntry::LONG_FILE_NAME {
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

impl Into<[u8; 32]> for SfnEntry {
    fn into(self) -> [u8; 32] {
        let mut buf = [0; 32];
        buf.copy_from_array(0, self.name);
        buf[11] = self.attr;
        buf[12] = self.nt_res;
        buf[13] = self.crt_time_tenth;
        buf.copy_from_array(14, self.crt_time.to_le_bytes());
        buf.copy_from_array(16, self.crt_date.to_le_bytes());
        buf.copy_from_array(18, self.lst_acc_date.to_le_bytes());
        buf.copy_from_array(20, self.fst_clus_hi.to_le_bytes());
        buf.copy_from_array(22, self.wrt_time.to_le_bytes());
        buf.copy_from_array(24, self.wrt_date.to_le_bytes());
        buf.copy_from_array(26, self.fst_clus_lo.to_le_bytes());
        buf.copy_from_array(28, self.file_size.to_le_bytes());
        buf
    }
}

/// Deserialized Long File Name entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(super) struct LfnEntry {
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

    pub(super) fn put_name_parts_into(&self, buf: &mut [u16]) {
        for i in 0..13 {
            buf[i] = u16::from_le_bytes(match i {
                0..=4 => self.name1.array::<2>(i * 2),
                5..=10 => self.name2.array::<2>((i - 5) * 2),
                11..=12 => self.name3.array::<2>((i - 11) * 2),
                _ => unreachable!(),
            });
        }
    }

    pub(super) fn is_last_entry(&self) -> bool {
        (self.ord & Self::LAST_LONG_ENTRY) == Self::LAST_LONG_ENTRY
            && matches!(self.ord & !Self::LAST_LONG_ENTRY, 1..=20)
    }

    pub(super) fn order(&self) -> usize {
        (self.ord & !Self::LAST_LONG_ENTRY) as usize
    }

    pub(super) fn checksum(&self) -> u8 {
        self.chksum
    }
}

impl TryFrom<[u8; 32]> for LfnEntry {
    type Error = &'static str;

    fn try_from(buf: [u8; 32]) -> Result<Self, Self::Error> {
        let ord = buf[0];
        let name1 = buf.array::<10>(1);
        let attr = buf[11];
        let ty = buf[12];
        let chksum = buf[13];
        let name2 = buf.array::<12>(14);
        let _fst_clus_lo = u16::from_le_bytes(buf.array::<2>(26));
        let name3 = buf.array::<4>(28);

        if (attr & DirEntry::LONG_FILE_NAME_MASK) != DirEntry::LONG_FILE_NAME {
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

impl Into<[u8; 32]> for LfnEntry {
    fn into(self) -> [u8; 32] {
        let mut buf = [0; 32];
        buf[0] = self.ord;
        buf.copy_from_array(1, self.name1);
        buf[11] = self.attr;
        buf[12] = self.ty;
        buf[13] = self.chksum;
        buf.copy_from_array(14, self.name2);
        buf.copy_from_array(28, self.name3);
        buf
    }
}
