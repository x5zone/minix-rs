//! Stream socket policy: connection flags, buffer bounds, broken-pipe rule.
//!
//! C correspondence: `minix3/minix/net/lwip/tcpsock.c` (2793 lines) with the
//! flag layout in `minix3/minix/net/lwip/ipsock.h:29-33` and the initial
//! sequence number generator in `minix3/minix/net/lwip/tcpisn.c` (203
//! lines). Connection storage, send and receive queues, protocol control
//! blocks, and hash-based sequence numbers stay in the service binary. This
//! module owns the portion that can be decided from numbers alone: which
//! progress flag means what, which buffer sizes pass, and when a write
//! counts as a broken pipe.
//!
//! The original tracks only a partial view of the connection on purpose. A
//! comment at `tcpsock.c:41` explains that the service reuses the stack state
//! machine and keeps five local flags for the events it cares about. The Rust
//! form keeps that split: an enumeration for the five flags, interval checks
//! for the bounds, and a two-condition rule for broken pipes.

/// Smallest stream send buffer (`TCP_SNDBUF_MIN`, `tcpsock.c:86`).
pub const SEND_BUFFER_MIN: u32 = 1;
/// Default stream send buffer (`TCP_SNDBUF_DEF`, `tcpsock.c:87`).
pub const SEND_BUFFER_DEFAULT: u32 = 32768;
/// Largest stream send buffer (`TCP_SNDBUF_MAX`, `tcpsock.c:88`).
pub const SEND_BUFFER_MAX: u32 = 131072;

/// Smallest stream receive buffer (`TCP_RCVBUF_MIN`, `tcpsock.c:89`, which
/// equals the stack window `TCP_WND`, 16384 bytes, `lwipopts.h:267`).
pub const RECEIVE_BUFFER_MIN: u32 = 16384;
/// Default stream receive buffer (`TCP_RCVBUF_DEF`, `tcpsock.c:90`, the
/// larger of the window and 32768).
pub const RECEIVE_BUFFER_DEFAULT: u32 = 32768;
/// Largest stream receive buffer (`TCP_RCVBUF_MAX`, `tcpsock.c:91`, the
/// larger of the window and 131072).
pub const RECEIVE_BUFFER_MAX: u32 = 131072;

/// Connection progress flags (`TCPF_*`, `ipsock.h:29-33`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ConnectionFlag {
    /// A connect operation is in flight.
    Connecting = 0x1000,
    /// The local side has sent its finish marker.
    SentFinish = 0x2000,
    /// The peer finish marker has been received.
    ReceivedFinish = 0x4000,
    /// The protocol send buffer is full.
    Full = 0x8000,
    /// A memory allocation failed while connecting.
    OutOfMemory = 0x10000,
}

/// All five flags as raw values, in header order.
pub const ALL_CONNECTION_FLAGS: [u32; 5] = [
    ConnectionFlag::Connecting as u32,
    ConnectionFlag::SentFinish as u32,
    ConnectionFlag::ReceivedFinish as u32,
    ConnectionFlag::Full as u32,
    ConnectionFlag::OutOfMemory as u32,
];

/// Whether a send buffer size passes (`tcpsock.c:86-88`, advertised through
/// the management tree at `tcpsock.c:162-163` and enforced when the socket is
/// created at `tcpsock.c:268-269`).
pub fn send_buffer_allowed(size: u32) -> bool {
    (SEND_BUFFER_MIN..=SEND_BUFFER_MAX).contains(&size)
}

/// Whether a receive buffer size passes (`tcpsock.c:89-91`, enforced through
/// the same limit table mechanism as the send side).
pub fn receive_buffer_allowed(size: u32) -> bool {
    (RECEIVE_BUFFER_MIN..=RECEIVE_BUFFER_MAX).contains(&size)
}

/// Whether a write fails with a broken pipe (`tcpsock_try_send`,
/// `tcpsock.c:1702-1714`).
///
/// Two independent conditions each mean a broken pipe: the protocol control
/// block is gone (`tcpsock.c:1699-1700`), or the local side has closed the
/// stream for writing (`tcpsock.c:1712-1714`). The signal itself is raised by
/// the virtual file system, not by this layer; a comment search for the pipe
/// signal in `tcpsock.c` finds no sender, which confirms the split. This
/// function reports the error condition; signal delivery stays outside.
pub fn is_broken_pipe(has_control_block: bool, write_open: bool) -> bool {
    !has_control_block || !write_open
}

/// Whether a close operation may release the socket now
/// (`tcpsock_may_close`, `tcpsock.c:543-558`: both finish markers seen and
/// the send queue drained).
pub fn may_close_now(sent_finish: bool, received_finish: bool, send_pending: u64) -> bool {
    sent_finish && received_finish && send_pending == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_bounds_match_socket_source() {
        assert_eq!(
            (SEND_BUFFER_MIN, SEND_BUFFER_DEFAULT, SEND_BUFFER_MAX),
            (1, 32768, 131072)
        );
        assert_eq!(
            (RECEIVE_BUFFER_MIN, RECEIVE_BUFFER_DEFAULT, RECEIVE_BUFFER_MAX),
            (16384, 32768, 131072)
        );
        assert!(send_buffer_allowed(1));
        assert!(send_buffer_allowed(32768));
        assert!(send_buffer_allowed(131072));
        assert!(!send_buffer_allowed(0));
        assert!(!send_buffer_allowed(131073));
        assert!(receive_buffer_allowed(16384));
        assert!(receive_buffer_allowed(32768));
        assert!(!receive_buffer_allowed(16383));
    }

    #[test]
    fn test_flags_match_header_values() {
        assert_eq!(ConnectionFlag::Connecting as u32, 0x1000);
        assert_eq!(ConnectionFlag::SentFinish as u32, 0x2000);
        assert_eq!(ConnectionFlag::ReceivedFinish as u32, 0x4000);
        assert_eq!(ConnectionFlag::Full as u32, 0x8000);
        assert_eq!(ConnectionFlag::OutOfMemory as u32, 0x10000);
        assert_eq!(ALL_CONNECTION_FLAGS.len(), 5);
    }

    #[test]
    fn test_pipe_breaks_without_block_or_write() {
        assert!(is_broken_pipe(false, true));
        assert!(is_broken_pipe(true, false));
        assert!(is_broken_pipe(false, false));
        assert!(!is_broken_pipe(true, true));
    }

    #[test]
    fn test_close_needs_both_markers_and_empty_queue() {
        assert!(may_close_now(true, true, 0));
        assert!(!may_close_now(true, false, 0));
        assert!(!may_close_now(false, true, 0));
        assert!(!may_close_now(true, true, 1));
    }
}
