// A page frame represents a memory section on a physical address,
// and does not manage the usage of linear addresses.

use core::mem;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub struct FrameId(usize);

impl FrameId {
    pub fn from_physical_address(address: usize) -> Self {
        Self(address / BYTES_PER_FRAME)
    }

    pub fn frame_ptr(self) -> *const u8 {
        (self.0 * BYTES_PER_FRAME) as *const u8
    }

    pub fn offset(self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    pub const MIN: Self = Self(1); // TODO: Why 1 instead of 0?
    pub const MAX: Self = Self(FRAME_COUNT);
}

const MAX_PHYSICAL_MEMORY_BYTES: usize = 128 * 1024 * 1024 * 1024; // 128GiB
const BYTES_PER_FRAME: usize = 4096; // 4KiB
const FRAME_COUNT: usize = MAX_PHYSICAL_MEMORY_BYTES / BYTES_PER_FRAME;

type MapLine = usize;
const BITS_PER_MAP_LINE: usize = 8 * mem::size_of::<MapLine>();
const MAP_LINE_COUNT: usize = FRAME_COUNT / BITS_PER_MAP_LINE;

pub struct BitmapMemoryManager {
    alloc_map: [MapLine; MAP_LINE_COUNT],
    begin: FrameId,
    end: FrameId,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub enum AllocateError {
    NotEnoughMemory,
}

impl BitmapMemoryManager {
    pub const fn new() -> Self {
        Self {
            alloc_map: [0; MAP_LINE_COUNT],
            begin: FrameId::MIN,
            end: FrameId::MAX,
        }
    }

    pub fn set_memory_range(&mut self, begin: FrameId, end: FrameId) {
        self.begin = begin;
        self.end = end;
    }

    pub fn get_bit(&self, id: FrameId) -> bool {
        let line_index = id.0 / BITS_PER_MAP_LINE;
        let bit_index = id.0 % BITS_PER_MAP_LINE;
        (self.alloc_map[line_index] & (1 << bit_index)) != 0
    }

    pub fn set_bit(&mut self, id: FrameId, allocated: bool) {
        let line_index = id.0 / BITS_PER_MAP_LINE;
        let bit_index = id.0 % BITS_PER_MAP_LINE;

        if allocated {
            self.alloc_map[line_index] |= 1 << bit_index;
        } else {
            self.alloc_map[line_index] &= !(1 << bit_index);
        }
    }

    pub fn mark_allocated_in_bytes(&mut self, start: FrameId, bytes: usize) {
        self.mark_allocated(start, bytes / BYTES_PER_FRAME)
    }

    pub fn allocate(&mut self, num_frames: usize) -> Result<FrameId, AllocateError> {
        // Doing the first fit allocation
        let mut id = self.begin;
        'search: loop {
            for i in 0..num_frames {
                if id.offset(i) >= self.end {
                    Err(AllocateError::NotEnoughMemory)?
                }
                if self.get_bit(id.offset(i)) {
                    id = id.offset(i + 1);
                    continue 'search;
                }
            }
            self.mark_allocated(id, num_frames);
            return Ok(id);
        }
    }

    pub fn mark_allocated(&mut self, id: FrameId, num_frames: usize) {
        for i in 0..num_frames {
            self.set_bit(id.offset(i), true);
        }
    }

    pub fn free(&mut self, id: FrameId, num_frames: usize) {
        for i in 0..num_frames {
            self.set_bit(id.offset(i), false);
        }
    }
}
