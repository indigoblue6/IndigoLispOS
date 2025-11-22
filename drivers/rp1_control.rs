// drivers/rp1_control.rs
//
// High-level helpers for powering / clocking / resetting RP1 peripherals.

use core::ptr;

/// High-level helper used before Ethernet init to make sure RP1 GBE/PHY
/// domains are powered and clocked.
pub fn rp1_init_gbe() {
    crate::print_str("[RP1] Powering up GBE + PHY via low-level regs\n");
    rp1_enable_ethernet_lowlevel();
    crate::print_str("[RP1] GBE + PHY ready\n");
}

// -----------------------------------------------------------------------
// Low-level register definitions (direct RP1 control block access)
// -----------------------------------------------------------------------
pub const RP1_SYS_BASE: usize = 0x60_0000_0000;
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
        crate::print_str("[RP1] Step 1: Assert reset\n");
        let rst = r32(RP1_RST_CTRL);
        crate::print_str("  RST_CTRL before: 0x");
        crate::print_hex(rst as usize);
        crate::print_str("\n");
        w32(RP1_RST_CTRL, rst | 0xFFFF); // Assert reset first
        let rst_after = r32(RP1_RST_CTRL);
        crate::print_str("  RST_CTRL after: 0x");
        crate::print_hex(rst_after as usize);
        crate::print_str("\n");

        crate::print_str("[RP1] Step 2: Power ON\n");
        let pwr = r32(RP1_PWR_CTRL);
        crate::print_str("  PWR_CTRL before: 0x");
        crate::print_hex(pwr as usize);
        crate::print_str("\n");
        w32(RP1_PWR_CTRL, pwr | 0xFFFF);
        let pwr_after = r32(RP1_PWR_CTRL);
        crate::print_str("  PWR_CTRL after: 0x");
        crate::print_hex(pwr_after as usize);
        crate::print_str("\n");

        crate::print_str("[RP1] Step 3: Clock enable\n");
        let clk = r32(RP1_CLK_CTRL);
        crate::print_str("  CLK_CTRL before: 0x");
        crate::print_hex(clk as usize);
        crate::print_str("\n");
        w32(RP1_CLK_CTRL, clk | 0xFFFF);
        let clk_after = r32(RP1_CLK_CTRL);
        crate::print_str("  CLK_CTRL after: 0x");
        crate::print_hex(clk_after as usize);
        crate::print_str("\n");

        crate::print_str("[RP1] Step 4: Initialize CLKGEN (Brute-force enable)\n");
        // Initialize CLKGEN registers. Try to enable everything in the range.
        const CLKGEN_BASE: usize = RP1_SYS_BASE + 0x18000;
        
        // Try enabling clocks in range 0x18000 - 0x18200
        // Heuristic:
        // If addr % 8 == 0, assume CTRL -> write 0xB00 (Enable | Side | Src)
        // If addr % 8 == 4, assume DIV -> write 0x100 (Div=1.0 approx)
        for i in 0..128 { 
            let addr = CLKGEN_BASE + (i * 4);
            if (addr & 4) == 0 {
                // Offset 0, 8, 16... -> CTRL
                w32(addr, 0x100 | 0x200 | 0x800); 
            } else {
                // Offset 4, 12, 20... -> DIV
                w32(addr, 0x100); 
            }
        }
        
        crate::print_str("  CLKGEN[0x18000]: 0x");
        crate::print_hex(r32(CLKGEN_BASE) as usize);
        crate::print_str("\n");

        crate::print_str("[RP1] Step 5: Deassert reset\n");
        let rst2 = r32(RP1_RST_CTRL);
        crate::print_str("  RST_CTRL before: 0x");
        crate::print_hex(rst2 as usize);
        crate::print_str("\n");
        w32(RP1_RST_CTRL, rst2 & !0xFFFF);
        let rst2_after = r32(RP1_RST_CTRL);
        crate::print_str("  RST_CTRL after: 0x");
        crate::print_hex(rst2_after as usize);
        crate::print_str("\n");

        // Small delay to let things stabilize
        for _ in 0..100000 {
            core::hint::spin_loop();
        }
    }
}
