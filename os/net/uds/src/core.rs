//! UNIX-domain service core policy: object count, states, hash, dispatch.
//!
//! C correspondence: `minix3/minix/net/uds/uds.c` (1417 lines) with the
//! object layout in `minix3/minix/net/uds/uds.h` (NR_UDSSOCK, UDSHASH_SLOTS,
//! UDS_BUF, UDS_CTL_MAX) and the status surface in
//! `minix3/minix/net/uds/stat.c` (186 lines). Socket storage, message
//! traffic, and the file-to-socket table stay in the service binary. This
//! module owns the portion that can be decided from numbers alone: how many
//! sockets exist, which states a connection socket can be in, how hash slots
//! are chosen, which domain and types pass, and when the main loop keeps
//! running.
//!
//! The state machine and the waiting-connection behavior are documented in
//! the header at `uds.h:86-135`; the comments below quote the relevant
//! lines so reviewers can check each rule without opening the C source.

/// Largest UNIX-domain sockets (`NR_UDSSOCK`, 256, `uds.h:15`; control
/// structures are static, each receive buffer is mapped only while in use).
pub const MAX_SOCKETS: usize = 256;

/// Slots in the device-and-inode hash table (`UDSHASH_SLOTS`, 64,
/// `uds.h:18`).
pub const HASH_SLOTS: usize = 64;

/// The only accepted protocol value (`UDSPROTO_UDS`, 0, `uds.h:21`; the
/// local domain has no protocols).
pub const PROTOCOL_UDS: i32 = 0;

/// Connection-socket states (`uds.h:86-105`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Freshly created or failed, no peer, not listening.
    Unconnected,
    /// Listening mode, has a queue of unaccepted sockets, no peer.
    Listening,
    /// Sitting on a listening socket queue, linked but not connected.
    Connecting,
    /// Connected to a peer socket, symmetric peer relationship.
    Connected,
    /// Formerly connected, peer closed, peer gone but flag still set.
    Disconnected,
}

/// All five states in header order.
pub const ALL_STATES: [ConnectionState; 5] = [
    ConnectionState::Unconnected,
    ConnectionState::Listening,
    ConnectionState::Connecting,
    ConnectionState::Connected,
    ConnectionState::Disconnected,
];

/// Hash slot for a device-and-inode pair (`udshash_slot`, `uds.c:30-33`:
/// the slot mixes both numbers; the exact mixing is service detail, but the
/// slot count bound is policy).
pub fn hash_slot(device: u64, inode: u64) -> usize {
    ((device ^ inode) % HASH_SLOTS as u64) as usize
}

/// Whether a domain value is accepted (`uds_socket`, `uds.c:230-236`:
/// only the local domain passes, anything else means misconfiguration).
pub fn domain_allowed(is_unix_domain: bool) -> bool {
    is_unix_domain
}

/// Whether a socket type value is accepted (`uds_socket`, `uds.c:239-248`:
/// streams, sequenced packets, and datagrams pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    /// Ordered reliable byte stream.
    Stream,
    /// Ordered reliable sequenced packets.
    SequencePacket,
    /// Unordered unreliable datagrams.
    Datagram,
}

/// Whether the main loop keeps running (`main`, `uds.c:1391`: while the
/// service is marked running or any socket remains in use).
pub fn loop_keeps_running(service_running: bool, sockets_in_use: usize) -> bool {
    service_running || sockets_in_use > 0
}

/// Whether a termination signal stops accepting new work but waits for
/// drain (`uds_signal`, `uds.c:1349-1361`: termination clears the running
/// flag; the loop exits once the use count reaches zero).
pub fn shutdown_waits_for_drain(sockets_in_use: usize) -> bool {
    sockets_in_use > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limits_match_header() {
        assert_eq!(MAX_SOCKETS, 256);
        assert_eq!(HASH_SLOTS, 64);
        assert_eq!(PROTOCOL_UDS, 0);
        assert!(hash_slot(0, 0) < HASH_SLOTS);
        assert!(hash_slot(u64::MAX, u64::MAX) < HASH_SLOTS);
    }

    #[test]
    fn test_states_cover_header_list() {
        assert_eq!(ALL_STATES.len(), 5);
        assert_eq!(ALL_STATES[0], ConnectionState::Unconnected);
        assert_eq!(ALL_STATES[4], ConnectionState::Disconnected);
    }

    #[test]
    fn test_domain_and_types_match_dispatch() {
        assert!(domain_allowed(true));
        assert!(!domain_allowed(false));
        let types = [
            SocketType::Stream,
            SocketType::SequencePacket,
            SocketType::Datagram,
        ];
        assert_eq!(types.len(), 3);
    }

    #[test]
    fn test_loop_runs_while_work_remains() {
        assert!(loop_keeps_running(true, 0));
        assert!(loop_keeps_running(false, 1));
        assert!(!loop_keeps_running(false, 0));
        assert!(shutdown_waits_for_drain(2));
        assert!(!shutdown_waits_for_drain(0));
    }
}
