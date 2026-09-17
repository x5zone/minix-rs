//! minix-usb wiring: the USB daemon's face over the URB bookkeeping.
//!
//! C correspondence: `usbd`'s HCD interaction — enumeration walks the
//! stage sequence (`hcd.c:473-565`), URB submission registers in the
//! pending list (`usb.c:65-68`), and completion unlinks and reports
//! (`usb.c:157-186`). The face composes the crate's policy modules over
//! the wire vocabulary of `minix-usb` (`wire.rs` slots, `urb.rs`
//! bookkeeping).
//!
//! URB transport (the actual host-controller messages) stays with the
//! service; this face is the admission and bookkeeping half.

use crate::enumerate::{Enumeration, EnumStage};
use crate::protocol::{decode_daemon_message, decode_driver_request};
use minix_usb::urb::{PendingUrb, TransferKind};
use minix_usb::wire::{fill_complete_urb, fill_send_urb, split_complete_urb, split_send_urb};

/// Reads the send-urb slots (test helper re-exporting the wire accessor).
fn split_send_urb_slot(msg: &Message) -> Option<(i64, i64)> {
    minix_usb::wire::split_send_urb(msg)
}
use minix_types::Message;

/// The usbd face: enumeration tracking plus the URB roster.
pub struct UsbdFace {
    /// Enumeration walk for the device being configured.
    pub enumeration: Enumeration,
    /// In-flight URB roster (minix-usb bookkeeping).
    pub pending: alloc::vec::Vec<PendingUrb>,
    /// Next identifier to hand out (starts at 1; zero is the invalid
    /// marker).
    next_id: u32,
}

impl UsbdFace {
    /// A face with a fresh enumeration walk and an empty roster.
    pub fn new() -> Self {
        UsbdFace {
            enumeration: Enumeration::new(),
            pending: alloc::vec::Vec::new(),
            next_id: 1,
        }
    }

    /// Submit a bulk URB: register it in the roster and fill the
    /// request's wire slots (grant id and size, `com.h:830-831`).
    pub fn submit_bulk_urb(&mut self, msg: &mut Message) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.push(PendingUrb {
            id,
            endpoint: 1,
            kind: TransferKind::Bulk,
            direction: minix_usb::urb::Direction::Out,
        });
        fill_send_urb(msg, id as i64, 512);
        id
    }

    /// Handle a completion message: unlink the roster entry and report
    /// whether it matched.
    pub fn complete(&mut self, msg: &mut Message) -> Option<PendingUrb> {
        let (id, result) = split_complete_urb(msg)?;
        let matched = minix_usb::urb::remove_pending(&mut self.pending, id as u32);
        // Fill the completion report for the service's reply path.
        fill_complete_urb(msg, id, result);
        matched
    }

    /// Whether the enumeration walk finished.
    pub fn is_enumerated(&self) -> bool {
        self.enumeration.is_enumerated()
    }
}

impl Default for UsbdFace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urb_submission_and_completion_round_trip() {
        let mut face = UsbdFace::new();
        let mut msg = Message::default();
        let id = face.submit_bulk_urb(&mut msg);
        assert_eq!(face.pending.len(), 1);
        // The wire slots carry the grant id and size (m4_l1/m4_l2).
        assert_eq!(split_send_urb_slot(&msg), Some((id as i64, 512)));
        let mut reply = Message::default();
        fill_complete_urb(&mut reply, id as i64, 0);
        assert!(face.complete(&mut reply).is_some());
        assert!(face.pending.is_empty());
        // A completion for an unknown id matches nothing.
        fill_complete_urb(&mut reply, id as i64 + 100, 0);
        assert!(face.complete(&mut reply).is_none());
    }

    #[test]
    fn test_decoders_reject_unknown_numbers() {
        assert!(decode_driver_request(0x9999).is_none());
        assert!(decode_daemon_message(0x9999).is_none());
    }

    #[test]
    fn test_enumeration_gates_transfers() {
        let face = UsbdFace::new();
        assert!(!face.is_enumerated());
    }

    #[test]
    fn test_stage_sequence_reaches_enumerated() {
        let mut face = UsbdFace::new();
        assert_eq!(face.enumeration.stage(), EnumStage::Detected);
        for _ in 0..5 {
            assert!(face.enumeration.note_success());
        }
        assert!(face.is_enumerated());
    }
}
