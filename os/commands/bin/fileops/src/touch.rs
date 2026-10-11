//! `touch` time decisions.
//!
//! Ground truth: `minix3/usr.bin/touch/touch.c` (NetBSD `touch.c`), the
//! `-t` / obsolete-specification path `stime_arg1` and `stime_arg2`. A time
//! operand has the shape `[[CC]YY]MMDDhhmm[.SS]`: two-digit fields read from
//! the left, and any field the operand leaves out keeps the value it has right
//! now (that is the whole point of `touch -t 0501` meaning "May 1st of this
//! year at the current hour and minute"). Seconds default to `00`, never to
//! the current seconds, and only an explicit `.SS` sets them. A two-digit year
//! pivots at 69: `00..68` land in `2000..2068`, `69..99` in `1969..1999`
//! (`touch.c` `if (yearset < 69)`).
//!
//! Timezone caveat, stated plainly: C runs the fields through `localtime` and
//! `mktime` with `tm_isdst = -1`, so the wall-clock fields are interpreted in
//! the machine's local zone with daylight saving. This tree ships no timezone
//! library (`date` broke down times as UTC for the same reason), so this module
//! interprets the fields as UTC. The default `touch file` path is unaffected —
//! it uses the raw epoch clock and touches no calendar fields at all.
//!
//! The civil-calendar helpers are Howard Hinnant's algorithms; they are
//! anchored by two independent facts (`1970-01-01` is day 0 and `2000-01-01`
//! is day 11016) and cross-checked by a round-trip test, so the epoch the
//! parser returns for a full specification can be trusted.

use crate::FileOpError;

/// Seconds in one day.
const SECONDS_PER_DAY: i64 = 86_400;

/// Days from the Unix epoch to the civil date `y-m-d` (month 1..12). Hinnant's
/// `days_from_civil`, which shifts the year so the leap day falls last.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// The inverse of [`days_from_civil`]: the civil date for a day count since the
/// epoch, as `(year, month 1..12, day)`.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Decompose an epoch second count into `(year, month 1..12, day, hour)` in
/// UTC. Minutes are not returned: every `[[CC]YY]MMDDhhmm` branch of
/// [`parse_time_spec`] supplies them, so the current minute is never needed.
fn break_down_utc(epoch: i64) -> (i64, i64, i64, i64) {
    let days = epoch.div_euclid(SECONDS_PER_DAY);
    let tod = epoch.rem_euclid(SECONDS_PER_DAY);
    let (y, mo, d) = civil_from_days(days);
    (y, mo, d, tod / 3600)
}

/// Recompose `(year, month 1..12, day, hour, minute, second)` into an epoch
/// second count in UTC.
fn compose_utc(y: i64, mo: i64, d: i64, hh: i64, mi: i64, ss: i64) -> i64 {
    days_from_civil(y, mo, d) * SECONDS_PER_DAY + hh * 3600 + mi * 60 + ss
}

/// Parse an `[[CC]YY]MMDDhhmm[.SS]` specification relative to `now_epoch`,
/// returning the target epoch seconds. The fields absent from `spec` are taken
/// from `now_epoch`; seconds default to zero unless `.SS` is given. A length
/// that is not an even count of one to six two-digit fields, a non-digit, or a
/// malformed `.SS` is rejected.
pub fn parse_time_spec(now_epoch: i64, spec: &str) -> Result<i64, FileOpError> {
    if spec.is_empty() {
        return Err(FileOpError::InvalidArgument);
    }
    // Split an optional ".SS" (exactly two digits) off the tail.
    let (fields, sec) = match spec.split_once('.') {
        Some((head, ss)) => {
            let b = ss.as_bytes();
            if b.len() != 2 || !b.iter().all(|c| c.is_ascii_digit()) {
                return Err(FileOpError::InvalidArgument);
            }
            (head, ((b[0] - b'0') * 10 + (b[1] - b'0')) as i64)
        }
        None => (spec, 0),
    };
    let bytes = fields.as_bytes();
    if bytes.len() % 2 != 0 || bytes.is_empty() || bytes.len() > 12 {
        return Err(FileOpError::InvalidArgument);
    }
    if !bytes.iter().all(|c| c.is_ascii_digit()) {
        return Err(FileOpError::InvalidArgument);
    }

    let mut pos = 0;
    let mut pair = || -> i64 {
        let v = ((bytes[pos] - b'0') * 10 + (bytes[pos + 1] - b'0')) as i64;
        pos += 2;
        v
    };

    let (by, bmo, bd, bhh) = break_down_utc(now_epoch);
    let y;
    let mo;
    let d;
    let hh;
    let mi;
    // The switch entry is the length: it names the first (leftmost, coarsest)
    // field the operand supplies; the remaining pairs run down to minutes.
    // Fields shorter than the entry are kept from the current time.
    match bytes.len() {
        12 => {
            let cc = pair();
            let yy = pair();
            y = cc * 100 + yy;
            mo = pair();
            d = pair();
            hh = pair();
            mi = pair();
        }
        10 => {
            let yy = pair();
            y = if yy < 69 { 2000 + yy } else { 1900 + yy };
            mo = pair();
            d = pair();
            hh = pair();
            mi = pair();
        }
        8 => {
            y = by;
            mo = pair();
            d = pair();
            hh = pair();
            mi = pair();
        }
        6 => {
            y = by;
            mo = bmo;
            d = pair();
            hh = pair();
            mi = pair();
        }
        4 => {
            y = by;
            mo = bmo;
            d = bd;
            hh = pair();
            mi = pair();
        }
        2 => {
            y = by;
            mo = bmo;
            d = bd;
            hh = bhh;
            mi = pair();
        }
        _ => return Err(FileOpError::InvalidArgument),
    }

    Ok(compose_utc(y, mo, d, hh, mi, sec))
}

