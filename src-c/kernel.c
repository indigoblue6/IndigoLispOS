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
extern void uart_puthex64(unsigned long v);
extern void uart_puthex32(unsigned int v);

// Probe helper: read 32-bit words at regular intervals and print
static void mmio_probe_range(unsigned long start, unsigned long end) {
    const unsigned long step = 0x1000UL; // 4KB steps
    unsigned int suppressed = 0;
    for (unsigned long addr = start; addr < end; addr += step) {
        volatile unsigned int *p = (volatile unsigned int *)addr;
        unsigned int v = *p;
        if (v != 0xFFFFFFFFu) {
            uart_puts("PROBE "); uart_puthex64(addr);
            uart_puts(" : "); uart_puthex32(v);
            uart_puts("\n");
        } else {
            suppressed++;
            // Print a small progress marker for each 1MB scanned
            if (((addr - start) & 0xFFFFF) == 0) {
                uart_puts(".");
            }
        }
    }
    if (suppressed) {
        uart_puts("[mmio_probe_range] suppressed "); uart_puthex32(suppressed); uart_puts(" empty reads\n");
    }
}

// MMU functions (implemented in mmu.c)
extern void mmu_init(void);

void kernel_main(void) {
    // Initialize UART for debugging
    uart_init();
    uart_puts("IndigoLispOS v0.2\n");
    uart_puts("Initializing...\n");
    // Initialize MMU (minimal setup)
    uart_puts("Setting up MMU...\n");
    mmu_init();
    // MMIO probe diagnostics (run after MMU init to avoid data aborts)
    uart_puts("MMIO probe: start (post-MMU)\n");

    // Ranges to probe (these match the investigation plan)
    mmio_probe_range(0x10000000UL, 0x10800000UL);      // BCM2712 mailbox / periph area
    mmio_probe_range(0x107C0000UL, 0x107C1000UL);      // Circle / RP1 mailbox candidate page
    mmio_probe_range(0x1F000000UL, 0x1F100000UL);      // RP1 / PCIe outbound region
    mmio_probe_range(0x100000000UL, 0x100100000UL);    // High 64-bit peripheral area

    uart_puts("MMIO probe: end\n");

    // Additional targeted probes: exact mailbox offsets and PCIe high 40-bit region
    uart_puts("MMIO extra probe: exact addresses (suppressed 0xFFFFFFFF)\n");
    unsigned int suppressed_exact = 0;
    // Mailbox candidate (computed previously as PERIPHERAL_BASE + 0xB880)
    volatile unsigned int *pm1 = (volatile unsigned int *)0x000000010000B880UL;
    unsigned int vpm1 = *pm1;
    if (vpm1 != 0xFFFFFFFFu) {
        uart_puts("MAILBOX @"); uart_puthex64(0x000000010000B880UL); uart_puts(" -> "); uart_puthex32(vpm1); uart_puts("\n");
    } else {
        suppressed_exact++;
    }
    // Circle RP1 mailbox candidate
    volatile unsigned int *pm2 = (volatile unsigned int *)0x00000000107C8000UL;
    unsigned int vpm2 = *pm2;
    if (vpm2 != 0xFFFFFFFFu) {
        uart_puts("MAILBOX_CIRCLE @"); uart_puthex64(0x00000000107C8000UL); uart_puts(" -> "); uart_puthex32(vpm2); uart_puts("\n");
    } else {
        suppressed_exact++;
    }
    if (suppressed_exact) {
        uart_puts("MMIO extra probe: suppressed "); uart_puthex32(suppressed_exact); uart_puts(" empty exact reads\n");
    }

    // Probe the PCIe outbound CPU base at 0x1F00000000 (40-bit address)
    // Keep this small (4 pages) to avoid huge logs
    mmio_probe_range(0x00000001F00000000UL, 0x00000001F00010000UL);
    uart_puts("MMIO extra probe: end\n");

    // Pass control to Rust
    uart_puts("Jumping to Rust...\n");
    rust_entry(&__heap_start, &__heap_end);

    // Should never reach here
    uart_puts("ERROR: Rust returned!\n");
    while(1) {
        asm volatile("wfe");
    }
}
