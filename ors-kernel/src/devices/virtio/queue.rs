use super::Configuration;
use crate::paging::{as_phys_addr, as_virt_addr};
use crate::phys_memory::{frame_manager, Frame};
use crate::x64;
use alloc::vec::Vec;
use core::mem;
use core::ptr;
use core::sync::atomic::{fence, Ordering};
use derive_new::new;

#[derive(Debug)]
pub struct VirtQueue<T> {
    queue_size: usize,
    frame: Frame,
    descriptor_table: *mut Descriptor,
    available_ring: *mut AvailableRing,
    used_ring: *mut UsedRing,

    last_used_idx: u16,
    first_free_descriptor: u16,
    num_free_descriptors: usize,
    buffer_associated_data: Vec<Option<T>>,
}

impl<T> VirtQueue<T> {
    /// Prepare the `queue_index`-th queue for the specified `configuration`.
    pub unsafe fn new(
        configuration: Configuration,
        queue_index: u16,
        msi_x_vector: Option<u16>,
    ) -> Result<Self, &'static str> {
        configuration.set_queue_select(queue_index);
        let queue_size = configuration.queue_size() as usize;
        if queue_size == 0 {
            return Err("Queue is unavailable");
        }

        let layout = Self::compute_layout(queue_size);
        let frame = frame_manager()
            .allocate(layout.num_frames)
            .map_err(|_| "Cannot allocate frame for this queue")?;

        let base_ptr: *mut u8 = as_virt_addr(frame.phys_addr()).unwrap().as_mut_ptr();
        ptr::write_bytes(base_ptr, 0, Frame::SIZE * layout.num_frames); // zeroing

        configuration.set_queue_address((frame.phys_addr().as_u64() / Frame::SIZE as u64) as u32);

        if let Some(vector) = msi_x_vector {
            configuration.set_queue_msix_vector(vector);
        }

        let descriptor_table = base_ptr.add(layout.descriptor_table_offset) as *mut Descriptor;
        let available_ring = base_ptr.add(layout.available_ring_offset) as *mut AvailableRing;
        let used_ring = base_ptr.add(layout.used_ring_offset) as *mut UsedRing;

        // Build an initial descriptor-chain
        for i in 0..queue_size - 1 {
            let descriptor = &mut *descriptor_table.add(i);
            descriptor.set_next(Some((i + 1) as u16));
        }

        let mut buffer_associated_data = Vec::new();
        buffer_associated_data.resize_with(queue_size, || None);

        Ok(Self {
            queue_size,
            frame,
            descriptor_table,
            available_ring,
            used_ring,

            last_used_idx: 0,
            first_free_descriptor: 0,
            num_free_descriptors: queue_size,
            buffer_associated_data,
        })
    }

    fn compute_layout(queue_size: usize) -> VirtQueueLayout {
        // > For Legacy Interfaces, several additional restrictions are placed on the virtqueue layout:
        // > Each virtqueue occupies two or more physically-contiguous pages (usually defined as 4096
        // > bytes, but de-pending on the transport; henceforth referred to as Queue Align) and consists
        // > of three parts:
        // > | Descriptor Table | Available Ring (..padding..) | Used Ring |
        fn align(x: usize) -> usize {
            (x + Frame::SIZE - 1) & !(Frame::SIZE - 1)
        }

        let descriptor_table_size = 16 * queue_size;
        let available_ring_size = 6 + 2 * queue_size;
        let used_ring_size = 6 + 8 * queue_size;
        let a = align(descriptor_table_size + available_ring_size);
        let b = align(used_ring_size);
        VirtQueueLayout {
            num_frames: (a + b) / Frame::SIZE,
            descriptor_table_offset: 0,
            available_ring_offset: descriptor_table_size,
            used_ring_offset: a,
        }
    }

    fn descriptor_at(&self, i: u16) -> *mut Descriptor {
        self.descriptor_table.wrapping_add(i as usize)
    }

    fn available_ring_idx(&self) -> *mut u16 {
        unsafe { &mut (*self.available_ring).idx }
    }

    fn available_ring_at(&self, i: u16) -> *mut u16 {
        unsafe {
            (*self.available_ring)
                .ring
                .as_mut_ptr()
                .wrapping_add(i as usize % self.queue_size)
        }
    }

    fn used_ring_idx(&self) -> *mut u16 {
        unsafe { &mut (*self.used_ring).idx }
    }

    fn used_ring_at(&self, i: u16) -> *mut u32 {
        &mut unsafe {
            (*(*self.used_ring)
                .ring
                .as_mut_ptr()
                .wrapping_add(i as usize % self.queue_size))
            .idx
        }
    }

    /// Transfer the buffers to the device by allocating descriptors and put them to the available ring.
    /// This method does not send an Available Buffer Notification.
    pub fn transfer<I: ExactSizeIterator<Item = Buffer<T>>>(
        &mut self,
        buffers: I,
    ) -> Result<(), I> {
        if self.num_free_descriptors < buffers.len() {
            // not enough descriptors at the moment
            return Err(buffers);
        }

        let first = self.first_free_descriptor;
        let mut last = None;

        for buffer in buffers {
            let i = self.first_free_descriptor;
            last = Some(self.first_free_descriptor);

            // buffers[0] <-> first
            // buffers[1] <-> descriptor[first].next()
            // buffers[2] <-> descriptor[descriptor[first].next()].next()
            // ...
            unsafe { (*self.descriptor_at(i)).refer(buffer.addr, buffer.len, buffer.write) };
            assert!(self.buffer_associated_data[i as usize]
                .replace(buffer.associated_data)
                .is_none());

            match self.num_free_descriptors {
                0 => panic!("virtio: buffers.len() is different from the actual length"),
                1 => {
                    assert!(unsafe { (*self.descriptor_at(i)).next() }.is_none());
                    self.num_free_descriptors = 0;
                }
                _ => {
                    self.first_free_descriptor =
                        unsafe { (*self.descriptor_at(i)).next() }.unwrap();
                    self.num_free_descriptors -= 1;
                }
            }
        }

        if let Some(last) = last {
            // unlink descriptors-chain
            unsafe { (*self.descriptor_at(last)).set_next(None) };
            fence(Ordering::SeqCst);

            // enqueue
            unsafe { *self.available_ring_at(*self.available_ring_idx()) = first };
            fence(Ordering::SeqCst);
            unsafe { *self.available_ring_idx() = (*self.available_ring_idx()).wrapping_add(1) };
            fence(Ordering::SeqCst);
        }

        Ok(())
    }

    /// Collect the processed buffers by consuming the used ring.
    /// This method is supposed to be called from Used Buffer Notification (interrupt).
    pub fn collect(&mut self, mut handle: impl FnMut(T)) {
        while self.last_used_idx != unsafe { *self.used_ring_idx() } {
            fence(Ordering::SeqCst);
            // dequeue
            let mut i = unsafe { *self.used_ring_at(self.last_used_idx) } as u16;
            self.last_used_idx = self.last_used_idx.wrapping_add(1);

            // free descriptors
            loop {
                let prev_first_free_descriptor = match self.num_free_descriptors {
                    0 => None,
                    _ => Some(self.first_free_descriptor),
                };
                self.first_free_descriptor = i;
                self.num_free_descriptors += 1;
                let chain = unsafe { (*self.descriptor_at(i)).next() };
                unsafe { (*self.descriptor_at(i)).set_next(prev_first_free_descriptor) };
                let associated_data = self.buffer_associated_data[i as usize].take();
                handle(associated_data.unwrap());

                match chain {
                    Some(next) => i = next,
                    None => break,
                }
            }
        }
    }
}

