//! `date`'s calendar core: epoch seconds to a broken-down UTC time, then a
//! `strftime` subset rendered into a caller buffer.
//!
//! Ground truth: `minix3/bin/date/date.c`. With no operand `date` reads the
//! clock, breaks it down with `localtime`, and prints
//! `strftime("+%a %b %e %H:%M:%S %Z %Y")` minus the leading `+`
//! (`date.c:146-166`). The calendar inverse reuses
//! [`crate::stamp::civil_from_days`] (Hinnant's algorithm, the counterpart
//! of `days_from_civil`), so there is no second source of date truth. `%s`
//! therefore does not re-derive the epoch from the fields: [`format_at`] takes
//! the seconds the breakdown came from, which is what `stat -s` needs, while
//! [`format`] (what `date` runs) leaves that directive unmodelled.
//!
//! Time zone: this layer carries no tz database, so breakdowns are UTC and
//! `%Z` renders `UTC`. This is the same declared boundary `stamp.rs` draws
//! for `who` (`date -u` and a `TZ`-unset default both land here), and the
//! set-the-time path (`date operand`) needs `settimeofday`, which has no
//! client wrapper yet — see the `date` binary's residual note.

use crate::ProcError;
use crate::stamp::civil_from_days;

/// Month abbreviations, `ctime`/`%b` order (`date.c` default layout).
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Full month names for `%B`.
const MONTHS_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Weekday abbreviations for `%a`, indexed so `0` is Sunday.
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Full weekday names for `%A`.
const WEEKDAYS_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Days before each month in a common year (January's column).
const MONTH_OFFSET: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

/// A broken-down UTC time — the fields `strftime` consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenTime {
    /// Full year (proleptic Gregorian; can exceed four digits far in the future).
    pub year: i64,
    /// Month, 1 to 12.
    pub month: u32,
    /// Day of month, 1 to 31.
    pub day: u32,
    /// Hour, 0 to 23.
    pub hour: u32,
    /// Minute, 0 to 59.
    pub minute: u32,
    /// Second, 0 to 60 (leap second allowed, though this layer never emits one).
    pub second: u32,
    /// Day of week, 0 (Sunday) through 6 (Saturday).
    pub weekday: u32,
    /// Day of year, 1 to 366.
    pub yearday: u32,
}

/// True for a proleptic-Gregorian leap `year`.
fn is_leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Break `epoch` seconds (UTC) into calendar and clock fields.
///
/// The day-of-week anchors on the epoch: 1970-01-01 (day `0`) is a Thursday,
/// so the index is `(days mod 7 + 4) mod 7` with `0` = Sunday. `div_euclid` /
/// `rem_euclid` keep pre-1970 (negative) seconds in range.
pub fn breakdown(epoch: i64) -> BrokenTime {
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let weekday = (days.rem_euclid(7) + 4).rem_euclid(7) as u32;
    let mut yearday = MONTH_OFFSET[(month - 1) as usize] + day;
    if month > 2 && is_leap(year) {
        yearday += 1;
    }
    BrokenTime {
        year,
        month,
        day,
        hour: (secs / 3_600) as u32,
        minute: ((secs % 3_600) / 60) as u32,
        second: (secs % 60) as u32,
        weekday,
        yearday,
    }
}

/// Render `time` through a `strftime` subset into `out`, returning the byte
/// count used. Recognised directives cover the `date` default layout and the
/// common POSIX set; an unrecognised `%X` is passed through verbatim (a
/// documented subset, matching how the text tools decline un-modelled faces).
pub fn format(fmt: &str, time: &BrokenTime, out: &mut [u8]) -> Result<usize, ProcError> {
    format_at(fmt, time, None, out)
}

