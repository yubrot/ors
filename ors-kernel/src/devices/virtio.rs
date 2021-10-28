//! VirtIO Legacy Driver

pub mod block;

use crate::devices::pci;
use crate::paging::as_virt_addr;
use crate::phys_memory;
use crate::x64;
use alloc::vec::Vec;
use core::ptr;
use core::sync::atomic::{fence, Ordering};
use derive_new::new;

// https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.pdf

// const DEVICE_STATUS_FAILED: u8 = 128; // something went wrong in the guest
const DEVICE_STATUS_ACKNOWLEDGE: u8 = 1; // the guest OS has found the device and recognized it
const DEVICE_STATUS_DRIVER: u8 = 2; // the guest OS knows how to drive the device
const DEVICE_STATUS_FEATURES_OK: u8 = 8; // the driver has acknowledged all the features it understands, and feature negotiation is complete
const DEVICE_STATUS_DRIVER_OK: u8 = 4; // the driver is set up and ready to drive the device

#[derive(Debug, Clone, Copy)]
struct VirtIO {
    addr: u16,
    msi_x_enabled: bool,
}

impl VirtIO {
    fn new(addr: u16, msi_x_enabled: bool) -> Self {
        Self {
            addr,
            msi_x_enabled,
        }
    }

    unsafe fn from_pci_device(device: pci::Device) -> Result<Self, &'static str> {
        // > Legacy drivers skipped the Device Layout Detection step,
        // > assuming legacy device configuration space in BAR0 in I/O space unconditionally.
        let io_addr = device
            .read_bar(0)
            .io_port()
            .ok_or("BAR0 is not an I/O address")?;

        Ok(Self::new(
            io_addr,
            device.msi_x().map_or(false, |m| m.is_enabled()),
        ))
    }

    unsafe fn read<T: x64::PortRead>(self, offset: u16) -> T {
        x64::Port::new(self.addr + offset).read()
    }

    unsafe fn write<T: x64::PortWrite>(self, offset: u16, value: T) {
        x64::Port::new(self.addr + offset).write(value)
    }

    /// Perform general driver initialization.
    /// After calling this, caller must perform device-specific setup (including virtqueue setup)
    /// and then call `VirtIO::set_driver_ok`.
    unsafe fn initialize(self, negotiate: impl FnOnce(u32) -> u32) -> Result<(), &'static str> {
        // 3.1.1 Driver Requirements: Device Initialization
        self.set_device_status(self.device_status() | DEVICE_STATUS_ACKNOWLEDGE);
        self.set_device_status(self.device_status() | DEVICE_STATUS_DRIVER);
        const RING_INDIRECT_DESC: u32 = 1 << 28;
        const RING_EVENT_IDX: u32 = 1 << 29;
        let features = self.device_features();
        self.set_driver_features(negotiate(features) & !RING_INDIRECT_DESC & !RING_EVENT_IDX);
        self.set_device_status(self.device_status() | DEVICE_STATUS_FEATURES_OK);

        if (self.device_status() & DEVICE_STATUS_FEATURES_OK) == 0 {
            return Err("FEATURES_OK");
        }

        Ok(())
    }

    unsafe fn set_driver_ok(self) {
        self.set_device_status(self.device_status() | DEVICE_STATUS_DRIVER_OK);
    }

    unsafe fn device_features(self) -> u32 {
        self.read(0)
    }

    unsafe fn set_driver_features(self, value: u32) {
        self.write(0x04, value)
    }

    #[allow(dead_code)]
    unsafe fn queue_address(self) -> u32 {
        self.read(0x08)
    }

    unsafe fn set_queue_address(self, value: u32) {
        self.write(0x08, value)
    }

    unsafe fn queue_size(self) -> u32 {
        self.read(0x0c)
    }

    #[allow(dead_code)]
    unsafe fn queue_select(self) -> u16 {
        self.read(0x0e)
    }

    unsafe fn set_queue_select(self, value: u16) {
        self.write(0x0e, value)
    }

    unsafe fn set_queue_notify(self, value: u16) {
        self.write(0x10, value)
    }

    unsafe fn device_status(self) -> u8 {
        self.read(0x12)
    }

    unsafe fn set_device_status(self, value: u8) {
        self.write(0x12, value)
    }

    // 0x13: ISR status (Not used when MSI-X is enabled)

    #[allow(dead_code)]
    unsafe fn set_config_msix_vector(self, value: u16) {
        assert!(self.msi_x_enabled);
        self.write(0x14, value)
    }

    unsafe fn set_queue_msix_vector(self, value: u16) {
        assert!(self.msi_x_enabled);
        self.write(0x16, value)
    }

    fn device_specific_offset(self) -> u16 {
        if self.msi_x_enabled {
            0x18
        } else {
            0x14
        }
    }

    unsafe fn read_device_specific<T: x64::PortRead>(self, offset: u16) -> T {
        self.read(self.device_specific_offset() + offset)
    }

    #[allow(dead_code)]
    unsafe fn write_device_specific<T: x64::PortWrite>(self, offset: u16, value: T) {
        self.write(self.device_specific_offset() + offset, value)
    }
}

