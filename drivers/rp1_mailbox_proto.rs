// drivers/rp1_mailbox_proto.rs
//
// Thin wrapper over the RP1 firmware mailbox. The actual firmware protocol
// is still unknown, so the command IDs and semantics here are placeholders.

use crate::drivers::mailbox_vc::MailboxError;
use crate::drivers::mailbox_rp1::rp1_mailbox_call;

/// Placeholder firmware command IDs.
pub mod cmd {
    pub const POWER_CTRL: u32 = 0x0001_0000;
    pub const CLOCK_CTRL: u32 = 0x0001_0001;
    pub const RESET_CTRL: u32 = 0x0001_0002;
    pub const GET_STATUS: u32 = 0x0001_0003;
}

/// RP1-internal device identifiers (placeholder values).
#[repr(u32)]
#[derive(Copy, Clone, Debug)]
pub enum Rp1DevId {
    Usb = 1,
    Ethernet = 2,
    Wifi = 3,
}

impl Rp1DevId {
    pub const fn as_raw(self) -> u32 {
        self as u32
    }
}

/// Power control flags.
pub mod power_flags {
    pub const ON: u32 = 1 << 0;
    pub const OFF: u32 = 1 << 1;
}

/// Clock control flags.
pub mod clock_flags {
    pub const ENABLE: u32 = 1 << 0;
    pub const DISABLE: u32 = 1 << 1;
}

/// Reset control flags.
pub mod reset_flags {
    pub const ASSERT: u32 = 1 << 0;
    pub const DEASSERT: u32 = 1 << 1;
}

pub fn rp1_fw_power_control(dev: Rp1DevId, flags: u32) -> Result<u32, MailboxError> {
    let resp = rp1_mailbox_call(cmd::POWER_CTRL, dev.as_raw(), flags, 0)?;
    Ok(resp[0])
}

pub fn rp1_fw_clock_control(dev: Rp1DevId, flags: u32, rate_hz: u32) -> Result<u32, MailboxError> {
    let resp = rp1_mailbox_call(cmd::CLOCK_CTRL, dev.as_raw(), flags, rate_hz)?;
    Ok(resp[0])
}

pub fn rp1_fw_reset_control(dev: Rp1DevId, flags: u32) -> Result<u32, MailboxError> {
    let resp = rp1_mailbox_call(cmd::RESET_CTRL, dev.as_raw(), flags, 0)?;
    Ok(resp[0])
}
