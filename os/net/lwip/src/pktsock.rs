//! Packet socket sharing: buffer defaults plus capacity gate.
//!
//! C correspondence: the send and receive buffer defaults (datagram:
//! send 8192, receive 32768, `udpsock.c:30-33`; raw: send largest
//! payload, receive 32768, `rawsock.c:51-54`), the receive capacity
//! gate (`pktsock_may_recv`, `pktsock.c:84-108`: queued length plus
//! the newcomer must fit the receive buffer), and the packet header
//! flags (`PKTHF_IPV6/MCAST/BCAST`, `pktsock.c:28-40`).
//!
//! Queue storage stays in the service binary; this module owns the
//! pure sharing half: default sizes and the capacity gate both
//! children reuse.

/// Default datagram send buffer (`UDP_SNDBUF_DEF`).
pub const DATAGRAM_SEND_DEFAULT: u32 = 8192;

/// Default datagram receive buffer (`UDP_RCVBUF_DEF`).
pub const DATAGRAM_RECEIVE_DEFAULT: u32 = 32768;

/// Largest datagram payload (`UDP_MAX_PAYLOAD`).
pub const DATAGRAM_MAX_PAYLOAD: u32 = 65535;

/// Default raw receive buffer (`RAW_RCVBUF_DEF`).
pub const RAW_RECEIVE_DEFAULT: u32 = 32768;

/// Header flag marking a sixth-version packet (`PKTHF_IPV6`).
pub const HEADER_IPV6: u8 = 0x01;
/// Header flag marking a multicast packet (`PKTHF_MCAST`).
pub const HEADER_MULTICAST: u8 = 0x02;
/// Header flag marking a broadcast packet (`PKTHF_BCAST`).
pub const HEADER_BROADCAST: u8 = 0x04;

/// Whether a newcomer packet fits the receive queue
/// (`pktsock_may_recv`, `pktsock.c:84-108`): queued length plus the
/// newcomer total must stay within the receive buffer.
pub fn may_receive(queued: u64, newcomer_total: u64, receive_buffer: u64) -> bool {
    queued + newcomer_total <= receive_buffer
}

/// Whether a header flag set contains one flag.
pub fn has_header_flag(set: u8, flag: u8) -> bool {
    set & flag != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_match_socket_sources() {
        assert_eq!(DATAGRAM_SEND_DEFAULT, 8192);
        assert_eq!(DATAGRAM_RECEIVE_DEFAULT, 32768);
        assert_eq!(DATAGRAM_MAX_PAYLOAD, 65535);
        assert_eq!(RAW_RECEIVE_DEFAULT, 32768);
    }

    #[test]
    fn test_capacity_gate_uses_totals() {
        assert!(may_receive(0, 1000, 32768));
        assert!(may_receive(32000, 768, 32768));
        assert!(!may_receive(32000, 769, 32768));
        assert!(!may_receive(40000, 100, 32768));
    }

    #[test]
    fn test_header_flags_match_packet_header() {
        assert_eq!(HEADER_IPV6, 0x01);
        assert_eq!(HEADER_MULTICAST, 0x02);
        assert_eq!(HEADER_BROADCAST, 0x04);
        assert!(has_header_flag(0x03, HEADER_IPV6));
        assert!(!has_header_flag(0x03, HEADER_BROADCAST));
    }
}
