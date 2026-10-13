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

/// Layout constants of the C printer (`cal.c:336-343`): three columns per
/// day cell (`DAY_LEN`), twenty columns per month (`WEEK_LEN`, seven
/// cells minus the trailing space), two spaces between month columns
/// (`HEAD_SEP`), three months per year row (`MONTH_PER_ROW`).
pub const WEEK_LEN: usize = 20;
/// `HEAD_SEP` (`cal.c:340`).
pub const HEAD_SEP: usize = 2;
/// `MONTH_PER_ROW` (`cal.c:342`).
pub const MONTH_PER_ROW: usize = 3;

/// Which numeral each printed day cell carries, and how wide the cell and the
/// year band are. BSD `cal -j` switches the day from day-of-month to
/// day-of-year and widens every cell from two digits to three: `cal.c:255`
/// sets `julian`, `cal.c:336-343` pick `J_DAY_LEN`/`J_WEEK_LEN`/
/// `J_MONTH_PER_ROW`, and `ascii_day` (`cal.c:584-603`) renders a three-digit
/// day with leading zeros dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayStyle {
    /// Day of month (`1`..`31`): two-column cells, three months per year row.
    Month,
    /// Day of year (`1`..`366`): three-column cells, two months per year row.
    Julian,
}

/// `J_WEEK_LEN` (`cal.c:339`): seven four-column cells minus the trailing space.
pub const J_WEEK_LEN: usize = 27;
/// `J_HEAD_SEP` (`cal.c:341`).
pub const J_HEAD_SEP: usize = 2;
/// `J_MONTH_PER_ROW` (`cal.c:343`).
pub const J_MONTH_PER_ROW: usize = 2;
/// `day_headings` (`cal.c:91`).
pub const DAY_HEADINGS: &str = " S  M Tu  W Th  F  S";
/// `j_day_headings` (`cal.c:92`).
pub const JULIAN_HEADINGS: &str = "  S   M  Tu   W  Th   F   S";

/// 1-based day of the year for the first of `month` (Gregorian leap rule),
/// mirroring `cal.c` `day_in_year`: `cal -j` prints this as the month's first
/// cell and counts up, so September 2025 opens at 244.
pub fn day_of_year_of_first(year: i32, month: u32) -> Result<u32, DocError> {
    if year <= 0 || !(1..=12).contains(&month) {
        return Err(DocError::InvalidArgument);
    }
    let mut doy = 1u32;
    for m in 1..month {
        doy += month_days(year, m, Reckoning::Gregorian)?;
    }
    Ok(doy)
}