/// True when an operand is an obsolete time specification: exactly 8 or 10
/// digits with nothing else (`touch.c` accepts these bare as a time only when
/// a time is not already set and more than one operand follows).
pub fn is_obsolete_time_spec(spec: &str) -> bool {
    (spec.len() == 8 || spec.len() == 10) && spec.bytes().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_from_civil_anchors() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 1, 1), 10957);
        // 2000 is a leap year: February has 29 days.
        assert_eq!(
            days_from_civil(2000, 3, 1) - days_from_civil(2000, 2, 28),
            2
        );
    }

    #[test]
    fn test_civil_round_trip() {
        for z in [-40_000i64, -1, 0, 1, 11016, 20000, 59_999] {
            let (y, mo, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, mo, d), z, "round trip failed for {z}");
        }
    }

    #[test]
    fn test_full_ccyy_specification() {
        // now = 2000-01-01 00:00:00 UTC. A 12-digit spec fully overrides it.
        let now = 946_684_800;
        let got = parse_time_spec(now, "202501021530.45").unwrap();
        assert_eq!(got, compose_utc(2025, 1, 2, 15, 30, 45));
        assert_eq!(got, 1_735_831_845);
    }

    #[test]
    fn test_two_digit_year_pivot() {
        let now = 946_684_800; // 2000-01-01 00:00 UTC
                               // YY=99 (>= 69) → 1999-12-31 23:59 UTC = 2000-01-01 00:00 minus 60s.
        assert_eq!(parse_time_spec(now, "9912312359").unwrap(), 946_684_740);
        // YY=70 (>= 69) → 1970-01-01 00:00 = 0.
        assert_eq!(parse_time_spec(now, "7001010000").unwrap(), 0);
        // YY=69 (69 is NOT < 69) → 1969-01-01 00:00.
        assert_eq!(parse_time_spec(now, "6901010000").unwrap(), -31_536_000);
    }

    #[test]
    fn test_omitted_fields_carry_current_time() {
        let now = 946_684_800; // 2000-01-01 00:00:00 UTC
                               // MMDDhhmm overrides month/day/hour/minute; year stays 2000.
        assert_eq!(parse_time_spec(now, "05010000").unwrap(), 957_139_200); // 2000-05-01 00:00
                                                                            // Only minutes (len 2): day/hour/month/year from now, but seconds → 0.
        assert_eq!(
            parse_time_spec(now, "30").unwrap(),
            compose_utc(2000, 1, 1, 0, 30, 0)
        );
    }

    #[test]
    fn test_seconds_default_zero_unless_explicit() {
        let now = 946_684_800 + 45; // 2000-01-01 00:00:45 UTC
                                    // No ".SS": seconds reset to 0 (NOT carried from now).
        assert_eq!(parse_time_spec(now, "01010000").unwrap(), 946_684_800);
        // ".SS" sets them.
        assert_eq!(parse_time_spec(now, "01010000.30").unwrap(), 946_684_830);
    }

    #[test]
    fn test_malformed_rejected() {
        let now = 0;
        assert!(parse_time_spec(now, "").is_err());
        assert!(parse_time_spec(now, "abc").is_err()); // non-digit
        assert!(parse_time_spec(now, "12345").is_err()); // odd length
        assert!(parse_time_spec(now, "1234567890123").is_err()); // > 6 pairs
        assert!(parse_time_spec(now, "01010000.3").is_err()); // .SS one digit
        assert!(parse_time_spec(now, "01010000.3x").is_err()); // .SS non-digit
    }

    #[test]
    fn test_obsolete_time_spec_shape() {
        assert!(is_obsolete_time_spec("01010000")); // 8 digits
        assert!(is_obsolete_time_spec("0001010000")); // 10 digits
        assert!(!is_obsolete_time_spec("0101000")); // 7
        assert!(!is_obsolete_time_spec("0101000a")); // non-digit
    }
}
