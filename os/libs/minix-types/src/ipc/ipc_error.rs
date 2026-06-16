//! IPC error types.
//!
//! Defines errors that can occur during IPC operations.

/// IPC operation error.
///
/// Corresponds to Minix3's IPC error conditions returned by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Operation would block (non-blocking mode).
    WouldBlock,
    /// Operation was interrupted.
    Interrupted,
    /// No permission to send to target.
    NoPerm,
}
