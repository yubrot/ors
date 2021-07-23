mod asm {
    pub use x86_64::instructions::port::Port;
}

static mut QEMU_DEBUG_EXIT: asm::Port<u32> = asm::Port::new(0xf4);

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
#[repr(u32)]
pub enum ExitCode {
    Success = 0x10,
    Failure = 0x11,
}

pub fn exit(exit_code: ExitCode) {
    unsafe { QEMU_DEBUG_EXIT.write(exit_code as u32) }
}
