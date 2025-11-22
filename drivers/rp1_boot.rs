// Minimal RP1 firmware boot sequence (Rust)
// Places taken from the user's environment (BAR1 mapped to 0x0000006000000000)

use crate::drivers::timer::TIMER;
use crate::drivers::mailbox_rp1::RP1_MBOX_RUNTIME_BASE;
use crate::drivers::pcie::{RP1_BAR0_CPU_BASE, RP1_BAR1_CPU_BASE};
use core::sync::atomic::Ordering;

// BAR0 base will be loaded from PCIe driver at runtime
// BAR0 contains: System registers, Mailbox window, etc.
fn get_bar0_base() -> u64 {
    RP1_BAR0_CPU_BASE.load(Ordering::SeqCst)
}

// BAR1 base for peripherals (GPIO, Ethernet, etc.)
fn get_bar1_base() -> u64 {
    RP1_BAR1_CPU_BASE.load(Ordering::SeqCst)
}

// Boot/reset/clock offsets (BAR1-based)
fn get_bootcfg_addr() -> u64 { get_bar1_base() + 0x0000_3000 }
fn get_reset_ctrl_addr() -> u64 { get_bar1_base() + 0x0000_3004 }
fn get_doorbell_addr() -> u64 { get_bar1_base() + 0x0000_3008 }
fn get_clk_ctrl_addr() -> u64 { get_bar1_base() + 0x0001_803C }

// RP1 SYSCFG registers for mailbox DOORBELL protocol (BAR1-based)
// (from Linux kernel drivers/mailbox/rp1-mailbox.c)
fn get_syscfg_base() -> u64 { get_bar1_base() + 0x0000_8000 }
fn get_syscfg_proc_events() -> u64 { get_syscfg_base() + 0x00000008 }
fn get_syscfg_host_events() -> u64 { get_syscfg_base() + 0x0000000c }
fn get_syscfg_host_event_irq_en() -> u64 { get_syscfg_base() + 0x00000010 }
fn get_syscfg_host_event_irq() -> u64 { get_syscfg_base() + 0x00000014 }

// Hardware register bit manipulation offsets (atomic set/clear)
const HW_SET_BITS: u64 = 0x00002000;  // Add to register address to set bits
const HW_CLR_BITS: u64 = 0x00003000;  // Add to register address to clear bits

// Mailbox doorbell event bits (we use bit 0 for firmware communication)
const MBOX_EVENT_BIT: u32 = 1 << 0;

#[inline(always)]
unsafe fn write32(addr: u64, val: u32) {
    core::ptr::write_volatile(addr as *mut u32, val);
}

#[inline(always)]
unsafe fn read32(addr: u64) -> u32 {
    core::ptr::read_volatile(addr as *const u32)
}

/// Dump BAR0 header to verify correct address decoding
unsafe fn dump_bar0_header(bar0_cpu_addr: u64) {
    crate::print_str("\n=== BAR0 Header Dump ===\n");
    crate::print_str("BAR0 CPU address (pointer): ");
    crate::print_hex(bar0_cpu_addr as usize);
    crate::print_str("\n");
    
    // Verify 4KB alignment
    if (bar0_cpu_addr & 0xFFF) != 0 {
        crate::print_str("ERROR: BAR0 address is NOT 4KB aligned!\n");
        crate::print_str("Lower 12 bits: ");
        crate::print_hex((bar0_cpu_addr & 0xFFF) as usize);
        crate::print_str("\n");
        return;
    }
    
    crate::print_str("BAR0 is correctly 4KB aligned\n");
    crate::print_str("\nReading first 0x40 bytes (16 DWORDs):\n");
    
    // Convert to pointer (DO NOT modify this pointer!)
    let bar0_ptr = bar0_cpu_addr as *const u32;
    
    // Read first 16 DWORDs (0x00 - 0x3C)
    for i in 0..16 {
        let val = core::ptr::read_volatile(bar0_ptr.add(i));
        
        crate::print_str("  [");
        crate::print_hex((i * 4) as usize);
        crate::print_str("] = ");
        crate::print_hex(val as usize);
        crate::print_str("\n");
    }
    
    // Also read mailbox window area (BAR0 + 0x3000)
    crate::print_str("\n=== BAR0 Mailbox Window (0x3000 - 0x300F) ===\n");
    let mbox_ptr = (bar0_cpu_addr + 0x3000) as *const u32;
    
    for i in 0..4 {
        let val = core::ptr::read_volatile(mbox_ptr.add(i));
        
        crate::print_str("  [");
        crate::print_hex((0x3000 + i * 4) as usize);
        crate::print_str("] = ");
        crate::print_hex(val as usize);
        crate::print_str("\n");
    }
    crate::print_str("==========================================\n\n");
}

