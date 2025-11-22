// drivers/rp1_fw_mdio.rs
//
// RP1 firmware MDIO bridge (IndigoLispOS version)
// Uses your mailbox_rp1.rs + print_str/print_hex + TIMER

use crate::print_str;
use crate::print_hex;
use crate::drivers::timer::TIMER;
use crate::drivers::mailbox_rp1::{mailbox_write, mailbox_read, MAILBOX_RP1_CHANNEL};

pub const RP1_MDIO_PHY_ADDR: u32 = 8;         // RP1 PHY address is 8 (hardware-wired, same as Linux)

// RP1 Firmware MDIO message IDs (from Linux rp1-phy driver)
// Message format: 0x00030010 + sub-command
const FW_MSG_MDIO_BASE: u32 = 0x0003_0010;
const FW_CMD_READ:  u32 = 0;  // Sub-command for read
const FW_CMD_WRITE: u32 = 1;  // Sub-command for write
const FW_CMD_GET_LINK: u32 = 2;  // Sub-command for get link
const FW_CMD_SET_LINK: u32 = 3;  // Sub-command for set link

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rp1MdioMsg {
    pub cmd: u32,     // 0=read, 1=write, 2=get_link, 3=set_link
    pub phy_id: u32,
    pub regnum: u32,
    pub val: u32,
}

impl Rp1MdioMsg {
    pub fn new(cmd: u32, phy_id: u32, regnum: u32, val: u32) -> Self {
        Self { cmd, phy_id, regnum, val }
    }
}

/// Perform a mailbox MDIO transaction.
/// Pointer can be passed directly because CPU and RP1 share the same view.
unsafe fn rp1_fw_mailbox_op(msg: &mut Rp1MdioMsg) -> bool {
    let ptr = msg as *mut Rp1MdioMsg as u64;

    // Send pointer to firmware using RP1 firmware channel (1)
    if let Err(_) = mailbox_write(MAILBOX_RP1_CHANNEL, ptr as u32) {
        print_str("[RP1 FW] mailbox_write failed\n");
        return false;
    }

    // Wait for response
    for _ in 0..200 {
        match mailbox_read(MAILBOX_RP1_CHANNEL) {
            Ok(resp_ptr) => {
                let resp_msg = (resp_ptr as *const Rp1MdioMsg).as_ref().unwrap();

                msg.cmd    = resp_msg.cmd;
                msg.phy_id = resp_msg.phy_id;
                msg.regnum = resp_msg.regnum;
                msg.val    = resp_msg.val;

                return true;
            }
            Err(_) => { /* retry */ }
        }

        TIMER.delay_us(5);
    }

    print_str("[RP1 FW] mailbox timeout\n");
    false
}

/// Public API: MDIO READ
pub fn rp1_mdio_read(phy: u32, reg: u32) -> Option<u32> {
    let mut msg = Rp1MdioMsg::new(FW_CMD_READ, phy, reg, 0);

    unsafe {
        if !rp1_fw_mailbox_op(&mut msg) {
            print_str("[MDIO] read failed reg=0x");
            print_hex(reg as usize);
            print_str("\n");
            return None;
        }
    }

    print_str("[MDIO] READ reg=0x");
    print_hex(reg as usize);
    print_str(" -> 0x");
    print_hex(msg.val as usize);
    print_str("\n");

    Some(msg.val)
}

/// Public API: MDIO WRITE
pub fn rp1_mdio_write(phy: u32, reg: u32, val: u32) -> bool {
    let mut msg = Rp1MdioMsg::new(FW_CMD_WRITE, phy, reg, val);

    unsafe {
        let ok = rp1_fw_mailbox_op(&mut msg);
        if ok {
            print_str("[MDIO] WRITE reg=0x");
            print_hex(reg as usize);
            print_str(" val=0x");
            print_hex(val as usize);
            print_str("\n");
        } else {
            print_str("[MDIO] WRITE FAIL reg=0x");
            print_hex(reg as usize);
            print_str("\n");
        }
        ok
    }
}

/// Optional: get link state
pub fn rp1_mdio_get_link() -> Option<u32> {
    let mut msg = Rp1MdioMsg::new(FW_CMD_GET_LINK, 0, 0, 0);

    unsafe {
        if !rp1_fw_mailbox_op(&mut msg) {
            return None;
        }
    }
    Some(msg.val)
}

/// Optional: force link state
pub fn rp1_mdio_set_link(val: u32) -> bool {
    let mut msg = Rp1MdioMsg::new(FW_CMD_SET_LINK, 0, 0, val);
    unsafe { rp1_fw_mailbox_op(&mut msg) }
}

/// MACB compatibility hooks (rp1_gbe.rs で使用)
pub fn macb_rp1_mdio_read(_base: u64, reg: u32) -> u32 {
    rp1_mdio_read(RP1_MDIO_PHY_ADDR, reg).unwrap_or(0xFFFF_FFFF)
}

pub fn macb_rp1_mdio_write(_base: u64, reg: u32, val: u32) {
    rp1_mdio_write(RP1_MDIO_PHY_ADDR, reg, val);
}
