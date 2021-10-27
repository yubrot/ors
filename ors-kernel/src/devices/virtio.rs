//! VirtIO Legacy Driver

pub mod block;

use crate::devices::pci;
use crate::paging::as_virt_addr;
use crate::phys_memory;
use crate::x64;
use core::ptr;

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

    unsafe fn queue_address(self) -> u32 {
        self.read(0x08)
    }

    unsafe fn set_queue_address(self, value: u32) {
        self.write(0x08, value)
    }

    unsafe fn queue_size(self) -> u32 {
        self.read(0x0c)
    }

    unsafe fn queue_select(self) -> u16 {
        self.read(0x0e)
    }

    unsafe fn set_queue_select(self, value: u16) {
        self.write(0x0e, value)
    }

    unsafe fn queue_notify(self) -> u16 {
        self.read(0x10)
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

    unsafe fn write_device_specific<T: x64::PortWrite>(self, offset: u16, value: T) {
        self.write(self.device_specific_offset() + offset, value)
    }
}

#[derive(Debug, Clone, Copy)]
struct VirtQueue {
    frame: phys_memory::Frame,
    queue_size: usize,
    descriptor_table: *mut Descriptor,
    available_ring: *mut AvailableRing,
    used_ring: *mut UsedRing,
}

impl VirtQueue {
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

        Ok(VirtQueue {
            frame,
            queue_size,
            descriptor_table: base_ptr.add(layout.descriptor_table_offset) as *mut Descriptor,
            available_ring: base_ptr.add(layout.available_ring_offset) as *mut AvailableRing,
            used_ring: base_ptr.add(layout.used_ring_offset) as *mut UsedRing,
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
}

#[derive(Debug, Clone, Copy)]
struct VirtQueueLayout {
    num_frames: usize,
    descriptor_table_offset: usize,
    available_ring_offset: usize,
    used_ring_offset: usize,
}

#[repr(C)]
struct Descriptor {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct AvailableRing {
    flags: u16,
    idx: u16,
    ring: [u16; 0],
}

#[repr(C)]
struct UsedRing {
    flags: u16,
    idx: u16,
    ring: [UsedElem; 0],
}

#[repr(C)]
struct UsedElem {
    id: u32,
    total: u32,
}
