// network.rs - Network stack integration with smoltcp

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;
use smoltcp::iface::{Config, Interface, SocketSet, SocketHandle, SocketStorage};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::{tcp, udp};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

const MTU: usize = 1500;

/// Wrapper for Ethernet driver to implement smoltcp Device trait
pub struct NetworkDevice;

impl Device for NetworkDevice {
    type RxToken<'a> = RxTokenImpl;
    type TxToken<'a> = TxTokenImpl;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut buffer = [0u8; MTU];
        
        if let Some(eth) = crate::drivers::rp1_ethernet::get_rp1_ethernet() {
            if let Some(len) = eth.recv(&mut buffer) {
                if len > 0 {
                    // Check EtherType to identify packet type
                    let ethertype = if len >= 14 {
                        ((buffer[12] as u16) << 8) | (buffer[13] as u16)
                    } else {
                        0
                    };
                    
                    crate::print_str("[NET] RX ");
                    crate::print_dec(len);
                    crate::print_str(" bytes, type=0x");
                    crate::print_hex(ethertype as usize);
                    crate::print_str("\n");
                    
                    let rx_buffer = buffer[..len].to_vec();
                    return Some((
                        RxTokenImpl { buffer: rx_buffer },
                        TxTokenImpl,
                    ));
                }
            }
        }
        None
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(TxTokenImpl)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = MTU;
        caps.medium = Medium::Ethernet;
        caps
    }
}

pub struct RxTokenImpl {
    buffer: Vec<u8>,
}

impl RxToken for RxTokenImpl {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        f(&mut self.buffer)
    }
}

pub struct TxTokenImpl;

impl TxToken for TxTokenImpl {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buffer = vec![0u8; len];
        let result = f(&mut buffer);
        
        crate::print_str("[NET] TX ");
        crate::print_dec(len);
        crate::print_str(" bytes\n");
        
        if let Some(eth) = crate::drivers::rp1_ethernet::get_rp1_ethernet() {
            let _ = eth.send(&buffer);
        }
        
        result
    }
}

/// Network stack manager
pub struct NetworkStack {
    iface: Interface,
    sockets: SocketSet<'static>,
    device: NetworkDevice,
}

impl NetworkStack {
    pub fn new(
        mac: [u8; 6],
        ip: Ipv4Address,
        gateway: Ipv4Address,
        socket_storage: &'static mut [SocketStorage<'static>],
    ) -> Self {
        let mut device = NetworkDevice;
        
        let ethernet_addr = EthernetAddress(mac);
        let ip_addrs = [IpCidr::new(IpAddress::v4(ip.0[0], ip.0[1], ip.0[2], ip.0[3]), 24)];
        
        let mut config = Config::new(ethernet_addr.into());
        config.random_seed = 0x12345678; // Should use actual random source
        
        let mut iface = Interface::new(config, &mut device, Instant::ZERO);
        iface.update_ip_addrs(|addrs| {
            addrs.push(ip_addrs[0]).unwrap();
        });
        
        iface.routes_mut().add_default_ipv4_route(gateway).unwrap();

        let sockets = SocketSet::new(&mut *socket_storage);

        NetworkStack {
            iface,
            sockets,
            device,
        }
    }

    /// Poll the network stack
    pub fn poll(&mut self, timestamp_ms: u64) {
        let timestamp = Instant::from_millis(timestamp_ms as i64);
        self.iface.poll(timestamp, &mut self.device, &mut self.sockets);
    }

    /// Add a UDP socket (simplified - uses static buffers)
    pub fn add_udp_socket(
        &mut self,
        local_port: u16,
        rx_meta: &'static mut [udp::PacketMetadata],
        rx_data: &'static mut [u8],
        tx_meta: &'static mut [udp::PacketMetadata],
        tx_data: &'static mut [u8],
    ) -> Result<SocketHandle, &'static str> {
        let udp_rx_buffer = udp::PacketBuffer::new(rx_meta, rx_data);
        let udp_tx_buffer = udp::PacketBuffer::new(tx_meta, tx_data);
        
        let mut udp_socket = udp::Socket::new(udp_rx_buffer, udp_tx_buffer);
        udp_socket.bind(local_port).map_err(|_| "Failed to bind UDP socket")?;
        
        let handle = self.sockets.add(udp_socket);
        Ok(handle)
    }

    /// Add a TCP socket (simplified - uses static buffers)
    pub fn add_tcp_socket(
        &mut self,
        rx_buffer: &'static mut [u8],
        tx_buffer: &'static mut [u8],
    ) -> Result<SocketHandle, &'static str> {
        let tcp_rx_buffer = tcp::SocketBuffer::new(rx_buffer);
        let tcp_tx_buffer = tcp::SocketBuffer::new(tx_buffer);
        
        let tcp_socket = tcp::Socket::new(tcp_rx_buffer, tcp_tx_buffer);
        
        let handle = self.sockets.add(tcp_socket);
        Ok(handle)
    }

    /// Get UDP socket
    pub fn get_udp_socket(&mut self, handle: SocketHandle) -> &mut udp::Socket<'static> {
        self.sockets.get_mut::<udp::Socket>(handle)
    }

    /// Get TCP socket
    pub fn get_tcp_socket(&mut self, handle: SocketHandle) -> &mut tcp::Socket<'static> {
        self.sockets.get_mut::<tcp::Socket>(handle)
    }
}

// Global network stack instance
static mut NETWORK_STACK: Option<NetworkStack> = None;
static mut SOCKET_STORAGE: [SocketStorage<'static>; 4] = [
    SocketStorage::EMPTY,
    SocketStorage::EMPTY,
    SocketStorage::EMPTY,
    SocketStorage::EMPTY,
];

pub fn init_network(mac: [u8; 6], ip: Ipv4Address, gateway: Ipv4Address) {
    unsafe {
        let storage = &mut SOCKET_STORAGE[..];
        NETWORK_STACK = Some(NetworkStack::new(mac, ip, gateway, storage));
    }
}

pub fn get_network_stack() -> Option<&'static mut NetworkStack> {
    unsafe { NETWORK_STACK.as_mut() }
}
