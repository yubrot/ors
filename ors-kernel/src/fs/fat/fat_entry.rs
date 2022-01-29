use super::Cluster;
use core::fmt;

/// Deserialized FAT entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(super) enum FatEntry {
    Unused,
    Reserved,
    UsedChained(Cluster),
    UsedEoc,
    Bad,
}

impl FatEntry {
    pub(super) fn chain(self) -> Option<Cluster> {
        match self {
            Self::UsedChained(c) => Some(c),
            _ => None,
        }
    }
}

impl fmt::Display for FatEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FatEntry::Unused => write!(f, "unused"),
            FatEntry::Reserved => write!(f, "reserved"),
            FatEntry::UsedChained(c) => write!(f, "used({})", c),
            FatEntry::UsedEoc => write!(f, "used(eoc)"),
            FatEntry::Bad => write!(f, "bad"),
        }
    }
}

impl From<Cluster> for FatEntry {
    fn from(c: Cluster) -> Self {
        Self::UsedChained(c)
    }
}

impl From<u32> for FatEntry {
    fn from(value: u32) -> Self {
        match value & 0x0fffffff {
            0 => Self::Unused,
            1 => Self::Reserved,
            n @ 0x00000002..=0x0ffffff6 => Self::UsedChained(Cluster::from_index(n as usize)),
            0x0ffffff7 => Self::Bad,
            0x0ffffff8..=0x0fffffff => Self::UsedEoc,
            0x10000000..=0xffffffff => unreachable!(),
        }
    }
}

impl Into<u32> for FatEntry {
    fn into(self) -> u32 {
        match self {
            Self::Unused => 0,
            Self::Reserved => 1,
            Self::UsedChained(cluster) => cluster.index() as u32,
            Self::UsedEoc => 0x0fffffff,
            Self::Bad => 0x0ffffff7,
        }
    }
}
