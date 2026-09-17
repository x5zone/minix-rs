//! minix-usb wiring: usb_storage's BOT transfer state over the URB
//! bookkeeping.
//!
//! C correspondence: `usb_storage.c`'s transfer phases — CBW out
//! (`send_scsi_cbw_out`, `usb_storage.c:230-259`), data in/out, CSW in
//! with the tag/signature/status check (`check_csw`, `scsi.c:267-288`).
//!
//! The face composes the crate's framing (CBW/CDB builders, tag
//! pairing) over `minix-usb`'s URB roster: an open transfer submits an
//! urb and its roster id doubles as the BOT tag, so the status wrapper
//! pairs against the same authority the completion path uses. Bulk
//! transport stays with the service.

use minix_usb::urb::{Direction, PendingUrb, TransferKind};
use minix_usb::wire::{fill_send_urb, split_complete_urb};
use minix_types::Message;

use crate::cbw::{
    cdb_read10, cdb_write10, transfer_allowed, Cbw, TagPairing, COMMAND_BLOCK_LENGTH,
};

/// The usb_storage face: tag pairing plus the URB roster.
pub struct StorageFace {
    pub pairing: TagPairing,
    pub pending: alloc::vec::Vec<PendingUrb>,
}

impl StorageFace {
    /// A face with an empty pairing and roster.
    pub fn new() -> Self {
        StorageFace {
            pairing: TagPairing::new(),
            pending: alloc::vec::Vec::new(),
        }
    }

    /// Open one read transfer: plan the CDB, submit the urb, fill the
    /// request's wire slots. Returns the tag (which equals the urb id).
    ///
    /// C: `send_scsi_cbw_out` builds the CBW and submits the bulk-out.
    pub fn submit_read(&mut self, msg: &mut Message, lba: u32, blocks: u16) -> Option<u32> {
        if !transfer_allowed(lba as u64 * 512, blocks as u64 * 512) {
            return None; // misaligned lba or block count
        }
        let tag = self.pairing.open();
        let cdb = cdb_read10(lba, blocks);
        self.pending.push(PendingUrb {
            id: tag,
            endpoint: 1,
            kind: TransferKind::Bulk,
            direction: Direction::In,
        });
        fill_send_urb(msg, tag as i64, blocks as i64 * 512);
        let _ = (cdb, COMMAND_BLOCK_LENGTH);
        Some(tag)
    }

    /// Open one write transfer: same as the read path with the write CDB.
    pub fn submit_write(&mut self, msg: &mut Message, lba: u32, blocks: u16) -> Option<u32> {
        if !transfer_allowed(lba as u64 * 512, blocks as u64 * 512) {
            return None;
        }
        let tag = self.pairing.open();
        let cdb = cdb_write10(lba, blocks);
        self.pending.push(PendingUrb {
            id: tag,
            endpoint: 1,
            kind: TransferKind::Bulk,
            direction: Direction::Out,
        });
        fill_send_urb(msg, tag as i64, blocks as i64 * 512);
        let _ = (cdb, COMMAND_BLOCK_LENGTH);
        Some(tag)
    }

    /// Handle a completion: unlink the roster entry; the pairing is
    /// closed by the caller once the CSW signature/status check passes.
    pub fn complete(&mut self, msg: &mut Message) -> Option<PendingUrb> {
        let (id, _result) = split_complete_urb(msg)?;
        minix_usb::urb::remove_pending(&mut self.pending, id as u32)
    }
}

impl Default for StorageFace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_submission_fills_wire_slots_and_roster() {
        let mut face = StorageFace::new();
        let mut msg = Message::default();
        let tag = face.submit_read(&mut msg, 0, 8).unwrap();
        assert_eq!(tag, 1);
        assert_eq!(face.pending.len(), 1);
        assert!(face.pairing.is_busy());
        // The wire slots carry the grant id (the tag) and the byte size.
        assert_eq!(minix_usb::wire::split_send_urb(&msg), Some((tag as i64, 4096)));
    }

    #[test]
    fn test_misaligned_transfer_refused_locally() {
        let mut face = StorageFace::new();
        let mut msg = Message::default();
        // Zero length: refused before any urb (transfer_allowed's first
        // clause).
        assert!(face.submit_read(&mut msg, 0, 0).is_none());
        assert!(face.pending.is_empty());
        assert!(!face.pairing.is_busy());
    }

    #[test]
    fn test_completion_unlinks_the_roster_entry() {
        let mut face = StorageFace::new();
        let mut msg = Message::default();
        let tag = face.submit_read(&mut msg, 0, 4).unwrap();
        let mut reply = Message::default();
        minix_usb::wire::fill_complete_urb(&mut reply, tag as i64, 0);
        assert!(face.complete(&mut reply).is_some());
        assert!(face.pending.is_empty());
    }

    #[test]
    fn test_write_submission_uses_the_write_cdb() {
        let mut face = StorageFace::new();
        let mut msg = Message::default();
        let tag = face.submit_write(&mut msg, 16, 2).unwrap();
        assert_eq!(tag, 1);
        assert_eq!(minix_usb::wire::split_send_urb(&msg), Some((1, 1024)));
    }
}
