//! Shell archive (`shar`) — generation and the classic `begin`/`end` framing.
//!
//! Ground truth for the command: `minix3/usr.bin/shar/shar.sh` (a shell
//! script). `shar` writes a self-extracting `sh` script to standard output:
//! a fixed comment preamble, a `#`-tab index naming every operand, then one
//! stanza per operand. A directory operand becomes `echo c - NAME` plus
//! `mkdir -p NAME`; a file operand becomes `echo x - NAME`, a here-document
//! `sed 's/^X//' >NAME << 'END-of-NAME'`, the file body with every source
//! line prefixed by `X` (so a line that equals the delimiter, or one that
//! would confuse mail transport, survives intact), and the `END-of-NAME`
//! terminator; the script closes with `exit`. [`generate`] reproduces that
//! output byte-for-byte — this is the direction the `shar` command itself
//! needs.
//!
//! [`scan_archive`] parses a *different*, older framing — the
//! `begin MODE NAME` / `end` stanza used by other shell-archive tools and
//! the `uudecode`-style extractor. It is kept so this crate can read such
//! archives (and `unwrap_line` drops their `X` wrapping), but it is NOT what
//! `minix3`'s own `shar` prints; the two framings do not round-trip into
//! each other.

use alloc::vec::Vec;

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

/// One operand of a generated `shar` archive.
///
/// Mirrors the branch `shar.sh` takes for each command-line operand: a
/// directory becomes an `mkdir -p` stanza, anything else becomes a here-doc
/// stanza carrying the file's raw bytes (the `sed 's/^/X/'` leg of the
/// script is folded into [`generate`] here, so the caller only reads the
/// file and hands its contents over).
#[derive(Debug, Clone, Copy)]
pub enum SharEntry<'a> {
    /// A directory operand.
    Dir(&'a str),
    /// A regular-file operand: its name and its raw contents.
    File { name: &'a str, body: &'a [u8] },
}

impl<'a> SharEntry<'a> {
    /// The operand name as it is echoed into the index and stanza headers.
    pub fn name(&self) -> &'a str {
        match self {
            SharEntry::Dir(name) => name,
            SharEntry::File { name, .. } => name,
        }
    }
}

