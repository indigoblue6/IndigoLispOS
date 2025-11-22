// pcie.rs - BCM2712 PCIe controller driver (modeled after Raspberry Pi Linux)
//
// This implementation mirrors the initialization flow used by
// drivers/pci/controller/pcie-brcmstb.c (rpi-6.12.y).  Only the pieces
// required for RP1 bring-up are implemented.

use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::drivers::timer::TIMER;
use crate::drivers::gic;

const PCIE_BASE: usize = 0x10_0012_0000;

// Controller register offsets (BCM7712/2712 variant)
const PCIE_MISC_MISC_CTRL: usize = 0x4008;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LO: usize = 0x400c;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_HI: usize = 0x4010;
const PCIE_MISC_CTRL_1: usize = 0x40a0;
const PCIE_MISC_UBUS_CTRL: usize = 0x40a4;
const PCIE_MISC_UBUS_TIMEOUT: usize = 0x40a8;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT: usize = 0x4070;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP: usize = 0x4074;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI: usize = 0x4080;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI: usize = 0x4084;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP_HI: usize = 0x4088;
const PCIE_MISC_RC_BAR1_CONFIG_LO: usize = 0x402c;
const PCIE_MISC_RC_BAR1_CONFIG_HI: usize = 0x4030;
const PCIE_MISC_RC_BAR2_CONFIG_LO: usize = 0x4034;
const PCIE_MISC_RC_BAR2_CONFIG_HI: usize = 0x4038;
const PCIE_MISC_RC_CONFIG_RETRY_TIMEOUT: usize = 0x405c;
const PCIE_MISC_PCIE_CTRL: usize = 0x4064;
const PCIE_MISC_PCIE_STATUS: usize = 0x4068;
const PCIE_MISC_AXI_INTF_CTRL: usize = 0x416c;
const PCIE_MISC_AXI_READ_ERROR_DATA: usize = 0x4170;
const PCIE_RC_CFG_PRIV1_ID_VAL3: usize = 0x043c;
const PCIE_RC_CFG_PRIV1_LINK_CAPABILITY: usize = 0x04dc;
const PCIE_RC_CFG_VENDOR_VENDOR_SPECIFIC_REG1: usize = 0x0188;
const PCIE_RC_PL_PHY_CTL_15: usize = 0x184c;
const PCIE_EXT_CFG_INDEX: usize = 0x9000;
const PCIE_EXT_CFG_DATA: usize = 0x9004;
const PCIE_RGR1_SW_INIT_1: usize = 0x9210;
const PCIE_RGR1_RBUS_TIMEOUT: usize = PCIE_RGR1_SW_INIT_1 - 8;
const PCIE_HARD_DEBUG: usize = 0x4304;
const PCIE_RC_DL_MDIO_ADDR: usize = 0x1100;
const PCIE_RC_DL_MDIO_WR_DATA: usize = 0x1104;
const PCIE_RC_DL_MDIO_RD_DATA: usize = 0x1108;

// VDM (Vendor Defined Message) registers - required for BCM2712
const PCIE_RC_TL_VDM_CTL0: usize = 0x0a20;
const PCIE_RC_TL_VDM_CTL1: usize = 0x0a0c;

// Misc control masks
const MISC_CTRL_RCB_64B: u32 = 0x80;
const MISC_CTRL_RCB_MPS: u32 = 0x400;
const MISC_CTRL_SCB_ACCESS_EN: u32 = 0x1000;
const MISC_CTRL_CFG_READ_UR: u32 = 0x2000;
const MISC_CTRL_MAX_BURST_MASK: u32 = 0x300000;
const MISC_CTRL_MAX_BURST_SHIFT: u32 = 20;
const MISC_CTRL1_EN_VDM_QOS_CONTROL: u32 = 1 << 5;

// CPU -> PCIe window (1 MiB granularity)
// Correct masks for BCM2712 (12-bit width for lower part)
const WIN_BASE_MASK: u32 = 0x0000_FFF0; // GENMASK(15, 4)
const WIN_LIMIT_MASK: u32 = 0xFFF0_0000; // GENMASK(31, 20)
const WIN_ADDR_UPPER_SHIFT: u32 = 12;
const WIN_BASE_HI_MASK: u32 = 0xff;
const WIN_LIMIT_HI_MASK: u32 = 0xff;

// RGR1 / reset bits
const BRIDGE_SW_INIT_MASK: u32 = 0x2;
const PCIE_MISC_PCIE_CTRL_PCIE_PERSTB_MASK: u32 = 0x4;

// HARD_DEBUG bits
const HARD_DEBUG_SERDES_IDDQ_MASK: u32 = 0x0800_0000;
const HARD_DEBUG_CLKREQ_MASK: u32 = 0x2 | 0x10000 | 0x100000 | 0x200000;
const HARD_DEBUG_L1SS_ENABLE: u32 = 0x200000;

// AXI / UBUS workarounds
const UBUS_REPLY_ERR_DIS: u32 = 1 << 13;
const UBUS_REPLY_DECERR_DIS: u32 = 1 << 19;
const AXI_EN_RCLK_QOS_ARRAY_FIX: u32 = 1 << 13;
const AXI_EN_QOS_UPDATE_TIMING_FIX: u32 = 1 << 12;
const AXI_DIS_QOS_GATING_IN_MASTER: u32 = 1 << 11;
const AXI_MASTER_MAX_OUTSTANDING_REQUESTS_MASK: u32 = 0x3f;

// Link capability / class code
const LINK_CAP_ASPM_MASK: u32 = 0xC00;
const LINK_CAP_ASPM_SHIFT: u32 = 10;
const LINK_CAP_ASPM_L0S_L1: u32 = 0x3;
const CLASS_CODE_MASK: u32 = 0x00FF_FFFF;
const CLASS_CODE_PCI_BRIDGE: u32 = 0x060400;

// MDIO helpers
const MDIO_DATA_MASK: u32 = 0x7fff_ffff;
const MDIO_DATA_DONE_MASK: u32 = 0x8000_0000;
const MDIO_PORT_EXT_MASK: u32 = 0x0020_0000;
const MDIO_REGAD_MASK: u32 = 0x0000_FFFF;
const MDIO_CMD_MASK: u32 = 0x0010_0000;
const MDIO_CMD_READ: u32 = 0x0010_0000;
const MDIO_CMD_WRITE: u32 = 0x0;
const MDIO_PORT0: u32 = 0;
const SET_ADDR_OFFSET: u32 = 0x1f;

const PCIE_RC_PL_PHY_CTL_15_PM_CLK_PERIOD_MASK: u32 = 0xff;

