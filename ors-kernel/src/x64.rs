//! Re-exporting x86_64 crate items and some additional definitions

pub use x86_64::instructions::hlt;
pub use x86_64::instructions::interrupts;
pub use x86_64::instructions::port::{Port, PortWriteOnly};
pub use x86_64::instructions::segmentation::{Segment, CS, SS};
pub use x86_64::instructions::tables::load_tss;
pub use x86_64::registers::control::{Cr2, Cr3, Cr3Flags};
pub use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
pub use x86_64::structures::idt::{
    InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode,
};
pub use x86_64::structures::paging::page_table::PageTableFlags;
pub use x86_64::structures::paging::{
    FrameAllocator, FrameDeallocator, Mapper, OffsetPageTable, PageSize, PageTable, PhysFrame,
    Size1GiB, Size2MiB, Size4KiB, Translate,
};
pub use x86_64::structures::tss::TaskStateSegment;
pub use x86_64::structures::DescriptorTablePointer;
pub use x86_64::{PhysAddr, VirtAddr};

use core::ptr;

#[derive(Debug, Clone, Copy)]
pub struct LApic {
    ptr: *mut u32,
}

impl LApic {
    pub fn new(addr: u64) -> Self {
        Self {
            ptr: addr as *mut u32,
        }
    }

    pub unsafe fn read(&self, offset: usize) -> u32 {
        ptr::read_volatile(self.ptr.add(offset))
    }

    pub unsafe fn write(&self, offset: usize, value: u32) {
        ptr::write_volatile(self.ptr.add(offset), value)
    }

    pub unsafe fn apic_id(&self) -> u32 {
        self.read(0x0020 / 4) >> 24
    }

    pub unsafe fn ver(&self) -> u32 {
        self.read(0x0030 / 4)
    }

    pub unsafe fn set_tpr(&self, value: u32) {
        self.write(0x0080 / 4, value)
    }

    pub unsafe fn set_eoi(&self, value: u32) {
        self.write(0x00B0 / 4, value)
    }

    pub unsafe fn set_svr(&self, value: u32) {
        self.write(0x00F0 / 4, value)
    }

    pub unsafe fn icrlo(&self) -> u32 {
        self.read(0x0300 / 4)
    }

    pub unsafe fn set_icrlo(&self, value: u32) {
        self.write(0x0300 / 4, value)
    }

    pub unsafe fn set_icrhi(&self, value: u32) {
        self.write(0x0310 / 4, value)
    }

    // Local Vector Table 0 (TIMER)
    pub unsafe fn set_timer(&self, value: u32) {
        self.write(0x0320 / 4, value)
    }

    pub unsafe fn set_pcint(&self, value: u32) {
        self.write(0x0340 / 4, value)
    }

    // Local Vector Table 1 (LINT0)
    pub unsafe fn set_lint0(&self, value: u32) {
        self.write(0x0350 / 4, value)
    }

    // Local Vector Table 2 (LINT1)
    pub unsafe fn set_lint1(&self, value: u32) {
        self.write(0x0360 / 4, value)
    }

    // Local Vector Table 3 (ERROR)
    pub unsafe fn set_error(&self, value: u32) {
        self.write(0x0370 / 4, value)
    }

    // Timer Initial Count
    pub unsafe fn set_ticr(&self, value: u32) {
        self.write(0x0380 / 4, value)
    }

    // Timer Current Count
    pub unsafe fn tccr(&self) -> u32 {
        self.read(0x0390 / 4)
    }

    // Timer Divide Configuration
    pub unsafe fn set_tdcr(&self, value: u32) {
        self.write(0x03E0 / 4, value)
    }
}

unsafe impl Sync for LApic {}

unsafe impl Send for LApic {}

#[derive(Debug)]
pub struct IoApic {
    ptr: *mut IoApicMmio,
}

#[repr(C)]
struct IoApicMmio {
    reg: u32,
    pad: [u32; 3],
    data: u32,
}

impl IoApic {
    pub fn new(addr: u64) -> Self {
        Self {
            ptr: addr as *mut IoApicMmio,
        }
    }

    pub unsafe fn read(&self, reg: u32) -> u32 {
        ptr::write_volatile(&mut (*self.ptr).reg, reg);
        ptr::read_volatile(&mut (*self.ptr).data)
    }

    pub unsafe fn write(&self, reg: u32, value: u32) {
        ptr::write_volatile(&mut (*self.ptr).reg, reg);
        ptr::write_volatile(&mut (*self.ptr).data, value);
    }

    pub unsafe fn apic_id(&self) -> u8 {
        (self.read(0x00) >> 24) as u8
    }

    pub unsafe fn ver(&self) -> u32 {
        self.read(0x01)
    }

    pub unsafe fn redirection_table_at(&self, index: u32) -> u64 {
        // configuration bits (low)
        (self.read(0x10 + 2 * index) as u64) |
            // a bitmask telling which CPUs can serve that interrupt (high)
            ((self.read(0x10 + 2 * index + 1) as u64) << 32)
    }

    pub unsafe fn set_redirection_table_at(&self, index: u32, value: u64) {
        self.write(0x10 + 2 * index, value as u32);
        self.write(0x10 + 2 * index + 1, (value >> 32) as u32);
    }
}
