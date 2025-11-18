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
const LOCAL_MAC: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
const LOCAL_IP: [u8; 4] = [192, 168, 10, 110];

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
                    handle_basic_packets(&rx_buffer);
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

fn handle_basic_packets(frame: &[u8]) {
    if frame.len() < 14 {
        return;
    }
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    match ethertype {
        0x0806 => handle_arp(frame),
        0x0800 => handle_ipv4(frame),
        _ => {}
    }
}

fn handle_arp(frame: &[u8]) {
    if frame.len() < 42 {
        return;
    }
    let op = u16::from_be_bytes([frame[20], frame[21]]);
    if op != 1 {
        return;
    }
    let target_ip = &frame[38..42];
    if target_ip != LOCAL_IP {
        return;
    }

    crate::print_str("[NET] ARP request for us\n");

    let mut reply = [0u8; 42];
    reply[0..6].copy_from_slice(&frame[6..12]);
    reply[6..12].copy_from_slice(&LOCAL_MAC);
    reply[12..14].copy_from_slice(&frame[12..14]);
    reply[14..18].copy_from_slice(&frame[14..18]);
    reply[18] = frame[18];
    reply[19] = frame[19];
    reply[20..22].copy_from_slice(&2u16.to_be_bytes());
    reply[22..28].copy_from_slice(&LOCAL_MAC);
    reply[28..32].copy_from_slice(&LOCAL_IP);
    reply[32..38].copy_from_slice(&frame[22..28]);
    reply[38..42].copy_from_slice(&frame[28..32]);

    send_frame(&reply);
}

fn handle_ipv4(frame: &[u8]) {
    if frame.len() < 34 {
        return;
    }
    let ihl = (frame[14] & 0x0F) as usize * 4;
    if frame.len() < 14 + ihl {
        return;
    }
    let dst_ip = &frame[30..34];
    if dst_ip != LOCAL_IP {
        return;
    }
    let protocol = frame[23];
    if protocol != 1 {
        return;
    }
    if frame.len() < 14 + ihl + 8 {
        return;
    }
    let icmp_offset = 14 + ihl;
    let icmp_len = frame.len() - icmp_offset;
    let icmp = &frame[icmp_offset..];
    if icmp[0] != 8 {
        return;
    }

    crate::print_str("[NET] ICMP echo request\n");

    let mut reply = frame.to_vec();
    reply[0..6].copy_from_slice(&frame[6..12]);
    reply[6..12].copy_from_slice(&LOCAL_MAC);
    reply[30..34].copy_from_slice(&frame[26..30]);
    reply[26..30].copy_from_slice(&LOCAL_IP);
    reply[20] = 0;
    reply[21] = 0;
    let hdr = &mut reply[14..14 + ihl];
    let checksum = ipv4_checksum(hdr);
    hdr[10] = (checksum >> 8) as u8;
    hdr[11] = checksum as u8;

    reply[icmp_offset] = 0;
    reply[icmp_offset + 1] = 0;
    reply[icmp_offset + 2] = 0;
    reply[icmp_offset + 3] = 0;
    let icmp_checksum = ipv4_checksum(&reply[icmp_offset..icmp_offset + icmp_len]);
    reply[icmp_offset + 2] = (icmp_checksum >> 8) as u8;
    reply[icmp_offset + 3] = icmp_checksum as u8;

    send_frame(&reply);
}

fn ipv4_checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(2);
    for chunk in &mut chunks {
        sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
    }
    if let Some(&last) = chunks.remainder().first() {
        sum += (last as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn send_frame(data: &[u8]) {
    if let Some(eth) = crate::drivers::rp1_ethernet::get_rp1_ethernet() {
        let _ = eth.send(data);
    }
}
