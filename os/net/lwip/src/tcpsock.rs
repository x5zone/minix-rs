//! Stream socket policy: connection states, buffer bounds, pipe rule.
//!
//! C correspondence: the connection tracking flags (`TCPF_*` in
//! `ipsock.h:29-33`: connecting, sent-fin, received-fin, full,
//! out-of-memory), the buffer bounds (send between one and 131072
//! with default 32768, `tcpsock.c:86-88`; receive floor at the
//! window size, `tcpsock.c:89-91`), and the broken-pipe rule (no
//! protocol control block, or locally closed for writing, means
//! broken pipe, `tcpsock.c:1699-1714`; the signal itself is raised
//! by the virtual file system, not here).
//!
//! Connection storage stays in the service binary; this module owns
//! the pure policy half: which state, which bounds, when a pipe
//! counts as broken.

/// Smallest send buffer (`TCP_SNDBUF_MIN`, via `SNDBUF_MIN`).
pub const SEND_BUFFER_MIN: u32 = 1;

/// Default send buffer (`TCP_SNDBUF_DEF`).
pub const SEND_BUFFER_DEFAULT: u32 = 32768;

/// Largest send buffer (`TCP_SNDBUF_MAX`).
pub const SEND_BUFFER_MAX: u32 = 131072;

/// Connection progress flags (`TCPF_*`, `ipsock.h:29-33`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ConnectionFlag {
    /// Connect in flight.
    Connecting = 0x1000,
    /// Our finish sent.
    SentFinish = 0x2000,
    /// Their finish received.
    ReceivedFinish = 0x4000,
    /// Connection fully open.
    Full = 0x8000,
    /// Out of memory while connecting.
    OutOfMemory = 0x10000,
}

/// Whether a send buffer size passes (`tcpsock.c:86-88`).
pub fn send_buffer_allowed(size: u32) -> bool {
    (SEND_BUFFER_MIN..=SEND_BUFFER_MAX).contains(&size)
}

/// Whether a write fails with a broken pipe (`tcpsock.c:1699-1714`):
/// no control block, or locally closed for writing.
pub fn is_broken_pipe(has_control_block: bool, write_open: bool) -> bool {
    !has_control_block || !write_open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_bounds_match_socket_source() {
        assert_eq!((SEND_BUFFER_MIN, SEND_BUFFER_DEFAULT, SEND_BUFFER_MAX), (1, 32768, 131072));
        assert!(send_buffer_allowed(1));
        assert!(send_buffer_allowed(32768));
        assert!(send_buffer_allowed(131072));
        assert!(!send_buffer_allowed(0));
        assert!(!send_buffer_allowed(131073));
    }

    #[test]
    fn test_flags_match_header_values() {
        assert_eq!(ConnectionFlag::Connecting as u32, 0x1000);
        assert_eq!(ConnectionFlag::SentFinish as u32, 0x2000);
        assert_eq!(ConnectionFlag::ReceivedFinish as u32, 0x4000);
        assert_eq!(ConnectionFlag::Full as u32, 0x8000);
        assert_eq!(ConnectionFlag::OutOfMemory as u32, 0x10000);
    }

    #[test]
    fn test_pipe_breaks_without_block_or_write() {
        assert!(is_broken_pipe(false, true));
        assert!(is_broken_pipe(true, false));
        assert!(is_broken_pipe(false, false));
        assert!(!is_broken_pipe(true, true));
    }
}
