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
use alloc::vec::Vec;

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

/// Parsed `cut` command line: the byte (`-b`/`-c`) and field (`-f`) list
/// selectors, the `-d` delimiter, the `-s` suppress flag, and file operands
/// (a bare `-` names standard input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutArgs<'a> {
    pub bytes_list: Option<&'a str>,
    pub fields_list: Option<&'a str>,
    pub delimiter: u8,
    pub suppress: bool,
    pub operands: Vec<&'a str>,
}

/// `cut` argument failure. Each arm maps to a distinct C path (the caller
/// renders it): `getopt` `?`/default → `Usage`, the `b_flag + f_flag` guard
/// (`cut.c:120`) → `Conflict`, a `-d` value that is not one byte → `BadDelimiter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CutError {
    Usage,
    Conflict,
    BadDelimiter,
}

/// Fold `cut`'s argv tail faithfully to BSD `getopt("b:c:d:f:sn")` (`cut.c:88`):
/// `-b`/`-c`/`-f`/`-d` take a value attached (`-f1-3`, `-d:`) or as the next
/// word; `-s`/`-n` are flags (so `-sf1-3` is `-s` then `-f 1-3`, and `-sn`/`-ns`
/// both set suppress); `--` ends option scanning; any other dash letter is a
/// usage error, never a leaked operand. `-b`/`-c` set the byte list (they
/// coincide here — the selection is byte-grained), and a byte list alongside a
/// field list is the C `conflict`.
pub fn parse_cut_args<'a>(args: &'a [&'a str]) -> Result<CutArgs<'a>, CutError> {
    let mut bytes_list: Option<&'a str> = None;
    let mut fields_list: Option<&'a str> = None;
    let mut delimiter: u8 = b'\t';
    let mut suppress = false;
    let mut operands: Vec<&'a str> = Vec::new();

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index];
        if arg == "--" {
            operands.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = arg.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'b' | b'c' | b'f' => {
                        let value = if pos + 1 < bytes.len() {
                            &arg[pos + 1..]
                        } else {
                            index += 1;
                            match args.get(index) {
                                Some(v) => *v,
                                None => return Err(CutError::Usage),
                            }
                        };
                        if bytes[pos] == b'f' {
                            if bytes_list.is_some() {
                                return Err(CutError::Conflict);
                            }
                            fields_list = Some(value);
                        } else {
                            if fields_list.is_some() {
                                return Err(CutError::Conflict);
                            }
                            bytes_list = Some(value);
                        }
                        pos = bytes.len(); // the value consumed the token's tail
                    }
                    b'd' => {
                        let value = if pos + 1 < bytes.len() {
                            &arg[pos + 1..]
                        } else {
                            index += 1;
                            match args.get(index) {
                                Some(v) => *v,
                                None => return Err(CutError::Usage),
                            }
                        };
                        let mut vb = value.bytes();
                        delimiter = match (vb.next(), vb.next()) {
                            (Some(one), None) => one,
                            _ => return Err(CutError::BadDelimiter),
                        };
                        pos = bytes.len();
                    }
                    b's' => {
                        suppress = true;
                        pos += 1;
                    }
                    // `-n`: accepted and ignored for compatibility
                    // (`cut.c:112-113` `case 'n': break`), never a leaked operand.
                    b'n' => pos += 1,
                    _ => return Err(CutError::Usage),
                }
            }
            index += 1;
            continue;
        }
        operands.push(arg);
        index += 1;
    }
    Ok(CutArgs {
        bytes_list,
        fields_list,
        delimiter,
        suppress,
        operands,
    })
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

    #[test]
    fn test_parse_cut_args_faithful_getopt() {
        // Defaults: a field list with attached and next-word values.
        let a = parse_cut_args(&["-f", "1-3", "file"]).unwrap();
        assert_eq!(a.fields_list, Some("1-3"));
        assert_eq!(a.operands, vec!["file"]);
        assert_eq!(a.delimiter, b'\t');
        // Attached list value (regression vs whole-token leak).
        assert_eq!(
            parse_cut_args(&["-f1-3", "file"]).unwrap().fields_list,
            Some("1-3")
        );
        // KEY regression: a flag clustered with a value option (`-sf1-3`).
        let s = parse_cut_args(&["-sf1-3", "file"]).unwrap();
        assert!(s.suppress);
        assert_eq!(s.fields_list, Some("1-3"));
        // `-sn` and `-ns` both set suppress (`-n` is the compat no-op).
        assert!(parse_cut_args(&["-sn", "-f1"]).unwrap().suppress);
        assert!(parse_cut_args(&["-ns", "-f1"]).unwrap().suppress);
        // `-d` attached and separate; `-b`/`-c` set the byte list.
        assert_eq!(parse_cut_args(&["-d:", "-f1"]).unwrap().delimiter, b':');
        assert_eq!(parse_cut_args(&["-d", ",", "-f1"]).unwrap().delimiter, b',');
        assert_eq!(
            parse_cut_args(&["-c1-2", "file"]).unwrap().bytes_list,
            Some("1-2")
        );
        // `--` ends scanning; bare `-` is an operand.
        assert_eq!(
            parse_cut_args(&["-f1", "--", "-x"]).unwrap().operands,
            vec!["-x"]
        );
        assert_eq!(parse_cut_args(&["-f1", "-"]).unwrap().operands, vec!["-"]);
        // error paths
        assert_eq!(parse_cut_args(&["-x", "file"]).err(), Some(CutError::Usage));
        assert_eq!(parse_cut_args(&["-f"]).err(), Some(CutError::Usage));
        assert_eq!(
            parse_cut_args(&["-b1", "-f1"]).err(),
            Some(CutError::Conflict)
        );
        assert_eq!(
            parse_cut_args(&["-f1", "-b1"]).err(),
            Some(CutError::Conflict)
        );
        assert_eq!(
            parse_cut_args(&["-dab", "-f1"]).err(),
            Some(CutError::BadDelimiter)
        );
    }
}
