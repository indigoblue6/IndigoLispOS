// pcie.rs - BCM2712 PCIe controller driver for RP1 access

use core::ptr;

// BCM2712 PCIe base addresses
const PCIE_BASE: usize = 0x10_0012_0000;

// PCIe registers offsets
const PCIE_RC_CFG_VENDOR_VENDOR_SPECIFIC_REG1: usize = 0x0188;
// Outbound window registers (for CPU->PCIe address translation)
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT: usize = 0x4070;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI: usize = 0x4080;
const PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI: usize = 0x4084;
// Inbound window registers (for PCIe->CPU memory access, needed for DMA)
const PCIE_MISC_RC_BAR2_CONFIG_LO: usize = 0x4034;
const PCIE_MISC_RC_BAR2_CONFIG_HI: usize = 0x4038;
const PCIE_MISC_RC_BAR1_CONFIG_LO: usize = 0x402c;
const PCIE_MISC_RC_BAR1_CONFIG_HI: usize = 0x4030;
const PCIE_MISC_MISC_CTRL: usize = 0x4008;
const PCIE_MISC_PCIE_CTRL: usize = 0x4064;
const PCIE_MISC_PCIE_STATUS: usize = 0x4068;
const PCIE_INTR2_CPU_STATUS: usize = 0x4300;
const PCIE_EXT_CFG_INDEX: usize = 0x9000;
const PCIE_EXT_CFG_DATA: usize = 0x8000;

// RP1 device constants
const RP1_VENDOR_ID: u16 = 0x1de4;
const RP1_DEVICE_ID: u16 = 0x0001;

pub struct PcieController {
    base: usize,
}

impl PcieController {
    pub fn new() -> Self {
        PcieController {
            base: PCIE_BASE,
        }
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

    /// Initialize PCIe controller
    pub fn init(&mut self) -> Result<(), &'static str> {
        crate::print_str("PCIe: Initializing controller...\n");

        // Setup outbound memory window to map CPU address to PCIe bus address
        // We want: CPU 0x1f_0000_0000 -> PCIe bus 0x0_0000_0000
        // This allows CPU to access RP1 peripherals via the outbound window
        crate::print_str("PCIe: Setting up outbound window...\n");
        
        // Outbound window configuration (based on Linux bcm2712-pcie driver):
        // BASE_LIMIT register format: [31:20] = base[31:20], [19:8] = limit[31:20]
        // BASE_HI: base[63:32]
        // LIMIT_HI: limit[63:32]
        //
        // We want to map:
        //   CPU address range: 0x1f_0000_0000 to 0x1f_ffff_ffff (4GB)
        //   to PCIe bus range:  0x0_0000_0000 to 0x0_ffff_ffff
        
        let cpu_base = 0x1f_0000_0000u64;
        let cpu_limit = 0x1f_ffff_ffffu64;
        
        // BASE_LIMIT: combine base[31:20] and limit[31:20]
        // For 4GB range: base=0x000, limit=0xFFF
        let base_limit = 0x0FFF0000u32;  // base[31:20] = 0x000, limit[31:20] = 0xFFF
        
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT, base_limit);
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI, 0x0000001f);
        self.write_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI, 0x0000001f);
        
        // Read back to verify
        let base_limit_readback = self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_LIMIT);
        let base_hi_readback = self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_BASE_HI);
        let limit_hi_readback = self.read_reg(PCIE_MISC_CPU_2_PCIE_MEM_WIN0_LIMIT_HI);
        
        crate::print_str("PCIe: Window configured - BASE_LIMIT=0x");
        crate::print_hex(base_limit_readback as usize);
        crate::print_str(" BASE_HI=0x");
        crate::print_hex(base_hi_readback as usize);
        crate::print_str(" LIMIT_HI=0x");
        crate::print_hex(limit_hi_readback as usize);
        crate::print_str("\n");
        
        // Setup inbound window for DMA (RP1 -> CPU memory access)
        // This allows RP1 Ethernet DMA to access CPU memory
        // We want RP1 to see CPU memory starting from 0x0 up to 4GB
        // Map it identity: RP1 address 0x0 -> CPU physical 0x0
        crate::print_str("PCIe: Setting up inbound window for DMA...\n");
        
        // RC_BAR2 is used for inbound window (DMA access to CPU memory)
        // Set it to map 4GB of CPU memory starting at 0x0
        // LO: lower 32 bits, HI: upper 32 bits (both 0 for identity mapping)
        self.write_reg(PCIE_MISC_RC_BAR2_CONFIG_LO, 0x00000000);
        self.write_reg(PCIE_MISC_RC_BAR2_CONFIG_HI, 0x00000000);
        
        let bar2_lo = self.read_reg(PCIE_MISC_RC_BAR2_CONFIG_LO);
        let bar2_hi = self.read_reg(PCIE_MISC_RC_BAR2_CONFIG_HI);
        crate::print_str("PCIe: Inbound window (RC_BAR2) - LO=0x");
        crate::print_hex(bar2_lo as usize);
        crate::print_str(" HI=0x");
        crate::print_hex(bar2_hi as usize);
        crate::print_str("\n");

        // Wait for link up
        let mut timeout = 1000000;
        while timeout > 0 {
            let status = self.read_reg(PCIE_MISC_PCIE_STATUS);
            if (status & 0x30) == 0x30 {
                // PHY link up and DL active
                crate::print_str("PCIe: Link up\n");
                break;
            }
            timeout -= 1;
        }

        if timeout == 0 {
            crate::print_str("PCIe: Link timeout\n");
            return Err("PCIe link timeout");
        }

        Ok(())
    }

    /// Read PCI configuration space
    pub fn read_config(&mut self, bus: u8, dev: u8, func: u8, reg: u16) -> u32 {
        let address = ((bus as u32) << 20) | ((dev as u32) << 15) | 
                     ((func as u32) << 12) | (reg as u32);
        self.write_reg(PCIE_EXT_CFG_INDEX, address);
        self.read_reg(PCIE_EXT_CFG_DATA + ((reg & 0xfff) as usize))
    }

    /// Write PCI configuration space
    pub fn write_config(&mut self, bus: u8, dev: u8, func: u8, reg: u16, value: u32) {
        let address = ((bus as u32) << 20) | ((dev as u32) << 15) | 
                     ((func as u32) << 12) | (reg as u32);
        self.write_reg(PCIE_EXT_CFG_INDEX, address);
        self.write_reg(PCIE_EXT_CFG_DATA + ((reg & 0xfff) as usize), value);
    }

    /// Scan for RP1 device
    pub fn find_rp1(&mut self) -> Option<Rp1Device> {
        crate::print_str("PCIe: Scanning for RP1...\n");

        // Scan multiple buses and devices
        for bus in 0..2 {
            for dev in 0..32 {
                let vendor_device = self.read_config(bus, dev, 0, 0);
                
                // Skip if all FFs (no device) or all zeros
                if vendor_device == 0xFFFFFFFF || vendor_device == 0 {
                    continue;
                }
                
                let vendor = (vendor_device & 0xFFFF) as u16;
                let device = ((vendor_device >> 16) & 0xFFFF) as u16;

                crate::print_str("PCIe: Found device at bus=");
                crate::print_dec(bus as usize);
                crate::print_str(" dev=");
                crate::print_dec(dev as usize);
                crate::print_str(" VID:DID=");
                crate::print_hex(vendor as usize);
                crate::print_str(":");
                crate::print_hex(device as usize);
                crate::print_str("\n");

                if vendor == RP1_VENDOR_ID && device == RP1_DEVICE_ID {
                    crate::print_str("PCIe: RP1 matched!\n");

                    return Some(Rp1Device {
                        pcie: self,
                        bus,
                        dev,
                        func: 0,
                    });
                }
            }
        }

        crate::print_str("PCIe: RP1 not found\n");
        None
    }
}

