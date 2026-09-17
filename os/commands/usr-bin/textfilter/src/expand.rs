//! Tab expansion for `expand`.
//!
//! Ground truth: `minix3/usr.bin/expand/expand.c` (NetBSD). Tab stops
//! come from `-t a,b,c` (1 to 256 per stop, strictly increasing, at most
//! eight stops — `getstops`, lines 143-166) with the obsolete `-N` form
//! as its one-stop special case (lines 69-73). Expansion rules
//! (lines 96-127): no stops — pad to the next multiple of eight; one
//! stop — pad to the next multiple of that stop; several — pad to the
//! next stop beyond the current column, or one space past the last
//! stop. A backspace passes through and pulls the column back (never
//! below zero); a newline resets the column; every other byte advances
//! it. File operands (`freopen`, line 88) wait for the gated
//! open-existing call.

use alloc::string::String;
use alloc::vec::Vec;

/// Expansion options: the tab stop list (empty = every eight columns).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExpandOptions {
    /// Tab stop columns, strictly increasing (empty = every 8).
    pub stops: Vec<usize>,
}

/// Parses a `-t` stop list: comma- or space-separated, each 1 to 256,
/// strictly increasing, at most eight stops (`getstops`, expand.c:143-166).
/// `None` marks any violation.
/// Parses a `-t` stop list: comma- or space-separated, each stop 1 to
/// 256, strictly increasing, at most eight stops (`getstops`,
/// expand.c:143-166; a trailing separator leaves an empty piece the C
/// rejects as stop zero). `None` marks any violation.
pub fn parse_stops(spec: &str) -> Option<Vec<usize>> {
    let mut stops: Vec<usize> = Vec::new();
    for piece in spec.split([',', ' ']) {
        let bytes = piece.as_bytes();
        let mut value = 0usize;
        let mut digits = 0usize;
        for &byte in bytes {
            if !byte.is_ascii_digit() {
                break;
            }
            value = value * 10 + (byte - b'0') as usize;
            digits += 1;
        }
        if digits != bytes.len() || value == 0 || value > 256 {
            return None;
        }
        if !stops.is_empty() && value <= stops[stops.len() - 1] {
            return None;
        }
        if stops.len() == 8 {
            return None;
        }
        stops.push(value);
    }
    if stops.is_empty() {
        None
    } else {
        Some(stops)
    }
}

/// Expands tabs to spaces over the whole input.
pub fn expand(input: &str, stops: &[usize]) -> String {
    let mut out = Vec::new();
    let mut column = 0usize;
    for &byte in input.as_bytes() {
        match byte {
            b'\t' => {
                if stops.is_empty() {
                    loop {
                        out.push(b' ');
                        column += 1;
                        if column & 7 == 0 {
                            break;
                        }
                    }
                } else if stops.len() == 1 {
                    let stop = stops[0];
                    loop {
                        out.push(b' ');
                        column += 1;
                        if (column - 1) % stop == stop - 1 {
                            break;
                        }
                    }
                } else {
                    let next = stops.iter().find(|&&stop| stop > column);
                    match next {
                        Some(&stop) => {
                            while column < stop {
                                out.push(b' ');
                                column += 1;
                            }
                        }
                        None => {
                            out.push(b' ');
                            column += 1;
                        }
                    }
                }
            }
            b'\x08' => {
                if column > 0 {
                    column -= 1;
                }
                out.push(b'\x08');
            }
            b'\n' => {
                out.push(b'\n');
                column = 0;
            }
            _ => {
                out.push(byte);
                column += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_eight_column_stops() {
        assert_eq!(expand("a\tb", &[]), "a       b");
        assert_eq!(expand("\t", &[]), "        ");
        // The C's do-while emits at least one space even on a stop, then
        // pads to the next multiple of eight (expand.c:101-106).
        assert_eq!(expand("12345678\t", &[]), "12345678        ");
    }

    #[test]
    fn test_single_stop_multiples() {
        let stops = [4];
        assert_eq!(expand("a\tb", &stops), "a   b");
        assert_eq!(expand("abcd\t", &stops), "abcd    ");
    }

    #[test]
    fn test_stop_list_picks_next_beyond_column() {
        let stops = [1, 5, 20];
        assert_eq!(expand("ab\t", &stops), "ab   ");
        // Past the last stop: a single space.
        assert_eq!(expand("aaaaaaaaaaaaaaaaaaaa\t", &stops), "aaaaaaaaaaaaaaaaaaaa ");
    }

    #[test]
    fn test_backspace_pulls_the_column_and_passes_through() {
        // The backspace bytes pass through untouched (expand.c:128-133);
        // only the column tracking pulls back.
        assert_eq!(
            expand("abc\x08\x08x", &[4]),
            "abc\u{8}\u{8}x".to_string()
        );
        // A tab right after sees the pulled-back column: x sits at
        // column 2, the tab pads to the stop at 4.
        assert_eq!(
            expand("abc\x08\x08x\t", &[4]),
            "abc\u{8}\u{8}x  ".to_string()
        );
    }

    #[test]
    fn test_newline_resets_the_column() {
        let stops = [4];
        assert_eq!(expand("ab\nc\td", &stops), "ab\nc   d");
    }

    #[test]
    fn test_stop_list_validation() {
        assert!(parse_stops("1,4,8").is_some());
        assert!(parse_stops("4,2").is_none());
        assert!(parse_stops("0").is_none());
        assert!(parse_stops("257").is_none());
    }
}
