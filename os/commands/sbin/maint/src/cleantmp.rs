//! Temporary directory cleaning decisions.
//!
//! Ground truth: `minix3/minix/commands/cleantmp/cleantmp.c`. A full day holds
//! `SEC_DAY` seconds (24 times 3600), hidden names keep `DOTDAYS 14` days of
//! extra protection, `days2time` rounds the current time back to midnight and
//! then steps back `days minus one` full days to obtain the `retired`
//! timestamp (regular names older than `retired` are removed, hidden names
//! older than `dotretired` are removed, where `dotretired` never lies ahead of
//! `retired`). The `force` flag removes everything, the `debug` flag prints
//! what would happen. Failures are reported on the standard error stream with
//! the system error text.

use crate::MaintError;

/// Seconds in one full day (24 hours times 3600 seconds).
pub const SECONDS_PER_DAY: i64 = 24 * 3600;

/// Extra protection window for hidden names, in days.
pub const HIDDEN_NAME_DAYS: u64 = 14;

/// Retention timestamps produced by [`days_to_retention`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// Regular names at or before this time are retired.
    pub retired: i64,
    /// Hidden names at or before this time are retired.
    pub hidden_retired: i64,
}

/// Convert a day count and the current time into retention timestamps.
///
/// `now_seconds` is the current time in seconds since the Unix epoch,
/// `days` is the requested retention window. The computation first rounds
/// `now_seconds` back to local midnight (the remainder modulo one day is
/// dropped), then steps back `days minus one` full days. When the clock sits
/// before the requested window both timestamps are zero (nothing is old
/// enough to remove). The hidden timestamp never lies ahead of the regular
/// timestamp.
pub fn days_to_retention(days: u64, now_seconds: i64) -> Result<Retention, MaintError> {
    if days == 0 {
        return Err(MaintError::InvalidArgument);
    }
    let midnight = now_seconds - now_seconds.rem_euclid(SECONDS_PER_DAY);
    let window = (days as i64 - 1)
        .checked_mul(SECONDS_PER_DAY)
        .ok_or(MaintError::InvalidArgument)?;
    if midnight < window {
        return Ok(Retention {
            retired: 0,
            hidden_retired: 0,
        });
    }
    let retired = midnight - window;
    let hidden_window = (HIDDEN_NAME_DAYS as i64 - 1) * SECONDS_PER_DAY;
    let mut hidden_retired = midnight - hidden_window;
    if hidden_retired > retired {
        hidden_retired = retired;
    }
    Ok(Retention {
        retired,
        hidden_retired,
    })
}

/// True when `name` is hidden (starts with a dot, but is not `.` or `..`).
pub fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

/// Decide whether one directory entry should be removed.
///
/// `force` removes everything. Otherwise regular names compare against
/// `retention.retired` and hidden names compare against
/// `retention.hidden_retired`. Entries strictly newer than the matching
/// timestamp stay.
pub fn should_remove(
    name: &str,
    modified_seconds: i64,
    retention: Retention,
    force: bool,
) -> bool {
    if force {
        return true;
    }
    if name == "." || name == ".." {
        return false;
    }
    if is_hidden_name(name) {
        modified_seconds <= retention.hidden_retired
    } else {
        modified_seconds <= retention.retired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midnight_rounding() {
        // Thirty days after the epoch, one day of retention: the regular
        // timestamp lands exactly on midnight, the hidden timestamp lies
        // thirteen days earlier (fourteen days of hidden protection).
        let midnight = 30 * SECONDS_PER_DAY;
        let retention = days_to_retention(1, midnight + 3600).unwrap();
        assert_eq!(retention.retired, midnight);
        assert_eq!(retention.hidden_retired, midnight - 13 * SECONDS_PER_DAY);
    }

    #[test]
    fn test_one_day_window_keeps_today() {
        let midnight = 10 * SECONDS_PER_DAY;
        let retention = days_to_retention(1, midnight + 3600).unwrap();
        assert_eq!(retention.retired, midnight);
    }

    #[test]
    fn test_hidden_never_ahead_of_regular() {
        let midnight = 30 * SECONDS_PER_DAY;
        let retention = days_to_retention(30, midnight).unwrap();
        assert!(retention.hidden_retired <= retention.retired);
    }

    #[test]
    fn test_clock_before_window_yields_zero() {
        let retention = days_to_retention(100, 5 * SECONDS_PER_DAY).unwrap();
        assert_eq!(retention.retired, 0);
        assert_eq!(retention.hidden_retired, 0);
    }

    #[test]
    fn test_zero_days_rejected() {
        assert_eq!(
            days_to_retention(0, 1000),
            Err(MaintError::InvalidArgument)
        );
    }

    #[test]
    fn test_hidden_detection() {
        assert!(is_hidden_name(".cache"));
        assert!(!is_hidden_name("cache"));
        assert!(!is_hidden_name("."));
        assert!(!is_hidden_name(".."));
    }

    #[test]
    fn test_old_regular_removed() {
        let retention = Retention {
            retired: 1000,
            hidden_retired: 500,
        };
        assert!(should_remove("scratch", 900, retention, false));
        assert!(!should_remove("fresh", 1100, retention, false));
    }

    #[test]
    fn test_hidden_uses_longer_window() {
        let retention = Retention {
            retired: 1000,
            hidden_retired: 500,
        };
        // Older than the regular timestamp but newer than the hidden one.
        assert!(!should_remove(".cache", 700, retention, false));
        assert!(should_remove(".cache", 400, retention, false));
    }

    #[test]
    fn test_dot_entries_never_removed() {
        let retention = Retention {
            retired: 1000,
            hidden_retired: 1000,
        };
        assert!(!should_remove(".", 0, retention, false));
        assert!(!should_remove("..", 0, retention, false));
    }

    #[test]
    fn test_force_removes_everything() {
        let retention = Retention {
            retired: 1000,
            hidden_retired: 1000,
        };
        assert!(should_remove("fresh", 2000, retention, true));
    }
}