// Vendor endian bits
const ENDN_BAR2_MASK: u32 = 0xC;

// RP1 identifiers
const RP1_VENDOR_ID: u16 = 0x1de4;
const RP1_DEVICE_ID: u16 = 0x0001;

// RP1 outbound mapping (CPU -> PCIe -> RP1)
// Circle uses 0x1F_0000_0000 for RP1 peripherals, so we match that.
// Circle uses 0x1F_0000_0000, but we use 0x60_0000_0000 as it is known to work on this setup.
//   CPU: 0x60_0000_0000 -> PCIe: 0xC0000000
pub const RP1_OUTBOUND_CPU_BASE: u64 = 0x60_0000_0000;
pub const RP1_OUTBOUND_SIZE: u64 = 0x1_0000_0000; // 4GB window (covers all RP1)
pub const RP1_BAR1_PCIE_BASE: u64 = 0xC000_0000; // PCIe address for BAR1 (avoid 0x0)
// Programmed BAR1/ BAR0 CPU bases (filled at runtime when BARs are programmed).
pub static RP1_BAR1_CPU_BASE: AtomicU64 = AtomicU64::new(RP1_OUTBOUND_CPU_BASE);
// Programmed BAR0 config value (lower bits are flags, masked when used)
pub static RP1_BAR0_CPU_BASE: AtomicU64 = AtomicU64::new(0);

pub const RP1_MAILBOX_OFFSET: usize = 0x0000_8000;
pub const RP1_CLOCKS_OFFSET: usize = 0x0001_8000;
pub const RP1_GPIO_OFFSET: usize = 0x000D_0000;
pub const RP1_ETH_OFFSET: usize = 0x0010_0000;
pub const RP1_SYS_OFFSET: usize = 0x0000_0000;

pub struct PcieController {
    base: usize,
}

impl PcieController {
    pub fn new() -> Self {
        PcieController { base: PCIE_BASE }
    }

    fn replace_bits(orig: u32, mask: u32, value: u32) -> u32 {
        if mask == 0 {
            return orig;
        }
        let shift = mask.trailing_zeros();
        let width = mask.count_ones();
        let value_mask = if width >= 32 {
            u32::MAX
        } else {
            (1u32 << width) - 1
        };
        (orig & !mask) | (((value & value_mask) << shift) & mask)
    }

    fn read_reg(&self, off: usize) -> u32 {
        unsafe { ptr::read_volatile((self.base + off) as *const u32) }
    }

    fn write_reg(&self, off: usize, val: u32) {
        unsafe { ptr::write_volatile((self.base + off) as *mut u32, val) }
    }

    fn log_axi_error(&self, prefix: &str, val: u32) {
        if val == 0 {
            return;
        }
        let resp = (val >> 28) & 0xF;
        let attr = (val >> 24) & 0xF;
        let id = (val >> 16) & 0xFF;
        let addr = val & 0xFFFF;
        crate::print_str(prefix);
        crate::print_str(" resp=0x");
        crate::print_hex(resp as usize);
        crate::print_str(" attr=0x");
        crate::print_hex(attr as usize);
        crate::print_str(" id=0x");
        crate::print_hex(id as usize);
        crate::print_str(" addr=0x");
        crate::print_hex(addr as usize);
        crate::print_str("\n");
    }

    fn delay_us(us: u32) {
        TIMER.delay_us(us);
    }

    fn delay_ms(ms: u32) {
        TIMER.delay_us(ms * 1000);
    }

    fn mdio_form_pkt(port: u32, reg: u32, cmd: u32) -> u32 {
        let mut pkt = 0;
        if port >= 16 {
            pkt |= MDIO_PORT_EXT_MASK;
        }
        pkt |= (port & 0xF) << 16;
        pkt |= reg & MDIO_REGAD_MASK;
        pkt |= cmd & MDIO_CMD_MASK;
        pkt
    }