pub unsafe fn rp1_boot_sequence() {
    crate::print_str("RP1: running minimal boot sequence...\n");

    // 1) Boot config magic
    write32(get_bootcfg_addr(), 0x5A00_0001);
    TIMER.delay_ms(10);

    // 2) Deassert reset
    write32(get_reset_ctrl_addr(), 0x0000_0000);
    TIMER.delay_ms(10);

    // 3) Enable clock
    let before = read32(get_clk_ctrl_addr());
    write32(get_clk_ctrl_addr(), before | 0x800);

    TIMER.delay_ms(1);

    // 4) Doorbell kick
    write32(get_doorbell_addr(), 1);
    TIMER.delay_ms(10);

    crate::print_str("RP1: boot sequence complete\n");
    
    // 5) Dump BAR0 header to verify address decoding
    // Only dump if BAR0 is initialized to a valid address
    let bar0_base = get_bar0_base();
    if bar0_base >= 0x0000006000000000 && (bar0_base & 0xFFF) == 0 {
        dump_bar0_header(bar0_base);
    } else {
        crate::print_str("RP1: BAR0 not initialized or invalid (0x");
        crate::print_hex(bar0_base as usize);
        crate::print_str("), skipping header dump\n");
    }
    
    // Note: Mailbox will be initialized later after BAR0 is programmed
}

unsafe fn dump_runtime_mailbox(label: &str, base: *mut u32) {
    crate::print_str(label);
    crate::print_str(" base=0x");
    crate::print_hex(base as usize);
    crate::print_str(" [0]=0x");
    crate::print_hex(core::ptr::read_volatile(base.add(0)) as usize);
    crate::print_str(" [1]=0x");
    crate::print_hex(core::ptr::read_volatile(base.add(1)) as usize);
    crate::print_str(" [2]=0x");
    crate::print_hex(core::ptr::read_volatile(base.add(2)) as usize);
    crate::print_str(" [3]=0x");
    crate::print_hex(core::ptr::read_volatile(base.add(3)) as usize);
    crate::print_str("\n");

    // Also show runtime doorbell/clock/reset for quick sanity checks
    let doorbell = read32(get_doorbell_addr());
    let reset = read32(get_reset_ctrl_addr());
    let clock = read32(get_clk_ctrl_addr());
    crate::print_str("  doorbell=0x");
    crate::print_hex(doorbell as usize);
    crate::print_str(" reset=0x");
    crate::print_hex(reset as usize);
    crate::print_str(" clk_ctrl=0x");
    crate::print_hex(clock as usize);
    crate::print_str("\n");
}

// =====================================================
// RP1 FIRMWARE SYSTEM OPERATIONS - PUBLIC API
// =====================================================

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

