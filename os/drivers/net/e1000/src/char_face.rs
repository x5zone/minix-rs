//! Netdriver wiring: e1000 as a netdriver card.
//!
//! C correspondence: `e1000.c`'s `e1000_table` (`e1000.c:38-49`) — nine
//! callbacks over the descriptor rings (`e1000.h:29-35`: 256 descriptors
//! of 2048 bytes each, receive and transmit).
//!
//! The face implements [`NetDriver`] over the descriptor-ring counters:
//! init reports the ring geometry, recv/send admit within the ring
//! bounds. Register traffic and descriptor programming stay in the
//! service.

use minix_netdriver::driver::{InitReport, LinkReport, NetDriver};
use minix_netdriver::protocol::{HardwareAddress, LinkState};

use crate::desc::DescRing;

/// Receive descriptors (`E1000_RX_DESCS`, `e1000.h:29`).
pub const RX_DESCS: u32 = 256;
/// Transmit descriptors (`E1000_TX_DESCS`, `e1000.h:30`).
pub const TX_DESCS: u32 = 256;
/// Buffer size per descriptor (`e1000.h:35`).
pub const BUFFER_SIZE: u32 = 2048;

/// The e1000 card's framework face.
pub struct E1000Face {
    /// Receive descriptor ring counter.
    pub rx: DescRing,
    /// Transmit descriptor ring counter.
    pub tx: DescRing,
    /// Hardware address (the service reads the EEPROM).
    pub hardware: HardwareAddress,
}

impl E1000Face {
    /// A face with fresh rings and a zero address.
    pub fn new() -> Self {
        E1000Face {
            rx: DescRing::receive(),
            tx: DescRing::transmit(),
            hardware: HardwareAddress::zero(6),
        }
    }
}

impl Default for E1000Face {
    fn default() -> Self {
        Self::new()
    }
}

impl NetDriver for E1000Face {
    fn name(&self) -> &str {
        "e1000"
    }

    fn init(&mut self, _instance: u32) -> InitReport {
        InitReport {
            hardware: self.hardware,
            capabilities: 0,
            ticks: 1,
        }
    }

    fn link(&mut self) -> LinkReport {
        LinkReport { link: LinkState::Unknown, media: 0 }
    }
}

impl E1000Face {
    /// Whether a receive at this many consumed bytes still fits the ring.
    pub fn rx_admits(&self, consumed_bytes: u64) -> bool {
        (consumed_bytes / BUFFER_SIZE as u64) < RX_DESCS as u64
    }

    /// Whether a transmit of this many descriptors still fits the ring.
    pub fn tx_admits(&self, descriptors: u64) -> bool {
        descriptors <= TX_DESCS as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_and_init_report() {
        let mut face = E1000Face::new();
        assert_eq!(face.name(), "e1000");
        let report = face.init(0);
        assert!(report.hardware.is_zero());
        assert_eq!(report.ticks, 1);
    }

    #[test]
    fn test_rx_and_tx_admission_bounds() {
        let face = E1000Face::new();
        assert!(face.rx_admits(0));
        assert!(face.rx_admits(255 * 2048));
        assert!(!face.rx_admits(256 * 2048));
        assert!(face.tx_admits(256));
        assert!(!face.tx_admits(257));
    }
}
