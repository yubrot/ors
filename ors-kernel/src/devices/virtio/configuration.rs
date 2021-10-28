use crate::devices::pci;
use crate::x64;

// https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.pdf

// const DEVICE_STATUS_FAILED: u8 = 128; // something went wrong in the guest
const DEVICE_STATUS_ACKNOWLEDGE: u8 = 1; // the guest OS has found the device and recognized it
const DEVICE_STATUS_DRIVER: u8 = 2; // the guest OS knows how to drive the device
const DEVICE_STATUS_FEATURES_OK: u8 = 8; // the driver has acknowledged all the features it understands, and feature negotiation is complete
const DEVICE_STATUS_DRIVER_OK: u8 = 4; // the driver is set up and ready to drive the device

#[derive(Debug, Clone, Copy)]
pub struct Configuration {
    addr: u16,
    msi_x_enabled: bool,
}

impl Configuration {
    pub fn new(addr: u16, msi_x_enabled: bool) -> Self {
        Self {
            addr,
            msi_x_enabled,
        }
    }

    pub unsafe fn from_pci_device(device: pci::Device) -> Result<Self, &'static str> {
        assert!(device.is_virtio());
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
    /// and then call `Configuration::set_driver_ok`.
    pub unsafe fn initialize(self, negotiate: impl FnOnce(u32) -> u32) -> Result<(), &'static str> {
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

    pub unsafe fn set_driver_ok(self) {
        self.set_device_status(self.device_status() | DEVICE_STATUS_DRIVER_OK);
    }

    unsafe fn device_features(self) -> u32 {
        self.read(0)
    }

    unsafe fn set_driver_features(self, value: u32) {
        self.write(0x04, value)
    }

    pub unsafe fn queue_address(self) -> u32 {
        self.read(0x08)
    }

    pub unsafe fn set_queue_address(self, value: u32) {
        self.write(0x08, value)
    }

    pub unsafe fn queue_size(self) -> u32 {
        self.read(0x0c)
    }

    pub unsafe fn set_queue_size(self, value: u32) {
        self.write(0x0c, value)
    }

    pub unsafe fn queue_select(self) -> u16 {
        self.read(0x0e)
    }

    pub unsafe fn set_queue_select(self, value: u16) {
        self.write(0x0e, value)
    }

    pub unsafe fn set_queue_notify(self, value: u16) {
        self.write(0x10, value)
    }

    unsafe fn device_status(self) -> u8 {
        self.read(0x12)
    }

    unsafe fn set_device_status(self, value: u8) {
        self.write(0x12, value)
    }

    // 0x13: ISR status (Unused when MSI-X is enabled)

    pub unsafe fn set_config_msix_vector(self, value: u16) {
        assert!(self.msi_x_enabled);
        self.write(0x14, value)
    }

    pub unsafe fn set_queue_msix_vector(self, value: u16) {
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

    pub unsafe fn read_device_specific<T: x64::PortRead>(self, offset: u16) -> T {
        self.read(self.device_specific_offset() + offset)
    }

    pub unsafe fn write_device_specific<T: x64::PortWrite>(self, offset: u16, value: T) {
        self.write(self.device_specific_offset() + offset, value)
    }
}
