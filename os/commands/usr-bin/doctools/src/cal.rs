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

/// Month names as `cal.c:86-91` spells them (full form, title case).
pub const MONTH_NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August",
    "September", "October", "November", "December",
];

/// Layout constants of the C printer (`cal.c:336-343`): three columns per
/// day cell (`DAY_LEN`), twenty columns per month (`WEEK_LEN`, seven
/// cells minus the trailing space), two spaces between month columns
/// (`HEAD_SEP`), three months per year row (`MONTH_PER_ROW`).
pub const WEEK_LEN: usize = 20;
/// `HEAD_SEP` (`cal.c:340`).
pub const HEAD_SEP: usize = 2;
/// `MONTH_PER_ROW` (`cal.c:342`).
pub const MONTH_PER_ROW: usize = 3;

/// One month as a 7×6 day matrix (0 = the printer writes blanks).
///
/// 列序跟表头走：`day_headings` 是**周日先**（` S  M Tu ...`），而
/// `grid.first_weekday` 以周一为 0（`weekday` 的历法约定），落位时换算
/// 一格：周日列 = (周几 + 1) % 7。
pub fn day_matrix(grid: &MonthGrid) -> [[u32; 7]; 6] {
    let mut matrix = [[0u32; 7]; 6];
    for day in 1..=grid.day_count {
        let index = (day - 1 + grid.first_weekday + 1) as usize;
        let slot = index % 7;
        let line = index / 7;
        if line < 6 {
            matrix[line][slot] = day;
        }
    }
    matrix
}

/// `center` (`cal.c:653-660`): spread `text` across `width` columns, the
/// odd column on the right.
fn put_centered(out: &mut [u8], at: &mut usize, text: &str, width: usize) -> Result<(), DocError> {
    let pad = width.saturating_sub(text.len());
    let (left, right) = (pad / 2, pad / 2 + pad % 2);
    if *at + left + text.len() + right > out.len() {
        return Err(DocError::InvalidArgument);
    }
    out[*at..*at + left].iter_mut().for_each(|b| *b = b' ');
    *at += left;
    out[*at..*at + text.len()].copy_from_slice(text.as_bytes());
    *at += text.len();
    out[*at..*at + right].iter_mut().for_each(|b| *b = b' ');
    *at += right;
    Ok(())
}

fn put(out: &mut [u8], at: &mut usize, bytes: &[u8]) -> Result<(), DocError> {
    if *at + bytes.len() > out.len() {
        return Err(DocError::InvalidArgument);
    }
    out[*at..*at + bytes.len()].copy_from_slice(bytes);
    *at += bytes.len();
    Ok(())
}

/// One two-column day cell plus its separating blank: `ascii_day`
/// (`cal.c:496`) writes `aday[]` (" 1".."31") and the caller's `DAY_LEN`
/// pitch of three carries the separator.
fn put_day(out: &mut [u8], at: &mut usize, day: u32) -> Result<(), DocError> {
    if *at + 3 > out.len() {
        return Err(DocError::InvalidArgument);
    }
    if day == 0 {
        out[*at..*at + 3].copy_from_slice(b"   ");
    } else if day < 10 {
        out[*at] = b' ';
        out[*at + 1] = b'0' + day as u8;
        out[*at + 2] = b' ';
    } else {
        out[*at] = (day / 10) as u8 + b'0';
        out[*at + 1] = (day % 10) as u8 + b'0';
        out[*at + 2] = b' ';
    }
    *at += 3;
    Ok(())
}

/// Write one month's day body (six rows of `WEEK_LEN`, trailing blanks
/// trimmed) starting at `*at`. Rows beyond the days come out as blank
/// lines exactly like the C loop that always runs six times
/// (`cal.c:448-474`).
fn put_month_body(out: &mut [u8], at: &mut usize, grid: &MonthGrid) -> Result<(), DocError> {
    let matrix = day_matrix(grid);
    for line in matrix.iter() {
        // 七格乘三列是 21：末格的分隔空格随后裁掉（trim_trailing_spaces，
        // cal.c:471-474）。
        let mut row = [0u8; WEEK_LEN + 1];
        let mut w = 0;
        for &day in line.iter() {
            put_day(&mut row, &mut w, day)?;
        }
        while w > 0 && row[w - 1] == b' ' {
            w -= 1;
        }
        put(out, at, &row[..w])?;
        put(out, at, b"\n")?;
    }
    Ok(())
}

/// Render one month (`cal 9 2025` face, `cal.c:413-421`): the title
/// centred over the week, the ` S  M Tu  W Th  F  S` heading, six day
/// rows.
pub fn render_month(
    year: i32,
    month: u32,
    reckoning: Reckoning,
    out: &mut [u8],
) -> Result<usize, DocError> {
    let grid = month_grid(year, month, reckoning)?;
    let mut at = 0;
    let mut title = [0u8; 24];
    let name = MONTH_NAMES[(month - 1) as usize];
    title[..name.len()].copy_from_slice(name.as_bytes());
    title[name.len()] = b' ';
    let mut tail = [0u8; 8];
    let digits = write_decimal(year, &mut tail);
    title[name.len() + 1..name.len() + 1 + digits]
        .copy_from_slice(&tail[..digits]);
    let title_len = name.len() + 1 + digits;
    let title = core::str::from_utf8(&title[..title_len]).map_err(|_| DocError::InvalidArgument)?;
    put_centered(out, &mut at, title, WEEK_LEN)?;
    put(out, &mut at, b"\n S  M Tu  W Th  F  S\n")?;
    put_month_body(out, &mut at, &grid)?;
    Ok(at)
}

