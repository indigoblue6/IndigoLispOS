// drivers/mailbox_rp1.rs
//
// RP1 mailbox (PCIe window) – IndigoLispOS version
// Provides mailbox_write / mailbox_read for RP1 firmware interface.

use crate::print_str;
use crate::print_hex;
use crate::drivers::timer::TIMER;
use core::sync::atomic::{AtomicUsize, Ordering};

// =============================================
// RP1 BAR0 mailbox register offsets
// (RP1 firmware mailbox mapped inside BAR0)
// =============================================
//
// RP1 mailbox lives at: BAR0 + 0x3000
// BAR0 is programmed dynamically by PCIe init,
// so we store the runtime base address here.
// =============================================

/// Runtime RP1 mailbox base address (set after BAR0 programming)
pub static RP1_MBOX_RUNTIME_BASE: AtomicUsize = AtomicUsize::new(0);

// Mailbox register offsets from base
const MBOX_READ_OFFSET:   usize = 0x00;
const MBOX_POLL_OFFSET:   usize = 0x10;
const MBOX_SENDER_OFFSET: usize = 0x14;
const MBOX_STATUS_OFFSET: usize = 0x18;
const MBOX_CONFIG_OFFSET: usize = 0x1C;
const MBOX_WRITE_OFFSET:  usize = 0x20;

// Status bits
const STATUS_EMPTY: u32 = 1 << 30;
const STATUS_FULL:  u32 = 1 << 31;

// RP1 Firmware mailbox channels
pub const MAILBOX_RP1_CHANNEL: u32 = 1;  // MDIO/PHY channel
pub const MAILBOX_RP1_SYS_CHANNEL: u32 = 8;  // System control channel

const TIMEOUT: usize = 1_000_000;

// RP1 Firmware system operations (reverse-engineered from Linux/Circle)
const FW_OP_ENABLE_GBE: u32   = 0x0002_0001;
const FW_OP_ENABLE_MDIO: u32  = 0x0002_0002;
const FW_OP_ENABLE_CLOCK: u32 = 0x0002_0003;
const FW_OP_ETH_ENABLE: u32 = 0x0005_0001;
#[allow(dead_code)]
const FW_OP_ETH_DISABLE: u32 = 0x0005_0002;
#[allow(dead_code)]
const FW_OP_MDIO_READ: u32   = 0x0005_0003;
#[allow(dead_code)]
const FW_OP_MDIO_WRITE: u32  = 0x0005_0004;


/// Initialize RP1 mailbox base address after BAR0 is programmed
pub fn init_runtime_mailbox_base(bar0_cpu_base: usize) {
    let mbox_base = bar0_cpu_base + 0x3000;
    RP1_MBOX_RUNTIME_BASE.store(mbox_base, Ordering::SeqCst);
    
    print_str("[RP1 MBOX] Runtime base initialized: 0x");
    print_hex(mbox_base);
    print_str("\n");
}

// =====================================================
// PUBLIC API
// =====================================================

/// Write value to RP1 mailbox channel
pub fn mailbox_write(channel: u32, data: u32) -> Result<(), ()> {
    let base = RP1_MBOX_RUNTIME_BASE.load(Ordering::SeqCst);
    if base == 0 {
        print_str("[RP1 MBOX] ERROR: Runtime base not initialized!\n");
        return Err(());
    }
    
    let val = (data & !0xF) | (channel & 0xF);

    for _ in 0..TIMEOUT {
        let status = unsafe { core::ptr::read_volatile((base + MBOX_STATUS_OFFSET) as *const u32) };
        if status & STATUS_FULL == 0 {
            unsafe {
                core::ptr::write_volatile((base + MBOX_WRITE_OFFSET) as *mut u32, val);
            }
            return Ok(());
        }
        // wait
        TIMER.delay_us(2);
    }

    print_str("[RP1 MBOX] write timeout\n");
    Err(())
}