/// [`format`] with the epoch seconds that produced `time`, for templates that
/// ask for `%s`. Pass `None` when the breakdown is all the caller has.
pub fn format_at(
    fmt: &str,
    time: &BrokenTime,
    epoch: Option<i64>,
    out: &mut [u8],
) -> Result<usize, ProcError> {
    let mut w = Sink { out, pos: 0 };
    let bytes = fmt.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            w.byte(bytes[i])?;
            i += 1;
            continue;
        }
        i += 1;
        let spec = *bytes.get(i).ok_or(ProcError::InvalidArgument)?;
        i += 1;
        match spec {
            b'%' => w.byte(b'%')?,
            b'n' => w.byte(b'\n')?,
            b't' => w.byte(b'\t')?,
            b'a' => w.text(WEEKDAYS[time.weekday as usize])?,
            b'A' => w.text(WEEKDAYS_FULL[time.weekday as usize])?,
            b'b' | b'h' => w.text(MONTHS[(time.month - 1) as usize])?,
            b'B' => w.text(MONTHS_FULL[(time.month - 1) as usize])?,
            b'c' => {
                // `%a %b %e %H:%M:%S %Y` (the C locale ctime form, minus the year-zone).
                w.text(WEEKDAYS[time.weekday as usize])?;
                w.byte(b' ')?;
                w.text(MONTHS[(time.month - 1) as usize])?;
                w.byte(b' ')?;
                w.space2(time.day)?;
                w.byte(b' ')?;
                w.pad2(time.hour)?;
                w.byte(b':')?;
                w.pad2(time.minute)?;
                w.byte(b':')?;
                w.pad2(time.second)?;
                w.byte(b' ')?;
                w.number(time.year)?;
            }
            b'C' => w.number(time.year.div_euclid(100))?,
            b'd' => w.pad2(time.day)?,
            b'e' => w.space2(time.day)?,
            b'F' => {
                w.number(time.year)?;
                w.byte(b'-')?;
                w.pad2(time.month)?;
                w.byte(b'-')?;
                w.pad2(time.day)?;
            }
            b'H' => w.pad2(time.hour)?,
            b'I' => {
                let h12 = match time.hour % 12 {
                    0 => 12,
                    h => h,
                };
                w.pad2(h12)?;
            }
            b'j' => w.pad_n(time.yearday, 3)?,
            b'm' => w.pad2(time.month)?,
            b'M' => w.pad2(time.minute)?,
            b'p' => w.text(if time.hour < 12 { "AM" } else { "PM" })?,
            b'R' => {
                w.pad2(time.hour)?;
                w.byte(b':')?;
                w.pad2(time.minute)?;
            }
            b'S' => w.pad2(time.second)?,
            b's' => match epoch {
                // `%s` is the seconds this breakdown came from. Taking them
                // from the caller keeps a single calendar encoder in the tree
                // (`crate::stamp::civil_from_days` is the only direction we
                // own); a caller that hands over only fields - `date` does -
                // declines the directive instead of re-deriving the epoch.
                Some(secs) => w.number(secs)?,
                None => return Err(ProcError::InvalidArgument),
            },
            b'T' => {
                w.pad2(time.hour)?;
                w.byte(b':')?;
                w.pad2(time.minute)?;
                w.byte(b':')?;
                w.pad2(time.second)?;
            }
            b'u' => w.byte(b'0' + ((time.weekday + 6) % 7 + 1) as u8)?,
            b'w' => w.byte(b'0' + time.weekday as u8)?,
            b'Y' => w.number(time.year)?,
            b'y' => w.pad2(time.year.rem_euclid(100) as u32)?,
            b'Z' => w.text("UTC")?,
            other => {
                // Unmodelled directive: pass `%X` through, as a documented subset.
                w.byte(b'%')?;
                w.byte(other)?;
            }
        }
    }
    Ok(w.pos)
}

/// Minimal fixed-buffer byte sink that fails closed when full (no allocation
/// in the library; the caller sizes the buffer and re-tries if needed, the
/// same shape C uses growing `bufsiz`).
struct Sink<'a> {
    out: &'a mut [u8],
    pos: usize,
}

impl<'a> Sink<'a> {
    fn byte(&mut self, b: u8) -> Result<(), ProcError> {
        if self.pos >= self.out.len() {
            return Err(ProcError::InvalidArgument);
        }
        self.out[self.pos] = b;
        self.pos += 1;
        Ok(())
    }

    fn text(&mut self, s: &str) -> Result<(), ProcError> {
        for &b in s.as_bytes() {
            self.byte(b)?;
        }
        Ok(())
    }

