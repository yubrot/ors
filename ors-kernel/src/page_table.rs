mod x64 {
    pub use x86_64::registers::control::{Cr3, Cr3Flags};
    pub use x86_64::structures::paging::page_table::PageTableFlags;
    pub use x86_64::structures::paging::{PageTable, PhysFrame};
    pub use x86_64::PhysAddr;
    pub const EMPTY_PAGE_TABLE: PageTable = PageTable::new();
}

const PAGE_SIZE_4K: u64 = 4096;
const PAGE_SIZE_2M: u64 = 512 * PAGE_SIZE_4K;
const PAGE_SIZE_1G: u64 = 512 * PAGE_SIZE_2M;

static mut PML4_TABLE: x64::PageTable = x64::PageTable::new();
static mut PDP_TABLE: x64::PageTable = x64::PageTable::new();
static mut PAGE_DIRECTORY: [x64::PageTable; 64] = [x64::EMPTY_PAGE_TABLE; 64];

pub unsafe fn initialize() {
    use x64::PageTableFlags as Flags;

    // Initialize identity mapping

    unsafe fn phys_frame(page_table: &'static x64::PageTable) -> x64::PhysFrame {
        // `&'static x64::PageTable` are frame aligned
        x64::PhysFrame::from_start_address(
            // The virtual address of the `page_table` is identical to its physical address
            x64::PhysAddr::new(page_table as *const x64::PageTable as u64),
        )
        .unwrap()
    }

    // PML4_TABLE[0] -> PDP_TABLE
    PML4_TABLE[0].set_frame(phys_frame(&PDP_TABLE), Flags::PRESENT | Flags::WRITABLE);
    for (i, d) in PAGE_DIRECTORY.iter_mut().enumerate() {
        // PDP_TABLE[i] -> PAGE_DIRECTORY[i]
        PDP_TABLE[i].set_frame(phys_frame(d), Flags::PRESENT | Flags::WRITABLE);

        for (j, p) in PAGE_DIRECTORY[i].iter_mut().enumerate() {
            // PAGE_DIRECTORY[i][j] -> (identical mapping)
            p.set_addr(
                x64::PhysAddr::new(i as u64 * PAGE_SIZE_1G + j as u64 * PAGE_SIZE_2M),
                Flags::PRESENT | Flags::WRITABLE | Flags::HUGE_PAGE,
            );
        }
    }
    x64::Cr3::write(phys_frame(&PML4_TABLE), x64::Cr3Flags::empty());
}
