// drivers/rp1_control.rs
//
// High-level helpers for powering / clocking / resetting RP1 peripherals.

use crate::drivers::mailbox_vc::MailboxError;
use core::ptr;
use crate::drivers::rp1_mailbox_proto as proto;

#[derive(Copy, Clone, Debug)]
pub enum Rp1Device {
    Usb,
    Ethernet,
    Wifi,
}

fn to_dev_id(dev: Rp1Device) -> proto::Rp1DevId {
    match dev {
        Rp1Device::Usb => proto::Rp1DevId::Usb,
        Rp1Device::Ethernet => proto::Rp1DevId::Ethernet,
        Rp1Device::Wifi => proto::Rp1DevId::Wifi,
    }
}

pub fn rp1_power_on(dev: Rp1Device) -> Result<(), MailboxError> {
    let id = to_dev_id(dev);
    let status = proto::rp1_fw_power_control(id, proto::power_flags::ON)?;
    log_status("power_on", id, status);
    Ok(())
}

pub fn rp1_power_off(dev: Rp1Device) -> Result<(), MailboxError> {
    let id = to_dev_id(dev);
    let status = proto::rp1_fw_power_control(id, proto::power_flags::OFF)?;
    log_status("power_off", id, status);
    Ok(())
}

pub fn rp1_clock_enable(dev: Rp1Device) -> Result<(), MailboxError> {
    let id = to_dev_id(dev);
    let status = proto::rp1_fw_clock_control(id, proto::clock_flags::ENABLE, 0)?;
    log_status("clock_enable", id, status);
    Ok(())
}

pub fn rp1_clock_disable(dev: Rp1Device) -> Result<(), MailboxError> {
    let id = to_dev_id(dev);
    let status = proto::rp1_fw_clock_control(id, proto::clock_flags::DISABLE, 0)?;
    log_status("clock_disable", id, status);
    Ok(())
}

pub fn rp1_reset_deassert(dev: Rp1Device) -> Result<(), MailboxError> {
    let id = to_dev_id(dev);
    let status = proto::rp1_fw_reset_control(id, proto::reset_flags::DEASSERT)?;
    log_status("reset_deassert", id, status);
    Ok(())
}

pub fn rp1_reset_assert(dev: Rp1Device) -> Result<(), MailboxError> {
    let id = to_dev_id(dev);
    let status = proto::rp1_fw_reset_control(id, proto::reset_flags::ASSERT)?;
    log_status("reset_assert", id, status);
    Ok(())
}

fn log_status(action: &str, dev: proto::Rp1DevId, status: u32) {
    crate::print_str("RP1 ");
    crate::print_str(action);
    crate::print_str(" dev=0x");
    crate::print_hex(dev.as_raw() as usize);
    crate::print_str(" status=0x");
    crate::print_hex(status as usize);
    crate::print_str("\n");
}

// -----------------------------------------------------------------------
// Low-level register definitions (direct RP1 control block access)
// -----------------------------------------------------------------------
pub const RP1_SYS_BASE: usize = 0x1000_0000;
pub const RP1_RST_CTRL: usize = RP1_SYS_BASE + 0x0000_1000;
pub const RP1_CLK_CTRL: usize = RP1_SYS_BASE + 0x0000_2000;
pub const RP1_PWR_CTRL: usize = RP1_SYS_BASE + 0x0000_3000;

pub const RP1_PWR_ETH: u32 = 1 << 4;
pub const RP1_CLK_ETH: u32 = 1 << 4;
pub const RP1_RST_ETH: u32 = 1 << 4;

#[inline(always)]
unsafe fn w32(addr: usize, val: u32) {
    ptr::write_volatile(addr as *mut u32, val);
}

#[inline(always)]
unsafe fn r32(addr: usize) -> u32 {
    ptr::read_volatile(addr as *const u32)
}

/// Minimal RP1 Ethernet bring-up via direct register access.
/// This mirrors Circle's bootloader sequence.
pub fn rp1_enable_ethernet_lowlevel() {
    unsafe {
        // Step 1: Power ON
        let pwr = r32(RP1_PWR_CTRL);
        w32(RP1_PWR_CTRL, pwr | RP1_PWR_ETH);

        // Step 2: Clock enable
        let clk = r32(RP1_CLK_CTRL);
        w32(RP1_CLK_CTRL, clk | RP1_CLK_ETH);

        // Step 3: Reset deassert
        let rst = r32(RP1_RST_CTRL);
        w32(RP1_RST_CTRL, rst & !RP1_RST_ETH);
    }
}
