//! User-space socket call policy: call list, flag handling, fallback rule.
//!
//! C correspondence: `minix3/minix/lib/libc/sys/socket.c` and the 14
//! companion files (3173 lines total). Actual system-call traps stay with
//! the callers in this crate. This module owns the portion that can be
//! decided without trapping: which calls exist, how type flags map to file
//! flags, and when the legacy device fallback applies.
//!
//! Main path first: every call goes through the virtual file system socket
//! request. Only when the file system answers address-family-not-supported
//! or function-not-implemented does the wrapper fall back to opening a
//! legacy device node. [ARCH N-2]: the rewrite drops the fallback and keeps
//! the file system path as the only path; the rule below documents where
//! the old code would have branched so reviewers can verify its absence.

/// Socket-related calls wrapped by the user library, in source order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketCall {
    /// Create a socket.
    Socket,
    /// Create a connected pair.
    SocketPair,
    /// Bind to a local address.
    Bind,
    /// Connect to a peer.
    Connect,
    /// Mark as listening.
    Listen,
    /// Accept one connection.
    Accept,
    /// Send to an address.
    SendTo,
    /// Receive from an address.
    ReceiveFrom,
    /// Send a message.
    SendMessage,
    /// Receive a message.
    ReceiveMessage,
    /// Set an option.
    SetOption,
    /// Get an option.
    GetOption,
    /// Query the local address.
    GetSocketName,
    /// Query the peer address.
    GetPeerName,
    /// Shut down one direction.
    Shutdown,
}

/// All 15 calls in source order.
pub const ALL_CALLS: [SocketCall; 15] = [
    SocketCall::Socket,
    SocketCall::SocketPair,
    SocketCall::Bind,
    SocketCall::Connect,
    SocketCall::Listen,
    SocketCall::Accept,
    SocketCall::SendTo,
    SocketCall::ReceiveFrom,
    SocketCall::SendMessage,
    SocketCall::ReceiveMessage,
    SocketCall::SetOption,
    SocketCall::GetOption,
    SocketCall::GetSocketName,
    SocketCall::GetPeerName,
    SocketCall::Shutdown,
];

/// Close-on-exec flag bit in the socket type (`SOCK_CLOEXEC`).
pub const FLAG_CLOSE_ON_EXEC: i32 = 0x01;
/// Non-blocking flag bit in the socket type (`SOCK_NONBLOCK`).
pub const FLAG_NON_BLOCKING: i32 = 0x02;
/// Suppress-broken-pipe flag bit (`SOCK_NOSIGPIPE`).
pub const FLAG_NO_BROKEN_PIPE_SIGNAL: i32 = 0x04;

/// Open flags produced from socket type bits (`_socket_flags`,
/// `socket.c`: close-on-exec maps to `O_CLOEXEC`, non-blocking to
/// `O_NONBLOCK`, no-broken-pipe-signal to `O_NOSIGPIPE`).
pub fn open_flags_from_socket_type(socket_type: i32) -> i32 {
    const OPEN_CLOSE_ON_EXEC: i32 = 0x01;
    const OPEN_NON_BLOCKING: i32 = 0x02;
    const OPEN_NO_BROKEN_PIPE_SIGNAL: i32 = 0x04;
    let mut result = 0;
    if socket_type & FLAG_CLOSE_ON_EXEC != 0 {
        result |= OPEN_CLOSE_ON_EXEC;
    }
    if socket_type & FLAG_NON_BLOCKING != 0 {
        result |= OPEN_NON_BLOCKING;
    }
    if socket_type & FLAG_NO_BROKEN_PIPE_SIGNAL != 0 {
        result |= OPEN_NO_BROKEN_PIPE_SIGNAL;
    }
    result
}

/// Whether the legacy device fallback applies to a file system failure.
///
/// Only address-family-not-supported and function-not-implemented trigger
/// the fallback in the original (`socket.c`: the wrapper returns early for
/// any other outcome). [ARCH N-2]: the rewrite never falls back; this
/// function documents the old branch condition so its removal is reviewable.
pub fn legacy_fallback_applies(failed: bool, errno: i32) -> bool {
    const EAFNOSUPPORT: i32 = 47;
    const ENOSYS: i32 = 78;
    failed && (errno == EAFNOSUPPORT || errno == ENOSYS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_list_covers_all_wrappers() {
        assert_eq!(ALL_CALLS.len(), 15);
        assert_eq!(ALL_CALLS[0], SocketCall::Socket);
        assert_eq!(ALL_CALLS[14], SocketCall::Shutdown);
    }

    #[test]
    fn test_flags_map_bit_for_bit() {
        assert_eq!(open_flags_from_socket_type(0), 0);
        assert_eq!(
            open_flags_from_socket_type(FLAG_CLOSE_ON_EXEC),
            0x01
        );
        assert_eq!(
            open_flags_from_socket_type(FLAG_CLOSE_ON_EXEC | FLAG_NON_BLOCKING),
            0x03
        );
        assert_eq!(
            open_flags_from_socket_type(
                FLAG_CLOSE_ON_EXEC | FLAG_NON_BLOCKING | FLAG_NO_BROKEN_PIPE_SIGNAL
            ),
            0x07
        );
    }

    #[test]
    fn test_fallback_only_on_two_errors() {
        assert!(legacy_fallback_applies(true, 47));
        assert!(legacy_fallback_applies(true, 78));
        assert!(!legacy_fallback_applies(true, 22));
        assert!(!legacy_fallback_applies(false, 47));
    }
}
