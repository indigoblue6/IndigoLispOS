// drivers/mailbox_vc.rs
// VideoCore property mailbox (BCM2712 / Pi 5) implementation.

use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};

// Pi 5 (BCM2712) VideoCore property mailbox base exposed to the Cortex-A76.
// Access the legacy 0x1000_00B880 window, which is identity-mapped as
// device memory in `mmu.c` and avoids the RP1 BAR aliases.
pub const VC_MAILBOX_BASE: usize = 0x100000B880;

// レジスタオフセット（従来の BCM mailbox と同じ）
fn vc_mbox_read_addr() -> usize   { VC_MAILBOX_BASE + 0x00 }
fn vc_mbox_status_addr() -> usize { VC_MAILBOX_BASE + 0x18 }
fn vc_mbox_write_addr() -> usize  { VC_MAILBOX_BASE + 0x20 }

const MAILBOX_STATUS_EMPTY: u32 = 1 << 30;
const MAILBOX_STATUS_FULL:  u32 = 1 << 31;

const MAILBOX_PROPERTY_CHANNEL: u32 = 8;
const MAILBOX_TIMEOUT:       usize = 1_000_000;

const PROPERTY_REQUEST:  u32 = 0;
const PROPERTY_RESPONSE: u32 = 0x8000_0000;
const PROPERTY_END_TAG:  u32 = 0;

// VC bus alias for system RAM (matches Linux VC mailbox driver)
const VC_BUS_ADDR_OFFSET: usize = 0xC0_00_0000;

