//! Notification type definitions.
//!
//! Corresponds to Minix3's notification types used in IPC.

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
