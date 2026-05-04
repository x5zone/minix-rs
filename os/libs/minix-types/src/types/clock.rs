//! Time type definitions.
//!
//! 64-bit time-related types.

/// Clock tick count (64-bit signed integer).
///
/// On 64-bit systems, `clock_t` is 8 bytes.
pub type Clock = i64;

/// Timestamp (64-bit signed integer).
pub type Time = i64;

/// File offset (64-bit signed integer).
pub type Off = i64;
