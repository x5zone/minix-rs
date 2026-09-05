//! Network request protocol: numbers, modes, capabilities, and addresses.
//!
//! C correspondence: `minix3/minix/include/minix/com.h:1085-1144` (request
//! and reply numbers, configuration subtypes, mode bits, capability bits,
//! flag bits, link states) and `minix3/minix/include/minix/config.h:102-104`
//! (name, hardware address, and vector sizing).

/// Base of the network request range (stack to driver).
///
/// C: `NDEV_RQ_BASE 0x1A00` (`com.h:1085`).
pub const NDEV_REQUEST_BASE: i32 = 0x1A00;

/// Base of the network reply range (driver to stack).
///
/// C: `NDEV_RS_BASE 0x1A80` (`com.h:1086`).
pub const NDEV_REPLY_BASE: i32 = 0x1A80;

/// Maximum driver name length including the trailing zero.
///
/// C: `NDEV_NAME_MAX 16` (`config.h:102`).
pub const NDEV_NAME_MAX: usize = 16;

/// Maximum hardware address length in bytes.
///
/// C: `NDEV_HWADDR_MAX 6` (`config.h:103`): six bytes cover an Ethernet
/// address, the only wire type Minix3 network drivers serve.
pub const NDEV_HWADDR_MAX: usize = 6;

/// Maximum elements in one input-output vector.
///
/// C: `NDEV_IOV_MAX 8` (`config.h:104`).
pub const NDEV_IOV_MAX: usize = 8;

/// Maximum queued outgoing packets.
///
/// C: `NETDRIVER_SENDQ_MAX 8` (`netdriver.c:34`).
pub const SEND_QUEUE_BOUND: usize = 8;

/// Maximum queued incoming packets.
///
/// C: `NETDRIVER_RECVQ_MAX 2` (`netdriver.c:38`).
pub const RECV_QUEUE_BOUND: usize = 2;

/// Maximum multicast addresses copied in from the stack per configuration.
///
/// C: `NETDRIVER_MCAST_MAX 16` (`netdriver.c:46`). When the stack offers
/// more, the driver is told to receive all multicast packets instead.
pub const MULTICAST_LIST_MAX: usize = 16;

/// Configuration subtype: set mode and multicast list.
///
/// C: `NDEV_SET_MODE 0x01` (`com.h:1111`).
pub const NDEV_SET_MODE: u32 = 0x01;
/// Configuration subtype: enable or disable capabilities.
///
/// C: `NDEV_SET_CAPS 0x02` (`com.h:1112`).
pub const NDEV_SET_CAPS: u32 = 0x02;
/// Configuration subtype: set driver-specific flags.
///
/// C: `NDEV_SET_FLAGS 0x04` (`com.h:1113`).
pub const NDEV_SET_FLAGS: u32 = 0x04;
/// Configuration subtype: set media type.
///
/// C: `NDEV_SET_MEDIA 0x08` (`com.h:1114`).
pub const NDEV_SET_MEDIA: u32 = 0x08;
/// Configuration subtype: change the hardware address.
///
/// C: `NDEV_SET_HWADDR 0x10` (`com.h:1115`).
pub const NDEV_SET_HWADDR: u32 = 0x10;

/// Mode bit: transmission and receipt disabled.
///
/// C: `NDEV_MODE_DOWN 0x00` (`com.h:1118`).
pub const NDEV_MODE_DOWN: u32 = 0x00;
/// Mode bit: receive unicast packets addressed to this card.
///
/// C: `NDEV_MODE_UP 0x01` (`com.h:1119`).
pub const NDEV_MODE_UP: u32 = 0x01;
/// Mode bit: receive broadcast packets.
///
/// C: `NDEV_MODE_BCAST 0x02` (`com.h:1120`).
pub const NDEV_MODE_BCAST: u32 = 0x02;
/// Mode bit: receive the listed multicast packets.
///
/// C: `NDEV_MODE_MCAST_LIST 0x04` (`com.h:1121`).
pub const NDEV_MODE_MCAST_LIST: u32 = 0x04;
/// Mode bit: receive all multicast packets.
///
/// C: `NDEV_MODE_MCAST_ALL 0x08` (`com.h:1122`).
pub const NDEV_MODE_MCAST_ALL: u32 = 0x08;
/// Mode bit: receive every packet on the wire.
///
/// C: `NDEV_MODE_PROMISC 0x10` (`com.h:1123`).
pub const NDEV_MODE_PROMISC: u32 = 0x10;

