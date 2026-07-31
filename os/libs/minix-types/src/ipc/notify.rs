//! Notification type definitions.
//!
//! Corresponds to Minix3's notification types used in IPC.

use crate::ipc::message::MESSAGE_PAYLOAD_SIZE;

/// Notification type.
///
/// Identifies the kind of asynchronous notification sent to a process.
/// Corresponds to Minix3's notify message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum NotifyType {
    /// Hardware interrupt.
    HardInt = 1,
    /// Clock tick.
    ClockTick = 2,
    /// System event.
    SysEvent = 3,
}

/// Notification message payload.
///
/// C: `mess_notify` — `minix3/minix/include/minix/ipc.h:1714-1719`
///
/// Filled by `BuildNotifyMessage` (proc.c:98-114) when a notification
/// is delivered synchronously (dst was in RECEIVE). The `m_type` field
/// of the enclosing `Message` is set to `NOTIFY_MESSAGE`.
///
/// # Field semantics by source
///
/// | Source | `timestamp` | `interrupts` | `sigset` |
/// |--------|-------------|--------------|----------|
/// | HARDWARE | ✅ get_monotonic() | ✅ `s_int_pending` (then cleared) | zero |
/// | SYSTEM | ✅ get_monotonic() | zero | ✅ `s_sig_pending` (then cleared) |
/// | Process | ✅ get_monotonic() | zero | zero |
///
/// # Layout
///
/// Total size = 56 bytes (= `MESSAGE_PAYLOAD_SIZE`), matching the C
/// `mess_notify` union member. Rust's `SigSet` is `u64` (8 bytes);
/// C's `sigset_t` is 16 bytes on 32-bit Minix3. Since minix-rs is a
/// complete Rust rewrite with no C wire-format compatibility, we use
/// `u64` for `sigset` and pad to 56 bytes.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MessNotify {
    /// Monotonic timestamp at notification time.
    /// C: `m_notify.timestamp = get_monotonic()`
    pub timestamp: u64,
    /// Pending hardware interrupt bitmap.
    /// Valid only when source == HARDWARE; copied from `priv(dst)->s_int_pending` then cleared.
    /// C: `m_notify.interrupts = priv(dst_ptr)->s_int_pending`
    pub interrupts: u64,
    /// Pending signal bitmap.
    /// Valid only when source == SYSTEM; copied from `priv(dst)->s_sig_pending` then cleared.
    /// C: `m_notify.sigset` (C uses `sigset_t`; Rust uses `u64` matching `SigSet`)
    pub sigset: u64,
    /// Padding to fill `MESSAGE_PAYLOAD_SIZE` (56 bytes total).
    _padding: [u8; 32],
}

impl MessNotify {
    /// Create a zeroed notification payload.
    pub const fn zeroed() -> Self {
        Self {
            timestamp: 0,
            interrupts: 0,
            sigset: 0,
            _padding: [0u8; 32],
        }
    }

    /// Create a notification payload with the given field values.
    ///
    /// Used by `build_notify_message` to construct a `MessNotify` from
    /// the source-specific fields (timestamp / interrupts / sigset).
    /// `_padding` is zeroed and kept private — callers cannot set it.
    pub const fn new(timestamp: u64, interrupts: u64, sigset: u64) -> Self {
        Self {
            timestamp,
            interrupts,
            sigset,
            _padding: [0u8; 32],
        }
    }
}

impl Default for MessNotify {
    fn default() -> Self {
        Self::zeroed()
    }
}

// Compile-time size assertion: MessNotify must fit in the message payload.
const _: () = assert!(
    core::mem::size_of::<MessNotify>() <= MESSAGE_PAYLOAD_SIZE,
    "MessNotify exceeds MESSAGE_PAYLOAD_SIZE"
);
