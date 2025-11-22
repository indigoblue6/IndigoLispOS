/* Linker script for IndigoLispOS on AArch64 */

ENTRY(_start)

MEMORY {
    KERNEL      (rxw) : ORIGIN = 0x0000000000200000, LENGTH = 16M
    DRAM_REGION (rw)  : ORIGIN = 0x0000000020000000, LENGTH = 512M
}

SECTIONS {
    . = 0x200000;

    .text : { *(.text*) } >KERNEL
    .rodata : { *(.rodata*) } >KERNEL
    .data : { *(.data*) } >KERNEL

    .bss : {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        __bss_end = .;
    } >KERNEL

    .dma (NOLOAD) : ALIGN(4096) {
        __dma_start = .;
        *(.dma*)
        *(.dram*)
        __dma_end = .;
    } >DMA_REGION

    . = ALIGN(16);
    . += 0x100000;
    __stack_top = .;

    . = ALIGN(16);
    __heap_start = .;
    . += 0x1000000;
    __heap_end = .;
}
