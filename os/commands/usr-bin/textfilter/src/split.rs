//! Chunk computation for `split`.
//!
//! Ground truth: `minix3/usr.bin/split/split.c` (NetBSD): the input
//! splits into chunks of `DEFLINE` (1000, split.c:56) lines — `-l`
//! moves the count — and the chunk files are named by a suffix that
//! enumerates `aa`, `ab`, ... `zz` (`sfxlen` defaults to 2,
//! split.c:61). This module computes the chunks (which lines go to
//! which chunk) and the suffix sequence; the doing half's file writes
//! wait for the gated create/write face on the host, so the doing half
//! can only report what it would create. An explicit `-b` byte count
//! and the suffix-length flag are later batches.

use alloc::string::String;
use alloc::vec::Vec;

/// Default lines per chunk (`DEFLINE`, split.c:56).
pub const DEFAULT_LINES: usize = 1000;

/// Computes the chunks: each entry holds that chunk's lines (the last
/// one may be short). `lines_per_chunk` of zero is a caller error the
/// doing half rejects.
pub fn chunks(lines: &[String], lines_per_chunk: usize) -> Vec<Vec<String>> {
    let mut chunks = Vec::new();
    if lines_per_chunk == 0 {
        return chunks;
    }
    let mut cursor = 0usize;
    while cursor < lines.len() {
        let end = (cursor + lines_per_chunk).min(lines.len());
        chunks.push(lines[cursor..end].to_vec());
        cursor = end;
    }
    chunks
}

/// The file suffix for chunk `index` (0 based): two lowercase letters
/// enumerating `aa`, `ab`, ..., `zz` (the C's default `sfxlen` 2).
/// `None` when the sequence is exhausted (past `zz`).
pub fn suffix(index: usize) -> Option<String> {
    if index >= 26 * 26 {
        return None;
    }
    let mut out = Vec::new();
    let mut value = index;
    let places = 2;
    for place in (0..places).rev() {
        let divisor = 26usize.pow(place);
        out.push(b'a' + (value / divisor) as u8);
        value %= divisor;
    }
    Some(out.into_iter().map(|b| b as char).collect())
}

/// Parsed `split` command line: the resolved line count and the file operands
/// in order (a bare `-` names standard input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitArgs<'a> {
    pub lines_per_chunk: usize,
    pub operands: Vec<&'a str>,
}

/// `split` argument failure, each arm mirroring a distinct C path so the caller
/// can reproduce C's message and exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitError<'a> {
    /// `getopt` `default:`, a missing `-l` value, or a `-l` once the count is
    /// already set (`numlines != 0`): the C `usage()`.
    Usage,
    /// A `-l`/`-N` value that is not all-decimal or is zero: C `errx(1, "%s:
    /// illegal line count.", …)`; carries the offending value for the message.
    IllegalLineCount(&'a str),
    /// `-a`/`-b`/`-n` — valid `getopt` letters the line-only deciding half
    /// (`chunks`/`suffix`) does not yet implement (suffix width, byte split,
    /// chunk count); reported rather than silently ignored.
    Unmodelled,
}

/// An all-decimal, nonzero line count, or `None` (the C `strtoull` + `*ep !=
/// '\0'` + `== 0` rejection). An over-wide digit run (past `usize`) is also
/// `None` here — an absurd input C would saturate, treated as illegal rather
/// than panicking.
fn line_count(v: &str) -> Option<usize> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match v.parse::<usize>() {
        Ok(n) if n > 0 => Some(n),
        _ => None,
    }
}