    pub fn mdio_write(&mut self, port: u32, reg: u32, val: u16) -> Result<(), &'static str> {
        let pkt = Self::mdio_form_pkt(port, reg, MDIO_CMD_WRITE);
        self.write_reg(PCIE_RC_DL_MDIO_ADDR, pkt);
        self.read_reg(PCIE_RC_DL_MDIO_ADDR);
        self.write_reg(PCIE_RC_DL_MDIO_WR_DATA, MDIO_DATA_DONE_MASK | (val as u32));
        for _ in 0..1000 {
            let data = self.read_reg(PCIE_RC_DL_MDIO_WR_DATA);
            if (data & MDIO_DATA_DONE_MASK) == 0 {
                return Ok(());
            }
            Self::delay_us(10);
        }
        Err("MDIO write timeout")
    }

    pub fn mdio_read(&mut self, port: u32, reg: u32) -> Result<u16, &'static str> {
        let pkt = Self::mdio_form_pkt(port, reg, MDIO_CMD_READ);
        self.write_reg(PCIE_RC_DL_MDIO_ADDR, pkt);
        self.read_reg(PCIE_RC_DL_MDIO_ADDR);
        for _ in 0..1000 {
            let data = self.read_reg(PCIE_RC_DL_MDIO_RD_DATA);
            if (data & MDIO_DATA_DONE_MASK) != 0 {
                return Ok((data & MDIO_DATA_MASK) as u16);
            }
            Self::delay_us(10);
        }
        Err("MDIO read timeout")
    }

    fn bridge_reset(&mut self, assert: bool) {
        let mut val = self.read_reg(PCIE_RGR1_SW_INIT_1);
        if assert {
            val |= BRIDGE_SW_INIT_MASK;
        } else {
            val &= !BRIDGE_SW_INIT_MASK;
        }
        self.write_reg(PCIE_RGR1_SW_INIT_1, val);
    }

    fn perst(&mut self, assert: bool) {
        let mut val = self.read_reg(PCIE_MISC_PCIE_CTRL);
        if assert {
            val &= !PCIE_MISC_PCIE_CTRL_PCIE_PERSTB_MASK;
        } else {
            val |= PCIE_MISC_PCIE_CTRL_PCIE_PERSTB_MASK;
        }
        self.write_reg(PCIE_MISC_PCIE_CTRL, val);
    }

    fn release_serdes(&mut self) {
        let mut val = self.read_reg(PCIE_HARD_DEBUG);
        val &= !HARD_DEBUG_SERDES_IDDQ_MASK;
        self.write_reg(PCIE_HARD_DEBUG, val);
    }

    fn configure_clkreq(&mut self) {
        let mut val = self.read_reg(PCIE_HARD_DEBUG);
        val &= !HARD_DEBUG_CLKREQ_MASK;
        val |= HARD_DEBUG_L1SS_ENABLE;
        self.write_reg(PCIE_HARD_DEBUG, val);
        self.extend_rbus_timeout();
    }

    fn extend_rbus_timeout(&mut self) {
        self.write_reg(PCIE_RGR1_RBUS_TIMEOUT, 0xFFFF_FFFF);
    }

    fn set_misc_ctrl(&mut self) {
        let mut val = self.read_reg(PCIE_MISC_MISC_CTRL);
        val |= MISC_CTRL_SCB_ACCESS_EN
            | MISC_CTRL_CFG_READ_UR
            | MISC_CTRL_RCB_MPS
            | MISC_CTRL_RCB_64B;
        val &= !MISC_CTRL_MAX_BURST_MASK;
        val |= (2 << MISC_CTRL_MAX_BURST_SHIFT) & MISC_CTRL_MAX_BURST_MASK;
        self.write_reg(PCIE_MISC_MISC_CTRL, val);

        crate::print_str("PCIe: MISC_CTRL configured=0x");
        crate::print_hex(val as usize);
        crate::print_str("\n");
    }

    fn configure_outbound_win(&mut self) {
        let cpu_base  = 0x6000_0000u64;
        let size      = 0x4000_0000u64; // 1GB
        let pcie_base = 0xC000_0000u64;

        let start_mb = cpu_base >> 20;
        let limit_mb = (cpu_base + size - 1) >> 20;
        let remap_mb = pcie_base >> 20;

        // BASE/LIMIT
        let base_limit =
            ((start_mb & 0xFFF) << 20) |
            (limit_mb & 0xFFF);

        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT, base_limit as u32);

        // HI registers
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI, 0);
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI, 0);

        // REMAP
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP,     (remap_mb & 0xFFFF_FFFF) as u32);
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP_HI,  0);

        crate::print_str("PCIe: WIN0 configured (Linux/Circle compatible)\n");
    }

    fn configure_inbound_win(&mut self) {
        // Disable BAR1
        self.write_reg(PCIE_MISC_RC_BAR1_CONFIG_LO, 0);
        self.write_reg(PCIE_MISC_RC_BAR1_CONFIG_HI, 0);

        // Configure BAR2 for 4GB inbound window at CPU 0x0
        // This allows RP1 to access system memory (e.g. for Mailbox DMA)
        // Size encoding for 4GB (2^32): 32 - 15 = 17 (0x11)
        // LO = (cpu_addr_lo & ~0xFFF) | size_enc
        let size_enc = 17; // 4GB
        let cpu_addr_lo = 0;
        let val_lo = (cpu_addr_lo & !0xFFF) | size_enc;

        self.write_reg(PCIE_MISC_RC_BAR2_CONFIG_LO, val_lo);
        self.write_reg(PCIE_MISC_RC_BAR2_CONFIG_HI, 0); // CPU addr hi = 0
        
        self.write_reg(PCIE_MISC_RC_BAR2_CONFIG_LO, val_lo);
        self.write_reg(PCIE_MISC_RC_BAR2_CONFIG_HI, 0);

        crate::print_str("PCIe: Inbound BAR2 configured for 4GB @ CPU 0x0\n");
    }

    fn configure_axi_workarounds(&mut self) {
        let mut val = self.read_reg(PCIE_MISC_UBUS_CTRL);
        val |= UBUS_REPLY_ERR_DIS | UBUS_REPLY_DECERR_DIS;
        self.write_reg(PCIE_MISC_UBUS_CTRL, val);
        self.write_reg(PCIE_MISC_AXI_READ_ERROR_DATA, 0xffff_ffff);
        self.write_reg(PCIE_MISC_UBUS_TIMEOUT, 0x0B2D_0000);
        self.write_reg(PCIE_MISC_RC_CONFIG_RETRY_TIMEOUT, 0x0ABA_0000);

        let mut axi = self.read_reg(PCIE_MISC_AXI_INTF_CTRL);
        axi |= AXI_EN_RCLK_QOS_ARRAY_FIX
            | AXI_EN_QOS_UPDATE_TIMING_FIX
            | AXI_DIS_QOS_GATING_IN_MASTER;
        self.write_reg(PCIE_MISC_AXI_INTF_CTRL, axi);

        if (axi & AXI_EN_QOS_UPDATE_TIMING_FIX) == 0 {
            axi &= !AXI_MASTER_MAX_OUTSTANDING_REQUESTS_MASK;
            axi |= 15;
            self.write_reg(PCIE_MISC_AXI_INTF_CTRL, axi);
        }

        let mut ctrl1 = self.read_reg(PCIE_MISC_CTRL_1);
        ctrl1 &= !MISC_CTRL1_EN_VDM_QOS_CONTROL;
        self.write_reg(PCIE_MISC_CTRL_1, ctrl1);
    }

    fn post_setup_bcm2712(&mut self) -> Result<(), &'static str> {
        self.mdio_write(MDIO_PORT0, SET_ADDR_OFFSET, 0x1600)?;
        let regs = [0x16, 0x17, 0x18, 0x19, 0x1b, 0x1c, 0x1e];
        let data = [0x50b9, 0xbda1, 0x0094, 0x97b4, 0x5030, 0x5030, 0x0007];
        for (reg, val) in regs.iter().zip(data.iter()) {
            self.mdio_write(MDIO_PORT0, *reg as u32, *val)?;
        }

        Self::delay_us(200);

        let mut val = self.read_reg(PCIE_RC_PL_PHY_CTL_15);
        val &= !PCIE_RC_PL_PHY_CTL_15_PM_CLK_PERIOD_MASK;
        val |= 0x12;
        self.write_reg(PCIE_RC_PL_PHY_CTL_15, val);

        self.configure_axi_workarounds();
        Ok(())
    }

    fn set_link_capabilities(&mut self) {
        let mut val = self.read_reg(PCIE_RC_CFG_PRIV1_LINK_CAPABILITY);
        val &= !LINK_CAP_ASPM_MASK;
        val |= (LINK_CAP_ASPM_L0S_L1 << LINK_CAP_ASPM_SHIFT) & LINK_CAP_ASPM_MASK;
        self.write_reg(PCIE_RC_CFG_PRIV1_LINK_CAPABILITY, val);
    }

    fn set_class_code(&mut self) {
        let mut val = self.read_reg(PCIE_RC_CFG_PRIV1_ID_VAL3);
        val &= !CLASS_CODE_MASK;
        val |= CLASS_CODE_PCI_BRIDGE;
        self.write_reg(PCIE_RC_CFG_PRIV1_ID_VAL3, val);
    }

    fn set_vendor_endian(&mut self) {
        let mut val = self.read_reg(PCIE_RC_CFG_VENDOR_VENDOR_SPECIFIC_REG1);
        val &= !ENDN_BAR2_MASK;
        self.write_reg(PCIE_RC_CFG_VENDOR_VENDOR_SPECIFIC_REG1, val);
    }

    fn wait_link(&mut self) -> Result<(), &'static str> {
        let mut timeout = 1_000_000;
        while timeout > 0 {
            let st = self.read_reg(PCIE_MISC_PCIE_STATUS);
            if (st & 0x30) == 0x30 {
                crate::print_str("PCIe: Link up (status=0x");
                crate::print_hex(st as usize);
                crate::print_str(")\n");

                let link_cap = self.read_reg(PCIE_RC_CFG_PRIV1_LINK_CAPABILITY);
                crate::print_str("PCIe: Link capability=0x");
                crate::print_hex(link_cap as usize);
                crate::print_str("\n");

                return Ok(());
            }
            timeout -= 1;
        }
        Err("PCIe link timeout")
    }

    pub fn init(&mut self) -> Result<(), &'static str> {
        crate::print_str("PCIe: Initializing controller...\n");

        self.bridge_reset(true);
        self.perst(true);
        Self::delay_us(200);

        self.bridge_reset(false);
        self.release_serdes();
        Self::delay_us(200);
        self.configure_clkreq();

        self.set_misc_ctrl();

        self.configure_outbound_win();

        self.configure_inbound_win();
        self.set_link_capabilities();
        self.set_class_code();
        self.set_vendor_endian();

        self.post_setup_bcm2712()?;

        crate::print_str("PCIe: Configuring VDM...\n");
        self.write_reg(PCIE_RC_TL_VDM_CTL0, 0x1);
        self.write_reg(PCIE_RC_TL_VDM_CTL1, 0x0);

        self.perst(false);
        Self::delay_ms(250);

        self.wait_link()?;

        let axi_err = self.read_reg(PCIE_MISC_AXI_READ_ERROR_DATA);
        crate::print_str("PCIe: AXI read error data=0x");
        crate::print_hex(axi_err as usize);
        crate::print_str("\n");
        self.log_axi_error("PCIe: AXI error detail", axi_err);

        if axi_err != 0 {
            self.write_reg(PCIE_MISC_AXI_READ_ERROR_DATA, 0xFFFF_FFFF);
            crate::print_str("PCIe: Cleared AXI errors\n");
        }

        Ok(())
    }

    // BCM2712 PCIe uses INDEX/DATA register pair for config space access
    // (NOT ECAM - that's a common misconception!)
    // 
    // How it works:
    // 1. Write bus/dev/func/reg to PCIE_EXT_CFG_INDEX
    // 2. Read/write from PCIE_EXT_CFG_DATA (FIXED address - do NOT add offset!)
    //
    // This matches Linux drivers/pci/controller/pcie-brcmstb.c
    fn cfg_index(&mut self, bus: u8, dev: u8, func: u8, reg: u16) {
        let index =
            ((bus  as u32) << 20) |
            ((dev  as u32) << 15) |
            ((func as u32) << 12) |
            ((reg as u32) & 0xFFC);

        // Debug logging to verify what we are sending
        crate::print_str("PCIe: cfg_index(bus=");
        crate::print_hex(bus as usize);
        crate::print_str(" dev=");
        crate::print_hex(dev as usize);
        crate::print_str(" func=");
        crate::print_hex(func as usize);
        crate::print_str(" reg=0x");
        crate::print_hex(reg as usize);
        crate::print_str(") -> INDEX=0x");
        crate::print_hex(index as usize);
        crate::print_str("\n");

        self.write_reg(PCIE_EXT_CFG_INDEX, index);
    }

    pub fn read_config(&mut self, bus: u8, dev: u8, func: u8, reg: u16) -> u32 {
        // Set index register with bus/dev/func/reg
        self.cfg_index(bus, dev, func, reg);

        // Debug: Dump raw register values to verify offsets (only for reg 0)
        if reg == 0 {
             let idx_val = self.read_reg(PCIE_EXT_CFG_INDEX);
             let data_val = self.read_reg(PCIE_EXT_CFG_DATA);
             crate::print_str("PCIe DEBUG: INDEX_REG(0x9000)=0x");
             crate::print_hex(idx_val as usize);
             crate::print_str(" DATA_REG(0x9004)=0x");
             crate::print_hex(data_val as usize);
             crate::print_str("\n");
        }

        // Read from DATA register at FIXED offset (do NOT add reg offset!)
        self.read_reg(PCIE_EXT_CFG_DATA)
    }

    pub fn write_config(&mut self, bus: u8, dev: u8, func: u8, reg: u16, val: u32) {
        // Set index register with bus/dev/func/reg
        self.cfg_index(bus, dev, func, reg);
        // Write to DATA register at FIXED offset (do NOT add reg offset!)
        self.write_reg(PCIE_EXT_CFG_DATA, val);
    }

    pub fn find_rp1(&mut self) -> Option<Rp1Device<'_>> {
        for dev in 0..32 {
            let id = self.read_config(0, dev, 0, 0);
            // Verbose diagnostic: print every device ID read during scan so
            // we can see if config reads are returning 0xFFFF_FFFF (unreachable)
            crate::print_str("PCIe: scan dev ");
            crate::print_hex(dev as usize);
            crate::print_str(" id=0x");
            crate::print_hex(id as usize);
            crate::print_str("\n");
            if id == 0xffff_ffff {
                continue;
            }
            let vendor = (id & 0xFFFF) as u16;
            let device = ((id >> 16) & 0xFFFF) as u16;
            if vendor == RP1_VENDOR_ID && device == RP1_DEVICE_ID {
                crate::print_str("PCIe: Found RP1 device\n");
                return Some(Rp1Device {
                    controller: self,
                    bus: 0,
                    dev,
                    func: 0,
                });
            }
        }
        None
    }
}

