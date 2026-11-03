//! Free-form date parser behind `touch -d`, ported from
//! `minix3/lib/libutil/parsedate.y`.
//!
//! C's `touch -d` calls `parsedate(arg, NULL, NULL)` (`touch.c:214-218`),
//! which tokenizes the argument with the lexer at `parsedate.y:772-860`,
//! parses it with the grammar at `:113-383`, and converts the collected
//! fields with `Convert` (`:625-660`) and the relative-date helpers
//! (`:662-730`). This module keeps that pipeline: the same tables, the same
//! token classes, the same greedy item sequence the LALR parser produces,
//! and the same field arithmetic.
//!
//! The tree has no timezone database: `date` and `touch` run on UTC
//! (`crate::dates`' note, "Minix with no timezone set runs UTC"). C's
//! `USE_LOCAL_TIME` path therefore reduces to `mktime_z(NULL, …)` here, and
//! `DSTcorrect`'s wall-clock correction is identically zero. An explicit
//! timezone token still shifts the result by its minutes east of UTC.
//!
//! Deliberate C quirks reproduced rather than fixed:
//! - a month/day word of exactly three letters is matched by its first three
//!   letters against the table in order (`sep` → September);
//! - `12 pm` adds twelve hours to twelve, landing on the next midnight
//!   (`Convert` does not fold the 12-hour clock);
//! - an `ago` negates the **whole** relative accumulator, not just the unit
//!   before it;
//! - a bare `+500` is a *signed* token, and the ISO/`-0500` rules negate
//!   signed fields again, so their sign flips twice.

use alloc::string::String;
use alloc::vec::Vec;

/// `USE_LOCAL_TIME` (`parsedate.y:45`): the timezone field holds no explicit
/// zone and the conversion runs in local time (UTC in this tree).
const USE_LOCAL_TIME: i64 = 99999;

/// `SECSPERDAY` (`parsedate.y:43`).
const SECSPERDAY: i64 = 24 * 60 * 60;

// ── tables (`parsedate.y:388-604`) ───────────────────────────────────────

/// `MonthDayTable` (`:389-415`): month and weekday names, with the value each
/// carries.
const MONTH_DAY_TABLE: &[(&str, Word, i64)] = &[
    ("january", Word::Month, 1),
    ("february", Word::Month, 2),
    ("march", Word::Month, 3),
    ("april", Word::Month, 4),
    ("may", Word::Month, 5),
    ("june", Word::Month, 6),
    ("july", Word::Month, 7),
    ("august", Word::Month, 8),
    ("september", Word::Month, 9),
    ("sept", Word::Month, 9),
    ("october", Word::Month, 10),
    ("november", Word::Month, 11),
    ("december", Word::Month, 12),
    ("sunday", Word::Day, 0),
    ("monday", Word::Day, 1),
    ("tuesday", Word::Day, 2),
    ("tues", Word::Day, 2),
    ("wednesday", Word::Day, 3),
    ("wednes", Word::Day, 3),
    ("thursday", Word::Day, 4),
    ("thur", Word::Day, 4),
    ("thurs", Word::Day, 4),
    ("friday", Word::Day, 5),
    ("saturday", Word::Day, 6),
];

/// `UnitsTable` (`:418-430`): the unit after a count.
const UNITS_TABLE: &[(&str, Word, i64)] = &[
    ("year", Word::MonthUnit, 12),
    ("month", Word::MonthUnit, 1),
    ("fortnight", Word::MinuteUnit, 14 * 24 * 60),
    ("week", Word::MinuteUnit, 7 * 24 * 60),
    ("day", Word::MinuteUnit, 24 * 60),
    ("hour", Word::MinuteUnit, 60),
    ("minute", Word::MinuteUnit, 1),
    ("min", Word::MinuteUnit, 1),
    ("second", Word::SecUnit, 1),
    ("sec", Word::SecUnit, 1),
];

/// `OtherTable` (`:433-467`): relative words and small counts.
const OTHER_TABLE: &[(&str, Word, i64)] = &[
    ("tomorrow", Word::MinuteUnit, 24 * 60),
    ("yesterday", Word::MinuteUnit, -24 * 60),
    ("today", Word::MinuteUnit, 0),
    ("now", Word::MinuteUnit, 0),
    ("last", Word::Num, -1),
    ("this", Word::MinuteUnit, 0),
    ("next", Word::Num, 2),
    ("first", Word::Num, 1),
    ("one", Word::Num, 1),
    ("two", Word::Num, 2),
    ("third", Word::Num, 3),
    ("three", Word::Num, 3),
    ("fourth", Word::Num, 4),
    ("four", Word::Num, 4),
    ("fifth", Word::Num, 5),
    ("five", Word::Num, 5),
    ("sixth", Word::Num, 6),
    ("six", Word::Num, 6),
    ("seventh", Word::Num, 7),
    ("seven", Word::Num, 7),
    ("eighth", Word::Num, 8),
    ("eight", Word::Num, 8),
    ("ninth", Word::Num, 9),
    ("nine", Word::Num, 9),
    ("tenth", Word::Num, 10),
    ("ten", Word::Num, 10),
    ("eleventh", Word::Num, 11),
    ("eleven", Word::Num, 11),
    ("twelfth", Word::Num, 12),
    ("twelve", Word::Num, 12),
    ("ago", Word::Ago, 1),
];

