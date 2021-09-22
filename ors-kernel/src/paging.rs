use crate::x64::{self, PageSize};
use acpi::{AcpiHandler, PhysicalMapping};
use core::ptr::NonNull;
use log::trace;

const EMPTY_PAGE_TABLE: x64::PageTable = x64::PageTable::new();

static mut PML4_TABLE: x64::PageTable = x64::PageTable::new();
static mut PDP_TABLE: x64::PageTable = x64::PageTable::new();
static mut PAGE_DIRECTORY: [x64::PageTable; 64] = [EMPTY_PAGE_TABLE; 64]; // supports up to 64GiB

pub unsafe fn initialize() {
    trace!("INITIALIZING paging");
    initialize_identity_mapping();
}

unsafe fn initialize_identity_mapping() {
    // Initialize identity mapping (always available but user inaccessible)
    use x64::PageTableFlags as Flags;

    let flags = Flags::PRESENT | Flags::WRITABLE | Flags::GLOBAL;

    unsafe fn phys_frame(page_table: &'static x64::PageTable) -> x64::PhysFrame {
        // `&'static x64::PageTable` are frame aligned
        x64::PhysFrame::from_start_address(
            // The virtual address of the `page_table` is identical to its physical address
            x64::PhysAddr::new(page_table as *const x64::PageTable as u64),
        )
        .unwrap()
    }

    // PML4_TABLE[0] -> PDP_TABLE
    PML4_TABLE[0].set_frame(phys_frame(&PDP_TABLE), flags);

    for (i, d) in PAGE_DIRECTORY.iter_mut().enumerate() {
        // PDP_TABLE[i] -> PAGE_DIRECTORY[i]
        PDP_TABLE[i].set_frame(phys_frame(d), flags);

        for (j, p) in PAGE_DIRECTORY[i].iter_mut().enumerate() {
            // PAGE_DIRECTORY[i][j] -> (identical mapping)
            let addr =
                x64::PhysAddr::new(i as u64 * x64::Size1GiB::SIZE + j as u64 * x64::Size2MiB::SIZE);
            p.set_addr(addr, flags | Flags::HUGE_PAGE);
        }
    }
    x64::Cr3::write(phys_frame(&PML4_TABLE), x64::Cr3Flags::empty());
}

pub unsafe fn mapper() -> impl x64::Mapper<x64::Size4KiB> + x64::Translate {
    // Since ors uses identity mapping, we can use OffsetPageTable with offset=0.
    // TODO: Replace it with manually implemented one
    x64::OffsetPageTable::new(&mut PML4_TABLE, x64::VirtAddr::zero())
}

pub fn as_virt_addr(addr: x64::PhysAddr) -> Option<x64::VirtAddr> {
    if addr.as_u64() < x64::Size1GiB::SIZE * 64 {
        // Physical memory areas of up to 64 GiB are identity-mapped.
        Some(x64::VirtAddr::new(addr.as_u64()))
    } else {
        None
    }
}

pub fn as_phys_addr(addr: x64::VirtAddr) -> Option<x64::PhysAddr> {
    if addr.as_u64() < x64::Size1GiB::SIZE * 64 {
        // Virtual memory areas of up to 64 GiB are identity-mapped.
        Some(x64::PhysAddr::new(addr.as_u64()))
    } else {
        // TODO: How this should be handled?
        // unsafe { mapper().translate_addr(addr) }
        None
    }
}

#[derive(Clone, Debug)]
pub struct KernelAcpiHandler;

impl AcpiHandler for KernelAcpiHandler {
    unsafe fn map_physical_region<T>(&self, addr: usize, size: usize) -> PhysicalMapping<Self, T> {
        let ptr = as_virt_addr(x64::PhysAddr::new(addr as u64))
            .unwrap()
            .as_mut_ptr();
        PhysicalMapping::new(addr, NonNull::new(ptr).unwrap(), size, size, self.clone())
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {}
}
