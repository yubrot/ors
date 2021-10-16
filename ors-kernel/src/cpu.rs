//! This module works on the assumption that the processor information is initialized by
//! calling `initialize` before any processor other than BSP is enabled.

use crate::acpi;
use crate::task::Task;
use crate::x64;
use ors_common::non_contiguous::Array;
use spin::{Mutex, Once};

static SYSTEM_INFO: Once<SystemInfo> = Once::new();
static BOOT_STRAP_CPU_STATE: Mutex<CpuState> = Mutex::new(CpuState::new());

#[derive(Debug)]
struct SystemInfo {
    lapic: x64::LApic,
    boot_strap_lapic_id: u32,
    application_cpu_state: Array<u32, Mutex<CpuState>, 64>,
}

pub fn initialize() {
    SYSTEM_INFO.call_once(move || {
        let processor_info = acpi::processor_info();
        let mut application_cpu_state = Array::new();
        for ap in processor_info.application_processors.iter() {
            application_cpu_state.insert(ap.local_apic_id, Mutex::new(CpuState::new()));
        }
        SystemInfo {
            lapic: x64::LApic::new(acpi::apic_info().local_apic_address),
            boot_strap_lapic_id: processor_info.boot_processor.local_apic_id,
            application_cpu_state,
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
            info.application_cpu_state
                .iter()
                .map(|(lapic_id, _)| Self::Application(*lapic_id))
        }))
    }

    /// Get the state of this CPU.
    /// This Mutex does not get interrupt lock (`crate::interrupts::Cli`). Moreover, acquiring and
    /// releasing `crate::sync::mutex::Mutex` will lock this mutex through interrupt lock.
    /// We need to be careful about deadlocks when using this method.
    pub fn state(self) -> &'static Mutex<CpuState> {
        match self {
            Self::BootStrap => &BOOT_STRAP_CPU_STATE,
            Self::Application(lapic_id) => SYSTEM_INFO
                .wait() // works under the module assumption
                .application_cpu_state
                .get(lapic_id)
                .expect("Unknown CPU"),
        }
    }
}

#[derive(Debug)]
pub struct CpuState {
    pub running_task: Option<Task>,
    pub thread_state: CpuThreadState,
}

impl CpuState {
    const fn new() -> Self {
        Self {
            running_task: None,
            thread_state: CpuThreadState::new(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpuThreadState {
    pub ncli: u32,  // Depth of pushcli (processing with interrupts disabled) nesting
    pub zcli: bool, // Were interrupts disabled before pushcli?
}

impl CpuThreadState {
    pub const fn new() -> Self {
        Self {
            ncli: 0,
            zcli: false, // interrupts are enabled by default
        }
    }
}
