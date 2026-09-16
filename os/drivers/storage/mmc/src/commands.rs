//! MMC command set and initialization order: opcodes, power-up sequence.
//!
//! C correspondence: the opcode table (`sdmmcreg.h:24-65`, including
//! the SD extras `SD_SEND_IF_COND` and `APP_OP_COND`), the per-command
//! wrappers (`emmc.c:262-403`), and the power-up sequence
//! (`emmc.c:790-890`: CMD0, CMD1 polling, CMD2, CMD3, CMD9, CMD7,
//! CMD6 high-speed switch, CMD16 with 512-byte blocks). Single-block
//! reads and writes go through CMD17 and CMD24 (`emmc.c:544-558`).
//!
//! Host register traffic (OMAP in `mmchost_mmchs.c`, 1267 lines, or the
//! dummy host in `mmchost_dummy.c`, 170 lines, behind `mmchost.h`)
//! stays in the service binary; this module owns the card-facing order:
//! which command comes next and which reply counts as success.

/// Card commands used during power-up and transfer (`sdmmcreg.h:24-55`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CardCommand {
    /// Reset the card to idle (`GO_IDLE_STATE`, 0).
    GoIdle = 0,
    /// Ask the card for its operating conditions (`SEND_OP_COND`, 1).
    SendOpCond = 1,
    /// Fetch the card identifier (`ALL_SEND_CID`, 2).
    AllSendCid = 2,
    /// Publish the card address (`SET_RELATIVE_ADDR`, 3).
    SetRelativeAddr = 3,
    /// Switch card mode, e.g. high speed (`SWITCH`, 6).
    Switch = 6,
    /// Select the card (`SELECT_DESELECT_CARD`, 7).
    Select = 7,
    /// Fetch the extended card-specific data (`SEND_EXT_CSD`, 8).
    SendExtCsd = 8,
    /// Fetch card-specific data (`SEND_CSD`, 9).
    SendCsd = 9,
    /// Poll the card state (`SEND_STATUS`, 13).
    SendStatus = 13,
    /// Fix the block length (`SET_BLOCKLEN`, 16).
    SetBlockLength = 16,
    /// Read one block (`READ_SINGLE_BLOCK`, 17).
    ReadSingle = 17,
    /// Write one block (`WRITE_BLOCK`, 24).
    WriteSingle = 24,
}

/// Block size the driver negotiates with CMD16 (`emmc.c`, 512 bytes).
pub const NEGOTIATED_BLOCK_SIZE: u32 = 512;

/// The card bring-up order, straight down the C path.
///
/// C: `emmc_card_initialize` (`emmc.c:790-890`): reset, voltage/polling,
/// identity, addressing, CSD, selection, EXT_CSD fetch, then the two
/// CMD6 switches (high-speed timing, bus width) each confirmed by a
/// CMD13 status check, and finally CMD16 fixing the block length at 512.
/// Arguments (switch targets, address value) are service data; the order
/// is the protocol.
pub const CARD_SEQUENCE: [CardCommand; 12] = [
    CardCommand::GoIdle,
    CardCommand::SendOpCond,
    CardCommand::AllSendCid,
    CardCommand::SetRelativeAddr,
    CardCommand::SendCsd,
    CardCommand::Select,
    CardCommand::SendExtCsd,
    CardCommand::Switch,
    CardCommand::SendStatus,
    CardCommand::Switch,
    CardCommand::SendStatus,
    CardCommand::SetBlockLength,
];

/// Power-up sequence driver: walks [`CARD_SEQUENCE`] one success at a time.
///
/// A step advances only on `note_success`; a polling command (CMD1) is
/// simply re-fed until the service sees it accepted, and a failed switch
/// (CMD6) stops the walk where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitSequence {
    step: usize,
}

impl InitSequence {
    /// A card that has just been powered on.
    pub const fn new() -> Self {
        InitSequence { step: 0 }
    }

    /// Position in the bring-up order (zero-based).
    pub const fn step(&self) -> usize {
        self.step
    }

    /// The command to send next, or None once the card is ready.
    pub const fn next_command(&self) -> Option<CardCommand> {
        if self.step < CARD_SEQUENCE.len() {
            Some(CARD_SEQUENCE[self.step])
        } else {
            None
        }
    }

    /// Advance one command after a successful reply.
    ///
    /// False once the sequence is exhausted — every card command in
    /// [`CARD_SEQUENCE`] has been accepted and transfers may flow.
    pub fn note_success(&mut self) -> bool {
        if self.step >= CARD_SEQUENCE.len() {
            return false;
        }
        self.step += 1;
        true
    }

    /// Whether transfers may flow (CMD17/CMD24 allowed).
    pub fn is_ready(&self) -> bool {
        self.step >= CARD_SEQUENCE.len()
    }
}

impl Default for InitSequence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_starts_with_reset_command() {
        let seq = InitSequence::new();
        assert_eq!(seq.step(), 0);
        assert_eq!(seq.next_command(), Some(CardCommand::GoIdle));
        assert!(!seq.is_ready());
    }

    #[test]
    fn test_sequence_walks_the_full_c_order() {
        // emmc.c:790-890, verbatim: reset, poll, identity, address, CSD,
        // select, EXT_CSD, switch+status twice, block length. Every step
        // advances only on success; the walk ends ready with no command.
        let mut seq = InitSequence::new();
        for expected in CARD_SEQUENCE {
            assert_eq!(seq.next_command(), Some(expected));
            assert!(seq.note_success());
        }
        assert!(seq.is_ready());
        assert_eq!(seq.next_command(), None);
        assert!(!seq.note_success());
    }

    #[test]
    fn test_every_opcode_is_reachable_in_the_walk() {
        // The old model defined commands its sequence never returned; this
        // pins the fix: each enum value appears in the bring-up order (the
        // transfer pair CMD17/24 runs after ready, outside the walk).
        for opcode in [
            CardCommand::GoIdle,
            CardCommand::SendOpCond,
            CardCommand::AllSendCid,
            CardCommand::SetRelativeAddr,
            CardCommand::Switch,
            CardCommand::Select,
            CardCommand::SendExtCsd,
            CardCommand::SendCsd,
            CardCommand::SendStatus,
            CardCommand::SetBlockLength,
        ] {
            assert!(CARD_SEQUENCE.contains(&opcode), "{opcode:?} unreachable");
        }
    }

    #[test]
    fn test_opcode_numbers_match_spec_table() {
        assert_eq!(CardCommand::GoIdle as u8, 0);
        assert_eq!(CardCommand::SendOpCond as u8, 1);
        assert_eq!(CardCommand::Switch as u8, 6);
        assert_eq!(CardCommand::SetBlockLength as u8, 16);
        assert_eq!(CardCommand::ReadSingle as u8, 17);
        assert_eq!(CardCommand::WriteSingle as u8, 24);
        assert_eq!(NEGOTIATED_BLOCK_SIZE, 512);
    }
}
