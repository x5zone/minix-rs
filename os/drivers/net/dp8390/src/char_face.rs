//! Netdriver wiring: dp8390 as a netdriver card.
//!
//! C correspondence: `dp8390.c`'s `ndr_*` table — the receive walk
//! (`do_recv`, `dp8390.c:598-720`) riding the ring cursor, the length
//! guard, and the init report with the hardware address read from the
//! PROM.
//!
//! The face implements [`NetDriver`] over the receive cursor: init
//! reports the ring geometry, `recv` gates packet lengths and advances
//! the boundary (G9 semantics: the stop-page special case included).
//! Port traffic and the PROM read stay in the service.

use minix_netdriver::driver::{LinkReport, NetDriver};
use minix_netdriver::protocol::{HardwareAddress, LinkState};

use crate::ring::RecvCursor;

/// The dp8390 card's framework face.
pub struct Dp8390Face {
    /// Receive ring cursor (the library's pure half).
    pub cursor: RecvCursor,
    /// Hardware address burned into the PROM (the service reads it).
    pub hardware: HardwareAddress,
    /// Maximum packet length the driver keeps.
    pub max_packet: u16,
}

/// DP8390 receive-ring geometry for QEMU's ne2000 at the classic pages.
const START_PAGE: u8 = 0x46;
const STOP_PAGE: u8 = 0x60;
/// Ethernet maximum frame (`NDEV_ETH_PACKET_MAX`).
const MAX_FRAME: u16 = 1514;

impl Dp8390Face {
    /// A face with the classic ring geometry and a zero address (the
    /// service fills the PROM bytes at init).
    pub fn new() -> Self {
        Dp8390Face {
            cursor: RecvCursor::new(START_PAGE, STOP_PAGE),
            hardware: HardwareAddress::zero(6),
            max_packet: MAX_FRAME,
        }
    }
}

impl Default for Dp8390Face {
    fn default() -> Self {
        Self::new()
    }
}

impl NetDriver for Dp8390Face {
    fn name(&self) -> &str {
        "dp8390"
    }

    fn init(&mut self, _instance: u32) -> minix_netdriver::driver::InitReport {
        minix_netdriver::driver::InitReport {
            hardware: self.hardware,
            capabilities: 0,
            ticks: 1, // polling cadence: the dp8390 has no link interrupt
        }
    }

    fn link(&mut self) -> LinkReport {
        // The dp8390 has no link-state register: unknown (assumed up).
        LinkReport { link: LinkState::Unknown, media: 0 }
    }
}

impl Dp8390Face {
    /// Whether a packet with this length is worth keeping
    /// (`length_allowed`, the runt/giant guard).
    pub fn length_ok(&self, length: u16) -> bool {
        crate::ring::length_allowed(length, self.max_packet)
    }

    /// Advance past a consumed packet ending at `next_page`
    /// (`dp8390.c:668-671`, the G9 stop-page special case included).
    pub fn advance(&mut self, next_page: u8) {
        self.cursor.advance(next_page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_and_init_report() {
        let mut face = Dp8390Face::new();
        assert_eq!(face.name(), "dp8390");
        let report = face.init(0);
        assert!(report.hardware.is_zero());
        assert_eq!(report.ticks, 1);
    }

    #[test]
    fn test_length_guard_uses_the_frame_bounds() {
        let face = Dp8390Face::new();
        assert!(face.length_ok(60));
        assert!(face.length_ok(1514));
        assert!(!face.length_ok(59));
        assert!(!face.length_ok(1515));
    }

    #[test]
    fn test_advance_wraps_at_start_page() {
        let mut face = Dp8390Face::new();
        face.advance(START_PAGE); // chain ends at the start page
        assert_eq!(face.cursor.boundary(), STOP_PAGE - 1);
        face.advance(0x47);
        assert_eq!(face.cursor.boundary(), 0x46);
    }
}
