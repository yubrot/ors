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
