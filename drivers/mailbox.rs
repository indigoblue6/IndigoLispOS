// drivers/mailbox.rs
//
// IndigoLispOS 用 RP1 / BCM2712 mailbox ドライバ
//
// 1. RP1 mailbox
//    - RP1_CTRL の doorbell/status/debug 用
//    - init_mailbox(pcie_win0_cpu_base, pcie_win0_bus_base) で
//      BUS=0xC00FFF00 → CPU=0x6000_5003F00 みたいな位置を計算しておく
//
// 2. BCM2712 <-> VideoCore property mailbox
//    - 旧版 mailbox.rs と同じ “property tag” インターフェース
//    - Ethernet の電源・クロック制御（SET_POWER_STATE / SET_CLOCK_STATE）などで使用
//
//   rp1_ethernet.rs からは DeviceId::Ethernet / ClockId::Ethernet を
//   渡してくる想定。
//   → RP1 自体のクロック/リセットは別レジスタだが、
//     SoC 側の clock/power をちゃんと叩いておくと
//     Linux と同等に 1G / link up の状態に近づけられる。

use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};

//==============================================================
// 1) RP1 mailbox（PCIe WIN0 経由でアクセスするやつ）
//==============================================================

static mut RP1_MAILBOX_BASE: *mut u32 = core::ptr::null_mut();

// RP1 mailbox の bus アドレス（ログより確定）
const RP1_MAILBOX_BUS_ADDR: u64   = 0x0000_0000_C00F_FF00;
const RP1_PCIE_WIN0_BUS_BASE: u64 = 0x0000_0000_C000_0000;

//--------------------------------------------------------------
// Logging ヘルパ
//--------------------------------------------------------------
fn log(s: &str) {
    crate::print_str(s);
}

fn log_hex32(label: &str, v: u32) {
    crate::print_str(label);
    crate::print_hex(v as usize);
    crate::print_str("\n");
}

fn log_hex64(label: &str, v: u64) {
    crate::print_str(label);
    crate::print_hex(v as usize);
    crate::print_str("\n");
}

//--------------------------------------------------------------
// RP1 mailbox 初期化
//   呼び出し元は (pcie_win0_cpu_base, pcie_win0_bus_base) を渡してくる
//--------------------------------------------------------------
pub fn init_mailbox(pcie_win0_cpu_base: usize, _pcie_win0_bus_base: usize) {
    log("MAILBOX (RP1): init_mailbox()\n");

    let offset = RP1_MAILBOX_BUS_ADDR - RP1_PCIE_WIN0_BUS_BASE;
    let cpu_addr = (pcie_win0_cpu_base as u64).wrapping_add(offset);

    log_hex64("  cpu_win_base = ", pcie_win0_cpu_base as u64);
    log_hex64("  bus addr      = ", RP1_MAILBOX_BUS_ADDR);
    log_hex64("  offset        = ", offset);
    log_hex64("  mailbox cpu   = ", cpu_addr);

    unsafe {
        RP1_MAILBOX_BASE = cpu_addr as usize as *mut u32;

        // 試し読み（ヒットしなくても panic はさせない）
        let test = ptr::read_volatile(RP1_MAILBOX_BASE);
        log_hex32("MAILBOX (RP1): init read = ", test);
    }
}

// RP1 mailbox init 済み判定
pub fn is_initialized() -> bool {
    unsafe { !RP1_MAILBOX_BASE.is_null() }
}

// RP1 mailbox base (未初期化なら panic)
pub fn mailbox_base() -> *mut u32 {
    unsafe {
        if RP1_MAILBOX_BASE.is_null() {
            panic!("MAILBOX (RP1): mailbox_base() called before init_mailbox()");
        }
        RP1_MAILBOX_BASE
    }
}

// RP1 mailbox 用 生 read/write
pub unsafe fn read_reg(offset: usize) -> u32 {
    ptr::read_volatile(mailbox_base().add(offset / 4))
}

pub unsafe fn write_reg(offset: usize, val: u32) {
    ptr::write_volatile(mailbox_base().add(offset / 4), val);
}

// RP1 mailbox 用 debug dump
pub unsafe fn dump_regs(start: usize, count: usize) {
    log("MAILBOX (RP1): dump_regs\n");
    for i in 0..count {
        let off = start + i * 4;
        let val = read_reg(off);
        crate::print_str("  [");
        crate::print_hex(off as usize);
        crate::print_str("] = ");
        crate::print_hex(val as usize);
        crate::print_str("\n");
    }
}
// Re-export VideoCore mailbox API implemented in `mailbox_vc.rs`.
// This keeps the external API stable: callers can continue to use
// `crate::drivers::mailbox::set_power_state` / `set_clock_state` etc.
pub use crate::drivers::mailbox_vc::*;

/// 旧 API との互換用。今は「RP1 mailbox ベースがあるかログるだけ」
pub fn probe_mailbox_candidates_once() {
    crate::print_str("MAILBOX: probe_mailbox_candidates_once\n");
    crate::print_str("  RP1_MAILBOX_BASE=");
    unsafe {
        crate::print_hex(RP1_MAILBOX_BASE as usize);
    }
    crate::print_str(" VC_MAILBOX_BASE=");
    crate::print_hex(crate::drivers::mailbox_vc::VC_MAILBOX_BASE);
    crate::print_str("\n");
}

/// STATUS を 1 回読むだけの簡易 smoke test（RP1 側）
pub fn smoke_status() {
    crate::print_str("MAILBOX (RP1): smoke_status\n");
    if !is_initialized() {
        crate::print_str("  RP1 mailbox not initialized\n");
        return;
    }
    unsafe {
        let val = read_reg(0);
        crate::print_str("  RP1 mailbox[0] = 0x");
        crate::print_hex(val as usize);
        crate::print_str("\n");
    }
}

/// runtime からの debug dump
pub fn debug_dump_runtime_base() {
    crate::print_str("MAILBOX: debug_dump_runtime_base\n");
    unsafe {
        crate::print_str("  RP1_MAILBOX_BASE=");
        crate::print_hex(RP1_MAILBOX_BASE as usize);
        crate::print_str("\n");
    }
    crate::print_str("  VC_MAILBOX_BASE=");
    crate::print_hex(crate::drivers::mailbox_vc::VC_MAILBOX_BASE);
    crate::print_str("\n");
}