/// `TimezoneTable` (`:470-561`): values are minutes east of UTC.
const TIMEZONE_TABLE: &[(&str, Word, i64)] = &[
    ("gmt", Word::Zone, 0),
    ("ut", Word::Zone, 0),
    ("utc", Word::Zone, 0),
    ("wet", Word::Zone, 0),
    ("bst", Word::DayZone, 0),
    ("wat", Word::Zone, 60),
    ("at", Word::Zone, 120),
    ("ast", Word::Zone, 240),
    ("adt", Word::DayZone, 240),
    ("est", Word::Zone, 300),
    ("edt", Word::DayZone, 300),
    ("cst", Word::Zone, 360),
    ("cdt", Word::DayZone, 360),
    ("mst", Word::Zone, 420),
    ("mdt", Word::DayZone, 420),
    ("pst", Word::Zone, 480),
    ("pdt", Word::DayZone, 480),
    ("yst", Word::Zone, 540),
    ("ydt", Word::DayZone, 540),
    ("hst", Word::Zone, 600),
    ("hdt", Word::DayZone, 600),
    ("cat", Word::Zone, 600),
    ("ahst", Word::Zone, 600),
    ("nt", Word::Zone, 660),
    ("idlw", Word::Zone, 720),
    ("cet", Word::Zone, -60),
    ("met", Word::Zone, -60),
    ("mewt", Word::Zone, -60),
    ("mest", Word::DayZone, -60),
    ("swt", Word::Zone, -60),
    ("sst", Word::DayZone, -60),
    ("fwt", Word::Zone, -60),
    ("fst", Word::DayZone, -60),
    ("eet", Word::Zone, -120),
    ("bt", Word::Zone, -180),
    ("zp4", Word::Zone, -240),
    ("zp5", Word::Zone, -300),
    ("zp6", Word::Zone, -360),
    ("wast", Word::Zone, -420),
    ("wadt", Word::DayZone, -420),
    ("cct", Word::Zone, -480),
    ("jst", Word::Zone, -540),
    ("east", Word::Zone, -600),
    ("eadt", Word::DayZone, -600),
    ("gst", Word::Zone, -600),
    ("nzt", Word::Zone, -720),
    ("nzst", Word::Zone, -720),
    ("nzdt", Word::DayZone, -720),
    ("idle", Word::Zone, -720),
];

/// `MilitaryTable` (`:564-593`): single-letter zones.
const MILITARY_TABLE: &[(&str, Word, i64)] = &[
    ("a", Word::Zone, 60),
    ("b", Word::Zone, 120),
    ("c", Word::Zone, 180),
    ("d", Word::Zone, 240),
    ("e", Word::Zone, 300),
    ("f", Word::Zone, 360),
    ("g", Word::Zone, 420),
    ("h", Word::Zone, 480),
    ("i", Word::Zone, 540),
    ("k", Word::Zone, 600),
    ("l", Word::Zone, 660),
    ("m", Word::Zone, 720),
    ("n", Word::Zone, -60),
    ("o", Word::Zone, -120),
    ("p", Word::Zone, -180),
    ("q", Word::Zone, -240),
    ("r", Word::Zone, -300),
    ("s", Word::Zone, -360),
    ("t", Word::Zone, -420),
    ("u", Word::Zone, -480),
    ("v", Word::Zone, -540),
    ("w", Word::Zone, -600),
    ("x", Word::Zone, -660),
    ("y", Word::Zone, -720),
    ("z", Word::Zone, 0),
];

/// Which table a word came from — the token class the yacc lexer returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Word {
    Month,
    Day,
    Zone,
    DayZone,
    MinuteUnit,
    MonthUnit,
    SecUnit,
    Num,
    Ago,
}

/// The token classes of `parsedate.y:101-103` plus punctuation characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    /// `tUNUMBER`: an unsigned decimal count.
    Num(i64),
    /// `tSNUMBER`: a count that carried an explicit sign.
    Signed(i64),
    Month(i64),
    Day(i64),
    Zone(i64),
    DayZone(i64),
    /// `tMERIDIAN`: `true` is PM.
    Meridian(bool),
    MinuteUnit(i64),
    MonthUnit(i64),
    SecUnit(i64),
    Ago,
    Dst,
    At,
    /// Any other single character, including `:`, `/`, `,`, `.` and `(`.
    Char(u8),
    /// `tID`: a word no table knows; no grammar rule accepts it.
    Id,
}

/// A UTC breakdown of an instant, the fields C reads from `struct tm`.
#[derive(Debug, Clone, Copy)]
struct Fields {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    /// 0 = Sunday through 6 = Saturday, as `tm_wday`.
    weekday: i64,
}

/// Split seconds since the epoch into UTC calendar fields.
fn utc_fields(seconds: i64) -> Fields {
    let days = seconds.div_euclid(SECSPERDAY);
    let of_day = seconds.rem_euclid(SECSPERDAY);
    let (year, month, day) = civil_from_days(days);
    Fields {
        year,
        month,
        day,
        hour: of_day / 3600,
        minute: (of_day % 3600) / 60,
        second: of_day % 60,
        // 1970-01-01 was a Thursday (4).
        weekday: (days + 4).rem_euclid(7),
    }
}

/// Gregorian civil date to days since 1970-01-01 (Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let year_of_era = shifted - era * 400;
    let month_index = (month + 9).rem_euclid(12);
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146097 + day_of_era - 719468
}

/// Days since 1970-01-01 to a Gregorian date (Hinnant's inverse).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let day_of_era = z - era * 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

// ── lexer (`parsedate.y:772-860`) ────────────────────────────────────────

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn is_alpha(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
}

fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

