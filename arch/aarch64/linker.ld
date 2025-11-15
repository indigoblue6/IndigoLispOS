/* Linker script for IndigoLispOS on AArch64 */

ENTRY(_start)

SECTIONS
{
    /* Start at 0x200000 for Raspberry Pi 5 (Ubuntu compatible) */
    . = 0x200000;

    .text : {
        KEEP(*(.text.boot))
        *(.text*)
    }

    .rodata : {
        *(.rodata*)
    }

    .data : {
        *(.data*)
    }

    .bss : {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        __bss_end = .;
    }

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
