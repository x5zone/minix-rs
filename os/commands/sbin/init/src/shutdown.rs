//! Shutdown states ('c' catatonia, 'd' death).
//!
//! Covers `minix3/sbin/init/init.c:1634-1698`.
//! Design contract: `.design/11-design.v1.md §1.1-§1.2`.

use crate::state_machine::sig::{SIGNAL_HANGUP, SIGNAL_KILL, SIGNAL_TERMINATE};

/// Seconds per death round (C: `DEATH_WATCH`, init.c:96).
pub const DEATH_WATCH_SECS: u64 = 10;

/// Kill escalation sequence (C: `death_sigs`, init.c:1667):
/// SIGHUP, SIGTERM, SIGKILL — values from the Minix3 numbering authority.
pub const DEATH_SEQUENCE: [i32; 3] = [SIGNAL_HANGUP, SIGNAL_TERMINATE, SIGNAL_KILL];

/// One death-round outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathRoundOutcome {
    AllDead,
    NextRound,
    StuckWarn,
}

/// Classify one round (pure).
pub fn classify_round(reaped_all: bool, timed_out: bool) -> DeathRoundOutcome {
    if reaped_all {
        DeathRoundOutcome::AllDead
    } else if timed_out {
        DeathRoundOutcome::NextRound
    } else {
        DeathRoundOutcome::StuckWarn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_death_sequence_order() {
        assert_eq!(DEATH_SEQUENCE, [1, 15, 9]);
    }

    #[test]
    fn test_round_all_dead() {
        assert_eq!(classify_round(true, false), DeathRoundOutcome::AllDead);
    }

    #[test]
    fn test_round_timeout_next() {
        assert_eq!(classify_round(false, true), DeathRoundOutcome::NextRound);
    }

    #[test]
    fn test_stuck_warns() {
        assert_eq!(classify_round(false, false), DeathRoundOutcome::StuckWarn);
    }
}
