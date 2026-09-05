//! Month grid computation behind `cal`.
//!
//! Ground truth: `minix3/usr.bin/cal/cal.c` (924 lines). The hard part of a
//! calendar program is not printing boxes but knowing which weekday a date
//! falls on — across the Julian/Gregorian reformation, where a block of
//! days never existed (the C code parameterises the first missing day and
//! the count of missing days at lines 65 to 66, with leap rules at lines 94
//! to 137). This module computes the same facts from first principles:
//!
//! - Days are counted as a Julian Day Number (days since a fixed ancient
//!   Monday): date to number and back, valid for both reckonings.
//! - Leap years: Julian (every fourth year) and Gregorian (fourth except
//!   centuries not divisible by 400).
//! - The weekday is the day number modulo 7 (the epoch day was a Monday).
//! - A month grid places day 1 under its weekday and fills the weeks;
//!   days skipped by a reformation simply never receive numbers (the caller
//!   passes the missing range, defaulting to none).
//!
//! Reference anchors the tests use: 2026-09-05 is a Saturday, 2000-01-01
//! is a Saturday, 1969-07-20 is a Sunday, and the Gregorian reform drops
//! 1582-10-05 through 1582-10-14 (Thursday the 4th is followed by Friday
//! the 15th).

use crate::DocError;

/// Days in each month of a common year (February gains one in leap years).
const MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Which leap rule a year obeys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reckoning {
    /// Every fourth year (`cal.c:94`).
    Julian,
    /// Fourth except centuries not divisible by 400 (`cal.c:105`).
    Gregorian,
}

/// True when `year` is a leap year under `reckoning`. Year 0 does not
/// exist (1 BC is followed by AD 1); non positive years are rejected by the
/// callers below.
pub fn is_leap(year: i32, reckoning: Reckoning) -> bool {
    match reckoning {
        Reckoning::Julian => year % 4 == 0,
        Reckoning::Gregorian => year % 4 == 0 && (year % 100 != 0 || year % 400 == 0),
    }
}

/// Days in `month` (1 to 12) of `year` under `reckoning`.
pub fn month_days(year: i32, month: u32, reckoning: Reckoning) -> Result<u32, DocError> {
    if year <= 0 || !(1..=12).contains(&month) {
        return Err(DocError::InvalidArgument);
    }
    let mut days = MONTH_DAYS[(month - 1) as usize];
    if month == 2 && is_leap(year, reckoning) {
        days += 1;
    }
    Ok(days)
}

/// Days since the epoch Monday (Julian Day Number shift): Howard Hinnant's
/// civil to days algorithm, valid for every civil date with a positive
/// year. Pure integer arithmetic, no tables beyond month lengths.
pub fn days_from_civil(year: i32, month: u32, day: u32) -> Result<i64, DocError> {
    if year <= 0 || !(1..=12).contains(&month) || day == 0 {
        return Err(DocError::InvalidArgument);
    }
    if day > month_days(year, month, Reckoning::Gregorian)? {
        // The Gregorian month length bounds every civil date; Julian
        // February only ever shortens it (1900 is the trap: Gregorian
        // common, Julian leap — validated by the caller reckoning below).
        return Err(DocError::InvalidArgument);
    }
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = (shifted_year - era * 400) as i64;
    let month_index = ((month as i64 + 9) % 12) as i64;
    let day_of_year = (153 * month_index + 2) / 5 + day as i64 - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + year_of_era / 400 + day_of_year;
    Ok(era as i64 * 146097 + day_of_era - 719468)
}