pub struct Rp1Device<'a> {
    controller: &'a mut PcieController,
    bus: u8,
    dev: u8,
    func: u8,
}

impl<'a> Rp1Device<'a> {
    pub fn enable(&mut self) -> Result<(), &'static str> {
        crate::print_str("PCIe: RP1 Device enable() called\n");
        
        // CRITICAL FIX: Enable COMMAND register BEFORE programming BARs
        // Linux does this in pci_enable_device() before any BAR programming
        let cmd_before = self.controller.read_config(self.bus, self.dev, self.func, 0x04);
        crate::print_str("PCIe: RP1 Command BEFORE enable: 0x");
        crate::print_hex(cmd_before as usize);
        crate::print_str("\n");

        // Linux pci_enable_device() sets these bits:
        // bit0 = IO Space Enable (required for RP1 mailbox IRQ delivery)
        // bit1 = Memory Space Enable (required for BAR decoding)
        // bit2 = Bus Master Enable (required for RP1 DMA)
        let new_cmd = cmd_before | 0x0007;
        
        crate::print_str("PCIe: Writing COMMAND register: 0x");
        crate::print_hex(new_cmd as usize);
        crate::print_str("\n");
        
        self.controller.write_config(self.bus, self.dev, self.func, 0x04, new_cmd);
        
