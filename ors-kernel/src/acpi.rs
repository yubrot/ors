use super::paging::as_virt_addr;
use acpi::{AcpiHandler, AcpiTables, PhysicalMapping, PlatformInfo};
use core::ptr::NonNull;

mod x64 {
    pub use x86_64::PhysAddr;
}

#[derive(Clone, Debug)]
struct Handler;

impl AcpiHandler for Handler {
    unsafe fn map_physical_region<T>(&self, addr: usize, size: usize) -> PhysicalMapping<Self, T> {
        let ptr = as_virt_addr(x64::PhysAddr::new(addr as u64))
            .unwrap()
            .as_mut_ptr();
        PhysicalMapping::new(addr, NonNull::new(ptr).unwrap(), size, size, self.clone())
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {}
}

pub unsafe fn platform_info(rsdp: usize) -> PlatformInfo {
    AcpiTables::from_rsdp(Handler, rsdp)
        .unwrap()
        .platform_info()
        .unwrap()
}
