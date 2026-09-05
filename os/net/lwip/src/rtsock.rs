//! Routing socket policy: buffer bounds and message version check.
//!
//! C correspondence: `minix3/minix/net/lwip/rtsock.c` (1912 lines).
//! Socket storage, message parsing, and table updates stay in the service
//! binary. This module owns the portion that can be decided from numbers
//! alone: which send and receive sizes pass, and which message versions are
//! accepted.
//!
//! Routing messages and address structures must only cross the boundary in
//! this module. Other modules must not reference the routing header or the
//! stack address types directly; they go through the routing socket.

/// Largest routing send buffer (`RT_SNDBUF_MAX`, 512, `rtsock.c:28`; there
/// is deliberately no minimum or default because sends are single messages).
pub const SEND_BUFFER_MAX: usize = 512;

/// Smallest routing receive buffer (`RT_RCVBUF_MIN`, 0, `rtsock.c:30`).
pub const RECEIVE_BUFFER_MIN: usize = 0;

/// Default routing receive buffer (`RT_RCVBUF_DEF`, 16384, `rtsock.c:31`,
/// installed at creation in `rtsock.c:338`).
pub const RECEIVE_BUFFER_DEFAULT: usize = 16384;

/// Largest routing receive buffer (`RT_RCVBUF_MAX`, 65536, `rtsock.c:32`).
pub const RECEIVE_BUFFER_MAX: usize = 65536;

/// Expected routing message version (`RTM_VERSION`, 4,
/// `minix3/sys/net/route.h:208`, checked at `rtsock.c:535`: mismatched
/// versions are rejected before the type switch).
pub const MESSAGE_VERSION: u8 = 4;

/// Whether a send length passes (`rtsock_pre_send`, `rtsock.c:634-651`:
/// messages longer than the send maximum are refused).
pub fn send_length_allowed(length: usize) -> bool {
    length <= SEND_BUFFER_MAX
}

/// Whether a receive buffer size passes (the option range at
/// `rtsock.c:825-830`).
pub fn receive_buffer_allowed(size: usize) -> bool {
    (RECEIVE_BUFFER_MIN..=RECEIVE_BUFFER_MAX).contains(&size)
}

/// Whether a message version is accepted.
pub fn message_version_allowed(version: u8) -> bool {
    version == MESSAGE_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_bounds_match_socket_source() {
        assert_eq!(SEND_BUFFER_MAX, 512);
        assert_eq!(RECEIVE_BUFFER_DEFAULT, 16384);
        assert_eq!(RECEIVE_BUFFER_MAX, 65536);
        assert!(send_length_allowed(512));
        assert!(!send_length_allowed(513));
        assert!(receive_buffer_allowed(0));
        assert!(receive_buffer_allowed(65536));
        assert!(!receive_buffer_allowed(65537));
    }

    #[test]
    fn test_message_version_is_checked_first() {
        assert!(message_version_allowed(4));
        assert!(!message_version_allowed(3));
        assert!(!message_version_allowed(5));
    }
}