/// Link state: unknown, assumed up.
///
/// C: `NDEV_LINK_UNKNOWN 0` (`com.h:1143`).
pub const NDEV_LINK_UNKNOWN: u32 = 0;
/// Link state: link is up.
///
/// C: `NDEV_LINK_UP 1` (`com.h:1144`).
pub const NDEV_LINK_UP: u32 = 1;

/// Network request kind, one variant per request number.
///
/// C: `NDEV_INIT` through `NDEV_STATUS_REPLY` (`com.h:1096-1101`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NdevRequest {
    /// Initialize the driver; also resets peer expectations after a stack
    /// restart (`NDEV_INIT`, base plus zero).
    Init,
    /// Configure mode, capabilities, flags, media, or address (`NDEV_CONF`).
    Configure,
    /// Hand one outgoing packet to the card (`NDEV_SEND`).
    Send,
    /// Offer one incoming buffer to the card (`NDEV_RECV`).
    Receive,
    /// Reserved control slot (`NDEV_IOCTL`).
    Ioctl,
    /// Acknowledge a status report (`NDEV_STATUS_REPLY`).
    StatusReply,
}

impl NdevRequest {
    /// Small index of the request (zero through five).
    pub const fn index(self) -> i32 {
        match self {
            NdevRequest::Init => 0,
            NdevRequest::Configure => 1,
            NdevRequest::Send => 2,
            NdevRequest::Receive => 3,
            NdevRequest::Ioctl => 4,
            NdevRequest::StatusReply => 5,
        }
    }

    /// Full message type of the request (base plus index).
    pub const fn message_type(self) -> i32 {
        NDEV_REQUEST_BASE + self.index()
    }

    /// Decode a raw message type; `None` means "not a network request".
    pub const fn decode(message_type: i32) -> Option<NdevRequest> {
        match message_type - NDEV_REQUEST_BASE {
            0 => Some(NdevRequest::Init),
            1 => Some(NdevRequest::Configure),
            2 => Some(NdevRequest::Send),
            3 => Some(NdevRequest::Receive),
            4 => Some(NdevRequest::Ioctl),
            5 => Some(NdevRequest::StatusReply),
            _ => None,
        }
    }
}

/// Returns true when a raw message type is a network request.
///
/// C: `IS_NDEV_RQ(type)` (`com.h:1088`).
pub const fn is_net_request(message_type: i32) -> bool {
    (message_type & !0x7f) == NDEV_REQUEST_BASE
}

/// Hardware address of a network card (Ethernet: six bytes).
///
/// C: `netdriver_addr_t` (`netdriver.h:18-20`): a byte array of
/// `NDEV_HWADDR_MAX` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HardwareAddress {
    /// Address bytes, significant prefix first.
    pub bytes: [u8; NDEV_HWADDR_MAX],
    /// Number of significant bytes (at most six).
    pub len: usize,
}

impl HardwareAddress {
    /// All-zero address of the given length.
    pub const fn zero(len: usize) -> HardwareAddress {
        HardwareAddress {
            bytes: [0; NDEV_HWADDR_MAX],
            len: if len > NDEV_HWADDR_MAX {
                NDEV_HWADDR_MAX
            } else {
                len
            },
        }
    }

    /// True when every significant byte is zero.
    pub fn is_zero(&self) -> bool {
        self.bytes[..self.len].iter().all(|byte| *byte == 0)
    }
}