        // Verify write succeeded
        let cmd_verify = self.controller.read_config(self.bus, self.dev, self.func, 0x04);
        crate::print_str("PCIe: RP1 Command AFTER write: 0x");
        crate::print_hex(cmd_verify as usize);
        crate::print_str("\n");
        
        // Check if bit0 is set (CRITICAL for firmware mailbox)
        if (cmd_verify & 0x0007) != 0x0007 {
            crate::print_str("PCIe: ERROR - COMMAND bits 0/1/2 not all set!\n");
            crate::print_str("PCIe:   bit0 (IO Enable) = ");
            crate::print_hex((cmd_verify & 0x01) as usize);
            crate::print_str("\n");
            crate::print_str("PCIe:   bit1 (Mem Enable) = ");
            crate::print_hex(((cmd_verify >> 1) & 0x01) as usize);
            crate::print_str("\n");
            crate::print_str("PCIe:   bit2 (Bus Master) = ");
            crate::print_hex(((cmd_verify >> 2) & 0x01) as usize);
            crate::print_str("\n");
            
            // Try one more time with explicit value
            crate::print_str("PCIe: Retrying with explicit 0x0007...\n");
            self.controller.write_config(self.bus, self.dev, self.func, 0x04, 0x0007);
            
            let cmd_retry = self.controller.read_config(self.bus, self.dev, self.func, 0x04);
            crate::print_str("PCIe: RP1 Command after retry: 0x");
            crate::print_hex(cmd_retry as usize);
            crate::print_str("\n");
            
            if (cmd_retry & 0x0007) != 0x0007 {
                crate::print_str("PCIe: FATAL - Cannot set COMMAND register bits!\n");
                crate::print_str("PCIe: This will prevent firmware mailbox from working.\n");
            }
        } else {
            crate::print_str("PCIe: ✓ COMMAND register bits 0/1/2 all set correctly\n");
        }

        // Now program BARs (after COMMAND is enabled)
        self.program_bars();

        // Clear Interrupt Disable bit (bit 10) to enable INTx
        let mut cmd_intx = self.controller.read_config(self.bus, self.dev, self.func, 0x04);
        if (cmd_intx & (1 << 10)) != 0 {
            crate::print_str("PCIe: Clearing Interrupt Disable bit (bit 10)...\n");
            cmd_intx &= !(1 << 10);
            self.controller.write_config(self.bus, self.dev, self.func, 0x04, cmd_intx);
            
            let cmd_verify_intx = self.controller.read_config(self.bus, self.dev, self.func, 0x04);
            crate::print_str("PCIe: RP1 Command after INTx enable: 0x");
            crate::print_hex(cmd_verify_intx as usize);
            crate::print_str("\n");
        }

        // Enable GIC IRQ 229 for RP1 Mailbox
        crate::print_str("PCIe: Enabling GIC IRQ 229 for RP1 Mailbox...\n");
        unsafe {
            gic::gic_enable_irq(229);
            gic::gic_set_priority(229, 0x80);
            gic::gic_set_target_cpu(229, 1); // Target CPU 0 (mask 1)
        }

        let cmd_final = self.controller.read_config(self.bus, self.dev, self.func, 0x04);
        crate::print_str("PCIe: RP1 Command FINAL: 0x");
        crate::print_hex(cmd_final as usize);
        crate::print_str("\n");
        
        // Give RP1 firmware time to initialize
        crate::drivers::timer::TIMER.delay_ms(100);
        crate::print_str("PCIe: RP1 firmware initialization delay complete\n");

        self.dump_and_configure_msi();

        crate::print_str("PCIe: Diagnostic - Reading RP1 config space...\n");
        for i in 0..16 {
            let val = self.controller.read_config(self.bus, self.dev, self.func, (i * 4) as u16);
            if val != 0xFFFF_FFFF && val != 0x0 {
                crate::print_str("  Config[0x");
                crate::print_hex((i * 4) as usize);
                crate::print_str("]=0x");
                crate::print_hex(val as usize);
                crate::print_str("\n");
            }
        }

