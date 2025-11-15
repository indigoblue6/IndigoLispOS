// rp1_ethernet.rs - RP1 Ethernet controller driver
//
// Raspberry Pi 5 Gigabit Ethernet (Cadence GEM/MACB) Controller
// Physical base address (as seen from ARM cores): 0x1F00100000
// This maps to the peripheral address space within the RP1 I/O controller chip.
// In Linux device tree: macb 1f00100000.ethernet
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
// │ 0x00.0000.0000 - 0x7F.FFFF.FFFF : PCIe Outbound direct (512GiB) │
// │ 0x80.0000.0000 - 0x8F.FFFF.FFFF : PCIe Outbound ATU (256GiB)    │
// │ 0xC0.2000.0000 - 0xC0.203F.FFFF : Shared SRAM (64kiB)           │
// │ 0xC0.4000.0000 - ...            : Peripherals (APB/AHB/AXI)     │
// └─────────────────────────────────────────────────────────────────┘
//
// When RP1 DMA masters access BCM2712 DRAM:
// - Use physical address >= 0x0000_8000_0000 (BCM2712 DRAM start)
// - RP1 sends this as PCIe Outbound transaction
// - BCM2712 Root Complex maps it to actual DRAM

use core::ptr;

// Physical base address as seen from ARM cores (for reference)
// Note: In practice, this is obtained via PCIe BAR mapping
const RP1_PHY_BASE: usize = 0x1F00100000;  // Direct peripheral access address

// RP1 CPU memory address mapping (via PCIe outbound window)
// 0x1f_0000_0000 (CPU addr) <-> 0x4000_0000 (RP1 proc addr)
const RP1_CPU_BASE: usize = 0x1f_0000_0000;

// BCM2712 DRAM Physical Address Map
// CRITICAL: DRAM does NOT start at 0x0! Lower 2GiB is MMIO.
const BCM2712_DRAM_BASE: usize = 0x0000_8000_0000;  // DRAM starts at 2GiB offset
const BCM2712_MMIO_SIZE: usize = 0x0000_8000_0000;  // First 2GiB is MMIO/PCIe/RP1

// RP1 Ethernet is accessed via PCIe at BAR1 + offset
// Ethernet/MACB offset in RP1: 0x00100000 (1MB from RP1 base)
// Linux device tree shows: macb@100000 under RP1
const RP1_ETH_OFFSET: usize = 0x00100000;

// RP1 Clock control registers (CLKGEN)
const RP1_CLKGEN_BASE_OFFSET: usize = 0x0001_8000;
const CLKGEN_CLK_ETH_CTRL: usize = 0x3c;
const CLKGEN_CLK_ETH_CTRL_ENABLE: u32 = 1 << 11;

// ETH_CFG register offsets (base: 0x40104000 in RP1 peripheral space = ETH base + 0x4000)
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

// ETH_CFG CONTROL register bits
const ETH_CFG_CONTROL_MEM_PD: u32 = 1 << 4;
const ETH_CFG_CONTROL_BUSERR_EN: u32 = 1 << 3;
const ETH_CFG_CONTROL_TSU_INC_CTRL_MASK: u32 = 0b11 << 1;
const ETH_CFG_CONTROL_TSU_MS: u32 = 1 << 0;

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

// Network control register bits
const GEM_ENABLE_TX: u32 = 1 << 9;
const GEM_ENABLE_RX: u32 = 1 << 2;
const GEM_MPE: u32 = 1 << 4; // Management port enable

// Network config register bits
const GEM_FD: u32 = 1 << 0; // Full duplex
const GEM_SPD: u32 = 1 << 10; // Speed (1=100Mbps)
const GEM_RXCSUM_EN: u32 = 1 << 24; // RX checksum offload

// DMA config
const GEM_DISC_WHEN_NO_AHB: u32 = 1 << 10;
const GEM_FBLDO_INCR4: u32 = 4 << 16;

#[repr(C, align(8))]
#[derive(Copy, Clone)]
pub struct RxDescriptor {
    addr: u32,
    status: u32,
}

#[repr(C, align(8))]
#[derive(Copy, Clone)]
pub struct TxDescriptor {
    addr: u32,
    status: u32,
}

