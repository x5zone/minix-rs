//! Field list parsing and column selection behind `cut`.
//!
//! Ground truth: `minix3/usr.bin/cut/cut.c` (option string `b:c:d:f:sn` at
//! line 88). Three selection modes share one list grammar (comma separated
//! numbers and `low-high` ranges, 1 based):
//!
//! - `-b`: byte positions.
//! - `-c`: character positions (identical to bytes here; multibyte
//!   awareness is a documented follow-up, matching the engine's byte
//!   orientation).
//! - `-f`: delimiter separated fields with a custom delimiter (`-d`),
//!   suppressing lines without the delimiter unless `-s` is off (default
//!   prints them whole).

use crate::TextError;

/// Maximum ranges in one field list.
pub const MAX_RANGES: usize = 16;

/// One selected interval, 1 based and inclusive on both ends.
/// `high == u32::MAX` means "to the end of the line" (`3-`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// First selected position, 1 based.
    pub low: u32,
    /// Last selected position, inclusive.
    pub high: u32,
}

/// Parse a `cut` list (`1,3-5,8-`) into up to 16 ranges, sorted by `low`
/// with overlaps kept (overlapping output is deduplicated at selection
/// time, matching the C tool printing each byte once).
pub fn parse_list(text: &str) -> Result<([Range; MAX_RANGES], usize), TextError> {
    if text.is_empty() {
        return Err(TextError::InvalidArgument);
    }
    let mut ranges: [Range; MAX_RANGES] = [Range { low: 1, high: 0 }; MAX_RANGES];
    let mut count = 0;
    for part in text.split(',') {
        if count >= MAX_RANGES {
            return Err(TextError::InvalidArgument);
        }
        ranges[count] = parse_part(part)?;
        count += 1;
    }
    // Insertion sort by low (lists are tiny).
    for i in 1..count {
        let mut j = i;
        while j > 0 && ranges[j].low < ranges[j - 1].low {
            ranges.swap(j, j - 1);
            j -= 1;
        }
    }
    Ok((ranges, count))
}

fn parse_part(part: &str) -> Result<Range, TextError> {
    if part.is_empty() {
        return Err(TextError::InvalidArgument);
    }
    match part.split_once('-') {
        None => {
            let n = parse_number(part)?;
            Ok(Range { low: n, high: n })
        }
        Some((low_text, high_text)) => {
            if low_text.is_empty() {
                // `-N`: from the start through N.
                Ok(Range {
                    low: 1,
                    high: parse_number(high_text)?,
                })
            } else if high_text.is_empty() {
                // `N-`: from N to the end of the line.
                Ok(Range {
                    low: parse_number(low_text)?,
                    high: u32::MAX,
                })
            } else {
                let low = parse_number(low_text)?;
                let high = parse_number(high_text)?;
                if low == 0 || high == 0 || low > high {
                    return Err(TextError::InvalidArgument);
                }
                Ok(Range { low, high })
            }
        }
    }
}

fn parse_number(text: &str) -> Result<u32, TextError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TextError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add((byte - b'0') as u32))
            .ok_or(TextError::InvalidArgument)?;
    }
    if value == 0 {
        return Err(TextError::InvalidArgument);
    }
    Ok(value)
}

/// Select byte ranges from `line` (1 based positions), writing into `out`.
///
/// Returns the used byte count. Overlapping ranges print each byte once.
pub fn select_bytes(
    line: &str,
    ranges: &[Range],
    out: &mut [u8],
) -> Result<usize, TextError> {
    let bytes = line.as_bytes();
    let mut written = 0;
    let mut next = 1u32;
    for range in ranges {
        let low = range.low.max(next);
        let high = range.high.min(bytes.len() as u32);
        if low <= high {
            for index in (low - 1)..high {
                if written >= out.len() {
                    return Err(TextError::TooLong);
                }
                out[written] = bytes[index as usize];
                written += 1;
            }
            next = high + 1;
        }
    }
    Ok(written)
}

/// Select delimiter separated fields from `line`.
///
/// Fields are 1 based; `ranges` reuse the same grammar. Lines without the
/// delimiter print whole unless `suppress` (`-s`) is set. The delimiter is
/// one byte.
pub fn select_fields(
    line: &str,
    ranges: &[Range],
    delimiter: u8,
    suppress: bool,
    out: &mut [u8],
) -> Result<usize, TextError> {
    if !line.as_bytes().contains(&delimiter) {
        if suppress {
            return Ok(0);
        }
        if line.len() > out.len() {
            return Err(TextError::TooLong);
        }
        out[..line.len()].copy_from_slice(line.as_bytes());
        return Ok(line.len());
    }
    let bytes = line.as_bytes();
    let mut written = 0;
    let mut field: u32 = 1;
    let mut start = 0;
    let mut first = true;
    let mut pos = 0;
    while pos <= bytes.len() {
        let end = pos == bytes.len() || bytes[pos] == delimiter;
        if end {
            if in_ranges(ranges, field) {
                if !first {
                    if written >= out.len() {
                        return Err(TextError::TooLong);
                    }
                    out[written] = delimiter;
                    written += 1;
                }
                first = false;
                for byte in &bytes[start..pos] {
                    if written >= out.len() {
                        return Err(TextError::TooLong);
                    }
                    out[written] = *byte;
                    written += 1;
                }
            }
            field += 1;
            start = pos + 1;
        }
        pos += 1;
    }
    Ok(written)
}

fn in_ranges(ranges: &[Range], field: u32) -> bool {
    ranges.iter().any(|range| range.low <= field && field <= range.high)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes_of(line: &str, list: &str) -> String {
        let (ranges, count) = parse_list(list).unwrap();
        let mut out = [0u8; 64];
        let len = select_bytes(line, &ranges[..count], &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    fn fields_of(line: &str, list: &str, delimiter: u8, suppress: bool) -> String {
        let (ranges, count) = parse_list(list).unwrap();
        let mut out = [0u8; 64];
        let len = select_fields(line, &ranges[..count], delimiter, suppress, &mut out).unwrap();
        String::from_utf8_lossy(&out[..len]).into_owned()
    }

    #[test]
    fn test_byte_selection() {
        assert_eq!(bytes_of("hello", "2-4"), "ell");
        assert_eq!(bytes_of("hello", "1,5"), "ho");
        assert_eq!(bytes_of("hello", "3-"), "llo");
        assert_eq!(bytes_of("hello", "-2"), "he");
    }

    #[test]
    fn test_overlaps_print_once() {
        assert_eq!(bytes_of("hello", "1-3,2-4"), "hell");
    }

    #[test]
    fn test_field_selection() {
        assert_eq!(fields_of("a:b:c", "2", b':', false), "b");
        assert_eq!(fields_of("a:b:c", "1,3", b':', false), "a:c");
        assert_eq!(fields_of("a:b:c", "2-", b':', false), "b:c");
    }

    #[test]
    fn test_missing_delimiter_policy() {
        assert_eq!(fields_of("abc", "1", b':', false), "abc");
        assert_eq!(fields_of("abc", "1", b':', true), "");
    }

    #[test]
    fn test_bad_lists_rejected() {
        assert!(parse_list("").is_err());
        assert!(parse_list("0").is_err());
        assert!(parse_list("5-2").is_err());
        assert!(parse_list("1,,2").is_err());
    }
}
