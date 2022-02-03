use super::{Cluster, SliceExt};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

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

    pub(super) fn lfn_sequence(name: &str, mut sfn: SfnEntry) -> Option<Vec<DirEntry>> {
        if sfn.set_or_generate_name(name) {
            Some(vec![Self::Sfn(sfn)])
        } else if name.chars().all(LfnEntry::is_lfn_compatible_char) {
            let mut buf = name.encode_utf16().collect::<Vec<_>>();
            if 255 < buf.len() {
                return None;
            }
            let padding = (13 - buf.len() % 13) % 13;
            for i in 0..padding {
                buf.push(if i == 0 { 0x0000 } else { 0xffff });
            }
            let checksum = sfn.checksum();
            let num_lfn = buf.len() / 13;
            let mut entries = vec![DirEntry::Unused; num_lfn + 1];
            entries[num_lfn] = DirEntry::Sfn(sfn);
            for i in 0..num_lfn {
                let order = num_lfn - i;
                let mut lfn = LfnEntry::new(order, i == 0, checksum);
                lfn.write_name_parts(&buf[(order - 1) * 13..order * 13]);
                entries[i] = DirEntry::Lfn(lfn);
            }
            Some(entries)
        } else {
            None
        }
    }

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
    const BASE_LOWER: u8 = 0x08;
    const EXT_LOWER: u8 = 0x10;

    pub(super) fn new() -> Self {
        Self {
            name: [b' '; 11],
            attr: 0,
            nt_res: 0,
            crt_time_tenth: 0,
            crt_time: 0,
            crt_date: 0,
            lst_acc_date: 0,
            fst_clus_hi: 0,
            wrt_time: 0,
            wrt_date: 0,
            fst_clus_lo: 0,
            file_size: 0,
        }
    }

    pub(super) fn current(c: Option<Cluster>) -> SfnEntry {
        let mut entry = Self::new();
        entry.name = *b".          ";
        entry.set_is_directory(true);
        entry.set_cluster(c);
        entry
    }

    pub(super) fn parent(c: Option<Cluster>) -> SfnEntry {
        let mut entry = Self::new();
        entry.name = *b"..         ";
        entry.set_is_directory(true);
        entry.set_cluster(c);
        entry
    }

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
        put(
            &self.name[0..8],
            (self.nt_res & Self::BASE_LOWER) == Self::BASE_LOWER,
        );
        if self.name[8] != 32 {
            put(b".", false);
        }
        put(
            &self.name[8..11],
            (self.nt_res & Self::EXT_LOWER) == Self::EXT_LOWER,
        );
        (is_irreversible, dest)
    }

    pub(super) fn set_or_generate_name(&mut self, name: &str) -> bool {
        let is_sfn_compatible = self.set_name(name);
        if !is_sfn_compatible {
            // FIXME: Avoid name collisions
            for (i, c) in name
                .chars()
                .filter_map(|c| {
                    Self::is_sfn_compatible_char(c).then(|| c.to_ascii_uppercase() as u8)
                })
                .chain(core::iter::repeat(' ' as u8))
                .take(11)
                .enumerate()
            {
                self.name[i] = c;
            }
        }
        is_sfn_compatible
    }

    pub(super) fn set_name(&mut self, name: &str) -> bool {
        let (base, ext) = match name.find('.') {
            Some(index) => {
                let base = &name[0..index];
                let ext = &name[index + 1..];
                if !matches!(base.len(), 1..=8) || !matches!(ext.len(), 1..=3) {
                    return false;
                }
                (base, ext)
            }
            None => {
                if !matches!(name.len(), 1..=8) {
                    return false;
                }
                (name, "")
            }
        };
        let base_contains_lower = base.chars().any(|c| c.is_ascii_lowercase());
        let base_contains_upper = base.chars().any(|c| c.is_ascii_uppercase());
        let ext_contains_lower = ext.chars().any(|c| c.is_ascii_lowercase());
        let ext_contains_upper = ext.chars().any(|c| c.is_ascii_uppercase());
        if !base.chars().all(Self::is_sfn_compatible_char)
            || !ext.chars().all(Self::is_sfn_compatible_char)
            || (base_contains_lower && base_contains_upper)
            || (ext_contains_lower && ext_contains_upper)
        {
            return false;
        }
        for (i, c) in base.chars().enumerate() {
            self.name[i] = c.to_ascii_uppercase() as u8;
        }
        for i in base.len()..8 {
            self.name[i] = ' ' as u8;
        }
        for (i, c) in ext.chars().enumerate() {
            self.name[i + 8] = c.to_ascii_uppercase() as u8;
        }
        for i in ext.len()..3 {
            self.name[i + 8] = ' ' as u8;
        }
        if base_contains_lower {
            self.nt_res |= Self::BASE_LOWER;
        } else {
            self.nt_res &= !Self::BASE_LOWER;
        }
        if ext_contains_lower {
            self.nt_res |= Self::EXT_LOWER;
        } else {
            self.nt_res &= !Self::EXT_LOWER;
        }
        true
    }

    pub(super) fn cluster(&self) -> Option<Cluster> {
        let index = self.fst_clus_lo as usize | ((self.fst_clus_hi as usize) << 16);
        (index != 0).then(|| Cluster::from_index(index))
    }

    pub(super) fn set_cluster(&mut self, cluster: Option<Cluster>) {
        let (lo, hi) = match cluster {
            Some(c) => (c.index() as u16, (c.index() >> 16) as u16),
            None => (0, 0),
        };
        self.fst_clus_lo = lo;
        self.fst_clus_hi = hi;
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

    pub(super) fn set_is_directory(&mut self, is_directory: bool) {
        if is_directory {
            self.attr |= DirEntry::DIRECTORY;
        } else {
            self.attr &= !DirEntry::DIRECTORY;
        }
    }

    pub(super) fn archive(&self) -> bool {
        (self.attr & DirEntry::ARCHIVE) == DirEntry::ARCHIVE
    }

    pub(super) fn mark_archive(&mut self) {
        self.attr |= DirEntry::ARCHIVE;
    }

    pub(super) fn checksum(&self) -> u8 {
        self.name.iter().fold(0u8, |sum, c| {
            (sum >> 1).wrapping_add(sum << 7).wrapping_add(*c)
        })
    }

    // TODO: Support create_datetime, last_access_date
    // FIXME: Support update_datetime (it is mandatory)

    pub(super) fn file_size(&self) -> usize {
        self.file_size as usize
    }

    pub(super) fn set_file_size(&mut self, size: usize) {
        assert!(size <= u32::MAX as usize);
        self.file_size = size as u32;
    }

    fn is_sfn_compatible_char(c: char) -> bool {
        matches!(c, '0'..='9' | 'A'..='Z' | 'a'..='z' | '!' | '#' | '$' | '%' | '&' | '\'' | '(' | ')' | '-' | '@' | '^' | '_' | '`' | '{' | '}' | '~')
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

    pub(super) fn new(order: usize, last: bool, chksum: u8) -> Self {
        assert!(1 <= order && order <= 20);
        Self {
            ord: (order as u8) | (if last { Self::LAST_LONG_ENTRY } else { 0 }),
            name1: [0; 10],
            attr: DirEntry::LONG_FILE_NAME,
            ty: 0,
            chksum,
            name2: [0; 12],
            _fst_clus_lo: 0,
            name3: [0; 4],
        }
    }

    pub(super) fn write_name_parts(&mut self, buf: &[u16]) {
        debug_assert_eq!(buf.len(), 13);
        for i in 0..13 {
            let bytes = buf[i].to_le_bytes();
            match i {
                0..=4 => self.name1.copy_from_array::<2>(i * 2, bytes),
                5..=10 => self.name2.copy_from_array::<2>((i - 5) * 2, bytes),
                11..=12 => self.name3.copy_from_array::<2>((i - 11) * 2, bytes),
                _ => unreachable!(),
            }
        }
    }

    pub(super) fn read_name_parts(&self, buf: &mut [u16]) {
        debug_assert_eq!(buf.len(), 13);
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

    fn is_lfn_compatible_char(c: char) -> bool {
        !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
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

#[derive(Debug)]
pub(super) enum LfnReader {
    Init,
    LfnSequence(u8, usize, Vec<u16>),
}

#[derive(Debug)]
pub(super) enum ReadLfnResult {
    Meta(DirEntry),
    Complete(String, SfnEntry),
    Incomplete,
    Broken(LfnReader, DirEntry),
}

impl LfnReader {
    pub(super) fn read(&mut self, e: DirEntry) -> ReadLfnResult {
        match (core::mem::replace(self, Self::Init), e) {
            (Self::Init, DirEntry::Lfn(lfn)) if lfn.is_last_entry() => {
                let checksum = lfn.checksum();
                let order = lfn.order();
                let mut buf = vec![0; order * 13];
                lfn.read_name_parts(&mut buf[(order - 1) * 13..order * 13]);
                *self = Self::LfnSequence(checksum, order - 1, buf);
                ReadLfnResult::Incomplete
            }
            (Self::Init, e @ DirEntry::Lfn(_)) => ReadLfnResult::Broken(Self::Init, e),
            (Self::Init, DirEntry::Sfn(sfn)) if !sfn.is_volume_id() => {
                let (_, name) = sfn.name();
                ReadLfnResult::Complete(name, sfn)
            }
            (Self::Init, e) => ReadLfnResult::Meta(e),
            (Self::LfnSequence(checksum, order, mut buf), DirEntry::Lfn(lfn))
                if order == lfn.order() && checksum == lfn.checksum() =>
            {
                lfn.read_name_parts(&mut buf[(order - 1) * 13..order * 13]);
                *self = Self::LfnSequence(checksum, order - 1, buf);
                ReadLfnResult::Incomplete
            }
            (Self::LfnSequence(checksum, order, mut buf), DirEntry::Sfn(sfn))
                if order == 0 && checksum == sfn.checksum() =>
            {
                // LFN is 0x0000-terminated and padded with 0xffff
                while matches!(buf.last(), Some(0xffff)) {
                    buf.pop();
                }
                if matches!(buf.last(), Some(0x0000)) {
                    buf.pop();
                }
                let name = String::from_utf16_lossy(buf.as_slice());
                ReadLfnResult::Complete(name, sfn)
            }
            (state @ Self::LfnSequence(..), e) => ReadLfnResult::Broken(state, e),
        }
    }
}