        crate::print_str("PCIe: RP1 enable() complete\n");
        Ok(())
    }

    fn program_bars(&mut self) {
        // --- BAR0 ---
        crate::print_str("PCIe: Programming BAR0...\n");
        
        // Linux-compatible BAR size probe:
        // 1. Save original value
        let old_bar0 = self.controller.read_config(self.bus, self.dev, self.func, 0x10);
        crate::print_str("PCIe: BAR0 old value (before probe): 0x");
        crate::print_hex(old_bar0 as usize);
        crate::print_str("\n");
        
        // 2. Write all 1s
        self.controller.write_config(self.bus, self.dev, self.func, 0x10, 0xFFFF_FFFF);
        crate::print_str("PCIe: BAR0 wrote 0xFFFFFFFF for size probe\n");
        
        // 3. Read back masked value
        let size_mask = self.controller.read_config(self.bus, self.dev, self.func, 0x10);
        crate::print_str("PCIe: BAR0 value after writing 0xFFFFFFFF: 0x");
        crate::print_hex(size_mask as usize);
        crate::print_str("\n");
        
        // 4. Restore original value
        self.controller.write_config(self.bus, self.dev, self.func, 0x10, old_bar0);
        crate::print_str("PCIe: BAR0 restored to: 0x");
        crate::print_hex(old_bar0 as usize);
        crate::print_str("\n");
        
        // 5. Calculate size: size = ~(val & bar_mask) + 1
        // BAR mask for MMIO is 0xFFFFFFF0 (bits 31:4)
        let bar_mask = 0xFFFFFFF0u32;
        let masked_val = size_mask & bar_mask;
        let bar_size = (!masked_val).wrapping_add(1);
        
        crate::print_str("PCIe: BAR0 size_mask & 0xFFFFFFF0 = 0x");
        crate::print_hex(masked_val as usize);
        crate::print_str("\n");
        crate::print_str("PCIe: BAR0 calculated size: 0x");
        crate::print_hex(bar_size as usize);
        crate::print_str(" (");
        crate::print_hex((bar_size / 1024) as usize);
        crate::print_str(" KB)\n");
        
        // Expected: RP1 BAR0 size = 64KB (0x10000)
        if bar_size != 0x10000 {
            crate::print_str("PCIe: WARNING - BAR0 size mismatch! Expected 0x10000 (64KB)\n");
        } else {
            crate::print_str("PCIe: BAR0 size matches expected 64KB\n");
        }

        // ALWAYS program BAR0 to 0xC0400000 (Linux default)
        // Do NOT trust existing value - it might be garbage
        let bar0_addr = 0xC0400000u32;
        crate::print_str("PCIe: Writing BAR0 PCIe address: 0x");
        crate::print_hex(bar0_addr as usize);
        crate::print_str("\n");
        self.controller
            .write_config(self.bus, self.dev, self.func, 0x10, bar0_addr);

        let final_bar0 = self.controller.read_config(self.bus, self.dev, self.func, 0x10);
        crate::print_str("PCIe: BAR0 read back (PCIe addr): 0x");
        crate::print_hex(final_bar0 as usize);
        crate::print_str("\n");
        
        // Calculate CPU-visible address for BAR0
        // BAR0 PCIe address (e.g., 0xC0400000) maps to CPU address via outbound window
        // CPU_addr = RP1_OUTBOUND_CPU_BASE + (PCIe_addr - RP1_BAR1_PCIE_BASE)
        let bar0_pcie_addr = (final_bar0 & 0xFFFF_FFF0) as u64;
        
        crate::print_str("PCIe: BAR0 PCIe addr (masked): 0x");
        crate::print_hex(bar0_pcie_addr as usize);
        crate::print_str("\n");
        crate::print_str("PCIe: RP1_OUTBOUND_CPU_BASE: 0x");
        crate::print_hex(RP1_OUTBOUND_CPU_BASE as usize);
        crate::print_str("\n");
        crate::print_str("PCIe: RP1_BAR1_PCIE_BASE: 0x");
        crate::print_hex(RP1_BAR1_PCIE_BASE as usize);
        crate::print_str("\n");
        
        let bar0_cpu_addr = RP1_OUTBOUND_CPU_BASE + (bar0_pcie_addr - RP1_BAR1_PCIE_BASE);
        
        crate::print_str("PCIe: BAR0 CPU address (calculated): 0x");
        crate::print_hex(bar0_cpu_addr as usize);
        crate::print_str("\n");
        
        // Verify alignment (must be 4KB aligned)
        if (bar0_cpu_addr & 0xFFF) != 0 {
            crate::print_str("PCIe: ERROR - BAR0 CPU address is NOT 4KB aligned!\n");
        } else {
            crate::print_str("PCIe: BAR0 CPU address is correctly 4KB aligned\n");
        }
        
        // Store CPU address (NOT PCIe address!) in global variable
        RP1_BAR0_CPU_BASE.store(bar0_cpu_addr, Ordering::SeqCst);
        
        // Initialize RP1 mailbox runtime base (BAR0 + 0x3000)
        crate::drivers::mailbox_rp1::init_runtime_mailbox_base(bar0_cpu_addr as usize);

        // --- BAR1 ---
        let existing_bar1_lo = self.controller.read_config(self.bus, self.dev, self.func, 0x14);
        let existing_bar1_hi = self.controller.read_config(self.bus, self.dev, self.func, 0x18); // 64-bit BAR?

        crate::print_str("PCIe: BAR1 existing: LO=0x");
        crate::print_hex(existing_bar1_lo as usize);
        crate::print_str(" HI=0x");
        crate::print_hex(existing_bar1_hi as usize);
        crate::print_str("\n");
        
        // Check if BAR1 is 64-bit (bit 2 set)
        let is_64bit_bar1 = (existing_bar1_lo & 0x4) != 0;

        let addr_bits_1 = existing_bar1_lo & 0xFFFF_FFF0;
        let looks_like_mask_1 = addr_bits_1 >= 0xF000_0000;
        let is_zero_1 = addr_bits_1 == 0;

        if !looks_like_mask_1 && !is_zero_1 {
            crate::print_str("PCIe: BAR1 already programmed, keeping it\n");
             RP1_BAR1_CPU_BASE.store(RP1_OUTBOUND_CPU_BASE, Ordering::SeqCst); // Assume mapped 1:1 to outbound
        } else {
            crate::print_str("PCIe: BAR1 not programmed, programming it\n");
            self.controller.write_config(self.bus, self.dev, self.func, 0x14, 0xFFFF_FFFF);
            if is_64bit_bar1 {
                self.controller.write_config(self.bus, self.dev, self.func, 0x18, 0xFFFF_FFFF);
            }
            
            let size_mask_lo = self.controller.read_config(self.bus, self.dev, self.func, 0x14);
            let size_mask_hi = if is_64bit_bar1 { self.controller.read_config(self.bus, self.dev, self.func, 0x18) } else { 0 };
            
            crate::print_str("PCIe: BAR1 size probe: LO=0x");
            crate::print_hex(size_mask_lo as usize);
            crate::print_str(" HI=0x");
            crate::print_hex(size_mask_hi as usize);
            crate::print_str("\n");

            // Program BAR1 to RP1_BAR1_PCIE_BASE (PCIe address)
            let bar1_addr = RP1_BAR1_PCIE_BASE as u32;
            crate::print_str("PCIe: Writing BAR1: LO=0x");
            crate::print_hex(bar1_addr as usize);
            crate::print_str(" HI=0x0\n");
            
            self.controller.write_config(self.bus, self.dev, self.func, 0x14, bar1_addr);
            if is_64bit_bar1 {
                self.controller.write_config(self.bus, self.dev, self.func, 0x18, 0);
            }
            
            // Update global base for drivers
            RP1_BAR1_CPU_BASE.store(RP1_OUTBOUND_CPU_BASE, Ordering::SeqCst);
        }
        
        let final_bar1_lo = self.controller.read_config(self.bus, self.dev, self.func, 0x14);
        let final_bar1_hi = if is_64bit_bar1 { self.controller.read_config(self.bus, self.dev, self.func, 0x18) } else { 0 };
        crate::print_str("PCIe: BAR1 programmed: LO=0x");
        crate::print_hex(final_bar1_lo as usize);
        crate::print_str(" HI=0x");
        crate::print_hex(final_bar1_hi as usize);
        crate::print_str("\n");

        // --- BAR2 (if BAR1 was 32-bit, offset 0x18. If 64-bit, offset 0x1C) ---
        let bar2_offset = if is_64bit_bar1 { 0x1C } else { 0x18 };
        let existing_bar2 = self.controller.read_config(self.bus, self.dev, self.func, bar2_offset as u16);
        
        crate::print_str("PCIe: BAR2 (offset 0x"); crate::print_hex(bar2_offset); crate::print_str(") existing: 0x");
        crate::print_hex(existing_bar2 as usize);
        crate::print_str("\n");
        
        // Probe BAR2
        self.controller.write_config(self.bus, self.dev, self.func, bar2_offset as u16, 0xFFFF_FFFF);
        let size_mask_bar2 = self.controller.read_config(self.bus, self.dev, self.func, bar2_offset as u16);
        
        if size_mask_bar2 != 0 && size_mask_bar2 != 0xFFFFFFFF {
             crate::print_str("PCIe: BAR2 size probe: 0x");
             crate::print_hex(size_mask_bar2 as usize);
             crate::print_str("\n");
             
             // Program BAR2 to 0xC1000000 (Avoid overlap with BAR0/BAR1 peripherals)
             let bar2_addr = 0xC1000000;
             self.controller.write_config(self.bus, self.dev, self.func, bar2_offset as u16, bar2_addr);
             crate::print_str("PCIe: BAR2 programmed to 0x"); crate::print_hex(bar2_addr as usize); crate::print_str("\n");

             // Calculate CPU address for BAR2
             // CPU_addr = RP1_OUTBOUND_CPU_BASE + (BAR2_PCIe_Addr - RP1_BAR1_PCIE_BASE)
             let bar2_pcie = bar2_addr as u64;
             let bar2_cpu = RP1_OUTBOUND_CPU_BASE + (bar2_pcie - RP1_BAR1_PCIE_BASE);
             
             crate::print_str("PCIe: BAR2 CPU = 0x");
             crate::print_hex(bar2_cpu as usize);
             crate::print_str("\n");

             // Dump RP1_SYS registers
             unsafe {
                 let sys_base = bar2_cpu as usize;
                 let core_ctrl = ptr::read_volatile((sys_base + 0x1000) as *const u32);
                 let fw_ctrl   = ptr::read_volatile((sys_base + 0x1004) as *const u32);
                 let status    = ptr::read_volatile((sys_base + 0x1008) as *const u32);
             
                 crate::print_str("RP1_SYS_CORE_CTRL: 0x"); crate::print_hex(core_ctrl as usize); crate::print_str("\n");
                 crate::print_str("RP1_SYS_FW_CTRL  : 0x"); crate::print_hex(fw_ctrl as usize); crate::print_str("\n");
                 crate::print_str("RP1_SYS_STATUS   : 0x"); crate::print_hex(status as usize); crate::print_str("\n");
             }
        } else {
             crate::print_str("PCIe: BAR2 not present or invalid\n");
             // Restore if it was something else
             self.controller.write_config(self.bus, self.dev, self.func, bar2_offset as u16, existing_bar2);
        }

        // RP1 mailbox is already initialized at line 682 via mailbox_rp1::init_runtime_mailbox_base
    }

    fn program_bar1(&mut self) {
        let id = self.controller.read_config(self.bus, self.dev, self.func, 0x00);
        crate::print_str("PCIe: RP1 ID: 0x");
        crate::print_hex(id as usize);
        crate::print_str("\n");

        let existing_lo = self.controller.read_config(self.bus, self.dev, self.func, 0x14);
        let existing_hi = self.controller.read_config(self.bus, self.dev, self.func, 0x18);

        crate::print_str("PCIe: BAR1 existing: LO=0x");
        crate::print_hex(existing_lo as usize);
        crate::print_str(" HI=0x");
        crate::print_hex(existing_hi as usize);
        crate::print_str("\n");

        let addr_bits = existing_lo & 0xFFFF_FFF0;
        let looks_like_mask = addr_bits >= 0xF000_0000;
        let is_zero = addr_bits == 0;
        let is_valid_bar = !looks_like_mask && !is_zero;

        if is_valid_bar {
            crate::print_str("PCIe: BAR1 already programmed by firmware: 0x");
            crate::print_hex((existing_lo & 0xFFFF_FFF0) as usize);
            crate::print_str(", keeping it\n");
            let programmed_lo = (existing_lo & 0xFFFF_FFF0) as u64;
            let programmed_hi = (existing_hi as u64) & 0xFFFF_FFFFu64;
            let bar1_cpu = (programmed_hi << 32) | programmed_lo;
            RP1_BAR1_CPU_BASE.store(bar1_cpu, Ordering::SeqCst);
            return;
        }

        crate::print_str("PCIe: BAR1 not programmed (looks like size mask or zero), will program it\n");

        self.controller
            .write_config(self.bus, self.dev, self.func, 0x14, 0xFFFF_FFFF);
        self.controller
            .write_config(self.bus, self.dev, self.func, 0x18, 0xFFFF_FFFF);

        let size_lo = self.controller.read_config(self.bus, self.dev, self.func, 0x14);
        let size_hi = self.controller.read_config(self.bus, self.dev, self.func, 0x18);

        crate::print_str("PCIe: BAR1 size probe: LO=0x");
        crate::print_hex(size_lo as usize);
        crate::print_str(" HI=0x");
        crate::print_hex(size_hi as usize);
        crate::print_str("\n");

        // If size_hi is non-zero, it means the BAR supports 64-bit addressing
        // (it returned a mask for the upper bits).
        let supports_64bit = size_hi != 0;

        let (lo, hi) = if supports_64bit {
            (
                ((RP1_BAR1_PCIE_BASE as u32) & 0xFFFF_FFF0) | 0x4, // 64-bit, Non-Prefetchable
                (RP1_BAR1_PCIE_BASE >> 32) as u32,
            )
        } else {
            crate::print_str("PCIe: RP1 uses 32-bit BAR\n");
            (
                ((RP1_BAR1_PCIE_BASE as u32) & 0xFFFF_FFF0) | 0x0, // 32-bit, Non-Prefetchable
                0,
            )
        };

        crate::print_str("PCIe: Writing BAR1: LO=0x");
        crate::print_hex(lo as usize);
        crate::print_str(" HI=0x");
        crate::print_hex(hi as usize);
        crate::print_str("\n");

        self.controller
            .write_config(self.bus, self.dev, self.func, 0x14, lo);
        self.controller
            .write_config(self.bus, self.dev, self.func, 0x18, hi);

        let bar1_lo = self.controller.read_config(self.bus, self.dev, self.func, 0x14);
        let bar1_hi = self.controller.read_config(self.bus, self.dev, self.func, 0x18);

        crate::print_str("PCIe: BAR1 programmed: LO=0x");
        crate::print_hex(bar1_lo as usize);
        crate::print_str(" HI=0x");
        crate::print_hex(bar1_hi as usize);
        crate::print_str("\n");

        let programmed_lo = (bar1_lo & 0xFFFF_FFF0) as u64;
        let programmed_hi = (bar1_hi as u64) & 0xFFFF_FFFFu64;
        let bar1_cpu = (programmed_hi << 32) | programmed_lo;
        RP1_BAR1_CPU_BASE.store(bar1_cpu, Ordering::SeqCst);
    }

    pub fn bar1_cpu_base(&self) -> usize {
        RP1_BAR1_CPU_BASE.load(Ordering::SeqCst) as usize
    }

    pub fn sys_base(&self) -> usize {
        RP1_BAR1_CPU_BASE.load(Ordering::SeqCst) as usize + RP1_SYS_OFFSET
    }

    pub fn ethernet_base(&self) -> usize {
        RP1_BAR1_CPU_BASE.load(Ordering::SeqCst) as usize + RP1_ETH_OFFSET
    }

    pub fn mailbox_base(&self) -> usize {
        RP1_BAR1_CPU_BASE.load(Ordering::SeqCst) as usize + RP1_MAILBOX_OFFSET
    }

    pub fn clocks_base(&self) -> usize {
        RP1_BAR1_CPU_BASE.load(Ordering::SeqCst) as usize + RP1_CLOCKS_OFFSET
    }

    pub fn gpio_base(&self) -> usize {
        RP1_BAR1_CPU_BASE.load(Ordering::SeqCst) as usize + RP1_GPIO_OFFSET
    }

    fn dump_and_configure_msi(&mut self) {
        // Raw dump as requested
        crate::print_str("PCIe: Raw Capability Dump (0x34..0x100):\n");
        let mut off = 0x34;
        while off < 0x100 {
            let val = self.controller.read_config(self.bus, self.dev, self.func, off as u16);
            if val != 0 && val != 0xFFFFFFFF {
                 crate::print_str("  [0x");
                 crate::print_hex(off);
                 crate::print_str("] = 0x");
                 crate::print_hex(val as usize);
                 crate::print_str("\n");
            }
            off += 4;
        }

        crate::print_str("PCIe: Scanning Capabilities...\n");
        let mut ptr = self.controller.read_config(self.bus, self.dev, self.func, 0x34) as u8;
        let mut loop_guard = 0;
        while ptr != 0 && loop_guard < 48 {
            let header = self.controller.read_config(self.bus, self.dev, self.func, ptr as u16);
            let cap_id = (header & 0xFF) as u8;
            let next_ptr = ((header >> 8) & 0xFF) as u8;
            
            crate::print_str("  Cap @ 0x");
            crate::print_hex(ptr as usize);
            crate::print_str(": ID=0x");
            crate::print_hex(cap_id as usize);
            crate::print_str("\n");
            
            if cap_id == 0x05 { // MSI Capability
                self.configure_msi(ptr, header);
            }
            
            ptr = next_ptr;
            loop_guard += 1;
        }
    }

    fn configure_msi(&mut self, offset: u8, header: u32) {
        crate::print_str("PCIe: Found MSI Capability. Configuring...\n");
        
        let ctrl = (header >> 16) as u16;
        let is_64bit = (ctrl & 0x80) != 0;
        
        // Enable MSI (bit 0)
        let new_ctrl = ctrl | 0x1;
        
        let new_header = (header & 0xFFFF) | ((new_ctrl as u32) << 16);
        self.controller.write_config(self.bus, self.dev, self.func, offset as u16, new_header);
        
        crate::print_str("PCIe: MSI Control 0x");
        crate::print_hex(ctrl as usize);
        crate::print_str(" -> 0x");
        crate::print_hex(new_ctrl as usize);
        crate::print_str("\n");
        
        // Address: 0xC0000000, Data: 1
        let msi_addr = 0xC0000000;
        let msi_data = 1;
        
        self.controller.write_config(self.bus, self.dev, self.func, (offset + 4) as u16, msi_addr);
        
        if is_64bit {
            self.controller.write_config(self.bus, self.dev, self.func, (offset + 8) as u16, 0);
            self.controller.write_config(self.bus, self.dev, self.func, (offset + 12) as u16, msi_data);
        } else {
            self.controller.write_config(self.bus, self.dev, self.func, (offset + 8) as u16, msi_data);
        }
        
        crate::print_str("PCIe: MSI Configured (Addr=0xC0000000, Data=1)\n");
    }

    pub fn read_chip_id(&self) -> u32 {
        unsafe { ptr::read_volatile(self.sys_base() as *const u32) }
    }
}

