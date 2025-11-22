// rp1_ethernet.rs - RP1 Ethernet controller driver
//
// Raspberry Pi 5 Gigabit Ethernet (Cadence GEM/MACB) Controller
// Physical base address (as seen from ARM cores via PCIe window): 0x1FC0_1000_00
// This maps to the peripheral address space within the RP1 I/O controller chip.
// In Linux device tree: macb 1f00100000.ethernet (PCIe bus address 0xC040_1000_00).
//
// Important notes for bare-metal programming:
// - Memory mapping has changed significantly from previous models (e.g., Pi 4's 0x3F000000)
// - On Raspberry Pi 5, all addresses seen by ARM cores are bus master addresses
// - RP1 chip is accessed via PCIe
//
// RP1→DRAM Bus Master Access Architecture:
// ┌─────────────────────────────────────────────────────────────────┐
// │ BCM2712 (ARM Cortex-A76) Physical Memory Map                   │
// ├─────────────────────────────────────────────────────────────────┤
// │ 0x0000_0000_0000 - 0x0000_7FFF_FFFF : MMIO/PCIe/RP1/VPU (2GiB) │
// │ 0x0000_8000_0000 - 0x0000_FFFF_FFFF : DRAM (starts here!)      │
// └─────────────────────────────────────────────────────────────────┘
//
// RP1 System Address Space (40-bit, for bus masters like DMA):
// ┌─────────────────────────────────────────────────────────────────┐
// │ 0x00.0000.0000 - 0x7F.FFFF.FFFF : PCIe Outbound direct (512GiB)│
// │ 0x80.0000.0000 - 0x8F.FFFF.FFFF : PCIe Outbound ATU (256GiB)   │
// │ 0xC0.2000.0000 - 0xC0.203F.FFFF : Shared SRAM (64kiB)          │
// │ 0xC0.4000.0000 - ...            : Peripherals (APB/AHB/AXI)    │
// └─────────────────────────────────────────────────────────────────┘
//
// When RP1 DMA masters access BCM2712 DRAM:
// - Use physical address >= 0x0000_8000_0000 (BCM2712 DRAM start)
// - RP1 sends this as PCIe Outbound transaction
// - BCM2712 Root Complex maps it to actual DRAM

use core::ptr;
use core::sync::atomic::Ordering;

use crate::drivers::pcie::RP1_BAR0_CPU_BASE;

// RP1 Ethernet is accessed via PCIe at BAR1 + offset
// Ethernet/MACB offset in RP1: 0x0010_0000 (1MB from RP1 base)
// Linux device tree shows: macb@100000 under RP1
const RP1_ETH_OFFSET: usize = 0x0010_0000;

// RP1 Clock control registers (CLKGEN)
const RP1_CLKGEN_BASE_OFFSET: usize = 0x0001_8000;
const CLKGEN_CLK_ETH_CTRL: usize = 0x3c;

// ETH_CFG register offsets (base: ETH base + 0x4000)
const ETH_CFG_OFFSET: usize = 0x4000;
const ETH_CFG_CONTROL: usize = 0x00;
const ETH_CFG_STATUS: usize = 0x04;
const ETH_CFG_TSU_TIMER_CNT0: usize = 0x08;
const ETH_CFG_TSU_TIMER_CNT1: usize = 0x0c;
const ETH_CFG_TSU_TIMER_CNT2: usize = 0x10;
const ETH_CFG_CLKGEN: usize = 0x14;
const ETH_CFG_CLK2FC: usize = 0x18;
const ETH_CFG_INTR: usize = 0x1c;
const ETH_CFG_INTE: usize = 0x20;
const ETH_CFG_INTF: usize = 0x24;
const ETH_CFG_INTS: usize = 0x28;

// ETH_CFG STATUS register bits
const ETH_CFG_STATUS_AWLEN_ILLEGAL: u32 = 1 << 5;
const ETH_CFG_STATUS_ARLEN_ILLEGAL: u32 = 1 << 4;
const ETH_CFG_STATUS_RGMII_DUPLEX: u32 = 1 << 3;
const ETH_CFG_STATUS_RGMII_SPEED_MASK: u32 = 0b11 << 1;
const ETH_CFG_STATUS_RGMII_SPEED_10M: u32 = 0 << 1;
const ETH_CFG_STATUS_RGMII_SPEED_100M: u32 = 1 << 1;
const ETH_CFG_STATUS_RGMII_SPEED_1G: u32 = 2 << 1;
const ETH_CFG_STATUS_RGMII_LINK_STATUS: u32 = 1 << 0;

// ETH_CFG CLKGEN register bits
const ETH_CFG_CLKGEN_TXCLKDELEN: u32 = 1 << 9;
const ETH_CFG_CLKGEN_DC50: u32 = 1 << 8;
const ETH_CFG_CLKGEN_ENABLE: u32 = 1 << 7;
const ETH_CFG_CLKGEN_KILL: u32 = 1 << 6;
const ETH_CFG_CLKGEN_SPEED_FROM_MAC_MASK: u32 = 0b11 << 4;
const ETH_CFG_CLKGEN_SPEED_OVERRIDE_EN: u32 = 1 << 3;
const ETH_CFG_CLKGEN_SPEED_OVERRIDE_MASK: u32 = 0b11 << 0;
const ETH_CFG_CLKGEN_SPEED_10M: u32 = 0;
const ETH_CFG_CLKGEN_SPEED_100M: u32 = 1;
const ETH_CFG_CLKGEN_SPEED_1000M: u32 = 2;

// ETH_CFG Interrupt bits (INTR, INTE, INTF, INTS)
const ETH_CFG_IRQ_TSU_TIMER_CMP_VAL: u32 = 1 << 12;
const ETH_CFG_IRQ_IEEE1588_SOF_RX: u32 = 1 << 11;
const ETH_CFG_IRQ_IEEE1588_SYNC_FRAME_RX: u32 = 1 << 10;
const ETH_CFG_IRQ_IEEE1588_DELAY_REQ_RX: u32 = 1 << 9;
const ETH_CFG_IRQ_IEEE1588_PDELAY_REQ_RX: u32 = 1 << 8;
const ETH_CFG_IRQ_IEEE1588_PDELAY_RESP_RX: u32 = 1 << 7;
const ETH_CFG_IRQ_IEEE1588_SOF_TX: u32 = 1 << 6;
const ETH_CFG_IRQ_IEEE1588_SYNC_FRAME_TX: u32 = 1 << 5;
const ETH_CFG_IRQ_IEEE1588_DELAY_REQ_TX: u32 = 1 << 4;
const ETH_CFG_IRQ_IEEE1588_PDELAY_REQ_TX: u32 = 1 << 3;
const ETH_CFG_IRQ_IEEE1588_PDELAY_RESP_TX: u32 = 1 << 2;
const ETH_CFG_IRQ_WOL: u32 = 1 << 1;
const ETH_CFG_IRQ_ETHERNET: u32 = 1 << 0;