/// `LookupWord` (`parsedate.y:736-770`): the word tables in C's order.
fn lookup_word(raw: &str) -> Tok {
    let mut word = String::from(raw);
    word.make_ascii_lowercase();
    if word == "am" || word == "a.m." {
        return Tok::Meridian(false);
    }
    if word == "pm" || word == "p.m." {
        return Tok::Meridian(true);
    }
    let abbrev = if word.len() == 3 {
        Some(word.as_str())
    } else if word.len() == 4 && word.ends_with('.') {
        Some(&word[..3])
    } else {
        None
    };
    for &(name, kind, value) in MONTH_DAY_TABLE {
        let hit = match abbrev {
            Some(prefix) => name.as_bytes()[..3] == prefix.as_bytes()[..3],
            None => name == word,
        };
        if hit {
            return match kind {
                Word::Month => Tok::Month(value),
                _ => Tok::Day(value),
            };
        }
    }
    for &(name, kind, value) in TIMEZONE_TABLE {
        if name == word {
            return match kind {
                Word::DayZone => Tok::DayZone(value),
                _ => Tok::Zone(value),
            };
        }
    }
    if word == "dst" {
        return Tok::Dst;
    }
    for &(name, kind, value) in UNITS_TABLE {
        if name == word {
            return unit_token(kind, value);
        }
    }
    // Strip one plural `s` and retry the units table.
    if word.len() > 1 && word.ends_with('s') {
        let singular = &word[..word.len() - 1];
        for &(name, kind, value) in UNITS_TABLE {
            if name == singular {
                return unit_token(kind, value);
            }
        }
    }
    for &(name, kind, value) in OTHER_TABLE {
        if name == word {
            return match kind {
                Word::MinuteUnit => Tok::MinuteUnit(value),
                Word::MonthUnit => Tok::MonthUnit(value),
                Word::SecUnit => Tok::SecUnit(value),
                Word::Ago => Tok::Ago,
                _ => Tok::Num(value),
            };
        }
    }
    if word.len() == 1 && is_alpha(word.as_bytes()[0]) {
        for &(name, _kind, value) in MILITARY_TABLE {
            if name == word {
                return Tok::Zone(value);
            }
        }
    }
    // Drop periods and retry only the timezone table (`:763-769`).
    if word.contains('.') {
        let stripped: String = word.chars().filter(|&c| c != '.').collect();
        for &(name, kind, value) in TIMEZONE_TABLE {
            if name == stripped {
                return match kind {
                    Word::DayZone => Tok::DayZone(value),
                    _ => Tok::Zone(value),
                };
            }
        }
    }
    Tok::Id
}

fn unit_token(kind: Word, value: i64) -> Tok {
    match kind {
        Word::MinuteUnit => Tok::MinuteUnit(value),
        Word::MonthUnit => Tok::MonthUnit(value),
        Word::SecUnit => Tok::SecUnit(value),
        _ => Tok::Id,
    }
}

/// `yylex` (`parsedate.y:772-860`): numbers with an optional sign, words of
/// letters and periods, `@`, and single characters. Parenthesised comments
/// are skipped (nested); an unterminated one ends the token stream.
fn tokenize(text: &str) -> Vec<Tok> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    'outer: while i < bytes.len() {
        while i < bytes.len() && is_space(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let byte = bytes[i];
        if is_digit(byte) || byte == b'-' || byte == b'+' {
            let mut sign = 0i64;
            if byte == b'-' || byte == b'+' {
                sign = if byte == b'-' { -1 } else { 1 };
                i += 1;
                if i >= bytes.len() || !is_digit(bytes[i]) {
                    continue;
                }
            }
            let mut value: i64 = 0;
            while i < bytes.len() && is_digit(bytes[i]) {
                value = value
                    .saturating_mul(10)
                    .saturating_add(i64::from(bytes[i] - b'0'));
                i += 1;
            }
            if sign < 0 {
                value = -value;
            }
            tokens.push(if sign == 0 {
                Tok::Num(value)
            } else {
                Tok::Signed(value)
            });
            continue;
        }
        if is_alpha(byte) {
            let mut word = String::new();
            while i < bytes.len() && (is_alpha(bytes[i]) || bytes[i] == b'.') {
                if word.len() < 19 {
                    word.push(bytes[i] as char);
                }
                i += 1;
            }
            tokens.push(lookup_word(&word));
            continue;
        }
        if byte == b'@' {
            tokens.push(Tok::At);
            i += 1;
            continue;
        }
        if byte == b'(' {
            let mut depth = 0i32;
            loop {
                let ch = match bytes.get(i) {
                    Some(&ch) => ch,
                    None => break 'outer,
                };
                i += 1;
                if ch == b'(' {
                    depth += 1;
                } else if ch == b')' {
                    depth -= 1;
                }
                if depth <= 0 {
                    break;
                }
            }
            continue;
        }
        tokens.push(Tok::Char(byte));
        i += 1;
    }
    tokens
}

// ── parser grammar state (`parsedate.y:74-98`) ───────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Meridian {
    Mer24,
    MerAm,
    MerPm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DstMode {
    On,
    Off,
    Maybe,
}

/// `struct dateinfo` (`parsedate.y:74-98`).
struct DateInfo {
    have_rel: i32,
    have_time: i32,
    have_zone: i32,
    have_date: i32,
    have_day: i32,
    have_full_year: bool,
    timezone: i64,
    day: i64,
    hour: i64,
    minutes: i64,
    month: i64,
    seconds: i64,
    year: i64,
    meridian: Meridian,
    rel_month: i64,
    rel_seconds: i64,
    dst: DstMode,
    day_ordinal: i64,
    day_number: i64,
}

impl DateInfo {
    fn seeded(now: i64) -> Self {
        let fields = utc_fields(now);
        Self {
            have_rel: 0,
            have_time: 0,
            have_zone: 0,
            have_date: 0,
            have_day: 0,
            have_full_year: false,
            timezone: USE_LOCAL_TIME,
            day: fields.day,
            hour: 0,
            minutes: 0,
            month: fields.month,
            seconds: 0,
            year: fields.year,
            meridian: Meridian::Mer24,
            rel_month: 0,
            rel_seconds: 0,
            dst: DstMode::Maybe,
            day_ordinal: 0,
            day_number: 0,
        }
    }
}

