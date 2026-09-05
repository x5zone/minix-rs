//! Shell archive (`shar`) stanza parsing.
//!
//! Ground truth: `minix3/usr.bin/shar/shar.sh` (a shell script). A shell
//! archive is a shell script that recreates files when run: a preamble of
//! comments, then one stanza per file opening with a `begin`-style marker
//! carrying the mode and name, the file body, and an `end` marker. This
//! module parses those stanzas back into (mode, name, body) triples — the
//! direction an extractor needs. Generation (printing stanzas) is plain
//! formatting the caller owns.
//!
//! Format accepted here (the script's stable subset):
//!
//! ```text
//! # : shar archive, created ...
//! begin 644 notes.txt
//! <body lines>
//! end
//! ```
//!
//! Body lines starting with `X` are unwrapped (the script prefixes lines
//! that would otherwise confuse mail transport or the shell). A stanza
//! without its `end` marker is an error, not a silent truncation.

use crate::CompressError;

/// Maximum files per archive scan.
pub const MAX_FILES: usize = 16;

/// One archived file stanza.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharFile<'a> {
    /// File permission bits in octal (as written, e.g. 644).
    pub mode: u32,
    /// File name as written.
    pub name: &'a str,
    /// Body lines joined verbatim (wrapping `X` prefixes included; unwrap
    /// with [`unwrap_line`]), without the trailing newline.
    pub body: &'a str,
    /// Byte offsets of the body within the scanned text (for callers that
    /// re-slice instead of copying).
    pub body_range: (usize, usize),
}

/// Scan `text` for file stanzas, returning up to 16 entries.
///
/// Comment lines (`#` first) and blank lines outside stanzas are skipped.
/// A `begin` line has the shape `begin MODE NAME`; the stanza runs to the
/// next line that is exactly `end`. Missing `end`, missing name, or a
/// non octal mode is an error.
pub fn scan_archive(text: &str) -> Result<([SharFile<'_>; MAX_FILES], usize), CompressError> {
    let mut files: [SharFile<'_>; MAX_FILES] = [SharFile {
        mode: 0,
        name: "",
        body: "",
        body_range: (0, 0),
    }; MAX_FILES];
    let mut count = 0;
    let mut offset = 0;
    let bytes = text.as_bytes();
    while offset < bytes.len() {
        let line_end = match text[offset..].find('\n') {
            Some(index) => offset + index,
            None => bytes.len(),
        };
        let line = &text[offset..line_end];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            offset = line_end + 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("begin ") {
            if count >= MAX_FILES {
                return Err(CompressError::TooLong);
            }
            let (mode_text, name) = rest.split_once([' ', '\t']).ok_or(CompressError::InvalidArgument)?;
            let mode = parse_octal(mode_text.trim())?;
            let name = name.trim();
            if name.is_empty() {
                return Err(CompressError::InvalidArgument);
            }
            let body_start = line_end + 1;
            let (body_end, after) = find_end(text, body_start)?;
            files[count] = SharFile {
                mode,
                name,
                // The body stays verbatim (wrapping `X` prefixes included);
                // callers unwrap line by line with `unwrap_line`.
                body: &text[body_start..body_end],
                body_range: (body_start, body_end),
            };
            count += 1;
            offset = after;
        } else {
            return Err(CompressError::InvalidArgument);
        }
    }
    Ok((files, count))
}

/// Find the closing `end` line from `offset`; returns the body end (start
/// of the `end` line) plus the offset past it.
fn find_end(text: &str, offset: usize) -> Result<(usize, usize), CompressError> {
    let bytes = text.as_bytes();
    let mut cursor = offset;
    while cursor <= bytes.len() {
        let line_end = match text[cursor..].find('\n') {
            Some(index) => cursor + index,
            None => bytes.len(),
        };
        if text[cursor..line_end].trim() == "end" {
            return Ok((cursor, (line_end + 1).min(bytes.len())));
        }
        if cursor == bytes.len() {
            break;
        }
        cursor = line_end + 1;
    }
    Err(CompressError::InvalidArgument)
}

/// Copy one body line into `out`, dropping a single leading `X` when
/// present. Returns the used byte count.
pub fn unwrap_line(line: &str, out: &mut [u8]) -> Result<usize, CompressError> {
    let bytes = line.as_bytes();
    let body = bytes.strip_prefix(b"X").unwrap_or(bytes);
    if body.len() > out.len() {
        return Err(CompressError::TooLong);
    }
    out[..body.len()].copy_from_slice(body);
    Ok(body.len())
}

fn parse_octal(text: &str) -> Result<u32, CompressError> {
    if text.is_empty() || text.len() > 4 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(CompressError::InvalidArgument);
    }
    let mut value: u32 = 0;
    for byte in text.bytes() {
        if !(b'0'..=b'7').contains(&byte) {
            return Err(CompressError::InvalidArgument);
        }
        value = value * 8 + (byte - b'0') as u32;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARCHIVE: &str = "# : shar archive\nbegin 644 notes.txt\nhello\nX#not-a-comment\nend\nbegin 600 empty\nend\n";

    #[test]
    fn test_two_stanzas() {
        let (files, count) = scan_archive(ARCHIVE).unwrap();
        assert_eq!(count, 2);
        assert_eq!(files[0].mode, 0o644);
        assert_eq!(files[0].name, "notes.txt");
        assert_eq!(files[1].mode, 0o600);
        assert_eq!(files[1].body, "");
    }

    #[test]
    fn test_body_kept_verbatim() {
        let (files, _) = scan_archive(ARCHIVE).unwrap();
        assert_eq!(files[0].body, "hello\nX#not-a-comment\n");
    }

    #[test]
    fn test_unwrap_line_drops_x() {
        let mut out = [0u8; 32];
        let len = unwrap_line("X#not-a-comment", &mut out).unwrap();
        assert_eq!(&out[..len], b"#not-a-comment");
        let len = unwrap_line("plain", &mut out).unwrap();
        assert_eq!(&out[..len], b"plain");
    }

    #[test]
    fn test_missing_end_rejected() {
        assert_eq!(
            scan_archive("begin 644 x\nhalf\n").map(|_| ()),
            Err(CompressError::InvalidArgument)
        );
    }

    #[test]
    fn test_bad_mode_rejected() {
        assert_eq!(
            scan_archive("begin 999 x\nend\n").map(|_| ()),
            Err(CompressError::InvalidArgument)
        );
        assert_eq!(
            scan_archive("begin\nend\n").map(|_| ()),
            Err(CompressError::InvalidArgument)
        );
    }

    #[test]
    fn test_stray_text_rejected() {
        assert_eq!(
            scan_archive("hello\n").map(|_| ()),
            Err(CompressError::InvalidArgument)
        );
    }
}