// MACB (Cadence GEM) register offsets
const GEM_NWCTRL: usize = 0x000;
const GEM_NWCFG: usize = 0x004;
const GEM_NWSR: usize = 0x008;
const GEM_DMACFG: usize = 0x010;
const GEM_TXSR: usize = 0x014;
const GEM_RXQBASE: usize = 0x018;
const GEM_TXQBASE: usize = 0x01C;
const GEM_RXSR: usize = 0x020;
const GEM_SPADDR1LO: usize = 0x088;
const GEM_SPADDR1HI: usize = 0x08C;

// GEM NWCFG2 (GMAC specific) - RGMII / timing control (offset may be SoC dependent)
const GEM_NWCFG2: usize = 0x08C;
const NWCFG2_RGMII_EN: u32 = 1 << 0;
const NWCFG2_INBAND_DISABLE: u32 = 1 << 1;
const NWCFG2_RX_CLK_EN: u32 = 1 << 2;
const NWCFG2_TX_CLK_EN: u32 = 1 << 3;

// Network control register bits
const GEM_ENABLE_TX: u32 = 1 << 9;
const GEM_ENABLE_RX: u32 = 1 << 2;
const GEM_MPE: u32 = 1 << 4; // Management port enable
// RX interrupt enable bit (per RP1 MACB spec)
const GEM_RX_IRQ_EN: u32 = 1 << 20;

// Network config register bits
const GEM_FD: u32 = 1 << 0; // Full duplex
const GEM_SPD: u32 = 1 << 10; // Speed (1=100Mbps)
const GEM_GIGE: u32 = 1 << 11; // 1 = 1Gbps/GIGE bit
const GEM_RXCSUM_EN: u32 = 1 << 24; // RX checksum offload

// DMA config
const GEM_DISC_WHEN_NO_AHB: u32 = 1 << 10;
const GEM_FBLDO_INCR4: u32 = 4 << 16;

// Descriptor layout for Cadence GEM/MACB
// Use 32-bit addr/status fields per hardware spec.
#[repr(C, align(8))]
#[derive(Copy, Clone)]
pub struct RxDescriptor {
    pub addr: u32,
    pub status: u32,
}

#[repr(C, align(8))]
#[derive(Copy, Clone)]
pub struct TxDescriptor {
    pub addr: u32,
    pub status: u32,
}

impl RxDescriptor {
    // addr low bits: bit0 = address valid, bit1 = wrap (last descriptor)
    pub const ADDR_VALID: u32 = 1 << 0;
    pub const ADDR_WRAP: u32 = 1 << 1;
    // status bit31 = OWN (1 = hardware owns descriptor)
    pub const STATUS_OWN: u32 = 1 << 31;
}

impl TxDescriptor {
    pub const STATUS_USED: u32 = 1 << 31; // 1 = hardware done
    pub const STATUS_WRAP: u32 = 1 << 30;
    pub const STATUS_LAST: u32 = 1 << 15;
}

// Aligned buffer wrapper so we can place large buffers with specific alignment.
#[repr(align(2048))]
#[derive(Copy, Clone)]
struct Buffer2048([u8; 2048]);

// Place descriptors and buffers in a named section so the linker can
// position them appropriately for DMA.
#[link_section = ".dram"]
static mut RX_DESCRIPTORS: [RxDescriptor; 4] = [RxDescriptor { addr: 0, status: 0 }; 4];

#[link_section = ".dram"]
static mut TX_DESCRIPTORS: [TxDescriptor; 4] = [TxDescriptor { addr: 0, status: 0 }; 4];

#[link_section = ".dram"]
static mut RX_BUFFERS: [Buffer2048; 4] = [Buffer2048([0u8; 2048]); 4];

#[link_section = ".dram"]
static mut TX_BUFFERS: [Buffer2048; 4] = [Buffer2048([0u8; 2048]); 4];

pub struct Rp1Ethernet {
    base: usize,
    eth_cfg_base: usize,
    rx_descriptors: &'static mut [RxDescriptor; 4],
    tx_descriptors: &'static mut [TxDescriptor; 4],
    rx_buffers: &'static mut [Buffer2048; 4],
    tx_buffers: &'static mut [Buffer2048; 4],
    rx_index: usize,
    tx_index: usize,
}

impl Rp1Ethernet {
    pub fn new(rp1_base: usize) -> Self {
        let eth_base = rp1_base + RP1_ETH_OFFSET;
        Rp1Ethernet {
            base: eth_base,
            eth_cfg_base: eth_base + ETH_CFG_OFFSET,
            rx_descriptors: unsafe { &mut RX_DESCRIPTORS },
            tx_descriptors: unsafe { &mut TX_DESCRIPTORS },
            rx_buffers: unsafe { &mut RX_BUFFERS },
            tx_buffers: unsafe { &mut TX_BUFFERS },
            rx_index: 0,
            tx_index: 0,
        }
    }

    unsafe fn phy_reset(&self) {
        crate::print_str("PHY: asserting reset on GPIO4...\n");

        const GPIO_BASE: usize = 0x1F0000_0000usize + 0xD0000; // 0x1F000D0000
        const GPIO_FSEL0: usize = GPIO_BASE + 0x00;
        const GPIO_SET0: usize = GPIO_BASE + 0x1C;
        const GPIO_CLR0: usize = GPIO_BASE + 0x28;

        // Set GPIO4 as output (function 1 = output)
        let fsel_ptr = GPIO_FSEL0 as *mut u32;
        let mut fsel = core::ptr::read_volatile(fsel_ptr);
        fsel = (fsel & !(7 << 12)) | (1 << 12);
        core::ptr::write_volatile(fsel_ptr, fsel);

        // GPIO4 = 0 (assert reset)
        core::ptr::write_volatile(GPIO_CLR0 as *mut u32, 1 << 4);

        // hold reset for ~150ms (give PHY ample time)
        crate::drivers::timer::TIMER.delay_ms(150);

        // GPIO4 = 1 (release reset)
        crate::print_str("PHY: deassert reset...\n");
        core::ptr::write_volatile(GPIO_SET0 as *mut u32, 1 << 4);

        // wait PHY stabilization
        crate::drivers::timer::TIMER.delay_ms(150);

        crate::print_str("PHY: reset complete\n");
    }

