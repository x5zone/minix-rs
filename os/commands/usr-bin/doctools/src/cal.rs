//! Month grid computation behind `cal`.
//!
//! Ground truth: `minix3/usr.bin/cal/cal.c` (924 lines). The hard part of a
//! calendar program is not printing boxes but knowing which weekday a date
//! falls on across the Julian/Gregorian reformation, where a block of days
//! never existed. C parameterises that block as a `reform` record — the first
//! missing day and how many days vanish (`cal.c:65-66`, `:155-197`) — and
//! installs the British September 1752 switch as the default even when the
//! user names no reform (`cal.c:282-283`). This module carries the same
//! record, the same table, the same `-R` parser and the same weekday/missing
//! day arithmetic.
//!
//! - [`Reform`] is one switch: the first missing date plus C's derived
//!   `first_missing_day`/`missing_days` anchors.
//! - Leap years follow the switch: Julian before the reform year, Gregorian
//!   after it (`cal.c:105-108`).
//! - `-R` accepts a full date (`y/m/d`, separators `/`, `,` or `-`), a
//!   partial date matched against the table, or a country name
//!   (case-insensitive), with C's ambiguity rule.
//! - A month that meets the gap is laid out by the same walk as C's
//!   `reform_day_array` (`cal.c:829-865`): only existing days advance the
//!   column, and the four months after the switch are precomputed that way.
//!
//! Reference anchors: 1752-09-02 was a Wednesday and 1752-09-14 a Thursday
//! (Britain dropped 3 through 13); 1582-10-04 was a Thursday and
//! 1582-10-15 a Friday (the papal switch).

use crate::DocError;

/// Days in each month of a common year (February gains one in leap years).
const MONTH_DAYS: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// One Julian-to-Gregorian switch (`cal.c:155-190`): the first day that never
/// existed, how many days vanish there, and the two day numbers the weekday
/// arithmetic anchors on. The default is Britain's September 1752 reform,
/// which C installs even without `-r`/`-R` (`cal.c:282-283`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reform {
    /// Year of the switch.
    pub year: i32,
    /// Month of the first missing day (1 to 12).
    pub month: u32,
    /// Day of the first missing day.
    pub date: u32,
    /// C's `first_missing_day` (`cal.c:783-786`).
    pub first_missing_day: i64,
    /// How many days vanish at the switch (`cal.c:776-779`).
    pub missing_days: i32,
}

/// C's `reforms[]` (`cal.c:165-197`): country name, ambiguity bound, and the
/// first missing date. The `{ NULL }` terminator is not a member.
const REFORM_TABLE: [(&str, i32, i32, u32, u32); 29] = [
    ("DEFAULT", 0, 1752, 9, 3),
    ("Italy", 1, 1582, 10, 5),
    ("Spain", 1, 1582, 10, 5),
    ("Portugal", 1, 1582, 10, 5),
    ("Poland", 1, 1582, 10, 5),
    ("France", 2, 1582, 12, 10),
    ("Luxembourg", 2, 1582, 12, 22),
    ("Netherlands", 2, 1582, 12, 22),
    ("Bavaria", 0, 1583, 10, 6),
    ("Austria", 2, 1584, 1, 7),
    ("Switzerland", 2, 1584, 1, 12),
    ("Hungary", 0, 1587, 10, 22),
    ("Germany", 0, 1700, 2, 19),
    ("Norway", 0, 1700, 2, 19),
    ("Denmark", 0, 1700, 2, 19),
    ("Great Britain", 0, 1752, 9, 3),
    ("England", 0, 1752, 9, 3),
    ("America", 0, 1752, 9, 3),
    ("Sweden", 0, 1753, 2, 18),
    ("Finland", 0, 1753, 2, 18),
    ("Japan", 0, 1872, 12, 20),
    ("China", 0, 1911, 11, 7),
    ("Bulgaria", 0, 1916, 4, 1),
    ("U.S.S.R.", 0, 1918, 2, 1),
    ("Serbia", 0, 1919, 1, 19),
    ("Romania", 0, 1919, 1, 19),
    ("Greece", 0, 1924, 3, 10),
    ("Turkey", 0, 1925, 12, 19),
    ("Egypt", 0, 1928, 9, 18),
];

/// Why a `-R` argument was rejected. Every variant renders one of C's
/// `errx` sentences (`cal.c:733-802`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReformError {
    /// `date specifier %s invalid`.
    DateSpecifier,
    /// `%d: illegal year value: use 1-9999`.
    IllegalYear(i32),
    /// `%d: illegal month value: use 1-12`.
    IllegalMonth(i32),
    /// `%d: illegal date value: use 1-%d`.
    IllegalDate {
        /// The date the user gave.
        date: i32,
        /// The last valid day of that month under the short-year table.
        max: u32,
    },
    /// `%s: ambiguous short reform date specification`.
    Ambiguous,
    /// `reform name %s invalid`.
    InvalidName,
}

/// Julian leap day in `month`/`date` (`j_leap_year`, `cal.c:94-96`).
fn j_leap_day(year: i32, month: u32) -> i64 {
    if month > 2 && year % 4 == 0 { 1 } else { 0 }
}

/// Gregorian leap day in `month`/`date` (`g_leap_year`, `cal.c:105-108`).
fn g_leap_day(year: i32, month: u32) -> i64 {
    if month > 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
        1
    } else {
        0
    }
}

