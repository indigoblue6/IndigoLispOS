// kernel.c - C Kernel Entry Point for IndigoLispOS

// Type definitions (replacing stdint.h)
typedef unsigned char uint8_t;
typedef unsigned int uint32_t;
typedef unsigned long uint64_t;

// External symbols from linker script
extern uint32_t __heap_start;
extern uint32_t __heap_end;

// External Rust entry point
extern void rust_entry(void* heap_start, void* heap_end);

// UART functions (implemented in uart.c)
extern void uart_init(void);
extern void uart_puts(const char* str);

// MMU functions (implemented in mmu.c)
extern void mmu_init(void);

void kernel_main(void) {
    // Initialize UART for debugging
    uart_init();
    uart_puts("IndigoLispOS v0.1\n");
    uart_puts("Initializing...\n");

    // Initialize MMU (minimal setup)
    uart_puts("Setting up MMU...\n");
    mmu_init();

    // Pass control to Rust
    uart_puts("Jumping to Rust...\n");
    rust_entry(&__heap_start, &__heap_end);

    // Should never reach here
    uart_puts("ERROR: Rust returned!\n");
    while(1) {
        asm volatile("wfe");
    }
}
