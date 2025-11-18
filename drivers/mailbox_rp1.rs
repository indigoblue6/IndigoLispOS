// drivers/mailbox_rp1.rs
//
// Minimal RP1 firmware mailbox (property tag style) implementation.
// This is used to power/clock RP1 peripherals (e.g. Ethernet) via the
// RP1-side mailbox located in the PCIe WIN0 region.

use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};

use crate::drivers::mailbox;
use crate::drivers::mailbox_vc::MailboxError;
use crate::drivers::pcie::{RP1_BAR0_CPU_BASE};
use crate::drivers::timer::TIMER;

const MAILBOX_STATUS_EMPTY: u32 = 1 << 30;
const MAILBOX_STATUS_FULL: u32 = 1 << 31;
const MAILBOX_PROPERTY_CHANNEL: u32 = 8;
const MAILBOX_TIMEOUT: usize = 1_000_000;

/// Perform an RP1 firmware mailbox property-call.
/// The buffer format matches the VC property interface (size / code / tags).
pub fn property_call(buffer: &mut [u32]) -> Result<(), MailboxError> {
    if buffer.len() < 3 {
        return Err(MailboxError::BufferTooSmall);
    }

    let addr = buffer.as_ptr() as usize;
    if (addr & 0xF) != 0 {
        return Err(MailboxError::Alignment);
    }

    buffer[1] = 0; // PROPERTY_REQUEST
    if let Some(last) = buffer.last_mut() {
        *last = 0; // PROPERTY_END_TAG
    }

    compiler_fence(Ordering::SeqCst);

    let mut waited = 0usize;
    while RP1_BAR0_CPU_BASE.load(Ordering::Acquire) == 0 {
        if waited >= 1000 {
            crate::print_str("MAILBOX (RP1): BAR0 not programmed (timeout)\n");
            return Err(MailboxError::ResponseError);
        }
        TIMER.delay_ms(1);
        waited += 1;
    }

    if !mailbox::is_initialized() {
        crate::print_str("MAILBOX (RP1): mailbox not initialized\n");
        return Err(MailboxError::ResponseError);
    }

    let bus_addr = arm_to_rp1(addr);

    mailbox_write(MAILBOX_PROPERTY_CHANNEL, bus_addr as u32)?;
    let resp = mailbox_read(MAILBOX_PROPERTY_CHANNEL)?;

    if resp != (bus_addr as u32) {
        crate::print_str("MAILBOX (RP1): response mismatch\n");
        return Err(MailboxError::ResponseError);
    }

    compiler_fence(Ordering::SeqCst);

    if (buffer[1] & 0x8000_0000) == 0 {
        crate::print_str("MAILBOX (RP1): PROPERTY_RESPONSE bit not set\n");
        return Err(MailboxError::ResponseError);
    }

    Ok(())
}

/// Temporary helper: issue a firmware command using a simple 4-word payload.
/// Protocol is TBD, so for now this only logs the request and returns zeros.
pub fn rp1_mailbox_call(cmd: u32, arg0: u32, arg1: u32, arg2: u32) -> Result<[u32; 4], MailboxError> {
    crate::print_str("MAILBOX (RP1): rp1_mailbox_call cmd=0x");
    crate::print_hex(cmd as usize);
    crate::print_str(" args=[0x");
    crate::print_hex(arg0 as usize);
    crate::print_str(",0x");
    crate::print_hex(arg1 as usize);
    crate::print_str(",0x");
    crate::print_hex(arg2 as usize);
    crate::print_str("]\n");

    // TODO: implement actual property buffer layout once RP1 firmware protocol is known.
    let _ = (cmd, arg0, arg1, arg2);
    Ok([0, 0, 0, 0])
}

fn mailbox_write(channel: u32, data: u32) -> Result<(), MailboxError> {
    let value = (data & !0xF) | (channel & 0xF);
    let mut last_status = 0;

    for i in 0..MAILBOX_TIMEOUT {
        let status = unsafe { mailbox::read_reg(0x18) };
        last_status = status;

        if status & MAILBOX_STATUS_FULL == 0 {
            unsafe {
                mailbox::write_reg(0x20, value);
            }
            return Ok(());
        }

        if i % 100_000 == 0 {
            crate::print_str("MAILBOX (RP1): waiting to write, STATUS=0x");
            crate::print_hex(status as usize);
            crate::print_str("\n");
        }
    }

    crate::print_str("MAILBOX (RP1): write timeout, last STATUS=0x");
    crate::print_hex(last_status as usize);
    crate::print_str("\n");
    Err(MailboxError::Timeout)
}

fn mailbox_read(channel: u32) -> Result<u32, MailboxError> {
    let mut last_status = 0;

    for i in 0..MAILBOX_TIMEOUT {
        let status = unsafe { mailbox::read_reg(0x18) };
        last_status = status;

        if status & MAILBOX_STATUS_EMPTY != 0 {
            if i % 100_000 == 0 {
                crate::print_str("MAILBOX (RP1): read waiting, STATUS=0x");
                crate::print_hex(status as usize);
                crate::print_str("\n");
            }
            continue;
        }

        let value = unsafe { mailbox::read_reg(0x00) };
        if (value & 0xF) == (channel & 0xF) {
            return Ok(value & !0xF);
        }
    }

    crate::print_str("MAILBOX (RP1): read timeout, last STATUS=0x");
    crate::print_hex(last_status as usize);
    crate::print_str("\n");
    Err(MailboxError::Timeout)
}

#[inline(always)]
fn arm_to_rp1(addr: usize) -> usize {
    addr
}
