// interrupt.rs - Interrupt handling for IndigoLispOS
// Provides IRQ handler called from assembly

use crate::drivers::timer;

// External assembly functions
extern "C" {
    fn init_exception_vectors();
}

// Initialize interrupt system
pub fn init() {
    unsafe {
        init_exception_vectors();
    }
    
    // Enable IRQ
    unsafe {
        core::arch::asm!(
            "msr daifclr, #2"
        );
    }
}

// IRQ handler called from assembly
#[no_mangle]
pub extern "C" fn irq_handler() {
    // Handle timer interrupt
    timer::handle_interrupt();
    // Handle RP1 Ethernet IRQ (if any). This is a minimal dispatch:
    // rp1_eth_irq_handler will check status and poll the network stack.
    crate::drivers::rp1_ethernet::rp1_eth_irq_handler();
    
    // Call scheduler for task switching (if enabled in future)
    // Currently scheduler is not integrated with IRQ
}