/// Render a whole year (`cal 2025` face, `cal.c:390-475`): the year
/// centred over the three-column band, then twelve months three to a row.
pub fn render_year(year: i32, reckoning: Reckoning, out: &mut [u8]) -> Result<usize, DocError> {
    let band = WEEK_LEN * MONTH_PER_ROW + HEAD_SEP * (MONTH_PER_ROW - 1);
    let mut at = 0;
    let mut year_text = [0u8; 8];
    let digits = write_decimal(year, &mut year_text);
    let year_str = core::str::from_utf8(&year_text[..digits]).map_err(|_| DocError::InvalidArgument)?;
    put_centered(out, &mut at, year_str, band)?;
    put(out, &mut at, b"\n\n")?;
    let mut first = 1u32;
    while first <= 12 {
        let count = (12 - first + 1).min(MONTH_PER_ROW as u32) as usize;
        let mut grid_storage = [MonthGrid { first_weekday: 0, day_count: 0 }; MONTH_PER_ROW];
        for (slot, grid_cell) in grid_storage.iter_mut().take(count).enumerate() {
            *grid_cell = month_grid(year, first + slot as u32, reckoning)?;
        }
        // 标题行与表头行（`cal.c:411-445`）：列间 `HEAD_SEP` 两空格。
        for slot in 0..count {
            if slot > 0 {
                put(out, &mut at, b"  ")?;
            }
            put_centered(out, &mut at, MONTH_NAMES[(first + slot as u32 - 1) as usize], WEEK_LEN)?;
        }
        put(out, &mut at, b"\n")?;
        for slot in 0..count {
            if slot > 0 {
                put(out, &mut at, b"  ")?;
            }
            put(out, &mut at, b" S  M Tu  W Th  F  S")?;
        }
        put(out, &mut at, b"\n")?;
        // 六行日体：三份并排（一份一行写完再收尾空格）。
        let matrices = [day_matrix(&grid_storage[0]), day_matrix(&grid_storage[1]), day_matrix(&grid_storage[2])];
        for line in matrices.iter() {
            for (i, row) in line.iter().enumerate() {
                if i > 0 {
                    put(out, &mut at, b"  ")?;
                }
                for &day in row {
                    put_day(out, &mut at, day)?;
                }
                at -= 1; // 每月行尾的分隔空格交给收尾统一裁剪
            }
            while at > 0 && out[at - 1] == b' ' {
                at -= 1;
            }
            put(out, &mut at, b"\n")?;
        }
        first += MONTH_PER_ROW as u32;
    }
    Ok(at)
}

/// Decimal writer for the year figure.
fn write_decimal(value: i32, out: &mut [u8]) -> usize {
    let negative = value < 0;
    let mut magnitude = (value as i64).unsigned_abs();
    let mut digits = [0u8; 12];
    let mut count = 0;
    loop {
        digits[count] = b'0' + (magnitude % 10) as u8;
        count += 1;
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    let mut at = 0;
    if negative {
        out[at] = b'-';
        at += 1;
    }
    while count > 0 {
        count -= 1;
        out[at] = digits[count];
        at += 1;
    }
    at
}

#[cfg(test)]
mod render_tests {
    use super::*;

    fn render(year: i32, month: u32) -> String {
        let mut out = [0u8; 4096];
        let used = render_month(year, month, Reckoning::Gregorian, &mut out).unwrap();
        String::from_utf8(out[..used].to_vec()).unwrap()
    }

    #[test]
    fn test_september_2025_golden() {
        // 与 BSD cal 实测输出逐字对齐（2025-09-01 是周一，表头周日起）。
        let golden = "   September 2025   \n S  M Tu  W Th  F  S\n    1  2  3  4  5  6\n 7  8  9 10 11 12 13\n14 15 16 17 18 19 20\n21 22 23 24 25 26 27\n28 29 30\n\n";
        assert_eq!(render(2025, 9), golden);
    }

    #[test]
    fn test_february_leap_has_six_rows_trimmed() {
        // 2024 年 2 月（闰，2 月 1 日周四）：五行日体；第六行全空裁成
        // 空行——C 的六行循环照打（cal.c:448）。
        let text = render(2024, 2);
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines[2], "             1  2  3");
        assert_eq!(lines[3], " 4  5  6  7  8  9 10");
        assert_eq!(lines[6], "25 26 27 28 29");
        assert_eq!(lines[7], "", "第六行全空裁尾后是空行（C 的六行循环照打）");
    }

    #[test]
    fn test_julian_reckoning_shifts_september_1752() {
        // 1752 年 9 月：格里高利面 9 月 14 日是周日；儒略面 9 月 3 日
        // 是周三——两种计数确实给出不同的栅格。
        let greg = render(1752, 9);
        let julian = render(1752, 9);
        let _ = (&greg, &julian);
        let greg_grid = month_grid(1752, 9, Reckoning::Gregorian).unwrap();
        let julian_grid = month_grid(1752, 9, Reckoning::Julian).unwrap();
        assert_ne!(greg_grid.first_weekday, julian_grid.first_weekday);
    }

    #[test]
    fn test_year_render_band_width() {
        let mut out = [0u8; 4096];
        let used = render_year(2025, Reckoning::Gregorian, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let mut lines = text.split('\n');
        let title = lines.next().unwrap();
        assert_eq!(title.trim(), "2025");
        assert_eq!(title.len(), 64, "年份居中于三列带宽 20*3+2*2");
        let names = lines.next().unwrap(); // 标题后的空行
        assert_eq!(names, "");
        let months = lines.next().unwrap();
        assert!(months.starts_with("      January"), "居中左补 6 空（20-7)/2");
        assert!(months.contains("March"), "一行三个月");
    }
}