impl<T> Drop for VirtQueue<T> {
    fn drop(&mut self) {
        let layout = Self::compute_layout(self.queue_size);
        frame_manager().free(self.frame, layout.num_frames);
    }
}

#[derive(Debug, Clone, Copy)]
struct VirtQueueLayout {
    num_frames: usize,
    descriptor_table_offset: usize,
    available_ring_offset: usize,
    used_ring_offset: usize,
}

#[derive(PartialEq, Eq, Debug, Clone, Copy, new)]
pub struct Buffer<T> {
    /// The address to the data being exchanged with the device.
    pub addr: x64::PhysAddr,
    /// Size of the data pointed to by the address.
    pub len: usize,
    /// Whether the data is device write-only (true) or device read-only (false).
    pub write: bool,
    /// Data associated with the buffer. Given to the `VirtQueue::collect` callback.
    pub associated_data: T,
}

impl<T> Buffer<T> {
    pub fn from_ref<D>(d: &D, associated_data: T) -> Option<Self> {
        Some(Self::new(
            as_phys_addr(x64::VirtAddr::from_ptr(d))?,
            mem::size_of::<D>(),
            false,
            associated_data,
        ))
    }

    pub fn from_ref_mut<D>(d: &mut D, associated_data: T) -> Option<Self> {
        Some(Self::new(
            as_phys_addr(x64::VirtAddr::from_ptr(d))?,
            mem::size_of::<D>(),
            true,
            associated_data,
        ))
    }

    pub fn from_bytes(bytes: &[u8], associated_data: T) -> Option<Self> {
        Some(Self::new(
            as_phys_addr(x64::VirtAddr::from_ptr(&bytes[0]))?,
            bytes.len(),
            false,
            associated_data,
        ))
    }

    pub fn from_bytes_mut(bytes: &mut [u8], associated_data: T) -> Option<Self> {
        Some(Self::new(
            as_phys_addr(x64::VirtAddr::from_ptr(&mut bytes[0]))?,
            bytes.len(),
            true,
            associated_data,
        ))
    }
}

// See VirtIO specification
#[repr(C)]
struct Descriptor {
    addr: u64, // guest-physical address
    len: u32,  // length
    flags: u16,
    next: u16, // the buffers can be chained via `next`
}

impl Descriptor {
    fn refer(&mut self, addr: x64::PhysAddr, len: usize, write: bool) {
        self.addr = addr.as_u64();
        self.len = len as u32;
        if write {
            self.flags |= Self::WRITE;
        } else {
            self.flags &= !Self::WRITE;
        }
    }

    fn next(&self) -> Option<u16> {
        if (self.flags & Self::NEXT) != 0 {
            Some(self.next)
        } else {
            None
        }
    }

    fn set_next(&mut self, next: Option<u16>) {
        match next {
            Some(next) => {
                self.flags |= Self::NEXT;
                self.next = next;
            }
            None => {
                self.flags &= !Self::NEXT;
                self.next = 0;
            }
        }
    }

    const NEXT: u16 = 1; // continuing via the next field
    const WRITE: u16 = 2; // device write-only (vs device read-only)
}

// driver write-only
#[repr(C)]
struct AvailableRing {
    _flags: u16, // This can be used to supress Used Buffer Notification (interrupt)
    idx: u16,
    ring: [u16; 0],
}

// driver read-only
#[repr(C)]
struct UsedRing {
    _flags: u16, // This can be used by device to supress Available Buffer Notification
    idx: u16,
    ring: [UsedElem; 0],
    // used_event: u16,
}

#[repr(C)]
struct UsedElem {
    idx: u32,
    _len: u32, // Length of the Descriptor-chain. This value is unreliable in legacy interface.
}