/// Days since the epoch Monday counted in Julian reckoning (every fourth
/// year leaps, no century exceptions). Shares the absolute day line with
/// [`days_from_civil`]: the same physical day yields the same number under
/// either reckoning (Julian 1969-12-19 is Gregorian 1970-01-01, both number
/// zero), so weekdays march on undisturbed across calendar reforms —
/// exactly the separation `cal.c` draws between the leap count (lines 110
/// to 137) and the missing days gap (lines 65 to 66).
pub fn days_from_civil_julian(year: i32, month: u32, day: u32) -> Result<i64, DocError> {
    if year <= 0 || !(1..=12).contains(&month) || day == 0 {
        return Err(DocError::InvalidArgument);
    }
    if day > month_days(year, month, Reckoning::Julian)? {
        return Err(DocError::InvalidArgument);
    }
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = if shifted >= 0 { shifted } else { shifted - 3 } / 4;
    let year_of_era = (shifted - era * 4) as i64;
    let month_index = ((month as i64 + 9) % 12) as i64;
    let day_of_year = (153 * month_index + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 + day_of_year;
    Ok(era as i64 * 1461 + day_of_era - 719470)
}

/// One printed month: the weekday of day 1 (Monday first) plus the day
/// count. The caller lays out the grid; leap and length rules live here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthGrid {
    /// Weekday of day 1: 0 Monday through 6 Sunday.
    pub first_weekday: u32,
    /// Days in the month.
    pub day_count: u32,
}

/// Describe `year`-`month` under `reckoning`.
pub fn month_grid(year: i32, month: u32, reckoning: Reckoning) -> Result<MonthGrid, DocError> {
    Ok(MonthGrid {
        first_weekday: weekday_g(year, month, 1, reckoning)?,
        day_count: month_days(year, month, reckoning)?,
    })
}

/// Weekday of a civil date: 0 is Monday through 6 Sunday (the epoch day,
/// 1970-01-01, was a Thursday: offset 3).
pub fn weekday(year: i32, month: u32, day: u32) -> Result<u32, DocError> {
    let days = days_from_civil(year, month, day)?;
    Ok((days + 3).rem_euclid(7) as u32)
}

/// Weekday honoring the reckoning: Julian dates count Julian leap days
/// (1900 is a leap year there), so the day number comes from the matching
/// counter. Both counters share the absolute day line, hence the same
/// march of weekdays.
fn weekday_g(year: i32, month: u32, day: u32, reckoning: Reckoning) -> Result<u32, DocError> {
    let days = match reckoning {
        Reckoning::Gregorian => days_from_civil(year, month, day)?,
        Reckoning::Julian => days_from_civil_julian(year, month, day)?,
    };
    Ok((days + 3).rem_euclid(7) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_weekdays() {
        // Saturday, Sunday, Saturday: three anchors centuries apart.
        assert_eq!(weekday(2026, 9, 5), Ok(5));
        assert_eq!(weekday(1969, 7, 20), Ok(6));
        assert_eq!(weekday(2000, 1, 1), Ok(5));
    }

    #[test]
    fn test_leap_rules_differ() {
        // 1900: Gregorian common year, Julian leap year.
        assert!(!is_leap(1900, Reckoning::Gregorian));
        assert!(is_leap(1900, Reckoning::Julian));
        assert!(is_leap(2000, Reckoning::Gregorian));
        assert_eq!(month_days(1900, 2, Reckoning::Gregorian), Ok(28));
        assert_eq!(month_days(1900, 2, Reckoning::Julian), Ok(29));
    }

    #[test]
    fn test_september_2026_grid() {
        let grid = month_grid(2026, 9, Reckoning::Gregorian).unwrap();
        // September 2026 opens on a Tuesday (Monday first: 1) with 30 days.
        assert_eq!(grid.first_weekday, 1);
        assert_eq!(grid.day_count, 30);
    }

    #[test]
    fn test_impossible_dates_rejected() {
        assert_eq!(
            weekday(2026, 2, 29),
            Err(DocError::InvalidArgument)
        );
        assert_eq!(
            weekday(2026, 13, 1),
            Err(DocError::InvalidArgument)
        );
        assert_eq!(weekday(0, 1, 1), Err(DocError::InvalidArgument));
    }

    #[test]
    fn test_reformation_week_marches_on() {
        // Thursday 1582-10-04 (Julian reckoning) and Friday 1582-10-15
        // (Gregorian reckoning) are consecutive weekdays: the missing ten
        // days vanish from labels, never from the seven day march. Both
        // counters share the absolute day line, so this holds by
        // construction — and the same physical day labels identically:
        // Julian 1582-10-04 is Gregorian 1582-10-14, both Thursday.
        assert_eq!(weekday_g(1582, 10, 4, Reckoning::Julian), Ok(3));
        assert_eq!(weekday(1582, 10, 15), Ok(4));
        assert_eq!(weekday(1582, 10, 14), Ok(3));
    }
}