/// `j_leap_days` (`cal.c:112-114`).
fn j_leap_days(year: i32, month: u32) -> i64 {
    (year as i64 - 1) / 4 + j_leap_day(year, month)
}

/// `g_leap_days` (`cal.c:117-120`).
fn g_leap_days(year: i32, month: u32) -> i64 {
    (year as i64 - 1) / 4 - (year as i64 - 1) / 100
        + (year as i64 - 1) / 400
        + g_leap_day(year, month)
}

/// Length of `month` under a leap/common February (`days_in_month`,
/// `cal.c:70-73`).
fn month_length(leap: bool, month: u32) -> u32 {
    if month == 2 && leap {
        29
    } else {
        MONTH_DAYS[(month - 1) as usize]
    }
}

/// Move `(year, month)` by `delta` months.
fn add_months(year: i32, month: u32, delta: i64) -> (i32, u32) {
    let index = year as i64 * 12 + (month as i64 - 1) + delta;
    ((index / 12) as i32, (index % 12) as u32 + 1)
}

/// Scan one C `%d` integer starting at `pos`, returning the value and the new
/// position. Leading whitespace and a sign are accepted, as `strtol`/`%d`
/// accept them.
fn scan_int(bytes: &[u8], pos: &mut usize) -> Option<i64> {
    let mut i = *pos;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let sign = match bytes.get(i) {
        Some(b'-') => {
            i += 1;
            -1
        }
        Some(b'+') => {
            i += 1;
            1
        }
        _ => 1,
    };
    let start = i;
    let mut value: i64 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(bytes[i] - b'0'));
        i += 1;
    }
    if i == start {
        return None;
    }
    *pos = i;
    Some(sign * value)
}

/// True when `bytes[pos]` is one of C's `sscanf` separators `[/,-]`.
fn is_separator(byte: u8) -> bool {
    matches!(byte, b'/' | b',' | b'-')
}

impl Reform {
    /// Britain's September 1752 switch, C's `DEFAULT` entry.
    pub fn standard() -> Self {
        Self::at(1752, 9, 3)
    }

    /// Build a switch from the first missing date and derive C's two
    /// anchors (`cal.c:772-786`).
    pub fn at(year: i32, month: u32, date: u32) -> Self {
        let mut reform = Self {
            year,
            month,
            date,
            first_missing_day: 0,
            missing_days: 0,
        };
        reform.missing_days = (j_leap_days(year, month) - g_leap_days(year, month) - 2) as i32;
        reform.first_missing_day = (year as i64 - 1) * 365
            + reform.day_in_year(date as i64, month, year)
            + date as i64
            + j_leap_days(year, month);
        reform
    }

    /// Parse a `-R` argument (`gregorian_reform`, `cal.c:727-802`): a full
    /// date, a partial date matched against the table, or a country name.
    pub fn from_spec(spec: &str) -> Result<Self, ReformError> {
        let bytes = spec.as_bytes();
        let mut pos = 0usize;
        let first = scan_int(bytes, &mut pos);
        let (conversions, year, month, date) = match first {
            None => (0i32, -1i32, -1i32, -1i32),
            Some(year) => {
                let mut month = 0i32;
                let mut date = 0i32;
                let mut conversions = 1i32;
                if pos < bytes.len() && is_separator(bytes[pos]) {
                    let mut after = pos + 1;
                    if let Some(value) = scan_int(bytes, &mut after) {
                        month = value as i32;
                        conversions = 2;
                        pos = after;
                        if pos < bytes.len() && is_separator(bytes[pos]) {
                            let mut after = pos + 1;
                            if let Some(value) = scan_int(bytes, &mut after) {
                                date = value as i32;
                                conversions = 3;
                                pos = after;
                                if pos < bytes.len() {
                                    conversions = 4;
                                }
                            }
                        }
                    }
                }
                (conversions, year as i32, month, date)
            }
        };
        if conversions == 4 {
            return Err(ReformError::DateSpecifier);
        }
        if conversions >= 1 {
            if !(1..=9999).contains(&year) {
                return Err(ReformError::IllegalYear(year));
            }
            if conversions > 1 && !(1..=12).contains(&month) {
                return Err(ReformError::IllegalMonth(month));
            }
            let table_month = if (1..=12).contains(&month) {
                month as usize
            } else {
                0
            };
            let max = month_length(
                true,
                if table_month == 0 {
                    1
                } else {
                    table_month as u32
                },
            );
            let max = if table_month == 0 { 0 } else { max };
            if (conversions == 3 && date < 1) || date < 0 || date as u32 > max {
                return Err(ReformError::IllegalDate { date, max });
            }
            if date > 0 {
                return Ok(Self::at(year, month as u32, date as u32));
            }
        }
        if conversions == 0 {
            let wanted = spec.to_ascii_lowercase();
            return REFORM_TABLE
                .iter()
                .find(|entry| entry.0.to_ascii_lowercase() == wanted)
                .map(|entry| Self::at(entry.2, entry.3, entry.4))
                .ok_or(ReformError::InvalidName);
        }
        for entry in REFORM_TABLE {
            if (year == 0 || year == entry.2)
                && (month == 0 || month == entry.3 as i32)
                && (date == 0 || month == entry.4 as i32)
            {
                if conversions <= entry.1 {
                    return Err(ReformError::Ambiguous);
                }
                return Ok(Self::at(entry.2, entry.3, entry.4));
            }
        }
        Err(ReformError::InvalidName)
    }

