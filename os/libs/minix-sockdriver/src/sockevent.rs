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

bitflags::bitflags! {
    /// Events one socket can raise (`SEV_*`, `sockevent.h:7-12`).
    ///
    /// A bitflag struct rather than an enum because events combine into
    /// masks (raise sets, continuations wake on masks); the distinct type
    /// keeps event bits from mixing with flag bits or raw wire numbers at
    /// compile time.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SocketEvent: u32 {
        /// Bound to an address (`SEV_BIND`, 0x01).
        const Bind = 0x01;
        /// Connected to a peer (`SEV_CONNECT`, 0x02).
        const Connect = 0x02;
        /// Peer waiting to accept (`SEV_ACCEPT`, 0x04).
        const Accept = 0x04;
        /// Ready to send (`SEV_SEND`, 0x08).
        const Send = 0x08;
        /// Data waiting to receive (`SEV_RECV`, 0x10).
        const Receive = 0x10;
        /// Closed by the peer or locally (`SEV_CLOSE`, 0x20).
        const Close = 0x20;
    }
}

bitflags::bitflags! {
    /// Socket state flags (`SFL_*`, `sockevent.h:15-19`), distinct from
    /// [`SocketEvent`] so the two mask families cannot mix.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SocketFlag: u32 {
        /// Read direction shut down (`SFL_SHUT_RD`, 0x01).
        const ShutRead = 0x01;
        /// Write direction shut down (`SFL_SHUT_WR`, 0x02).
        const ShutWrite = 0x02;
        /// Close in progress (`SFL_CLOSING`, 0x04).
        const Closing = 0x04;
        /// Cloned from a listening socket (`SFL_CLONED`, 0x08).
        const Cloned = 0x08;
        /// Timer armed (`SFL_TIMER`, 0x10).
        const Timer = 0x10;
    }
}

/// Hash slot of one socket identifier (`sockhash_slot`,
/// `sockevent.c:48-57`).
pub fn hash_slot(id: u32) -> u32 {
    (id + (id >> 16)) % HASH_SLOTS
}

/// Whether an event set contains one event.
pub fn has_event(set: u32, event: SocketEvent) -> bool {
    set & event.bits() != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_bits_match_header() {
        assert_eq!(SocketEvent::Bind.bits(), 0x01);
        assert_eq!(SocketEvent::Connect.bits(), 0x02);
        assert_eq!(SocketEvent::Accept.bits(), 0x04);
        assert_eq!(SocketEvent::Send.bits(), 0x08);
        assert_eq!(SocketEvent::Receive.bits(), 0x10);
        assert_eq!(SocketEvent::Close.bits(), 0x20);
    }

    #[test]
    fn test_flag_bits_match_header() {
        assert_eq!(SocketFlag::ShutRead.bits(), 0x01);
        assert_eq!(SocketFlag::ShutWrite.bits(), 0x02);
        assert_eq!(SocketFlag::Closing.bits(), 0x04);
        assert_eq!(SocketFlag::Cloned.bits(), 0x08);
        assert_eq!(SocketFlag::Timer.bits(), 0x10);
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
        let set = SocketEvent::Send.bits() | SocketEvent::Receive.bits();
        assert!(has_event(set, SocketEvent::Send));
        assert!(!has_event(set, SocketEvent::Close));
    }
}
