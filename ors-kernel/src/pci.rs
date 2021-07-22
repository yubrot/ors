use core::mem;
use derive_new::new;
use heapless::Vec;
use modular_bitfield::prelude::*;
use ors_common::asm;

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
    pub fn vendor_id(self) -> u16 {
        ConfigAddress::at(self.bus, self.device, self.function, 0x00).write();
        ConfigData::read().0 as u16
    }

    pub fn device_id(self) -> u16 {
        ConfigAddress::at(self.bus, self.device, self.function, 0x00).write();
        (ConfigData::read().0 >> 16) as u16
    }

    pub fn class_code(self) -> ClassCode {
        ConfigAddress::at(self.bus, self.device, self.function, 0x08).write();
        let reg = ConfigData::read().0;
        ClassCode::new((reg >> 24) as u8, (reg >> 16) as u8, (reg >> 8) as u8)
    }

    pub fn header_type(self) -> u8 {
        ConfigAddress::at(self.bus, self.device, self.function, 0x0C).write();
        let reg = ConfigData::read().0;
        (ConfigData::read().0 >> 16) as u8
    }

    pub fn is_single_function(self) -> bool {
        (self.header_type() & 0x80) == 0
    }

    pub fn bus_numbers(self) -> (u8, u8) {
        assert!(self.class_code().is_standard_pci_pci_bridge());
        ConfigAddress::at(self.bus, self.device, self.function, 0x18).write();
        let reg = ConfigData::read().0;
        (reg as u8, (reg >> 8) as u8) // (primary, secondary)
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
        if dest.is_full() {
            Err(ScanError::Full)?;
        }
        let d = Self::new(bus, device, function);
        dest.push(d);

        if d.class_code().is_standard_pci_pci_bridge() {
            let (_, secondary_bus) = d.bus_numbers();
            Self::scan_bus(secondary_bus, dest)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, new)]
pub struct ClassCode {
    pub base: u8,
    pub sub: u8,
    pub interface: u8,
}

impl ClassCode {
    pub fn is_usb_3_0(self) -> bool {
        self.interface == 0x30
    }

    pub fn is_standard_pci_pci_bridge(self) -> bool {
        self.base == 0x06 && self.sub == 0x04
    }
}
