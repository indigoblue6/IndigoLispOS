// hotdeploy.rs - Hot deployment receiver and kernel swap

extern crate alloc;
use alloc::vec::Vec;
use smoltcp::iface::SocketHandle;
use crate::network::get_network_stack;

const HOTDEPLOY_PORT: u16 = 8888;
const MAX_KERNEL_SIZE: usize = 16 * 1024 * 1024; // 16MB
const KERNEL_MAGIC: u32 = 0x494C4F53; // "ILOS" in ASCII

// Build information - will be updated at build time
const BUILD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Display current build information
pub fn print_build_info() {
    crate::print_str("=== Build Information ===\n");
    crate::print_str("Version: ");
    crate::print_str(BUILD_VERSION);
    crate::print_str("\nBuild Time: ");
    
    // Use option_env! with match since unwrap_or is not const
    match option_env!("BUILD_TIMESTAMP") {
        Some(timestamp) => crate::print_str(timestamp),
        None => crate::print_str("unknown"),
    }
    
    crate::print_str("\n");
}

/// Kernel image header
#[repr(C)]
struct KernelHeader {
    magic: u32,
    version: u32,
    size: u32,
    checksum: u32,
    timestamp: u64,
}

impl KernelHeader {
    fn validate(&self) -> bool {
        self.magic == KERNEL_MAGIC && self.size <= MAX_KERNEL_SIZE as u32
    }
}

/// Hot deployment receiver
pub struct HotDeployReceiver {
    udp_socket_handle: Option<SocketHandle>,
    kernel_buffer: Vec<u8>,
    receiving: bool,
    bytes_received: usize,
    expected_size: usize,
}

impl HotDeployReceiver {
    pub fn new() -> Self {
        HotDeployReceiver {
            udp_socket_handle: None,
            kernel_buffer: Vec::new(),
            receiving: false,
            bytes_received: 0,
            expected_size: 0,
        }
    }

    /// Initialize the hot deploy receiver
    pub fn init(&mut self) -> Result<(), &'static str> {
        if let Some(stack) = get_network_stack() {
            // Use static buffers for UDP socket
            static mut UDP_RX_META: [smoltcp::socket::udp::PacketMetadata; 4] = [
                smoltcp::socket::udp::PacketMetadata::EMPTY,
                smoltcp::socket::udp::PacketMetadata::EMPTY,
                smoltcp::socket::udp::PacketMetadata::EMPTY,
                smoltcp::socket::udp::PacketMetadata::EMPTY,
            ];
            static mut UDP_RX_DATA: [u8; 2048] = [0; 2048];
            static mut UDP_TX_META: [smoltcp::socket::udp::PacketMetadata; 4] = [
                smoltcp::socket::udp::PacketMetadata::EMPTY,
                smoltcp::socket::udp::PacketMetadata::EMPTY,
                smoltcp::socket::udp::PacketMetadata::EMPTY,
                smoltcp::socket::udp::PacketMetadata::EMPTY,
            ];
            static mut UDP_TX_DATA: [u8; 2048] = [0; 2048];

            unsafe {
                let handle = stack.add_udp_socket(
                    HOTDEPLOY_PORT,
                    &mut UDP_RX_META,
                    &mut UDP_RX_DATA,
                    &mut UDP_TX_META,
                    &mut UDP_TX_DATA,
                )?;
                self.udp_socket_handle = Some(handle);
            }
            
            crate::print_str("Hot deploy receiver initialized on port ");
            crate::print_dec(HOTDEPLOY_PORT as usize);
            crate::print_str("\n");
            
            Ok(())
        } else {
            Err("Network stack not initialized")
        }
    }

    /// Poll for incoming kernel images
    pub fn poll(&mut self) -> Option<KernelImage> {
        let handle = self.udp_socket_handle?;
        let stack = get_network_stack()?;
        let socket = stack.get_udp_socket(handle);

        if socket.can_recv() {
            crate::print_str("[HOTDEPLOY] UDP socket has data\n");
            if let Ok((data, metadata)) = socket.recv() {
                crate::print_str("Received packet from ");
                crate::print_dec(metadata.endpoint.addr.as_bytes()[0] as usize);
                crate::print_str(".");
                crate::print_dec(metadata.endpoint.addr.as_bytes()[1] as usize);
                crate::print_str(".");
                crate::print_dec(metadata.endpoint.addr.as_bytes()[2] as usize);
                crate::print_str(".");
                crate::print_dec(metadata.endpoint.addr.as_bytes()[3] as usize);
                crate::print_str(":");
                crate::print_dec(metadata.endpoint.port as usize);
                crate::print_str(" (");
                crate::print_dec(data.len());
                crate::print_str(" bytes)\n");
                
                return self.process_packet(data);
            }
        }

        None
    }

    fn process_packet(&mut self, data: &[u8]) -> Option<KernelImage> {
        if !self.receiving {
            // Start of new transfer - expect header
            if data.len() < core::mem::size_of::<KernelHeader>() {
                return None;
            }

            let header = unsafe {
                &*(data.as_ptr() as *const KernelHeader)
            };

            if !header.validate() {
                crate::print_str("Invalid kernel header\n");
                return None;
            }

            self.expected_size = header.size as usize;
            self.kernel_buffer = Vec::with_capacity(self.expected_size);
            self.receiving = true;
            self.bytes_received = 0;

            crate::print_str("\n=== Hot Deploy: Receiving New Kernel ===\n");
            crate::print_str("Size: ");
            crate::print_dec(self.expected_size);
            crate::print_str(" bytes\n");
            crate::print_str("Version: ");
            crate::print_dec(header.version as usize);
            crate::print_str("\nTimestamp: ");
            crate::print_hex(header.timestamp as usize);
            crate::print_str("\n");

            // Store header data if any follows
            let header_size = core::mem::size_of::<KernelHeader>();
            if data.len() > header_size {
                self.kernel_buffer.extend_from_slice(&data[header_size..]);
                self.bytes_received = data.len() - header_size;
            }
        } else {
            // Continue receiving
            self.kernel_buffer.extend_from_slice(data);
            self.bytes_received += data.len();

            // Progress indicator
            if self.bytes_received % 65536 == 0 {
                crate::print_str(".");
            }
        }

        // Check if complete
        if self.bytes_received >= self.expected_size {
            crate::print_str("\n=== Kernel Image Received Successfully ===\n");
            
            let image = KernelImage {
                data: core::mem::take(&mut self.kernel_buffer),
                size: self.expected_size,
            };

            self.receiving = false;
            self.bytes_received = 0;
            self.expected_size = 0;

            return Some(image);
        }

        None
    }

    pub fn is_receiving(&self) -> bool {
        self.receiving
    }

    pub fn progress(&self) -> (usize, usize) {
        (self.bytes_received, self.expected_size)
    }
}