/// Read value from RP1 mailbox channel
pub fn mailbox_read(channel: u32) -> Result<u32, ()> {
    let base = RP1_MBOX_RUNTIME_BASE.load(Ordering::SeqCst);
    if base == 0 {
        print_str("[RP1 MBOX] ERROR: Runtime base not initialized!\n");
        return Err(());
    }
    
    for _ in 0..TIMEOUT {
        let status = unsafe { core::ptr::read_volatile((base + MBOX_STATUS_OFFSET) as *const u32) };

        if status & STATUS_EMPTY == 0 {
            let val = unsafe { core::ptr::read_volatile((base + MBOX_READ_OFFSET) as *const u32) };
            if (val & 0xF) == (channel & 0xF) {
                return Ok(val & !0xF);
            }
        }

        TIMER.delay_us(2);
    }

    print_str("[RP1 MBOX] read timeout\n");
    Err(())
}

/// Write 64-bit message packet to RP1 firmware mailbox (Linux rp1.c format)
/// 
/// RP1 firmware expects a 64-bit message packet:
///   [31:0]  = tag (opcode) | channel
///   [63:32] = value
/// 
/// This is done by writing twice to MBOX_WRITE:
///   1. write(tag | channel)
///   2. write(val)
pub fn mailbox_write64(channel: u32, tag: u32, val: u32) -> Result<(), ()> {
    let base = RP1_MBOX_RUNTIME_BASE.load(Ordering::SeqCst);
    if base == 0 {
        print_str("[RP1 MBOX] ERROR: Runtime base not initialized!\n");
        return Err(());
    }

    // Linux does:
    //   write(tag | channel)
    //   write(val)
    let header = (tag & !0xF) | (channel & 0xF);

    for _ in 0..TIMEOUT {
        let status = unsafe { core::ptr::read_volatile((base + MBOX_STATUS_OFFSET) as *const u32) };
        if status & STATUS_FULL == 0 {
            unsafe {
                core::ptr::write_volatile((base + MBOX_WRITE_OFFSET) as *mut u32, header);
                core::ptr::write_volatile((base + MBOX_WRITE_OFFSET) as *mut u32, val);
            }
            return Ok(());
        }
        TIMER.delay_us(2);
    }

    print_str("[RP1 MBOX] write64 timeout\n");
    Err(())
}

/// Read 64-bit message packet from RP1 firmware mailbox (Linux rp1.c format)
/// 
/// RP1 firmware sends a 64-bit message packet:
///   [31:0]  = tag (opcode) | channel
///   [63:32] = value
/// 
/// This is done by reading twice from MBOX_READ:
///   1. header = read() -> (tag | channel)
///   2. val = read()
/// 
/// Returns: (tag, val)
pub fn mailbox_read64(channel: u32) -> Result<(u32, u32), ()> {
    let base = RP1_MBOX_RUNTIME_BASE.load(Ordering::SeqCst);
    if base == 0 {
        print_str("[RP1 MBOX] ERROR: Runtime base not initialized!\n");
        return Err(());
    }

    for _ in 0..TIMEOUT {
        let status = unsafe { core::ptr::read_volatile((base + MBOX_STATUS_OFFSET) as *const u32) };
        if status & STATUS_EMPTY == 0 {
            let header = unsafe { core::ptr::read_volatile((base + MBOX_READ_OFFSET) as *const u32) };
            if (header & 0xF) == (channel & 0xF) {
                let val = unsafe { core::ptr::read_volatile((base + MBOX_READ_OFFSET) as *const u32) };
                return Ok((header & !0xF, val));
            }
        }
        TIMER.delay_us(2);
    }

    print_str("[RP1 MBOX] read64 timeout\n");
    Err(())
}

/// For compatibility — RP1 mailbox does *not* use VC property tags.
/// Always return Err.
pub fn property_call(_buffer: &mut [u32]) -> Result<(), ()> {
    print_str("[RP1 MBOX] property_call is not supported for RP1\n");
    Err(())
}