    /// Configure MDIO pins (GPIO2 = MDC, GPIO3 = MDIO) to ALT4 (function 3)
    unsafe fn setup_mdio_pins(&self) {
        crate::print_str("RP1: configuring MDIO pins (GPIO2/3)...\n");

        const GPIO_BASE: usize = 0x1F0000_0000usize + 0xD0000; // 0x1F000D0000
        const GPIO_FSEL0: usize = GPIO_BASE + 0x00;

        let fsel_ptr = GPIO_FSEL0 as *mut u32;
        let mut fsel = core::ptr::read_volatile(fsel_ptr);

        // GPIO2 = ALT4 (function 3)
        fsel &= !(7 << (2 * 3));
        fsel |= 3 << (2 * 3);

        // GPIO3 = ALT4 (function 3)
        fsel &= !(7 << (3 * 3));
        fsel |= 3 << (3 * 3);

        core::ptr::write_volatile(fsel_ptr, fsel);

        crate::print_str("RP1: MDIO pins configured.\n");
    }

    /// Compute MDIO register base for MACB/GEM
    /// MACB main registers: BAR0 + 0x10000
    /// MDIO registers: MACB base + 0x200
    fn mdio_base(&self) -> Option<usize> {
        let bar0 = RP1_BAR0_CPU_BASE.load(Ordering::SeqCst) as usize;
        if bar0 == 0 {
            return None;
        }
        let macb_base = bar0 + 0x10000;
        Some(macb_base + 0x200)
    }

    unsafe fn mdio_read_reg(&self, phy: u8, reg: u8) -> Result<u16, &'static str> {
        const NPHY: usize = 0x14;
        const NDATA: usize = 0x18;

        let mdio = match self.mdio_base() {
            Some(v) => v,
            None => return Err("MDIO: BAR0 not programmed"),
        };

        // CMD: bit15 = start, bits14:7 = phy, bits6:2 = reg, bits1:0 = op (2 = read)
        let cmd: u32 = (1 << 15) | ((phy as u32) << 7) | ((reg as u32) << 2) | 2;

        core::ptr::write_volatile((mdio + NPHY) as *mut u32, cmd);

        // wait for busy clear (bit0 == 0)
        for _ in 0..1000 {
            let v = core::ptr::read_volatile((mdio + NPHY) as *const u32);
            if (v & 1) == 0 {
                break;
            }
            crate::drivers::timer::TIMER.delay_us(10);
        }

        let v = core::ptr::read_volatile((mdio + NPHY) as *const u32);
        if (v & 1) != 0 {
            return Err("MDIO read timeout");
        }

