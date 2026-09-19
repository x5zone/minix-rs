//! Login-time stamps for `who`: epoch seconds to the classic 12-column
//! `ctime` slice.
//!
//! Ground truth: `minix3/usr.bin/who/who.c:317` prints `%.12s` of
//! `ctime(&t)` starting at byte 4 — `"Sep 20 10:30"` (month abbreviation,
//! space-padded day, `hh:mm`). `ctime` renders local time; this layer has
//! no time-zone database, so stamps are UTC — a declared boundary
//! (12-process-tools.md §5), same hosted-first trade as the other faces.
//!
//! The calendar math is Howard Hinnant's `civil_from_days` (the inverse
//! of `days_from_civil`, whose Gregorian form `10-doctools`' `cal.rs`
//! carries): days from 1970-01-01 to a proleptic-Gregorian date, no
//! tables, no libc.

use crate::ProcError;

/// Month abbreviations in `ctime` order (`Jan` through `Dec`).
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Days since 1970-01-01 to `(year, month, day)` in the proleptic
/// Gregorian calendar. `z` may be negative (pre-1970 logins).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Format `epoch` seconds as the 12-column stamp `who` prints:
/// `Sep 20 10:30` (day space-padded, hours and minutes zero padded).
/// Writes into `out` and returns the used byte count.
pub fn format_login_time(epoch: u32, out: &mut [u8]) -> Result<usize, ProcError> {
    let days = epoch as i64 / 86_400;
    let seconds = epoch % 86_400;
    let (_year, month, day) = civil_from_days(days);
    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    if out.len() < 12 {
        return Err(ProcError::InvalidArgument);
    }
    out[0..3].copy_from_slice(MONTHS[(month - 1) as usize].as_bytes());
    out[3] = b' ';
    out[4] = if day < 10 { b' ' } else { b'0' + (day / 10) as u8 };
    out[5] = b'0' + (day % 10) as u8;
    out[6] = b' ';
    out[7] = b'0' + (hour / 10) as u8;
    out[8] = b'0' + (hour % 10) as u8;
    out[9] = b':';
    out[10] = b'0' + (minute / 10) as u8;
    out[11] = b'0' + (minute % 10) as u8;
    Ok(12)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epoch_zero_is_new_years() {
        let mut out = [0u8; 16];
        let used = format_login_time(0, &mut out).unwrap();
        assert_eq!(&out[..used], b"Jan  1 00:00");
    }

    #[test]
    fn test_known_stamp_september_nine_2001() {
        // 1_000_000_000 seconds: the famous party time, UTC.
        let mut out = [0u8; 16];
        let used = format_login_time(1_000_000_000, &mut out).unwrap();
        assert_eq!(&out[..used], b"Sep  9 01:46");
    }

    #[test]
    fn test_leap_year_february_twenty_nine() {
        // 2024-03-01 minus one day lands on the leap day.
        let mut out = [0u8; 16];
        let used = format_login_time(1_709_164_800, &mut out).unwrap();
        assert_eq!(&out[..used], b"Feb 29 00:00");
    }

    #[test]
    fn test_civil_matches_days_from_civil() {
        // Round-trip against the Gregorian encoder in 10-doctools' cal.rs
        // (independent implementation, same contract).
        for &epoch in &[0u32, 86_399, 86_400, 1_000_000_000, 2_147_483_647] {
            let days = epoch as i64 / 86_400;
            let (y, m, d) = civil_from_days(days);
            let back = minix_doctools::cal::days_from_civil(y as i32, m, d).unwrap();
            assert_eq!(back, days, "epoch {epoch}");
        }
    }
}