/// Perform RP1 firmware system operation (Linux rp1-firmware.c equivalent)
/// 
/// RP1 Firmware Mailbox uses a 16-byte fixed buffer format:
///   offset 0x00: FW operation code (32bit)
///   offset 0x04: FW argument      (32bit)
///   offset 0x08: status bit (FW writes back) (32bit)
///   offset 0x0C: reserved         (32bit)
/// 
/// This is NOT a property mailbox, NOT a channel-based mailbox,
/// and NOT a 64-bit message packet format.
/// 
/// Communication protocol (from Linux kernel drivers/mailbox/rp1-mailbox.c):
/// 1. Write command to mailbox buffer
/// 2. Ring DOORBELL by writing MBOX_EVENT_BIT to SYSCFG_PROC_EVENTS + HW_SET_BITS
/// 3. Wait for firmware response via SYSCFG_HOST_EVENT_IRQ
/// 4. Clear HOST_EVENT by writing to SYSCFG_HOST_EVENTS + HW_CLR_BITS
pub unsafe fn rp1_fw_sys_op(op: u32, arg: u32) -> bool {
    let base = RP1_MBOX_RUNTIME_BASE.load(Ordering::SeqCst) as *mut u32;
    
    if base.is_null() {
        crate::print_str("[RP1-FW] mailbox base not initialized\n");
        return false;
    }

    // 1) Dump mailbox window before touching it
    dump_runtime_mailbox("[RP1-FW] mailbox window BEFORE request", base);

    // 2) Write request (16-byte buffer)
    crate::print_str("[RP1-FW] sending op=0x");
    crate::print_hex(op as usize);
    crate::print_str(" arg=0x");
    crate::print_hex(arg as usize);
    crate::print_str(" @");
    crate::print_hex(base as usize);
    crate::print_str("\n");

    core::ptr::write_volatile(base.add(0), op);     // offset 0x0: operation code
    core::ptr::write_volatile(base.add(1), arg);    // offset 0x4: argument
    core::ptr::write_volatile(base.add(2), 0);      // offset 0x8: status = 0
    core::ptr::write_volatile(base.add(3), 0);      // offset 0xC: reserved

    dump_runtime_mailbox("[RP1-FW] mailbox window AFTER write", base);

    // 3) Ring DOORBELL: write MBOX_EVENT_BIT to SYSCFG_PROC_EVENTS + HW_SET_BITS
    //    This atomically sets the event bit to notify RP1 firmware
    crate::print_str("[RP1-FW] ringing DOORBELL (SYSCFG_PROC_EVENTS + HW_SET_BITS)\n");
    write32(get_syscfg_proc_events() + HW_SET_BITS, MBOX_EVENT_BIT);
    
    // Read back to ensure write completed
    let proc_events = read32(get_syscfg_proc_events());
    crate::print_str("[RP1-FW] SYSCFG_PROC_EVENTS=0x");
    crate::print_hex(proc_events as usize);
    crate::print_str("\n");

    // 4) Poll for firmware response via SYSCFG_HOST_EVENT_IRQ
    //    Firmware will set MBOX_EVENT_BIT in HOST_EVENT_IRQ when it responds
    crate::print_str("[RP1-FW] waiting for firmware response (SYSCFG_HOST_EVENT_IRQ)\n");
    
    let mut got_response = false;
    for i in 0..2000 {
        let host_irq = read32(get_syscfg_host_event_irq());
        
        if (host_irq & MBOX_EVENT_BIT) != 0 {
            crate::print_str("[RP1-FW] firmware responded! HOST_EVENT_IRQ=0x");
            crate::print_hex(host_irq as usize);
            crate::print_str("\n");
            
            // Clear the HOST_EVENT by writing to SYSCFG_HOST_EVENTS + HW_CLR_BITS
            write32(get_syscfg_host_events() + HW_CLR_BITS, MBOX_EVENT_BIT);
            
            got_response = true;
            break;
        }
        
        if i % 400 == 0 && i != 0 {
            crate::print_str("[RP1-FW] still waiting, HOST_EVENT_IRQ=0x");
            crate::print_hex(host_irq as usize);
            crate::print_str("\n");
        }
        TIMER.delay_us(5);
    }

    if !got_response {
        crate::print_str("[RP1-FW] timeout waiting for firmware DOORBELL response\n");
        dump_runtime_mailbox("[RP1-FW] mailbox window ON TIMEOUT", base);
        return false;
    }

    // 5) Check status in mailbox buffer
    let status = core::ptr::read_volatile(base.add(2));
    
    if status == 1 {
        crate::print_str("[RP1-FW] op completed successfully, status=1\n");
        dump_runtime_mailbox("[RP1-FW] mailbox window AFTER completion", base);
        return true;
    } else {
        crate::print_str("[RP1-FW] op=0x");
        crate::print_hex(op as usize);
        crate::print_str(" failed, status=0x");
        crate::print_hex(status as usize);
        crate::print_str("\n");
        dump_runtime_mailbox("[RP1-FW] mailbox window AFTER failure", base);
        return false;
    }
}

pub unsafe fn rp1_fw_init_ethernet() -> bool {
    crate::print_str("[RP1-FW] Initializing Ethernet subsystem via firmware...\n");

    // 1. Legacy GBE enable (may be ignored by newer firmware)
    if !rp1_fw_sys_op(FW_OP_ENABLE_GBE, 1) {
        crate::print_str("[RP1-FW] GBE enable failed (ignored on new firmware)\n");
    } else {
        crate::print_str("[RP1-FW] GBE enabled\n");
    }

    // 2. Legacy MDIO enable (may be ignored)
    if !rp1_fw_sys_op(FW_OP_ENABLE_MDIO, 1) {
        crate::print_str("[RP1-FW] MDIO enable failed (ignored on new firmware)\n");
    } else {
        crate::print_str("[RP1-FW] MDIO enabled\n");
    }

    // 3. Legacy clock enable (may be ignored)
    if !rp1_fw_sys_op(FW_OP_ENABLE_CLOCK, 1) {
        crate::print_str("[RP1-FW] Clock enable failed (ignored on new firmware)\n");
    } else {
        crate::print_str("[RP1-FW] Clocks enabled\n");
    }

    // 4. New firmware Ethernet enable (real gate)
    crate::print_str("[RP1-FW] Enabling Ethernet domain via FW_OP_ETH_ENABLE...\n");
    if !rp1_fw_sys_op(FW_OP_ETH_ENABLE, 1) {
        crate::print_str("[RP1-FW] Ethernet enable failed (firmware did not respond)\n");
        return false;
    }
    crate::print_str("[RP1-FW] Ethernet enabled via firmware\n");

    crate::print_str("[RP1-FW] Ethernet firmware initialization complete\n");
    true
}