#[derive(Debug)]
struct VirtQueue<T> {
    queue_size: usize,
    frame: phys_memory::Frame,
    descriptor_table: *mut Descriptor,
    available_ring: *mut AvailableRing,
    used_ring: *mut UsedRing,

    last_used_idx: u16,
    first_free_descriptor: u16,
    num_free_descriptors: usize,
    buffer_associated_data: Vec<Option<T>>,
}

impl<T> VirtQueue<T> {
    /// Prepare the `queue_index`-th queue for the specified `virtio`.
    unsafe fn new(
        virtio: VirtIO,
        queue_index: u16,
        msi_x_vector: Option<u16>,
    ) -> Result<Self, &'static str> {
        virtio.set_queue_select(queue_index);
        let queue_size = virtio.queue_size() as usize;
        if queue_size == 0 {
            return Err("Queue is unavailable");
        }

        let layout = Self::compute_layout(queue_size);
        let frame = phys_memory::frame_manager()
            .allocate(layout.num_frames)
            .map_err(|_| "Cannot allocate frame for this queue")?;

        let base_ptr: *mut u8 = as_virt_addr(frame.phys_addr()).unwrap().as_mut_ptr();
        ptr::write_bytes(base_ptr, 0, phys_memory::Frame::SIZE * layout.num_frames); // zeroing

        virtio.set_queue_address(
            (frame.phys_addr().as_u64() / phys_memory::Frame::SIZE as u64) as u32,
        );

        if let Some(vector) = msi_x_vector {
            virtio.set_queue_msix_vector(vector);
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
            (x + phys_memory::Frame::SIZE - 1) & !(phys_memory::Frame::SIZE - 1)
        }

        let descriptor_table_size = 16 * queue_size;
        let available_ring_size = 6 + 2 * queue_size;
        let used_ring_size = 6 + 8 * queue_size;
        let a = align(descriptor_table_size + available_ring_size);
        let b = align(used_ring_size);
        VirtQueueLayout {
            num_frames: (a + b) / phys_memory::Frame::SIZE,
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
    fn transfer<I: ExactSizeIterator<Item = Buffer<T>>>(&mut self, buffers: I) -> Result<(), I> {
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
    fn collect(&mut self, mut handle: impl FnMut(T)) {
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
                let next = unsafe { (*self.descriptor_at(i)).next() };
                unsafe { (*self.descriptor_at(i)).set_next(prev_first_free_descriptor) };
                let associated_data = self.buffer_associated_data[i as usize].take();
                handle(associated_data.unwrap());

                match next {
                    Some(next_i) => i = next_i,
                    None => break,
                }
            }
        }
    }
}

impl<T> Drop for VirtQueue<T> {
    fn drop(&mut self) {
        let layout = Self::compute_layout(self.queue_size);
        phys_memory::frame_manager().free(self.frame, layout.num_frames);
    }
}

#[derive(Debug, Clone, Copy)]
struct VirtQueueLayout {
    num_frames: usize,
    descriptor_table_offset: usize,
    available_ring_offset: usize,
    used_ring_offset: usize,
}

#[derive(Debug, new)]
struct Buffer<T> {
    /// The address to the data being exchanged with the device.
    addr: x64::PhysAddr,
    /// Size of the data pointed to by the address.
    len: usize,
    /// Whether the data is device write-only (true) or device read-only (false).
    write: bool,
    /// Data associated with the buffer. Given to the `VirtQueue::collect` callback.
    associated_data: T,
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
