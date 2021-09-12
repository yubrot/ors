//! Re-exporting x86_64 crate items and some additional definitions

pub use x86_64::instructions::hlt;
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

#[derive(Debug)]
pub struct LApic {
    ptr: *mut u32,
}

impl LApic {
    pub fn new(addr: u64) -> Self {
        Self {
            ptr: addr as *mut u32,
        }
    }
}

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
}
