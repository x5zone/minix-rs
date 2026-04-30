//! Process identifier type definitions.
//!
//! Provides Process ID (Pid) type.

/// Process ID (32-bit signed integer).
///
/// Corresponds to C's `pid_t`, 4 bytes on both 32-bit and 64-bit systems.
pub type Pid = i32;