// uart.rs - UART Driver for Raspberry Pi 5

use core::ptr;

// BCM2712 UART base address - Pi5 specific
const UART0_BASE: usize = 0x107d001000;

// UART registers
const UART0_DR: *mut u32 = (UART0_BASE + 0x00) as *mut u32;
const UART0_FR: *mut u32 = (UART0_BASE + 0x18) as *mut u32;

// UART FR register bits
const UART_FR_TXFF: u32 = 1 << 5; // Transmit FIFO full
const UART_FR_RXFE: u32 = 1 << 4; // Receive FIFO empty

pub struct Uart;

impl Uart {
    pub fn new() -> Self {
        Uart
    }

    pub fn putc(&self, c: u8) {
        unsafe {
            // Wait until TX FIFO is not full
            while ptr::read_volatile(UART0_FR) & UART_FR_TXFF != 0 {}
            ptr::write_volatile(UART0_DR, c as u32);
        }
    }

    pub fn puts(&self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.putc(b'\r');
            }
            self.putc(byte);
        }
    }

    pub fn getc(&self) -> u8 {
        unsafe {
            // Wait until RX FIFO is not empty
            while ptr::read_volatile(UART0_FR) & UART_FR_RXFE != 0 {}
            (ptr::read_volatile(UART0_DR) & 0xFF) as u8
        }
    }

    pub fn try_getc(&self) -> Option<u8> {
        unsafe {
            if ptr::read_volatile(UART0_FR) & UART_FR_RXFE != 0 {
                None
            } else {
                Some((ptr::read_volatile(UART0_DR) & 0xFF) as u8)
            }
        }
    }
}

use core::fmt::{self, Write};

impl Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.puts(s);
        Ok(())
    }
}

// Wrapper for static UART access with interior mutability
pub struct UartWriter;

impl Write for UartWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        UART.puts(s);
        Ok(())
    }
}

// Global UART instance
pub static UART: Uart = Uart;

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            let _ = write!($crate::drivers::uart::UartWriter, $($arg)*);
        }
    };
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => {
        {
            use core::fmt::Write;
            let _ = write!($crate::drivers::uart::UartWriter, $($arg)*);
            $crate::print!("\n");
        }
    };
}