    /// Leap rule in force for `year`: Julian up to the reform year,
    /// Gregorian after it (`cal.c:105-108`).
    pub fn leap_year(&self, year: i32) -> bool {
        if year <= self.year {
            year % 4 == 0
        } else {
            year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
        }
    }

    /// Days in `month` of `year` (`days_in_month[leap_year(year)]`).
    pub fn month_days(&self, year: i32, month: u32) -> Result<u32, DocError> {
        if year <= 0 || !(1..=12).contains(&month) {
            return Err(DocError::InvalidArgument);
        }
        Ok(month_length(self.leap_year(year), month))
    }

    /// `day_in_year` (`cal.c:566-575`): the 1-based day of the year.
    fn day_in_year(&self, day: i64, month: u32, year: i32) -> i64 {
        let leap = self.leap_year(year);
        let mut result = day;
        for m in 1..month {
            result += month_length(leap, m) as i64;
        }
        result
    }

    /// `leap_years_since_year_1` (`cal.c:152-153`, with the reform-bounded
    /// century terms at `:143-149`).
    fn leap_years_since_year_1(&self, year: i64) -> i64 {
        let centuries = if year > self.year as i64 {
            year / 100 - (self.year as i64 / 100)
        } else {
            0
        };
        let quad_centuries = if year > self.year as i64 {
            year / 400 - (self.year as i64 / 400)
        } else {
            0
        };
        year / 4 - centuries + quad_centuries
    }

    /// `day_in_week` (`cal.c:549-559`): 0 is Sunday through 6 Saturday, with
    /// `week_start` (`-d`) rotating the columns. A date inside the missing
    /// block answers the anchor column C returns.
    pub fn weekday(
        &self,
        day: i64,
        month: u32,
        year: i32,
        week_start: usize,
    ) -> Result<u32, DocError> {
        if year <= 0 || !(1..=12).contains(&month) || day < 1 {
            return Err(DocError::InvalidArgument);
        }
        if day > self.month_days(year, month)? as i64 {
            return Err(DocError::InvalidArgument);
        }
        let temp = (year as i64 - 1) * 365
            + self.leap_years_since_year_1(year as i64 - 1)
            + self.day_in_year(day, month, year);
        let dow = week_start as i64;
        let base = temp - dow + 6 + 6; // 1 Jan 1 was a Saturday (`cal.c:63`)
        let column = if temp < self.first_missing_day {
            base.rem_euclid(7)
        } else if temp >= self.first_missing_day + self.missing_days as i64 {
            (base - self.missing_days as i64).rem_euclid(7)
        } else {
            (self.first_missing_day - dow + 6 + 6).rem_euclid(7)
        };
        Ok(column as u32)
    }

    /// `day_in_year(1, month, year)` — the numeral `cal -j` puts on the first
    /// of the month.
    pub fn day_of_year_of_first(&self, year: i32, month: u32) -> Result<u32, DocError> {
        if year <= 0 || !(1..=12).contains(&month) {
            return Err(DocError::InvalidArgument);
        }
        Ok(self.day_in_year(1, month, year) as u32)
    }

    /// Build the printed cells of one month. A month inside the four-month
    /// reform window follows C's `reform_day_array` walk; every other month
    /// starts day one at its weekday column and counts straight through.
    pub fn month_matrix(
        &self,
        year: i32,
        month: u32,
        style: DayStyle,
        week_start: usize,
    ) -> Result<[[u32; 7]; 6], DocError> {
        if year <= 0 || year > 9999 || !(1..=12).contains(&month) {
            return Err(DocError::InvalidArgument);
        }
        for index in 0..4usize {
            if add_months(self.year, self.month, index as i64) == (year, month) {
                return Ok(self.window_month(index, style, week_start));
            }
        }
        let first_column = self.weekday(1, month, year, week_start)? as usize;
        let count = self.month_days(year, month)?;
        let first_value = match style {
            DayStyle::Month => 1,
            DayStyle::Julian => self.day_of_year_of_first(year, month)?,
        };
        Ok(fill_matrix(first_column, count, first_value))
    }

    /// The walk of `reform_day_array` (`cal.c:829-865`) over the four months
    /// starting at the reform month; only existing days advance a column.
    /// C uses the Gregorian leap rule for these months' lengths (`:848-851`).
    fn window_month(&self, target: usize, style: DayStyle, week_start: usize) -> [[u32; 7]; 6] {
        let mut date = 1i64;
        let mut done = 0i64;
        let mut column = self
            .weekday(self.date as i64, self.month, self.year, week_start)
            .unwrap_or(0) as i64;
        // Back up from the anchor to the first of the reform month:
        // `diw = (diw + 8 - (date % 7)) % 7` (`cal.c:789-790`).
        column = (column + 8 - (self.date as i64 % 7)).rem_euclid(7);
        let mut day_of_year = self.day_in_year(1, self.month, self.year);
        let mut result = [[0u32; 7]; 6];
        for index in 0..=target {
            let (year, month) = add_months(self.year, self.month, index as i64);
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            let length = month_length(leap, month);
            let mut flat = [0u32; 42];
            let mut slot = column.rem_euclid(7) as usize;
            while date - done <= length as i64 {
                if date < self.date as i64 || date >= self.date as i64 + self.missing_days as i64 {
                    if slot < 42 {
                        flat[slot] = match style {
                            DayStyle::Month => (date - done) as u32,
                            DayStyle::Julian => day_of_year as u32,
                        };
                    }
                    slot += 1;
                }
                date += 1;
                day_of_year += 1;
            }
            done += length as i64;
            column = slot as i64;
            if index == target {
                result = matrix_from_flat(flat);
            }
        }
        result
    }
}

