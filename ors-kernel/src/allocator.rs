use super::global::frame_manager;
use super::paging::{as_phys_addr, as_virt_addr};
use super::phys_memory::Frame;
use crate::x64;
use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr;
use log::trace;
use spin::Mutex;

#[derive(Debug)]
enum AllocationMode {
    Block(usize),
    Frame(usize),
}

impl From<Layout> for AllocationMode {
    fn from(l: Layout) -> Self {
        let size = l.size().max(l.align());
        match BLOCK_SIZES.iter().position(|s| *s >= size) {
            Some(index) => Self::Block(index),
            None => Self::Frame((size + Frame::SIZE - 1) / Frame::SIZE),
        }
    }
}

const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

pub struct KernelAllocator {
    available_blocks: Mutex<[*mut u8; BLOCK_SIZES.len()]>,
}

impl KernelAllocator {
    pub const fn new() -> Self {
        Self {
            available_blocks: Mutex::new([ptr::null_mut(); BLOCK_SIZES.len()]),
        }
    }
}

unsafe impl Sync for KernelAllocator {}

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match layout.into() {
            AllocationMode::Block(index) => {
                let mut available_blocks = self.available_blocks.lock();
                let mut ptr = available_blocks[index];
                if ptr.is_null() {
                    ptr = allocate_frame_for_block(index);
                }
                if !ptr.is_null() {
                    available_blocks[index] = (ptr as *const u64).read() as *mut u8;
                }
                trace!(
                    "allocator: allocate block (size = {}) -> {:?}",
                    BLOCK_SIZES[index],
                    x64::VirtAddr::from_ptr(ptr)
                );
                ptr
            }
            AllocationMode::Frame(num) => match frame_manager().allocate(num) {
                Ok(frame) => {
                    let addr = as_virt_addr(frame.phys_addr()).unwrap();
                    trace!("allocator: allocate frame (num = {}) -> {:?}", num, addr);
                    addr.as_mut_ptr()
                }
                Err(_) => ptr::null_mut(),
            },
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match layout.into() {
            AllocationMode::Block(index) => {
                trace!(
                    "allocator: deallocate block (size = {}) -> {:?}",
                    BLOCK_SIZES[index],
                    x64::VirtAddr::from_ptr(ptr)
                );
                let mut available_blocks = self.available_blocks.lock();
                let next = available_blocks[index];
                (ptr as *mut u64).write(next as u64);
                available_blocks[index] = ptr;
            }
            AllocationMode::Frame(num) => {
                let addr = x64::VirtAddr::from_ptr(ptr as *const u8);
                trace!("allocator: deallocate frame (num = {}) -> {:?}", num, addr);
                let frame = Frame::from_phys_addr(as_phys_addr(addr).unwrap());
                frame_manager().free(frame, num);
            }
        }
    }
}

fn allocate_frame_for_block(index: usize) -> *mut u8 {
    let block_size = BLOCK_SIZES[index];
    let num_blocks_per_frame = Frame::SIZE / block_size;
    // NOTE: Frames for AllocationMode::Block are never deallocated
    let ptr: *mut u8 = match frame_manager().allocate(1) {
        Ok(frame) => as_virt_addr(frame.phys_addr()).unwrap().as_mut_ptr(),
        Err(_) => return ptr::null_mut(),
    };
    trace!(
        "allocator: allocate_frame_for_block(size = {}) -> {:?}",
        block_size,
        x64::VirtAddr::from_ptr(ptr)
    );
    for i in 0..num_blocks_per_frame {
        let current = unsafe { ptr.add(i * block_size) };
        let next = if i == num_blocks_per_frame - 1 {
            ptr::null_mut()
        } else {
            unsafe { current.add(block_size) }
        };
        unsafe { (current as *mut u64).write(next as u64) };
    }
    ptr
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use log::trace;

    #[test_case]
    fn test_frame() {
        trace!("TESTING allocator::test_frame");

        let a = Box::new([0u8; 4096]);
        let b = Box::new([0u8; 4096]);
        drop(a);
        let c = Box::new([0u8; 4096]);
        drop(b);
        drop(c);

        let d = Box::new([0u8; 4096 + 2048]);
        let e = Box::new([0u8; 4096 * 2]);
        let f = Box::new([0u8; 4096 * 3]);
        drop(d);
        drop(e);
        drop(f);
    }

    #[test_case]
    fn test_block1() {
        trace!("TESTING allocator::test_block1");

        let a = Box::new([0u8; 8]);
        let b = Box::new([0u8; 8]);
        drop(b);
        let c = Box::new([0u8; 8]);
        let d = Box::new([0u8; 8]);
        drop(d);
        drop(a);
        let e = Box::new([0u8; 8]);
        drop(c);
        drop(e);
        let _ = [Box::new([0u8; 8]), Box::new([0u8; 8]), Box::new([0u8; 8])];
    }

    #[test_case]
    fn test_block2() {
        trace!("TESTING allocator::test_block2");

        let a = Box::new([0u8; 1024]);
        let b = Box::new([0u8; 1024]);
        let c = Box::new([0u8; 1024]);
        let d = Box::new([0u8; 1024]);
        let e = Box::new([0u8; 1024]);
        drop(b);
        drop(d);
        let f = Box::new([0u8; 1024]);
        let g = Box::new([0u8; 1024]);
        let h = Box::new([0u8; 1024]);
        drop(a);
        drop(c);
        drop(e);
        drop(g);
        drop(f);
        drop(h);
    }
}