fn num_at(tokens: &[Tok], at: usize) -> Option<i64> {
    match tokens.get(at) {
        Some(Tok::Num(value)) => Some(*value),
        _ => None,
    }
}

fn signed_at(tokens: &[Tok], at: usize) -> Option<i64> {
    match tokens.get(at) {
        Some(Tok::Signed(value)) => Some(*value),
        _ => None,
    }
}

fn char_at(tokens: &[Tok], at: usize) -> Option<u8> {
    match tokens.get(at) {
        Some(Tok::Char(byte)) => Some(*byte),
        _ => None,
    }
}

/// Parse one grammar `item` starting at `pos`, updating `info` exactly as the
/// yacc actions do. Returns the position after the item, or `None` for a
/// syntax error (yacc's `yyparse` failure).
fn parse_item(tokens: &[Tok], pos: usize, info: &mut DateInfo) -> Option<usize> {
    // cvsstamp: N '.' N '.' N '.' N '.' N '.' N (`:147-161`).
    if num_at(tokens, pos).is_some() && char_at(tokens, pos + 1) == Some(b'.') {
        let slots = [pos, pos + 2, pos + 4, pos + 6, pos + 8, pos + 10];
        if slots.iter().all(|&slot| num_at(tokens, slot).is_some())
            && (1..6).all(|n| char_at(tokens, slots[n] - 1) == Some(b'.'))
        {
            let mut values = [0i64; 6];
            for (index, &slot) in slots.iter().enumerate() {
                values[index] = num_at(tokens, slot).unwrap_or(0);
            }
            info.year = values[0];
            if info.year < 100 {
                info.year += 1900;
            }
            info.have_full_year = true;
            info.month = values[1];
            info.day = values[2];
            info.hour = values[3];
            info.minutes = values[4];
            info.seconds = values[5];
            info.dst = DstMode::Off;
            info.timezone = 0;
            info.have_time += 1;
            info.have_date += 1;
            info.have_zone += 1;
            return Some(pos + 11);
        }
    }

    // epochdate: AT_SIGN number (`:163-186`); the item then counts as date,
    // time and zone.
    if tokens.get(pos) == Some(&Tok::At) {
        let value = num_at(tokens, pos + 1).or_else(|| signed_at(tokens, pos + 1))?;
        let fields = utc_fields(value);
        info.year = fields.year;
        info.month = fields.month;
        info.day = fields.day;
        info.hour = fields.hour;
        info.minutes = fields.minute;
        info.seconds = fields.second;
        info.have_full_year = true;
        info.dst = DstMode::Off;
        info.timezone = 0;
        info.have_time += 1;
        info.have_date += 1;
        info.have_zone += 1;
        return Some(pos + 2);
    }

    // time_numericzone with seconds (`:223-231`): N ':' N ':' N SNUMBER.
    if let Some(first) = num_at(tokens, pos)
        && char_at(tokens, pos + 1) == Some(b':')
        && let Some(second) = num_at(tokens, pos + 2)
        && char_at(tokens, pos + 3) == Some(b':')
        && let Some(third) = num_at(tokens, pos + 4)
        && let Some(zone) = signed_at(tokens, pos + 5)
    {
        info.hour = first;
        info.minutes = second;
        info.seconds = third;
        info.meridian = Meridian::Mer24;
        info.dst = DstMode::Off;
        info.timezone = -(zone % 100 + (zone / 100) * 60);
        info.have_time += 1;
        info.have_zone += 1;
        return Some(pos + 6);
    }

    // time with seconds, optional meridian or milliseconds (`:201-220`).
    if let Some(first) = num_at(tokens, pos)
        && char_at(tokens, pos + 1) == Some(b':')
        && let Some(second) = num_at(tokens, pos + 2)
        && char_at(tokens, pos + 3) == Some(b':')
        && let Some(third) = num_at(tokens, pos + 4)
    {
        info.hour = first;
        info.minutes = second;
        info.seconds = third;
        if char_at(tokens, pos + 5) == Some(b'.') && num_at(tokens, pos + 6).is_some() {
            info.meridian = Meridian::Mer24;
            info.have_time += 1;
            return Some(pos + 7);
        }
        if let Some(Tok::Meridian(pm)) = tokens.get(pos + 5) {
            info.meridian = if *pm {
                Meridian::MerPm
            } else {
                Meridian::MerAm
            };
            info.have_time += 1;
            return Some(pos + 6);
        }
        info.meridian = Meridian::Mer24;
        info.have_time += 1;
        return Some(pos + 5);
    }

    // time_numericzone without seconds (`:216-222`): N ':' N SNUMBER.
    if let Some(first) = num_at(tokens, pos)
        && char_at(tokens, pos + 1) == Some(b':')
        && let Some(second) = num_at(tokens, pos + 2)
        && let Some(zone) = signed_at(tokens, pos + 3)
    {
        info.hour = first;
        info.minutes = second;
        info.meridian = Meridian::Mer24;
        info.dst = DstMode::Off;
        info.timezone = -(zone % 100 + (zone / 100) * 60);
        info.have_time += 1;
        info.have_zone += 1;
        return Some(pos + 4);
    }

    // time: N ':' N o_merid (`:195-200`).
    if let Some(first) = num_at(tokens, pos)
        && char_at(tokens, pos + 1) == Some(b':')
        && let Some(second) = num_at(tokens, pos + 2)
    {
        info.hour = first;
        info.minutes = second;
        info.seconds = 0;
        match tokens.get(pos + 3) {
            Some(Tok::Meridian(pm)) => {
                info.meridian = if *pm {
                    Meridian::MerPm
                } else {
                    Meridian::MerAm
                };
                info.have_time += 1;
                return Some(pos + 4);
            }
            _ => {
                info.meridian = Meridian::Mer24;
                info.have_time += 1;
                return Some(pos + 3);
            }
        }
    }

    // time: N MERIDIAN (`:188-194`).
    if let Some(hour) = num_at(tokens, pos)
        && let Some(Tok::Meridian(pm)) = tokens.get(pos + 1)
    {
        info.hour = hour;
        info.minutes = 0;
        info.seconds = 0;
        info.meridian = if *pm {
            Meridian::MerPm
        } else {
            Meridian::MerAm
        };
        info.have_time += 1;
        return Some(pos + 2);
    }

    // zone: tZONE | tDAYZONE | tZONE tDST (`:233-248`).
    match tokens.get(pos) {
        Some(Tok::Zone(value)) => {
            info.timezone = *value;
            info.dst = DstMode::Off;
            if tokens.get(pos + 1) == Some(&Tok::Dst) {
                info.dst = DstMode::On;
                info.have_zone += 1;
                return Some(pos + 2);
            }
            info.have_zone += 1;
            return Some(pos + 1);
        }
        Some(Tok::DayZone(value)) => {
            info.timezone = *value;
            info.dst = DstMode::On;
            info.have_zone += 1;
            return Some(pos + 1);
        }
        _ => {}
    }

    // date patterns (`:250-292`).
    if let Some(first) = num_at(tokens, pos) {
        if char_at(tokens, pos + 1) == Some(b'/')
            && let Some(second) = num_at(tokens, pos + 2)
        {
            if char_at(tokens, pos + 3) == Some(b'/')
                && let Some(third) = num_at(tokens, pos + 4)
            {
                if first >= 100 {
                    info.year = first;
                    info.month = second;
                    info.day = third;
                } else {
                    info.month = first;
                    info.day = second;
                    info.year = third;
                }
                info.have_date += 1;
                return Some(pos + 5);
            }
            info.month = first;
            info.day = second;
            info.have_date += 1;
            return Some(pos + 3);
        }
        // ISO 8601: N SNUMBER SNUMBER (`:262-267`).
        if let (Some(month), Some(day)) = (signed_at(tokens, pos + 1), signed_at(tokens, pos + 2)) {
            info.year = first;
            info.have_full_year = true;
            info.month = -month;
            info.day = -day;
            info.have_date += 1;
            return Some(pos + 3);
        }
        if let Some(Tok::Month(month)) = tokens.get(pos + 1) {
            if let Some(third) = num_at(tokens, pos + 2) {
                info.month = *month;
                info.day = first;
                info.year = third;
                info.have_date += 1;
                return Some(pos + 3);
            }
            if let Some(third) = signed_at(tokens, pos + 2) {
                // e.g. 17-JUN-1992 (`:269-274`).
                info.day = first;
                info.month = *month;
                info.year = -third;
                info.have_date += 1;
                return Some(pos + 3);
            }
            info.month = *month;
            info.day = first;
            info.have_date += 1;
            return Some(pos + 2);
        }
        // day: N tDAY (`:250-259`).
        if let Some(Tok::Day(day)) = tokens.get(pos + 1) {
            info.day_ordinal = first;
            info.day_number = *day;
            info.have_day += 1;
            return Some(pos + 2);
        }
        // relunit with a count (`:294-330`).
        if let Some(next) = tokens.get(pos + 1) {
            match next {
                Tok::MinuteUnit(unit) => {
                    info.rel_seconds += first * unit * 60;
                    info.have_rel += 1;
                    return parse_ago(tokens, pos + 2, info);
                }
                Tok::SecUnit(unit) => {
                    info.rel_seconds += first * unit;
                    info.have_rel += 1;
                    return parse_ago(tokens, pos + 2, info);
                }
                Tok::MonthUnit(unit) => {
                    info.rel_month += first * unit;
                    info.have_rel += 1;
                    return parse_ago(tokens, pos + 2, info);
                }
                _ => {}
            }
        }
        // number (`:332-357`).
        number_rule(info, first);
        return Some(pos + 1);
    }

    if let Some(first) = signed_at(tokens, pos) {
        if let Some(next) = tokens.get(pos + 1) {
            match next {
                Tok::MinuteUnit(unit) => {
                    info.rel_seconds += first * unit * 60;
                    info.have_rel += 1;
                    return parse_ago(tokens, pos + 2, info);
                }
                Tok::SecUnit(unit) => {
                    info.rel_seconds += first * unit;
                    info.have_rel += 1;
                    return parse_ago(tokens, pos + 2, info);
                }
                Tok::MonthUnit(unit) => {
                    info.rel_month += first * unit;
                    info.have_rel += 1;
                    return parse_ago(tokens, pos + 2, info);
                }
                _ => return None,
            }
        }
        return None;
    }

    match tokens.get(pos) {
        Some(Tok::Month(month)) => {
            if let Some(day) = num_at(tokens, pos + 1) {
                if char_at(tokens, pos + 2) == Some(b',')
                    && let Some(year) = num_at(tokens, pos + 3)
                {
                    info.month = *month;
                    info.day = day;
                    info.year = year;
                    info.have_date += 1;
                    return Some(pos + 4);
                }
                info.month = *month;
                info.day = day;
                info.have_date += 1;
                return Some(pos + 2);
            }
            None
        }
        Some(Tok::Day(day)) => {
            info.day_ordinal = 1;
            info.day_number = *day;
            info.have_day += 1;
            if char_at(tokens, pos + 1) == Some(b',') {
                return Some(pos + 2);
            }
            Some(pos + 1)
        }
        Some(Tok::MinuteUnit(unit)) => {
            info.rel_seconds += unit * 60;
            info.have_rel += 1;
            parse_ago(tokens, pos + 1, info)
        }
        Some(Tok::SecUnit(unit)) => {
            info.rel_seconds += *unit;
            info.have_rel += 1;
            parse_ago(tokens, pos + 1, info)
        }
        Some(Tok::MonthUnit(unit)) => {
            info.rel_month += unit;
            info.have_rel += 1;
            parse_ago(tokens, pos + 1, info)
        }
        _ => None,
    }
}