/// Error counters reported to the stack in status reports.
///
/// C: `stat_oerror`, `stat_coll`, `stat_ierror`, `stat_iqdrop`
/// (`netdriver.c:45`) with the four `netdriver_stat_*` accumulators. Zero
/// reports are ignored in C (`if (count == 0) return`); [`NetStats::add`]
/// keeps that rule so callers cannot dirty the pending flag with empties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NetStats {
    /// Output errors on the wire.
    pub output_errors: u32,
    /// Packet collisions observed.
    pub collisions: u32,
    /// Input errors on the wire.
    pub input_errors: u32,
    /// Incoming packets dropped for lack of buffers.
    pub input_drops: u32,
}

/// Which counter a report increments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatKind {
    /// Output error (`netdriver_stat_oerror`).
    OutputError,
    /// Collision (`netdriver_stat_coll`).
    Collision,
    /// Input error (`netdriver_stat_ierror`).
    InputError,
    /// Input queue drop (`netdriver_stat_iqdrop`).
    InputDrop,
}

impl NetStats {
    /// Add a report; returns false (and changes nothing) for a zero count.
    pub fn add(&mut self, kind: StatKind, count: u32) -> bool {
        if count == 0 {
            return false;
        }
        match kind {
            StatKind::OutputError => self.output_errors = self.output_errors.saturating_add(count),
            StatKind::Collision => self.collisions = self.collisions.saturating_add(count),
            StatKind::InputError => self.input_errors = self.input_errors.saturating_add(count),
            StatKind::InputDrop => self.input_drops = self.input_drops.saturating_add(count),
        }
        true
    }

    /// Sum of all four counters.
    pub fn total(&self) -> u64 {
        self.output_errors as u64
            + self.collisions as u64
            + self.input_errors as u64
            + self.input_drops as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_indices_match_c_offsets() {
        assert_eq!(NdevRequest::Init.message_type(), 0x1A00);
        assert_eq!(NdevRequest::Configure.message_type(), 0x1A01);
        assert_eq!(NdevRequest::Send.message_type(), 0x1A02);
        assert_eq!(NdevRequest::Receive.message_type(), 0x1A03);
        assert_eq!(NdevRequest::Ioctl.message_type(), 0x1A04);
        assert_eq!(NdevRequest::StatusReply.message_type(), 0x1A05);
    }

    #[test]
    fn test_decode_round_trips_all_six_requests() {
        let all = [
            NdevRequest::Init,
            NdevRequest::Configure,
            NdevRequest::Send,
            NdevRequest::Receive,
            NdevRequest::Ioctl,
            NdevRequest::StatusReply,
        ];
        for request in all {
            assert_eq!(NdevRequest::decode(request.message_type()), Some(request));
        }
    }

    #[test]
    fn test_decode_rejects_neighbor_ranges() {
        assert_eq!(NdevRequest::decode(0x1A06), None);
        assert_eq!(NdevRequest::decode(0x19FF), None);
        assert_eq!(NdevRequest::decode(0x400), None);
    }

    #[test]
    fn test_sizing_constants_match_c_headers() {
        assert_eq!(NDEV_NAME_MAX, 16);
        assert_eq!(NDEV_HWADDR_MAX, 6);
        assert_eq!(NDEV_IOV_MAX, 8);
        assert_eq!(SEND_QUEUE_BOUND, 8);
        assert_eq!(RECV_QUEUE_BOUND, 2);
        assert_eq!(MULTICAST_LIST_MAX, 16);
    }

    #[test]
    fn test_zero_count_reports_change_nothing() {
        let mut stats = NetStats::default();
        assert!(!stats.add(StatKind::OutputError, 0));
        assert_eq!(stats.total(), 0);
        assert!(stats.add(StatKind::Collision, 3));
        assert!(stats.add(StatKind::InputDrop, 2));
        assert_eq!(stats.total(), 5);
    }

    #[test]
    fn test_hardware_address_zero_check() {
        let zero = HardwareAddress::zero(6);
        assert!(zero.is_zero());
        let mut addr = HardwareAddress::zero(6);
        addr.bytes[5] = 1;
        assert!(!addr.is_zero());
    }
}
