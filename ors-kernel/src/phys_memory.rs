// A frame represents a memory section on a physical address,
// and does not manage the usage of linear (virtual) addresses.

use crate::sync::mutex::{Mutex, MutexGuard};
use crate::x64;
use core::mem;
use log::trace;

static FRAME_MANAGER: Mutex<BitmapFrameManager> = Mutex::new(BitmapFrameManager::new());

pub fn frame_manager() -> MutexGuard<'static, BitmapFrameManager> {
    FRAME_MANAGER.lock()
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub struct Frame(usize);

impl Frame {
    pub unsafe fn from_phys_addr(addr: x64::PhysAddr) -> Self {
        Self(addr.as_u64() as usize / Frame::SIZE)
    }

    pub fn phys_addr(self) -> x64::PhysAddr {
        x64::PhysAddr::new((self.0 * Frame::SIZE) as u64)
    }

    pub fn phys_frame(self) -> x64::PhysFrame {
        x64::PhysFrame::from_start_address(self.phys_addr()).unwrap()
    }

    fn offset(self, offset: usize) -> Self {
        Self(self.0 + offset)
    }

    const MIN: Self = Self(1); // TODO: Why 1 instead of 0?
    const MAX: Self = Self(FRAME_COUNT);

    pub const SIZE: usize = 4096; // 4KiB (= 2 ** 12)
}

const MAX_PHYSICAL_MEMORY_BYTES: usize = 128 * 1024 * 1024 * 1024; // 128GiB
const FRAME_COUNT: usize = MAX_PHYSICAL_MEMORY_BYTES / Frame::SIZE;

type MapLine = usize;
const BITS_PER_MAP_LINE: usize = 8 * mem::size_of::<MapLine>();
const MAP_LINE_COUNT: usize = FRAME_COUNT / BITS_PER_MAP_LINE;

pub struct BitmapFrameManager {
    alloc_map: [MapLine; MAP_LINE_COUNT],
    begin: Frame,
    end: Frame,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub enum AllocateError {
    NotEnoughFrame,
}

impl BitmapFrameManager {
    pub const fn new() -> Self {
        Self {
            alloc_map: [0; MAP_LINE_COUNT],
            begin: Frame::MIN,
            end: Frame::MAX,
        }
    }

    pub fn total_frames(&self) -> usize {
        self.end.0 - self.begin.0
    }

    pub fn available_frames(&self) -> usize {
        (self.begin.0..self.end.0)
            .filter(|i| self.get_bit(Frame(*i)))
            .count()
    }

    pub fn availability_in_range(&self, a: f64, b: f64) -> f64 {
        assert!(0.0 <= a && a < b && b <= 1.0);
        let a = self.begin.0 + ((self.end.0 - self.begin.0) as f64 * a) as usize;
        let b = self.begin.0 + ((self.end.0 - self.begin.0) as f64 * b) as usize;
        let n = (a..b).filter(|i| self.get_bit(Frame(*i))).count();
        n as f64 / (b - a) as f64
    }

    fn set_memory_range(&mut self, begin: Frame, end: Frame) {
        self.begin = begin;
        self.end = end;
    }

    fn get_bit(&self, frame: Frame) -> bool {
        let line_index = frame.0 / BITS_PER_MAP_LINE;
        let bit_index = frame.0 % BITS_PER_MAP_LINE;
        (self.alloc_map[line_index] & (1 << bit_index)) != 0
    }

    fn set_bit(&mut self, frame: Frame, allocated: bool) {
        let line_index = frame.0 / BITS_PER_MAP_LINE;
        let bit_index = frame.0 % BITS_PER_MAP_LINE;

        if allocated {
            self.alloc_map[line_index] |= 1 << bit_index;
        } else {
            self.alloc_map[line_index] &= !(1 << bit_index);
        }
    }

    fn mark_allocated_in_bytes(&mut self, start: Frame, bytes: usize) {
        self.mark_allocated(start, bytes / Frame::SIZE, true)
    }

    pub fn allocate(&mut self, num_frames: usize) -> Result<Frame, AllocateError> {
        // Doing the first fit allocation
        let mut frame = self.begin;
        'search: loop {
            for i in 0..num_frames {
                if frame.offset(i) >= self.end {
                    Err(AllocateError::NotEnoughFrame)?
                }
                if self.get_bit(frame.offset(i)) {
                    frame = frame.offset(i + 1);
                    continue 'search;
                }
            }
            self.mark_allocated(frame, num_frames, false);
            return Ok(frame);
        }
    }

    fn mark_allocated(&mut self, frame: Frame, num_frames: usize, init: bool) {
        for i in 0..num_frames {
            if !init {
                trace!("phys_memory: allocate {:?}", frame.offset(i).phys_addr());
            }
            self.set_bit(frame.offset(i), true);
        }
    }

    pub fn free(&mut self, frame: Frame, num_frames: usize) {
        for i in 0..num_frames {
            trace!("phys_memory: deallocate {:?}", frame.offset(i).phys_addr());
            self.set_bit(frame.offset(i), false);
        }
    }

    /// Caller must ensure that the given MemoryMap is valid.
    pub unsafe fn initialize(&mut self, mm: &ors_common::memory_map::MemoryMap) {
        trace!("INITIALIZING PhysMemoryManager");
        let mut phys_available_end = 0;
        for d in mm.descriptors() {
            let phys_start = d.phys_start as usize;
            let phys_end = d.phys_end as usize;
            if phys_available_end < d.phys_start as usize {
                self.mark_allocated_in_bytes(
                    Frame::from_phys_addr(x64::PhysAddr::new(phys_available_end as u64)),
                    phys_start - phys_available_end,
                );
            }
            phys_available_end = phys_end;
        }
        self.set_memory_range(
            Frame::MIN,
            Frame::from_phys_addr(x64::PhysAddr::new(phys_available_end as u64)),
        );
    }
}

unsafe impl x64::FrameAllocator<x64::Size4KiB> for BitmapFrameManager {
    fn allocate_frame(&mut self) -> Option<x64::PhysFrame<x64::Size4KiB>> {
        match self.allocate(1) {
            Ok(frame) => Some(frame.phys_frame()),
            Err(_) => None,
        }
    }
}

impl x64::FrameDeallocator<x64::Size4KiB> for BitmapFrameManager {
    unsafe fn deallocate_frame(&mut self, frame: x64::PhysFrame<x64::Size4KiB>) {
        self.free(Frame::from_phys_addr(frame.start_address()), 1)
    }
}

#[cfg(test)]
mod tests {
    use super::frame_manager;
    use log::info;

    #[test_case]
    fn test_frame_manager() {
        info!("TESTING phys_memory::test_frame_manager");

        let a = frame_manager().allocate(1).unwrap();
        let b = frame_manager().allocate(1).unwrap();
        assert_ne!(a, b);

        let c = frame_manager().allocate(3).unwrap();
        assert_ne!(b, c);

        frame_manager().free(a, 1);
        frame_manager().free(b, 1);
        frame_manager().free(c, 3);
    }
}
