//! The scheduler trait: one interface, one implementation per flavour.
//!
//! Minix ships two periodic schedulers with deliberately different shapes:
//! `cron` fires jobs off a wall clock timetable, while the `update` daemon
//! wakes up every fixed number of seconds to flush dirty blocks. Both
//! answer the same question ("is it time to act yet?") from different
//! inputs, so both implement [`ScheduleMatcher`]. This is the same move
//! Redox makes with its scheme traits: depend on the behaviour interface,
//! keep the two time models interchangeable for the program layer.

use crate::cron::{ClockTime, CronEntry, entry_matches};

/// Answers "should the job run now?" for one scheduling flavour.
///
/// The trait is deliberately narrow: matching a timetable is pure
/// computation over the current reading, while running the job (forking,
/// waiting, logging) stays with the caller.
pub trait ScheduleMatcher {
    /// Decide whether the job fires at `now`.
    fn due(&self, now: &ClockTime) -> bool;
}

/// Timetable matching for `cron`.
///
/// Wraps one parsed [`CronEntry`]; firing means every field matches.
pub struct CronMatcher<'a> {
    /// The timetable this matcher tests against.
    pub entry: CronEntry<'a>,
}

impl ScheduleMatcher for CronMatcher<'_> {
    fn due(&self, now: &ClockTime) -> bool {
        entry_matches(&self.entry, now)
    }
}

/// Fixed interval matching for the `update` daemon.
///
/// Ground truth: `minix3/minix/commands/update/update.c` wakes up every
/// fixed number of seconds and issues a sync. The matcher fires when the
/// elapsed whole seconds since the daemon started are a positive multiple
/// of the interval: second zero is startup itself, not a sync point.
pub struct IntervalMatcher {
    /// Seconds between wake ups; must be positive.
    pub interval_seconds: u64,
}

impl ScheduleMatcher for IntervalMatcher {
    fn due(&self, now: &ClockTime) -> bool {
        due_every(self.interval_seconds, clock_to_seconds(now))
    }
}

/// Pure core shared by the matcher: fire on positive multiples of
/// `interval`, never at second zero, never for a zero interval.
pub fn due_every(interval: u64, elapsed_seconds: u64) -> bool {
    interval != 0 && elapsed_seconds != 0 && elapsed_seconds.is_multiple_of(interval)
}

/// Fold a clock reading into a second count for interval comparison.
///
/// The fold is intentionally simple (30 day months, leap days ignored):
/// interval matching only needs a monotone tick, not calendar truth.
pub fn clock_to_seconds(now: &ClockTime) -> u64 {
    const SECONDS_PER_MINUTE: u64 = 60;
    const SECONDS_PER_HOUR: u64 = 3_600;
    const SECONDS_PER_DAY: u64 = 86_400;
    const SECONDS_PER_MONTH: u64 = 30 * SECONDS_PER_DAY;
    u64::from(now.month) * SECONDS_PER_MONTH
        + u64::from(now.day_of_month) * SECONDS_PER_DAY
        + u64::from(now.hour) * SECONDS_PER_HOUR
        + u64::from(now.minute) * SECONDS_PER_MINUTE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::parse_cron_line;

    fn reading(minute: u32, hour: u32) -> ClockTime {
        ClockTime {
            minute,
            hour,
            day_of_month: 10,
            month: 5,
            day_of_week: 3,
        }
    }

    #[test]
    fn test_cron_matcher_fires_on_timetable() {
        let entry = parse_cron_line("30 4 * * * /bin/sync").unwrap().unwrap();
        let matcher = CronMatcher { entry };
        assert!(matcher.due(&reading(30, 4)));
        assert!(!matcher.due(&reading(31, 4)));
    }

    #[test]
    fn test_interval_matcher_fires_on_multiples() {
        let matcher = IntervalMatcher { interval_seconds: 30 };
        assert!(matcher.due(&reading(1, 0)));
    }

    #[test]
    fn test_due_every_core() {
        assert!(due_every(30, 30));
        assert!(due_every(30, 60));
        assert!(!due_every(30, 31));
        // Second zero is startup, not a sync point.
        assert!(!due_every(30, 0));
        // A zero interval never fires.
        assert!(!due_every(0, 30));
    }

    #[test]
    fn test_matchers_are_interchangeable_as_trait_objects() {
        let entry = parse_cron_line("* * * * * tick").unwrap().unwrap();
        let cron = CronMatcher { entry };
        let interval = IntervalMatcher { interval_seconds: 60 };
        let matchers: [&dyn ScheduleMatcher; 2] = [&cron, &interval];
        let now = reading(12, 3);
        let fired: usize = matchers.iter().filter(|m| m.due(&now)).count();
        assert!(fired >= 1);
    }
}