/// Fold `split`'s argv tail faithfully to BSD `getopt("0123456789b:l:a:n:")`
/// (`split.c:80`).
///
/// Parity over the old whole-token matcher (`arg == "-l"`, value forced onto a
/// separate word): `-l` now takes its value either attached (`-l100`) or as the
/// next word (`-l 100`), and an all-digit `-N` is the obsolete line-count
/// kludge (`split.c:82-98`, applied only while the count is unset so a later
/// `-l` still trips the `numlines != 0` usage as C does). `--` ends option
/// scanning; a bare `-` and any non-option token are operands. `-a`/`-b`/`-n`
/// are reported `Unmodelled` (the deciding half splits on line counts only).
pub fn parse_split_args<'a>(args: &'a [&'a str]) -> Result<SplitArgs<'a>, SplitError<'a>> {
    let mut lines_per_chunk = DEFAULT_LINES;
    let mut set = false;
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
            let tail = &arg[1..];
            // obsolete `-N`: an all-digit tail sets the line count, but only
            // while it is unset (C runs the kludge `if (numlines == 0)`).
            if tail.bytes().all(|b| b.is_ascii_digit()) {
                if !set {
                    match line_count(tail) {
                        Some(n) => {
                            lines_per_chunk = n;
                            set = true;
                        }
                        None => return Err(SplitError::IllegalLineCount(tail)),
                    }
                }
                index += 1;
                continue;
            }
            match bytes[1] {
                b'l' => {
                    // C `case 'l'`: `if (numlines != 0) usage()`.
                    if set {
                        return Err(SplitError::Usage);
                    }
                    // Value attached after `l` (`-l100`) or the next word
                    // (`-l 100`).
                    let rest = &tail[1..];
                    let value = if rest.is_empty() {
                        index += 1;
                        match args.get(index) {
                            Some(v) => *v,
                            None => return Err(SplitError::Usage),
                        }
                    } else {
                        rest
                    };
                    match line_count(value) {
                        Some(n) => lines_per_chunk = n,
                        None => return Err(SplitError::IllegalLineCount(value)),
                    }
                    set = true;
                }
                b'a' | b'b' | b'n' => return Err(SplitError::Unmodelled),
                _ => return Err(SplitError::Usage),
            }
            index += 1;
            continue;
        }
        // A bare `-` (length 1) and any non-option token are operands.
        operands.push(arg);
        index += 1;
    }

    // C reads one input operand then the output prefix (`split.c:135-143`);
    // more than two operands is rejected here (the old shell's guard, kept).
    if operands.len() > 2 {
        return Err(SplitError::Usage);
    }
    Ok(SplitArgs {
        lines_per_chunk,
        operands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_chunks_of_thousand() {
        let lines: Vec<String> = (1..=2500).map(|n| n.to_string()).collect();
        let chunks = chunks(&lines, DEFAULT_LINES);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), 1000);
        assert_eq!(chunks[1].len(), 1000);
        assert_eq!(chunks[2].len(), 500);
    }

    #[test]
    fn test_explicit_count_and_empty_input() {
        let lines: Vec<String> = (1..=5).map(|n| n.to_string()).collect();
        let groups = chunks(&lines, 2);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[2], vec!["5".to_string()]);
        assert!(chunks(&[], 10).is_empty());
    }

    #[test]
    fn test_zero_count_is_no_chunks() {
        let lines = vec!["a".to_string()];
        assert!(chunks(&lines, 0).is_empty());
    }

    #[test]
    fn test_suffix_enumerates_two_letters() {
        assert_eq!(suffix(0).as_deref(), Some("aa"));
        assert_eq!(suffix(1).as_deref(), Some("ab"));
        assert_eq!(suffix(25).as_deref(), Some("az"));
        assert_eq!(suffix(26).as_deref(), Some("ba"));
        assert_eq!(suffix(675).as_deref(), Some("zz"));
        assert_eq!(suffix(676), None);
    }

    #[test]
    fn parse_default_and_separated_value() {
        // 无参：缺省 1000 行。分离值（向后兼容）。
        let d = parse_split_args(&[]).unwrap();
        assert_eq!(d.lines_per_chunk, DEFAULT_LINES);
        assert!(d.operands.is_empty());
        let a = parse_split_args(&["-l", "100", "f"]).unwrap();
        assert_eq!(a.lines_per_chunk, 100);
        assert_eq!(a.operands, vec!["f"]);
    }

    #[test]
    fn parse_attached_and_obsolete_accepted() {
        // 关键回归：旧前门拒这两个合法形。
        let a = parse_split_args(&["-l100", "f"]).unwrap();
        assert_eq!(a.lines_per_chunk, 100);
        assert_eq!(a.operands, vec!["f"]);
        let b = parse_split_args(&["-1000", "in", "out"]).unwrap();
        assert_eq!(b.lines_per_chunk, 1000);
        assert_eq!(b.operands, vec!["in", "out"]);
        // 裸 `-` 为操数、`--` 终止扫描。
        let c = parse_split_args(&["-l", "5", "-", "pfx"]).unwrap();
        assert_eq!(c.operands, vec!["-", "pfx"]);
        let e = parse_split_args(&["--", "-l", "-x"]).unwrap();
        assert_eq!(e.operands, vec!["-l", "-x"]);
        // obsolete 在 `-l` 之后再出现被忽略（C 只 `if (numlines == 0)` 才解）。
        let f = parse_split_args(&["-l", "3", "-5"]).unwrap();
        assert_eq!(f.lines_per_chunk, 3);
    }

    #[test]
    fn parse_error_paths() {
        assert_eq!(
            parse_split_args(&["-a", "3", "f"]).err(),
            Some(SplitError::Unmodelled)
        );
        assert_eq!(
            parse_split_args(&["-b", "100"]).err(),
            Some(SplitError::Unmodelled)
        );
        assert_eq!(
            parse_split_args(&["-n", "4"]).err(),
            Some(SplitError::Unmodelled)
        );
        assert_eq!(
            parse_split_args(&["-z", "f"]).err(),
            Some(SplitError::Usage)
        ); // 集外
        assert_eq!(parse_split_args(&["-l"]).err(), Some(SplitError::Usage)); // 缺值
        assert_eq!(
            parse_split_args(&["-l0"]).err(),
            Some(SplitError::IllegalLineCount("0"))
        );
        assert_eq!(
            parse_split_args(&["-lx"]).err(),
            Some(SplitError::IllegalLineCount("x"))
        );
        assert_eq!(
            parse_split_args(&["-l", "3", "-l", "4"]).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&["-5", "-l", "3"]).err(),
            Some(SplitError::Usage)
        );
        assert_eq!(
            parse_split_args(&["a", "b", "c"]).err(),
            Some(SplitError::Usage)
        ); // >2 操数
    }
}
