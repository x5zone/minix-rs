//! Transmit slots and receive ring: four slots, 64K wrap.
//!
//! C correspondence: the transmit slot count (`RL_N_TX`, 4,
//! `rtl8139.h:19`), the four transmit status registers (`TSD0`
//! plus slot times four, `rtl8139.h:20-34`, programmed at send time,
//! `rtl8139.c:711`), and the continuous 64K receive buffer
//! (`RX_BUFSIZE`, `rtl8139.h:435`, defined as
//! `RL_RCR_RBLEN_64K_SIZE`, i.e. 64 times 1024) with its two cursors (`CAPR`
//! and `CBR`, `rtl8139.h:46-48`) wrapping packet fetches
//! (`rtl8139.c:619-688`).
//!
//! Register writes stay in the service binary; this module owns the
//! pure cursor half: which transmit slot is next and where the
//! receive read cursor wraps.

/// Transmit slots (`RL_N_TX`).
pub const TX_SLOT_COUNT: usize = 4;

/// Receive buffer size in bytes (`RX_BUFSIZE`, 64K).
pub const RX_BUFFER_SIZE: usize = 65536;

/// Transmit slot picker: round-robin over four slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxSlots {
    /// Next slot to use.
    head: usize,
}

impl TxSlots {
    /// All slots free, starting at slot zero.
    pub fn new() -> Self {
        TxSlots { head: 0 }
    }

    /// Current head slot.
    pub fn head(&self) -> usize {
        self.head
    }

    /// Move to the next slot, wrapping after the fourth.
    pub fn advance(&mut self) {
        self.head = (self.head + 1) % TX_SLOT_COUNT;
    }
}

impl Default for TxSlots {
    fn default() -> Self {
        Self::new()
    }
}

/// Advance a receive read cursor by `consumed` bytes inside the 64K
/// buffer, wrapping at the end.
pub fn rx_advance(cursor: usize, consumed: usize) -> usize {
    (cursor + consumed) % RX_BUFFER_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_four_slots_round_robin() {
        let mut slots = TxSlots::new();
        assert_eq!(TX_SLOT_COUNT, 4);
        for expected in [1, 2, 3, 0, 1] {
            slots.advance();
            assert_eq!(slots.head(), expected);
        }
    }

    #[test]
    fn test_receive_cursor_wraps_at_64k() {
        assert_eq!(RX_BUFFER_SIZE, 65536);
        assert_eq!(rx_advance(0, 100), 100);
        assert_eq!(rx_advance(65500, 100), 64);
        assert_eq!(rx_advance(65535, 1), 0);
    }
}
