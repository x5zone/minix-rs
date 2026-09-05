//! Link-layer socket policy: creation guards and address sizing.
//!
//! C correspondence: `minix3/minix/net/lwip/lnksock.c` (77 lines) with the
//! link-layer routing data in `minix3/minix/net/lwip/lldata.c` (584 lines)
//! and the address layout in `minix3/minix/net/lwip/lwip.h:26-47`. Socket
//! storage, routing tables, and neighbor caches stay in the service binary.
//! This module owns the portion that can be decided from numbers alone:
//! which type and protocol values a link socket accepts, how many such
//! sockets can exist, and how large a link address may be.
//!
//! Link sockets exist for one narrow purpose: giving configuration tools a
//! way to issue interface control operations. They carry no packet traffic
//! of their own, which is why the creation rules are deliberately strict.

/// Number of link sockets (`NR_LNKSOCK`, `lnksock.c:11`).
pub const MAX_LINK_SOCKETS: usize = 4;

/// Socket type for datagrams (`SOCK_DGRAM`, 2, `minix3/sys/sys/socket.h:106`).
pub const SOCK_DGRAM: i32 = 2;

/// Largest interface name length, including the terminating zero, used when
/// sizing link addresses (`IFNAMSIZ`, 16 on the reference system).
pub const INTERFACE_NAME_MAX: usize = 16;

/// Largest hardware address length accepted in a link address
/// (`NETIF_MAX_HWADDR_LEN`, 6 for Ethernet-class interfaces).
pub const HARDWARE_ADDRESS_MAX: usize = 6;

/// Whether a socket type value is accepted for a link socket
/// (`lnksock_socket`, `lnksock.c:46-47`).
///
/// Only datagrams pass. Streams and raw sockets have no meaning at the link
/// layer in this service.
pub fn socket_type_allowed(socket_type: i32) -> bool {
    socket_type == SOCK_DGRAM
}

/// Whether a protocol value is accepted for a link socket
/// (`lnksock_socket`, `lnksock.c:49-50`).
///
/// Only the wildcard 0 passes. Link sockets do not multiplex by protocol
/// number; they exist to carry control operations.
pub fn protocol_allowed(protocol: i32) -> bool {
    protocol == 0
}

/// Whether one more link socket can be created given the live count
/// (`lnksock_socket`, `lnksock.c:52-53`, failing with no-buffer-space when
/// the free list is empty).
pub fn creation_allowed(live_sockets: usize) -> bool {
    live_sockets < MAX_LINK_SOCKETS
}

/// Length of a link address carrying `name_length` name bytes and
/// `hardware_length` hardware bytes, or `None` when either part exceeds its
/// cap (`addr_put_link`, `addr.c:360-395`, with the header size from
/// `sockaddr_dlx`).
///
/// The header itself is 16 bytes on the reference layout; the total is the
/// header plus both parts. The service binary enforces the same caps before
/// copying to user memory; this helper lets callers pre-validate lengths.
pub fn link_address_length(name_length: usize, hardware_length: usize) -> Option<usize> {
    const HEADER_LENGTH: usize = 16;
    if name_length >= INTERFACE_NAME_MAX {
        return None;
    }
    if hardware_length > HARDWARE_ADDRESS_MAX {
        return None;
    }
    Some(HEADER_LENGTH + name_length + hardware_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation_guards_match_link_source() {
        assert_eq!(MAX_LINK_SOCKETS, 4);
        assert_eq!(SOCK_DGRAM, 2);
        assert!(socket_type_allowed(2));
        assert!(!socket_type_allowed(1));
        assert!(!socket_type_allowed(3));
        assert!(protocol_allowed(0));
        assert!(!protocol_allowed(1));
        assert!(!protocol_allowed(17));
    }

    #[test]
    fn test_capacity_is_four_sockets() {
        assert!(creation_allowed(0));
        assert!(creation_allowed(3));
        assert!(!creation_allowed(4));
        assert!(!creation_allowed(5));
    }

    #[test]
    fn test_link_address_sizing_enforces_caps() {
        assert_eq!(link_address_length(5, 6), Some(27));
        assert_eq!(link_address_length(0, 0), Some(16));
        assert_eq!(link_address_length(16, 0), None);
        assert_eq!(link_address_length(0, 7), None);
    }
}
