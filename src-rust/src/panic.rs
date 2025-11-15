// panic.rs - Panic handler for kernel

use core::panic::PanicInfo;
use core::ptr;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Print panic message to UART
    print_str("\n\n*** KERNEL PANIC ***\n");
    
    if let Some(location) = info.location() {
        print_str("Location: ");
        print_str(location.file());
        print_str(":");
        print_num(location.line() as usize);
        print_str(":");
        print_num(location.column() as usize);
        print_str("\n");
    }
    
    // Print panic payload if available
    print_str("Message: <panic>\n");
    
    print_str("\nSystem halted.\n");
    
    // Halt the system
    loop {
        unsafe {
            core::arch::asm!("wfe");
        }
    }
}

fn print_str(s: &str) {
    for byte in s.bytes() {
        uart_putc(byte);
    }
}

fn print_num(mut n: usize) {
    if n == 0 {
        uart_putc(b'0');
        return;
    }
    
    let mut buf = [0u8; 20];
    let mut i = 0;
    
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    
    while i > 0 {
        i -= 1;
        uart_putc(buf[i]);
    }
}

fn uart_putc(c: u8) {
    const UART0_BASE: usize = 0x107D001000;
    const UART0_DR: *mut u32 = (UART0_BASE + 0x00) as *mut u32;
    const UART0_FR: *mut u32 = (UART0_BASE + 0x18) as *mut u32;
    const UART_FR_TXFF: u32 = 1 << 5;

    unsafe {
        while ptr::read_volatile(UART0_FR) & UART_FR_TXFF != 0 {}
        ptr::write_volatile(UART0_DR, c as u32);
    }
}