        let data = core::ptr::read_volatile((mdio + NDATA) as *const u32) as u16;
        Ok(data)
    }

    unsafe fn mdio_write_reg(&self, phy: u8, reg: u8, val: u16) -> Result<(), &'static str> {
        const NPHY: usize = 0x14;
        const NDATA: usize = 0x18;

        let mdio = match self.mdio_base() {
            Some(v) => v,
            None => return Err("MDIO: BAR0 not programmed"),
        };

        // Prepare write by writing data with DONE bit set
        core::ptr::write_volatile((mdio + NDATA) as *mut u32, 0x8000_0000u32 | (val as u32));

        // CMD: write operation (op = 0)
        let cmd: u32 = ((phy as u32) << 7) | ((reg as u32) << 2) | 0;
        core::ptr::write_volatile((mdio + NPHY) as *mut u32, cmd);

        // wait for busy clear
        for _ in 0..1000 {
            let v = core::ptr::read_volatile((mdio + NPHY) as *const u32);
            if (v & 1) == 0 {
                return Ok(());
            }
            crate::drivers::timer::TIMER.delay_us(10);
        }

        Err("MDIO write timeout")
    }

    /// PHY のオートネゴ完了とリンクUPを待つヘルパ
    ///
    /// - MII_ANAR (0x04) / MII_GBCR (0x09) に広告能力を書き込み
    /// - MII_BMCR (0x00) の ANENABLE/RESTARTAN をセット
    /// - MII_BMSR (0x01) の LINK_STATUS / AUTONEG_COMPLETE をポーリング
    ///
    unsafe fn phy_autoneg_and_wait(&self, phy: u8) -> Result<(), &'static str> {
        // MII レジスタ定数
        const MII_BMCR: u8 = 0x00;
        const MII_BMSR: u8 = 0x01;
        const MII_ANAR: u8 = 0x04;
        const MII_GBCR: u8 = 0x09;

        // BMCR bits
        const BMCR_ANENABLE: u16 = 1 << 12;
        const BMCR_RESTARTAN: u16 = 1 << 9;

        // BMSR bits
        const BMSR_LINK_STATUS: u16 = 1 << 2;
        const BMSR_AUTONEG_COMPLETE: u16 = 1 << 5;

        crate::print_str("RP1: PHY auto-negotiation start\n");

        // 1) advertise 10/100/1000 (ほぼ Linux と同じセット)
        //   ANAR: 10/100 full/half + pause
        //   0x01E1 = 10H/10F/100H/100F + symmetric/asymmetric pause
        let _ = self.mdio_write_reg(phy, MII_ANAR, 0x01E1);

        //   GBCR: 1000base-T full/half
        let _ = self.mdio_write_reg(phy, MII_GBCR, 0x0300);

        // 2) BMCR 読み出し → ANENABLE/RESTARTAN をセット
        let mut bmcr = self.mdio_read_reg(phy, MII_BMCR).unwrap_or(0);
        bmcr |= BMCR_ANENABLE | BMCR_RESTARTAN;
        let _ = self.mdio_write_reg(phy, MII_BMCR, bmcr);

        crate::print_str("RP1: waiting for PHY autoneg + link\n");

        // 3) BMSR をポーリングして LINK + AUTONEG_COMPLETE を待つ
        //    BMSR は latched なので毎回2回読む
        let mut ok = false;
        for _ in 0..100 {
            crate::drivers::timer::TIMER.delay_ms(100);

            let _ = self.mdio_read_reg(phy, MII_BMSR); // latch clear
            if let Ok(bmsr) = self.mdio_read_reg(phy, MII_BMSR) {
                if (bmsr & BMSR_LINK_STATUS) != 0 && (bmsr & BMSR_AUTONEG_COMPLETE) != 0 {
                    ok = true;
                    crate::print_str("RP1: PHY autoneg complete, link up (BMSR=0x");
                    crate::print_hex(bmsr as usize);
                    crate::print_str(")\n");
                    break;
                }
            }
        }

        if !ok {
            crate::print_str("RP1: PHY autoneg timeout (link did not come up)\n");
            return Err("PHY autoneg timeout");
        }

        Ok(())
    }

    /// PHYの結果に合わせて GEM_NWCFG と ETH_CFG CLKGEN / NWCFG2 を揃える
    unsafe fn configure_mac_from_phy(&self) {
        // ETH_CFG_STATUS から RGMII の状態を読む
        let status = self.read_eth_cfg(ETH_CFG_STATUS);

        let link_up  = (status & ETH_CFG_STATUS_RGMII_LINK_STATUS) != 0;
        let speed    = (status & ETH_CFG_STATUS_RGMII_SPEED_MASK) >> 1; // 0=10,1=100,2=1000
        let duplex   = (status & ETH_CFG_STATUS_RGMII_DUPLEX) != 0;

        crate::print_str("RP1: ETH_CFG_STATUS=0x");
        crate::print_hex(status as usize);
        crate::print_str(" (via PHY)\n");

        crate::print_str("RP1: link=");
        crate::print_str(if link_up { "UP" } else { "DOWN" });
        crate::print_str(" speed=");
        match speed {
            0 => crate::print_str("10M"),
            1 => crate::print_str("100M"),
            2 => crate::print_str("1G"),
            _ => crate::print_str("?"),
        }
        crate::print_str(" duplex=");
        crate::print_str(if duplex { "FULL" } else { "HALF" });
        crate::print_str("\n");

        // NWCFG の組み立て
        let mut nwcfg = GEM_RXCSUM_EN;
        if duplex {
            nwcfg |= GEM_FD;
        }
        match speed {
            0 => { /* 10M: SPD=0, GIGE=0 */ }
            1 => { nwcfg |= GEM_SPD; }      // 100M
            2 => { nwcfg |= GEM_GIGE; }     // 1000M
            _ => {}
        }
        self.write_reg(GEM_NWCFG, nwcfg);
        crate::print_str("RP1: GEM_NWCFG updated from PHY\n");

        // ETH_CFG CLKGEN を PHY の speed にあわせて override
        crate::print_str("RP1: Setting ETH_CFG CLKGEN override from PHY speed\n");
        self.configure_eth_clkgen(speed as u32, true);

        // NWCFG2: RGMII + inband disable + RX/TX clk enable
        let nwcfg2 =
            NWCFG2_RGMII_EN | NWCFG2_INBAND_DISABLE | NWCFG2_RX_CLK_EN | NWCFG2_TX_CLK_EN;
        self.write_reg(GEM_NWCFG2, nwcfg2);
        crate::print_str("RP1: NWCFG2 configured for RGMII\n");
    }

    unsafe fn enable_eth_clock(&self, rp1_base: usize) {
        crate::print_str("RP1: enabling ETH clock...\n");

        let clk_ctrl_addr =
            (rp1_base + RP1_CLKGEN_BASE_OFFSET + CLKGEN_CLK_ETH_CTRL) as *mut u32;

        let before = core::ptr::read_volatile(clk_ctrl_addr);
        crate::print_str("CLK_ETH_CTRL (before)=0x");
        crate::print_hex(before as usize);
        crate::print_str("\n");

        // keep firmware upper bits, just enable MAC related bits
        let mut after = before;

        const ENABLE: u32 = 1 << 11; // main enable
        const TXCLKDELEN: u32 = 1 << 9;
        const DC50: u32 = 1 << 8;

        after |= ENABLE;
        after |= TXCLKDELEN;
        after |= DC50;

        core::ptr::write_volatile(clk_ctrl_addr, after);

        let verify = core::ptr::read_volatile(clk_ctrl_addr);
        crate::print_str("CLK_ETH_CTRL (after)=0x");
        crate::print_hex(verify as usize);
        crate::print_str("\n");
    }

    /// Convert CPU virtual/physical address to RP1 DMA address.
    /// RP1 expects the BCM2712 physical address as-is.
    fn cpu_to_dma_addr(cpu_addr: usize) -> u64 {
        cpu_addr as u64
    }

    fn read_reg(&self, offset: usize) -> u32 {
        unsafe { ptr::read_volatile((self.base + offset) as *const u32) }
    }

    fn write_reg(&self, offset: usize, value: u32) {
        unsafe { ptr::write_volatile((self.base + offset) as *mut u32, value) }
    }

    // ETH_CFG register access methods
    fn read_eth_cfg(&self, offset: usize) -> u32 {
        unsafe { ptr::read_volatile((self.eth_cfg_base + offset) as *const u32) }
    }

    fn write_eth_cfg(&self, offset: usize, value: u32) {
        unsafe { ptr::write_volatile((self.eth_cfg_base + offset) as *mut u32, value) }
    }

    /// Get TSU (Time Stamp Unit) timer count value (94-bit counter)
    pub fn get_tsu_timer(&self) -> u128 {
        let cnt0 = self.read_eth_cfg(ETH_CFG_TSU_TIMER_CNT0);
        let cnt1 = self.read_eth_cfg(ETH_CFG_TSU_TIMER_CNT1);
        let cnt2 = self.read_eth_cfg(ETH_CFG_TSU_TIMER_CNT2) & 0x3FFF_FFFF; // only 30 bits

        ((cnt2 as u128) << 64) | ((cnt1 as u128) << 32) | (cnt0 as u128)
    }

    /// Get RGMII link status
    pub fn get_link_status(&self) -> bool {
        let status = self.read_eth_cfg(ETH_CFG_STATUS);
        (status & ETH_CFG_STATUS_RGMII_LINK_STATUS) != 0
    }

    /// Get RGMII speed (0=10Mb, 1=100Mb, 2=1Gb)
    pub fn get_link_speed(&self) -> u8 {
        let status = self.read_eth_cfg(ETH_CFG_STATUS);
        ((status & ETH_CFG_STATUS_RGMII_SPEED_MASK) >> 1) as u8
    }

    /// Get RGMII duplex mode
    pub fn get_duplex_mode(&self) -> bool {
        let status = self.read_eth_cfg(ETH_CFG_STATUS);
        (status & ETH_CFG_STATUS_RGMII_DUPLEX) != 0
    }

    /// Configure ETH_CFG clock generator
    pub fn configure_eth_clkgen(&self, speed: u32, enable: bool) {
        let mut clkgen = self.read_eth_cfg(ETH_CFG_CLKGEN);

        // Clear speed override bits, then set speed in the proper bit positions.
        // ETH_CFG_CLKGEN_SPEED_OVERRIDE_MASK defines the bits occupied by the
        // speed field (bits 1:0). We shift the provided `speed` value into
        // that mask before OR'ing.
        clkgen &= !ETH_CFG_CLKGEN_SPEED_OVERRIDE_MASK;
        clkgen |= (speed << ETH_CFG_CLKGEN_SPEED_OVERRIDE_MASK.trailing_zeros()) & ETH_CFG_CLKGEN_SPEED_OVERRIDE_MASK;

        if enable {
            // Enable speed-override mode so MAC/PHY do not fight over clocking.
            clkgen |= ETH_CFG_CLKGEN_SPEED_OVERRIDE_EN;
            // Ensure TX clock delay is enabled (required for many RGMII PHYs).
            clkgen |= ETH_CFG_CLKGEN_TXCLKDELEN;
            // Keep DC50 if previously set by firmware; leave it untouched here.
            // Also set the main enable bit.
            clkgen |= ETH_CFG_CLKGEN_ENABLE;
        } else {
            // Disable override and main enable if requested.
            clkgen &= !ETH_CFG_CLKGEN_SPEED_OVERRIDE_EN;
            clkgen &= !ETH_CFG_CLKGEN_ENABLE;
        }

        crate::print_str("RP1: ETH_CFG CLKGEN (writing)=0x");
        crate::print_hex(clkgen as usize);
        crate::print_str("\n");
        self.write_eth_cfg(ETH_CFG_CLKGEN, clkgen);
        let verify = self.read_eth_cfg(ETH_CFG_CLKGEN);
        crate::print_str("RP1: ETH_CFG CLKGEN (after)=0x");
        crate::print_hex(verify as usize);
        crate::print_str("\n");
    }

    /// Enable/disable ETH_CFG interrupts
    pub fn set_eth_cfg_interrupts(&self, mask: u32, enable: bool) {
        let mut inte = self.read_eth_cfg(ETH_CFG_INTE);

        if enable {
            inte |= mask;
        } else {
            inte &= !mask;
        }

        self.write_eth_cfg(ETH_CFG_INTE, inte);
    }

    /// Clear ETH_CFG interrupts (write-to-clear)
    pub fn clear_eth_cfg_interrupts(&self, mask: u32) {
        self.write_eth_cfg(ETH_CFG_INTR, mask);
    }

    /// Get ETH_CFG interrupt status
    pub fn get_eth_cfg_interrupt_status(&self) -> u32 {
        self.read_eth_cfg(ETH_CFG_INTS)
    }

    pub fn init(&mut self, mac: [u8; 6]) -> Result<(), &'static str> {
        crate::print_str("RP1 Ethernet: Initializing...\n");
        crate::print_str("RP1 Ethernet: Base address: 0x");
        crate::print_hex(self.base);
        crate::print_str("\n");
        crate::print_str("RP1 Ethernet: ETH_CFG base address: 0x");
        crate::print_hex(self.eth_cfg_base);
        crate::print_str("\n");

        // Read ETH_CFG STATUS register
        let eth_cfg_status = self.read_eth_cfg(ETH_CFG_STATUS);
        crate::print_str("RP1 Ethernet: ETH_CFG STATUS=0x");
        crate::print_hex(eth_cfg_status as usize);
        crate::print_str("\n");

        let link_up = (eth_cfg_status & ETH_CFG_STATUS_RGMII_LINK_STATUS) != 0;
        let speed = (eth_cfg_status & ETH_CFG_STATUS_RGMII_SPEED_MASK) >> 1;
        let duplex = (eth_cfg_status & ETH_CFG_STATUS_RGMII_DUPLEX) != 0;

        crate::print_str("RP1 Ethernet: Link=");
        if link_up {
            crate::print_str("UP");
        } else {
            crate::print_str("DOWN");
        }
        crate::print_str(" Speed=");
        match speed {
            0 => crate::print_str("10M"),
            1 => crate::print_str("100M"),
            2 => crate::print_str("1G"),
            _ => crate::print_str("?"),
        }
        crate::print_str(" Duplex=");
        if duplex {
            crate::print_str("FULL");
        } else {
            crate::print_str("HALF");
        }
        crate::print_str("\n");

        // PHY reset: assert LOW then release HIGH (GPIO4)
        unsafe { self.phy_reset(); }
        // RP1 GBE core reset (BAR1 + 0x0080): assert before requesting clocks,
        // request clocks/power from firmware, then release reset so MAC comes up
        let rp1_base = self.base - RP1_ETH_OFFSET;
        let gbe_reset_addr = (rp1_base + 0x0080) as *mut u32;

        crate::print_str("RP1: asserting GBE core reset (BAR1+0x0080)\n");
        unsafe { core::ptr::write_volatile(gbe_reset_addr, 1); }
        crate::drivers::timer::TIMER.delay_us(10);

        crate::print_str("RP1: enabling Ethernet domain via RP1 mailbox\n");
        unsafe {
            if !crate::drivers::rp1_boot::rp1_fw_init_ethernet() {
                crate::print_str("RP1: rp1_enable_ethernet() reported failure\n");
            }
        }

        // Ensure RP1 firmware powers/clocks Ethernet via mailbox property tags
        crate::print_str("RP1: requesting Ethernet power/clock via mailbox\n");
        match crate::drivers::mailbox::set_power_state(
            crate::drivers::mailbox::DeviceId::Ethernet,
            true,
            true,
        ) {
            Ok(v) => {
                crate::print_str("MAILBOX: set_power_state OK val=0x");
                crate::print_hex(v as usize);
                crate::print_str("\n");
            }
            Err(_) => {
                crate::print_str("MAILBOX: set_power_state FAILED\n");
            }
        }
        match crate::drivers::mailbox::set_clock_state(
            crate::drivers::mailbox::ClockId::Ethernet,
            true,
        ) {
            Ok(v) => {
                crate::print_str("MAILBOX: set_clock_state OK val=0x");
                crate::print_hex(v as usize);
                crate::print_str("\n");
            }
            Err(_) => {
                crate::print_str("MAILBOX: set_clock_state FAILED\n");
            }
        }

        // Release GBE core reset after clocks are requested
        crate::print_str("RP1: releasing GBE core reset (BAR1+0x0080)\n");
        crate::drivers::timer::TIMER.delay_us(10);
        unsafe { core::ptr::write_volatile(gbe_reset_addr, 0); }
        crate::drivers::timer::TIMER.delay_us(10);

        // Enable ETH clock generator (preserve firmware upper bits; OR in enable flags)
        let rp1_base = self.base - RP1_ETH_OFFSET;
        unsafe { self.enable_eth_clock(rp1_base); }
        // Force the ETH_CFG clock block to output a stable 125MHz RGMII clock.
        self.configure_eth_clkgen(ETH_CFG_CLKGEN_SPEED_1000M, true);

        // After enabling clocks, try to configure PHY via MDIO: enable auto-negotiation
        crate::print_str("RP1: attempting MDIO PHY probe for auto-negotiation\n");
        unsafe {
            // Ensure MDIO pins are in MDIO (ALT4) mode
            self.setup_mdio_pins();

            // 1) PHY アドレス探索
            let mut found: Option<u8> = None;
            for phy in 0..32u8 {
                if let Ok(val) = self.mdio_read_reg(phy, 1) {
                    if val != 0xFFFF && val != 0 {
                        found = Some(phy);
                        break;
                    }
                }
            }

            if let Some(phy) = found {
                crate::print_str("RP1: PHY found at addr ");
                crate::print_dec(phy as usize);
                crate::print_str("\n");

                // デバッグ用に ID を読む
                if let Ok(id1) = self.mdio_read_reg(phy, 2) {
                    crate::print_str("RP1: PHY ID1=0x");
                    crate::print_hex(id1 as usize);
                    crate::print_str("\n");
                }
                if let Ok(id2) = self.mdio_read_reg(phy, 3) {
                    crate::print_str("RP1: PHY ID2=0x");
                    crate::print_hex(id2 as usize);
                    crate::print_str("\n");
                }

                // 2) オートネゴ + リンクUP待ち
                if let Err(e) = self.phy_autoneg_and_wait(phy) {
                    crate::print_str("RP1: PHY autoneg failed: ");
                    crate::print_str(e);
                    crate::print_str("\n");
                }

                // 3) PHY の結果に合わせて MAC 側 (NWCFG / CLKGEN / NWCFG2) 設定
                self.configure_mac_from_phy();
            } else {
                crate::print_str("RP1: No PHY responded to MDIO probe\n");
            }
        }

        // Also try checking RP1 reset/power control
        // RP1 SYS registers for reset control
        let sys_reset_addr = (rp1_base + 0x000c) as *mut u32; // RESET register
        let sys_reset = unsafe { core::ptr::read_volatile(sys_reset_addr) };
        crate::print_str("RP1: SYS RESET: 0x");
        crate::print_hex(sys_reset as usize);
        crate::print_str("\n");

        // Wait a bit for clock to stabilize
        for _ in 0..1000 {
            unsafe { core::arch::asm!("nop") };
        }

        // Now try to read chip ID
        crate::print_str("RP1: Reading chip ID at 0x");
        crate::print_hex(rp1_base);
        crate::print_str("\n");

        let chip_id = unsafe { core::ptr::read_volatile(rp1_base as *const u32) };
        crate::print_str("RP1: Chip ID: 0x");
        crate::print_hex(chip_id as usize);
        crate::print_str("\n");

        // Debug: Try reading various offsets to find the correct registers
        crate::print_str("RP1 Ethernet: Probing register space...\n");

        let test_bases = [
            (0x000000, "RP1_SYS"),
            (0x0d0000, "GPIO (known working)"),
            (0x100000, "Expected MACB"),
            (0x1c0000, "Alternative 1"),
            (0x004000, "Small offset"),
        ];

        for (offset, name) in test_bases.iter() {
            let addr = rp1_base + offset;
            let val = unsafe { core::ptr::read_volatile(addr as *const u32) };
            crate::print_str("  ");
            crate::print_str(name);
            crate::print_str(" @0x");
            crate::print_hex(*offset);
            crate::print_str(": 0x");
            crate::print_hex(val as usize);
            crate::print_str("\n");
        }

        // Try accessing via PCIe BAR0 (config space for RP1 peripherals)
        crate::print_str("  Trying PCIe BAR0 region...\n");
        let bar0_bases = [
            (0x1f00410000usize, "BAR0 base"),
            (0x1f00400000usize, "BAR2 base"),
        ];

        for (addr, name) in bar0_bases.iter() {
            let val = unsafe { core::ptr::read_volatile(*addr as *const u32) };
            crate::print_str("    ");
            crate::print_str(name);
            crate::print_str(" @0x");
            crate::print_hex(*addr);
            crate::print_str(": 0x");
            crate::print_hex(val as usize);
            crate::print_str("\n");
        }

        // Also try reading from self.base which we calculated
        let reg_0x000 = self.read_reg(0x000);
        let reg_0x004 = self.read_reg(0x004);
        let reg_0x008 = self.read_reg(0x008);

        crate::print_str("  Current base (0x");
        crate::print_hex(self.base);
        crate::print_str(") @0x000: 0x");
        crate::print_hex(reg_0x000 as usize);
        crate::print_str(" @0x004: 0x");
        crate::print_hex(reg_0x004 as usize);
        crate::print_str(" @0x008: 0x");
        crate::print_hex(reg_0x008 as usize);
        crate::print_str("\n");

        // Disable TX and RX before configuring
        self.write_reg(GEM_NWCTRL, 0);

        // Setup descriptors
        for i in 0..4 {
            // RX descriptors
            let rx_buf_ptr = self.rx_buffers[i].0.as_ptr();
            let rx_cpu_addr = rx_buf_ptr as usize;
            let rx_dma_addr = Self::cpu_to_dma_addr(rx_cpu_addr);

            crate::print_str("[ETH] RX desc[");
            crate::print_dec(i);
            crate::print_str("] CPU addr=0x");
            crate::print_hex(rx_cpu_addr);
            crate::print_str(" -> DMA addr=0x");
            crate::print_hex(rx_dma_addr as usize);
            crate::print_str("\n");

            let mut desc_addr = (rx_dma_addr as u32) | RxDescriptor::ADDR_VALID;
            if i == 3 {
                desc_addr |= RxDescriptor::ADDR_WRAP;
            }
            self.rx_descriptors[i].addr = desc_addr;
            self.rx_descriptors[i].status = RxDescriptor::STATUS_OWN;

            // TX descriptors
            let tx_buf_ptr = self.tx_buffers[i].0.as_ptr();
            let tx_cpu_addr = tx_buf_ptr as usize;
            let tx_dma_addr = Self::cpu_to_dma_addr(tx_cpu_addr);

            crate::print_str("[ETH] TX desc[");
            crate::print_dec(i);
            crate::print_str("] CPU addr=0x");
            crate::print_hex(tx_cpu_addr);
            crate::print_str(" -> DMA addr=0x");
            crate::print_hex(tx_dma_addr as usize);
            crate::print_str("\n");

            self.tx_descriptors[i].addr = tx_dma_addr as u32;
            self.tx_descriptors[i].status = TxDescriptor::STATUS_USED;
            if i == 3 {
                self.tx_descriptors[i].status |= TxDescriptor::STATUS_WRAP;
            }
        }

        // Set descriptor queue base addresses
        let rxqbase_cpu = self.rx_descriptors.as_ptr() as usize;
        let txqbase_cpu = self.tx_descriptors.as_ptr() as usize;
        let rxqbase_dma = Self::cpu_to_dma_addr(rxqbase_cpu);
        let txqbase_dma = Self::cpu_to_dma_addr(txqbase_cpu);

        crate::print_str("RP1 Ethernet: Setting descriptor queues:\n");
        crate::print_str("  RXQBASE: CPU=0x");
        crate::print_hex(rxqbase_cpu);
        crate::print_str(" DMA=0x");
        crate::print_hex(rxqbase_dma as usize);
        crate::print_str("\n  TXQBASE: CPU=0x");
        crate::print_hex(txqbase_cpu);
        crate::print_str(" DMA=0x");
        crate::print_hex(txqbase_dma as usize);
        crate::print_str("\n");

        // Clean cache for descriptors to ensure DMA sees them
        unsafe {
            // Clean RX descriptors
            let rx_desc_start = rxqbase_cpu;
            let rx_desc_end = rx_desc_start + core::mem::size_of_val(&self.rx_descriptors);
            let mut addr = rx_desc_start & !63;
            while addr < rx_desc_end {
                core::arch::asm!("dc cvac, {0}", in(reg) addr, options(nostack));
                addr += 64;
            }
            // Clean TX descriptors
            let tx_desc_start = txqbase_cpu;
            let tx_desc_end = tx_desc_start + core::mem::size_of_val(&self.tx_descriptors);
            let mut addr = tx_desc_start & !63;
            while addr < tx_desc_end {
                core::arch::asm!("dc cvac, {0}", in(reg) addr, options(nostack));
                addr += 64;
            }
            core::arch::asm!("dsb sy", options(nostack));
        }

        // Write lower 32 bits of queue base addresses (GEM registers are 32-bit)
        self.write_reg(GEM_RXQBASE, rxqbase_dma as u32);
        self.write_reg(GEM_TXQBASE, txqbase_dma as u32);

        // Set MAC address
        let mut effective_mac = mac;
        if mac == [0u8; 6] {
            effective_mac = [0x02, 0x11, 0x22, 0x33, 0x44, 0x55];
            crate::print_str("RP1 Ethernet: Warning - MAC was all-zero, using fallback\n");
        }

        let mac_lo =
            u32::from_le_bytes([effective_mac[0], effective_mac[1], effective_mac[2], effective_mac[3]]);
        let mac_hi = u16::from_le_bytes([effective_mac[4], effective_mac[5]]) as u32;
        self.write_reg(GEM_SPADDR1LO, mac_lo);
        self.write_reg(GEM_SPADDR1HI, mac_hi);

        crate::print_str("RP1 Ethernet: MAC set to ");
        for i in 0..6 {
            if i > 0 {
                crate::print_str(":");
            }
            crate::print_hex(effective_mac[i] as usize);
        }
        crate::print_str("\n");

        // Configure network (default 100M full w/ RX checksum)
        let nwcfg = GEM_FD | GEM_SPD | GEM_RXCSUM_EN;
        self.write_reg(GEM_NWCFG, nwcfg);

        // Configure DMA
        let dmacfg = GEM_DISC_WHEN_NO_AHB | GEM_FBLDO_INCR4;
        self.write_reg(GEM_DMACFG, dmacfg);

        // Enable TX/RX/MDIO and RX/TX interrupt bits (Linux-like mask)
        let linux_like_nwctrl: u32 = (1 << 1) | (1 << 2) | (1 << 4) | (1 << 20) | (1 << 21);
        self.write_reg(GEM_NWCTRL, linux_like_nwctrl);

        // Debug: read back and print
        let nwctrl_rb = self.read_reg(GEM_NWCTRL);
        crate::print_str("RP1 Ethernet: NWCTRL(after write)=0x");
        crate::print_hex(nwctrl_rb as usize);
        crate::print_str("\n");

        // Also enable ETH_CFG-level ethernet interrupt so RP1 raises its IRQ
        self.set_eth_cfg_interrupts(ETH_CFG_IRQ_ETHERNET, true);

        // Debug: Read back all important registers
        let nwcfg_rb = self.read_reg(GEM_NWCFG);
        let dmacfg_rb = self.read_reg(GEM_DMACFG);
        let rxqbase_rb = self.read_reg(GEM_RXQBASE);
        let txqbase_rb = self.read_reg(GEM_TXQBASE);

        crate::print_str("RP1 Ethernet: NWCTRL=0x");
        crate::print_hex(nwctrl_rb as usize);
        crate::print_str(" NWCFG=0x");
        crate::print_hex(nwcfg_rb as usize);
        crate::print_str("\n");

        crate::print_str("RP1 Ethernet: DMACFG=0x");
        crate::print_hex(dmacfg_rb as usize);
        crate::print_str("\n");

        crate::print_str("RP1 Ethernet: RXQBASE=0x");
        crate::print_hex(rxqbase_rb as usize);
        crate::print_str(" TXQBASE=0x");
        crate::print_hex(txqbase_rb as usize);
        crate::print_str("\n");

        crate::print_str("RP1 Ethernet: RX desc[0] addr=0x");
        crate::print_hex(self.rx_descriptors[0].addr as usize);
        crate::print_str(" RX buf[0]=0x");
        crate::print_hex(self.rx_buffers[0].0.as_ptr() as usize);
        crate::print_str("\n");

        // Debug: Read back status
        let nwsr = self.read_reg(GEM_NWSR);
        crate::print_str("RP1 Ethernet: Network status=0x");
        crate::print_hex(nwsr as usize);
        crate::print_str("\n");

        crate::print_str("RP1 Ethernet: Initialized\n");
        Ok(())
    }

    pub fn send(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() > 2048 {
            return Err("Packet too large");
        }
        // Prefer the new low-level rp1_gbe implementation.
        unsafe {
            crate::drivers::rp1_gbe::send_frame(data);
        }

        Ok(())
    }

    pub fn recv(&mut self, buffer: &mut [u8]) -> Option<usize> {
        // Try the new low-level rp1_gbe path first.
        unsafe {
            if let Some(buf) = crate::drivers::rp1_gbe::poll_rx() {
                let len = buf.len().min(buffer.len());
                buffer[..len].copy_from_slice(&buf[..len]);
                return Some(len);
            }
        }

        // Fallback to original descriptor-based code if rp1_gbe has no frames.
        // Invalidate cache for descriptor to ensure we see DMA updates
        unsafe {
            let desc_addr = &self.rx_descriptors[self.rx_index] as *const _ as usize;
            core::arch::asm!("dc civac, {0}", in(reg) desc_addr, options(nostack));
            core::arch::asm!("dsb sy", options(nostack));
        }

        let desc = &mut self.rx_descriptors[self.rx_index];

        // Check ownership: status.bit31 == 1 means hardware still owns it
        if (desc.status & RxDescriptor::STATUS_OWN) != 0 {
            return None;
        }

        crate::print_str("[ETH] RX frame received! desc[");
        crate::print_dec(self.rx_index);
        crate::print_str("] addr=0x");
        crate::print_hex(desc.addr as usize);
        crate::print_str(" status=0x");
        crate::print_hex(desc.status as usize);
        crate::print_str("\n");

        let frame_len = (desc.status & 0x1FFF) as usize;

        // Check frame length is valid
        if frame_len == 0 || frame_len > buffer.len() || frame_len > 2048 {
            // Re-arm descriptor for DMA
            let buf_ptr = self.rx_buffers[self.rx_index].0.as_ptr() as usize;
            let mut new_addr =
                (Self::cpu_to_dma_addr(buf_ptr) as u32) | RxDescriptor::ADDR_VALID;
            if self.rx_index == 3 {
                new_addr |= RxDescriptor::ADDR_WRAP;
            }
            desc.addr = new_addr;
            desc.status = RxDescriptor::STATUS_OWN;
            self.rx_index = (self.rx_index + 1) % 4;
            return None;
        }

        // Invalidate cache for the receive buffer before reading
        unsafe {
            let buf_start = self.rx_buffers[self.rx_index].0.as_ptr() as usize;
            let buf_end = buf_start + frame_len;
            let mut addr = buf_start & !63;
            while addr < buf_end {
                core::arch::asm!("dc civac, {0}", in(reg) addr, options(nostack));
                addr += 64;
            }
            core::arch::asm!("dsb sy", options(nostack));
        }

        // Copy data from buffer
        buffer[..frame_len]
            .copy_from_slice(&self.rx_buffers[self.rx_index].0[..frame_len]);

        // Release descriptor back to DMA
        let buf_ptr = self.rx_buffers[self.rx_index].0.as_ptr() as usize;
        let mut new_addr =
            (Self::cpu_to_dma_addr(buf_ptr) as u32) | RxDescriptor::ADDR_VALID;
        if self.rx_index == 3 {
            new_addr |= RxDescriptor::ADDR_WRAP;
        }
        desc.addr = new_addr;
        desc.status = RxDescriptor::STATUS_OWN;

        self.rx_index = (self.rx_index + 1) % 4;

        Some(frame_len)
    }
}

