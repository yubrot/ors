#![allow(dead_code)]

use crate::x64;
use bit_field::BitField;
use core::ptr;
use derive_new::new;
use heapless::Vec;
use log::trace;
use spin::Once;

static DEVICES: Once<Vec<Device, 32>> = Once::new();

pub fn initialize_devices() {
    DEVICES.call_once(|| {
        trace!("INITIALIZING PCI devices");
        unsafe { Device::scan::<32>() }.unwrap()
    });
}

pub fn devices() -> &'static Vec<Device, 32> {
    DEVICES
        .get()
        .expect("pci::devices is called before pci::initialize_devices")
}

// https://wiki.osdev.org/PCI
// https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html

static mut CONFIG_ADDRESS: x64::PortWriteOnly<u32> = x64::PortWriteOnly::new(0x0cf8);
static mut CONFIG_DATA: x64::Port<u32> = x64::Port::new(0x0cfc);

#[derive(Debug, Clone, Copy)]
struct ConfigAddress(u32);

impl ConfigAddress {
    fn new(bus: u8, device: u8, function: u8, reg: u8) -> Self {
        let mut value = 0;
        value.set_bits(0..8, reg as u32);
        value.set_bits(8..11, function as u32);
        value.set_bits(11..16, device as u32);
        value.set_bits(16..24, bus as u32);
        value.set_bit(31, true);
        Self(value)
    }

    unsafe fn write(self) {
        CONFIG_ADDRESS.write(self.0)
    }
}

#[derive(Debug, Clone, Copy)]
struct ConfigData(u32);

impl ConfigData {
    unsafe fn read() -> Self {
        ConfigData(CONFIG_DATA.read())
    }

    unsafe fn write(self) {
        CONFIG_DATA.write(self.0)
    }
}

#[derive(Debug, Clone, Copy, new)]
pub struct Device {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[derive(Debug, Clone)]
pub enum ScanError {
    Full,
}

impl Device {
    unsafe fn read(self, addr: u8) -> u32 {
        ConfigAddress::new(self.bus, self.device, self.function, addr).write();
        ConfigData::read().0
    }

    unsafe fn write(self, addr: u8, value: u32) {
        ConfigAddress::new(self.bus, self.device, self.function, addr).write();
        ConfigData(value).write();
    }

    pub unsafe fn vendor_id(self) -> u16 {
        self.read(0x00) as u16
    }

    pub unsafe fn is_vendor_intel(self) -> bool {
        self.vendor_id() == 0x8086
    }

    pub unsafe fn device_id(self) -> u16 {
        (self.read(0x00) >> 16) as u16
    }

    pub unsafe fn is_virtio(self) -> bool {
        // NOTE: Should this be named is_transitional_virtio?
        let vendor_id = self.vendor_id();
        let device_id = self.device_id();
        vendor_id == 0x1af4 && 0x1000 <= device_id && device_id <= 0x103f
    }

    pub unsafe fn command(self) -> u16 {
        self.read(0x04) as u16
    }

    pub unsafe fn status(self) -> u16 {
        (self.read(0x04) >> 16) as u16
    }

    pub unsafe fn device_type(self) -> DeviceType {
        let data = self.read(0x08);
        DeviceType::new((data >> 24) as u8, (data >> 16) as u8, (data >> 8) as u8)
    }

    pub unsafe fn header_type(self) -> u8 {
        let data = self.read(0x0c);
        (data >> 16) as u8 & 0x7f
    }

    pub unsafe fn is_single_function(self) -> bool {
        let data = self.read(0x0c);
        (data & (0x80 << 16)) == 0
    }

    // BIST

    pub unsafe fn num_bars(self) -> u8 {
        match self.header_type() {
            0x00 => 6,
            0x01 => 2,
            _ => 0,
        }
    }

    pub unsafe fn read_bar(self, index: u8) -> Bar {
        assert!(index < self.num_bars());

        // https://wiki.osdev.org/PCI#Base_Address_Registers
        let bar = self.read(base_address_register_address(index));
        if (bar & 0x1) != 0 {
            let bar = (bar & !0x3) as u16;
            Bar::IoPort(bar)
        } else {
            if (bar & 0x4) != 0 {
                let bar_lower = (bar as u64) & !0xf;
                let bar_upper = self.read(base_address_register_address(index + 1));
                let bar_upper = (bar_upper as u64) << 32;
                Bar::MemoryAddress(bar_lower | bar_upper)
            } else {
                let bar = (bar as u64) & !0xf;
                Bar::MemoryAddress(bar)
            }
        }
    }

