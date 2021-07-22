#![allow(dead_code)]

use core::mem;
use derive_new::new;
use heapless::Vec;
use modular_bitfield::prelude::*;
use ors_common::asm;

// https://wiki.osdev.org/PCI

const CONFIG_ADDRESS: u16 = 0x0cf8;
const CONFIG_DATA: u16 = 0x0cfc;

#[bitfield(bits = 32)]
#[derive(Debug, Clone, Copy)]
struct ConfigAddress {
    register_offset: B8,
    function_number: B3,
    device_number: B5,
    bus_number: B8,
    reserved: B7,
    enabled: B1,
}

impl ConfigAddress {
    fn at(bus: u8, device: u8, function: u8, reg: u8) -> Self {
        Self::new()
            .with_enabled(1)
            .with_bus_number(bus)
            .with_device_number(device)
            .with_function_number(function)
            .with_register_offset(reg)
    }

    fn write(self) {
        asm::io_out(CONFIG_ADDRESS, unsafe { mem::transmute(self) })
    }
}

#[derive(Debug, Clone, Copy)]
struct ConfigData(u32);

impl ConfigData {
    fn read() -> Self {
        Self(asm::io_in(CONFIG_DATA))
    }

    fn write(self) {
        asm::io_out(CONFIG_DATA, self.0);
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
    fn read(self, addr: u8) -> u32 {
        ConfigAddress::at(self.bus, self.device, self.function, addr).write();
        ConfigData::read().0
    }

    fn write(self, addr: u8, value: u32) {
        ConfigAddress::at(self.bus, self.device, self.function, addr).write();
        ConfigData(value).write();
    }

    pub fn vendor_id(self) -> u16 {
        self.read(0x00) as u16
    }

    pub fn is_vendor_intel(self) -> bool {
        self.vendor_id() == 0x8086
    }

    pub fn device_id(self) -> u16 {
        (self.read(0x00) >> 16) as u16
    }

    pub fn class_code(self) -> ClassCode {
        let data = self.read(0x08);
        ClassCode::new((data >> 24) as u8, (data >> 16) as u8, (data >> 8) as u8)
    }

    pub fn header_type(self) -> u8 {
        let data = self.read(0x0C);
        (data >> 16) as u8
    }

    pub fn is_single_function(self) -> bool {
        (self.header_type() & 0x80) == 0
    }

    pub fn bus_numbers(self) -> (u8, u8) {
        assert!(self.class_code().is_standard_pci_pci_bridge());
        let data = self.read(0x18);
        (data as u8, (data >> 8) as u8) // (primary, secondary)
    }

    pub fn read_bar(self, index: u8) -> Bar {
        // https://wiki.osdev.org/PCI#Base_Address_Registers
        let bar = self.read(base_address_register_address(index));
        if (bar & 0x1) != 0 {
            let bar = bar & !0x3;
            Bar::IoAddress(bar)
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

    pub fn scan<const N: usize>() -> Result<Vec<Self, N>, ScanError> {
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

    fn scan_bus<const N: usize>(bus: u8, dest: &mut Vec<Self, N>) -> Result<(), ScanError> {
        for device in 0..32 {
            if Self::new(bus, device, 0).vendor_id() != 0xffff {
                Self::scan_device(bus, device, dest)?;
            }
        }
        Ok(())
    }

    fn scan_device<const N: usize>(
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

    fn scan_function<const N: usize>(
        bus: u8,
        device: u8,
        function: u8,
        dest: &mut Vec<Self, N>,
    ) -> Result<(), ScanError> {
        let d = Self::new(bus, device, function);
        dest.push(d).map_err(|_| ScanError::Full)?;

        if d.class_code().is_standard_pci_pci_bridge() {
            let (_, secondary_bus) = d.bus_numbers();
            Self::scan_bus(secondary_bus, dest)?;
        }

        Ok(())
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum Bar {
    MemoryAddress(u64),
    IoAddress(u32),
}

impl Bar {
    pub fn mmio_base(self) -> usize {
        match self {
            Bar::MemoryAddress(addr) => addr as usize,
            Bar::IoAddress(_) => panic!("Not a memory-mapped I/O address: {:?}", self),
        }
    }
}

#[derive(Debug, Clone, Copy, new)]
pub struct ClassCode {
    pub base: u8,
    pub sub: u8,
    pub interface: u8,
}

impl ClassCode {
    pub fn is_standard_pci_pci_bridge(self) -> bool {
        self.base == 0x06 && self.sub == 0x04
    }

    pub fn is_xhci(self) -> bool {
        self.base == 0x0c && self.sub == 0x03 && self.interface == 0x30
    }
}

fn base_address_register_address(index: u8) -> u8 {
    assert!(index < 6);
    0x10 + 4 * index
}
