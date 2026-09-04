//! Shutdown states ('c' catatonia, 'd' death).
//!
//! Covers `minix3/sbin/init/init.c:1634-1698`.
//! Design contract: `.design/11-design.v1.md §1.1-§1.2`.

/// Seconds per death round (C: `DEATH_WATCH`, init.c:96).
pub const DEATH_WATCH_SECS: u64 = 10;

/// Kill escalation sequence (C: `death_sigs`, init.c:1667).
/// Values are POSIX signal numbers: SIGHUP=1, SIGTERM=15, SIGKILL=9.
pub const DEATH_SEQUENCE: [i32; 3] = [1, 15, 9];

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

/// Count sessions catatonia would mark (C: init.c:1639-1640).
pub fn catatonia_marks(session_count: usize) -> usize {
    session_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catatonia_marks_all() {
        assert_eq!(catatonia_marks(5), 5);
    }

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
