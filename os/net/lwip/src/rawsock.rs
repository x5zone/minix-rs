//! Raw socket policy: protocol range, privilege rule, header handling.
//!
//! C correspondence: `minix3/minix/net/lwip/rawsock.c` (1341 lines) with the
//! creation gate in `minix3/minix/net/lwip/lwip.c:152-188`. Queue storage,
//! protocol control blocks, and header construction stay in the service
//! binary. This module owns the portion that can be decided from numbers
//! alone: which protocol values pass, why creation always requires the
//! superuser, when checksums are mandatory, and which send guards apply.
//!
//! Raw sockets bypass the transport layer, so the kernel must restrict who
//! can create them. The gate lives one layer above this module, in the
//! socket allocator, which checks the privilege before dispatching here.

/// Internet protocol number for control messages (`IPPROTO_ICMP`, 1,
/// `minix3/sys/netinet/in.h:77`).
pub const IPPROTO_ICMP: i32 = 1;

/// Internet protocol number for version 6 control messages
/// (`IPPROTO_ICMPV6`, 58, `minix3/sys/netinet/in.h:98`).
pub const IPPROTO_ICMPV6: i32 = 58;

/// Largest raw payload in bytes (`RAW_MAX_PAYLOAD`, `rawsock.c:48`).
pub const MAX_PAYLOAD: usize = 65535;

/// Default multicast time to live for a new raw socket (`rawsock.c:319`).
pub const MULTICAST_TTL_DEFAULT: u8 = 1;

/// Whether multicast loopback is enabled by default (`rawsock.c:322`).
pub const MULTICAST_LOOP_DEFAULT: bool = true;

/// Whether a protocol value is accepted for a raw socket
/// (`rawsock_socket`, `rawsock.c:290-299`).
///
/// Unlike datagrams, raw sockets accept any 8-bit protocol number, from 0
/// through 255. Values outside that range have no meaning on the wire.
pub fn protocol_allowed(protocol: i32) -> bool {
    (0..=255).contains(&protocol)
}

/// Whether raw socket creation requires the superuser.
///
/// Always true. The allocator enforces it before dispatching
/// (`alloc_socket`, `lwip.c:169-170`: when the type is raw and the caller is
/// not the root user, creation fails with access denied). Keeping the rule
/// in one named function documents the security invariant where reviewers
/// expect it, instead of hiding it in a comment at the call site.
pub fn creation_requires_root() -> bool {
    true
}

/// Whether checksum handling is mandatory for a raw socket
/// (`rawsock_socket`, `rawsock.c:329-340`).
///
/// For version 6 control messages, checksum generation and verification are
/// mandatory and incoming type filtering is enabled. For every other
/// protocol, including version 4 control messages, checksumming stays
/// optional and the caller may enable it later.
pub fn checksum_mandatory(is_version6: bool, protocol: i32) -> bool {
    is_version6 && protocol == IPPROTO_ICMPV6
}

/// Whether a send flag set passes (`rawsock_pre_send`, `rawsock.c:463-464`,
/// identical to the datagram rule).
pub fn send_flags_allowed(flags: u32) -> bool {
    flags & !crate::udpsock::MSG_DONTROUTE == 0
}

/// Whether header plus payload fits the largest payload
/// (`rawsock_send`, `rawsock.c:704-713`).
pub fn payload_fits(header_length: usize, payload_length: usize) -> bool {
    header_length.saturating_add(payload_length) <= MAX_PAYLOAD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_accepts_full_8_bit_range() {
        assert!(protocol_allowed(0));
        assert!(protocol_allowed(1));
        assert!(protocol_allowed(58));
        assert!(protocol_allowed(255));
        assert!(!protocol_allowed(-1));
        assert!(!protocol_allowed(256));
    }

    #[test]
    fn test_creation_always_requires_superuser() {
        assert!(creation_requires_root());
    }

    #[test]
    fn test_checksum_mandatory_only_for_version6_control() {
        assert_eq!(IPPROTO_ICMPV6, 58);
        assert!(checksum_mandatory(true, 58));
        assert!(!checksum_mandatory(true, 17));
        assert!(!checksum_mandatory(false, 58));
        assert!(!checksum_mandatory(false, 1));
    }

    #[test]
    fn test_send_flags_match_datagram_rule() {
        assert!(send_flags_allowed(0));
        assert!(send_flags_allowed(crate::udpsock::MSG_DONTROUTE));
        assert!(!send_flags_allowed(0x02));
    }

    #[test]
    fn test_payload_bound_matches_raw_max() {
        assert_eq!(MAX_PAYLOAD, 65535);
        assert!(payload_fits(20, 65515));
        assert!(!payload_fits(20, 65516));
    }
}
