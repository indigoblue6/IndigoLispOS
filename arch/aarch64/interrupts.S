// interrupts.S - AArch64 Interrupt Vector Table and Handlers
// For IndigoLispOS v0.3

.section ".text.vectors"

// Align to 2KB (required for VBAR_EL1)
.align 11

.global exception_vectors
exception_vectors:
    // Current EL with SP0
    .align 7
    b       sync_current_sp0
    .align 7
    b       irq_current_sp0
    .align 7
    b       fiq_current_sp0
    .align 7
    b       serror_current_sp0

    // Current EL with SPx
    .align 7
    b       sync_current_spx
    .align 7
    b       irq_current_spx
    .align 7
    b       fiq_current_spx
    .align 7
    b       serror_current_spx

    // Lower EL using AArch64
    .align 7
    b       sync_lower_a64
    .align 7
    b       irq_lower_a64
    .align 7
    b       fiq_lower_a64
    .align 7
    b       serror_lower_a64

    // Lower EL using AArch32
    .align 7
    b       sync_lower_a32
    .align 7
    b       irq_lower_a32
    .align 7
    b       fiq_lower_a32
    .align 7
    b       serror_lower_a32

// Save all registers to stack
.macro save_context
    sub     sp, sp, #272        // 34 registers * 8 bytes
    stp     x0, x1, [sp, #0]
    stp     x2, x3, [sp, #16]
    stp     x4, x5, [sp, #32]
    stp     x6, x7, [sp, #48]
    stp     x8, x9, [sp, #64]
    stp     x10, x11, [sp, #80]
    stp     x12, x13, [sp, #96]
    stp     x14, x15, [sp, #112]
    stp     x16, x17, [sp, #128]
    stp     x18, x19, [sp, #144]
    stp     x20, x21, [sp, #160]
    stp     x22, x23, [sp, #176]
    stp     x24, x25, [sp, #192]
    stp     x26, x27, [sp, #208]
    stp     x28, x29, [sp, #224]
    str     x30, [sp, #240]
    mrs     x0, elr_el1
    mrs     x1, spsr_el1
    stp     x0, x1, [sp, #248]
.endm

// Restore all registers from stack
.macro restore_context
    ldp     x0, x1, [sp, #248]
    msr     elr_el1, x0
    msr     spsr_el1, x1
    ldp     x0, x1, [sp, #0]
    ldp     x2, x3, [sp, #16]
    ldp     x4, x5, [sp, #32]
    ldp     x6, x7, [sp, #48]
    ldp     x8, x9, [sp, #64]
    ldp     x10, x11, [sp, #80]
    ldp     x12, x13, [sp, #96]
    ldp     x14, x15, [sp, #112]
    ldp     x16, x17, [sp, #128]
    ldp     x18, x19, [sp, #144]
    ldp     x20, x21, [sp, #160]
    ldp     x22, x23, [sp, #176]
    ldp     x24, x25, [sp, #192]
    ldp     x26, x27, [sp, #208]
    ldp     x28, x29, [sp, #224]
    ldr     x30, [sp, #240]
    add     sp, sp, #272
.endm

// Exception handlers

sync_current_sp0:
    b       hang

irq_current_sp0:
    save_context
    bl      irq_handler
    restore_context
    eret

fiq_current_sp0:
    b       hang

serror_current_sp0:
    b       hang

sync_current_spx:
    b       hang

irq_current_spx:
    save_context
    bl      irq_handler
    restore_context
    eret

fiq_current_spx:
    b       hang

serror_current_spx:
    b       hang

sync_lower_a64:
    b       hang

irq_lower_a64:
    save_context
    bl      irq_handler
    restore_context
    eret

fiq_lower_a64:
    b       hang

serror_lower_a64:
    b       hang

sync_lower_a32:
    b       hang

irq_lower_a32:
    b       hang

fiq_lower_a32:
    b       hang

serror_lower_a32:
    b       hang

hang:
    wfe
    b       hang

// Initialize exception vectors
.global init_exception_vectors
init_exception_vectors:
    ldr     x0, =exception_vectors
    msr     vbar_el1, x0
    isb
    ret

// Context switch helper
.global switch_context
switch_context:
    // x0 = &current_task->context
    // x1 = &next_task->context
    
    // Save current context
    stp     x19, x20, [x0, #0]
    stp     x21, x22, [x0, #16]
    stp     x23, x24, [x0, #32]
    stp     x25, x26, [x0, #48]
    stp     x27, x28, [x0, #64]
    stp     x29, x30, [x0, #80]
    mov     x2, sp
    str     x2, [x0, #96]
    
    // Restore next context
    ldp     x19, x20, [x1, #0]
    ldp     x21, x22, [x1, #16]
    ldp     x23, x24, [x1, #32]
    ldp     x25, x26, [x1, #48]
    ldp     x27, x28, [x1, #64]
    ldp     x29, x30, [x1, #80]
    ldr     x2, [x1, #96]
    mov     sp, x2
    
    ret