/// `rel: relunit tAGO` (`:294-301`): the `ago` negates both accumulators,
/// including everything earlier in the same argument.
fn parse_ago(tokens: &[Tok], pos: usize, info: &mut DateInfo) -> Option<usize> {
    if tokens.get(pos) == Some(&Tok::Ago) {
        info.rel_seconds = -info.rel_seconds;
        info.rel_month = -info.rel_month;
        return Some(pos + 1);
    }
    Some(pos)
}

/// `number` (`:332-357`).
fn number_rule(info: &mut DateInfo, value: i64) {
    if info.have_time > 0 && info.have_date > 0 && info.have_rel == 0 {
        info.year = value;
    } else if value > 10000 {
        info.have_date += 1;
        info.day = value % 100;
        info.month = (value / 100) % 100;
        info.year = value / 10000;
    } else {
        info.have_time += 1;
        if value < 100 {
            info.hour = value;
            info.minutes = 0;
        } else {
            info.hour = value / 100;
            info.minutes = value % 100;
        }
        info.seconds = 0;
        info.meridian = Meridian::Mer24;
    }
}

// ── conversion (`parsedate.y:611-730`) ───────────────────────────────────

/// `Convert` (`:625-660`) in UTC: `mktime_z(NULL, &tm)` normalizes the
/// fields, and an explicit timezone then shifts by its minutes east of UTC.
#[allow(clippy::too_many_arguments)] // the fields are C's Convert parameters
fn convert(
    month: i64,
    day: i64,
    year: i64,
    hours: i64,
    minutes: i64,
    seconds: i64,
    timezone: i64,
    meridian: Meridian,
) -> i64 {
    let hour = hours + if meridian == Meridian::MerPm { 12 } else { 0 };
    // mktime normalizes a month outside 1..=12; keep that by moving whole
    // years and clamping only the month index.
    let year_adjust = (month - 1).div_euclid(12);
    let normalized_month = (month - 1).rem_euclid(12) + 1;
    let normalized_year = year + year_adjust;
    let days = days_from_civil(normalized_year, normalized_month, 1) + day - 1;
    let mut result = days * SECSPERDAY + hour * 3600 + minutes * 60 + seconds;
    if timezone != USE_LOCAL_TIME {
        result += timezone * 60;
    }
    result
}