pub struct Rp1Device<'a> {
    pcie: &'a mut PcieController,
    bus: u8,
    dev: u8,
    func: u8,
}

impl<'a> Rp1Device<'a> {
    /// Enable RP1 device
    pub fn enable(&mut self) -> Result<(), &'static str> {
        crate::print_str("RP1: Enabling device...\n");

        // Read current command register
        let cmd = self.pcie.read_config(self.bus, self.dev, self.func, 0x04);
        crate::print_str("RP1: Command before: 0x");
        crate::print_hex(cmd as usize);
        crate::print_str("\n");
        
        // Enable memory space and bus master
        self.pcie.write_config(self.bus, self.dev, self.func, 0x04, cmd | 0x06);

        // Configure BARs explicitly
        // BAR0: MSI (16KiB) - keep existing value
        // BAR1: Main peripherals - map to start of outbound window
        // BAR2: SRAM (64KiB) - keep existing value
        
        crate::print_str("RP1: Configuring BARs...\n");
        
        // Read BAR1 size by writing all 1s and reading back
        self.pcie.write_config(self.bus, self.dev, self.func, 0x14, 0xFFFFFFFF);
        let bar1_size_raw = self.pcie.read_config(self.bus, self.dev, self.func, 0x14);
        crate::print_str("RP1: BAR1 size raw: 0x");
        crate::print_hex(bar1_size_raw as usize);
        crate::print_str("\n");
        
        // Set BAR1 to base of RP1 peripheral space (0x0 in PCIe space, accessed via 0x1f00000000 from CPU)
        self.pcie.write_config(self.bus, self.dev, self.func, 0x14, 0x00000000);
        
        // Read back BARs
        crate::print_str("RP1: BARs after configuration:\n");
        for i in 0..3 {
            let bar = self.pcie.read_config(self.bus, self.dev, self.func, 0x10 + (i * 4));
            crate::print_str("  BAR");
            crate::print_dec(i as usize);
            crate::print_str(": 0x");
            crate::print_hex(bar as usize);
            crate::print_str("\n");
        }

        crate::print_str("RP1: Device enabled\n");
        Ok(())
    }

    /// Get BAR1 base address (main peripherals)
    pub fn get_bar1_base(&self) -> usize {
        // Use the same address scheme as GPIO which is working
        // GPIO is at 0x1f000d0000, so Ethernet should be at 0x1f001c0000
        0x1f00000000
    }
}

static mut PCIE_CONTROLLER: Option<PcieController> = None;

pub fn init_pcie() -> Result<(), &'static str> {
    unsafe {
        let mut pcie = PcieController::new();
        pcie.init()?;
        PCIE_CONTROLLER = Some(pcie);
    }
    Ok(())
}

pub fn get_pcie() -> Option<&'static mut PcieController> {
    unsafe { PCIE_CONTROLLER.as_mut() }
}