//--------------------------------------------------------------
// Property tag IDs / DeviceId / ClockId
//--------------------------------------------------------------
pub mod tag {
    pub const GET_CLOCK_RATE:          u32 = 0x0003_0002;
    pub const GET_CLOCK_RATE_MEASURED: u32 = 0x0003_0047;
    pub const SET_CLOCK_STATE:         u32 = 0x0003_8001;
    pub const SET_CLOCK_RATE:          u32 = 0x0003_8002;
    pub const SET_POWER_STATE:         u32 = 0x0002_8001;
    pub const GET_POWER_STATE:         u32 = 0x0002_0001;
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum ClockId {
    Core     = 1,
    Apb      = 2,
    Ethernet = 3,
}

impl ClockId {
    pub const fn as_raw(self) -> u32 {
        self as u32
    }
}

#[repr(u32)]
#[derive(Copy, Clone)]
pub enum DeviceId {
    UsbHcd   = 3,
    Ethernet = 4,
}

impl DeviceId {
    pub const fn as_raw(self) -> u32 {
        self as u32
    }
}

// Power state bits
pub mod power_state {
    pub const ON:        u32 = 1 << 0;
    pub const WAIT:      u32 = 1 << 1;
    pub const NO_DEVICE: u32 = 1 << 1; // response bit
}

// Clock state bits
pub mod clock_state {
    pub const ENABLE: u32 = 1 << 0;
}

#[derive(Debug)]
pub enum MailboxError {
    Alignment,
    BufferTooSmall,
    Timeout,
    NoDevice,
    ResponseError,
}

//--------------------------------------------------------------
// VideoCore mailbox low-level read/write
//--------------------------------------------------------------
fn vc_mbox_write(channel: u32, data: u32) -> Result<(), MailboxError> {
    let value = (data & !0xF) | (channel & 0xF);
    let mut last_status = 0;

    // Debug: dump raw VC mailbox registers before attempting write
    dump_raw_vc_regs();

    for i in 0..MAILBOX_TIMEOUT {
        let status = unsafe { ptr::read_volatile(vc_mbox_status_addr() as *const u32) };
        last_status = status;

        if status & MAILBOX_STATUS_FULL == 0 {
            unsafe {
                ptr::write_volatile(vc_mbox_write_addr() as *mut u32, value);
            }
            return Ok(());
        }

        if i % 100_000 == 0 {
            crate::print_str("MAILBOX (VC): waiting to write, STATUS=0x");
            crate::print_hex(status as usize);
            crate::print_str("\n");
        }
    }

    crate::print_str("MAILBOX (VC): write timeout, last STATUS=0x");
    crate::print_hex(last_status as usize);
    crate::print_str("\n");
    Err(MailboxError::Timeout)
}

fn vc_mbox_read(channel: u32) -> Result<u32, MailboxError> {
    let mut last_status = 0;
    // Debug: dump raw VC mailbox registers before attempting read
    dump_raw_vc_regs();
    for i in 0..MAILBOX_TIMEOUT {
        let status = unsafe { ptr::read_volatile(vc_mbox_status_addr() as *const u32) };
        last_status = status;

        if status & MAILBOX_STATUS_EMPTY != 0 {
            if i % 100_000 == 0 {
                crate::print_str("MAILBOX (VC): read waiting, STATUS=0x");
                crate::print_hex(status as usize);
                crate::print_str("\n");
            }
            continue;
        }

        let value = unsafe { ptr::read_volatile(vc_mbox_read_addr() as *const u32) };
        if (value & 0xF) == (channel & 0xF) {
            return Ok(value & !0xF);
        }
    }

    crate::print_str("MAILBOX (VC): read timeout, last STATUS=0x");
    crate::print_hex(last_status as usize);
    crate::print_str("\n");
    Err(MailboxError::Timeout)
}

// Dump raw VC mailbox registers for debugging
fn dump_raw_vc_regs() {
    let _base = VC_MAILBOX_BASE as *const u32;
    let status_addr = (VC_MAILBOX_BASE + 0x18) as *const u32;
    let write_addr = (VC_MAILBOX_BASE + 0x20) as *const u32;
    let read_addr = VC_MAILBOX_BASE as *const u32;

    let r_val = unsafe { ptr::read_volatile(read_addr) };
    let s_val = unsafe { ptr::read_volatile(status_addr) };
    let w_val = unsafe { ptr::read_volatile(write_addr) };

    crate::print_str("MAILBOX (VC): RAW DUMP base=0x");
    crate::print_hex(VC_MAILBOX_BASE);
    crate::print_str(" read=0x");
    crate::print_hex(r_val as usize);
    crate::print_str(" status=0x");
    crate::print_hex(s_val as usize);
    crate::print_str(" write_reg=0x");
    crate::print_hex(w_val as usize);
    crate::print_str("\n");
}

// ARM 物理アドレス → VC バスアドレス変換。
fn arm_to_vc(addr: usize) -> usize {
    (addr & 0x3FFF_FFFF) | VC_BUS_ADDR_OFFSET
}

//--------------------------------------------------------------
// property_call: 旧版 mailbox.rs のロジックをそのまま移植
//--------------------------------------------------------------
/// Property call の生実装。
/// buffer は 16 バイトアライン & property header/footer を持つこと。
pub fn property_call(buffer: &mut [u32]) -> Result<(), MailboxError> {
    if buffer.len() < 3 {
        return Err(MailboxError::BufferTooSmall);
    }

    let addr = buffer.as_ptr() as usize;
    if (addr & 0xF) != 0 {
        return Err(MailboxError::Alignment);
    }

    // リクエストコードを REQUEST にセット
    buffer[1] = PROPERTY_REQUEST;
    // 末尾に終端タグを保証
    if let Some(last) = buffer.last_mut() {
        *last = PROPERTY_END_TAG;
    }

    compiler_fence(Ordering::SeqCst);

    let bus_addr = arm_to_vc(addr);

    // Quick availability check: Pi 5 では VC mailbox が ARM から利用できないため、
    // 既知のゴミ値 (0xFFFF_FFFF) が返ったら NoDevice 扱いにする。
    // 0x7469_6D65 ("time") は Stub からの応答の可能性があるため、警告しつつ続行する。
    let raw_status = unsafe { ptr::read_volatile(vc_mbox_status_addr() as *const u32) };
    if raw_status == 0xFFFF_FFFF {
        crate::print_str("MAILBOX (VC): not present on this platform (0xFFFFFFFF), skipping property_call\n");
        return Err(MailboxError::NoDevice);
    }
    if raw_status == 0x7469_6D65 {
        crate::print_str("MAILBOX (VC): status is 'time' (0x74696D65), attempting to proceed...\n");
    }

    // VC property mailbox にバスアドレスを書き込む
    vc_mbox_write(MAILBOX_PROPERTY_CHANNEL, bus_addr as u32)?;
    let resp = vc_mbox_read(MAILBOX_PROPERTY_CHANNEL)?;

    if resp != (bus_addr as u32) {
        crate::print_str("MAILBOX (VC): response mismatch\n");
        return Err(MailboxError::ResponseError);
    }

    compiler_fence(Ordering::SeqCst);

    if (buffer[1] & PROPERTY_RESPONSE) == 0 {
        crate::print_str("MAILBOX (VC): PROPERTY_RESPONSE bit not set\n");
        return Err(MailboxError::ResponseError);
    }

    Ok(())
}

//--------------------------------------------------------------
// 高レベル API: clock / power 制御
//--------------------------------------------------------------
/// Clock レート取得（Hz）
pub fn get_clock_rate(clock: ClockId) -> Result<u32, MailboxError> {
    #[repr(align(16))]
    struct PropertyBuf([u32; 8]);

    let mut buf = PropertyBuf([0; 8]);
    let data = &mut buf.0;

    let byte_size = (data.len() * core::mem::size_of::<u32>()) as u32;
    data[0] = byte_size;
    data[1] = PROPERTY_REQUEST;
    data[2] = tag::GET_CLOCK_RATE;
    data[3] = 8; // value buffer size
    data[4] = 4; // request size (clock_id)
    data[5] = clock.as_raw();
    data[6] = 0; // placeholder
    data[7] = PROPERTY_END_TAG;

    match property_call(data) {
        Ok(()) => {
            if (data[4] & PROPERTY_RESPONSE) == 0 {
                return Err(MailboxError::ResponseError);
            }
            Ok(data[6])
        }
        Err(MailboxError::NoDevice) => {
            crate::print_str("MAILBOX (VC): get_clock_rate - VC mailbox absent, returning 0\n");
            Ok(0)
        }
        Err(e) => Err(e),
    }
}

/// Clock ON/OFF
pub fn set_clock_state(clock: ClockId, enable: bool) -> Result<u32, MailboxError> {
    #[repr(align(16))]
    struct PropertyBuf([u32; 8]);
    let mut buf = PropertyBuf([0; 8]);
    let data = &mut buf.0;

    data[0] = (data.len() * core::mem::size_of::<u32>()) as u32;
    data[1] = PROPERTY_REQUEST;
    data[2] = tag::SET_CLOCK_STATE;
    data[3] = 8; // value buffer size
    data[4] = 8; // request size
    data[5] = clock.as_raw();
    data[6] = if enable { clock_state::ENABLE } else { 0 };
    data[7] = PROPERTY_END_TAG;

    match property_call(data) {
        Ok(()) => Ok(data[6]),
        Err(MailboxError::NoDevice) => {
            crate::print_str("MAILBOX (VC): set_clock_state - VC mailbox absent, skipping\n");
            Ok(0)
        }
        Err(e) => Err(e),
    }
}

/// Clock レート設定（Hz）
pub fn set_clock_rate(clock: ClockId, rate_hz: u32, skip_turbo: bool) -> Result<u32, MailboxError> {
    #[repr(align(16))]
    struct PropertyBuf([u32; 9]);
    let mut buf = PropertyBuf([0; 9]);
    let data = &mut buf.0;

    data[0] = (data.len() * core::mem::size_of::<u32>()) as u32;
    data[1] = PROPERTY_REQUEST;
    data[2] = tag::SET_CLOCK_RATE;
    data[3] = 12;
    data[4] = 12;
    data[5] = clock.as_raw();
    data[6] = rate_hz;
    data[7] = if skip_turbo { 1 } else { 0 };
    data[8] = PROPERTY_END_TAG;

    match property_call(data) {
        Ok(()) => Ok(data[6]),
        Err(MailboxError::NoDevice) => {
            crate::print_str("MAILBOX (VC): set_clock_rate - VC mailbox absent, skipping\n");
            Ok(0)
        }
        Err(e) => Err(e),
    }
}

/// Power ON/OFF
pub fn set_power_state(device: DeviceId, enable: bool, wait: bool) -> Result<u32, MailboxError> {
    #[repr(align(16))]
    struct PropertyBuf([u32; 9]);
    let mut buf = PropertyBuf([0; 9]);
    let data = &mut buf.0;

    data[0] = (data.len() * core::mem::size_of::<u32>()) as u32;
    data[1] = PROPERTY_REQUEST;
    data[2] = tag::SET_POWER_STATE;
    data[3] = 8;
    data[4] = 8;
    data[5] = device.as_raw();
    let mut state = if enable { power_state::ON } else { 0 };
    if wait {
        state |= power_state::WAIT;
    }
    data[6] = state;
    data[7] = PROPERTY_END_TAG;
    data[8] = PROPERTY_END_TAG;

    match property_call(data) {
        Ok(()) => Ok(data[6]),
        Err(MailboxError::NoDevice) => {
            crate::print_str("MAILBOX (VC): set_power_state - VC mailbox absent, skipping\n");
            Ok(0)
        }
        Err(e) => Err(e),
    }
}

// Debug helpers
pub fn probe_vc_mailbox_base() {
    crate::print_str("MAILBOX (VC): probe_vc_mailbox_base\n");
    crate::print_str("  VC_MAILBOX_BASE=");
    crate::print_hex(VC_MAILBOX_BASE);
    crate::print_str("\n");
}
