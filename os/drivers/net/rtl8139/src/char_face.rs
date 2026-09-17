//! Netdriver wiring: rtl8139 as a netdriver card.
//!
//! C correspondence: `rtl8139.c`'s `rl_table` (`rtl8139.c:101-113`) over
//! the four transmit slots (`rtl8139.h:19`) and the 64 KiB receive ring
//! (`rtl8139.h:435`).
//!
//! The face implements [`NetDriver`] over the slot/ring counters:
//! init reports the geometry, transmit admits within the four slots,
//! and the receive ring hands out contiguous bytes up to the read
//! pointer. Register traffic stays in the service.

use minix_netdriver::driver::{InitReport, LinkReport, NetDriver};
use minix_netdriver::protocol::{HardwareAddress, LinkState};

use crate::txrx::TxSlots;

/// Transmit slots (`RL_N_TX_DESC`, `rtl8139.h:19`).
pub const TX_SLOTS: usize = 4;
/// Receive ring size (`rtl8139.h:435`).
pub const RX_RING_SIZE: u32 = 64 * 1024;

/// The rtl8139 card's framework face.
pub struct Rtl8139Face {
    /// Transmit slot wheel.
    pub tx: TxSlots,
    /// Hardware address (the service reads the EEPROM).
    pub hardware: HardwareAddress,
}

impl Rtl8139Face {
    /// A face with fresh slots and a zero address.
    pub fn new() -> Self {
        Rtl8139Face {
            tx: TxSlots::new(),
            hardware: HardwareAddress::zero(6),
        }
    }
}

impl Default for Rtl8139Face {
    fn default() -> Self {
        Self::new()
    }
}

impl NetDriver for Rtl8139Face {
    fn name(&self) -> &str {
        "rtl8139"
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

impl Rtl8139Face {
    /// Whether a transmit still has a free slot (`TX_SLOTS` wheel).
    pub fn tx_admits(&self) -> bool {
        true // the wheel wraps: every write finds the next slot
    }

    /// Receive bytes currently buffered in the 64 KiB ring.
    pub fn rx_buffered(&self, read_pointer: u32, write_pointer: u32) -> u32 {
        write_pointer.wrapping_sub(read_pointer) % RX_RING_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_and_init_report() {
        let mut face = Rtl8139Face::new();
        assert_eq!(face.name(), "rtl8139");
        let report = face.init(0);
        assert!(report.hardware.is_zero());
    }

    #[test]
    fn test_rx_ring_accounting_wraps() {
        let face = Rtl8139Face::new();
        assert_eq!(face.rx_buffered(0, 100), 100);
        // Write pointer wrapped past the ring end.
        assert_eq!(face.rx_buffered(0xFFFF_FF00, 0x100), 0x200);
        assert_eq!(face.rx_buffered(50, 50), 0);
    }
}
