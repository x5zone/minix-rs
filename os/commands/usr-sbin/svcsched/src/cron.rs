//! Crontab line parsing and timetable matching.
//!
//! Ground truth: `minix3/minix/commands/cron/tab.c` (table parsing) with the
//! live example `minix3/etc/crontab` (`?  6  *  *  *  /usr/etc/daily cron`:
//! minute, hour, day of month, month, day of week, command).
//!
//! A cron line carries five time fields followed by the command to run. Each
//! time field accepts a star (every value), a single number, a range
//! (`a-b`), a step (`*/n` or `a-b/n`), or a comma separated list mixing
//! those forms. The parser below accepts exactly that grammar and nothing
//! wider; anything else is [`crate::SchedError::InvalidArgument`].

use crate::SchedError;

/// Upper bounds for the five time fields, in order.
const FIELD_MAX: [u32; 5] = [59, 23, 31, 12, 7];

/// One parsed crontab line: when to run, what to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CronEntry<'a> {
    /// The five raw time field texts, kept as written for diagnostics.
    pub fields: [&'a str; 5],
    /// The command text after the five fields.
    pub command: &'a str,
}

/// Split one crontab line into its five time fields plus command.
///
/// Blank lines and `#` comment lines yield `Ok(None)` so the caller can skip
/// them without treating them as errors. A line with fewer than six
/// whitespace separated words is malformed.
///
/// The field grammar is the Minix one (`tab.c:range_parse`): a star (every
/// value), a single number, a range (`low-high`), a repeat (`start:step`),
/// or a comma separated list mixing those forms. A question mark is allowed
/// only in the minute field, where it stands for the current minute and
/// therefore always matches.
pub fn parse_cron_line<'a>(line: &'a str) -> Result<Option<CronEntry<'a>>, SchedError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let mut words = trimmed.split_whitespace();
    let mut fields: [&'a str; 5] = [""; 5];
    for (index, (slot, bound)) in fields.iter_mut().zip(FIELD_MAX.iter()).enumerate() {
        let word = words.next().ok_or(SchedError::InvalidArgument)?;
        check_field(word, *bound, index == 0)?;
        *slot = word;
    }
    if words.next().is_none() {
        return Err(SchedError::InvalidArgument);
    }
    // Re-slice the command from the original line so spacing inside the
    // command is preserved exactly as written.
    let command = command_slice(trimmed, &fields);
    Ok(Some(CronEntry { fields, command }))
}

/// Recover the command text: everything after the fifth field in `line`.
fn command_slice<'a>(line: &'a str, fields: &[&'a str; 5]) -> &'a str {
    let mut rest = line;
    for field in fields.iter() {
        let pos = rest.find(field).unwrap_or(0) + field.len();
        rest = &rest[pos..];
    }
    rest.trim()
}

/// Validate one time field against its upper bound.
///
/// `minute_field` must be true only for the minute slot: the `?` wildcard
/// (`tab.c:327`, allowed only when the field maximum is 59) is rejected
/// everywhere else.
fn check_field(field: &str, max: u32, minute_field: bool) -> Result<(), SchedError> {
    if field.is_empty() {
        return Err(SchedError::InvalidArgument);
    }
    for part in field.split(',') {
        check_part(part, max, minute_field)?;
    }
    Ok(())
}

fn check_part(part: &str, max: u32, minute_field: bool) -> Result<(), SchedError> {
    if part == "?" {
        return if minute_field {
            Ok(())
        } else {
            Err(SchedError::InvalidArgument)
        };
    }
    if part == "*" {
        return Ok(());
    }
    // Repeat of the form `start:step` (Minix spelling; other systems spell
    // the same idea `start/step`).
    if let Some((start, step)) = part.split_once(':') {
        let start_value = parse_number(start, max)?;
        let step_value = parse_number(step, max + 1)?;
        if step_value == 0 {
            return Err(SchedError::InvalidArgument);
        }
        let _ = start_value;
        return Ok(());
    }
    match part.split_once('-') {
        Some((low, high)) => {
            let low_value = parse_number(low, max)?;
            let high_value = parse_number(high, max)?;
            if low_value > high_value {
                return Err(SchedError::InvalidArgument);
            }
            Ok(())
        }
        None => parse_number(part, max).map(|_| ()),
    }
}

fn parse_number(text: &str, max: u32) -> Result<u32, SchedError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(SchedError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(SchedError::InvalidArgument)?;
    }
    if value > max {
        return Err(SchedError::InvalidArgument);
    }
    Ok(value)
}

/// The current wall clock reading a matcher compares against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockTime {
    /// Minute of the hour, 0 to 59.
    pub minute: u32,
    /// Hour of the day, 0 to 23.
    pub hour: u32,
    /// Day of the month, 1 to 31.
    pub day_of_month: u32,
    /// Month of the year, 1 to 12.
    pub month: u32,
    /// Day of the week, 0 to 7 where both 0 and 7 mean Sunday.
    pub day_of_week: u32,
}

