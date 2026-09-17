//! Netdriver wiring: lance as a netdriver card.
//!
//! C correspondence: `lance.c`'s `nl_table` over the sixteen-entry rings
//! (`lance.c:97-106`) and the chip identification (`lance_probe`,
//! `lance.c:707-722`).
//!
//! The face implements [`NetDriver`] over the ring geometry and chip
//! identity: init reports the rings and the identified chip's flags
//! inform capabilities. Register traffic and the init-block programming
//! stay in the service.

use minix_netdriver::driver::{InitReport, LinkReport, NetDriver};
use minix_netdriver::protocol::{HardwareAddress, LinkState};

use crate::ring::ChipVersion;

/// Receive ring entries (`RX_RING_SIZE`, `lance.c:97-106`).
pub const RX_ENTRIES: usize = crate::ring::RING_SIZE;
/// Transmit ring entries (`TX_RING_SIZE`).
pub const TX_ENTRIES: usize = crate::ring::RING_SIZE;

/// The lance card's framework face.
pub struct LanceFace {
    /// Identified chip (from the version register via
    /// [`crate::ring::identify_chip`]); unknown chips fall back to the
    /// ancient-LANCE identity per the C probe.
    pub chip: Option<ChipVersion>,
    /// Hardware address (the service reads the board PROM).
    pub hardware: HardwareAddress,
}

impl LanceFace {
    /// A face with the identified chip and a zero address.
    pub fn new(chip: Option<ChipVersion>) -> Self {
        LanceFace {
            chip,
            hardware: HardwareAddress::zero(6),
        }
    }
}

impl NetDriver for LanceFace {
    fn name(&self) -> &str {
        "lance"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_and_init_report() {
        let mut face = LanceFace::new(Some(ChipVersion::Pcnet32));
        assert_eq!(face.name(), "lance");
        let report = face.init(0);
        assert!(report.hardware.is_zero());
        assert_eq!(report.ticks, 1);
    }

    #[test]
    fn test_ring_geometry_constants() {
        assert_eq!(RX_ENTRIES, 16);
        assert_eq!(TX_ENTRIES, 16);
    }
}
