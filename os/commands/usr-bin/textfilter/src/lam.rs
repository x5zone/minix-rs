//! lam — laminate files side by side (minix3/usr.bin/lam/lam.c).
//!
//! Deciding half: the per-stream specs (separator, width format, pad
//! flag, line terminator character) and the row assembly loop over
//! in-memory line streams. File opening stays behind the hosted
//! command seam (registered as 待开放路径 like tee/split).
use alloc::vec::Vec;
use alloc::string::{String, ToString};
use alloc::{format, vec};

/// Per-stream options. Capitalized flags (`-F -P -S -T`) persist to
/// the streams that follow, matching the C's sticky globals.
#[derive(Debug, Clone, PartialEq)]
pub struct LamSpec {
    /// `-s sepstring` — printed before each of this stream's lines.
    pub sep: String,
    /// `-f min.max` — `%[-][min].[max]s` width clipping.
    pub width: Option<LamWidth>,
    /// `-p min.max` — like `-f` but pads missing (eof) fragments.
    pub pad: bool,
    /// `-t c` — the character ending this stream's lines.
    pub eol: u8,
}

impl Default for LamSpec {
    fn default() -> Self {
        LamSpec {
            sep: String::new(),
            width: None,
            pad: false,
            eol: b'\n',
        }
    }
}

/// Parsed `min.max` width spec; the C builds `"%<flags><min>.<max>s"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LamWidth {
    /// `-` flag: left justify.
    pub left: bool,
    /// Minimum field width (pad with spaces).
    pub min: usize,
    /// Maximum characters printed (precision).
    pub max: usize,
}

/// Errors mirroring lam.c's `error`/`errx` exits.
#[derive(Debug, PartialEq)]
pub enum LamError {
    TooManyStreams,
    NoStreams,
    NeedStringAfter(char),
    InvalidFormat(String),
}

/// Maximum input streams (the C's MAXOFILES).
pub const MAX_STREAMS: usize = 20;

/// Applies the width format exactly like `snprintf(lp, "%[-][min].[max]s", s)`:
/// clip to `max` characters, then pad with spaces to `min`.
pub fn apply_width(w: &LamWidth, s: &str) -> String {
    let mut total = 0usize;
    let mut end = 0usize;
    for (i, ch) in s.char_indices() {
        if total >= w.max {
            break;
        }
        // Byte-oriented width in the C; non-ASCII inputs are a
        // registered corner, so only count bytes here.
        total += ch.len_utf8();
        end = i + ch.len_utf8();
    }
    let mut out: String = s[..end].to_string();
    if out.len() < w.min {
        if w.left {
            out.push_str(&" ".repeat(w.min - out.len()));
        } else {
            out = format!("{}{}", " ".repeat(w.min - out.len()), out);
        }
    }
    out
}

/// Splits raw stream bytes into fragments on `eol`, keeping a final
/// fragment without terminator (the C reads until the eol char or
/// buffer end; EOF ends the stream).
pub fn split_stream(bytes: &[u8], eol: u8) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        if *b == eol {
            out.push(String::from_utf8_lossy(&bytes[start..i]).into_owned());
            start = i + 1;
        }
    }
    if start < bytes.len() {
        out.push(String::from_utf8_lossy(&bytes[start..]).into_owned());
    }
    out
}

/// Laminates the streams. Mirrors `gatherline`/`pad` + the main loop:
/// each round emits `sep + width(line)` for every stream; a stream
/// past EOF contributes `sep + (pad ? width("") : "")`; rounds stop
/// once every stream is exhausted.
pub fn laminate(streams: &[Vec<String>], specs: &[LamSpec]) -> Vec<String> {
    debug_assert_eq!(streams.len(), specs.len());
    let mut rows: Vec<String> = Vec::new();
    let mut pos = vec![0usize; streams.len()];
    loop {
        let mut alive = false;
        let mut row = String::new();
        for (i, lines) in streams.iter().enumerate() {
            let spec = &specs[i];
            row.push_str(&spec.sep);
            let fragment = if pos[i] < lines.len() {
                alive = true;
                let l = &lines[pos[i]];
                pos[i] += 1;
                match spec.width {
                    Some(w) => apply_width(&w, l),
                    None => l.clone(),
                }
            } else if spec.pad {
                match spec.width {
                    Some(w) => apply_width(&w, ""),
                    None => String::new(),
                }
            } else {
                String::new()
            };
            row.push_str(&fragment);
        }
        if !alive {
            break;
        }
        rows.push(row);
    }
    rows
}

