// ethernet.rs - Ethernet driver wrapper (now uses RP1 via PCIe)

pub use crate::drivers::rp1_ethernet::get_rp1_ethernet as get_ethernet;

#[derive(Clone, Copy)]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    pub fn new(bytes: [u8; 6]) -> Self {
        MacAddress(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }
}

pub fn init_ethernet(_mac_addr: MacAddress) {
    // Initialization is now handled via PCIe + RP1
    crate::print_str("Legacy ethernet init bypassed - using RP1\n");
}
