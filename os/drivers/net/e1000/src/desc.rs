//! Descriptor rings: counts, buffer size, head and tail.
//!
//! C correspondence: the descriptor counts (`E1000_RXDESC_NR` and
//! `E1000_TXDESC_NR`, 256 each, `e1000.h:29-32`), the buffer size
//! (`E1000_IOBUF_SIZE`, 2048 bytes, `e1000.h:35`), and the receive and transmit
//! descriptor layouts (`e1000_rx_desc_t` and `e1000_tx_desc_t`,
//! `e1000_hw.h:31-46`).
//!
//! Register writes stay in the service binary; this module owns the
//! pure ring half: how many descriptors, how big each buffer, and
//! where head and tail sit.

/// Receive descriptors in the ring (`RXDESC_NR`).
pub const RX_DESC_COUNT: usize = 256;

/// Transmit descriptors in the ring (`TXDESC_NR`).
pub const TX_DESC_COUNT: usize = 256;

/// Bytes in one I/O buffer (`IOBUF`).
pub const BUFFER_SIZE: usize = 2048;

/// One descriptor ring: head where hardware writes, tail where the
/// driver has consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescRing {
    /// Descriptors in this ring.
    count: usize,
    /// Next descriptor the driver will consume.
    tail: usize,
}

impl DescRing {
    /// Receive ring with 256 descriptors.
    pub fn receive() -> Self {
        DescRing { count: RX_DESC_COUNT, tail: 0 }
    }

    /// Transmit ring with 256 descriptors.
    pub fn transmit() -> Self {
        DescRing { count: TX_DESC_COUNT, tail: 0 }
    }

    /// Current tail position.
    pub fn tail(&self) -> usize {
        self.tail
    }

    /// Advance past one consumed descriptor, wrapping at the end.
    pub fn advance(&mut self) {
        self.tail = (self.tail + 1) % self.count;
    }

    /// Whether the ring holds no unconsumed descriptor (tail caught
    /// up with head).
    pub fn is_empty(&self, head: usize) -> bool {
        head % self.count == self.tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_sizes_match_header() {
        assert_eq!(RX_DESC_COUNT, 256);
        assert_eq!(TX_DESC_COUNT, 256);
        assert_eq!(BUFFER_SIZE, 2048);
        assert_eq!(DescRing::receive().tail(), 0);
    }

    #[test]
    fn test_tail_wraps_at_ring_end() {
        let mut ring = DescRing::transmit();
        for _ in 0..255 {
            ring.advance();
        }
        assert_eq!(ring.tail(), 255);
        ring.advance();
        assert_eq!(ring.tail(), 0);
    }

    #[test]
    fn test_empty_when_tail_catches_head() {
        let mut ring = DescRing::receive();
        assert!(ring.is_empty(0));
        assert!(ring.is_empty(256));
        assert!(!ring.is_empty(1));
        ring.advance();
        assert!(ring.is_empty(1));
    }
}
