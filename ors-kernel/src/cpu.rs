//! This module works on the assumption that the processor information is initialized by
//! calling `initialize` before any processor other than BSP is enabled.

use crate::task::Task;
use crate::x64;
use ors_common::non_contiguous::Array;
use spin::{Mutex, Once};

static SYSTEM_INFO: Once<SystemInfo> = Once::new();
static BOOT_STRAP_CPU_INFO: Mutex<CpuInfo> = Mutex::new(CpuInfo::new());

#[derive(Debug)]
struct SystemInfo {
    lapic: x64::LApic,
    boot_strap_lapic_id: u32,
    application_cpu_info: Array<u32, Mutex<CpuInfo>, 64>,
}

pub fn initialize(
    lapic_address: u64,
    bsp_lapic_id: u32,
    ap_lapic_ids: impl IntoIterator<Item = u32>,
) {
    SYSTEM_INFO.call_once(move || {
        let mut application_cpu_info = Array::new();
        for ap_lapic_id in ap_lapic_ids {
            application_cpu_info.insert(ap_lapic_id, Mutex::new(CpuInfo::new()));
        }
        SystemInfo {
            lapic: x64::LApic::new(lapic_address),
            boot_strap_lapic_id: bsp_lapic_id,
            application_cpu_info,
        }
    });
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy, Hash)]
pub enum Cpu {
    BootStrap,
    Application(u32),
}

impl Cpu {
    pub fn current() -> Self {
        if let Some(info) = SYSTEM_INFO.get() {
            let id = unsafe { info.lapic.apic_id() };
            if id == info.boot_strap_lapic_id {
                Self::BootStrap
            } else {
                Self::Application(id)
            }
        } else {
            Self::BootStrap // works under the module assumption
        }
    }

    pub fn list() -> impl Iterator<Item = Cpu> {
        core::iter::once(Self::BootStrap).chain(SYSTEM_INFO.get().into_iter().flat_map(|info| {
            info.application_cpu_info
                .iter()
                .map(|(lapic_id, _)| Self::Application(*lapic_id))
        }))
    }

    /// Get information about the CPU.
    /// This Mutex does not get interrupt lock (`crate::interrupts::Cli`).
    /// Moreover, acquiring and releasing `crate::mutex::Mutex` will lock this mutex through interrupt lock.
    /// We need to be careful about deadlocks when using this method.
    pub fn info(self) -> &'static Mutex<CpuInfo> {
        match self {
            Self::BootStrap => &BOOT_STRAP_CPU_INFO,
            Self::Application(lapic_id) => SYSTEM_INFO
                .wait() // works under the module assumption
                .application_cpu_info
                .get(lapic_id)
                .expect("Unknown CPU"),
        }
    }
}

#[derive(Debug)]
pub struct CpuInfo {
    pub ncli: u32,  // Depth of pushcli (processing with interrupts disabled) nesting
    pub zcli: bool, // Were interrupts disabled before pushcli?
    pub running_task: Option<Task>,
}

impl CpuInfo {
    const fn new() -> Self {
        Self {
            ncli: 0,
            zcli: false, // interrupts are enabled by default
            running_task: None,
        }
    }
}
