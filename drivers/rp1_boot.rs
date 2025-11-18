// Minimal RP1 firmware boot sequence (Rust)
// Places taken from the user's environment (BAR1 mapped to 0x0000006000000000)

use crate::drivers::timer::TIMER;

const RP1_BASE: u64 = 0x0000006000000000;

// Boot/reset/clock offsets (as suggested)
const RP1_BOOTCFG: u64 = RP1_BASE + 0x0000_3000;
const RP1_RESET_CTRL: u64 = RP1_BASE + 0x0000_3004;
const RP1_CLK_CTRL: u64 = RP1_BASE + 0x0001_803C;

// Mailbox base inside BAR1 (candidate provided)
const RP1_MBOX_BASE: u64 = 0x000000600010B880;
const REG_MAILBOX_STATUS: u64 = RP1_MBOX_BASE + 0x18;
// Doorbell / kick register inside RP1 control block
const RP1_DOORBELL: u64 = RP1_BASE + 0x0000_3008;

#[inline(always)]
unsafe fn write32(addr: u64, val: u32) {
    core::ptr::write_volatile(addr as *mut u32, val);
}

#[inline(always)]
unsafe fn read32(addr: u64) -> u32 {
    core::ptr::read_volatile(addr as *const u32)
}

pub unsafe fn rp1_boot_sequence() {
    crate::print_str("RP1: running minimal boot sequence...\n");

    // 1) Boot config magic
    crate::print_str("RP1: writing BOOTCFG magic\n");
    write32(RP1_BOOTCFG, 0x5A00_0001);

    // small delay
    TIMER.delay_ms(10);

    // 2) Deassert reset
    crate::print_str("RP1: deassert reset\n");
    write32(RP1_RESET_CTRL, 0x0000_0000);
    TIMER.delay_ms(10);

    // 3) Enable clock (OR in enable bit)
    let before = read32(RP1_CLK_CTRL);
    crate::print_str("RP1: clock ctrl before=0x"); crate::print_hex(before as usize); crate::print_str("\n");
    write32(RP1_CLK_CTRL, before | 0x800);
    TIMER.delay_ms(1);

    // 3.5) Doorbell / kick RP1 firmware to advance internal state
    crate::print_str("RP1: doorbell kick\n");
    write32(RP1_DOORBELL, 1);
    // Give RP1 a bit more time to react
    TIMER.delay_ms(10);
    crate::print_str("RP1: mailbox status after doorbell=0x");
    crate::print_hex(read32(REG_MAILBOX_STATUS) as usize);
    crate::print_str("\n");

    // 4) Poll mailbox alive status (0xFFFFFFFF indicates dead)
    crate::print_str("RP1: polling mailbox status\n");
    for _ in 0..100 {
        let st = read32(REG_MAILBOX_STATUS);
        if st != 0xFFFF_FFFF {
            crate::print_str("RP1 MAILBOX IS ALIVE! STATUS=0x");
            crate::print_hex(st as usize);
            crate::print_str("\n");
            return;
        }
        TIMER.delay_ms(1);
    }

    crate::print_str("WARNING: RP1 mailbox still dead (status=0x");
    crate::print_hex(read32(REG_MAILBOX_STATUS) as usize);
    crate::print_str(")\n");
}

// ---------------------------------------------------------------------------
// RP1 firmware mailbox RPC (Linux-compatible structures and helpers)
// This implements the tag-based property interface used by Linux's
// drivers/firmware/raspberrypi/rp1-ctrl.c and the accompanying headers.
// The layout and tag IDs below intentionally match the upstream Linux
// definitions so the RP1 firmware accepts and responds to requests.
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Rp1MailboxHeader {
    pub buf_size: u32, // total buffer size in bytes (including this header)
    pub code: u32,     // PROPERTY_REQUEST (0) for request, PROPERTY_RESPONSE (0x8000_0000) for response
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Rp1TagHeader {
    pub tag: u32,      // tag id (e.g. 0x00010001)
    pub buf_size: u32, // size of value buffer in bytes
    pub req_resp: u32, // 0 = request, 1 = response
}

// We allocate a fixed payload (words) area here. Most requests are small
// so keeping a compact buffer is fine; adjust size if you need larger
// messages or multiple tags in a single buffer.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct Rp1MailboxBuffer {
    pub header: Rp1MailboxHeader,
    pub tag_header: Rp1TagHeader,
    pub payload: [u32; 8], // payload words (u32)
    pub end_tag: u32,
}

impl core::fmt::Debug for Rp1MailboxBuffer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Rp1MailboxBuffer")
            .field("header", &self.header)
            .field("tag_header", &self.tag_header)
            .field("payload", &&self.payload[..])
            .field("end_tag", &self.end_tag)
            .finish()
    }
}

impl Default for Rp1MailboxBuffer {
    fn default() -> Self {
        Self {
            header: Rp1MailboxHeader { buf_size: 0, code: 0 },
            tag_header: Rp1TagHeader { tag: 0, buf_size: 0, req_resp: 0 },
            payload: [0u32; 8],
            end_tag: 0,
        }
    }
}