// =====================================================
// RP1 FIRMWARE SYSTEM OPERATIONS
// =====================================================

/// Perform RP1 firmware system operation (Linux rp1.c equivalent)
/// 
/// Sends a 64-bit message packet to RP1 firmware:
///   write64(channel=8, tag=op, val=val)
///   read64(channel=8) -> (resp_tag, resp_val)
pub unsafe fn rp1_fw_sys_op(op: u32, arg: u32) -> bool {
    // 1) 送信
    print_str("[RP1-FW] sending op=0x");
    print_hex(op as usize);
    print_str(" arg=0x");
    print_hex(arg as usize);
    print_str("\n");

    if mailbox_write64(MAILBOX_RP1_SYS_CHANNEL, op, arg).is_err() {
        print_str("[RP1-FW] mailbox_write64 failed\n");
        return false;
    }

    // 2) 応答待ち
    match mailbox_read64(MAILBOX_RP1_SYS_CHANNEL) {
        Ok((resp_tag, resp_val)) => {
            print_str("[RP1-FW] resp_tag=0x");
            print_hex(resp_tag as usize);
            print_str(" resp_val=0x");
            print_hex(resp_val as usize);
            print_str("\n");

            // 成功条件は FW 側の仕様次第だが、
            // 「同じ op が返ってきて resp_val == 1」をとりあえず成功扱い
            resp_tag == (op & !0xF) && resp_val == 1
        }
        Err(_) => {
            print_str("[RP1-FW] mailbox_read64 timeout\n");
            false
        }
    }
}


/// Initialize RP1 Firmware for Ethernet (must be called before PHY access)
pub unsafe fn rp1_fw_init_ethernet() {
    print_str("[RP1-FW] Initializing Ethernet subsystem via firmware...\n");
    
    print_str("[RP1-FW] Enabling GBE (legacy 0x0002_0001)...\n");
    if rp1_fw_sys_op(FW_OP_ENABLE_GBE, 1) {
        print_str("[RP1-FW] GBE enabled\n");
    } else {
        print_str("[RP1-FW] GBE enable failed (ignored on new firmware)\n");
    }

    print_str("[RP1-FW] Enabling MDIO (legacy 0x0002_0002)...\n");
    if rp1_fw_sys_op(FW_OP_ENABLE_MDIO, 1) {
        print_str("[RP1-FW] MDIO enabled\n");
    } else {
        print_str("[RP1-FW] MDIO enable failed (ignored on new firmware)\n");
    }

    print_str("[RP1-FW] Enabling clocks (legacy 0x0002_0003)...\n");
    if rp1_fw_sys_op(FW_OP_ENABLE_CLOCK, 1) {
        print_str("[RP1-FW] Clocks enabled\n");
    } else {
        print_str("[RP1-FW] Clock enable failed (ignored on new firmware)\n");
    }

    print_str("[RP1-FW] Enabling Ethernet domain via FW_OP_ETH_ENABLE...\n");
    if rp1_fw_sys_op(FW_OP_ETH_ENABLE, 1) {
        print_str("[RP1-FW] Ethernet enabled\n");
    } else {
        print_str("[RP1-FW] Ethernet enable failed (firmware did not respond)\n");
    }
    
    print_str("[RP1-FW] Ethernet firmware initialization complete\n");
}

/// RP1 Mailbox IRQ handler
/// 
/// Reads status to clear interrupt (if applicable) and handles messages.
pub fn rp1_mbox_irq_handler(_intid: u32) {
    unsafe {
        let base = RP1_MBOX_RUNTIME_BASE.load(Ordering::SeqCst);
        if base == 0 {
            return;
        }
        
        // Read status
        let status = core::ptr::read_volatile((base + MBOX_STATUS_OFFSET) as *const u32);
        
        // If not empty, read message
        if status & STATUS_EMPTY == 0 {
            let _val = core::ptr::read_volatile((base + MBOX_READ_OFFSET) as *const u32);
            // TODO: Handle message
        }
    }
}
