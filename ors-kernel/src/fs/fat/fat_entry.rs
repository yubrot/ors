use super::Cluster;

/// Deserialized FAT entry.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub(super) enum FatEntry {
    Unused,
    Reserved,
    UsedChained(Cluster),
    UsedEoc,
    Bad,
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
            FatEntry::Unused => 0,
            FatEntry::Reserved => 1,
            FatEntry::UsedChained(cluster) => cluster.index() as u32,
            FatEntry::UsedEoc => 0x0fffffff,
            FatEntry::Bad => 0x0ffffff7,
        }
    }
}
