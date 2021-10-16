use crate::x64;
use acpi::platform::address::AddressSpace;
use acpi::platform::interrupt::Apic;
use acpi::platform::{PmTimer, ProcessorInfo};
use acpi::{AcpiHandler, AcpiTables, PlatformInfo};
use spin::Once;

static PLATFORM_INFO: Once<PlatformInfo> = Once::new();

/// Caller must ensure that the given rsdp is valid.
pub unsafe fn initialize(handler: impl AcpiHandler, rsdp: usize) {
    PLATFORM_INFO.call_once(|| {
        // https://wiki.osdev.org/MADT
        AcpiTables::from_rsdp(handler, rsdp)
            .unwrap()
            .platform_info()
            .unwrap()
    });
}

fn platform_info() -> &'static PlatformInfo {
    PLATFORM_INFO.wait()
}

pub fn apic_info() -> &'static Apic {
    match platform_info().interrupt_model {
        acpi::InterruptModel::Apic(ref apic) => apic,
        _ => panic!("Could not find APIC"),
    }
}

pub fn processor_info() -> &'static ProcessorInfo {
    platform_info()
        .processor_info
        .as_ref()
        .expect("Could not find processor information")
}

pub fn pm_timer() -> &'static PmTimer {
    platform_info()
        .pm_timer
        .as_ref()
        .expect("Could not find ACPI PM Timer")
}

pub fn wait_milliseconds_with_pm_timer(msec: u32) {
    // https://wiki.osdev.org/ACPI_Timer
    let pm_timer = pm_timer();
    assert_eq!(pm_timer.base.address_space, AddressSpace::SystemIo); // TODO: MMIO Support
    assert_eq!(pm_timer.base.bit_width, 32);
    let mut time = x64::Port::<u32>::new(pm_timer.base.address as u16);

    const PM_TIMER_FREQ: usize = 3579545;
    let start = unsafe { time.read() };
    let mut end = start.wrapping_add((PM_TIMER_FREQ * msec as usize / 1000) as u32);
    if !pm_timer.supports_32bit {
        end &= 0x00ffffff;
    }
    if end < start {
        while unsafe { time.read() } >= start {}
    }
    while unsafe { time.read() } < end {}
}
