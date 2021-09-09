// A frame represents a memory section on a physical address,
// and does not manage the usage of linear (virtual) addresses.

use core::mem;

mod x64 {
    pub use x86_64::structures::paging::PhysFrame;
    pub use x86_64::PhysAddr;
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub struct FrameId(usize);

impl FrameId {
    fn from_phys_addr(addr: x64::PhysAddr) -> Self {
        Self(addr.as_u64() as usize / BYTES_PER_FRAME)
    }

    pub fn phys_addr(self) -> x64::PhysAddr {
        x64::PhysAddr::new((self.0 * BYTES_PER_FRAME) as u64)
    }

    pub fn phys_frame(self) -> x64::PhysFrame {
        x64::PhysFrame::from_start_address(self.phys_addr()).unwrap()
    }

    fn offset(self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    const MIN: Self = Self(1); // TODO: Why 1 instead of 0?
    const MAX: Self = Self(FRAME_COUNT);
}

const MAX_PHYSICAL_MEMORY_BYTES: usize = 128 * 1024 * 1024 * 1024; // 128GiB
const BYTES_PER_FRAME: usize = 4096; // 4KiB (= 2 ** 12)
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

    pub fn initialize(&mut self, mm: &ors_common::memory_map::MemoryMap) {
        let mut phys_available_end = 0;
        for d in mm.descriptors() {
            let phys_start = d.phys_start as usize;
            let phys_end = d.phys_end as usize;
            if phys_available_end < d.phys_start as usize {
                self.mark_allocated_in_bytes(
                    FrameId::from_phys_addr(x64::PhysAddr::new(phys_available_end as u64)),
                    phys_start - phys_available_end,
                );
            }
            phys_available_end = phys_end;
        }
        self.set_memory_range(
            FrameId::MIN,
            FrameId::from_phys_addr(x64::PhysAddr::new(phys_available_end as u64)),
        );
    }
}