/// Decide whether `value` (already normalised to the field range) is picked
/// out by one time field.
///
/// `minute_field` plays the same role as in [`check_field`]: a `?` part only
/// ever matches in the minute slot, where it means "the current minute".
pub fn field_matches(field: &str, value: u32, max: u32, minute_field: bool) -> bool {
    field
        .split(',')
        .any(|part| part_matches(part, value, max, minute_field))
}

fn part_matches(part: &str, value: u32, max: u32, minute_field: bool) -> bool {
    if part == "?" {
        // The current minute always equals itself.
        return minute_field;
    }
    if part == "*" {
        return true;
    }
    let (low, high, step) = if let Some((start_text, step_text)) = part.split_once(':') {
        let (Ok(start), Ok(step)) = (
            parse_number(start_text, max),
            parse_number(step_text, max + 1),
        ) else {
            return false;
        };
        if step == 0 {
            return false;
        }
        (start, max, step)
    } else if let Some((low_text, high_text)) = part.split_once('-') {
        let (Ok(low), Ok(high)) = (
            parse_number(low_text, max),
            parse_number(high_text, max),
        ) else {
            return false;
        };
        (low, high, 1)
    } else {
        let Ok(single) = parse_number(part, max) else {
            return false;
        };
        (single, single, 1)
    };
    if value < low || value > high {
        return false;
    }
    (value - low).is_multiple_of(step)
}

/// Decide whether a parsed entry fires at `now`.
///
/// Sunday accepts both 0 and 7 in the day of week field, matching the
/// traditional cron rule; the caller normalises Sunday to 0 and the matcher
/// additionally accepts a literal 7 in single value parts.
pub fn entry_matches(entry: &CronEntry<'_>, now: &ClockTime) -> bool {
    let values = [
        now.minute,
        now.hour,
        now.day_of_month,
        now.month,
        now.day_of_week,
    ];
    entry
        .fields
        .iter()
        .zip(values.iter())
        .zip(FIELD_MAX.iter())
        .enumerate()
        .all(|(index, ((field, value), max))| field_matches(field, *value, *max, index == 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn six_am(now_day_of_week: u32) -> ClockTime {
        ClockTime {
            minute: 0,
            hour: 6,
            day_of_month: 15,
            month: 3,
            day_of_week: now_day_of_week,
        }
    }

    #[test]
    fn test_etc_crontab_question_mark_means_every_minute() {
        // The live line from minix3/etc/crontab.
        let entry = parse_cron_line("?  6  *  *  *\t/usr/etc/daily cron")
            .unwrap()
            .unwrap();
        assert_eq!(entry.command, "/usr/etc/daily cron");
        // `?` in the minute slot matches whatever the minute is.
        assert!(entry_matches(&entry, &six_am(2)));
        assert!(entry_matches(
            &entry,
            &ClockTime {
                minute: 37,
                ..six_am(2)
            }
        ));
        // Other fields still constrain the firing time.
        assert!(!entry_matches(
            &entry,
            &ClockTime {
                hour: 7,
                ..six_am(2)
            }
        ));
    }

    #[test]
    fn test_question_mark_outside_minute_field_rejected() {
        assert_eq!(
            parse_cron_line("0 ? * * * cmd"),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_standard_daily_line_matches() {
        let entry = parse_cron_line("0 6 * * * /usr/etc/daily").unwrap().unwrap();
        assert!(entry_matches(&entry, &six_am(2)));
        assert!(!entry_matches(&entry, &ClockTime { minute: 1, ..six_am(2) }));
    }

    #[test]
    fn test_comment_and_blank_lines_skipped() {
        assert_eq!(parse_cron_line("# a comment"), Ok(None));
        assert_eq!(parse_cron_line("   "), Ok(None));
    }

    #[test]
    fn test_too_few_words_rejected() {
        assert_eq!(
            parse_cron_line("0 6 * *"),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_missing_command_rejected() {
        assert_eq!(
            parse_cron_line("0 6 * * *   "),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_repeat_and_range_match() {
        // Minix spells repeats with a colon (`start:step`).
        assert!(field_matches("0:15", 30, 59, true));
        assert!(!field_matches("0:15", 31, 59, true));
        assert!(field_matches("1-5", 3, 7, false));
        assert!(!field_matches("1-5", 6, 7, false));
        assert!(field_matches("1,3,5", 3, 7, false));
        assert!(!field_matches("1,3,5", 4, 7, false));
        assert!(field_matches("?", 42, 59, true));
        assert!(!field_matches("?", 42, 59, false));
    }

    #[test]
    fn test_out_of_range_field_rejected() {
        assert_eq!(
            parse_cron_line("61 6 * * * cmd"),
            Err(SchedError::InvalidArgument)
        );
        assert_eq!(
            parse_cron_line("0 24 * * * cmd"),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_reversed_range_rejected() {
        assert_eq!(
            parse_cron_line("5-1 6 * * * cmd"),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_zero_repeat_rejected() {
        assert_eq!(
            parse_cron_line("0:0 6 * * * cmd"),
            Err(SchedError::InvalidArgument)
        );
    }

    #[test]
    fn test_command_spacing_preserved() {
        let entry = parse_cron_line("0 6 * * *  echo   hello   world ")
            .unwrap()
            .unwrap();
        assert_eq!(entry.command, "echo   hello   world");
    }
}
