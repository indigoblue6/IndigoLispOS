// timer.rs - System Timer Driver for Raspberry Pi 5

use core::ptr;

// System Timer base address (BCM2712)
// 1MHz counter
const TIMER_BASE: usize = 0xfe003000;

// Timer registers
const TIMER_CS: *mut u32 = (TIMER_BASE + 0x00) as *mut u32;
const TIMER_CLO: *mut u32 = (TIMER_BASE + 0x04) as *mut u32;
const TIMER_CHI: *mut u32 = (TIMER_BASE + 0x08) as *mut u32;

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
}

// Global Timer instance
pub static TIMER: Timer = Timer;
