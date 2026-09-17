//! Netdriver wiring: virtio_net as a netdriver card.
//!
//! C correspondence: `virtio_net.c`'s five-callback table
//! (`virtio_net_table`, `virtio_net.c:92-99`: name, send, interrupt,
//! receive, no set-mode/get-link) plus the refill rule (`refill_rx`,
//! `virtio_net.c:216-243`) and the minimum pad (`virtio_net.c:369-370`).
//!
//! The face implements [`NetDriver`] over the refill discipline and the
//! three-queue topology; queue traffic stays with the service's virtio
//! transport.

use minix_netdriver::driver::{InitReport, LinkReport, NetDriver};
use minix_netdriver::protocol::{HardwareAddress, LinkState};

use crate::queues::{padded_length, refill_needed, NetQueue};

/// The virtio_net card's framework face.
pub struct VirtioNetFace {
    /// Hardware address the host reported at negotiation.
    pub hardware: HardwareAddress,
    /// Buffers outstanding in the receive queue.
    pub in_flight: u32,
}

impl VirtioNetFace {
    /// A face with the full buffer complement and a zero address.
    pub fn new() -> Self {
        VirtioNetFace {
            hardware: HardwareAddress::zero(6),
            in_flight: crate::queues::BUFFER_COUNT,
        }
    }

    /// Consume one buffer (a packet moved up); the service then asks the
    /// refill rule whether to hand an empty one back.
    pub fn consume_buffer(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    /// Whether the receive queue wants its buffers topped up.
    pub fn refill_wanted(&self) -> bool {
        refill_needed(self.in_flight)
    }

    /// Return a buffer to the complement (after the service handed it to
    /// the queue).
    pub fn note_refilled(&mut self) {
        self.in_flight = (self.in_flight + 1).min(crate::queues::BUFFER_COUNT);
    }

    /// Pad a frame to the Ethernet minimum before handing it to the
    /// transmit queue.
    pub fn pad_frame(&self, length: usize) -> usize {
        padded_length(length)
    }
}

impl Default for VirtioNetFace {
    fn default() -> Self {
        Self::new()
    }
}

impl NetDriver for VirtioNetFace {
    fn name(&self) -> &str {
        "virtio_net"
    }

    fn init(&mut self, _instance: u32) -> InitReport {
        InitReport {
            hardware: self.hardware,
            capabilities: 0,
            ticks: 0, // interrupt-driven; no polling tick
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
        let mut face = VirtioNetFace::new();
        assert_eq!(face.name(), "virtio_net");
        let report = face.init(0);
        assert!(report.hardware.is_zero());
        assert_eq!(report.ticks, 0);
    }

    #[test]
    fn test_refill_rule_tracks_consumption() {
        let mut face = VirtioNetFace::new();
        assert!(!face.refill_wanted());
        // Consume past half: refill kicks in (N1 semantics, 64/32).
        for _ in 0..33 {
            face.consume_buffer();
        }
        assert!(face.refill_wanted());
        // One refill takes it back to the threshold: no longer wanted.
        face.note_refilled();
        assert_eq!(face.in_flight, 32);
        assert!(!face.refill_wanted());
    }

    #[test]
    fn test_pad_frame_to_ethernet_minimum() {
        let face = VirtioNetFace::new();
        assert_eq!(face.pad_frame(0), 60);
        assert_eq!(face.pad_frame(1514), 1514);
    }
}
