#[repr(C)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct MemoryMap {
    pub descriptors: *const Descriptor,
    pub descriptors_len: u64,
}

#[repr(C)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct Descriptor {
    pub phys_start: u64,
    pub phys_end: u64,
}
