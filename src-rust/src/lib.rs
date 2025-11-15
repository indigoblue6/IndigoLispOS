#![no_std]
#![no_main]

extern crate alloc;

mod allocator;
mod panic;
mod interrupt;
mod scheduler;

#[path = "../../drivers/mod.rs"]
mod drivers;

#[path = "../../lisp/mod.rs"]
mod lisp;

use core::ptr;
use allocator::ALLOCATOR;

// Rust entry point called from C
#[no_mangle]
pub extern "C" fn rust_entry(heap_start: *mut u8, heap_end: *mut u8) -> ! {
    // Initialize the global allocator
    let heap_size = heap_end as usize - heap_start as usize;
    unsafe {
        ALLOCATOR.init(heap_start, heap_size);
    }

    // Test allocation immediately
    print_str("Testing allocator...\n");
    {
        use alloc::vec::Vec;
        let mut test_vec = Vec::new();
        test_vec.push(1);
        test_vec.push(2);
        test_vec.push(3);
        print_str("Allocator test passed\n");
    }

    // Print startup message
    print_str("Rust runtime initialized\n");
    print_str("Heap: ");
    print_hex(heap_start as usize);
    print_str(" - ");
    print_hex(heap_end as usize);
    print_str("\n");

    // Initialize kernel subsystems
    kernel_init();

    // Enter main loop
    kernel_main_loop();
}

fn kernel_init() {
    drivers::uart::UART.puts("Initializing kernel subsystems...\n");
    
    // Initialize interrupt system
    drivers::uart::UART.puts("Initializing interrupts...\n");
    interrupt::init();
    drivers::uart::UART.puts("Interrupts enabled\n");
    
    // Initialize timer interrupt
    drivers::uart::UART.puts("Initializing timer interrupt...\n");
    drivers::timer::TIMER.init_interrupt();
    drivers::uart::UART.puts("Timer interrupt ready\n");
    
    drivers::uart::UART.puts("GPIO ready\n");
    drivers::uart::UART.puts("Timer ready\n");
    
    // Test GPIO - blink LED on pin 21
    drivers::uart::UART.puts("Setting GPIO function...\n");
    drivers::gpio::GPIO.set_function(21, drivers::gpio::GpioFunction::Output);
    drivers::uart::UART.puts("GPIO function set\n");
    
    drivers::uart::UART.puts("Setting GPIO high...\n");
    drivers::gpio::GPIO.set(21);
    drivers::uart::UART.puts("GPIO set high\n");
    
    drivers::uart::UART.puts("Delaying...\n");
    drivers::timer::TIMER.delay_ms(100);
    drivers::uart::UART.puts("Delay done\n");
    
    drivers::uart::UART.puts("Clearing GPIO...\n");
    drivers::gpio::GPIO.clear(21);
    drivers::uart::UART.puts("GPIO cleared\n");
    
    drivers::uart::UART.puts("Kernel initialized!\n");
}

