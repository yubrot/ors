use bit_field::BitField;
use core::mem;
use x64::Segment;

mod x64 {
    pub use x86_64::addr::VirtAddr;
    pub use x86_64::instructions::segmentation::{Segment, CS, DS, ES, FS, GS, SS};
    pub use x86_64::instructions::tables::lgdt;
    pub use x86_64::structures::gdt::SegmentSelector;
    pub use x86_64::structures::DescriptorTablePointer;
}

pub unsafe fn initialize() {
    GDT[1].initialize_code_segment(0);
    GDT[2].initialize_data_segment(0);
    x64::lgdt(&x64::DescriptorTablePointer {
        limit: (GDT.len() * mem::size_of::<SegmentDescriptor>() - 1) as u16,
        base: x64::VirtAddr::new(&GDT[0] as *const SegmentDescriptor as u64),
    });
    x64::DS::set_reg(x64::SegmentSelector(0));
    x64::ES::set_reg(x64::SegmentSelector(0));
    x64::FS::set_reg(x64::SegmentSelector(0));
    x64::GS::set_reg(x64::SegmentSelector(0));
    x64::CS::set_reg(x64::SegmentSelector(1 << 3));
    x64::SS::set_reg(x64::SegmentSelector(2 << 3));
}

static mut GDT: [SegmentDescriptor; 3] = [SegmentDescriptor(0); 3];

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
struct SegmentDescriptor(u64);

impl SegmentDescriptor {
    fn set_ty(&mut self, ty: u8) {
        // S and Type fields, together, specify the descriptor type and its access characteristics.
        self.0.set_bits(40..44, ty as u64);
    }

    fn set_s(&mut self, value: bool) {
        // false = system segment, true = code or data segment
        self.0.set_bit(44, value);
    }

    fn set_dpl(&mut self, value: u8) {
        self.0.set_bits(45..47, value as u64);
    }

    fn set_present(&mut self, value: bool) {
        self.0.set_bit(47, value);
    }

    fn set_available(&mut self, value: bool) {
        self.0.set_bit(52, value);
    }

    fn set_long_mode(&mut self, value: bool) {
        self.0.set_bit(53, value);
    }

    fn set_default_operation_size(&mut self, value: bool) {
        self.0.set_bit(54, value);
    }

    fn initialize_code_segment(&mut self, descriptor_privilege_level: u8) {
        self.set_ty(0b1010); // Read/Write Data
        self.set_s(true);
        self.set_dpl(descriptor_privilege_level);
        self.set_present(true);
        self.set_available(false);
        self.set_long_mode(true);
        self.set_default_operation_size(false); // derived from long_mode on data segment
    }

    fn initialize_data_segment(&mut self, descriptor_privilege_level: u8) {
        self.set_ty(0b0010); // Execute/Read Code
        self.set_s(true);
        self.set_dpl(descriptor_privilege_level);
        self.set_present(true);
        self.set_available(false);
        self.set_long_mode(false); // reserved
        self.set_default_operation_size(true); // ignored, but set to 1 to make compatible with sycall
    }
}
