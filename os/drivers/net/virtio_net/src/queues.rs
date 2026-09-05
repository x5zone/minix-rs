//! Queue roles and refill discipline: receive, transmit, control.
//!
//! C correspondence: the three queues (`RX_Q`, `TX_Q`, `CTRL_Q`,
//! `virtio_net.c:36`), the packet ceiling (`MAX_PACK_SIZE` from
//! `NDEV_ETH_PACKET_MAX`, `virtio_net.c:44`), the refill rule (keep
//! handing empty buffers to the receive queue while fewer than half
//! the buffers are outstanding, `refill_rx`, `virtio_net.c:216-243`),
//! and the minimum pad (pad short frames up to the Ethernet minimum,
//! `virtio_net.c:369-370`).
//!
//! Queue traffic stays in the service binary; this module owns the
//! pure discipline half: which queue does what and when to refill.

/// Which virtual queue a buffer travels on (`virtio_net.c:36`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetQueue {
    /// Incoming packets.
    Receive,
    /// Outgoing packets.
    Transmit,
    /// Control commands (only when negotiated).
    Control,
}

/// Largest packet buffer in bytes (`MAX_PACK_SIZE`).
pub const MAX_PACKET_SIZE: usize = 1514;

/// Buffers the driver keeps outstanding (`BUF_PACKETS` scale).
pub const BUFFER_COUNT: u32 = 256;

/// Refill while fewer than this many buffers are outstanding (half
/// of `BUFFER_COUNT`, `virtio_net.c:239`).
pub const REFILL_THRESHOLD: u32 = BUFFER_COUNT / 2;

/// Pad short frames up to this length (`NDEV_ETH_PACKET_MIN`).
pub const MIN_FRAME_LEN: usize = 60;

/// Whether the receive queue needs more empty buffers
/// (`refill_rx`, `virtio_net.c:216-243`).
pub fn refill_needed(in_flight: u32) -> bool {
    in_flight < REFILL_THRESHOLD
}

/// Pad a frame length up to the Ethernet minimum
/// (`virtio_net.c:369-370`).
pub fn padded_length(length: usize) -> usize {
    length.max(MIN_FRAME_LEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queues_cover_three_roles() {
        let roles = [NetQueue::Receive, NetQueue::Transmit, NetQueue::Control];
        assert_eq!(roles.len(), 3);
        assert_eq!(MAX_PACKET_SIZE, 1514);
    }

    #[test]
    fn test_refill_below_half_buffers() {
        assert_eq!(REFILL_THRESHOLD, 128);
        assert!(refill_needed(0));
        assert!(refill_needed(127));
        assert!(!refill_needed(128));
        assert!(!refill_needed(256));
    }

    #[test]
    fn test_short_frames_pad_to_minimum() {
        assert_eq!(padded_length(0), MIN_FRAME_LEN);
        assert_eq!(padded_length(59), MIN_FRAME_LEN);
        assert_eq!(padded_length(60), 60);
        assert_eq!(padded_length(1514), 1514);
    }
}