impl RxDescriptor {
    const ADDR_WRAP: u32 = 1 << 1;
    const ADDR_OWNERSHIP: u32 = 1 << 0;
}

impl TxDescriptor {
    const STATUS_USED: u32 = 1 << 31;
    const STATUS_WRAP: u32 = 1 << 30;
    const STATUS_LAST: u32 = 1 << 15;
}

pub struct Rp1Ethernet {
    base: usize,
    eth_cfg_base: usize,
    rx_descriptors: [RxDescriptor; 4],
    tx_descriptors: [TxDescriptor; 4],
    rx_buffers: [[u8; 1536]; 4],
    tx_buffers: [[u8; 1536]; 4],
    rx_index: usize,
    tx_index: usize,
}

impl Rp1Ethernet {
    pub fn new(rp1_base: usize) -> Self {
        let eth_base = rp1_base + RP1_ETH_OFFSET;
        Rp1Ethernet {
            base: eth_base,
            eth_cfg_base: eth_base + ETH_CFG_OFFSET,
            rx_descriptors: [RxDescriptor { addr: 0, status: 0 }; 4],
            tx_descriptors: [TxDescriptor { addr: 0, status: 0 }; 4],
            rx_buffers: [[0u8; 1536]; 4],
            tx_buffers: [[0u8; 1536]; 4],
            rx_index: 0,
            tx_index: 0,
        }
    }

    /// Convert CPU virtual address to RP1 DMA physical address
    /// 
    /// RP1 bus masters (DMA, Ethernet, etc.) must use BCM2712 physical addresses
    /// when accessing DRAM via PCIe Outbound transactions.
    /// 
    /// For bare-metal with identity mapping: virt_addr == phys_addr
    /// But we must ensure phys_addr >= BCM2712_DRAM_BASE (0x8000_0000)
    fn cpu_to_dma_addr(&self, cpu_addr: usize) -> u32 {
        // In bare-metal mode, we assume virtual == physical (identity mapped)
        let phys_addr = cpu_addr;
        
        // Sanity check: DRAM must be >= 0x8000_0000 on BCM2712
        if phys_addr < BCM2712_DRAM_BASE {
            // This is in MMIO region, not DRAM - likely a bug!
            // For now, we'll allow it but print a warning
            // In production, this should panic or return an error
        }
        
        // RP1 DMA sees the same physical address space as ARM cores
        // (PCIe Outbound direct-mapped space handles the translation)
        phys_addr as u32
    }

    fn read_reg(&self, offset: usize) -> u32 {
        unsafe {
            ptr::read_volatile((self.base + offset) as *const u32)
        }
    }

    fn write_reg(&self, offset: usize, value: u32) {
        unsafe {
            ptr::write_volatile((self.base + offset) as *mut u32, value);
        }
    }

    // ETH_CFG register access methods
    fn read_eth_cfg(&self, offset: usize) -> u32 {
        unsafe {
            ptr::read_volatile((self.eth_cfg_base + offset) as *const u32)
        }
    }

    fn write_eth_cfg(&self, offset: usize, value: u32) {
        unsafe {
            ptr::write_volatile((self.eth_cfg_base + offset) as *mut u32, value);
        }
    }

