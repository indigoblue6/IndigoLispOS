/* Linker script for IndigoLispOS on AArch64 */

ENTRY(_start)

/* Define memory regions. Keep kernel image at the usual low address
   while creating a separate DRAM_REGION where DMA-only sections
   (.dram, .dma, etc.) will be placed. DRAM_REGION is marked for the
   real physical DRAM base used by RP1 DMA masters. */
MEMORY {
    KERNEL (rxw) : ORIGIN = 0x0000000000200000, LENGTH = 16M
    DRAM_REGION (rw) : ORIGIN = 0x0000000080000000, LENGTH = 512M
}

SECTIONS
{
    /* Start the kernel image at 0x200000 as before */
    . = 0x200000;

    .text : {
        KEEP(*(.text.boot))
        *(.text*)
    } >KERNEL

    .rodata : {
        *(.rodata*)
    } >KERNEL

    .data : {
        *(.data*)
    } >KERNEL

    .bss : {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        __bss_end = .;
    } >KERNEL

    /* DMA data grouping: place only DMA-related sections into the
       DRAM region as a separate VMA. Use NOLOAD so the flash/ROM image
       does not contain these contents (they are initialized at runtime).
       Align to page (4096) to be friendly with MMU mappings. */
    .dram (NOLOAD) : ALIGN(4096) {
        __dram_start = .;
        KEEP(*(.dram*))
        *(.dram*)
        __dram_end = .;
    } >DRAM_REGION

    /* Optional dedicated DMA section (recommended): place explicit
       DMA buffers here from Rust via `#[link_section = ".dma"]`. */
    .dma (NOLOAD) : ALIGN(4096) {
        __dma_start = .;
        KEEP(*(.dma*))
        *(.dma*)
        __dma_end = .;
    } >DRAM_REGION

    /* Stack grows downward from here */
    . = ALIGN(16);
    . = . + 0x100000; /* 1MB stack */
    __stack_top = .;

    /* Heap for Rust allocator */
    . = ALIGN(16);
    __heap_start = .;
    . = . + 0x1000000; /* 16MB heap */
    __heap_end = .;
}