fn kernel_main_loop() -> ! {
    drivers::uart::UART.puts("Entering main loop...\n");
    drivers::uart::UART.puts("IndigoLispOS is ready!\n");
    drivers::uart::UART.puts("\nWelcome to IndigoLispOS REPL v0.3\n");
    drivers::uart::UART.puts("Features: Interrupts, Task Scheduler, Lambda, Macros\n");
    drivers::uart::UART.puts("New: (spawn fn), (task-id), (sleep ms), (ticks)\n");
    drivers::uart::UART.puts("Type S-expressions to evaluate\n\n");

    // Create Lisp evaluator and REPL editor
    let mut evaluator = lisp::Evaluator::new();
    let mut repl = lisp::ReplEditor::new();
    
    // Multi-line input buffer
    let mut multi_line_buffer = heapless::String::<512>::new();
    
    loop {
        // Determine prompt based on multi-line state
        let prompt = if multi_line_buffer.is_empty() {
            "> "
        } else {
            "... "
        };
        drivers::uart::UART.puts(prompt);
        
        // Read a line with advanced features
        let line = repl.read_line(
            || drivers::uart::UART.getc(),
            &|c| drivers::uart::UART.putc(c)
        );
        
        match line {
            None => {
                // Ctrl+C pressed
                multi_line_buffer.clear();
                continue;
            }
            Some(input) => {
                // Append to multi-line buffer
                if !multi_line_buffer.is_empty() {
                    let _ = multi_line_buffer.push(' ');
                }
                let _ = multi_line_buffer.push_str(&input);
                
                // Check if input is complete
                if !lisp::is_balanced(&multi_line_buffer) {
                    // Continue reading
                    continue;
                }
                
                // Parse and evaluate
                let input_str = multi_line_buffer.as_str();
                let mut parser = lisp::Parser::new(input_str);
                
                match parser.parse() {
                    Ok(expr) => {
                        match evaluator.eval(&expr) {
                            Ok(result) => {
                                // Simple output without allocation
                                match result {
                                    lisp::Expr::Number(n) => {
                                        if n < 0 {
                                            drivers::uart::UART.puts("-");
                                            print_decimal((-n) as usize);
                                        } else {
                                            print_decimal(n as usize);
                                        }
                                        drivers::uart::UART.puts("\n");
                                    }
                                    lisp::Expr::Bool(b) => {
                                        if b {
                                            drivers::uart::UART.puts("true\n");
                                        } else {
                                            drivers::uart::UART.puts("false\n");
                                        }
                                    }
                                    lisp::Expr::Nil => {
                                        drivers::uart::UART.puts("nil\n");
                                    }
                                    lisp::Expr::Lambda(..) => {
                                        drivers::uart::UART.puts("<lambda>\n");
                                    }
                                    lisp::Expr::Macro(..) => {
                                        drivers::uart::UART.puts("<macro>\n");
                                    }
                                    lisp::Expr::String(s) => {
                                        drivers::uart::UART.puts("\"");
                                        drivers::uart::UART.puts(&s);
                                        drivers::uart::UART.puts("\"\n");
                                    }
                                    lisp::Expr::Symbol(s) => {
                                        drivers::uart::UART.puts(&s);
                                        drivers::uart::UART.puts("\n");
                                    }
                                    _ => {
                                        drivers::uart::UART.puts("<result>\n");
                                    }
                                }
                            }
                            Err(e) => {
                                drivers::uart::UART.puts("Error: ");
                                drivers::uart::UART.puts(e);
                                drivers::uart::UART.puts("\n");
                            }
                        }
                    }
                    Err(e) => {
                        drivers::uart::UART.puts("Parse error: ");
                        drivers::uart::UART.puts(&e);
                        drivers::uart::UART.puts("\n");
                    }
                }
                
                // Clear multi-line buffer for next input
                multi_line_buffer.clear();
            }
        }
    }
}

// Simple UART output functions (will be replaced with proper driver)
fn print_str(s: &str) {
    for byte in s.bytes() {
        uart_putc(byte);
    }
}

fn print_hex(n: usize) {
    const HEX_CHARS: &[u8] = b"0123456789ABCDEF";
    uart_putc(b'0');
    uart_putc(b'x');
    
    for i in (0..16).rev() {
        let digit = ((n >> (i * 4)) & 0xF) as usize;
        uart_putc(HEX_CHARS[digit]);
    }
}

fn print_decimal(mut n: usize) {
    if n == 0 {
        uart_putc(b'0');
        return;
    }
    
    let mut digits = [0u8; 20];
    let mut i = 0;
    
    while n > 0 {
        digits[i] = (n % 10) as u8 + b'0';
        n /= 10;
        i += 1;
    }
    
    while i > 0 {
        i -= 1;
        uart_putc(digits[i]);
    }
}

fn uart_putc(c: u8) {
    const UART0_BASE: usize = 0x107d001000;
    const UART0_DR: *mut u32 = (UART0_BASE + 0x00) as *mut u32;
    const UART0_FR: *mut u32 = (UART0_BASE + 0x18) as *mut u32;
    const UART_FR_TXFF: u32 = 1 << 5;

    unsafe {
        // Wait until TX FIFO is not full
        while ptr::read_volatile(UART0_FR) & UART_FR_TXFF != 0 {}
        ptr::write_volatile(UART0_DR, c as u32);
    }
}