    /// Get TSU (Time Stamp Unit) timer count value (94-bit counter)
    pub fn get_tsu_timer(&self) -> u128 {
        let cnt0 = self.read_eth_cfg(ETH_CFG_TSU_TIMER_CNT0);
        let cnt1 = self.read_eth_cfg(ETH_CFG_TSU_TIMER_CNT1);
        let cnt2 = self.read_eth_cfg(ETH_CFG_TSU_TIMER_CNT2) & 0x3FFFFFFF; // Only 30 bits
        
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
        
        // Clear speed override bits
        clkgen &= !ETH_CFG_CLKGEN_SPEED_OVERRIDE_MASK;
        clkgen |= speed & ETH_CFG_CLKGEN_SPEED_OVERRIDE_MASK;
        
        // Enable/disable
        if enable {
            clkgen |= ETH_CFG_CLKGEN_ENABLE;
        } else {
            clkgen &= !ETH_CFG_CLKGEN_ENABLE;
        }
        
        self.write_eth_cfg(ETH_CFG_CLKGEN, clkgen);
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
        if link_up { crate::print_str("UP"); } else { crate::print_str("DOWN"); }
        crate::print_str(" Speed=");
        match speed {
            0 => crate::print_str("10M"),
            1 => crate::print_str("100M"),
            2 => crate::print_str("1G"),
            _ => crate::print_str("?"),
        }
        crate::print_str(" Duplex=");
        if duplex { crate::print_str("FULL"); } else { crate::print_str("HALF"); }
        crate::print_str("\n");
        
        // First, enable Ethernet clock in RP1 CLKGEN
        let rp1_base = self.base - RP1_ETH_OFFSET;
        let clkgen_base = rp1_base + RP1_CLKGEN_BASE_OFFSET;
        let clk_eth_ctrl_addr = (clkgen_base + CLKGEN_CLK_ETH_CTRL) as *mut u32;
        
        crate::print_str("RP1: Enabling Ethernet clock at 0x");
        crate::print_hex(clk_eth_ctrl_addr as usize);
        crate::print_str("\n");
        
        // Read current clock control value
        let clk_ctrl = unsafe { core::ptr::read_volatile(clk_eth_ctrl_addr) };
        crate::print_str("RP1: Clock control before: 0x");
        crate::print_hex(clk_ctrl as usize);
        crate::print_str("\n");
        
        // Try different approaches to enable the clock
        // Method 1: Just set enable bit
        unsafe {
            core::ptr::write_volatile(clk_eth_ctrl_addr, CLKGEN_CLK_ETH_CTRL_ENABLE);
        }
        
        // Read back to verify
        let clk_ctrl_after = unsafe { core::ptr::read_volatile(clk_eth_ctrl_addr) };
        crate::print_str("RP1: Clock control after enable: 0x");
        crate::print_hex(clk_ctrl_after as usize);
        crate::print_str("\n");
        
        // If that didn't work, try setting more bits (from Linux driver pattern)
        if clk_ctrl_after == 0 {
            crate::print_str("RP1: Clock still 0, trying alternative method...\n");
            unsafe {
                // Try a full clock setup (enable + other control bits)
                core::ptr::write_volatile(clk_eth_ctrl_addr, 0x00000820); // Enable with divider
            }
            let clk_ctrl_alt = unsafe { core::ptr::read_volatile(clk_eth_ctrl_addr) };
            crate::print_str("RP1: Clock control after alt: 0x");
            crate::print_hex(clk_ctrl_alt as usize);
            crate::print_str("\n");
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
            unsafe { core::arch::asm!("nop"); }
        }
        
        // Now try to read chip ID
        crate::print_str("RP1: Reading chip ID at 0x");
        crate::print_hex(rp1_base);
        crate::print_str("\n");
        
        let chip_id = unsafe {
            core::ptr::read_volatile(rp1_base as *const u32)
        };
        crate::print_str("RP1: Chip ID: 0x");
        crate::print_hex(chip_id as usize);
        crate::print_str("\n");
        
        // Debug: Try reading various offsets to find the correct registers
        crate::print_str("RP1 Ethernet: Probing register space...\n");
        
        // Try different base addresses - maybe Ethernet is not at 0x100000
        // Let's try the addresses from RP1 datasheet
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
        // BAR0 was 0x410000 in the PCIe scan
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

        // Disable TX and RX
        self.write_reg(GEM_NWCTRL, 0);

        // Setup descriptors
        for i in 0..4 {
            // RX descriptors
            // CRITICAL: RP1 DMA masters must use BCM2712 DRAM physical addresses
            // These addresses must be >= 0x8000_0000 (BCM2712_DRAM_BASE)
            let rx_buf_ptr = self.rx_buffers[i].as_ptr();
            let rx_cpu_addr = rx_buf_ptr as usize;
            let rx_dma_addr = self.cpu_to_dma_addr(rx_cpu_addr);
            
            crate::print_str("[ETH] RX desc[");
            crate::print_dec(i);
            crate::print_str("] CPU addr=0x");
            crate::print_hex(rx_cpu_addr);
            crate::print_str(" -> DMA addr=0x");
            crate::print_hex(rx_dma_addr as usize);
            
            // Warn if address is in MMIO region (< 0x8000_0000)
            if rx_cpu_addr < BCM2712_DRAM_BASE {
                crate::print_str(" [WARN: MMIO region!]");
            }
            crate::print_str("\n");
            
            // Ownership bit = 0 means DMA controller owns it (ready to receive)
            self.rx_descriptors[i].addr = rx_dma_addr & !RxDescriptor::ADDR_OWNERSHIP;
            self.rx_descriptors[i].status = 0;
            
            // Mark last descriptor with wrap bit
            if i == 3 {
                self.rx_descriptors[i].addr |= RxDescriptor::ADDR_WRAP;
            }

            // TX descriptors
            let tx_buf_ptr = self.tx_buffers[i].as_ptr();
            let tx_cpu_addr = tx_buf_ptr as usize;
            let tx_dma_addr = self.cpu_to_dma_addr(tx_cpu_addr);
            
            crate::print_str("[ETH] TX desc[");
            crate::print_dec(i);
            crate::print_str("] CPU addr=0x");
            crate::print_hex(tx_cpu_addr);
            crate::print_str(" -> DMA addr=0x");
            crate::print_hex(tx_dma_addr as usize);
            
            if tx_cpu_addr < BCM2712_DRAM_BASE {
                crate::print_str(" [WARN: MMIO region!]");
            }
            crate::print_str("\n");
            
            self.tx_descriptors[i].addr = tx_dma_addr;
            self.tx_descriptors[i].status = TxDescriptor::STATUS_USED;
            
            if i == 3 {
                self.tx_descriptors[i].status |= TxDescriptor::STATUS_WRAP;
            }
        }

        // Set descriptor queue base addresses
        // CRITICAL: Descriptor arrays themselves must also be in DRAM (>= 0x8000_0000)
        let rxqbase_cpu = self.rx_descriptors.as_ptr() as usize;
        let txqbase_cpu = self.tx_descriptors.as_ptr() as usize;
        let rxqbase_dma = self.cpu_to_dma_addr(rxqbase_cpu);
        let txqbase_dma = self.cpu_to_dma_addr(txqbase_cpu);
        
        crate::print_str("RP1 Ethernet: Setting descriptor queues:\n");
        crate::print_str("  RXQBASE: CPU=0x");
        crate::print_hex(rxqbase_cpu);
        crate::print_str(" DMA=0x");
        crate::print_hex(rxqbase_dma as usize);
        if rxqbase_cpu < BCM2712_DRAM_BASE {
            crate::print_str(" [WARN: MMIO!]");
        }
        crate::print_str("\n  TXQBASE: CPU=0x");
        crate::print_hex(txqbase_cpu);
        crate::print_str(" DMA=0x");
        crate::print_hex(txqbase_dma as usize);
        if txqbase_cpu < BCM2712_DRAM_BASE {
            crate::print_str(" [WARN: MMIO!]");
        }
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
        
        self.write_reg(GEM_RXQBASE, rxqbase_dma);
        self.write_reg(GEM_TXQBASE, txqbase_dma);

        // Set MAC address
        let mac_lo = u32::from_le_bytes([mac[0], mac[1], mac[2], mac[3]]);
        let mac_hi = u16::from_le_bytes([mac[4], mac[5]]) as u32;
        self.write_reg(GEM_SPADDR1LO, mac_lo);
        self.write_reg(GEM_SPADDR1HI, mac_hi);

        // Configure network
        let nwcfg = GEM_FD | GEM_SPD | GEM_RXCSUM_EN;
        self.write_reg(GEM_NWCFG, nwcfg);

        // Configure DMA
        let dmacfg = GEM_DISC_WHEN_NO_AHB | GEM_FBLDO_INCR4;
        self.write_reg(GEM_DMACFG, dmacfg);

        // Enable TX and RX
        let nwctrl = GEM_ENABLE_TX | GEM_ENABLE_RX | GEM_MPE;
        self.write_reg(GEM_NWCTRL, nwctrl);

        // Debug: Read back all important registers
        let nwctrl_rb = self.read_reg(GEM_NWCTRL);
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
        crate::print_hex(self.rx_buffers[0].as_ptr() as usize);
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
        if data.len() > 1536 {
            return Err("Packet too large");
        }

        let desc = &mut self.tx_descriptors[self.tx_index];
        
        // Wait for descriptor to be free
        if (desc.status & TxDescriptor::STATUS_USED) == 0 {
            return Err("TX descriptor busy");
        }

        crate::print_str("[ETH] TX sending ");
        crate::print_dec(data.len());
        crate::print_str(" bytes from desc[");
        crate::print_dec(self.tx_index);
        crate::print_str("]\n");

        // Copy data to buffer
        let buf = &mut self.tx_buffers[self.tx_index];
        buf[..data.len()].copy_from_slice(data);

        // Clean cache for TX buffer to ensure DMA sees the data
        unsafe {
            let buf_start = buf.as_ptr() as usize;
            let buf_end = buf_start + data.len();
            let mut addr = buf_start & !63;
            while addr < buf_end {
                core::arch::asm!(
                    "dc cvac, {0}",  // Clean by VA to PoC (write back)
                    in(reg) addr,
                    options(nostack)
                );
                addr += 64;
            }
            core::arch::asm!("dsb sy", options(nostack));
        }

        // Update descriptor
        desc.status = data.len() as u32 | TxDescriptor::STATUS_LAST;
        if self.tx_index == 3 {
            desc.status |= TxDescriptor::STATUS_WRAP;
        }

        self.tx_index = (self.tx_index + 1) % 4;

        // Trigger transmission
        self.write_reg(GEM_NWCTRL, self.read_reg(GEM_NWCTRL) | (1 << 9));

        Ok(())
    }

