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

/// Transfer direction flag inside the command wrapper
/// (`CBW_FLAGS_OUT 0x00` / `CBW_FLAGS_IN 0x80`, `bulk.h:14-15`).
pub const CBW_FLAGS_OUT: u8 = 0x00;
pub const CBW_FLAGS_IN: u8 = 0x80;

/// The 31-byte Command Block Wrapper that opens every transfer
/// (`struct mass_storage_cbw`, `bulk.h:18-27`, packed).
///
/// The `#[repr(C, packed)]` overlays (and produces) the exact wire
/// bytes the bulk-only protocol ships to the device; the const asserts
/// pin the 31-byte size the spec mandates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct Cbw {
    /// Signature: [`CBW_SIGNATURE`].
    pub signature: u32,
    /// Echoed by the matching status wrapper.
    pub tag: u32,
    /// Bytes of data the host expects to move.
    pub data_transfer_length: u32,
    /// Direction plus reserved bits (`CBW_FLAGS_IN`/`CBW_FLAGS_OUT`).
    pub flags: u8,
    /// Logical unit number.
    pub lun: u8,
    /// Length of the command block below.
    pub cdb_length: u8,
    /// The SCSI command block.
    pub cdb: [u8; COMMAND_BLOCK_LENGTH],
}

const _: () = assert!(core::mem::size_of::<Cbw>() == 31);

impl Cbw {
    /// Build a wrapper: signature, tag, transfer length and direction,
    /// logical unit, and the command block with its length.
    pub fn new(
        tag: u32,
        data_transfer_length: u32,
        to_device: bool,
        lun: u8,
        cdb: [u8; COMMAND_BLOCK_LENGTH],
        cdb_length: u8,
    ) -> Self {
        Cbw {
            signature: CBW_SIGNATURE,
            tag,
            data_transfer_length,
            flags: if to_device { CBW_FLAGS_OUT } else { CBW_FLAGS_IN },
            lun,
            cdb_length,
            cdb,
        }
    }
}

/// The 13-byte Command Status Wrapper that closes every transfer
/// (`struct mass_storage_csw`, `bulk.h:35-42`, packed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, packed)]
pub struct Csw {
    /// Signature: [`CSW_SIGNATURE`].
    pub signature: u32,
    /// Echo of the command wrapper's tag.
    pub tag: u32,
    /// Bytes of the transfer the device did not move.
    pub data_residue: u32,
    /// Outcome (`CommandStatus` discriminants).
    pub status: u8,
}

const _: () = assert!(core::mem::size_of::<Csw>() == 13);

/// Build a 10-byte READ(10) command block: opcode, big-endian LBA at 2,
/// big-endian block count at 7 (`SCSI_SET_READ_*`, `scsi.h:88-90`).
pub fn cdb_read10(lba: u32, blocks: u16) -> [u8; COMMAND_BLOCK_LENGTH] {
    let mut cdb = [0u8; COMMAND_BLOCK_LENGTH];
    cdb[0] = ScsiCommand::Read as u8;
    cdb[2..6].copy_from_slice(&lba.to_be_bytes());
    cdb[7..9].copy_from_slice(&blocks.to_be_bytes());
    cdb
}

/// Build a 10-byte WRITE(10) command block (`SCSI_SET_WRITE_*`,
/// `scsi.h:92-94`).
pub fn cdb_write10(lba: u32, blocks: u16) -> [u8; COMMAND_BLOCK_LENGTH] {
    let mut cdb = cdb_read10(lba, blocks);
    cdb[0] = ScsiCommand::Write as u8;
    cdb
}

/// Build a 6-byte INQUIRY command block with allocation length
/// (`SCSI_SET_INQUIRY_*`, `scsi.h:79-83`): opcode, page code 0 at 2,
/// allocation length at 4.
pub fn cdb_inquiry(allocation_length: u8) -> [u8; COMMAND_BLOCK_LENGTH] {
    let mut cdb = [0u8; COMMAND_BLOCK_LENGTH];
    cdb[0] = ScsiCommand::Inquiry as u8;
    cdb[4] = allocation_length;
    cdb
}

