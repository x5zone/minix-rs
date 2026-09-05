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
    /// Fetch card-specific data (`SEND_CSD`, 9).
    SendCsd = 9,
    /// Fix the block length (`SET_BLOCKLEN`, 16).
    SetBlockLength = 16,
    /// Read one block (`READ_SINGLE_BLOCK`, 17).
    ReadSingle = 17,
    /// Write one block (`WRITE_BLOCK`, 24).
    WriteSingle = 24,
}

/// Block size the driver negotiates with CMD16 (`emmc.c`, 512 bytes).
pub const NEGOTIATED_BLOCK_SIZE: u32 = 512;

/// Power-up stage: where the card is in its initialization sequence
/// (`emmc.c:790-890`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitStage {
    /// Nothing sent yet; CMD0 comes first.
    Fresh,
    /// CMD0 sent; polling the card with CMD1.
    PollingOpCond,
    /// Card ready; fetching identity (CMD2, CMD3, CMD9).
    Identifying,
    /// Card selected (CMD7); switching speed and block size.
    Configuring,
    /// CMD6 and CMD16 accepted; reads and writes may flow.
    Ready,
}

/// Power-up sequence driver: one command at a time, in fixed order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitSequence {
    stage: InitStage,
}

impl InitSequence {
    /// A card that has just been powered on.
    pub fn new() -> Self {
        InitSequence { stage: InitStage::Fresh }
    }

    /// Current stage of the sequence.
    pub fn stage(&self) -> InitStage {
        self.stage
    }

    /// The command to send next for the current stage.
    pub fn next_command(&self) -> CardCommand {
        match self.stage {
            InitStage::Fresh => CardCommand::GoIdle,
            InitStage::PollingOpCond => CardCommand::SendOpCond,
            InitStage::Identifying => CardCommand::AllSendCid,
            InitStage::Configuring => CardCommand::Switch,
            InitStage::Ready => CardCommand::ReadSingle,
        }
    }

    /// Advance after a successful reply; out-of-order success is refused.
    pub fn note_success(&mut self) -> bool {
        let next = match self.stage {
            InitStage::Fresh => InitStage::PollingOpCond,
            InitStage::PollingOpCond => InitStage::Identifying,
            InitStage::Identifying => InitStage::Configuring,
            InitStage::Configuring => InitStage::Ready,
            InitStage::Ready => return false,
        };
        self.stage = next;
        true
    }

    /// Whether transfers may flow (CMD17/CMD24 allowed).
    pub fn is_ready(&self) -> bool {
        self.stage == InitStage::Ready
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
        assert_eq!(seq.stage(), InitStage::Fresh);
        assert_eq!(seq.next_command(), CardCommand::GoIdle);
        assert!(!seq.is_ready());
    }

    #[test]
    fn test_sequence_reaches_ready_in_four_steps() {
        let mut seq = InitSequence::new();
        for _ in 0..4 {
            assert!(seq.note_success());
        }
        assert!(seq.is_ready());
        assert!(!seq.note_success());
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