/// Place `count` consecutive numerals from `first_column` in the flat 42-slot
/// week array and cut it into rows of seven.
fn fill_matrix(first_column: usize, count: u32, first_value: u32) -> [[u32; 7]; 6] {
    let mut flat = [0u32; 42];
    for offset in 0..count as usize {
        let slot = first_column + offset;
        if slot < 42 {
            flat[slot] = first_value + offset as u32;
        }
    }
    matrix_from_flat(flat)
}

fn matrix_from_flat(flat: [u32; 42]) -> [[u32; 7]; 6] {
    let mut matrix = [[0u32; 7]; 6];
    for (index, value) in flat.into_iter().enumerate() {
        matrix[index / 7][index % 7] = value;
    }
    matrix
}

/// Proleptic Gregorian civil-to-days: days since 1970-01-01 (Hinnant's
/// algorithm). This is the plain Gregorian encoder, independent of the
/// reform table — `proctools`'s `stamp` tests round-trip their own
/// `civil_from_days` against it, so the two implementations check each
/// other across crates.
pub fn days_from_civil(year: i32, month: u32, day: u32) -> Result<i64, DocError> {
    if year <= 0 || !(1..=12).contains(&month) || day == 0 {
        return Err(DocError::InvalidArgument);
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let mut month_days = MONTH_DAYS[(month - 1) as usize];
    if month == 2 && leap {
        month_days += 1;
    }
    if day > month_days {
        return Err(DocError::InvalidArgument);
    }
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = if shifted_year >= 0 {
        shifted_year
    } else {
        shifted_year - 399
    } / 400;
    let year_of_era = (shifted_year - era * 400) as i64;
    let month_index = (month as i64 + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day as i64 - 1;
    let day_of_era =
        year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + year_of_era / 400 + day_of_year;
    Ok(era as i64 * 146097 + day_of_era - 719468)
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

/// Write the weekday heading row, rotated by `week_start` when nonzero.
/// Mirrors `cal.c:435-442`: when `dow` is set, print `base[day_len*dow..]`,
/// a space, then `base[..day_len*dow-1]` — a cyclic column shift. When `dow`
/// is zero, print `base` verbatim (the C `else` branch), so the default path
/// stays byte-identical.
fn put_headings(
    out: &mut [u8],
    at: &mut usize,
    base: &str,
    day_len: usize,
    week_start: usize,
) -> Result<(), DocError> {
    let b = base.as_bytes();
    let cut = day_len * (week_start % 7);
    if cut == 0 || cut >= b.len() {
        return put(out, at, b);
    }
    put(out, at, &b[cut..])?;
    put(out, at, b" ")?;
    put(out, at, &b[..cut - 1])
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
        // Seven cells times day_len (a week is at most 28 columns): the
        // separator space of the last cell is trimmed afterwards
        // (trim_trailing_spaces, cal.c:471-474).
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

/// Render one month (`cal [-d n] 9 2025` face, `cal.c:413-421`): the title
/// centred over the week, the weekday heading (rotated by `week_start`), six
/// day rows.
pub fn render_month(
    year: i32,
    month: u32,
    reform: &Reform,
    style: DayStyle,
    week_start: usize,
    out: &mut [u8],
) -> Result<usize, DocError> {
    let (week_len, day_len, headings) = match style {
        DayStyle::Month => (WEEK_LEN, 3, DAY_HEADINGS),
        DayStyle::Julian => (J_WEEK_LEN, 4, JULIAN_HEADINGS),
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
    put_headings(out, &mut at, headings, day_len, week_start)?;
    put(out, &mut at, b"\n")?;
    let matrix = reform.month_matrix(year, month, style, week_start)?;
    put_month_body(out, &mut at, &matrix, day_len)?;
    Ok(at)
}

/// Render a whole year (`cal 2025` face, `cal.c:390-475`): the year
/// centred over the three-column band, then twelve months three to a row.
pub fn render_year(
    year: i32,
    reform: &Reform,
    style: DayStyle,
    week_start: usize,
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
        let mut matrices = [[[0u32; 7]; 6]; 3];
        for (slot, matrix) in matrices.iter_mut().enumerate().take(count) {
            *matrix = reform.month_matrix(year, first + slot as u32, style, week_start)?;
        }
        // Title row and heading row (`cal.c:411-445`): `HEAD_SEP` two spaces
        // between columns.
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
            put_headings(out, &mut at, headings, day_len, week_start)?;
        }
        put(out, &mut at, b"\n")?;
        // Six day-body rows: outer over weeks, inner over months. The outer
        // `week` must cut the same row out of all three month grids (taking a
        // band by week), which is a fixed-column access across several arrays,
        // so the iterator form is actually more convoluted; clippy's
        // needless_range_loop is a false positive here — local allow plus the
        // reason.
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
                at -= 1; // the separator space at each month's line end is left to the final trim
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

/// Shift `(year, month)` by `delta` months (delta = -1 or +1), normalizing
/// across the year boundary. `month` is 1..=12; the result year must stay in
/// `1..=9999` (`cal.c:381` rejects out-of-range start/end years).
/// Render a month range — C's `monthrange` (`cal.c:346-...`) for the
/// `-A`/`-B`/`-C` context flags. Two layouts: within one year the months
/// print in rows of up to three with `Name Year` titles (the `render_triple`
/// shape); a range that crosses a year boundary switches to the year-banner
/// layout (the `render_year` shape) with month-name-only titles. The first
/// row is aligned down to a three-column boundary, leaving the skipped
/// columns blank (`cal.c:386-390`). Rows never straddle a year because rows
/// start at month 0 or 3, 6, 9 and a year has twelve months.
pub fn render_range(
    start: (i32, u32),
    end: (i32, u32),
    reform: &Reform,
    style: DayStyle,
    week_start: usize,
    out: &mut [u8],
) -> Result<usize, DocError> {
    if !(1..=9999).contains(&start.0) || !(1..=9999).contains(&end.0) {
        return Err(DocError::InvalidArgument);
    }
    if !(1..=12).contains(&start.1) || !(1..=12).contains(&end.1) {
        return Err(DocError::InvalidArgument);
    }
    let first_index = start.0 as i64 * 12 + (start.1 as i64 - 1);
    let last_index = end.0 as i64 * 12 + (end.1 as i64 - 1);
    if last_index < first_index {
        return Err(DocError::InvalidArgument);
    }
    let (week_len, day_len, month_per_row, headings) = match style {
        DayStyle::Month => (WEEK_LEN, 3, MONTH_PER_ROW, DAY_HEADINGS),
        DayStyle::Julian => (J_WEEK_LEN, 4, J_MONTH_PER_ROW, JULIAN_HEADINGS),
    };
    let inayear = start.0 != end.0;
    let band = week_len * month_per_row + HEAD_SEP * (month_per_row - 1);
    let mut at = 0;
    // C aligns the first row only in the year-banner mode (`cal.c:386-390`:
    // `if (inayear) { skip = month % month_per_row; month -= skip; }`); a
    // same-year range starts in column one with the start month.
    let mut row_base = if inayear {
        first_index - first_index.rem_euclid(month_per_row as i64)
    } else {
        first_index
    };
    let mut first_row = true;
    let mut banner_year: Option<i32> = None;
    while row_base <= last_index {
        let row_year = (row_base / 12) as i32;
        if inayear && banner_year != Some(row_year) {
            let mut year_text = [0u8; 8];
            let digits = write_decimal(row_year, &mut year_text);
            let year_str = core::str::from_utf8(&year_text[..digits])
                .map_err(|_| DocError::InvalidArgument)?;
            put_centered(out, &mut at, year_str, band)?;
            put(out, &mut at, b"\n\n")?;
            banner_year = Some(row_year);
        }
        let skip = if inayear && first_row {
            (first_index.rem_euclid(month_per_row as i64)) as usize
        } else {
            0
        };
        // Titles: `Name Year` within one year, the bare name under a banner.
        let mut titles: [(i32, u32); 3] = [(row_year, 1), (row_year, 1), (row_year, 1)];
        let mut count = 0usize;
        for slot in skip..month_per_row {
            let index = row_base + slot as i64;
            if index > last_index {
                break;
            }
            if index >= first_index {
                titles[count] = (((index / 12) as i32), (index % 12) as u32 + 1);
                count += 1;
            }
        }
        // C renders the first row's skipped columns as blank titles.
        let mut slot = 0usize;
        for column in 0..month_per_row {
            if column > 0 {
                put(out, &mut at, b"  ")?;
            }
            if column < skip || column - skip >= count {
                for _ in 0..week_len {
                    put(out, &mut at, b" ")?;
                }
            } else {
                let (y, m) = titles[slot];
                slot += 1;
                if inayear {
                    put_centered(out, &mut at, MONTH_NAMES[(m - 1) as usize], week_len)?;
                } else {
                    put_month_year_header(out, &mut at, y, m, week_len)?;
                }
            }
        }
        put(out, &mut at, b"\n")?;
        // Day-heading row.
        for column in 0..month_per_row {
            if column > 0 {
                put(out, &mut at, b"  ")?;
            }
            if column < skip || column - skip >= count {
                for _ in 0..week_len {
                    put(out, &mut at, b" ")?;
                }
            } else {
                put_headings(out, &mut at, headings, day_len, week_start)?;
            }
        }
        put(out, &mut at, b"\n")?;
        // Six week rows across the present months.
        let mut matrices: [[[u32; 7]; 6]; 3] = [[[0; 7]; 6]; 3];
        for slot in 0..count {
            let (y, m) = titles[slot];
            matrices[slot] = reform.month_matrix(y, m, style, week_start)?;
        }
        #[allow(clippy::needless_range_loop)]
        for week in 0..6 {
            for column in 0..month_per_row {
                if column > 0 {
                    put(out, &mut at, b"  ")?;
                }
                if column < skip || column - skip >= count {
                    for _ in 0..week_len {
                        put(out, &mut at, b" ")?;
                    }
                    continue;
                }
                let slot = column - skip;
                for &day in &matrices[slot][week] {
                    put_day(out, &mut at, day)?;
                }
                // The triple window drops each month's trailing cell space
                // (`render_triple`'s `at -= 1`) so the column separator is
                // the two-space join, not three.
                at -= 1;
            }
            // Trim the row's trailing blanks (C's `trim_trailing_spaces`).
            while at > 0 && out[at - 1] == b' ' {
                at -= 1;
            }
            put(out, &mut at, b"\n")?;
        }
        row_base += month_per_row as i64;
        first_row = false;
    }
    Ok(at)
}

fn shift_month(year: i32, month: u32, delta: i32) -> Result<(i32, u32), DocError> {
    let idx = year * 12 + (month as i32 - 1) + delta;
    let y = idx.div_euclid(12);
    let m = idx.rem_euclid(12) as u32 + 1;
    if !(1..=9999).contains(&y) {
        return Err(DocError::InvalidArgument);
    }
    Ok((y, m))
}

/// Center a `"MonthName Year"` label over one month column, mirroring `cal.c`
/// `snprintf("%s %d", month_names, year)` + `center` on the non-yearly band.
fn put_month_year_header(
    out: &mut [u8],
    at: &mut usize,
    year: i32,
    month: u32,
    width: usize,
) -> Result<(), DocError> {
    let name = MONTH_NAMES[(month - 1) as usize];
    let mut buf = [0u8; 24];
    let nlen = name.len();
    buf[..nlen].copy_from_slice(name.as_bytes());
    buf[nlen] = b' ';
    let mut tail = [0u8; 8];
    let digits = write_decimal(year, &mut tail);
    buf[nlen + 1..nlen + 1 + digits].copy_from_slice(&tail[..digits]);
    let label =
        core::str::from_utf8(&buf[..nlen + 1 + digits]).map_err(|_| DocError::InvalidArgument)?;
    put_centered(out, at, label, width)
}

/// Render three consecutive months centered side by side (`cal -3 m y`
/// face, `cal.c:monthrange` with before=after=1). Because the band is not a
/// full year, each header is `"MonthName Year"` (the C `inayear` branch is
/// false), so a year boundary simply changes the outer month's own label.
pub fn render_triple(
    year: i32,
    month: u32,
    reform: &Reform,
    style: DayStyle,
    week_start: usize,
    out: &mut [u8],
) -> Result<usize, DocError> {
    if year <= 0 || !(1..=12).contains(&month) {
        return Err(DocError::InvalidArgument);
    }
    let (week_len, day_len, headings) = match style {
        DayStyle::Month => (WEEK_LEN, 3, DAY_HEADINGS),
        DayStyle::Julian => (J_WEEK_LEN, 4, JULIAN_HEADINGS),
    };
    let months = [
        shift_month(year, month, -1)?,
        (year, month),
        shift_month(year, month, 1)?,
    ];
    let mut matrices = [[[0u32; 7]; 6]; 3];
    for slot in 0..3 {
        let (y, m) = months[slot];
        matrices[slot] = reform.month_matrix(y, m, style, week_start)?;
    }
    let mut at = 0;
    // Title row: each month's "Name Year" centered in week_len, two spaces
    // between columns.
    for (i, (y, m)) in months.iter().enumerate() {
        if i > 0 {
            put(out, &mut at, b"  ")?;
        }
        put_month_year_header(out, &mut at, *y, *m, week_len)?;
    }
    put(out, &mut at, b"\n")?;
    for i in 0..3 {
        if i > 0 {
            put(out, &mut at, b"  ")?;
        }
        put_headings(out, &mut at, headings, day_len, week_start)?;
    }
    put(out, &mut at, b"\n")?;
    // Six day-body rows: outer over weeks, inner over months (fixed columns
    // across three arrays, allow as in render_year).
    #[allow(clippy::needless_range_loop)]
    for week in 0..6 {
        for (slot, matrix) in matrices.iter().enumerate() {
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
            at -= 1;
        }
        while at > 0 && out[at - 1] == b' ' {
            at -= 1;
        }
        put(out, &mut at, b"\n")?;
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
        let used = render_month(
            year,
            month,
            &Reform::standard(),
            DayStyle::Month,
            0,
            &mut out,
        )
        .unwrap();
        String::from_utf8(out[..used].to_vec()).unwrap()
    }

    /// The restored proleptic encoder: the epoch and the leap day anchor
    /// the values `proctools`'s round-trip test relies on.
    #[test]
    fn days_from_civil_anchors_hold() {
        assert_eq!(days_from_civil(1970, 1, 1), Ok(0));
        assert_eq!(days_from_civil(2000, 2, 29), Ok(11016));
        assert_eq!(days_from_civil(1969, 12, 31), Ok(-1));
        assert_eq!(days_from_civil(2024, 2, 30), Err(DocError::InvalidArgument));
    }

    #[test]
    fn test_september_2025_golden() {
        // Verbatim alignment with measured BSD cal output (2025-09-01 is a
        // Monday, headings start on Sunday).
        let golden = "   September 2025   \n S  M Tu  W Th  F  S\n    1  2  3  4  5  6\n 7  8  9 10 11 12 13\n14 15 16 17 18 19 20\n21 22 23 24 25 26 27\n28 29 30\n\n";
        assert_eq!(render(2025, 9), golden);
    }

    #[test]
    fn test_february_leap_has_six_rows_trimmed() {
        // February 2024 (leap, Feb 1 is a Thursday): five day-body rows; the
        // sixth all-empty row is trimmed to an empty line — C still prints its
        // six-row loop (cal.c:448).
        let text = render(2024, 2);
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines[2], "             1  2  3");
        assert_eq!(lines[3], " 4  5  6  7  8  9 10");
        assert_eq!(lines[6], "25 26 27 28 29");
        assert_eq!(lines[7], "", "第六行全空裁尾后是空行（C 的六行循环照打）");
    }

    /// Britain's default reform: 3 through 13 September 1752 never existed.
    /// The 2nd was a Wednesday and the 14th a Thursday (`cal.c:165`), so the
    /// grid jumps a column across the seam.
    #[test]
    fn the_default_reform_skips_september_1752() {
        let reform = Reform::standard();
        let matrix = reform.month_matrix(1752, 9, DayStyle::Month, 0).unwrap();
        let days: Vec<u32> = matrix
            .iter()
            .flatten()
            .copied()
            .filter(|&day| day != 0)
            .collect();
        assert_eq!(
            days,
            (1..=30u32)
                .filter(|day| !(3..=13).contains(day))
                .collect::<Vec<_>>()
        );
        // Sunday-first columns: the 1st (Tuesday) sits in column 2, the 14th
        // (Thursday) in column 4 of its week row.
        assert_eq!(matrix[0][2], 1);
        let (row, column) = matrix
            .iter()
            .enumerate()
            .find_map(|(row, cells)| {
                cells
                    .iter()
                    .position(|&day| day == 14)
                    .map(|column| (row, column))
            })
            .expect("the 14th is printed");
        assert_eq!((row, column), (0, 4), "14 September 1752 was a Thursday");
        assert_eq!(reform.missing_days, 11);
    }

    /// The table is a selector: `-R Italy` moves the gap to October 1582,
    /// where the 4th was a Thursday and the 15th a Friday.
    #[test]
    fn a_named_reform_moves_the_gap() {
        let italy = Reform::from_spec("Italy").unwrap();
        let matrix = italy.month_matrix(1582, 10, DayStyle::Month, 0).unwrap();
        let days: Vec<u32> = matrix
            .iter()
            .flatten()
            .copied()
            .filter(|&day| day != 0)
            .collect();
        assert_eq!(
            days,
            (1..=31u32)
                .filter(|day| !(5..=14).contains(day))
                .collect::<Vec<_>>()
        );
        let (row, column) = matrix
            .iter()
            .enumerate()
            .find_map(|(row, cells)| {
                cells
                    .iter()
                    .position(|&day| day == 15)
                    .map(|column| (row, column))
            })
            .expect("the 15th is printed");
        assert_eq!((row, column), (0, 5), "15 October 1582 was a Friday");
        // The default reform leaves October 1582 untouched.
        assert_eq!(Reform::standard().month_days(1582, 10).unwrap(), 31);
    }

    #[test]
    fn test_year_render_band_width() {
        let mut out = [0u8; 4096];
        let used = render_year(2025, &Reform::standard(), DayStyle::Month, 0, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let mut lines = text.split('\n');
        let title = lines.next().unwrap();
        assert_eq!(title.trim(), "2025");
        assert_eq!(title.len(), 64, "年份居中于三列带宽 20*3+2*2");
        let names = lines.next().unwrap(); // the blank line after the title
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
        // Regression guard (a previous bug): render_year's day body must be one
        // line per week with three months side by side on the same band. The old
        // body had the two loops swapped (outer = month, inner = week), so
        // January's six rows were spliced horizontally into one line and
        // February/March started separately — test_year_render_band_width only
        // checked the title band and missed it.
        let mut out = [0u8; 4096];
        let used = render_year(2025, &Reform::standard(), DayStyle::Month, 0, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let lines: Vec<&str> = text.split('\n').collect();
        // Four bands a year × (1 month name + 1 heading + 6 day rows) + the top
        // year line/blank line + the trailing empty string.
        assert_eq!(lines.len(), 4 * (1 + 1 + 6) + 2 + 1, "年视图总行数");
        // First row of the first band: 2025-01-01 is a Wednesday (S/M/Tu empty,
        // W=1), each of the three columns taking WEEK_LEN.
        assert_eq!(
            lines[4],
            "          1  2  3  4                     1                     1"
        );
        // Next row: Jan 5–11 | Feb 2–8 | Mar 2–8 side by side.
        assert_eq!(
            lines[5],
            " 5  6  7  8  9 10 11   2  3  4  5  6  7  8   2  3  4  5  6  7  8"
        );
    }

    #[test]
    fn test_julian_month_day_of_year() {
        // cal -j 9 2025: September 1 is day 244 of the year (Gregorian 2025 is
        // not a leap year), landing on a Monday (headings start Sunday, M
        // column), cell width three columns plus a one-space separator (BSD
        // cal -j).
        let mut out = [0u8; 4096];
        let used =
            render_month(2025, 9, &Reform::standard(), DayStyle::Julian, 0, &mut out).unwrap();
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
        assert_eq!(
            Reform::standard().day_of_year_of_first(2025, 9).unwrap(),
            244
        );
        assert_eq!(
            Reform::standard().day_of_year_of_first(2024, 3).unwrap(),
            61,
            "闰年 3/1 = 31+29+1"
        );
        assert_eq!(Reform::standard().day_of_year_of_first(2025, 1).unwrap(), 1);
    }

    #[test]
    fn test_julian_year_two_per_row() {
        // cal -j 2024: the year view puts 2 months per line (J_MONTH_PER_ROW),
        // Feb 1 = day 32 of the year.
        let mut out = [0u8; 8192];
        let used = render_year(2024, &Reform::standard(), DayStyle::Julian, 0, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let lines: Vec<&str> = text.split('\n').collect();
        // Six bands (12 months / 2) × (1 name + 1 heading + 6 day rows) + the top
        // two lines + the trailing empty one.
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

    #[test]
    fn test_shift_month_across_year_boundary() {
        assert_eq!(shift_month(2025, 1, -1).unwrap(), (2024, 12));
        assert_eq!(shift_month(2025, 12, 1).unwrap(), (2026, 1));
        assert_eq!(shift_month(2025, 9, -1).unwrap(), (2025, 8));
        assert_eq!(shift_month(2025, 9, 1).unwrap(), (2025, 10));
        // The month before year 1 falls in year 0 → rejected (matching C's
        // startyear<0 check).
        assert!(shift_month(1, 1, -1).is_err());
    }

    #[test]
    fn test_triple_centers_three_months_with_year_labels() {
        // cal -3 1 2025: the year-boundary triple = Dec 2024 / Jan 2025 /
        // Feb 2025, each labelled "Name Year".
        let mut out = [0u8; 8192];
        let used =
            render_triple(2025, 1, &Reform::standard(), DayStyle::Month, 0, &mut out).unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(
            lines[0], "   December 2024          January 2025         February 2025    ",
            "三标题各居 week_len、列间两空格"
        );
        // Title + heading + 6 day-body rows + the trailing empty string.
        assert_eq!(lines.len(), 1 + 1 + 6 + 1, "三联总行数");
    }

    #[test]
    fn test_week_start_rotation() {
        // cal -d: rotate the whole week of columns forward, with the first day's
        // landing column in sync with the headings.
        let reform = Reform::standard();
        // week_start=0: Sep 1 (Monday) lands in the M column of the Sunday-first
        // headings (cell 1), and the Sunday column is empty.
        let m0 = reform.month_matrix(2025, 9, DayStyle::Month, 0).unwrap();
        assert_eq!(m0[0][1], 1, "dow=0：Sep 1 在 M 列");
        assert_eq!(m0[0][0], 0, "dow=0：周日列空");
        // week_start=1 (cal -d 1, Monday first): Sep 1 moves to the first column,
        // and Sunday the 7th lands in the last column.
        let m1 = reform.month_matrix(2025, 9, DayStyle::Month, 1).unwrap();
        assert_eq!(m1[0][0], 1, "cal -d 1：Sep 1（周一）落首列");
        assert_eq!(m1[0][6], 7, "cal -d 1：周日 7 落末列");
        // Heading rotation: dow=1 makes day_headings start at the M column and
        // Sunday fall back to the end (matching cal.c:435-439).
        let mut buf = [0u8; 32];
        let mut at = 0;
        put_headings(&mut buf, &mut at, DAY_HEADINGS, 3, 1).unwrap();
        assert_eq!(
            core::str::from_utf8(&buf[..at]).unwrap(),
            " M Tu  W Th  F  S  S",
            "周一起的表头循环移位（C day_headings+3 首位带一空格）"
        );
        // dow=0 is verbatim the base headings (the default path has zero
        // regressions).
        let mut b0 = [0u8; 32];
        let mut a0 = 0;
        put_headings(&mut b0, &mut a0, DAY_HEADINGS, 3, 0).unwrap();
        assert_eq!(&b0[..a0], DAY_HEADINGS.as_bytes());
    }

    /// A same-year range renders exactly like the triple window over the same
    /// months — the two paths share C's row layout only if this holds.
    #[test]
    fn a_three_month_range_equals_the_triple_window() {
        let mut a = [0u8; 4096];
        let mut b = [0u8; 4096];
        let range = render_range(
            (2024, 2),
            (2024, 4),
            &Reform::standard(),
            DayStyle::Month,
            0,
            &mut a,
        )
        .unwrap();
        let triple =
            render_triple(2024, 3, &Reform::standard(), DayStyle::Month, 0, &mut b).unwrap();
        assert_eq!(&a[..range], &b[..triple]);
    }

    /// A range that crosses a year boundary switches to the year-banner
    /// layout: both years appear as banner lines, titles carry no year.
    #[test]
    fn a_cross_year_range_prints_year_banners() {
        let mut out = [0u8; 8192];
        let used = render_range(
            (2024, 12),
            (2025, 1),
            &Reform::standard(),
            DayStyle::Month,
            0,
            &mut out,
        )
        .unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        assert!(text.contains("2024") && text.contains("2025"), "{text}");
        assert!(
            !text.contains("December 2024"),
            "banner mode drops the year from titles"
        );
    }

    /// A mid-row start leaves the skipped columns blank: the February range
    /// begins at column 2 of a row that would hold Jan-Feb-Mar.
    #[test]
    fn a_mid_row_start_leaves_blank_columns() {
        // Year-banner mode aligns the first row to a three-column boundary:
        // December 2024 starts two columns in (its absolute month index mod
        // three is two), so the banner row is followed by two blank columns
        // before the December title.
        let mut out = [0u8; 8192];
        let used = render_range(
            (2024, 12),
            (2025, 2),
            &Reform::standard(),
            DayStyle::Month,
            0,
            &mut out,
        )
        .unwrap();
        let text = core::str::from_utf8(&out[..used]).unwrap();
        let title_row = text.lines().nth(2).unwrap();
        // Two blank columns (20 spaces + two-space join each) plus the
        // six-space centring pad put the title text at column 50.
        assert!(
            title_row.starts_with(&" ".repeat(50)) && title_row[50..].starts_with("December"),
            "December starts in column three: {title_row:?}"
        );
    }
}