impl Rp1MailboxBuffer {
    /// Create a new single-tag mailbox buffer. `payload_size_bytes` must be
    /// a multiple of 4 and fit in the `payload` area.
    pub fn new_single_tag(tag: u32, payload_size_bytes: usize) -> Self {
        let mut buf = Self::default();
        // Total bytes = header(8) + tag_header(12) + payload + end_tag(4)
        let total = 8 + 12 + payload_size_bytes + 4;
        buf.header.buf_size = total as u32;
        // Standard property request code
        buf.header.code = 0; // PROPERTY_REQUEST
        buf.tag_header.tag = tag;
        buf.tag_header.buf_size = payload_size_bytes as u32;
        // For requests the third word is the request size (in bytes)
        buf.tag_header.req_resp = payload_size_bytes as u32;
        buf.end_tag = 0;
        buf
    }
}

// ---------------------------------------------------------------------------
// RP1 tag / id definitions (match Linux values)
// ---------------------------------------------------------------------------

// Use the standard property tag IDs (match drivers/mailbox.rs)
pub const RP1_SET_POWER_STATE: u32 = 0x0002_8001;
pub const RP1_SET_CLOCK_STATE: u32 = 0x0003_8001;
pub const RP1_SET_CLOCK_RATE: u32 = 0x0003_8002;
pub const RP1_GET_CLOCK_RATE: u32 = 0x0003_0002;
pub const RP1_SET_THROTTLED: u32 = 0x0003_0003; // RP1-specific (keep as-is)

#[repr(u32)]
#[derive(Debug, Copy, Clone)]
pub enum Rp1PowerDomain {
    GBE = 0,
    PCIE = 1,
    USB0 = 2,
    USB1 = 3,
    SDIO = 4,
    IO_BANK0 = 5,
    IO_BANK1 = 6,
    ANA = 7,
    DMA = 8,
}

#[repr(u32)]
#[derive(Debug, Copy, Clone)]
pub enum Rp1ClockId {
    GBE = 0,
    PCIE = 1,
    USB0 = 2,
    USB1 = 3,
    SDIO = 4,
    REF = 5,
    ANA = 6,
    DMA = 7,
}

// ---------------------------------------------------------------------------
// Mailbox call implementation
// ---------------------------------------------------------------------------

/// Perform a single-tag mailbox call to RP1 firmware.
/// Returns true on success (response OK), false otherwise.
/// Use the central mailbox `property_call` implementation to perform
/// a property request. This builds on `drivers::mailbox::property_call`
/// which handles bus-addr conversion and mailbox register I/O.
fn rp1_property_call(buffer: &mut [u32]) -> bool {
    match crate::drivers::mailbox_rp1::property_call(buffer) {
        Ok(()) => true,
        Err(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Helper functions: power / clock operations (single-tag wrappers)
// ---------------------------------------------------------------------------

pub unsafe fn rp1_set_power_state(domain: Rp1PowerDomain, state: u32) -> bool {
    // Build a property buffer (aligned 16) matching mailbox.rs conventions.
    #[repr(align(16))]
    struct PropertyBuf([u32; 8]);

    let mut buf = PropertyBuf([0u32; 8]);
    let data = &mut buf.0;

    data[0] = (data.len() * core::mem::size_of::<u32>()) as u32; // byte size
    // data[1] will be set by property_call to PROPERTY_REQUEST
    data[2] = RP1_SET_POWER_STATE;
    data[3] = 8; // value buffer byte size
    data[4] = 8; // request bytes
    data[5] = domain as u32;
    data[6] = state;
    // data[last] will be set by property_call to PROPERTY_END_TAG

    rp1_property_call(data)
}

pub unsafe fn rp1_set_clock_state(device: Rp1PowerDomain, id: Rp1ClockId, state: u32) -> bool {
    #[repr(align(16))]
    struct PropertyBuf([u32; 9]);
    let mut buf = PropertyBuf([0u32; 9]);
    let data = &mut buf.0;

    data[0] = (data.len() * core::mem::size_of::<u32>()) as u32;
    data[2] = RP1_SET_CLOCK_STATE;
    data[3] = 12; // payload bytes (dev, clock_id, state)
    data[4] = 12;
    data[5] = device as u32;
    data[6] = id as u32;
    data[7] = state;

    rp1_property_call(data)
}

pub unsafe fn rp1_set_clock_rate(id: Rp1ClockId, rate: u32) -> bool {
    #[repr(align(16))]
    struct PropertyBuf([u32; 8]);
    let mut buf = PropertyBuf([0u32; 8]);
    let data = &mut buf.0;

    data[0] = (data.len() * core::mem::size_of::<u32>()) as u32;
    data[2] = RP1_SET_CLOCK_RATE;
    data[3] = 8;
    data[4] = 8;
    data[5] = id as u32;
    data[6] = rate;

    rp1_property_call(data)
}

// ---------------------------------------------------------------------------
// Convenience example: enable Ethernet (GBE) power + clock
// ---------------------------------------------------------------------------

pub unsafe fn rp1_enable_ethernet() -> bool {
    crate::print_str("RP1: enabling Ethernet power\n");
    if !rp1_set_power_state(Rp1PowerDomain::GBE, 1) {
        crate::print_str("RP1: failed to set power state for GBE\n");
        return false;
    }

    crate::print_str("RP1: enabling Ethernet clock\n");
    if !rp1_set_clock_state(Rp1PowerDomain::GBE, Rp1ClockId::GBE, 1) {
        crate::print_str("RP1: failed to enable GBE clock\n");
        return false;
    }

    true
}
