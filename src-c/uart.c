// uart.c - UART Driver for Raspberry Pi 5 (PL011)

// Type definitions (replacing stdint.h)
typedef unsigned char uint8_t;
typedef unsigned int uint32_t;
typedef unsigned long uint64_t;

// Raspberry Pi 5 UART0 base address (BCM2712)
// Reference: pi5_os early_uart implementation
#define UART0_BASE 0x107d001000UL

// UART registers
#define UART0_DR     ((volatile uint32_t*)(UART0_BASE + 0x00))
#define UART0_FR     ((volatile uint32_t*)(UART0_BASE + 0x18))
#define UART0_IBRD   ((volatile uint32_t*)(UART0_BASE + 0x24))
#define UART0_FBRD   ((volatile uint32_t*)(UART0_BASE + 0x28))
#define UART0_LCRH   ((volatile uint32_t*)(UART0_BASE + 0x2C))
#define UART0_CR     ((volatile uint32_t*)(UART0_BASE + 0x30))
#define UART0_IMSC   ((volatile uint32_t*)(UART0_BASE + 0x38))
#define UART0_ICR    ((volatile uint32_t*)(UART0_BASE + 0x44))

// UART FR register bits
#define UART_FR_TXFF (1 << 5)  // Transmit FIFO full
#define UART_FR_RXFE (1 << 4)  // Receive FIFO empty

void uart_init(void) {
    // Pi5 UART is already initialized by bootloader
    // No initialization needed for early UART
    // Reference: pi5_os early_uart implementation
}

void uart_putc(char c) {
    // Wait until TX FIFO is not full
    while (*UART0_FR & UART_FR_TXFF);
    *UART0_DR = c;
}

void uart_puts(const char* str) {
    while (*str) {
        if (*str == '\n') {
            uart_putc('\r');
        }
        uart_putc(*str++);
    }
}

// Helper: print a 4-bit nibble as hex
static void uart_puthex_nibble(unsigned int v) {
    unsigned int nib = v & 0xF;
    if (nib < 10) uart_putc('0' + nib);
    else uart_putc('a' + (nib - 10));
}

// Print 32-bit value as 0xhhhhhhhh
void uart_puthex32(unsigned int v) {
    uart_puts("0x");
    for (int i = 7; i >= 0; --i) {
        unsigned int shift = i * 4;
        uart_puthex_nibble((v >> shift) & 0xF);
    }
}

// Print 64-bit value as 0xhhhhhhhhhhhhhhhh
void uart_puthex64(unsigned long v) {
    uart_puts("0x");
    for (int i = 15; i >= 0; --i) {
        unsigned long shift = (unsigned long)i * 4UL;
        unsigned int nib = (unsigned int)((v >> shift) & 0xFUL);
        if (nib < 10) uart_putc('0' + nib);
        else uart_putc('a' + (nib - 10));
    }
}

char uart_getc(void) {
    // Wait until RX FIFO is not empty
    while (*UART0_FR & UART_FR_RXFE);
    return (char)(*UART0_DR & 0xFF);
}
