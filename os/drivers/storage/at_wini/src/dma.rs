//! Direct-memory-access setup: start, stop, and error check.
//!
//! C correspondence: `check_dma`, `start_dma`, `stop_dma`, `error_dma`
//! (`at_wini.c:564-...,936-...`): programmed input-output below a size
//! threshold, direct access above it, with an error check after every
//! transfer.

/// Minimum sectors for direct access (below this, programmed transfer).
///
/// The C driver picks programmed transfer for small counts and direct
/// access above; the exact threshold is a tuning constant owned by the
/// service crate. This module only tracks the armed state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaState {
    /// Idle: no transfer armed.
    Idle,
    /// Armed for this direction and sector count.
    Armed { write: bool, sectors: usize },
    /// Transfer done, awaiting the error check.
    Done,
}

/// Direct-access arm/disarm policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaArm {
    state: DmaState,
}

impl DmaArm {
    /// Fresh arm (idle).
    pub const fn new() -> DmaArm {
        DmaArm {
            state: DmaState::Idle,
        }
    }

    /// Current state.
    pub const fn state(self) -> DmaState {
        self.state
    }

    /// Arm a transfer; refuses while one is armed or done-unchecked.
    pub fn start(&mut self, write: bool, sectors: usize) -> bool {
        if self.state != DmaState::Idle || sectors == 0 {
            return false;
        }
        self.state = DmaState::Armed { write, sectors };
        true
    }

    /// Note the transfer finished; the error check comes next.
    pub fn note_done(&mut self) -> bool {
        match self.state {
            DmaState::Armed { .. } => {
                self.state = DmaState::Done;
                true
            }
            _ => false,
        }
    }

    /// Check the device error flag after a transfer: clear means the
    /// bytes moved, set means they did not (caller resets).
    ///
    /// C: `error_dma` (`at_wini.c:961-...`).
    pub fn check(&mut self, device_error: bool) -> bool {
        if self.state != DmaState::Done {
            return false;
        }
        self.state = DmaState::Idle;
        !device_error
    }

    /// Stop an armed transfer early (reset path).
    ///
    /// C: `stop_dma` (`at_wini.c:936-...`).
    pub fn stop(&mut self) {
        self.state = DmaState::Idle;
    }
}

impl Default for DmaArm {
    fn default() -> Self {
        DmaArm::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arm_done_check_cycle() {
        let mut arm = DmaArm::new();
        assert!(arm.start(false, 128));
        assert!(!arm.start(true, 1));
        assert!(arm.note_done());
        assert!(arm.check(false));
        assert_eq!(arm.state(), DmaState::Idle);
    }

    #[test]
    fn test_device_error_fails_check() {
        let mut arm = DmaArm::new();
        arm.start(true, 8);
        arm.note_done();
        assert!(!arm.check(true));
        assert_eq!(arm.state(), DmaState::Idle);
    }

    #[test]
    fn test_zero_sectors_and_early_stop() {
        let mut arm = DmaArm::new();
        assert!(!arm.start(false, 0));
        arm.start(false, 4);
        arm.stop();
        assert_eq!(arm.state(), DmaState::Idle);
        assert!(!arm.note_done());
    }
}