/// `AdjustYear` (`:611-623`).
fn adjust_year(year: i64) -> i64 {
    let mut year = year;
    if year < 0 {
        year = -year;
    }
    if year < 70 {
        year += 2000;
    } else if year < 100 {
        year += 1900;
    }
    year
}

/// `RelativeMonth` (`:707-730`).
fn relative_month(start: i64, rel_month: i64, timezone: i64) -> i64 {
    if rel_month == 0 {
        return 0;
    }
    let fields = utc_fields(start);
    let month = 12 * fields.year + (fields.month - 1) + rel_month;
    let year = month / 12;
    let month = month % 12 + 1;
    let converted = convert(
        month,
        fields.day,
        year,
        fields.hour,
        fields.minute,
        fields.second,
        timezone,
        Meridian::Mer24,
    );
    // DSTcorrect under UTC: the wall-clock difference reduces to the elapsed
    // seconds between the two instants.
    converted - start
}

/// `RelativeDate` (`:699-705`).
fn relative_date(start: i64, ordinal: i64, day_number: i64) -> i64 {
    let fields = utc_fields(start);
    let mut now = start + SECSPERDAY * ((day_number - fields.weekday + 7) % 7);
    now += 7 * SECSPERDAY * (if ordinal <= 0 { ordinal } else { ordinal - 1 });
    now - start
}

