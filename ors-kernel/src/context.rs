use crate::segmentation;
use core::mem;
use core::sync::atomic::{AtomicBool, Ordering};

#[repr(C, align(16))]
#[derive(Debug)]
pub struct Context {
    pub cr3: u64,               // 0x00
    pub rip: u64,               // 0x08
    pub rflags: u64,            // 0x10
    pub _reserved1: u64,        // 0x18
    pub cs: u64,                // 0x20
    pub ss: u64,                // 0x28
    pub fs: u64,                // 0x30
    pub gs: u64,                // 0x38
    pub rax: u64,               // 0x40
    pub rbx: u64,               // 0x48
    pub rcx: u64,               // 0x50
    pub rdx: u64,               // 0x58
    pub rdi: u64,               // 0x60
    pub rsi: u64,               // 0x68
    pub rsp: u64,               // 0x70
    pub rbp: u64,               // 0x78
    pub r8: u64,                // 0x80
    pub r9: u64,                // 0x88
    pub r10: u64,               // 0x90
    pub r11: u64,               // 0x98
    pub r12: u64,               // 0xa0
    pub r13: u64,               // 0xa8
    pub r14: u64,               // 0xb0
    pub r15: u64,               // 0xb8
    pub fxsave_area: [u8; 512], // 0xc0
    /// Used to confirm the end of the context saving process
    pub saved: AtomicBool, // 0x2c0
}

impl Context {
    pub const INTERRUPT_FLAG: u64 = 0x200; // Maskable interrupt enabled

    pub fn new<E: EntryPoint>(stack_end: *mut u8, entry_point: E, args: E::Arg) -> Self {
        let mut ctx = Self::uninitialized();
        ctx.cr3 = unsafe { get_cr3() };
        ctx.rflags = Self::INTERRUPT_FLAG | 0x2; // bit=1 is always 1 in eflags
        ctx.cs = unsafe { mem::transmute::<_, u16>(segmentation::cs()) } as u64;
        ctx.ss = unsafe { mem::transmute::<_, u16>(segmentation::ss()) } as u64;
        ctx.rsp = stack_end as u64 & !0xf; // 16-byte aligned for sysv64
        ctx.rsp -= 8; // adjust to call
        unsafe { *(&mut ctx.fxsave_area[24] as *mut u8 as *mut u32) = 0x1f80 }; // mask all MXCSR exceptions
        entry_point.prepare_context(&mut ctx, args);
        ctx.saved.store(true, Ordering::SeqCst);
        ctx
    }

    /// Used to write a context that is currently running.
    /// Switching to an uninitialized context is undefined behavior.
    pub fn uninitialized() -> Self {
        Self {
            cr3: 0,
            rip: 0,
            rflags: 0,
            _reserved1: 0,
            cs: 0,
            ss: 0,
            fs: 0,
            gs: 0,
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rdi: 0,
            rsi: 0,
            rsp: 0,
            rbp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            fxsave_area: [0; 512],
            saved: AtomicBool::new(false),
        }
    }

    /// Mark the context as not saved.
    pub fn mark_as_not_saved(&self) {
        self.saved.store(false, Ordering::SeqCst);
    }

    /// Wait until the context has been saved.
    pub fn wait_saved(&self) {
        while !self.saved.load(Ordering::Relaxed) {
            core::hint::spin_loop();
        }
    }

    /// Perform context switching. The current context will be saved to `current_ctx`.
    pub unsafe fn switch(next_ctx: *const Self, current_ctx: *mut Self) {
        switch_context(next_ctx, current_ctx);
    }
}

extern "C" {
    fn get_cr3() -> u64;
    fn switch_context(next_ctx: *const Context, current_ctx: *mut Context);
}

pub trait EntryPoint {
    type Arg;
    fn prepare_context(self, ctx: &mut Context, arg: Self::Arg);
}
