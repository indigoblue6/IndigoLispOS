// allocator.rs - Simple bump allocator for kernel heap

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr;

pub struct BumpAllocator {
    heap_start: UnsafeCell<usize>,
    heap_end: UnsafeCell<usize>,
    next: UnsafeCell<usize>,
}

unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    pub const fn new() -> Self {
        Self {
            heap_start: UnsafeCell::new(0),
            heap_end: UnsafeCell::new(0),
            next: UnsafeCell::new(0),
        }
    }

    pub unsafe fn init(&self, heap_start: *mut u8, size: usize) {
        *self.heap_start.get() = heap_start as usize;
        *self.heap_end.get() = heap_start as usize + size;
        *self.next.get() = heap_start as usize;
    }

    fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            let heap_start = *self.heap_start.get();
            let next = *self.next.get();
            let heap_end = *self.heap_end.get();

            // Check if allocator is initialized
            if heap_start == 0 || heap_end == 0 {
                let uart_base = 0x107d001000usize;
                let uart_dr = (uart_base + 0x00) as *mut u32;
                let msg = b"\n*** ALLOC ERROR: not initialized ***\n";
                for &c in msg {
                    ptr::write_volatile(uart_dr, c as u32);
                }
                return ptr::null_mut();
            }

            // Align allocation
            let alloc_start = align_up(next, layout.align());
            let alloc_end = alloc_start.saturating_add(layout.size());

            if alloc_end > heap_end {
                // Out of memory - print debug info
                let uart_base = 0x107d001000usize;
                let uart_dr = (uart_base + 0x00) as *mut u32;
                let msg = b"\n*** OOM: size=";
                for &c in msg {
                    ptr::write_volatile(uart_dr, c as u32);
                }
                // Print size in hex
                let size = layout.size();
                for i in (0..16).rev() {
                    let digit = ((size >> (i * 4)) & 0xF) as u8;
                    let ch = if digit < 10 { b'0' + digit } else { b'A' + digit - 10 };
                    ptr::write_volatile(uart_dr, ch as u32);
                }
                let msg2 = b" used=";
                for &c in msg2 {
                    ptr::write_volatile(uart_dr, c as u32);
                }
                let used = next - heap_start;
                for i in (0..16).rev() {
                    let digit = ((used >> (i * 4)) & 0xF) as u8;
                    let ch = if digit < 10 { b'0' + digit } else { b'A' + digit - 10 };
                    ptr::write_volatile(uart_dr, ch as u32);
                }
                let msg3 = b" ***\n";
                for &c in msg3 {
                    ptr::write_volatile(uart_dr, c as u32);
                }
                ptr::null_mut()
            } else {
                *self.next.get() = alloc_end;
                alloc_start as *mut u8
            }
        }
    }
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.alloc(layout)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator doesn't support deallocation
        // TODO: Implement a more sophisticated allocator
    }
}

#[global_allocator]
pub static ALLOCATOR: BumpAllocator = BumpAllocator::new();



fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