static mut GLOBAL_PCIE: Option<PcieController> = None;

pub fn init_pcie() -> Result<(), &'static str> {
    unsafe {
        let mut ctrl = PcieController::new();
        ctrl.init()?;
        GLOBAL_PCIE = Some(ctrl);
    }
    Ok(())
}

pub fn get_pcie() -> Option<&'static mut PcieController> {
    unsafe { GLOBAL_PCIE.as_mut() }
}

/// Get DMA address for MACB controller (RP1 Ethernet)
/// This is equivalent to Circle's PTR_TO_DMA macro:
/// #define PTR_TO_DMA(p) ((uintptr) (p) | CBcmPCIeHostBridge::GetDMAAddress (PCIE_BUS_MACB))
///
/// For RP1 MACB, DMA addresses need to be translated to PCIe bus addresses.
/// The RP1 is accessed via PCIe, so CPU physical addresses need to be converted
/// to PCIe bus addresses that the RP1's DMA controller can understand.
pub fn get_macb_dma_address(cpu_addr: usize) -> usize {
    // RP1 BAR1 mapping:
    // CPU: 0x60_0000_0000 -> PCIe: 0xC000_0000
    // For DMA, we need to convert CPU addresses to PCIe bus addresses
    
    // Check if address is in the RP1 outbound window
    if cpu_addr >= RP1_OUTBOUND_CPU_BASE as usize 
        && cpu_addr < (RP1_OUTBOUND_CPU_BASE + RP1_OUTBOUND_SIZE) as usize {
        // Convert CPU address to PCIe bus address
        let offset = cpu_addr - RP1_OUTBOUND_CPU_BASE as usize;
        let pcie_addr = RP1_BAR1_PCIE_BASE as usize + offset;
        return pcie_addr;
    }
    
    // If not in RP1 window, return as-is (might be system RAM for DMA)
    // For system RAM, we need to use the physical address directly
    // as the RP1 can access it via the inbound window
    cpu_addr
}