    pub fn recv(&mut self, buffer: &mut [u8]) -> Option<usize> {
        // Invalidate cache for descriptor to ensure we see DMA updates
        // RP1 DMA writes directly to memory, bypassing CPU cache
        unsafe {
            let desc_addr = &self.rx_descriptors[self.rx_index] as *const _ as usize;
            // DC CIVAC (Clean and Invalidate by VA to PoC)
            core::arch::asm!(
                "dc civac, {0}",
                in(reg) desc_addr,
                options(nostack)
            );
            // DSB to ensure cache operation completes
            core::arch::asm!("dsb sy", options(nostack));
        }
        
        let desc = &mut self.rx_descriptors[self.rx_index];
        
        // Check if frame is available (ownership bit = 1 means CPU owns it)
        if (desc.addr & RxDescriptor::ADDR_OWNERSHIP) == 0 {
            return None;  // Still owned by DMA, no data yet
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
        if frame_len == 0 || frame_len > buffer.len() || frame_len > 1536 {
            // Release descriptor
            desc.addr &= !RxDescriptor::ADDR_OWNERSHIP;
            self.rx_index = (self.rx_index + 1) % 4;
            return None;
        }

        // Invalidate cache for the receive buffer before reading
        unsafe {
            let buf_start = self.rx_buffers[self.rx_index].as_ptr() as usize;
            let buf_end = buf_start + frame_len;
            // Invalidate cache lines covering the buffer
            let mut addr = buf_start & !63; // Align to 64-byte cache line
            while addr < buf_end {
                core::arch::asm!(
                    "dc civac, {0}",
                    in(reg) addr,
                    options(nostack)
                );
                addr += 64;
            }
            core::arch::asm!("dsb sy", options(nostack));
        }

        // Copy data from buffer (safe because we checked frame_len <= 1536)
        buffer[..frame_len].copy_from_slice(&self.rx_buffers[self.rx_index][..frame_len]);

        // Release descriptor back to DMA (clear ownership bit = DMA owns it)
        desc.addr &= !RxDescriptor::ADDR_OWNERSHIP;
        desc.status = 0;

        self.rx_index = (self.rx_index + 1) % 4;

        Some(frame_len)
    }
}

static mut RP1_ETHERNET: Option<Rp1Ethernet> = None;

pub fn init_rp1_ethernet(rp1_base: usize, mac: [u8; 6]) -> Result<(), &'static str> {
    unsafe {
        let mut eth = Rp1Ethernet::new(rp1_base);
        eth.init(mac)?;
        RP1_ETHERNET = Some(eth);
    }
    Ok(())
}

pub fn get_rp1_ethernet() -> Option<&'static mut Rp1Ethernet> {
    unsafe { RP1_ETHERNET.as_mut() }
}
