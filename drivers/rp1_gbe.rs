// drivers/rp1_gbe.rs
//
// Raspberry Pi 5 RP1 GBE Driver (MACB-compatible)
// Full implementation: DMA rings, MDIO, MAC init, RX/TX enable
//

#![allow(dead_code)]

use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};

use crate::print_str;
use crate::print_hex;

// ---------------------------------------------
// RP1 GBE base addresses
// ---------------------------------------------
pub const RP1_GBE_BASE: usize = 0xC010_0000;
pub const RP1_ETH_CFG:  usize = 0xC010_4000;

// ---------------------------------------------
// MACB Registers (offsets)
// ---------------------------------------------
const NCFGR: usize = 0x004;
const NSR:  usize = 0x008;
const TSR:  usize = 0x014;
const RBQP: usize = 0x018;
const TBQP: usize = 0x01C;
const ISR:  usize = 0x024;
const IER:  usize = 0x028;
const IDR:  usize = 0x02C;
const IMR:  usize = 0x030;

const MAN:  usize = 0x034;

const SA1L: usize = 0x098;
const SA1H: usize = 0x09C;

// ---------------------------------------------
// MDIO constants
// ---------------------------------------------
const MAN_SOF: u32 = 0x4002_0000;
const MAN_RW_READ:  u32 = 0x2000_0000;
const MAN_RW_WRITE: u32 = 0x1000_0000;
const MAN_PHYA_SHIFT: u32 = 23;
const MAN_REGA_SHIFT: u32 = 18;
const MAN_DATA_MASK:  u32 = 0x0000_FFFF;

// ---------------------------------------------
// DMA Ring config
// ---------------------------------------------
const RX_DESC_COUNT: usize = 32;
const TX_DESC_COUNT: usize = 32;
const RX_BUF_SIZE:    usize = 1536;

// Descriptor ownership bits
const RX_OWNERSHIP: u32 = 1 << 0;
const RX_WRAP:       u32 = 1 << 1;
const TX_USED:       u32 = 1 << 31;
const TX_WRAP:       u32 = 1 << 30;

// GEM_NCFGR bit helpers (mirrors Linux driver mapping)
const NCFGR_SPD:      u32 = 1 << 0;
const NCFGR_FDX:      u32 = 1 << 1;
const NCFGR_GBEN:     u32 = 1 << 10;
const NCFGR_CLK_DIV64:u32 = 4 << 18; // MDC clock divider (must keep <= 2.5MHz)
const NCFGR_DBW_64:   u32 = 1 << 21; // 64-bit bus width
const NCFGR_RXCOEN:   u32 = 1 << 24; // enable RX checksum offload path early

