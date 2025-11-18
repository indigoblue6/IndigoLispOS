// drivers/rp1_eth.rs
//
// Wrapper around rp1_gbe.rs
//

use crate::drivers::rp1_gbe;

static MAC_ADDR: [u8; 6] = [0x02, 0x12, 0x34, 0x56, 0x78, 0x9A];

pub fn init() {
    unsafe {
        rp1_gbe::gbe_init(MAC_ADDR);
    }
}

pub fn poll() {
    unsafe {
        if let Some(frame) = rp1_gbe::poll_rx() {
            crate::print_str("[ETH] RX frame: ");
            crate::print_hex(frame.len());
            crate::print_str(" bytes\n");
        }
    }
}
