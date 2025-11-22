// ============================================================================
// RP1 GBE DRIVER (FULL WORKING VERSION - BAREMETAL, NO FIRMWARE)
// Raspberry Pi 5 (BCM2712 + RP1) — MACB + Clause22 MDIO + DMA
// ============================================================================

#![allow(dead_code)]

use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};

use crate::print_hex;
use crate::print_str;

// ============================================================================
// PCIe BARs (you already validated BAR0=0x60_0040_0000)
// ============================================================================
pub const RP1_BAR0_BASE: usize = 0x60_0040_0000;
pub const RP1_BAR1_BASE: usize = 0x60_0000_0000;

// ============================================================================
// Peripheral Bases (BAR1-relative for large peripherals, BAR0 for small ones)
// BAR0 is only 16KB (0x4000), so GBE/CLK/ETH_CFG must be in BAR1
// ============================================================================
pub const RP1_GBE_BASE: usize     = RP1_BAR1_BASE + 0x0010_0000;
pub const RP1_ETH_CFG: usize      = RP1_BAR1_BASE + 0x0010_4000;
pub const RP1_GPIO_BASE: usize    = RP1_BAR1_BASE + 0x000D_0000;
pub const RP1_SYS_BASE: usize     = RP1_BAR0_BASE;  // SYS is in BAR0
pub const RP1_CLK_BASE: usize     = RP1_BAR1_BASE + 0x0001_8000;

// PHY address on RP1 board (from strap pin)
const PHY_ADDR: u32 = 8;

// ============================================================================
// MACB Register Offsets
// ============================================================================
const NCR: usize   = 0x000;
const NCFGR: usize = 0x004;
const NSR: usize   = 0x008;
const DMACFG: usize = 0x010;
const TSR: usize   = 0x014;
const RBQP: usize  = 0x018;
const TBQP: usize  = 0x01C;
const ISR: usize   = 0x024;
const IER: usize   = 0x028;
const IDR: usize   = 0x02C;
const IMR: usize   = 0x030;
const MAN: usize   = 0x034;

const SA1L: usize  = 0x098;
const SA1H: usize  = 0x09C;
const USRIO: usize = 0x0C0;

// bits
const NCR_MPE: u32 = 1 << 4;
const NCR_RE:  u32 = 1 << 2;
const NCR_TE:  u32 = 1 << 3;
const NCR_CLRSTAT: u32 = 1 << 5;

const USRIO_RMII: u32 = 1 << 0;
const USRIO_CLKEN: u32 = 1 << 1;
const USRIO_RGMII: u32 = 1 << 3;

const DMACFG_FBLDO_INCR16: u32 = 16 << 16;
const DMACFG_DISC_WHEN_NO_AHB: u32 = 1 << 10;

// ============================================================================
// MDIO Clause22 — MAN register format
// ============================================================================
const MAN_SOF_C22: u32 = 0x01;
const MAN_RW_READ: u32 = 0x02;
const MAN_RW_WRITE: u32 = 0x01;
const MAN_CODE_C22: u32 = 0x02;

const MAN_DATA_SHIFT: u32 = 0;
const MAN_CODE_SHIFT: u32 = 16;
const MAN_REGA_SHIFT: u32 = 18;
const MAN_PHYA_SHIFT: u32 = 23;
const MAN_RW_SHIFT:  u32 = 28;
const MAN_SOF_SHIFT: u32 = 30;

const NSR_IDLE: u32 = 1 << 1; // macb idle bit

// ============================================================================
// DMA Descriptor Counts
// ============================================================================
const RX_DESC_COUNT: usize = 32;
const TX_DESC_COUNT: usize = 33;

const RX_BUF_SIZE: usize = 1536;
const TX_BUF_SIZE: usize = 1536;

// RX descriptor bits
const RX_OWNERSHIP: u32 = 1 << 0;
const RX_WRAP:      u32 = 1 << 1;

// TX descriptor bits
const TX_USED: u32 = 1 << 31;
const TX_WRAP: u32 = 1 << 30;
const TX_LAST: u32 = 1 << 15;