    fn number(&mut self, value: i64) -> Result<(), ProcError> {
        if value < 0 {
            self.byte(b'-')?;
        }
        let mut digits = [0u8; 20];
        let mut n = value.unsigned_abs();
        let mut len = 0usize;
        loop {
            digits[len] = b'0' + (n % 10) as u8;
            len += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        for d in digits[..len].iter().rev() {
            self.byte(*d)?;
        }
        Ok(())
    }

    fn pad2(&mut self, value: u32) -> Result<(), ProcError> {
        self.byte(b'0' + (value / 10 % 10) as u8)?;
        self.byte(b'0' + (value % 10) as u8)?;
        Ok(())
    }

    fn pad_n(&mut self, value: u32, width: usize) -> Result<(), ProcError> {
        let mut digits = [0u8; 8];
        let mut n = value;
        let mut len = 0usize;
        loop {
            digits[len] = (n % 10) as u8;
            len += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        for _ in len..width {
            self.byte(b'0')?;
        }
        for d in digits[..len].iter().rev() {
            self.byte(b'0' + *d)?;
        }
        Ok(())
    }

    fn space2(&mut self, value: u32) -> Result<(), ProcError> {
        if value < 10 {
            self.byte(b' ')?;
        } else {
            self.byte(b'0' + (value / 10 % 10) as u8)?;
        }
        self.byte(b'0' + (value % 10) as u8)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(fmt: &str, epoch: i64) -> String {
        let t = breakdown(epoch);
        let mut buf = [0u8; 256];
        let used = format(fmt, &t, &mut buf).unwrap();
        String::from_utf8(buf[..used].to_vec()).unwrap()
    }

    // The default layout `date.c:146` uses (`+%a %b %e %H:%M:%S %Z %Y`).
    const DEFAULT_FMT: &str = "%a %b %e %H:%M:%S %Z %Y";

    fn rendered_at(fmt: &str, epoch: i64) -> String {
        let t = breakdown(epoch);
        let mut buf = [0u8; 256];
        let used = format_at(fmt, &t, Some(epoch), &mut buf).unwrap();
        String::from_utf8(buf[..used].to_vec()).unwrap()
    }

    #[test]
    fn percent_s_needs_the_seconds_the_breakdown_came_from() {
        // `stat -s` asks for exactly this letter as its time template (`stat.c:302-306`).
        assert_eq!(rendered_at("%s", 0), "0");
        assert_eq!(rendered_at("%s", 1_000_000_000), "1000000000");
        assert_eq!(rendered_at("%s", -100), "-100");
        assert_eq!(rendered_at("t=%sZ", 1_709_164_800), "t=1709164800Z");
        // A caller handing over only the fields (`date`) does not get the form: the
        // directive is declined rather than re-derived.
        let t = breakdown(0);
        let mut buf = [0u8; 16];
        assert!(format("%s", &t, &mut buf).is_err());
    }

    #[test]
    fn test_epoch_zero_default_layout() {
        // 1970-01-01 00:00:00 UTC was a Thursday.
        assert_eq!(rendered(DEFAULT_FMT, 0), "Thu Jan  1 00:00:00 UTC 1970");
    }

    #[test]
    fn test_party_time_2001() {
        // 1_000_000_000: Sun 2001-09-09 01:46:40 UTC (the famous party time).
        assert_eq!(
            rendered(DEFAULT_FMT, 1_000_000_000),
            "Sun Sep  9 01:46:40 UTC 2001"
        );
    }

    #[test]
    fn test_leap_day_2024() {
        // 1_709_164_800 was Feb 29 2024 00:00 (a Thursday).
        assert_eq!(
            rendered(DEFAULT_FMT, 1_709_164_800),
            "Thu Feb 29 00:00:00 UTC 2024"
        );
        // Next second crosses into March (yearday 60 -> 61 check).
        let t = breakdown(1_709_164_800);
        assert_eq!((t.month, t.day, t.yearday), (2, 29, 60));
    }

    #[test]
    fn test_negative_epoch_pre_1970() {
        // -86_400 is 1969-12-31 (a Wednesday).
        assert_eq!(
            rendered(DEFAULT_FMT, -86_400),
            "Wed Dec 31 00:00:00 UTC 1969"
        );
    }

    #[test]
    fn test_space_vs_zero_padded_day() {
        let t = breakdown(0); // Jan 1
        assert_eq!(rendered("%d", 0), "01");
        assert_eq!(rendered("%e", 0), " 1");
        let later = breakdown(86_400 * 20); // Jan 21
        assert_eq!((t.day, later.day), (1, 21));
        assert_eq!(rendered("%e", 86_400 * 20), "21");
    }

    #[test]
    fn test_iso_and_clock_directives() {
        let epoch = 1_709_164_800; // 2024-02-29 00:00:00 Thu
        assert_eq!(rendered("%F", epoch), "2024-02-29");
        assert_eq!(rendered("%T", epoch), "00:00:00");
        assert_eq!(rendered("%Y-%m-%dT%H:%M:%S", epoch), "2024-02-29T00:00:00");
        assert_eq!(rendered("%j", epoch), "060"); // leap day is the 60th
        assert_eq!(rendered("%A", epoch), "Thursday");
        assert_eq!(rendered("%B", epoch), "February");
        assert_eq!(rendered("%%", epoch), "%");
    }

    #[test]
    fn test_twelve_hour_and_am_pm() {
        // 13:00 -> 01 PM.
        let epoch = 13 * 3_600; // 1970-01-01 13:00:00
        assert_eq!(rendered("%I%p", epoch), "01PM");
        assert_eq!(rendered("%H", epoch), "13");
        // midnight -> 12 AM
        assert_eq!(rendered("%I%p", 0), "12AM");
    }

    #[test]
    fn test_century_and_two_digit_year() {
        assert_eq!(rendered("%C", 0), "19");
        assert_eq!(rendered("%y", 0), "70");
        // 2001 -> %C 20, %y 01
        assert_eq!(rendered("%C", 1_000_000_000), "20");
        assert_eq!(rendered("%y", 1_000_000_000), "01");
    }
}
