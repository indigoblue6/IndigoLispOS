// gpio.rs - GPIO Driver for Raspberry Pi 5

use core::ptr;

// Raspberry Pi 5 GPIO base address (RP1 chip)
// Reference: Ubuntu linux-raspi pinctrl-rp1.c
const GPIO_BASE: usize = 0x1f000d0000;

// GPIO registers
const GPFSEL0: *mut u32 = (GPIO_BASE + 0x00) as *mut u32;
const GPSET0: *mut u32 = (GPIO_BASE + 0x1C) as *mut u32;
const GPCLR0: *mut u32 = (GPIO_BASE + 0x28) as *mut u32;
const GPLEV0: *mut u32 = (GPIO_BASE + 0x34) as *mut u32;

#[derive(Debug, Clone, Copy)]
pub enum GpioFunction {
    Input = 0b000,
    Output = 0b001,
    Alt0 = 0b100,
    Alt1 = 0b101,
    Alt2 = 0b110,
    Alt3 = 0b111,
    Alt4 = 0b011,
    Alt5 = 0b010,
}

pub struct Gpio;

impl Gpio {
    pub fn new() -> Self {
        Gpio
    }

    /// Set GPIO pin function
    pub fn set_function(&self, pin: u32, _function: GpioFunction) {
        if pin > 53 {
            return;
        }

        let reg_offset = (pin / 10) as isize; // 10 pins per GPFSEL register
        let bit_offset = (pin % 10) * 3;
        unsafe {
            let reg_ptr = GPFSEL0.offset(reg_offset);
            let mut val = ptr::read_volatile(reg_ptr);
            // clear current 3 bits
            val &= !(0b111 << bit_offset);
            val |= ( (_function as u32) << bit_offset);
            ptr::write_volatile(reg_ptr, val);
        }
    }

    /// Set GPIO pin high
    pub fn set(&self, pin: u32) {
        if pin > 53 {
            return;
        }
        let reg_offset = (pin / 32) as isize;
        let bit = pin % 32;
        unsafe {
            ptr::write_volatile(GPSET0.offset(reg_offset), 1u32 << bit);
        }
    }

    /// Set GPIO pin low
    pub fn clear(&self, pin: u32) {
        if pin > 53 {
            return;
        }
        let reg_offset = (pin / 32) as isize;
        let bit = pin % 32;
        unsafe {
            ptr::write_volatile(GPCLR0.offset(reg_offset), 1u32 << bit);
        }
    }

    /// Write value to GPIO pin
    pub fn write(&self, pin: u32, value: bool) {
        if value {
            self.set(pin);
        } else {
            self.clear(pin);
        }
    }

    /// Read GPIO pin level
    pub fn read(&self, pin: u32) -> bool {
        if pin > 53 {
            return false;
        }

        let reg_offset = (pin / 32) as isize;
        let bit = pin % 32;

        unsafe {
            let val = ptr::read_volatile(GPLEV0.offset(reg_offset));
            (val & (1 << bit)) != 0
        }
    }
}

// Global GPIO instance
pub static GPIO: Gpio = Gpio;