    pub unsafe fn bus_numbers(self) -> (u8, u8) {
        assert!(self.device_type().is_standard_pci_to_pci_bridge());
        let data = self.read(0x18);
        (data as u8, (data >> 8) as u8) // (primary, secondary)
    }

    pub unsafe fn subsystem_vendor_id(self) -> u16 {
        assert_eq!(self.header_type(), 0x00);
        self.read(0x2C) as u16
    }

    pub unsafe fn subsystem_id(self) -> u16 {
        assert_eq!(self.header_type(), 0x00);
        (self.read(0x2C) >> 16) as u16
    }

    pub unsafe fn capability_pointer(self) -> Option<u8> {
        if matches!(self.header_type(), 0x00 | 0x01) && (self.status() & 0x16) != 0 {
            Some(self.read(0x34) as u8)
        } else {
            None
        }
    }

    pub unsafe fn capabilities(self) -> Capabilities {
        Capabilities::new(self, 0)
    }

    pub unsafe fn msi_x(self) -> Option<MsiX> {
        self.capabilities().find_map(|c| c.msi_x())
    }

    pub unsafe fn interrupt_line(self) -> u8 {
        self.read(0x3C) as u8
    }

    pub unsafe fn interrupt_pin(self) -> u8 {
        (self.read(0x3C) >> 8) as u8
    }

    pub unsafe fn scan<const N: usize>() -> Result<Vec<Self, N>, ScanError> {
        let mut devices = Vec::new();

        // Checks whether the host bridge (bus=0, device=0) is a multifunction device
        if Self::new(0, 0, 0).is_single_function() {
            Self::scan_bus(0, &mut devices)?;
        } else {
            // Each host bridge with function=N is responsible for bus=N
            for function in 0..8 {
                if Self::new(0, 0, function).vendor_id() != 0xffff {
                    Self::scan_bus(function, &mut devices)?;
                }
            }
        }
        Ok(devices)
    }

    unsafe fn scan_bus<const N: usize>(bus: u8, dest: &mut Vec<Self, N>) -> Result<(), ScanError> {
        for device in 0..32 {
            if Self::new(bus, device, 0).vendor_id() != 0xffff {
                Self::scan_device(bus, device, dest)?;
            }
        }
        Ok(())
    }

    unsafe fn scan_device<const N: usize>(
        bus: u8,
        device: u8,
        dest: &mut Vec<Self, N>,
    ) -> Result<(), ScanError> {
        Self::scan_function(bus, device, 0, dest)?;
        if !Self::new(bus, device, 0).is_single_function() {
            for function in 1..8 {
                if Self::new(bus, device, function).vendor_id() != 0xffff {
                    Self::scan_function(bus, device, function, dest)?;
                }
            }
        }
        Ok(())
    }

