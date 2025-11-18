// pcie.rs - BCM2712 PCIe controller driver (modeled after Raspberry Pi Linux)
//
// This implementation mirrors the initialization flow used by
// drivers/pci/controller/pcie-brcmstb.c (rpi-6.12.y).  Only the pieces
// required for RP1 bring-up are implemented.

use core::ptr;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::drivers::timer::TIMER;

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
const PCIE_EXT_CFG_DATA: usize = 0x8000;
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
const WIN_BASE_MASK: u32 = 0x000F_FFF0; // GENMASK(19, 4)
const WIN_LIMIT_MASK: u32 = 0xFFF0_0000; // GENMASK(31, 20)
const WIN_ADDR_UPPER_SHIFT: u32 = 16;
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

// RP1 outbound mapping
//
// Recommended mapping (Circle reference):
//   CPU  : 0x0000_0060_0000_0000 .. (base + size - 1)
//   PCIe : 0x0000_0000_C000_0000 .. (PCIe base + size - 1)
pub const RP1_OUTBOUND_CPU_BASE: u64 = 0x0000_0060_0000_0000;
pub const RP1_OUTBOUND_SIZE: u64 = 0x0000_0000_1000_0000; // 256 MiB window
pub const RP1_BAR1_PCIE_BASE: u64 = 0x0000_0000_C000_0000;

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

    fn configure_outbound_win(&mut self, cpu_addr: u64, size: u64, pcie_addr: u64) {
        const SZ_1M: u64 = 1024 * 1024;
        let start_mb = cpu_addr / SZ_1M;
        let limit_mb = (cpu_addr + size - 1) / SZ_1M;

        crate::print_str("PCIe: WIN0 setup - start_mb=0x");
        crate::print_hex(start_mb as usize);
        crate::print_str(" limit_mb=0x");
        crate::print_hex(limit_mb as usize);
        crate::print_str("\n");

        crate::print_str("PCIe: WIN0 BEFORE BASE_LIMIT=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT) as usize);
        crate::print_str(" REMAP_LO=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP) as usize);
        crate::print_str(" REMAP_HI=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP_HI) as usize);
        crate::print_str(" BASE_HI=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI) as usize);
        crate::print_str(" LIMIT_HI=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI) as usize);
        crate::print_str("\n");

        self.write_reg(
            PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LO,
            (pcie_addr & 0xffff_ffff) as u32,
        );
        self.write_reg(
            PCIE_MISC_CPU_2_PCIE_MEM_WIN0_HI,
            (pcie_addr >> 32) as u32,
        );

        let mut base_limit = self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT);
        base_limit = Self::replace_bits(base_limit, WIN_BASE_MASK, start_mb as u32);
        base_limit = Self::replace_bits(base_limit, WIN_LIMIT_MASK, limit_mb as u32);
        base_limit |= 0x1;

        crate::print_str("PCIe: WIN0 BASE_LIMIT computed=0x");
        crate::print_hex(base_limit as usize);
        crate::print_str("\n");

        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT, base_limit);

        let start_hi = ((start_mb >> WIN_ADDR_UPPER_SHIFT) as u32) & WIN_BASE_HI_MASK;
        let limit_hi = ((limit_mb >> WIN_ADDR_UPPER_SHIFT) as u32) & WIN_LIMIT_HI_MASK;

        crate::print_str("PCIe: WIN0 start_hi=0x");
        crate::print_hex(start_hi as usize);
        crate::print_str(" limit_hi=0x");
        crate::print_hex(limit_hi as usize);
        crate::print_str("\n");

        let mut start_hi_val = self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI);
        start_hi_val = Self::replace_bits(start_hi_val, WIN_BASE_HI_MASK, start_hi);
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI, start_hi_val);

        let mut limit_hi_val = self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI);
        limit_hi_val = Self::replace_bits(limit_hi_val, WIN_LIMIT_HI_MASK, limit_hi);
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI, limit_hi_val);

        let remap_mb = (pcie_addr / SZ_1M) as u64;
        self.write_reg(
            PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP,
            (remap_mb & 0xffff_ffff) as u32,
        );
        self.write_reg(
            PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP_HI,
            (remap_mb >> 32) as u32,
        );

        crate::print_str("PCIe: WIN0 -> CPU 0x");
        crate::print_hex(cpu_addr as usize);
        crate::print_str(" size 0x");
        crate::print_hex(size as usize);
        crate::print_str(" PCIe 0x");
        crate::print_hex(pcie_addr as usize);
        crate::print_str("\n");

        crate::print_str("PCIe: WIN0 BASE_LIMIT=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT) as usize);
        crate::print_str(" REMAP_LO=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP) as usize);
        crate::print_str(" REMAP_HI=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP_HI) as usize);
        crate::print_str("\n");

        crate::print_str("PCIe: WIN0 AFTER BASE_HI=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI) as usize);
        crate::print_str(" LIMIT_HI=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI) as usize);
        crate::print_str(" REMAP_HI=0x");
        crate::print_hex(self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_REMAP_HI) as usize);
        crate::print_str("\n");
    }

    fn configure_inbound_win(&mut self) {
        self.write_reg(PCIE_MISC_RC_BAR1_CONFIG_LO, 0);
        self.write_reg(PCIE_MISC_RC_BAR1_CONFIG_HI, 0);
        self.write_reg(PCIE_MISC_RC_BAR2_CONFIG_LO, 0);
        self.write_reg(PCIE_MISC_RC_BAR2_CONFIG_HI, 0);
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

        self.configure_outbound_win(
            RP1_OUTBOUND_CPU_BASE,
            RP1_OUTBOUND_SIZE,
            RP1_BAR1_PCIE_BASE,
        );

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

    fn cfg_index(&mut self, bus: u8, dev: u8, func: u8, reg: u16) {
        let idx = ((bus as u32) << 20)
            | ((dev as u32) << 15)
            | ((func as u32) << 12)
            | ((reg as u32) & 0xFFF);
        self.write_reg(PCIE_EXT_CFG_INDEX, idx);
    }

    pub fn read_config(&mut self, bus: u8, dev: u8, func: u8, reg: u16) -> u32 {
        self.cfg_index(bus, dev, func, reg);
        let off = PCIE_EXT_CFG_DATA + (reg as usize & 0xFFF);
        self.read_reg(off)
    }

    pub fn write_config(&mut self, bus: u8, dev: u8, func: u8, reg: u16, val: u32) {
        self.cfg_index(bus, dev, func, reg);
        let off = PCIE_EXT_CFG_DATA + (reg as usize & 0xFFF);
        self.write_reg(off, val);
    }

    pub fn find_rp1(&mut self) -> Option<Rp1Device> {
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
        let cmd = self.controller.read_config(self.bus, self.dev, self.func, 0x04);

        crate::print_str("PCIe: RP1 Command before: 0x");
        crate::print_hex(cmd as usize);
        crate::print_str("\n");

        self.program_bar0();
        self.program_bar1();

        let new_cmd = cmd | 0x0006;
        self.controller.write_config(self.bus, self.dev, self.func, 0x04, new_cmd);

        let cmd_after = self.controller.read_config(self.bus, self.dev, self.func, 0x04);
        crate::print_str("PCIe: RP1 Command after: 0x");
        crate::print_hex(cmd_after as usize);
        crate::print_str("\n");

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

        crate::print_str("PCIe: Skipping MMIO probe (Linux does not test BAR windows directly)\n");
        Ok(())
    }

    fn program_bar0(&mut self) {
        let existing_bar0 = self.controller.read_config(self.bus, self.dev, self.func, 0x10);

        crate::print_str("PCIe: BAR0 existing: 0x");
        crate::print_hex(existing_bar0 as usize);
        crate::print_str("\n");

        let addr_bits = existing_bar0 & 0xFFFF_FFF0;
        let looks_like_mask = addr_bits >= 0xF000_0000;
        let is_zero = addr_bits == 0;

        if !looks_like_mask && !is_zero {
            crate::print_str("PCIe: BAR0 already programmed, keeping it\n");
        } else {
            self.controller
                .write_config(self.bus, self.dev, self.func, 0x10, 0xFFFF_FFFF);
            let size_bar0 = self.controller.read_config(self.bus, self.dev, self.func, 0x10);

            crate::print_str("PCIe: BAR0 size probe: 0x");
            crate::print_hex(size_bar0 as usize);
            crate::print_str("\n");

            let bar0_addr = 0xC040_0000u32;
            let bar0_val = (bar0_addr & 0xFFFF_FFF0) | 0x0;

            crate::print_str("PCIe: Writing BAR0: 0x");
            crate::print_hex(bar0_val as usize);
            crate::print_str("\n");

            self.controller
                .write_config(self.bus, self.dev, self.func, 0x10, bar0_val);
        }

        let bar0_verify = self.controller.read_config(self.bus, self.dev, self.func, 0x10);
        crate::print_str("PCIe: BAR0 programmed: 0x");
        crate::print_hex(bar0_verify as usize);
        crate::print_str("\n");

        let bar0_clean = (bar0_verify & 0xFFFF_FFF0) as u64;
        RP1_BAR0_CPU_BASE.store(bar0_clean, Ordering::SeqCst);

        // Compute CPU-visible mailbox base and register it with mailbox driver.
        // PCIe: BAR1 window maps 0xC000_0000 → 0x6000_0000_00
        // BAR0 = 0xC040_0000 → CPU addr = 0x6000_0000_00 + (0xC040_0000 - 0xC000_0000)
        let bar0_pcie = bar0_clean;
        let cpu_for_bar0 = RP1_OUTBOUND_CPU_BASE + (bar0_pcie - RP1_BAR1_PCIE_BASE);
        let mailbox_cpu_addr = (cpu_for_bar0 as usize) + 0x4000;
        let mailbox_bus_addr = (bar0_pcie as usize) + 0x4000;

        crate::drivers::mailbox::init_mailbox(mailbox_cpu_addr, mailbox_bus_addr);
        crate::drivers::mailbox::debug_dump_runtime_base();
        crate::drivers::mailbox::smoke_status();
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

        let supports_64bit = size_hi == 0xFFFF_FFFF;

        let (lo, hi) = if supports_64bit {
            (
                ((RP1_BAR1_PCIE_BASE as u32) & 0xFFFF_FFF0) | 0xC,
                (RP1_BAR1_PCIE_BASE >> 32) as u32,
            )
        } else {
            crate::print_str("PCIe: RP1 uses 32-bit BAR\n");
            (
                ((RP1_BAR1_PCIE_BASE as u32) & 0xFFFF_FFF0) | 0x8,
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
