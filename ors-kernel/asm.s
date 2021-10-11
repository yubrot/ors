; System V AMD64 Calling Convention
; Registers: RDI, RSI, RDX, RCX, R8, R9

bits 64

extern kernel_main2

section .bss align=16
kernel_main_stack:
  resb 1024 * 1024

section .text
global kernel_main
kernel_main:
  mov rsp, kernel_main_stack + 1024 * 1024
  call kernel_main2
.fin:
  hlt
  jmp .fin

global get_cr3 ; fn get_cr3() -> u64;
get_cr3:
  mov rax, cr3
  ret

global switch_context
switch_context: ; fn switch_context(next_ctx: *const Context, current_ctx: *mut Context);
  ; Save
  mov [rsi + 0x40], rax
  mov [rsi + 0x48], rbx
  mov [rsi + 0x50], rcx
  mov [rsi + 0x58], rdx
  mov [rsi + 0x60], rdi
  mov [rsi + 0x68], rsi
  lea rax, [rsp + 8]    ; Save RSP by removing the offset of the return address (which was pushed by call inst)
  mov [rsi + 0x70], rax ; -> current_ctx.rsp
  mov [rsi + 0x78], rbp
  mov [rsi + 0x80], r8
  mov [rsi + 0x88], r9
  mov [rsi + 0x90], r10
  mov [rsi + 0x98], r11
  mov [rsi + 0xa0], r12
  mov [rsi + 0xa8], r13
  mov [rsi + 0xb0], r14
  mov [rsi + 0xb8], r15
  mov rax, cr3
  mov [rsi + 0x00], rax
  mov rax, [rsp]        ; Load the return address (which was pushed by call inst)
  mov [rsi + 0x08], rax ; -> current_ctx.rip
  pushfq
  pop qword [rsi + 0x10]
  mov ax, cs
  mov [rsi + 0x20], rax
  mov bx, ss
  mov [rsi + 0x28], rbx
  mov cx, fs
  mov [rsi + 0x30], rcx
  mov dx, gs
  mov [rsi + 0x38], rdx
  fxsave [rsi + 0xc0]
  ; Mark as saved
  mov al, 1
  xchg [rsi + 0x2c0], al
  ; Restore
  ; Build an stack frame for iret to switch the context
  push qword [rdi + 0x28] ; SS
  push qword [rdi + 0x70] ; RSP
  push qword [rdi + 0x10] ; RFLAGS
  push qword [rdi + 0x20] ; CS
  push qword [rdi + 0x08] ; RIP
  ; Inverse of save
  fxrstor [rdi + 0xc0]
  mov rax, [rdi + 0x00]
  mov cr3, rax
  mov rax, [rdi + 0x30]
  mov fs, ax
  mov rax, [rdi + 0x38]
  mov gs, ax
  mov rax, [rdi + 0x40]
  mov rbx, [rdi + 0x48]
  mov rcx, [rdi + 0x50]
  mov rdx, [rdi + 0x58]
  mov rsi, [rdi + 0x68]
  mov rbp, [rdi + 0x78]
  mov r8,  [rdi + 0x80]
  mov r9,  [rdi + 0x88]
  mov r10, [rdi + 0x90]
  mov r11, [rdi + 0x98]
  mov r12, [rdi + 0xa0]
  mov r13, [rdi + 0xa8]
  mov r14, [rdi + 0xb0]
  mov r15, [rdi + 0xb8]
  mov rdi, [rdi + 0x60]
  o64 iret
