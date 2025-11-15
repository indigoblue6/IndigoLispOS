// timer.rs - System Timer Driver for Raspberry Pi 5

use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};

// System Timer base address (BCM2712)
// 1MHz counter
const TIMER_BASE: usize = 0xfe003000;

// Timer registers
const TIMER_CS: *mut u32 = (TIMER_BASE + 0x00) as *mut u32;
const TIMER_CLO: *mut u32 = (TIMER_BASE + 0x04) as *mut u32;
const TIMER_CHI: *mut u32 = (TIMER_BASE + 0x08) as *mut u32;
const TIMER_C0: *mut u32 = (TIMER_BASE + 0x0C) as *mut u32;
const TIMER_C1: *mut u32 = (TIMER_BASE + 0x10) as *mut u32;

// ARM Generic Timer registers (for interrupts)
const CNTFRQ_EL0: u64 = 54000000; // 54MHz on RPi5

// Global tick counter
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

pub struct Timer;

impl Timer {
    pub fn new() -> Self {
        Timer
    }

    /// Get current time in microseconds (lower 32 bits)
    pub fn get_time(&self) -> u32 {
        unsafe { ptr::read_volatile(TIMER_CLO) }
    }

    /// Get current time in microseconds (full 64 bits)
    pub fn get_time_64(&self) -> u64 {
        unsafe {
            let hi = ptr::read_volatile(TIMER_CHI) as u64;
            let lo = ptr::read_volatile(TIMER_CLO) as u64;
            (hi << 32) | lo
        }
    }

    /// Busy-wait for specified microseconds
    pub fn delay_us(&self, us: u32) {
        // Use a simple busy loop instead of timer comparison
        // Approximate delay based on CPU cycles
        for _ in 0..(us * 100) {
            unsafe {
                core::arch::asm!("nop");
            }
        }
    }

    /// Busy-wait for specified milliseconds
    pub fn delay_ms(&self, ms: u32) {
        self.delay_us(ms * 1000);
    }
    
    /// Initialize timer interrupt (10ms tick)
    pub fn init_interrupt(&self) {
        // Reset tick counter
        TICK_COUNT.store(0, Ordering::SeqCst);
        
        unsafe {
            // Enable CNTV (virtual timer)
            let mut cntv_ctl: u64;
            core::arch::asm!("mrs {}, cntv_ctl_el0", out(reg) cntv_ctl);
            cntv_ctl |= 1; // Enable
            core::arch::asm!("msr cntv_ctl_el0, {}", in(reg) cntv_ctl);
            
            // Set timer value for 10ms (540000 ticks at 54MHz)
            let ticks: u64 = CNTFRQ_EL0 / 100; // 10ms
            core::arch::asm!("msr cntv_tval_el0, {}", in(reg) ticks);
        }
    }
    
    /// Get tick count (incremented on each timer interrupt)
    pub fn get_ticks(&self) -> u64 {
        TICK_COUNT.load(Ordering::SeqCst)
    }
}

// Global Timer instance
pub static TIMER: Timer = Timer;

// Handle timer interrupt
pub fn handle_interrupt() {
    // Increment tick counter
    TICK_COUNT.fetch_add(1, Ordering::SeqCst);
    
    // Re-arm timer for next 10ms
    unsafe {
        let ticks: u64 = CNTFRQ_EL0 / 100; // 10ms
        core::arch::asm!("msr cntv_tval_el0, {}", in(reg) ticks);
    }
}
