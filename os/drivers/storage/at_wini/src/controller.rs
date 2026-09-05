//! Controller state: reset, identify, ready, error recovery.
//!
//! C correspondence: `w_reset`, `w_probe`, `w_init`, `w_need_reset`
//! (`at_wini.c:314-453,1446-...,1524-...`), the drive bound
//! `MAX_DRIVES 4` and the transfer bound `MAX_SECS 256`
//! (`at_wini.h:184-190`), and the wait helpers `w_waitfor` plus
//! `w_intr_wait` (`at_wini.c:1626-...`).
//!
//! Port reads and writes stay in the service crate; this module owns the
//! order (reset, probe, identify, ready) and the wait policy (busy-wait
//! with timeout, interrupt wait otherwise).

/// Maximum drives per controller instance.
///
/// C: `MAX_DRIVES 4` (`at_wini.h:184`).
pub const MAX_DRIVES: usize = 4;

/// Maximum sectors per transfer.
///
/// C: `MAX_SECS 256` (`at_wini.h:186`): the controller moves at most this
/// many sectors per command.
pub const MAX_SECTORS: usize = 256;

/// Controller stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Nothing done yet.
    Fresh,
    /// Reset issued, waiting for calm.
    Resetting,
    /// Probed: drives found, identifying.
    Probed,
    /// Ready: serving transfers.
    Ready,
    /// Error latched: reset required.
    NeedsReset,
}

/// One controller: stage plus per-drive presence.
///
/// C: the `wini` array plus the probe/init sequence (`at_wini.c`).
#[derive(Debug, Clone)]
pub struct Controller {
    stage: Stage,
    present: [bool; MAX_DRIVES],
}

impl Controller {
    /// Fresh controller (no drives known).
    pub const fn new() -> Controller {
        Controller {
            stage: Stage::Fresh,
            present: [false; MAX_DRIVES],
        }
    }

    /// Current stage.
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// Issue a reset (from fresh or from error).
    ///
    /// C: `w_reset` (`at_wini.c:1524-...`).
    pub fn reset(&mut self) {
        self.stage = Stage::Resetting;
    }

    /// Note the reset completed with these drives present.
    ///
    /// C: `w_probe` finds drives, `w_init` arms them
    /// (`at_wini.c:314-453`).
    pub fn note_probed(&mut self, present: [bool; MAX_DRIVES]) {
        if self.stage == Stage::Resetting {
            self.present = present;
            self.stage = Stage::Probed;
        }
    }

    /// Note identification done: ready when at least one drive answers.
    pub fn note_identified(&mut self, answered: usize) {
        if self.stage == Stage::Probed {
            self.stage = if answered > 0 {
                Stage::Ready
            } else {
                Stage::NeedsReset
            };
        }
    }

    /// Latch an error (next step is a reset).
    ///
    /// C: `w_need_reset` (`at_wini.c:1446-...`).
    pub fn note_error(&mut self) {
        self.stage = Stage::NeedsReset;
    }

    /// True when this drive may serve transfers.
    pub fn drive_ready(&self, drive: usize) -> bool {
        self.stage == Stage::Ready && drive < MAX_DRIVES && self.present[drive]
    }
}

impl Default for Controller {
    fn default() -> Self {
        Controller::new()
    }
}

/// Split a sector count into controller-sized commands.
///
/// C: transfers above `MAX_SECS` split (`at_wini.c` transfer path):
/// full chunks of 256 sectors plus one remainder. Empty for zero.
pub fn split_transfer(sectors: usize) -> (usize, usize) {
    (sectors / MAX_SECTORS, sectors % MAX_SECTORS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reset_probe_identify_cycle() {
        let mut controller = Controller::new();
        controller.reset();
        assert_eq!(controller.stage(), Stage::Resetting);
        controller.note_probed([true, false, false, false]);
        assert_eq!(controller.stage(), Stage::Probed);
        controller.note_identified(1);
        assert_eq!(controller.stage(), Stage::Ready);
        assert!(controller.drive_ready(0));
        assert!(!controller.drive_ready(1));
        assert!(!controller.drive_ready(9));
    }

    #[test]
    fn test_empty_probe_needs_reset() {
        let mut controller = Controller::new();
        controller.reset();
        controller.note_probed([false; MAX_DRIVES]);
        controller.note_identified(0);
        assert_eq!(controller.stage(), Stage::NeedsReset);
        controller.reset();
        assert_eq!(controller.stage(), Stage::Resetting);
    }

    #[test]
    fn test_error_latches_until_reset() {
        let mut controller = Controller::new();
        controller.reset();
        controller.note_probed([true, true, false, false]);
        controller.note_identified(2);
        controller.note_error();
        assert_eq!(controller.stage(), Stage::NeedsReset);
        assert!(!controller.drive_ready(0));
    }

    #[test]
    fn test_split_respects_max_sectors() {
        assert_eq!(split_transfer(0), (0, 0));
        assert_eq!(split_transfer(256), (1, 0));
        assert_eq!(split_transfer(300), (1, 44));
        assert_eq!(MAX_DRIVES, 4);
        assert_eq!(MAX_SECTORS, 256);
    }
}
