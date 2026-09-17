//! Column removal for `colrm`.
//!
//! (Module note: `Vec` here comes from `alloc` via the crate root.)
//!
//! Ground truth: `minix3/usr.bin/colrm/colrm.c` (NetBSD), main at lines
//! 61-121: `colrm [start [stop]]`. Every input byte advances a display
//! column — printable bytes add one, a tab rounds up to the next
//! multiple of eight (`TAB`, line 55), a backspace pulls one back
//! (never below zero), a newline resets — and the byte survives when
//! the *new* column lands outside `[start, stop]`. With only `start`,
//! columns from `start` onward are removed; with no operands
//! everything passes through. Column one is the first byte of the line.

/// Removes columns `[start, stop]` (1 based; `stop` zero means through
/// end of line) from the input, mirroring the C byte walk: the display
/// column tracks tabs (next multiple of 8), backspaces (one back, not
/// below zero), and newlines (reset), and a byte is kept exactly when
/// its new column is outside the removed range.
use alloc::vec::Vec;

pub fn filter_column(input: &[u8], start: usize, stop: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut column = 0usize;
    for &byte in input {
        let new_column = match byte {
            0x08 => column.saturating_sub(1),
            b'\n' => 0,
            b'\t' => (column + 8) & !7,
            _ => column + 1,
        };
        let keep = start == 0
            || new_column < start
            || (stop != 0 && new_column > stop);
        if keep {
            out.push(byte);
        }
        column = new_column;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_removal() {
        // Columns 2-4 (b, c, d) drop; a, e, f survive.
        assert_eq!(filter_column(b"abcdef", 2, 4), b"aef");
    }

    #[test]
    fn test_start_only_removes_to_end_of_line() {
        // colrm 2 — no stop means "to end of line": stop stays zero.
        assert_eq!(filter_column(b"abcdef", 2, 0), b"a");
    }

    #[test]
    fn test_no_operands_passes_everything_through() {
        assert_eq!(filter_column(b"abc\tdef\n", 0, 0), b"abc\tdef\n");
    }

    #[test]
    fn test_tab_quantizes_the_column() {
        // 'a' sits at column 1; the tab itself rounds the display column
        // to 8 (inside the removed range 2..8) and is dropped, while 'b'
        // lands at column 9 and survives.
        assert_eq!(filter_column(b"a\tb", 2, 8), b"ab");
    }

    #[test]
    fn test_backspace_pulls_the_column_back() {
        // ab at columns 1-2, two backspaces pull the column back to 0,
        // then X lands at column 1: only b is in the removed range 2...
        assert_eq!(filter_column(b"ab\x08\x08X", 2, 0), b"a\x08\x08X");
    }

    #[test]
    fn test_newline_resets_the_column() {
        // The newline itself is column zero and survives; line two's
        // columns restart at one, so only d (column two) is removed.
        assert_eq!(filter_column(b"ab\ncd", 2, 3), b"a\nc");
    }

    #[test]
    fn test_column_one_boundary_keeps_nothing_in_range() {
        assert_eq!(filter_column(b"abc", 1, 1), b"bc");
    }

    #[test]
    fn test_empty_input_yields_empty_output() {
        assert!(filter_column(b"", 1, 4).is_empty());
    }
}
