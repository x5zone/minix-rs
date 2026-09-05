//! Bulk-only transport: command wrapper, tag pairing, status check.
//!
//! C correspondence: the wrapper signatures (`CBW_SIGNATURE
//! 0x43425355`, `CSW_SIGNATURE 0x53425355`, `bulk.h:13-42`), the
//! wrapper builders (`init_cbw`, `init_csw`, `bulk.c:14-39`), the
//! three-phase transfer (command, data, status: `send_scsi_cbw_out`,
//! `send_data_in`/`send_data_out`, `send_csw_in` with `check_csw`,
//! `usb_storage.c:230-347`), the tag pairing (`current_cbw_tag` and
//! `last_cbw_tag`, `usb_storage.c:132-133`), the SCSI opcodes
//! (`INQUIRY 0x12`, `READ 0x28`, `WRITE 0x2A`, `READ_CAPACITY 0x25`,
//! `TEST_UNIT_READY 0x00`, `REQUEST_SENSE 0x03`, `MODE_SENSE 0x5A`,
//! `scsi.h:30-36`), and the transfer guard (position and length must
//! divide evenly by the sector size, `transfer_restrictions`,
//! `usb_storage.c:795`).
//!
//! Endpoint traffic stays in the service binary; this module owns the
//! framing half: which bytes open a transfer, which tag pairs the
//! answer, and which status counts as success.

/// Signature opening every command wrapper (`bulk.h:13`).
pub const CBW_SIGNATURE: u32 = 0x4342_5355;

/// Signature opening every status wrapper (`bulk.h:30`).
pub const CSW_SIGNATURE: u32 = 0x5342_5355;

/// Command block length inside the wrapper (`bulk.h:16`).
pub const COMMAND_BLOCK_LENGTH: usize = 16;

/// Sector size guarding every transfer (`scsi.c`, `SECTOR_SIZE`).
pub const SECTOR_SIZE: u64 = 512;

/// SCSI command used by this driver (`scsi.h:30-36`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ScsiCommand {
    /// Probe whether the unit is ready (`TEST_UNIT_READY`, 0x00).
    TestUnitReady = 0x00,
    /// Ask for sense data after a failure (`REQUEST_SENSE`, 0x03).
    RequestSense = 0x03,
    /// Identify the device (`INQUIRY`, 0x12).
    Inquiry = 0x12,
    /// Fetch capacity (`READ_CAPACITY`, 0x25).
    ReadCapacity = 0x25,
    /// Read sectors (`READ_10`, 0x28).
    Read = 0x28,
    /// Write sectors (`WRITE_10`, 0x2A).
    Write = 0x2A,
    /// Fetch mode pages (`MODE_SENSE`, 0x5A).
    ModeSense = 0x5A,
}

/// Status closing a transfer (`bulk.h:30-42`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandStatus {
    /// Transfer completed (`STATUS_GOOD`, 0).
    Good = 0,
    /// Transfer failed (`STATUS_FAILED`, 1).
    Failed = 1,
    /// Phases disagreed (`STATUS_PHASE`, 2).
    PhaseError = 2,
}

/// Tag pairing between a command wrapper and its status wrapper
/// (`current_cbw_tag` / `last_cbw_tag`, `usb_storage.c:132-133`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagPairing {
    next_tag: u32,
    awaiting_tag: Option<u32>,
}

impl TagPairing {
    /// No transfer in flight yet.
    pub fn new() -> Self {
        TagPairing { next_tag: 1, awaiting_tag: None }
    }

    /// Open a transfer: mint the next tag and remember it.
    pub fn open(&mut self) -> u32 {
        let tag = self.next_tag;
        self.next_tag += 1;
        self.awaiting_tag = Some(tag);
        tag
    }

    /// Close a transfer: the status tag must match the open tag and
    /// carry a good status (`check_csw`, `scsi.c:267-288`).
    pub fn close(&mut self, tag: u32, signature: u32, status: CommandStatus) -> bool {
        if signature != CSW_SIGNATURE {
            return false;
        }
        if self.awaiting_tag != Some(tag) {
            return false;
        }
        self.awaiting_tag = None;
        status == CommandStatus::Good
    }

    /// Whether a transfer is currently awaiting its status.
    pub fn is_busy(&self) -> bool {
        self.awaiting_tag.is_some()
    }
}

impl Default for TagPairing {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a transfer at `position` with `length` bytes may start
/// (`transfer_restrictions`, `usb_storage.c:795`): both must divide
/// evenly by the sector size, and the length must be nonzero.
pub fn transfer_allowed(position: u64, length: u64) -> bool {
    length > 0 && position.is_multiple_of(SECTOR_SIZE) && length.is_multiple_of(SECTOR_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signatures_match_bulk_header() {
        assert_eq!(CBW_SIGNATURE, 0x4342_5355);
        assert_eq!(CSW_SIGNATURE, 0x5342_5355);
        assert_eq!(COMMAND_BLOCK_LENGTH, 16);
    }

    #[test]
    fn test_opcode_numbers_match_scsi_header() {
        assert_eq!(ScsiCommand::TestUnitReady as u8, 0x00);
        assert_eq!(ScsiCommand::RequestSense as u8, 0x03);
        assert_eq!(ScsiCommand::Inquiry as u8, 0x12);
        assert_eq!(ScsiCommand::ReadCapacity as u8, 0x25);
        assert_eq!(ScsiCommand::Read as u8, 0x28);
        assert_eq!(ScsiCommand::Write as u8, 0x2A);
        assert_eq!(ScsiCommand::ModeSense as u8, 0x5A);
    }

    #[test]
    fn test_tag_pairing_accepts_matching_good_status() {
        let mut pairing = TagPairing::new();
        let tag = pairing.open();
        assert!(pairing.is_busy());
        assert!(pairing.close(tag, CSW_SIGNATURE, CommandStatus::Good));
        assert!(!pairing.is_busy());
    }

    #[test]
    fn test_tag_pairing_rejects_mismatch_and_failure() {
        let mut pairing = TagPairing::new();
        let tag = pairing.open();
        assert!(!pairing.close(tag + 1, CSW_SIGNATURE, CommandStatus::Good));
        assert!(pairing.is_busy());
        assert!(!pairing.close(tag, 0x1234_5678, CommandStatus::Good));
        assert!(pairing.is_busy());
        assert!(!pairing.close(tag, CSW_SIGNATURE, CommandStatus::Failed));
        assert!(!pairing.is_busy());
        let next = pairing.open();
        assert_ne!(next, tag);
        assert!(!pairing.close(next, CSW_SIGNATURE, CommandStatus::PhaseError));
        assert!(!pairing.is_busy());
    }

    #[test]
    fn test_transfer_guard_demands_sector_alignment() {
        assert!(transfer_allowed(0, 512));
        assert!(transfer_allowed(1024, 4096));
        assert!(!transfer_allowed(0, 0));
        assert!(!transfer_allowed(100, 512));
        assert!(!transfer_allowed(0, 100));
    }
}
