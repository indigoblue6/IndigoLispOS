#!/usr/bin/env python3
"""
hotdeploy_send.py - Send kernel image to IndigoLispOS for hot deployment
"""

import sys
import socket
import struct
import time
from pathlib import Path

KERNEL_MAGIC = 0x494C4F53  # "ILOS"
DEFAULT_PORT = 8888
DEFAULT_IP = "192.168.10.110"
CHUNK_SIZE = 1400  # UDP safe payload size


def calculate_checksum(data: bytes) -> int:
    """Calculate simple 32-bit checksum"""
    checksum = 0
    for i in range(0, len(data), 4):
        chunk = data[i:i+4]
        if len(chunk) == 4:
            val = struct.unpack('<I', chunk)[0]
            checksum = (checksum + val) & 0xFFFFFFFF
    return checksum


def send_kernel(kernel_path: str, target_ip: str, target_port: int):
    """Send kernel image via UDP"""
    
    # Read kernel image
    kernel_file = Path(kernel_path)
    if not kernel_file.exists():
        print(f"Error: {kernel_path} not found")
        return False
    
    kernel_data = kernel_file.read_bytes()
    kernel_size = len(kernel_data)
    
    print(f"Kernel image: {kernel_path}")
    print(f"Size: {kernel_size} bytes ({kernel_size / 1024:.2f} KB)")
    print(f"Target: {target_ip}:{target_port}")
    
    # Calculate checksum
    checksum = calculate_checksum(kernel_data)
    print(f"Checksum: 0x{checksum:08X}")
    
    # Create header
    timestamp = int(time.time())
    header = struct.pack(
        '<IIIIQ',
        KERNEL_MAGIC,
        1,  # version
        kernel_size,
        checksum,
        timestamp
    )
    
    # Create socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(5.0)
    
    try:
        # Send header + first chunk
        first_chunk_size = min(CHUNK_SIZE - len(header), kernel_size)
        first_packet = header + kernel_data[:first_chunk_size]
        
        print(f"\nSending header + {first_chunk_size} bytes...")
        sock.sendto(first_packet, (target_ip, target_port))
        
        # Send remaining chunks
        offset = first_chunk_size
        chunk_num = 1
        
        while offset < kernel_size:
            chunk_end = min(offset + CHUNK_SIZE, kernel_size)
            chunk = kernel_data[offset:chunk_end]
            
            sock.sendto(chunk, (target_ip, target_port))
            
            chunk_num += 1
            offset = chunk_end
            
            # Progress indicator
            progress = (offset / kernel_size) * 100
            print(f"\rProgress: {progress:.1f}% ({offset}/{kernel_size} bytes)", end='')
            
            # Small delay to avoid overwhelming receiver
            time.sleep(0.001)
        
        print(f"\n\nTransfer complete! Sent {chunk_num} packets")
        print("Waiting for target to reload...")
        
        return True
        
    except socket.timeout:
        print("\nError: Connection timeout")
        return False
    except Exception as e:
        print(f"\nError: {e}")
        return False
    finally:
        sock.close()


def main():
    if len(sys.argv) < 2:
        print("Usage: hotdeploy_send.py <kernel8.img> [target_ip] [target_port]")
        print(f"  Default target: {DEFAULT_IP}:{DEFAULT_PORT}")
        sys.exit(1)
    
    kernel_path = sys.argv[1]
    target_ip = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_IP
    target_port = int(sys.argv[3]) if len(sys.argv) > 3 else DEFAULT_PORT
    
    success = send_kernel(kernel_path, target_ip, target_port)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
