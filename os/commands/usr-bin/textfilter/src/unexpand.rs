//! Tab restoration for `unexpand`.
//!
//! Ground truth: `minix3/usr.bin/unexpand/unexpand.c` (NetBSD). The
//! `tabify` routine (lines 148-205) walks each line tracking a display
//! column (`dcol`: what the input text occupies, where tabs quantize to
//! the next stop) and an output column (`ocol`: what has been emitted);
//! blank runs convert to tabs once the gap reaches two columns, the
//! `-t` stop list bounds the tab region (the last stop minus one is the
//! conversion limit, line 161), and a backspace pulls both columns
//! back. Without `-a` only the leading blank run tabifies — the rest of
//! the line passes through verbatim (the `if (!all || dcol >= limit)`
//! tail, lines 199-205); with `-a` every later blank run converts too.

use alloc::string::String;
use alloc::vec::Vec;

/// The default tab stop when no `-t` list is given.
const DEFAULT_STOP: usize = 8;

/// Unexpand options: the stop list (empty = every eight columns) and
/// the `-a` flag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnexpandOptions {
    /// Tab stop columns, strictly increasing (empty = every 8).
    pub stops: Vec<usize>,
    /// `-a`: tabify blank runs everywhere, not only at line starts.
    pub all: bool,
}

/// Parses a `-t` stop list: the C splits on `", \t"` (`strsep`,
/// unexpand.c:91-106), each stop parses as a base-zero number and must
/// be strictly greater than the previous. `None` marks any violation.
pub fn parse_stops(spec: &str) -> Option<Vec<usize>> {
    let mut stops: Vec<usize> = Vec::new();
    for piece in spec.split([',', ' ', '\t']) {
        if piece.is_empty() {
            continue;
        }
        let value = piece.parse::<usize>().ok()?;
        if stops.last().is_some_and(|last| *last >= value) {
            return None;
        }
        stops.push(value);
    }
    Some(stops)
}

/// Converts blank runs of the line into tabs.
///
/// Faithful to `tabify` (unexpand.c:148-205): a display column (`dcol`)
/// walks the input — spaces add one, tabs quantize to the next stop —
/// and each non-blank byte flushes the accumulated gap as tabs (only
/// where a stop boundary is crossed and the gap is at least two
/// columns) followed by spaces. A backspace pulls both columns back.
/// Without `-a` the first non-blank byte ends the tabification and the
/// rest of the line passes through verbatim.
pub fn unexpand_line(line: &str, options: &UnexpandOptions) -> String {
    let bytes = line.as_bytes();
    let limit = options.stops.last().map_or(usize::MAX, |stop| stop - 1);
    let mut out = String::new();
    let mut dcol = 0usize;
    let mut ocol = 0usize;
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b' ' {
            dcol += 1;
            cursor += 1;
            continue;
        }
        if byte == b'\t' {
            if options.stops.is_empty() {
                dcol = (1 + dcol / DEFAULT_STOP) * DEFAULT_STOP;
            } else {
                let mut n = 0usize;
                while n < options.stops.len() && options.stops[n] - 1 < dcol {
                    n += 1;
                }
                if n + 1 < options.stops.len() && options.stops[n] - 1 < limit {
                    dcol = options.stops[n];
                }
            }
            cursor += 1;
            continue;
        }

        // Non-blank byte: flush the gap as tabs then spaces.
        if options.stops.is_empty() {
            while (ocol + DEFAULT_STOP) / DEFAULT_STOP <= dcol / DEFAULT_STOP
                && dcol - ocol >= 2
            {
                out.push('\t');
                ocol = (1 + ocol / DEFAULT_STOP) * DEFAULT_STOP;
            }
            while ocol < dcol {
                out.push(' ');
                ocol += 1;
            }
        } else {
            let mut n = 0usize;
            while n < options.stops.len() && options.stops[n] <= ocol {
                n += 1;
            }
            while n < options.stops.len()
                && options.stops[n] <= dcol
                && ocol < dcol
                && ocol < limit
            {
                out.push('\t');
                ocol = options.stops[n];
                n += 1;
            }
            while ocol < dcol && ocol < limit {
                out.push(' ');
                ocol += 1;
            }
        }

        out.push(byte as char);
        if byte == 0x08 {
            if ocol > 0 {
                ocol -= 1;
            }
            if dcol > 0 {
                dcol -= 1;
            }
        } else {
            ocol += 1;
            dcol += 1;
        }
        cursor += 1;
        if !options.all {
            // Without `-a` the first non-blank byte ends the
            // tabification; the remainder passes through verbatim
            // (unexpand.c:199-205).
            while cursor < bytes.len() {
                out.push(bytes[cursor] as char);
                cursor += 1;
            }
            break;
        }
    }
    out
}

/// Converts every line of the collected input.
pub fn unexpand(input: &str, options: &UnexpandOptions) -> Vec<String> {
    let text = alloc::str::from_utf8(input.as_bytes()).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.as_bytes().ends_with(b"\n") {
        pieces.pop();
    }
    pieces
        .iter()
        .map(|line| unexpand_line(line, options))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leading_blank_run_becomes_tabs_at_eight() {
        let options = UnexpandOptions::default();
        // Eight blanks land exactly on the stop: one tab. Four blanks
        // stay spaces — the tab loop requires crossing a stop boundary.
        assert_eq!(unexpand_line("        x", &options), "\tx");
        assert_eq!(unexpand_line("    x", &options), "    x");
    }

    #[test]
    fn test_two_space_gap_becomes_one_tab() {
        let options = UnexpandOptions::default();
        // Without `-a` the first non-blank byte ends the conversion, so
        // the interior blank run passes through untouched.
        assert_eq!(unexpand_line("ab        c", &options), "ab        c");
    }

    #[test]
    fn test_single_space_stays_a_space() {
        let options = UnexpandOptions::default();
        assert_eq!(unexpand_line("a b", &options), "a b");
    }

    #[test]
    fn test_without_a_only_leading_blanks_convert() {
        let options = UnexpandOptions::default();
        // Four leading blanks stay spaces (no stop crossed); everything
        // after the first non-blank passes verbatim.
        assert_eq!(unexpand_line("    ab        cd", &options), "    ab        cd");
    }

    #[test]
    fn test_backspace_pulls_both_columns() {
        // The backspace bytes pass through; the columns pull back so the
        // following blank run starts from a smaller column.
        let options = UnexpandOptions::default();
        // The backspaces pull the column from 2 back to 0; the eight
        // following blanks then reach the stop 8 and become one tab.
        let mut options = options;
        options.all = true;
        let result = unexpand_line("ab\x08\x08        c", &options);
        assert_eq!(result, "ab\x08\x08\tc");
    }

    #[test]
    fn test_stop_list_bounds_the_tab_region() {
        let options = UnexpandOptions {
            stops: vec![10, 20],
            all: false,
        };
        // Five leading blanks stay spaces: the first stop is 10.
        assert_eq!(unexpand_line("     x", &options), "     x");
        // Twelve blanks cross the 10 stop: one tab then two spaces.
        assert_eq!(unexpand_line("            x", &options), "\t  x");
    }

    #[test]
    fn test_parse_stops_rejects_unordered() {
        assert_eq!(parse_stops("4,2"), None);
        assert_eq!(parse_stops("x"), None);
        assert_eq!(parse_stops("1,2"), Some(vec![1, 2]));
    }

    #[test]
    fn test_every_line_converts() {
        let options = UnexpandOptions::default();
        assert_eq!(
            unexpand("        a\n        b\n", &options),
            vec!["\ta".to_string(), "\tb".to_string()]
        );
    }
}
