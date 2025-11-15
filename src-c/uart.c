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

char uart_getc(void) {
    // Wait until RX FIFO is not empty
    while (*UART0_FR & UART_FR_RXFE);
    return (char)(*UART0_DR & 0xFF);
}