/// Parses the `min.max` width argument. The C restricts the string to
/// `-.0123456789` and wraps it into `%...s`.
pub fn parse_width(arg: &str) -> Result<LamWidth, LamError> {
    if !arg
        .bytes()
        .all(|b| b == b'-' || b == b'.' || b.is_ascii_digit())
    {
        return Err(LamError::InvalidFormat(arg.to_string()));
    }
    let left = arg.starts_with('-');
    let body = if left { &arg[1..] } else { arg };
    let (min_s, max_s) = match body.split_once('.') {
        Some((a, b)) => (a, b),
        None => (body, ""),
    };
    Ok(LamWidth {
        left,
        min: if min_s.is_empty() { 0 } else { min_s.parse().unwrap_or(0) },
        max: if max_s.is_empty() { usize::MAX } else { max_s.parse().unwrap_or(0) },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_two_streams_side_by_side() {
        let a = split_stream(b"1\n2\n", b'\n');
        let b = split_stream(b"x\ny\n", b'\n');
        let rows = laminate(&[a, b], &[LamSpec::default(), LamSpec::default()]);
        assert_eq!(rows, vec!["1x", "2y"]);
    }

    #[test]
    fn test_separator_applied_per_stream() {
        let a = split_stream(b"1\n2\n", b'\n');
        let b = split_stream(b"x\ny\n", b'\n');
        let specs = vec![
            LamSpec::default(),
            LamSpec {
                sep: "|".to_string(),
                ..LamSpec::default()
            },
        ];
        let rows = laminate(&[a, b], &specs);
        assert_eq!(rows, vec!["1|x", "2|y"]);
    }

    #[test]
    fn test_uneven_streams_pad_or_not() {
        // Stream b is shorter: without -p it just vanishes; with -p
        // the empty fragment is width-padded.
        let a = split_stream(b"1\n2\n3\n", b'\n');
        let b = split_stream(b"x\n", b'\n');
        let rows = laminate(&[a, b], &[LamSpec::default(), LamSpec::default()]);
        assert_eq!(rows, vec!["1x", "2", "3"]);
        let a = split_stream(b"1\n2\n3\n", b'\n');
        let b = split_stream(b"x\n", b'\n');
        let specs = vec![
            LamSpec::default(),
            LamSpec {
                pad: true,
                width: Some(LamWidth { left: false, min: 3, max: usize::MAX }),
                ..LamSpec::default()
            },
        ];
        let rows = laminate(&[a, b], &specs);
        // No `-` flag: right-justify, so "x" sits in the last column.
        assert_eq!(rows, vec!["1  x", "2   ", "3   "]);
    }

    #[test]
    fn test_width_clips_then_pads() {
        let w = parse_width("8.2").unwrap();
        assert_eq!(apply_width(&w, "abcdef"), "      ab");
        let w = parse_width("-8.2").unwrap();
        assert_eq!(apply_width(&w, "abcdef"), "ab      ");
        let w = parse_width("2.10").unwrap();
        assert_eq!(apply_width(&w, "abc"), "abc");
    }

    #[test]
    fn test_width_parse_rejects_junk() {
        assert_eq!(parse_width("8x"), Err(LamError::InvalidFormat("8x".into())));
        assert!(parse_width("8.10").is_ok());
        assert!(parse_width("-4").is_ok());
    }

    #[test]
    fn test_custom_terminator_character() {
        // -t : fragments end at any given byte.
        let a = split_stream(b"1:2:", b':');
        assert_eq!(a, vec!["1", "2"]);
    }

    #[test]
    fn test_more_than_two_streams() {
        let a = split_stream(b"a\n", b'\n');
        let b = split_stream(b"b\n", b'\n');
        let c = split_stream(b"c\n", b'\n');
        let rows = laminate(&[a, b, c], &[
            LamSpec::default(),
            LamSpec { sep: ",".into(), ..LamSpec::default() },
            LamSpec { sep: "-".into(), ..LamSpec::default() },
        ]);
        assert_eq!(rows, vec!["a,b-c"]);
    }
}