    unsafe fn scan_function<const N: usize>(
        bus: u8,
        device: u8,
        function: u8,
        dest: &mut Vec<Self, N>,
    ) -> Result<(), ScanError> {
        let d = Self::new(bus, device, function);
        dest.push(d).map_err(|_| ScanError::Full)?;

        if d.device_type().is_standard_pci_to_pci_bridge() {
            let (_, secondary_bus) = d.bus_numbers();
            Self::scan_bus(secondary_bus, dest)?;
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum Bar {
    MemoryAddress(u64),
    IoPort(u16),
}

impl Bar {
    pub fn mmio_base(self) -> Option<usize> {
        match self {
            Bar::MemoryAddress(addr) => Some(addr as usize),
            Bar::IoPort(_) => None,
        }
    }

    pub fn io_port(self) -> Option<u16> {
        match self {
            Bar::MemoryAddress(_) => None,
            Bar::IoPort(port) => Some(port),
        }
    }
}

#[derive(Debug, Clone, Copy, new)]
pub struct DeviceType {
    pub class_code: u8,
    pub subclass: u8,
    pub prog_interface: u8,
}

impl DeviceType {
    pub fn is_standard_pci_to_pci_bridge(self) -> bool {
        self.class_code == 0x06 && self.subclass == 0x04
    }

    pub fn is_xhci(self) -> bool {
        self.class_code == 0x0c && self.subclass == 0x03 && self.prog_interface == 0x30
    }
}

fn base_address_register_address(index: u8) -> u8 {
    assert!(index < 6);
    0x10 + 4 * index
}

#[derive(Debug, Clone, Copy, new)]
pub struct Capabilities {
    device: Device,
    pointer: u8,
}

impl Iterator for Capabilities {
    type Item = Capability;

    fn next(&mut self) -> Option<Self::Item> {
        let p = if self.pointer == 0 {
            unsafe { self.device.capability_pointer() }?
        } else {
            unsafe { Capability::new(self.device, self.pointer).next_capability_pointer() }?
        };
        self.pointer = p;
        Some(Capability::new(self.device, p))
    }
}

#[derive(Debug, Clone, Copy, new)]
pub struct Capability {
    device: Device,
    pointer: u8,
}

impl Capability {
    pub unsafe fn id(self) -> u8 {
        self.device.read(self.pointer) as u8
    }

    pub unsafe fn is_msi_x(self) -> bool {
        self.id() == 0x11
    }

    pub unsafe fn is_vendor_specific(self) -> bool {
        self.id() == 0x09
    }

    pub unsafe fn msi_x(self) -> Option<MsiX> {
        if self.is_msi_x() {
            Some(MsiX::new(self.device, self.pointer))
        } else {
            None
        }
    }

    pub unsafe fn next_capability_pointer(self) -> Option<u8> {
        match (self.device.read(self.pointer) >> 8) as u8 {
            0 => None,
            p => Some(p),
        }
    }
}

#[derive(Debug, Clone, Copy, new)]
pub struct MsiX {
    device: Device,
    pointer: u8,
}

impl MsiX {
    unsafe fn message_control(self) -> u16 {
        (self.device.read(self.pointer) >> 16) as u16
    }

    pub unsafe fn enable(self) {
        let value = self.device.read(self.pointer) | (1 << 31);
        self.device.write(self.pointer, value)
    }

    pub unsafe fn table_size(self) -> usize {
        (self.message_control() & 0x7ff) as usize + 1
    }

    /// Table BAR Indicator
    unsafe fn table_bir(self) -> u8 {
        self.device.read(self.pointer + 0x04) as u8
    }

    unsafe fn table_offset(self) -> u32 {
        self.device.read(self.pointer + 0x04) >> 8
    }

    unsafe fn table_bar(self) -> Bar {
        self.device.read_bar(self.table_bir())
    }

    pub unsafe fn table(self) -> MsiXTable {
        let addr = self.table_bar().mmio_base().unwrap() + self.table_offset() as usize;
        MsiXTable {
            ptr: addr as *mut u32,
            len: self.table_size(),
        }
    }

    /// Pending Bit Array BAR Indicator
    pub unsafe fn pba_bir(self) -> u8 {
        self.device.read(self.pointer + 0x08) as u8
    }

    pub unsafe fn pba_offset(self) -> u32 {
        self.device.read(self.pointer + 0x08) >> 8
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MsiXTable {
    ptr: *mut u32,
    len: usize,
}

impl MsiXTable {
    pub fn len(self) -> usize {
        self.len
    }

    pub fn entry(self, index: usize) -> MsiXTableEntry {
        assert!(index < self.len());
        MsiXTableEntry {
            ptr: unsafe { self.ptr.add(4 * index) },
        }
    }

    pub fn entries(self) -> impl Iterator<Item = MsiXTableEntry> {
        (0..self.len()).map(move |i| self.entry(i))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MsiXTableEntry {
    ptr: *mut u32,
}

impl MsiXTableEntry {
    pub unsafe fn enable(self, lapic_id: u32, vector: u32) {
        assert!(lapic_id < 256);
        assert!(32 <= vector && vector <= 254);

        const ADDRESS_SUFFIX: u32 = 0xfeee << 20;
        let reserved_bits = self.message_address() & 0xff0;
        self.set_message_address((lapic_id << 12) | ADDRESS_SUFFIX | reserved_bits); // TODO: RH | DM (See Intel SDM)
        const LEVEL: u32 = 1 << 15; // Level-triggered (vs edge-)
        let reserved_bits = self.message_data() & 0xffff3800;
        self.set_message_data(vector | LEVEL | reserved_bits); // TODO: DM
        let reserved_bits = self.vector_control() & !1; // unmask
        self.set_vector_control(reserved_bits);
    }

    pub unsafe fn disable(self) {
        let value = self.vector_control() | 1; // mask
        self.set_vector_control(value);
    }

    unsafe fn message_address(self) -> u32 {
        // NOTE: upper 32bits of Message address are not used in x86_64
        ptr::read_volatile(self.ptr)
    }

    unsafe fn set_message_address(self, value: u32) {
        ptr::write_volatile(self.ptr, value)
    }

    unsafe fn message_data(self) -> u32 {
        ptr::read_volatile(self.ptr.add(2))
    }

    unsafe fn set_message_data(self, value: u32) {
        ptr::write_volatile(self.ptr.add(2), value)
    }

    unsafe fn vector_control(self) -> u32 {
        ptr::read_volatile(self.ptr.add(3))
    }

    unsafe fn set_vector_control(self, value: u32) {
        ptr::write_volatile(self.ptr.add(3), value)
    }
}