/// One month as a 7×6 day matrix (0 = the printer writes blanks). The printed
/// numeral is `first_value + (day - 1)`: the month display passes
/// `first_value = 1` (day-of-month), `cal -j` passes the day-of-year of the
/// month's first.
///
/// 列序跟表头走：`day_headings` 是**周日先**（` S  M Tu ...`），而
/// `grid.first_weekday` 以周一为 0（`weekday` 的历法约定），落位时换算
/// 一格：周日列 = (周几 + 1) % 7。
pub fn day_matrix(grid: &MonthGrid, first_value: u32) -> [[u32; 7]; 6] {
    let mut matrix = [[0u32; 7]; 6];
    for day in 1..=grid.day_count {
        let index = (day - 1 + grid.first_weekday + 1) as usize;
        let slot = index % 7;
        let line = index / 7;
        if line < 6 {
            matrix[line][slot] = first_value + (day - 1);
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

/// One three-column Julian day-of-year cell plus its separating blank:
/// `ascii_day` in `julian` mode (`cal.c:584-603`) drops leading zero digits
/// (so `1` prints `  1`, `105` prints `105`), and the caller's `J_DAY_LEN`
/// pitch of four carries the separator.
fn put_day_julian(out: &mut [u8], at: &mut usize, day: u32) -> Result<(), DocError> {
    if *at + 4 > out.len() {
        return Err(DocError::InvalidArgument);
    }
    if day == 0 {
        out[*at..*at + 4].copy_from_slice(b"    ");
    } else {
        let (hundreds, rest) = (day / 100, day % 100);
        out[*at] = if hundreds != 0 {
            b'0' + hundreds as u8
        } else {
            b' '
        };
        out[*at + 1] = if hundreds != 0 || rest / 10 != 0 {
            b'0' + (rest / 10) as u8
        } else {
            b' '
        };
        out[*at + 2] = b'0' + (rest % 10) as u8;
        out[*at + 3] = b' ';
    }
    *at += 4;
    Ok(())
}

/// Write one month's day body (six rows) from a prebuilt `matrix`. `day_len`
/// is 3 for the month display and 4 for `cal -j`; trailing blanks are trimmed
/// exactly like the C loop that always runs six times (`cal.c:448-474`).
fn put_month_body(
    out: &mut [u8],
    at: &mut usize,
    matrix: &[[u32; 7]; 6],
    day_len: usize,
) -> Result<(), DocError> {
    for line in matrix.iter() {
        // 七格乘 day_len（周最多 28 列）：末格的分隔空格随后裁掉
        // （trim_trailing_spaces，cal.c:471-474）。
        let mut row = [0u8; 7 * 4];
        let mut w = 0;
        for &day in line.iter() {
            if day_len == 4 {
                put_day_julian(&mut row, &mut w, day)?;
            } else {
                put_day(&mut row, &mut w, day)?;
            }
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
    style: DayStyle,
    out: &mut [u8],
) -> Result<usize, DocError> {
    let grid = month_grid(year, month, reckoning)?;
    let (week_len, day_len, headings, first_value) = match style {
        DayStyle::Month => (WEEK_LEN, 3, DAY_HEADINGS, 1u32),
        DayStyle::Julian => (
            J_WEEK_LEN,
            4,
            JULIAN_HEADINGS,
            day_of_year_of_first(year, month)?,
        ),
    };
    let mut at = 0;
    let mut title = [0u8; 24];
    let name = MONTH_NAMES[(month - 1) as usize];
    title[..name.len()].copy_from_slice(name.as_bytes());
    title[name.len()] = b' ';
    let mut tail = [0u8; 8];
    let digits = write_decimal(year, &mut tail);
    title[name.len() + 1..name.len() + 1 + digits].copy_from_slice(&tail[..digits]);
    let title_len = name.len() + 1 + digits;
    let title = core::str::from_utf8(&title[..title_len]).map_err(|_| DocError::InvalidArgument)?;
    put_centered(out, &mut at, title, week_len)?;
    put(out, &mut at, b"\n")?;
    put(out, &mut at, headings.as_bytes())?;
    put(out, &mut at, b"\n")?;
    let matrix = day_matrix(&grid, first_value);
    put_month_body(out, &mut at, &matrix, day_len)?;
    Ok(at)
}

/// Render a whole year (`cal 2025` face, `cal.c:390-475`): the year
/// centred over the three-column band, then twelve months three to a row.
pub fn render_year(
    year: i32,
    reckoning: Reckoning,
    style: DayStyle,
    out: &mut [u8],
) -> Result<usize, DocError> {
    let (week_len, day_len, month_per_row, headings) = match style {
        DayStyle::Month => (WEEK_LEN, 3, MONTH_PER_ROW, DAY_HEADINGS),
        DayStyle::Julian => (J_WEEK_LEN, 4, J_MONTH_PER_ROW, JULIAN_HEADINGS),
    };
    let band = week_len * month_per_row + HEAD_SEP * (month_per_row - 1);
    let mut at = 0;
    let mut year_text = [0u8; 8];
    let digits = write_decimal(year, &mut year_text);
    let year_str =
        core::str::from_utf8(&year_text[..digits]).map_err(|_| DocError::InvalidArgument)?;
    put_centered(out, &mut at, year_str, band)?;
    put(out, &mut at, b"\n\n")?;
    let mut first = 1u32;
    while first <= 12 {
        let count = (12 - first + 1).min(month_per_row as u32) as usize;
        let mut grid_storage = [MonthGrid {
            first_weekday: 0,
            day_count: 0,
        }; 3];
        let mut values_storage = [1u32; 3];
        for slot in 0..count {
            let month = first + slot as u32;
            grid_storage[slot] = month_grid(year, month, reckoning)?;
            values_storage[slot] = match style {
                DayStyle::Month => 1,
                DayStyle::Julian => day_of_year_of_first(year, month)?,
            };
        }
        // 标题行与表头行（`cal.c:411-445`）：列间 `HEAD_SEP` 两空格。
        for slot in 0..count {
            if slot > 0 {
                put(out, &mut at, b"  ")?;
            }
            put_centered(
                out,
                &mut at,
                MONTH_NAMES[(first + slot as u32 - 1) as usize],
                week_len,
            )?;
        }
        put(out, &mut at, b"\n")?;
        for slot in 0..count {
            if slot > 0 {
                put(out, &mut at, b"  ")?;
            }
            put(out, &mut at, headings.as_bytes())?;
        }
        put(out, &mut at, b"\n")?;
        let matrices = [
            day_matrix(&grid_storage[0], values_storage[0]),
            day_matrix(&grid_storage[1], values_storage[1]),
            day_matrix(&grid_storage[2], values_storage[2]),
        ];
        // 六行日体：每周 outer、每月 inner。外层 `week` 要同切三份月栅格的
        // 同一行（按周取带），是固定列跨多数组的访问，迭代器形反而更绕，
        // clippy 的 needless_range_loop 在此属误报——局部 allow + 理由。
        #[allow(clippy::needless_range_loop)]
        for week in 0..6 {
            for (slot, matrix) in matrices.iter().enumerate().take(count) {
                if slot > 0 {
                    put(out, &mut at, b"  ")?;
                }
                for &day in &matrix[week] {
                    if day_len == 4 {
                        put_day_julian(out, &mut at, day)?;
                    } else {
                        put_day(out, &mut at, day)?;
                    }
                }
                at -= 1; // 每月行尾的分隔空格交给收尾统一裁剪
            }
            while at > 0 && out[at - 1] == b' ' {
                at -= 1;
            }
            put(out, &mut at, b"\n")?;
        }
        first += month_per_row as u32;
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
        let used =
            render_month(year, month, Reckoning::Gregorian, DayStyle::Month, &mut out).unwrap();
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
        let used = render_year(2025, Reckoning::Gregorian, DayStyle::Month, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let mut lines = text.split('\n');
        let title = lines.next().unwrap();
        assert_eq!(title.trim(), "2025");
        assert_eq!(title.len(), 64, "年份居中于三列带宽 20*3+2*2");
        let names = lines.next().unwrap(); // 标题后的空行
        assert_eq!(names, "");
        let months = lines.next().unwrap();
        assert!(
            months.starts_with("      January"),
            "居中左补 6 空（20-7)/2"
        );
        assert!(months.contains("March"), "一行三个月");
    }

    #[test]
    fn test_year_body_is_side_by_side() {
        // 回归铉（P1-bug）：render_year 的日体必须每周一行、同带三个月并排。
        // 旧体把两层循环写反（外层=月、内层=周），一月的 6 行被横拼成一条
        // 线、二月三月另起——test_year_render_band_width 只校标题带没抓到。
        let mut out = [0u8; 4096];
        let used = render_year(2025, Reckoning::Gregorian, DayStyle::Month, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let lines: Vec<&str> = text.split('\n').collect();
        // 全年 4 带 ×（1 月名 + 1 表头 + 6 日行）+ 顶部年份/空行 + 末尾空串。
        assert_eq!(lines.len(), 4 * (1 + 1 + 6) + 2 + 1, "年视图总行数");
        // 首带首行日：2025-01-01 是周三（S/M/Tu 空、W=1），三列各占 WEEK_LEN。
        assert_eq!(
            lines[4],
            "          1  2  3  4                     1                     1"
        );
        // 次行：Jan 5–11 | Feb 2–8 | Mar 2–8 并排。
        assert_eq!(
            lines[5],
            " 5  6  7  8  9 10 11   2  3  4  5  6  7  8   2  3  4  5  6  7  8"
        );
    }

    #[test]
    fn test_julian_month_day_of_year() {
        // cal -j 9 2025：September 1 是年内第 244 日（Gregorian 2025 非闰），
        // 落周一（表头周日起、M 列），单元宽三列加一空格分隔（BSD cal -j）。
        let mut out = [0u8; 4096];
        let used = render_month(2025, 9, Reckoning::Gregorian, DayStyle::Julian, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(
            lines[0], "      September 2025       ",
            "标题居中于 J_WEEK_LEN=27"
        );
        assert_eq!(lines[1], "  S   M  Tu   W  Th   F   S", "j_day_headings");
        assert_eq!(
            lines[2], "    244 245 246 247 248 249",
            "首行日 = 年内第 244–249"
        );
        assert_eq!(day_of_year_of_first(2025, 9).unwrap(), 244);
        assert_eq!(
            day_of_year_of_first(2024, 3).unwrap(),
            61,
            "闰年 3/1 = 31+29+1"
        );
        assert_eq!(day_of_year_of_first(2025, 1).unwrap(), 1);
    }

    #[test]
    fn test_julian_year_two_per_row() {
        // cal -j 2024：年视图每月 2 个一行（J_MONTH_PER_ROW），Feb 1 = 年内第 32 日。
        let mut out = [0u8; 8192];
        let used = render_year(2024, Reckoning::Gregorian, DayStyle::Julian, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let lines: Vec<&str> = text.split('\n').collect();
        // 6 带（12 月 / 2）× (1 名 + 1 表 + 6 日) + 顶 2 + 末空。
        assert_eq!(lines.len(), 6 * (1 + 1 + 6) + 2 + 1, "年积日视图 6 带行数");
        assert_eq!(
            lines[2], "          January                     February          ",
            "首带 = Jan + Feb 两月并排"
        );
        assert!(
            lines[4].starts_with("      1   2   3   4   5   6"),
            "Jan 1 起年内第 1 日"
        );
        assert!(lines[4].contains("32"), "Feb 1 落年内第 32 日（2024 闰）");
    }
}
