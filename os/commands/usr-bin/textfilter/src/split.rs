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
        let chunks = chunks(&lines, 2);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[2], vec!["5".to_string()]);
        assert!(chunks(&[], 10).is_empty());
    }

    #[test]
    fn test_zero_count_is_no_chunks() {
        assert!(chunks(&["a"], 0).is_empty());
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
}
