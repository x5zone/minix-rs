//! Line reversal for `rev`.
//!
//! Ground truth: `minix3/usr.bin/rev/rev.c` (NetBSD), main at lines 57-95:
//! each line's characters print in reverse order with a newline, files
//! (operands) fall back to stdin, and a read or open failure sets status
//! 1. The C reverses wide characters; this slice reverses Rust `char`s —
//! the same character-level guarantee, byte-safe through UTF-8.

use alloc::string::String;
use alloc::vec::Vec;

/// Reverses every line of the collected input (lines split on `\n`, the
/// final newline dropped), returning one reversed line per input line.
pub fn reversed_lines(input: &[u8]) -> Vec<String> {
    reversed_lines_of(split_lines(input))
}

/// Reverses already-split lines.
pub fn reversed_lines_of(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| line.chars().rev().collect())
        .collect()
}

fn split_lines(input: &[u8]) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = alloc::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lines_reverse_character_order() {
        assert_eq!(reversed_lines(b"abc\ndef\n"), vec!["cba", "fed"]);
    }

    #[test]
    fn test_unterminated_final_line_still_reverses() {
        assert_eq!(reversed_lines(b"abc\ndef"), vec!["cba", "fed"]);
    }

    #[test]
    fn test_multibyte_characters_stay_intact() {
        // Character-level reversal: a two-byte character does not split.
        assert_eq!(reversed_lines("éab\n".as_bytes()), vec!["baé"]);
    }

    #[test]
    fn test_empty_input_yields_no_lines() {
        assert!(reversed_lines(b"").is_empty());
    }
}