/// Assemble a `shar` self-extracting archive, byte-for-byte the shape
/// `minix3/usr.bin/shar/shar.sh` prints: preamble, `#`-tab index of every
/// operand, one stanza per operand, then `exit` and a trailing blank line.
///
/// File bodies are wrapped by prefixing every source line with `X` (the
/// script's `sed 's/^/X/'`), matching what an extractor's `sed 's/^X//'`
/// reverses. A file that lacks a final newline gains one in the archive —
/// the here-doc needs its terminator on a line of its own, so the wrapped
/// body always ends in a newline (see the module's declared residuals).
pub fn generate(entries: &[SharEntry<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    // Preamble: the `cat << EOF` block of `shar.sh:40-48`, kept verbatim
    // (including BSD's two-space sentence separators).
    out.extend(
        b"# This is a shell archive.  Save it in a file, remove anything before\n\
# this line, and then unpack it by entering \"sh file\".  Note, it may\n\
# create directories; files and directories will be owned by you and\n\
# have default permissions.\n\
#\n\
# This archive contains:\n\
#\n",
    );
    // Index: `for i; do echo "#\t$i"; done` then `echo "#"` (shar.sh:50-55).
    for entry in entries {
        out.extend(b"#\t");
        out.extend(entry.name().as_bytes());
        out.push(b'\n');
    }
    out.extend(b"#\n");
    // Stanzas: the second `for` loop (shar.sh:57-68).
    for entry in entries {
        match entry {
            SharEntry::Dir(name) => {
                out.extend(b"echo c - ");
                out.extend(name.as_bytes());
                out.push(b'\n');
                out.extend(b"mkdir -p ");
                out.extend(name.as_bytes());
                out.extend(b" > /dev/null 2>&1\n");
            }
            SharEntry::File { name, body } => {
                out.extend(b"echo x - ");
                out.extend(name.as_bytes());
                out.push(b'\n');
                out.extend(b"sed 's/^X//' >");
                out.extend(name.as_bytes());
                out.extend(b" << 'END-of-");
                out.extend(name.as_bytes());
                out.extend(b"'\n");
                for line in body_lines(body) {
                    out.push(b'X');
                    out.extend(line);
                    out.push(b'\n');
                }
                out.extend(b"END-of-");
                out.extend(name.as_bytes());
                out.push(b'\n');
            }
        }
    }
    // Footer: `echo exit` then `echo ""` (shar.sh:69-70).
    out.extend(b"exit\n");
    out.push(b'\n');
    out
}

/// Split a file body into its source lines, dropping the empty segment a
/// final newline would leave. An empty body yields no lines (the script's
/// `sed 's/^/X/'` prints nothing for an empty file).
fn body_lines(body: &[u8]) -> Vec<&[u8]> {
    if body.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&[u8]> = body.split(|&byte| byte == b'\n').collect();
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines
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

    /// The fixed `shar.sh:40-48` preamble, spelled once for the golden tests.
    const PREAMBLE: &str = "# This is a shell archive.  Save it in a file, remove anything before\n\
# this line, and then unpack it by entering \"sh file\".  Note, it may\n\
# create directories; files and directories will be owned by you and\n\
# have default permissions.\n\
#\n\
# This archive contains:\n\
#\n";

    #[test]
    fn test_generate_single_file_golden() {
        let out = generate(&[SharEntry::File { name: "a", body: b"hi\nthere\n" }]);
        let expected = format!(
            "{PREAMBLE}#\ta\n#\necho x - a\nsed 's/^X//' >a << 'END-of-a'\nXhi\nXthere\nEND-of-a\nexit\n\n"
        );
        assert_eq!(core::str::from_utf8(&out).unwrap(), expected);
    }

    #[test]
    fn test_generate_dir_stanza() {
        let out = generate(&[SharEntry::Dir("sub")]);
        let expected = format!("{PREAMBLE}#\tsub\n#\necho c - sub\nmkdir -p sub > /dev/null 2>&1\nexit\n\n");
        assert_eq!(core::str::from_utf8(&out).unwrap(), expected);
    }

    #[test]
    fn test_generate_index_lists_every_operand_in_order() {
        let out = generate(&[
            SharEntry::Dir("d1"),
            SharEntry::File { name: "f1", body: b"x" },
            SharEntry::File { name: "f2", body: b"y" },
        ]);
        let text = core::str::from_utf8(&out).unwrap();
        // The index lines are the only ones that start with `#` + a tab.
        let index = text
            .lines()
            .filter(|line| line.starts_with("#\t"))
            .collect::<Vec<_>>();
        assert_eq!(index, vec!["#\td1", "#\tf1", "#\tf2"]);
    }

    #[test]
    fn test_generate_empty_body_has_no_wrapped_lines() {
        // `sed 's/^/X/'` of an empty file is nothing, so the here-doc closes
        // on the very next line after its header.
        let out = generate(&[SharEntry::File { name: "e", body: b"" }]);
        let text = core::str::from_utf8(&out).unwrap();
        assert!(text.contains("sed 's/^X//' >e << 'END-of-e'\nEND-of-e\n"));
    }

    #[test]
    fn test_generate_body_wrapping_roundtrips_through_unwrap_line() {
        // Every wrapped line begins with `X`; the extractor's `unwrap_line`
        // recovers the original source line, tying the two directions of this
        // module together on the shared `X` convention.
        let body = b"first\nsecond\n";
        let out = generate(&[SharEntry::File { name: "f", body }]);
        let text = core::str::from_utf8(&out).unwrap();
        let wrapped = text
            .lines()
            .filter(|line| line.starts_with('X'))
            .collect::<Vec<_>>();
        assert_eq!(wrapped, vec!["Xfirst", "Xsecond"]);
        let mut buf = [0u8; 16];
        let len = unwrap_line(wrapped[1], &mut buf).unwrap();
        assert_eq!(&buf[..len], b"second");
    }

    #[test]
    fn test_generate_matches_real_shar_script() {
        // Pinned against the actual `sh shar.sh subdir hello.txt empty.txt`
        // output (dir + non-empty file + empty file), so the multi-operand
        // index ordering, per-type stanza shape, empty-body handling and the
        // `exit`/blank trailer are held to the ground-truth script byte-for-byte.
        let out = generate(&[
            SharEntry::Dir("subdir"),
            SharEntry::File {
                name: "hello.txt",
                body: b"hi\nthere\n",
            },
            SharEntry::File {
                name: "empty.txt",
                body: b"",
            },
        ]);
        let expected = format!(
            "{PREAMBLE}#\tsubdir\n#\thello.txt\n#\tempty.txt\n#\n\
echo c - subdir\nmkdir -p subdir > /dev/null 2>&1\n\
echo x - hello.txt
sed 's/^X//' >hello.txt << 'END-of-hello.txt'
Xhi
Xthere
END-of-hello.txt
\
echo x - empty.txt\nsed 's/^X//' >empty.txt << 'END-of-empty.txt'\nEND-of-empty.txt\n\
exit\n\n"
        );
        assert_eq!(core::str::from_utf8(&out).unwrap(), expected);
    }
}