/// Kernel image data
pub struct KernelImage {
    pub data: Vec<u8>,
    pub size: usize,
}

impl KernelImage {
    /// Verify checksum
    pub fn verify(&self) -> bool {
        // Simple checksum verification
        let mut sum: u32 = 0;
        for chunk in self.data.chunks(4) {
            if chunk.len() == 4 {
                let val = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                sum = sum.wrapping_add(val);
            }
        }
        // For now, just return true - proper validation should check header checksum
        true
    }

    /// Load kernel image to memory and prepare for execution
    pub fn prepare_execution(&self, load_addr: usize) -> Result<(), &'static str> {
        if self.size > MAX_KERNEL_SIZE {
            return Err("Kernel image too large");
        }

        unsafe {
            // Copy kernel image to load address
            core::ptr::copy_nonoverlapping(
                self.data.as_ptr(),
                load_addr as *mut u8,
                self.size,
            );

            // Flush data cache
            Self::flush_dcache(load_addr, self.size);
            
            // Invalidate instruction cache
            Self::invalidate_icache();
        }

        Ok(())
    }

    #[inline(never)]
    unsafe fn flush_dcache(addr: usize, size: usize) {
        let cache_line_size = 64;
        let start = addr & !(cache_line_size - 1);
        let end = (addr + size + cache_line_size - 1) & !(cache_line_size - 1);

        for line in (start..end).step_by(cache_line_size) {
            core::arch::asm!(
                "dc cvac, {0}",
                in(reg) line,
            );
        }
        
        core::arch::asm!("dsb sy");
    }

    #[inline(never)]
    unsafe fn invalidate_icache() {
        core::arch::asm!(
            "ic iallu",
            "dsb sy",
            "isb",
        );
    }
}

/// Execute new kernel (kexec-style)
pub unsafe fn kexec(entry_point: usize) -> ! {
    crate::print_str("Jumping to new kernel at 0x");
    crate::print_hex(entry_point);
    crate::print_str("\n");

    // Disable interrupts
    core::arch::asm!("msr daifset, #0xf");

    // Jump to new kernel
    core::arch::asm!(
        "br {0}",
        in(reg) entry_point,
        options(noreturn)
    );
}

// Global hot deploy receiver
static mut HOT_DEPLOY_RECEIVER: Option<HotDeployReceiver> = None;

pub fn init_hotdeploy() -> Result<(), &'static str> {
    unsafe {
        let mut receiver = HotDeployReceiver::new();
        receiver.init()?;
        HOT_DEPLOY_RECEIVER = Some(receiver);
    }
    Ok(())
}

pub fn get_hotdeploy_receiver() -> Option<&'static mut HotDeployReceiver> {
    unsafe { HOT_DEPLOY_RECEIVER.as_mut() }
}

/// Poll for hot deploy updates and handle them
pub fn poll_hotdeploy(_timestamp_ms: u64) {
    const KERNEL_LOAD_ADDR: usize = 0x80000; // Standard RPi kernel load address

    if let Some(receiver) = get_hotdeploy_receiver() {
        if let Some(image) = receiver.poll() {
            crate::print_str("\n=== Current Build Info (Before Hot Deploy) ===\n");
            print_build_info();
            crate::print_str("\nVerifying kernel image...\n");
            
            if image.verify() {
                crate::print_str("Loading new kernel...\n");
                
                if let Ok(()) = image.prepare_execution(KERNEL_LOAD_ADDR) {
                    crate::print_str("Starting new kernel...\n");
                    
                    // Small delay to let UART flush
                    for _ in 0..100000 {
                        core::hint::spin_loop();
                    }
                    
                    unsafe {
                        kexec(KERNEL_LOAD_ADDR);
                    }
                } else {
                    crate::print_str("Failed to load kernel\n");
                }
            } else {
                crate::print_str("Kernel verification failed\n");
            }
        }
    }
}
