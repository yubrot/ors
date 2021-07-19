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

global load_gdt ; load_gdt(limit: u16, ofs: u64)
load_gdt:
  push rbp
  mov rbp, rsp
  sub rsp, 10
  mov [rsp], di      ; limit
  mov [rsp + 2], rsi ; offset
  lgdt [rsp]
  mov rsp, rbp
  pop rbp
  ret

global set_segment_registers ; set_segment_registers(ds_es_fs_gs: u16, cs: u16, ss: u16)
set_segment_registers:
  push rbp
  mov rbp, rsp
  mov ds, di
  mov es, di
  mov fs, di
  mov gs, di
  mov ss, dx ; initialize differently from DS/ES to make compatible with sycall
  mov rax, .next
  push rsi ; CS
  push rax ; RIP
  o64 retf ; use far return to set CS
.next:
  mov rsp, rbp
  pop rbp
  ret

global set_cr3 ; set_cr3(address: u64)
set_cr3:
  mov cr3, rdi
  ret
