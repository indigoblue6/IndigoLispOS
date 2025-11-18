// rp1.rs - RP1 peripheral base addresses for Raspberry Pi 5
//
// Converted from Circle's bcm2712.h (RPi 5 memory map)
// to provide Rust-friendly constants.

#![allow(dead_code)]

pub const ARM_SOC_STEPPING: usize = 0x1001_5040_04;

// RP1 interrupt controller
pub const ARM_RP1_INTC: usize = 0x1F00_1080_00;

// RP1 GPIO0 (I/O, Register IO, Pads)
pub const ARM_GPIO0_IO_BASE: usize = 0x1F00_0D00_00;
pub const ARM_GPIO0_RIO_BASE: usize = 0x1F00_0E00_00;
pub const ARM_GPIO0_PADS_BASE: usize = 0x1F00_0F00_00;

// RP1 GPIO clocks
pub const ARM_GPIO_CLK_BASE: usize = 0x1F00_0180_00;

// RP1 DMA controller
pub const ARM_DMA_RP1_BASE: usize = 0x1F00_1880_00;
pub const ARM_DMA_RP1_END: usize = ARM_DMA_RP1_BASE + 0xFFF;

// RP1 MACB (Gigabit Ethernet)
pub const ARM_MACB_BASE: usize = 0x1F00_1000_00;
pub const ARM_MACB_END: usize = ARM_MACB_BASE + 0x3FFF;

// RP1 PWM
pub const ARM_PWM0_BASE: usize = 0x1F00_0980_00;
pub const ARM_PWM0_END: usize = ARM_PWM0_BASE + 0xFF;
pub const ARM_PWM1_BASE: usize = 0x1F00_09C0_00;
pub const ARM_PWM1_END: usize = ARM_PWM1_BASE + 0xFF;

// RP1 I2S
pub const ARM_I2S0_BASE: usize = 0x1F00_0A00_00;
pub const ARM_I2S0_END: usize = ARM_I2S0_BASE + 0xFFF;
pub const ARM_I2S1_BASE: usize = 0x1F00_0A40_00;
pub const ARM_I2S1_END: usize = ARM_I2S1_BASE + 0xFFF;

// Reset controllers
pub const ARM_RESET_BASE: usize = 0x1001_5043_18;
pub const ARM_RESET_END: usize = ARM_RESET_BASE + 0x2F;
pub const ARM_RESET_RESCAL_BASE: usize = 0x1000_1195_00;
pub const ARM_RESET_RESCAL_END: usize = ARM_RESET_RESCAL_BASE + 0x0F;
