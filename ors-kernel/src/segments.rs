#![allow(dead_code)]

use asm::Segment;
use core::mem;
use modular_bitfield::prelude::*;

mod asm {
    pub use x86_64::addr::VirtAddr;
    pub use x86_64::instructions::segmentation::{Segment, CS, DS, ES, FS, GS, SS};
    pub use x86_64::instructions::tables::lgdt;
    pub use x86_64::structures::gdt::SegmentSelector;
    pub use x86_64::structures::DescriptorTablePointer;
}

pub unsafe fn initialize() {
    GDT[1].initialize_code_segment(0);
    GDT[2].initialize_data_segment(0);
    asm::lgdt(&asm::DescriptorTablePointer {
        limit: (GDT.len() * mem::size_of::<SegmentDescriptor>() - 1) as u16,
        base: asm::VirtAddr::new(&GDT[0] as *const SegmentDescriptor as u64),
    });
    asm::DS::set_reg(asm::SegmentSelector(0));
    asm::ES::set_reg(asm::SegmentSelector(0));
    asm::FS::set_reg(asm::SegmentSelector(0));
    asm::GS::set_reg(asm::SegmentSelector(0));
    asm::CS::set_reg(asm::SegmentSelector(1 << 3));
    asm::SS::set_reg(asm::SegmentSelector(2 << 3));
}

static mut GDT: [SegmentDescriptor; 3] = [SegmentDescriptor::new(); 3];

#[bitfield(bits = 64)]
#[derive(Debug, Clone, Copy)]
struct SegmentDescriptor {
    limit_low: B16,
    base_low: B16,
    base_middle: B8,
    ty: B4, // S and Type fields, together, specify the descriptor type and its access characteristics.
    s: B1,  // 0 = system segment, 1 = code or data segment
    dpl: B2,
    present: B1,
    limit_high: B4,
    available: B1,
    long_mode: B1,
    default_operation_size: B1,
    granularity: B1,
    base_high: B8,
}

impl SegmentDescriptor {
    fn initialize_code_segment(&mut self, descriptor_privilege_level: u8) {
        self.set_ty(0b1010); // Read/Write Data
        self.set_s(1);
        self.set_dpl(descriptor_privilege_level);
        self.set_present(1);
        self.set_available(0);
        self.set_long_mode(1);
        self.set_default_operation_size(0); // derived from long_mode on data segment
    }

    fn initialize_data_segment(&mut self, descriptor_privilege_level: u8) {
        self.set_ty(0b0010); // Execute/Read Code
        self.set_s(1);
        self.set_dpl(descriptor_privilege_level);
        self.set_present(1);
        self.set_available(0);
        self.set_long_mode(0); // reserved
        self.set_default_operation_size(1); // ignored, but set to 1 to make compatible with sycall
    }
}