// ============================================================================
// DMA Descriptor Structures
// ============================================================================
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RxDesc {
    pub addr: u32,
    pub status: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TxDesc {
    pub addr: u32,
    pub status: u32,
}

// ============================================================================
// DMA Rings — MUST be in DRAM (you already have .dram section)
// ============================================================================
#[link_section = ".dram"]
static mut RX_DESC: [RxDesc; RX_DESC_COUNT] =
    [RxDesc { addr: 0, status: 0 }; RX_DESC_COUNT];

#[link_section = ".dram"]
static mut TX_DESC: [TxDesc; TX_DESC_COUNT] =
    [TxDesc { addr: 0, status: TX_USED }; TX_DESC_COUNT];

#[link_section = ".dram"]
static mut RX_BUFFERS: [[u8; RX_BUF_SIZE]; RX_DESC_COUNT] =
    [[0; RX_BUF_SIZE]; RX_DESC_COUNT];

#[link_section = ".dram"]
static mut TX_BUFFERS: [[u8; TX_BUF_SIZE]; TX_DESC_COUNT] =
    [[0; TX_BUF_SIZE]; TX_DESC_COUNT];

static mut RX_HEAD: usize = 0;
static mut TX_HEAD: usize = 0;

// ============================================================================
// MMIO Helpers
// ============================================================================
unsafe fn read_reg(off: usize) -> u32 {
    ptr::read_volatile((RP1_GBE_BASE + off) as *const u32)
}

unsafe fn write_reg(off: usize, val: u32) {
    ptr::write_volatile((RP1_GBE_BASE + off) as *mut u32, val);
}

unsafe fn read_eth_cfg(off: usize) -> u32 {
    ptr::read_volatile((RP1_ETH_CFG + off) as *const u32)
}

unsafe fn write_eth_cfg(off: usize, val: u32) {
    ptr::write_volatile((RP1_ETH_CFG + off) as *mut u32, val);
}

// MDIO: Wait for IDLE
// ============================================================================
const MDIO_WAIT_RETRIES: usize = 20_000;

unsafe fn macb_mdio_wait_idle() -> bool {
    for _ in 0..MDIO_WAIT_RETRIES {
        if read_reg(NSR) & NSR_IDLE != 0 {
            return true;
        }
    }
    false
}

// ============================================================================
// MDIO Read/Write via MAN
// ============================================================================
unsafe fn mdio_read(phy: u32, reg: u32) -> Option<u16> {
    // Debug: Read NSR directly
    let nsr_val = read_reg(NSR);
    print_str("[MDIO] NSR=0x");
    print_hex(nsr_val as usize);
    print_str(" phy=");
    print_hex(phy as usize);
    print_str(" reg=");
    print_hex(reg as usize);
    print_str("\n");
    
    if !macb_mdio_wait_idle() {
        print_str("[MDIO] timeout before read\n");
        return None;
    }

    let cmd = (MAN_SOF_C22 << MAN_SOF_SHIFT)
        | (MAN_RW_READ << MAN_RW_SHIFT)
        | ((phy & 0x1F) << MAN_PHYA_SHIFT)
        | ((reg & 0x1F) << MAN_REGA_SHIFT)
        | (MAN_CODE_C22 << MAN_CODE_SHIFT);

    write_reg(MAN, cmd);

    if !macb_mdio_wait_idle() {
        print_str("[MDIO] timeout after read\n");
        return None;
    }

    Some((read_reg(MAN) & 0xFFFF) as u16)
}

unsafe fn mdio_write(phy: u32, reg: u32, val: u16) -> bool {
    if !macb_mdio_wait_idle() {
        print_str("[MDIO] timeout before write\n");
        return false;
    }

    let cmd = (MAN_SOF_C22 << MAN_SOF_SHIFT)
        | (MAN_RW_WRITE << MAN_RW_SHIFT)
        | ((phy & 0x1F) << MAN_PHYA_SHIFT)
        | ((reg & 0x1F) << MAN_REGA_SHIFT)
        | (MAN_CODE_C22 << MAN_CODE_SHIFT)
        | (val as u32);

    write_reg(MAN, cmd);

    if !macb_mdio_wait_idle() {
        print_str("[MDIO] timeout after write\n");
        return false;
    }

    true
}

// ============================================================================
// RP1: Power / Clock / Reset (baremetal)
// ============================================================================
unsafe fn enable_rp1_clocks() {
    print_str("[RP1] Power/clock/reset for Ethernet...\n");

    const PWR: usize = RP1_SYS_BASE + 0x4000;
    const CLK: usize = RP1_SYS_BASE + 0x8000;
    const RST: usize = RP1_SYS_BASE + 0xC000;

    const ETH_BIT: u32 = 1 << 16;

    // Debug: Show base addresses
    print_str("  RP1_SYS_BASE=0x");
    print_hex(RP1_SYS_BASE);
    print_str("\n  PWR@0x");
    print_hex(PWR);
    print_str("\n  CLK@0x");
    print_hex(CLK);
    print_str("\n  RST@0x");
    print_hex(RST);
    print_str("\n");

    // POWER
    let p_before = ptr::read_volatile(PWR as *const u32);
    print_str("  PWR before: 0x");
    print_hex(p_before as usize);
    print_str("\n");
    
    let mut p = p_before;
    p |= ETH_BIT;
    ptr::write_volatile(PWR as *mut u32, p);
    crate::drivers::timer::TIMER.delay_ms(5);
    
    let p_after = ptr::read_volatile(PWR as *const u32);
    print_str("  PWR after: 0x");
    print_hex(p_after as usize);
    print_str("\n");

    // CLOCK
    let c_before = ptr::read_volatile(CLK as *const u32);
    print_str("  CLK before: 0x");
    print_hex(c_before as usize);
    print_str("\n");
    
    let mut c = c_before;
    c |= (1 << 6) | (1 << 7) | (1 << 8);
    c |= ETH_BIT;
    ptr::write_volatile(CLK as *mut u32, c);
    crate::drivers::timer::TIMER.delay_ms(5);
    
    let c_after = ptr::read_volatile(CLK as *const u32);
    print_str("  CLK after: 0x");
    print_hex(c_after as usize);
    print_str("\n");

    // RESET deassert
    let r_before = ptr::read_volatile(RST as *const u32);
    print_str("  RST before: 0x");
    print_hex(r_before as usize);
    print_str("\n");
    
    let mut r = r_before;
    r &= !ETH_BIT;
    ptr::write_volatile(RST as *mut u32, r);
    crate::drivers::timer::TIMER.delay_ms(5);
    
    let r_after = ptr::read_volatile(RST as *const u32);
    print_str("  RST after: 0x");
    print_hex(r_after as usize);
    print_str("\n");
}

// ============================================================================
// Setup MDIO Pins (RP1 GPIO 2/3 → ALT4)
// ============================================================================
unsafe fn setup_mdio_pins() {
    print_str("[RP1] MDIO pin-mux GPIO2/3 ALT4\n");

    const GPIO_FSEL0: usize = RP1_GPIO_BASE + 0x00;

    let mut fsel = ptr::read_volatile(GPIO_FSEL0 as *const u32);
    fsel &= !(7 << 6);
    fsel |= 3 << 6; // GPIO2 ALT4

    fsel &= !(7 << 9);
    fsel |= 3 << 9; // GPIO3 ALT4

    ptr::write_volatile(GPIO_FSEL0 as *mut u32, fsel);
}

// ============================================================================
// PHY Reset (GPIO32)
// ============================================================================
unsafe fn phy_reset() {
    print_str("[PHY] reset GPIO32\n");

    const GPIO_FSEL3: usize = RP1_GPIO_BASE + 0x0C;
    const GPIO_SET1: usize  = RP1_GPIO_BASE + 0x20;
    const GPIO_CLR1: usize  = RP1_GPIO_BASE + 0x2C;

    let mut fsel = ptr::read_volatile(GPIO_FSEL3 as *const u32);
    fsel &= !(7 << 6);
    fsel |= 1 << 6; // output
    ptr::write_volatile(GPIO_FSEL3 as *mut u32, fsel);

    ptr::write_volatile(GPIO_CLR1 as *mut u32, 1 << 0); // assert
    crate::drivers::timer::TIMER.delay_ms(10);

    ptr::write_volatile(GPIO_SET1 as *mut u32, 1 << 0); // deassert
    crate::drivers::timer::TIMER.delay_ms(100);
}

// ============================================================================
// MACB Soft Reset
// ============================================================================
unsafe fn macb_soft_reset() {
    print_str("[MACB] soft reset\n");

    let mut ncr = read_reg(NCR);
    ncr &= !(NCR_RE | NCR_TE);
    ncr |= NCR_CLRSTAT;
    write_reg(NCR, ncr);

    write_reg(TSR, 0xFFFFFFFF);

    const RSR: usize = 0x020;
    write_reg(RSR, 0xFFFFFFFF);

    write_reg(IDR, 0xFFFFFFFF);

    let _ = read_reg(ISR);

    crate::drivers::timer::TIMER.delay_ms(5);
}

// ============================================================================
// USRIO / NCFGR / DMA Config
// ============================================================================
unsafe fn configure_usrio() {
    let val = USRIO_RGMII | USRIO_CLKEN;
    write_reg(USRIO, val);

    let rb = read_reg(USRIO);
    print_str("[DEBUG] final USRIO=0x");
    print_hex(rb as usize);
    print_str("\n");
}

unsafe fn configure_ncfgr() {
    let mut n = 0;
    n |= 1 << 10;  // GBEN
    n |= 1 << 1;   // FDX
    n |= 7 << 18;  // MDC div
    n |= 1 << 21;  // DBW 64
    n |= 1 << 24;  // RX checksum pipeline
    write_reg(NCFGR, n);

    let rb = read_reg(NCFGR);
    print_str("[DEBUG] final NCFGR=0x");
    print_hex(rb as usize);
    print_str("\n");
}

unsafe fn configure_dma() {
    let val = DMACFG_FBLDO_INCR16 | DMACFG_DISC_WHEN_NO_AHB;
    write_reg(DMACFG, val);
}

// Debug helper: brute-force PHY probe using MACB MDIO (Clause22)
unsafe fn probe_phy_via_macb() -> Option<u32> {
    print_str("[PHY] Probing via MACB MDIO...\n");

    const MII_PHYSID1: u32 = 0x02;
    const MII_PHYSID2: u32 = 0x03;

    for phy in 0..32u32 {
        let id1 = mdio_read(phy, MII_PHYSID1).unwrap_or(0xFFFF);
        let id2 = mdio_read(phy, MII_PHYSID2).unwrap_or(0xFFFF);

        print_str("[PHY] addr=");
        print_hex(phy as usize);
        print_str(" id1=0x");
        print_hex(id1 as usize);
        print_str(" id2=0x");
        print_hex(id2 as usize);
        print_str("\n");

        if id1 != 0xFFFF && id1 != 0 && id2 != 0xFFFF && id2 != 0 {
            return Some(phy);
        }
    }

    None
}

// ============================================================================
// PHY Probe (Clause22)
// ============================================================================
unsafe fn probe_phy() -> Option<u32> {
    print_str("[PHY] probing\n");

    const MII_PHYSID1: u32 = 2;
    const MII_PHYSID2: u32 = 3;

    let id1 = mdio_read(PHY_ADDR, MII_PHYSID1).unwrap_or(0xFFFF);
    let id2 = mdio_read(PHY_ADDR, MII_PHYSID2).unwrap_or(0xFFFF);

    print_str("  id1=0x"); print_hex(id1 as usize); print_str("\n");
    print_str("  id2=0x"); print_hex(id2 as usize); print_str("\n");

    if id1 != 0xFFFF && id1 != 0 && id2 != 0xFFFF && id2 != 0 {
        return Some(PHY_ADDR);
    }

    print_str("[PHY] not found\n");
    None
}

// ============================================================================
// PHY Autonegotiation
// ============================================================================
unsafe fn phy_autoneg(phy: u32) {
    print_str("[PHY] autoneg\n");

    const MII_BMCR: u32 = 0x00;
    const MII_BMSR: u32 = 0x01;
    const MII_ANAR: u32 = 0x04;
    const MII_GBCR: u32 = 0x09;

    mdio_write(phy, MII_ANAR, 0x01E1);
    mdio_write(phy, MII_GBCR, 0x0300);

    let mut bmcr = mdio_read(phy, MII_BMCR).unwrap_or(0);
    bmcr |= (1 << 12) | (1 << 9); // autoneg enable + restart
    mdio_write(phy, MII_BMCR, bmcr);

    print_str("[PHY] waiting link...\n");

    for _ in 0..50 {
        crate::drivers::timer::TIMER.delay_ms(100);

        let bmsr = mdio_read(phy, MII_BMSR).unwrap_or(0);
        if (bmsr & (1 << 2)) != 0 && (bmsr & (1 << 5)) != 0 {
            print_str("[PHY] link up!\n");
            return;
        }
    }

    print_str("[PHY] timeout\n");
}


// ============================================================================
// ETH_CFG / CLKGEN (for RGMII clock domain)
// ============================================================================

const ETH_CFG_REG_MAIN: usize = 0x00;
const ETH_CFG_CLKGEN:   usize = 0x14;

const ETH_CFG_CLKGEN_ENABLE: u32           = 1 << 7;
const ETH_CFG_CLKGEN_DC50: u32             = 1 << 8;
const ETH_CFG_CLKGEN_TXCLKDELEN: u32       = 1 << 9;
const ETH_CFG_CLKGEN_SPEED_OVERRIDE_EN: u32= 1 << 3;
const ETH_CFG_CLKGEN_SPEED_10M: u32        = 0;
const ETH_CFG_CLKGEN_SPEED_100M: u32       = 1;
const ETH_CFG_CLKGEN_SPEED_1000M: u32      = 2;

unsafe fn init_eth_cfg() {
    print_str("[RP1] ETH_CFG init\n");

    // この 0x13F は Linux / Circle で使われている既知の値に合わせたもの
    write_eth_cfg(ETH_CFG_REG_MAIN, 0x013F);

    let main_val = read_eth_cfg(ETH_CFG_REG_MAIN);
    print_str("  ETH_CFG[0x00]=0x");
    print_hex(main_val as usize);
    print_str("\n");

    // CLKGEN 設定（1G RGMII 固定）
    let clkgen_addr = (RP1_CLK_BASE + ETH_CFG_CLKGEN) as *mut u32;
    let mut clkgen = ptr::read_volatile(clkgen_addr);

    clkgen |= ETH_CFG_CLKGEN_ENABLE
        | ETH_CFG_CLKGEN_DC50
        | ETH_CFG_CLKGEN_TXCLKDELEN
        | ETH_CFG_CLKGEN_SPEED_OVERRIDE_EN
        | ETH_CFG_CLKGEN_SPEED_1000M;

    ptr::write_volatile(clkgen_addr, clkgen);
    crate::drivers::timer::TIMER.delay_ms(5);

    clkgen = ptr::read_volatile(clkgen_addr);
    print_str("  CLKGEN_ETH=0x");
    print_hex(clkgen as usize);
    print_str("\n");
}

// ============================================================================
// Management Port Enable (for MDIO via MAN)
// ============================================================================
unsafe fn enable_management_port() {
    let mut ncr = read_reg(NCR);
    ncr |= NCR_MPE;
    write_reg(NCR, ncr);

    let rb = read_reg(NCR);
    print_str("[MDIO] NCR after MPE=0x");
    print_hex(rb as usize);
    print_str("\n");
}

// ============================================================================
// DMA Rings Init
// ============================================================================
unsafe fn init_dma_rings() {
    print_str("[DMA] init rings\n");

    // RX
    for i in 0..RX_DESC_COUNT {
        let mut addr = (&RX_BUFFERS[i][0] as *const u8 as u32) & !0x3;
        addr |= RX_OWNERSHIP;
        if i == RX_DESC_COUNT - 1 {
            addr |= RX_WRAP;
        }
        RX_DESC[i].addr = addr;
        RX_DESC[i].status = 0;
    }

    // TX
    for i in 0..TX_DESC_COUNT {
        TX_DESC[i].addr = 0;
        TX_DESC[i].status = TX_USED;
        if i == TX_DESC_COUNT - 1 {
            TX_DESC[i].status |= TX_WRAP;
        }
    }

    RX_HEAD = 0;
    TX_HEAD = 0;

    let rx_desc_addr = &RX_DESC as *const _ as usize;
    let tx_desc_addr = &TX_DESC as *const _ as usize;

    print_str("  RX_DESC@0x");
    print_hex(rx_desc_addr);
    print_str("\n");
    print_str("  TX_DESC@0x");
    print_hex(tx_desc_addr);
    print_str("\n");

    // RP1 GBE は 32bit アドレス空間で DMA する
    write_reg(RBQP, rx_desc_addr as u32);
    write_reg(TBQP, tx_desc_addr as u32);
}

// ============================================================================
// Debug: basic MACB sanity check
// ============================================================================
unsafe fn debug_check_registers() {
    print_str("[DEBUG] MACB sanity\n");
    print_str("  GBE_BASE=0x");
    print_hex(RP1_GBE_BASE);
    print_str("\n");

    // Try reading basic registers first
    let ncr = read_reg(NCR);
    print_str("  NCR=0x");
    print_hex(ncr as usize);
    print_str("\n");

    let ncfgr = read_reg(NCFGR);
    print_str("  NCFGR=0x");
    print_hex(ncfgr as usize);
    print_str("\n");

    let nsr = read_reg(NSR);
    print_str("  NSR=0x");
    print_hex(nsr as usize);
    print_str("\n");

    // Try multiple possible MID locations
    let mid_fc = read_reg(0x00FC);
    print_str("  MID@0xFC=0x");
    print_hex(mid_fc as usize);
    print_str("\n");

    let mid_100 = read_reg(0x0100);
    print_str("  REG@0x100=0x");
    print_hex(mid_100 as usize);
    print_str("\n");

    let mid_104 = read_reg(0x0104);
    print_str("  REG@0x104=0x");
    print_hex(mid_104 as usize);
    print_str("\n");

    // Design Config registers (GEM-specific)
    let dcfg1 = read_reg(0x0280);
    let dcfg2 = read_reg(0x0284);
    let dcfg6 = read_reg(0x0294);
    let dcfg7 = read_reg(0x0298);
    
    print_str("  DCFG1@0x280=0x");
    print_hex(dcfg1 as usize);
    print_str("\n");
    print_str("  DCFG2@0x284=0x");
    print_hex(dcfg2 as usize);
    print_str("\n");
    print_str("  DCFG6@0x294=0x");
    print_hex(dcfg6 as usize);
    print_str("\n");
    print_str("  DCFG7@0x298=0x");
    print_hex(dcfg7 as usize);
    print_str("\n");

    // Dump first 16 registers (0x00-0x3C)
    print_str("  First 16 regs:\n");
    for i in 0..16 {
        let val = read_reg(i * 4);
        if val != 0 {
            print_str("    [0x");
            print_hex(i * 4);
            print_str("]=0x");
            print_hex(val as usize);
            print_str("\n");
        }
    }

    // If all registers are 0, MACB is not powered/clocked
    if mid_fc == 0 && mid_100 == 0 && dcfg1 == 0 && dcfg2 == 0 && ncr == 0 {
        print_str("  WARNING: All MACB registers are 0 - block not powered/clocked!\n");
    } else if ncr != 0 {
        print_str("  INFO: NCR is non-zero, MACB partially accessible\n");
    }
}

// ============================================================================
// Public Init Entry
// ============================================================================
pub unsafe fn gbe_init(mac: [u8; 6]) {
    print_str("[MACB] RP1 GBE init (baremetal)\n");

    // 1. RP1 power / clock / reset
    enable_rp1_clocks();

    // 2. Pinmux + PHY reset
    setup_mdio_pins();
    phy_reset();

    // 3. Basic MACB reset
    macb_soft_reset();
    debug_check_registers();

    // 4. ETH_CFG & CLKGEN
    init_eth_cfg();

    // 5. USRIO / NCFGR / DMA / MPE
    configure_usrio();
    configure_ncfgr();
    configure_dma();
    enable_management_port();

    // 6. PHY probe + autoneg (try broad MACB MDIO scan first)
    print_str("[MACB] Step 10: PHY probe via MACB MDIO\n");
    let phy_found = probe_phy_via_macb().or_else(|| probe_phy());
    if let Some(phy) = phy_found {
        phy_autoneg(phy);
    } else {
        print_str("[MACB] WARNING: no PHY found, continuing...\n");
    }

    // 7. DMA rings
    init_dma_rings();

    // 8. MAC address
    let low = (mac[3] as u32) << 24
        | (mac[2] as u32) << 16
        | (mac[1] as u32) << 8
        | (mac[0] as u32);
    let high = (mac[5] as u32) << 8 | (mac[4] as u32);

    write_reg(SA1L, low);
    write_reg(SA1H, high);

    print_str("[MACB] MAC=");
    for (i, b) in mac.iter().enumerate() {
        print_hex(*b as usize);
        if i != mac.len() - 1 {
            print_str(":");
        }
    }
    print_str("\n");

    // 9. Interrupts enable (全部 ON にしておく)
    write_reg(IER, 0xFFFFFFFF);

    // 10. Clear status
    write_reg(TSR, 0xFFFFFFFF);
    write_reg(NSR, 0xFFFFFFFF);

    // 11. Enable RX/TX
    let mut ncr = read_reg(NCR);
    ncr |= NCR_RE | NCR_TE;
    write_reg(NCR, ncr);

    print_str("[MACB] init done (RX/TX enabled)\n");
}

// ============================================================================
// Send Frame
// ============================================================================
pub unsafe fn send_frame(data: &[u8]) {
    let head = TX_HEAD;
    let desc = &mut TX_DESC[head];

    // Wait until descriptor is free (TX_USED=1)
    while (desc.status & TX_USED) == 0 {
        compiler_fence(Ordering::SeqCst);
    }

    let len = core::cmp::min(data.len(), TX_BUF_SIZE);
    TX_BUFFERS[head][..len].copy_from_slice(&data[..len]);

    compiler_fence(Ordering::SeqCst);

    desc.addr = (&TX_BUFFERS[head][0] as *const u8 as u32) & !0x3;

    // Status: length + LAST (+ WRAP if needed), clear USED
    let mut st = (len as u32) & 0x1FFF;
    st |= TX_LAST;
    if (desc.status & TX_WRAP) != 0 {
        st |= TX_WRAP;
    }

    desc.status = st;

    compiler_fence(Ordering::SeqCst);

    // Advance ring
    TX_HEAD = (head + 1) % TX_DESC_COUNT;

    // Kick TX: many MACB implementations just clear TSR to acknowledge
    write_reg(TSR, 0xFFFFFFFF);
}

// ============================================================================
// Receive Frame
//   - Returns slice to RX buffer (static lifetime)
//   - Caller must process immediately or copy out before next poll_rx()
// ============================================================================
pub unsafe fn poll_rx() -> Option<&'static [u8]> {
    let head = RX_HEAD;
    let desc = &mut RX_DESC[head];

    // Ownership bit set → HW still owns → no packet
    if (desc.addr & RX_OWNERSHIP) != 0 {
        return None;
    }

    // Status lower 13 bits = frame length
    let len = (desc.status & 0x1FFF) as usize;
    if len == 0 || len > RX_BUF_SIZE {
        // Something weird, re-arm descriptor and skip
        let mut addr = (&RX_BUFFERS[head][0] as *const u8 as u32) & !0x3;
        addr |= RX_OWNERSHIP;
        if (desc.addr & RX_WRAP) != 0 {
            addr |= RX_WRAP;
        }
        desc.addr = addr;
        desc.status = 0;

        RX_HEAD = (head + 1) % RX_DESC_COUNT;
        return None;
    }

    let buf = &RX_BUFFERS[head][0..len];

    // Re-arm this descriptor
    let mut new_addr = (&RX_BUFFERS[head][0] as *const u8 as u32) & !0x3;
    new_addr |= RX_OWNERSHIP;
    if (desc.addr & RX_WRAP) != 0 {
        new_addr |= RX_WRAP;
    }

    desc.addr = new_addr;
    desc.status = 0;

    RX_HEAD = (head + 1) % RX_DESC_COUNT;

    Some(buf)
}