/// Parse one free-form date expression, with `now` (seconds since the epoch)
/// supplying the initial year/month/day and, for a pure relative expression,
/// the starting instant. Returns `None` where C returns -1 with `EINVAL`.
pub fn parsedate(text: &str, now: i64) -> Option<i64> {
    let tokens = tokenize(text);
    let mut info = DateInfo::seeded(now);
    let mut pos = 0usize;
    while pos < tokens.len() {
        pos = parse_item(&tokens, pos, &mut info)?;
    }
    if info.have_time > 1 || info.have_zone > 1 || info.have_date > 1 || info.have_day > 1 {
        return None;
    }
    let mut start = if info.have_date != 0 || info.have_time != 0 || info.have_day != 0 {
        if !info.have_full_year {
            info.year = adjust_year(info.year);
        }
        convert(
            info.month,
            info.day,
            info.year,
            info.hour,
            info.minutes,
            info.seconds,
            info.timezone,
            info.meridian,
        )
    } else {
        let mut start = now;
        if info.have_rel == 0 {
            let fields = utc_fields(now);
            start -= ((fields.hour * 60 + fields.minute) * 60) + fields.second;
        }
        start
    };
    start += info.rel_seconds;
    start += relative_month(start, info.rel_month, info.timezone);
    if info.have_day != 0 && info.have_date == 0 {
        start += relative_date(start, info.day_ordinal, info.day_number);
    }
    Some(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2025-09-05T12:34:56Z, a Friday (weekday 5).
    const NOW: i64 = 1_757_075_696;

    fn at(text: &str) -> Option<i64> {
        parsedate(text, NOW)
    }

    fn utc(y: i64, m: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
        days_from_civil(y, m, d) * SECSPERDAY + h * 3600 + mi * 60 + s
    }

    #[test]
    fn iso_dates_and_times() {
        assert_eq!(at("2025-09-05"), Some(utc(2025, 9, 5, 0, 0, 0)));
        assert_eq!(at("2025-09-05 12:34:56"), Some(NOW));
        assert_eq!(at("2025/09/05"), Some(utc(2025, 9, 5, 0, 0, 0)));
        assert_eq!(at("09/05/2025"), Some(utc(2025, 9, 5, 0, 0, 0)));
        assert_eq!(at("2025-09-05 12:34"), Some(utc(2025, 9, 5, 12, 34, 0)));
    }

    #[test]
    fn month_names_and_weekdays() {
        assert_eq!(at("5 September 2025"), Some(utc(2025, 9, 5, 0, 0, 0)));
        // Without the comma the trailing year is a `number` item, and the
        // number rule reads it as 20:25 (`parsedate.y:332-357`).
        assert_eq!(at("Sep 5 2025"), Some(utc(2025, 9, 5, 20, 25, 0)));
        assert_eq!(at("September 5, 2025"), Some(utc(2025, 9, 5, 0, 0, 0)));
        assert_eq!(at("5-sep-2025"), Some(utc(2025, 9, 5, 0, 0, 0)));
        // A bare weekday means the next such day at midnight; 2025-09-05 is
        // a Friday, so `friday` is today and `last friday` a week back.
        assert_eq!(at("friday"), Some(utc(2025, 9, 5, 0, 0, 0)));
        assert_eq!(
            at("monday"),
            Some(utc(2025, 9, 8, 0, 0, 0)),
            "the next Monday at midnight"
        );
        assert_eq!(
            at("next monday"),
            Some(utc(2025, 9, 15, 0, 0, 0)),
            "`next` is the ordinal two"
        );
        assert_eq!(at("last friday"), Some(utc(2025, 8, 29, 0, 0, 0)));
    }

    #[test]
    fn am_pm_and_timezones() {
        // Convert never folds the 12-hour clock: AM adds nothing, PM adds
        // twelve, so `12:00 am` stays noon and `12:00 pm` is the next
        // midnight (`parsedate.y:629`).
        assert_eq!(at("12:00 am"), Some(utc(2025, 9, 5, 12, 0, 0)));
        assert_eq!(
            at("12:00 pm"),
            Some(utc(2025, 9, 6, 0, 0, 0)),
            "Convert adds twelve to twelve"
        );
        assert_eq!(at("1:30 pm"), Some(utc(2025, 9, 5, 13, 30, 0)));
        assert_eq!(at("2025-09-05 12:00 EST"), Some(utc(2025, 9, 5, 17, 0, 0)));
        assert_eq!(at("2025-09-05 12:00-0500"), Some(utc(2025, 9, 5, 17, 0, 0)));
        assert_eq!(at("2025-09-05 12:00 UTC"), Some(utc(2025, 9, 5, 12, 0, 0)));
    }

    #[test]
    fn relative_expressions() {
        // A relative word keeps the current time of day; only an expression
        // with no date, time, day or rel falls back to midnight.
        assert_eq!(at("now"), Some(NOW));
        assert_eq!(at("today"), Some(NOW));
        assert_eq!(at("tomorrow"), Some(NOW + SECSPERDAY));
        assert_eq!(at("2 days"), Some(NOW + 2 * SECSPERDAY));
        assert_eq!(at("2 days ago"), Some(NOW - 2 * SECSPERDAY));
        assert_eq!(
            at("2 days 3 hours ago"),
            Some(NOW - 2 * SECSPERDAY - 3 * 3600),
            "ago negates the whole accumulator"
        );
        assert_eq!(at("1 month"), Some(utc(2025, 10, 5, 12, 34, 56)));
        assert_eq!(
            at("next week"),
            Some(NOW + 14 * SECSPERDAY),
            "`next` is two of the unit"
        );
        assert_eq!(at("12:00"), Some(utc(2025, 9, 5, 12, 0, 0)));
        assert_eq!(at("2025-09-05"), Some(utc(2025, 9, 5, 0, 0, 0)));
    }

    #[test]
    fn epoch_and_numeric_forms() {
        assert_eq!(at("@0"), Some(0));
        assert_eq!(at("@1757075696"), Some(NOW));
        assert_eq!(at("20250905"), Some(utc(2025, 9, 5, 0, 0, 0)));
        // C accepts a numeric zone only glued to a time.
        assert_eq!(at("12:00:00-0500"), Some(utc(2025, 9, 5, 17, 0, 0)));
    }

    /// Vectors generated from the C grammar itself: `minix3/lib/libutil/
    /// parsedate.y` compiled with bison/flex on the host, called as
    /// `parsedate(text, &now, NULL)` with `now` fixed to `NOW`. They pin the
    /// table lookups, the numeric quirks and the relative arithmetic.
    #[test]
    fn c_oracle_vectors() {
        let cases: &[(&str, Option<i64>)] = &[
            ("2025-09-05", Some(1_757_030_400)),
            ("2025-09-05 12:34:56", Some(NOW)),
            ("2025/09/05", Some(1_757_030_400)),
            ("09/05/2025", Some(1_757_030_400)),
            ("2025-09-05 12:34", Some(1_757_075_640)),
            ("5 September 2025", Some(1_757_030_400)),
            ("Sep 5 2025", Some(1_757_103_900)),
            ("September 5, 2025", Some(1_757_030_400)),
            ("5-sep-2025", Some(1_757_030_400)),
            ("friday", Some(1_757_030_400)),
            ("monday", Some(1_757_289_600)),
            ("next monday", Some(1_757_894_400)),
            ("last friday", Some(1_756_425_600)),
            ("12:00 am", Some(1_757_073_600)),
            ("12:00 pm", Some(1_757_116_800)),
            ("1:30 pm", Some(1_757_079_000)),
            ("2025-09-05 12:00 EST", Some(1_757_091_600)),
            ("2025-09-05 12:00-0500", Some(1_757_091_600)),
            ("2025-09-05 12:00 UTC", Some(1_757_073_600)),
            ("now", Some(NOW)),
            ("today", Some(NOW)),
            ("tomorrow", Some(NOW + 86_400)),
            ("2 days", Some(NOW + 2 * 86_400)),
            ("2 days ago", Some(NOW - 2 * 86_400)),
            ("2 days 3 hours ago", Some(NOW - 2 * 86_400 - 3 * 3600)),
            ("1 month", Some(1_759_667_696)),
            ("next week", Some(NOW + 14 * 86_400)),
            ("@0", Some(0)),
            ("@1757075696", Some(NOW)),
            ("20250905", Some(1_757_030_400)),
            ("12:00:00-0500", Some(1_757_091_600)),
            ("2025-13-05", Some(1_767_571_200)),
            ("2025-13-05x", Some(1_767_531_600)),
            ("sep 5 2025", Some(1_757_103_900)),
            ("sept 5 2025", Some(1_757_103_900)),
            ("2025-09-05 12:00:00.500", Some(1_757_073_600)),
            ("2005-04-05 12:00 WAST", Some(1_112_677_200)),
            ("12:00 Z", Some(1_757_073_600)),
            ("zzz", None),
            ("sept. 5 2025", None),
            ("12:00 13:00", None),
            ("1/2/3/4", None),
        ];
        for &(text, expected) in cases {
            assert_eq!(at(text), expected, "{text:?}");
        }

        // A second batch covering weekdays, plural units, comments, military
        // zones, the unreachable `zp4` spelling and the `cvsstamp` form.
        let more: &[(&str, Option<i64>)] = &[
            ("tuesday", Some(1_757_376_000)),
            ("tues 12:00", Some(1_757_419_200)),
            ("12:00:00 pm", Some(1_757_116_800)),
            ("1990-01-01 12:00", Some(631_195_200)),
            ("@-1", Some(-1)),
            ("+500", None),
            ("-500", None),
            ("2005-04-05", Some(1_112_659_200)),
            ("5/6", Some(1_746_489_600)),
            ("5/6/07", Some(1_178_409_600)),
            ("5 JUN 1992", Some(707_702_400)),
            ("JUN 5", Some(1_749_081_600)),
            ("JUN 5, 1992", Some(707_702_400)),
            ("17-JUN-1992", Some(708_739_200)),
            ("tomorrow 12:00", Some(1_757_160_000)),
            ("this week", Some(NOW + 7 * 86_400)),
            ("3 months ago", Some(1_749_126_896)),
            ("1 year", Some(1_788_611_696)),
            ("2 fortnights", Some(1_759_494_896)),
            ("60 seconds", Some(NOW + 60)),
            ("2 minutes", Some(NOW + 120)),
            ("monday 12:00", Some(1_757_332_800)),
            ("1.2.3.4.5.6", Some(-2_174_586_894)),
            ("2025-09-05(comment) 12:00", Some(1_757_073_600)),
            ("12:00 (x) pm", Some(1_757_116_800)),
            ("2025-09-05 12:00 ZP4", None),
            ("2025-09-05 12:00 a", Some(1_757_077_200)),
            ("12:00 BST", Some(1_757_073_600)),
            ("995", Some(1_757_068_500)),
            ("1234", Some(1_757_075_640)),
            ("007", Some(1_757_055_600)),
            ("0/0/0", Some(943_920_000)),
            ("1 2", None),
            ("12:00:00.5", Some(1_757_073_600)),
        ];
        for &(text, expected) in more {
            assert_eq!(at(text), expected, "{text:?}");
        }
    }

    #[test]
    fn invalid_forms_are_rejected() {
        // The empty string is the grammar's null spec: it resolves to
        // midnight today, exactly like an all-zero relative expression.
        assert_eq!(at(""), Some(utc(2025, 9, 5, 0, 0, 0)));
        for text in ["zzz", "2025-13-05#", "12:00 13:00", "1/2/3/4"] {
            assert_eq!(at(text), None, "{text:?}");
        }
        // The values themselves are left to Convert; `2025-13-05x` would
        // even succeed, because `x` is a military timezone.
        assert_eq!(at("2025-13-05"), Some(utc(2026, 1, 5, 0, 0, 0)));
    }

    #[test]
    fn fields_match_the_c_lexer() {
        // Three-letter abbreviations match the first three letters in table
        // order; `sept` is its own entry; `sept.` is not a month.
        assert_eq!(at("sep 5 2025"), Some(utc(2025, 9, 5, 20, 25, 0)));
        assert_eq!(at("sept 5 2025"), Some(utc(2025, 9, 5, 20, 25, 0)));
        // `sept.` is neither the explicit `sept` entry nor a three-letter
        // abbreviation, and the dot-stripping retry only covers timezones.
        assert_eq!(at("sept. 5 2025"), None);
        // An unknown word is a syntax error.
        assert_eq!(at("2025-09-05 zzz"), None);
    }
}
