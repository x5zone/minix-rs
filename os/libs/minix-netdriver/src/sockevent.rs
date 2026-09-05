//! Socket event objects: event masks, flags, hash slots.
//!
//! C correspondence: the event mask bits (`SEV_BIND/CONNECT/ACCEPT/
//! SEND/RECV/CLOSE`, `0x01` through `0x20`, `sockevent.h:7-12`), the
//! socket flags (`SFL_SHUT_RD/WR/CLOSING/CLONED/TIMER`, `0x01`
//! through `0x10`, `sockevent.h:15-19`), and the hash rule (256
//! slots, slot equals identifier plus identifier shifted right by
//! sixteen, modulo slots, `sockevent.c:12-14,48-57`; the high half
//! carries the socket class so the first sockets of two classes do
//! not collide, `sockevent.c:51-56`).
//!
//! Object storage stays in the service binary; this module owns the
//! pure naming half: which bit means what and which slot an
//! identifier lands in.

/// Hash slots (`SOCKHASH_SLOTS`).
pub const HASH_SLOTS: u32 = 256;

/// Events one socket can raise (`SEV_*`, `sockevent.h:7-12`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SocketEvent {
    /// Bound to an address (`SEV_BIND`, 0x01).
    Bind = 0x01,
    /// Connected to a peer (`SEV_CONNECT`, 0x02).
    Connect = 0x02,
    /// Peer waiting to accept (`SEV_ACCEPT`, 0x04).
    Accept = 0x04,
    /// Ready to send (`SEV_SEND`, 0x08).
    Send = 0x08,
    /// Data waiting to receive (`SEV_RECV`, 0x10).
    Receive = 0x10,
    /// Closed by the peer or locally (`SEV_CLOSE`, 0x20).
    Close = 0x20,
}

/// Socket state flags (`SFL_*`, `sockevent.h:15-19`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SocketFlag {
    /// Read direction shut down (`SFL_SHUT_RD`, 0x01).
    ShutRead = 0x01,
    /// Write direction shut down (`SFL_SHUT_WR`, 0x02).
    ShutWrite = 0x02,
    /// Close in progress (`SFL_CLOSING`, 0x04).
    Closing = 0x04,
    /// Cloned from a listening socket (`SFL_CLONED`, 0x08).
    Cloned = 0x08,
    /// Timer armed (`SFL_TIMER`, 0x10).
    Timer = 0x10,
}

/// Hash slot of one socket identifier (`sockhash_slot`,
/// `sockevent.c:48-57`).
pub fn hash_slot(id: u32) -> u32 {
    (id + (id >> 16)) % HASH_SLOTS
}

/// Whether an event set contains one event.
pub fn has_event(set: u32, event: SocketEvent) -> bool {
    set & event as u32 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bits_match_header() {
        assert_eq!(SocketEvent::Bind as u32, 0x01);
        assert_eq!(SocketEvent::Connect as u32, 0x02);
        assert_eq!(SocketEvent::Accept as u32, 0x04);
        assert_eq!(SocketEvent::Send as u32, 0x08);
        assert_eq!(SocketEvent::Receive as u32, 0x10);
        assert_eq!(SocketEvent::Close as u32, 0x20);
    }

    #[test]
    fn test_flag_bits_match_header() {
        assert_eq!(SocketFlag::ShutRead as u32, 0x01);
        assert_eq!(SocketFlag::ShutWrite as u32, 0x02);
        assert_eq!(SocketFlag::Closing as u32, 0x04);
        assert_eq!(SocketFlag::Cloned as u32, 0x08);
        assert_eq!(SocketFlag::Timer as u32, 0x10);
    }

    #[test]
    fn test_hash_spreads_classes_apart() {
        assert_eq!(HASH_SLOTS, 256);
        assert_eq!(hash_slot(0), 0);
        assert_eq!(hash_slot(1), 1);
        assert_eq!(hash_slot(256), 0);
        assert_ne!(hash_slot(1), hash_slot(1 + (1 << 16)));
    }

    #[test]
    fn test_event_set_membership() {
        let set = SocketEvent::Send as u32 | SocketEvent::Receive as u32;
        assert!(has_event(set, SocketEvent::Send));
        assert!(!has_event(set, SocketEvent::Close));
    }
}