/// Build a 6-byte TEST UNIT READY command block (`scsi.h:93-94`).
pub fn cdb_test_unit_ready() -> [u8; COMMAND_BLOCK_LENGTH] {
    let mut cdb = [0u8; COMMAND_BLOCK_LENGTH];
    cdb[0] = ScsiCommand::TestUnitReady as u8;
    cdb
}

/// Build a 6-byte REQUEST SENSE command block (`scsi.h:97-99`).
pub fn cdb_request_sense(allocation_length: u8) -> [u8; COMMAND_BLOCK_LENGTH] {
    let mut cdb = [0u8; COMMAND_BLOCK_LENGTH];
    cdb[0] = ScsiCommand::RequestSense as u8;
    cdb[4] = allocation_length;
    cdb
}

/// Build a 10-byte READ CAPACITY command block (`scsi.h:101-104`).
pub fn cdb_read_capacity() -> [u8; COMMAND_BLOCK_LENGTH] {
    let mut cdb = [0u8; COMMAND_BLOCK_LENGTH];
    cdb[0] = ScsiCommand::ReadCapacity as u8;
    cdb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cbw_wire_bytes_and_size() {
        assert_eq!(core::mem::size_of::<Cbw>(), 31);
        assert_eq!(core::mem::size_of::<Csw>(), 13);
        let cdb = cdb_read10(0x1234_5678, 4);
        let cbw = Cbw::new(9, 4 * 512, false, 0, cdb, 10);
        // Signature little-endian ("USBC"), tag echo, data length,
        // IN flag with LUN 0 and CDB length 10.
        let bytes: &[u8; 31] = unsafe { &*(&cbw as *const Cbw as *const [u8; 31]) };
        assert_eq!(&bytes[..4], &0x4342_5355u32.to_le_bytes());
        assert_eq!(&bytes[4..8], &9u32.to_le_bytes());
        assert_eq!(&bytes[8..12], &(4u32 * 512).to_le_bytes());
        assert_eq!(bytes[12], CBW_FLAGS_IN);
        assert_eq!(bytes[13], 0);
        assert_eq!(bytes[14], 10);
        assert_eq!(&bytes[15..17], &[0x28, 0x00]); // opcode, then flags
        assert_eq!(&bytes[17..21], &0x1234_5678u32.to_be_bytes()); // LBA
        assert_eq!(&bytes[22..24], &4u16.to_be_bytes()); // block count
        // CSW: signature, tag, residue, failed status.
        let csw = Csw { signature: CSW_SIGNATURE, tag: 9, data_residue: 0, status: CommandStatus::Failed as u8 };
        let csw_bytes: &[u8; 13] = unsafe { &*(&csw as *const Csw as *const [u8; 13]) };
        assert_eq!(&csw_bytes[..4], &0x5342_5355u32.to_le_bytes());
        assert_eq!(csw_bytes[12], 1);
    }

    #[test]
    fn test_cdb_builders_pin_opcodes_and_be_fields() {
        let read = cdb_read10(0x1234_5678, 4);
        assert_eq!(read[0], 0x28);
        assert_eq!(&read[2..6], &[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(&read[7..9], &[0, 4]);
        let write = cdb_write10(1, 2);
        assert_eq!(write[0], 0x2A);
        assert_eq!(&write[2..6], &[0, 0, 0, 1]);
        assert_eq!(&write[7..9], &[0, 2]);
        let inquiry = cdb_inquiry(36);
        assert_eq!(inquiry[0], 0x12);
        assert_eq!(inquiry[4], 36);
        assert_eq!(cdb_test_unit_ready()[0], 0x00);
        assert_eq!(cdb_request_sense(18)[0], 0x03);
        assert_eq!(cdb_request_sense(18)[4], 18);
        assert_eq!(cdb_read_capacity()[0], 0x25);
    }


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