static mut RP1_ETHERNET: Option<Rp1Ethernet> = None;

pub fn init_rp1_ethernet(rp1_base: usize, mac: [u8; 6]) -> Result<(), &'static str> {
    unsafe {
        crate::drivers::rp1_control::rp1_init_gbe();
        // Delegate low-level MAC/GBE init to the new rp1_gbe implementation
        // (minimal-intrusion approach). Then keep a Rp1Ethernet instance
        // around as the high-level API surface used by the rest of the kernel.
        crate::drivers::rp1_gbe::gbe_init(mac);

        let eth = Rp1Ethernet::new(rp1_base);
        // We do not call eth.init() here because the low-level initialization
        // is handled by rp1_gbe; Rp1Ethernet methods `send`/`recv` are
        // adjusted to delegate to `rp1_gbe` when available.
        RP1_ETHERNET = Some(eth);
    }
    Ok(())
}

pub fn get_rp1_ethernet() -> Option<&'static mut Rp1Ethernet> {
    unsafe { RP1_ETHERNET.as_mut() }
}

/// Minimal IRQ handler invoked from the central IRQ entry.
/// This checks ETH_CFG interrupt status, clears it, and triggers
/// a network stack poll so smoltcp will consume received frames.
pub fn rp1_eth_irq_handler(_intid: u32) {
    crate::print_str("RP1 Ethernet: rp1_eth_irq_handler invoked\n");
    if let Some(eth) = get_rp1_ethernet() {
        let status = eth.get_eth_cfg_interrupt_status();
        crate::print_str("RP1 Ethernet: ETH_CFG INTS=0x");
        crate::print_hex(status as usize);
        crate::print_str("\n");
        if status != 0 {
            eth.clear_eth_cfg_interrupts(status);
            crate::print_str("RP1 Ethernet: Cleared ETH_CFG INTS\n");
        }

        if let Some(stack) = crate::network::get_network_stack() {
            let ts = crate::drivers::timer::TIMER.get_ticks();
            stack.poll(ts);
        }
    } else {
        crate::print_str("RP1 Ethernet: rp1_eth_irq_handler - no eth instance\n");
    }
}
