#![no_std]
#![no_main]

extern crate alloc;

mod allocator;
mod panic;
mod interrupt;
mod scheduler;
mod network;
mod hotdeploy;

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
    
    // Initialize PCIe
    drivers::uart::UART.puts("Initializing PCIe...\n");
    let mut rp1_initialized = false;
    if let Err(e) = drivers::pcie::init_pcie() {
        drivers::uart::UART.puts("PCIe init failed: ");
        drivers::uart::UART.puts(e);
        drivers::uart::UART.puts("\n");
    } else {
        drivers::uart::UART.puts("PCIe initialized\n");
        
        // Find and enable RP1
        if let Some(pcie) = drivers::pcie::get_pcie() {
            if let Some(mut rp1) = pcie.find_rp1() {
                if let Err(e) = rp1.enable() {
                    drivers::uart::UART.puts("RP1 enable failed: ");
                    drivers::uart::UART.puts(e);
                    drivers::uart::UART.puts("\n");
                } else {
                    // Initialize RP1 Ethernet
                    drivers::uart::UART.puts("Initializing RP1 Ethernet...\n");
                    let rp1_base = rp1.get_bar1_base();
                    let mac = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
                    if let Err(e) = drivers::rp1_ethernet::init_rp1_ethernet(rp1_base, mac) {
                        drivers::uart::UART.puts("RP1 Ethernet init failed: ");
                        drivers::uart::UART.puts(e);
                        drivers::uart::UART.puts("\n");
                    } else {
                        drivers::uart::UART.puts("RP1 Ethernet initialized\n");
                        rp1_initialized = true;
                    }
                }
            } else {
                drivers::uart::UART.puts("WARNING: RP1 not found - network disabled\n");
            }
        }
    }
    
    // Initialize network stack only if RP1 is available
    if rp1_initialized {
        drivers::uart::UART.puts("Initializing network stack...\n");
        use smoltcp::wire::Ipv4Address;
        let ip = Ipv4Address::new(192, 168, 10, 110);
        let gateway = Ipv4Address::new(192, 168, 10, 1);
        network::init_network([0x02, 0x00, 0x00, 0x00, 0x00, 0x01], ip, gateway);
        drivers::uart::UART.puts("Network stack ready (192.168.10.110)\n");
        
        // Initialize hot deploy receiver
        drivers::uart::UART.puts("Initializing hot deploy...\n");
        if let Ok(()) = hotdeploy::init_hotdeploy() {
            drivers::uart::UART.puts("Hot deploy ready on port 8888\n");
        } else {
            drivers::uart::UART.puts("Hot deploy initialization failed\n");
        }
    } else {
        drivers::uart::UART.puts("Network stack disabled (RP1 not available)\n");
    }
    
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
    drivers::uart::UART.puts("\n");
    
    // Display build information
    hotdeploy::print_build_info();
    
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
        // Poll network stack and hot deploy continuously
        let timestamp_ms = drivers::timer::TIMER.get_ticks();
        if let Some(stack) = network::get_network_stack() {
            stack.poll(timestamp_ms);
        }
        hotdeploy::poll_hotdeploy(timestamp_ms);
        
        // Determine prompt based on multi-line state
        let prompt = if multi_line_buffer.is_empty() {
            "> "
        } else {
            "... "
        };
        drivers::uart::UART.puts(prompt);
        
        // Read a line with advanced features (modified to poll network)
        let line = {
            // Get current environment bindings for tab completion
            let env_bindings = evaluator.get_binding_names();
            repl.read_line(
                || {
                    // Non-blocking getc with network polling
                    loop {
                        if let Some(c) = drivers::uart::UART.try_getc() {
                            return c;
                        }
                        // Poll network while waiting for input
                        let ts = drivers::timer::TIMER.get_ticks();
                        if let Some(stack) = network::get_network_stack() {
                            stack.poll(ts);
                        }
                        hotdeploy::poll_hotdeploy(ts);
                    }
                },
                &|c| drivers::uart::UART.putc(c),
                &env_bindings
            )
        };
        
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

fn print_dec(n: usize) {
    print_decimal(n);
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
