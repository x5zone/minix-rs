//! Packet-layer sharing: buffer defaults, capacity gate, header flags.
//!
//! C correspondence: `minix3/minix/net/lwip/pktsock.c` (1236 lines) with the
//! datagram defaults in `minix3/minix/net/lwip/udpsock.c:29-34` and the raw
//! defaults in `minix3/minix/net/lwip/rawsock.c:48-55`. Queue storage,
//! packet copies, and wakeups stay in the service binary. This module owns
//! the portion both children reuse: which default sizes a new socket gets,
//! when a newcomer packet fits the receive queue, and which header flag bits
//! mean what.
//!
//! The sharing is deliberate. Datagram and raw sockets differ in protocol
//! handling but share queue mechanics, so the original factors the mechanics
//! into `pktsock_socket` (`pktsock.c:59-77`), `pktsock_may_recv`
//! (`pktsock.c:85-115`), and `pktsock_input` (`pktsock.c:139-256`). The Rust
//! form keeps the sharing but makes it explicit: constants for the defaults,
//! one pure gate function, and named flag constants instead of bare numbers.

/// Smallest datagram send buffer (`UDP_SNDBUF_MIN`, `udpsock.c:29`).
pub const DATAGRAM_SEND_MIN: u32 = 1;
/// Default datagram send buffer (`UDP_SNDBUF_DEF`, `udpsock.c:30`).
pub const DATAGRAM_SEND_DEFAULT: u32 = 8192;
/// Largest datagram payload and send buffer ceiling (`UDP_MAX_PAYLOAD`,
/// `udpsock.c:28`; `UDP_SNDBUF_MAX`, `udpsock.c:31`).
pub const DATAGRAM_MAX_PAYLOAD: u32 = 65535;
/// Smallest datagram receive buffer (`UDP_RCVBUF_MIN`, `udpsock.c:32`, which
/// equals one pool slice of 512 bytes, `lwipopts.h:49`).
pub const DATAGRAM_RECEIVE_MIN: u32 = 512;
/// Default datagram receive buffer (`UDP_RCVBUF_DEF`, `udpsock.c:33`).
pub const DATAGRAM_RECEIVE_DEFAULT: u32 = 32768;
/// Largest datagram receive buffer (`UDP_RCVBUF_MAX`, `udpsock.c:34`).
pub const DATAGRAM_RECEIVE_MAX: u32 = 65536;

/// Smallest raw send buffer (`RAW_SNDBUF_MIN`, `rawsock.c:50`).
pub const RAW_SEND_MIN: u32 = 1;
/// Default and largest raw send buffer (`RAW_SNDBUF_DEF` and
/// `RAW_SNDBUF_MAX`, `rawsock.c:51-52`, both equal the largest payload).
pub const RAW_SEND_DEFAULT: u32 = 65535;
/// Largest raw payload (`RAW_MAX_PAYLOAD`, `rawsock.c:48`, the largest
/// unsigned 16-bit value).
pub const RAW_MAX_PAYLOAD: u32 = 65535;
/// Smallest raw receive buffer (`RAW_RCVBUF_MIN`, `rawsock.c:53`, one pool
/// slice).
pub const RAW_RECEIVE_MIN: u32 = 512;
/// Default raw receive buffer (`RAW_RCVBUF_DEF`, `rawsock.c:54`).
pub const RAW_RECEIVE_DEFAULT: u32 = 32768;
/// Largest raw receive buffer (`RAW_RCVBUF_MAX`, `rawsock.c:55`).
pub const RAW_RECEIVE_MAX: u32 = 65536;

/// Header flag marking a version 6 packet (`PKTHF_IPV6`, `pktsock.c:38`).
pub const HEADER_IPV6: u8 = 0x01;
/// Header flag marking a multicast packet (`PKTHF_MCAST`, `pktsock.c:39`).
pub const HEADER_MULTICAST: u8 = 0x02;
/// Header flag marking a broadcast packet (`PKTHF_BCAST`, `pktsock.c:40`).
pub const HEADER_BROADCAST: u8 = 0x04;

/// Whether a newcomer packet fits the receive queue (`pktsock_may_recv`,
/// `pktsock.c:85-115`).
///
/// The gate compares total queued bytes plus the newcomer total against the
/// receive buffer (`pktsock.c:107-108`). It deliberately uses totals, not
/// packet counts: a comment at `pktsock.c:106` explains that an earlier
/// version compared against a fixed 64-kilobyte ceiling and dropped large
/// but legal packets. Multicast awareness (`pktsock.c:92-94`) and the cheap
/// pre-check for raw sockets (`pktsock_test_input`, `pktsock.c:117-136`)
/// stay in the service binary because they need socket and packet state.
pub fn may_receive(queued: u64, newcomer_total: u64, receive_buffer: u64) -> bool {
    queued.saturating_add(newcomer_total) <= receive_buffer
}

/// Whether a header flag set contains one flag.
pub fn has_header_flag(set: u8, flag: u8) -> bool {
    set & flag != 0
}

/// Whether a buffer size passes a protocol limit table.
///
/// Both children inject their defaults at creation (datagram at
/// `udpsock.c:138-139`, raw at `rawsock.c:310-311`) and advertise the same
/// limits through the management tree. This helper owns the closed-interval
/// comparison so the two call sites cannot drift apart.
pub fn buffer_size_allowed(value: u32, minimum: u32, maximum: u32) -> bool {
    minimum <= value && value <= maximum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_match_socket_sources() {
        assert_eq!(DATAGRAM_SEND_DEFAULT, 8192);
        assert_eq!(DATAGRAM_RECEIVE_DEFAULT, 32768);
        assert_eq!(DATAGRAM_MAX_PAYLOAD, 65535);
        assert_eq!(RAW_SEND_DEFAULT, 65535);
        assert_eq!(RAW_RECEIVE_DEFAULT, 32768);
    }

    #[test]
    fn test_limit_tables_match_socket_sources() {
        assert_eq!((DATAGRAM_SEND_MIN, DATAGRAM_MAX_PAYLOAD), (1, 65535));
        assert_eq!((DATAGRAM_RECEIVE_MIN, DATAGRAM_RECEIVE_MAX), (512, 65536));
        assert_eq!((RAW_SEND_MIN, RAW_MAX_PAYLOAD), (1, 65535));
        assert_eq!((RAW_RECEIVE_MIN, RAW_RECEIVE_MAX), (512, 65536));
        assert!(buffer_size_allowed(8192, DATAGRAM_SEND_MIN, DATAGRAM_MAX_PAYLOAD));
        assert!(!buffer_size_allowed(0, DATAGRAM_SEND_MIN, DATAGRAM_MAX_PAYLOAD));
    }

    #[test]
    fn test_capacity_gate_uses_totals() {
        assert!(may_receive(0, 1000, 32768));
        assert!(may_receive(32000, 768, 32768));
        assert!(!may_receive(32000, 769, 32768));
        assert!(!may_receive(40000, 100, 32768));
    }

    #[test]
    fn test_capacity_gate_saturates_instead_of_wrapping() {
        assert!(!may_receive(u64::MAX, 1, 32768));
    }

    #[test]
    fn test_header_flags_match_packet_header() {
        assert_eq!(HEADER_IPV6, 0x01);
        assert_eq!(HEADER_MULTICAST, 0x02);
        assert_eq!(HEADER_BROADCAST, 0x04);
        assert!(has_header_flag(0x03, HEADER_IPV6));
        assert!(has_header_flag(0x03, HEADER_MULTICAST));
        assert!(!has_header_flag(0x03, HEADER_BROADCAST));
    }
}
