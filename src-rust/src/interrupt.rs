// interrupt.rs - Interrupt handling for IndigoLispOS
// Provides IRQ handler called from assembly



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
    unsafe {
        crate::drivers::gic::gic_handle_irq();
    }
}
