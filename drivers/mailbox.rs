// drivers/mailbox.rs
//
// IndigoLispOS 用 BCM2712 / VideoCore mailbox ドライバ
//
// VideoCore property mailbox API を提供します。
// RP1 mailbox は drivers/mailbox_rp1.rs で実装されています。



//--------------------------------------------------------------
// Logging ヘルパ
//--------------------------------------------------------------
fn log(s: &str) {
    crate::print_str(s);
}

// Re-export VideoCore mailbox API implemented in `mailbox_vc.rs`.
// This keeps the external API stable: callers can continue to use
// `crate::drivers::mailbox::set_power_state` / `set_clock_state` etc.
pub use crate::drivers::mailbox_vc::*;

/// 旧 API との互換用。RP1 と VC の mailbox base をログ出力します。
pub fn probe_mailbox_candidates_once() {
    crate::print_str("MAILBOX: probe_mailbox_candidates_once\n");
    crate::print_str("  RP1_MBOX_RUNTIME_BASE=");
    crate::print_hex(crate::drivers::mailbox_rp1::RP1_MBOX_RUNTIME_BASE.load(core::sync::atomic::Ordering::SeqCst));
    crate::print_str(" VC_MAILBOX_BASE=");
    crate::print_hex(crate::drivers::mailbox_vc::VC_MAILBOX_BASE);
    crate::print_str("\n");
}

/// STATUS を 1 回読むだけの簡易 smoke test（VC 側）
pub fn smoke_status() {
    crate::print_str("MAILBOX (VC): smoke_status\n");
    crate::print_str("  VC_MAILBOX_BASE=");
    crate::print_hex(crate::drivers::mailbox_vc::VC_MAILBOX_BASE);
    crate::print_str("\n");
}

/// runtime からの debug dump
pub fn debug_dump_runtime_base() {
    crate::print_str("MAILBOX: debug_dump_runtime_base\n");
    crate::print_str("  RP1_MBOX_RUNTIME_BASE=");
    crate::print_hex(crate::drivers::mailbox_rp1::RP1_MBOX_RUNTIME_BASE.load(core::sync::atomic::Ordering::SeqCst));
    crate::print_str("\n");
    crate::print_str("  VC_MAILBOX_BASE=");
    crate::print_hex(crate::drivers::mailbox_vc::VC_MAILBOX_BASE);
    crate::print_str("\n");
}
