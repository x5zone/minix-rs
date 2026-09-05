//! User datagram protocol policy: protocol check, send guards, multicast defaults.
//!
//! C correspondence: `minix3/minix/net/lwip/udpsock.c` (997 lines). Queue
//! storage, protocol control blocks, and user-memory copies stay in the
//! service binary. This module owns the portion that can be decided from
//! numbers alone: which protocol values a datagram socket accepts, which send
//! flags pass, when a send length is too large, and which multicast defaults
//! a new socket gets.
//!
//! The design follows the same split the Linux datagram layer and the Redox
//! network stack use: validation helpers are pure and total, while the
//! service binary owns tables and input-output. Callers convert the boolean
//! answer into the Minix wire error at the message boundary.

/// Internet protocol number for user datagrams (`IPPROTO_UDP`, 17,
/// `minix3/sys/netinet/in.h:85`).
pub const IPPROTO_UDP: i32 = 17;

/// Send flag allowing the caller to bypass routing tables
/// (`MSG_DONTROUTE`, 0x04, `minix3/sys/sys/socket.h:510`).
pub const MSG_DONTROUTE: u32 = 0x04;

/// Largest datagram payload in bytes (`UDP_MAX_PAYLOAD`, `udpsock.c:27`,
/// the largest unsigned 16-bit value).
pub const MAX_PAYLOAD: usize = 65535;

/// Default multicast time to live for a new socket (`udpsock.c:147`,
/// set through `udp_set_multicast_ttl` with value 1).
pub const MULTICAST_TTL_DEFAULT: u8 = 1;

/// Whether multicast loopback is enabled by default for a new socket
/// (`udpsock.c:150`, `UDP_FLAGS_MULTICAST_LOOP` set at creation).
pub const MULTICAST_LOOP_DEFAULT: bool = true;

/// Whether a protocol value is accepted for a datagram socket
/// (`udpsock_socket`, `udpsock.c:116-135`).
///
/// Only the wildcard 0 and the datagram protocol number pass. The lightweight
/// checksum variant is explicitly rejected (`udpsock.c:128`, noting that the
/// reference system does not support it even though the underlying stack
/// does), as is every other protocol number.
pub fn protocol_allowed(protocol: i32) -> bool {
    protocol == 0 || protocol == IPPROTO_UDP
}

/// Whether a send flag set passes (`udpsock_pre_send`, `udpsock.c:263-264`,
/// and the raw counterpart at `rawsock.c:463-464`).
///
/// Only the do-not-route bit may be set. Any other bit means the caller asked
/// for an operation the datagram layer does not support.
pub fn send_flags_allowed(flags: u32) -> bool {
    flags & !MSG_DONTROUTE == 0
}

/// Whether a send length passes the two early checks
/// (`udpsock_pre_send`, `udpsock.c:278-280`, plus the full check at
/// `udpsock.c:486-493` once header length is known).
///
/// The early check compares the payload against the socket send buffer and
/// reports message-too-long when it exceeds it. The late check compares
/// header length plus payload against the largest payload. This helper
/// expresses the late bound, which is independent of socket state; the
/// service binary applies the early bound with the live buffer size.
pub fn payload_fits(header_length: usize, payload_length: usize) -> bool {
    header_length.saturating_add(payload_length) <= MAX_PAYLOAD
}

/// Whether a payload length passes a caller-supplied send buffer
/// (the early half of the length check, `udpsock.c:278-280`).
pub fn send_length_allowed(payload_length: usize, send_buffer: usize) -> bool {
    payload_length <= send_buffer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_accepts_wildcard_and_udp_only() {
        assert_eq!(IPPROTO_UDP, 17);
        assert!(protocol_allowed(0));
        assert!(protocol_allowed(17));
        assert!(!protocol_allowed(1));
        assert!(!protocol_allowed(6));
        assert!(!protocol_allowed(136));
    }

    #[test]
    fn test_send_flags_allow_only_do_not_route() {
        assert_eq!(MSG_DONTROUTE, 0x04);
        assert!(send_flags_allowed(0));
        assert!(send_flags_allowed(MSG_DONTROUTE));
        assert!(!send_flags_allowed(0x01));
        assert!(!send_flags_allowed(MSG_DONTROUTE | 0x08));
    }

    #[test]
    fn test_payload_bound_is_largest_16_bit_value() {
        assert_eq!(MAX_PAYLOAD, 65535);
        assert!(payload_fits(20, 65515));
        assert!(!payload_fits(20, 65516));
        assert!(!payload_fits(65535, 1));
    }

    #[test]
    fn test_send_length_compares_against_live_buffer() {
        assert!(send_length_allowed(100, 8192));
        assert!(send_length_allowed(8192, 8192));
        assert!(!send_length_allowed(8193, 8192));
    }

    #[test]
    fn test_multicast_defaults_match_creation() {
        assert_eq!(MULTICAST_TTL_DEFAULT, 1);
        assert!(MULTICAST_LOOP_DEFAULT);
    }
}
