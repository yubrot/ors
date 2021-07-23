mod asm {
    pub use x86_64::registers::control::{Cr3, Cr3Flags};
    pub use x86_64::structures::paging::page::Size2MiB;
    pub use x86_64::structures::paging::PhysFrame;
    pub use x86_64::PhysAddr;
}

const PAGE_SIZE_4K: u64 = 4096;
const PAGE_SIZE_2M: u64 = 512 * PAGE_SIZE_4K;
const PAGE_SIZE_1G: u64 = 512 * PAGE_SIZE_2M;

#[repr(align(4096))]
struct Pml4Table([u64; 512]);

#[repr(align(4096))]
struct PdpTable([u64; 512]);

#[repr(align(4096))]
struct PageDirectory([[u64; 512]; 64]);

static mut PML4_TABLE: Pml4Table = Pml4Table([0; 512]);
static mut PDP_TABLE: PdpTable = PdpTable([0; 512]);
static mut PAGE_DIRECTORY: PageDirectory = PageDirectory([[0; 512]; 64]);

pub unsafe fn initialize() {
    // Initialize identity mapping
    // PML4_TABLE[0] -> PDP_TABLE
    PML4_TABLE.0[0] = (&PDP_TABLE.0[0] as *const u64 as u64) | 0x3;

    for (i, d) in PAGE_DIRECTORY.0.iter_mut().enumerate() {
        // PDP_TABLE[i] -> PAGE_DIRECTORY[i]
        PDP_TABLE.0[i] = (d as *const u64 as u64) | 0x3;

        for (j, p) in PAGE_DIRECTORY.0[i].iter_mut().enumerate() {
            // PAGE_DIRECTORY[i][j] -> (identical address)
            *p = i as u64 * PAGE_SIZE_1G + j as u64 * PAGE_SIZE_2M | 0x83;
        }
    }
    let addr = asm::PhysAddr::new(&PML4_TABLE.0[0] as *const u64 as u64);
    asm::Cr3::write(
        asm::PhysFrame::from_start_address(addr).unwrap(),
        asm::Cr3Flags::empty(),
    );
}
