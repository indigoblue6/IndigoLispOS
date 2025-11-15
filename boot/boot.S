// boot.S - AArch64 Boot Code for IndigoLispOS
// Entry point for Raspberry Pi 5

.section ".text.boot"

.global _start

_start:
    // Read CPU ID - only CPU 0 should proceed
    mrs     x0, mpidr_el1
    and     x0, x0, #0xFF
    cbz     x0, master_cpu
    
    // Other CPUs: infinite loop
halt:
    wfe
    b       halt

master_cpu:
    // Set stack pointer
    ldr     x0, =__stack_top
    mov     sp, x0

    // Clear BSS section
    ldr     x0, =__bss_start
    ldr     x1, =__bss_end
    sub     x1, x1, x0
    bl      memzero

    // Jump to C kernel
    bl      kernel_main

    // If kernel_main returns, halt
    b       halt

// memzero - Zero out memory region
// x0 = start address, x1 = length
memzero:
    cbz     x1, memzero_done
memzero_loop:
    str     xzr, [x0], #8
    sub     x1, x1, #8
    cbnz    x1, memzero_loop
memzero_done:
    ret