// ---------------------------------------------
// DMA descriptor structures
// ---------------------------------------------
#[repr(C)]
#[derive(Copy, Clone)]
pub struct RxDesc {
    pub addr: u32,
    pub status: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct TxDesc {
    pub addr: u32,
    pub status: u32,
}

// ---------------------------------------------
// Static global DMA rings
// ---------------------------------------------
#[link_section = ".dram"]
static mut RX_DESC: [RxDesc; RX_DESC_COUNT] = [RxDesc { addr: 0, status: RX_OWNERSHIP }; RX_DESC_COUNT];

#[link_section = ".dram"]
static mut TX_DESC: [TxDesc; TX_DESC_COUNT] = [TxDesc { addr: 0, status: TX_USED }; TX_DESC_COUNT];

#[link_section = ".dram"]
static mut RX_BUFFERS: [[u8; RX_BUF_SIZE]; RX_DESC_COUNT] = [[0; RX_BUF_SIZE]; RX_DESC_COUNT];

static mut TX_HEAD: usize = 0;
static mut RX_HEAD: usize = 0;

// ---------------------------------------------
// Safe MMIO access
// ---------------------------------------------
unsafe fn read_reg(off: usize) -> u32 {
    ptr::read_volatile((RP1_GBE_BASE + off) as *const u32)
}

unsafe fn write_reg(off: usize, val: u32) {
    ptr::write_volatile((RP1_GBE_BASE + off) as *mut u32, val);
}

// =============================================
// MDIO read/write
// =============================================
pub unsafe fn mdio_read(phy: u32, reg: u32) -> u16 {
    let cmd = MAN_SOF
        | MAN_RW_READ
        | (phy << MAN_PHYA_SHIFT)
        | (reg << MAN_REGA_SHIFT)
        | 2; // code field

    write_reg(MAN, cmd);

    compiler_fence(Ordering::SeqCst);

    let mut val;
    loop {
        val = read_reg(MAN);
        if val & MAN_RW_READ == 0 {
            break;
        }
    }

    (val & MAN_DATA_MASK) as u16
}

pub unsafe fn mdio_write(phy: u32, reg: u32, value: u16) {
    let cmd = MAN_SOF
        | MAN_RW_WRITE
        | (phy << MAN_PHYA_SHIFT)
        | (reg << MAN_REGA_SHIFT)
        | (value as u32 & MAN_DATA_MASK)
        | 2;

    write_reg(MAN, cmd);

    compiler_fence(Ordering::SeqCst);

    loop {
        let v = read_reg(MAN);
        if v & MAN_RW_WRITE == 0 {
            break;
        }
    }
}

// Note: PHY bring-up is implemented in `drivers::rp1_phy`.

// =============================================
// DMA ring init
// =============================================
unsafe fn init_dma_rings() {
    print_str("[MACB] Init DMA rings\n");

    // RX descriptors
    for i in 0..RX_DESC_COUNT {
        RX_DESC[i].addr = (&RX_BUFFERS[i][0] as *const u8 as u32) & !0x3;
        RX_DESC[i].status = RX_OWNERSHIP;
        if i == RX_DESC_COUNT - 1 {
            RX_DESC[i].addr |= RX_WRAP;
        }
    }

    // TX descriptors
    for i in 0..TX_DESC_COUNT {
        TX_DESC[i].addr = 0;
        TX_DESC[i].status = TX_USED;
        if i == TX_DESC_COUNT - 1 {
            TX_DESC[i].status |= TX_WRAP;
        }
    }

    TX_HEAD = 0;
    RX_HEAD = 0;

    // Tell MACB where the rings are
    write_reg(RBQP, (&RX_DESC as *const _ as u32));
    write_reg(TBQP, (&TX_DESC as *const _ as u32));
}

// =============================================
// MAC init
// =============================================
pub unsafe fn gbe_init(mac: [u8; 6]) {
    print_str("[MACB] RP1 GBE init start\n");

    // 1) Try to bring up PHY via shared PHY helper before enabling DMA
    unsafe {
        match crate::drivers::rp1_phy::Rp1Phy::new() {
            Err(e) => {
                print_str("[PHY] Rp1Phy::new() failed\n");
            }
            Ok(phy) => {
                print_str("[PHY] starting bring-up\n");
                match phy.bring_up() {
                    Ok(_) => {
                        print_str("[PHY] bring-up succeeded\n");
                    }
                    Err(_) => {
                        print_str("[PHY] bring-up failed\n");
                    }
                }
            }
        }
    }

    init_dma_rings();

    // Configure MAC
    let mut ncfgr = NCFGR_FDX | NCFGR_SPD | NCFGR_GBEN | NCFGR_DBW_64 | NCFGR_RXCOEN;
    // Keep MDC below 2.5MHz (RP1 feeds MAC with >100MHz), so divide down.
    ncfgr |= NCFGR_CLK_DIV64;
    write_reg(NCFGR, ncfgr);
    print_str("[MACB] NCFGR configured (SPD|FDX|GBEN|CLK_DIV64|DBW64|RXCOEN)\n");

    // Set MAC address
    let low = (mac[3] as u32) << 24
        | (mac[2] as u32) << 16
        | (mac[1] as u32) << 8
        | (mac[0] as u32);
    let high = (mac[5] as u32) << 8 | (mac[4] as u32);

    write_reg(SA1L, low);
    write_reg(SA1H, high);

    // Enable interrupts (RX + errors)
    write_reg(IER, 0xFFFFFFFF);

    print_str("[MACB] DMA + MAC configured\n");

    // Start RX/TX
    write_reg(NSR, 0x01); // clear status
    write_reg(TSR, 0xFFFFFFFF);

    print_str("[MACB] RP1 GBE init done\n");
}

// =============================================
// Send frame
// =============================================
pub unsafe fn send_frame(data: &[u8]) {
    let head = TX_HEAD;

    let desc = &mut TX_DESC[head];

    // Wait until descriptor is free
    while desc.status & TX_USED == 0 {}

    desc.addr = data.as_ptr() as u32;
    desc.status = (data.len() as u32) & 0x1FFF;

    compiler_fence(Ordering::SeqCst);

    TX_HEAD = (head + 1) % TX_DESC_COUNT;

    // Kick TX
    write_reg(TSR, 0xFFFFFFFF);
}

// =============================================
// Receive frame
// =============================================
pub unsafe fn poll_rx() -> Option<&'static [u8]> {
    let head = RX_HEAD;
    let desc = &mut RX_DESC[head];

    if desc.status & RX_OWNERSHIP != 0 {
        return None;
    }

    let len = (desc.status >> 16) & 0x1FFF;

    let buf = &RX_BUFFERS[head][0..len as usize];

    // Return ownership
    desc.status = RX_OWNERSHIP;

    RX_HEAD = (head + 1) % RX_DESC_COUNT;

    Some(buf)
}
