//! `cat` cooked output (line numbering and blank-line squeezing).
//!
//! Ground truth: `minix3/bin/cat/cat.c` `cook_buf`. The cook path is taken
//! when a line-oriented flag (`-n`, `-b`, or `-s`) is set; it walks the input
//! byte by byte and, at the start of each line (`prev == '\n'`):
//!
//! - `-s` (squeeze): a blank line (a `'\n'` at line start) that immediately
//!   follows another blank line is dropped — the first blank of a run is kept,
//!   consecutive blanks are swallowed. A non-blank line resets the squeeze
//!   state so the next blank run starts fresh.
//! - `-n` (number): a `%6d\t` right-justified line number is emitted before the
//!   line. `-b` (number non-blank) suppresses the number on blank lines.
//!
//! The visible-character flags (`-e`, `-t`, `-v`, which turn control bytes into
//! `^X` / `M-x` and end lines with `$`) are the byte-visualization face and are
//! not modelled here; the caller rejects them. This module owns only the pure
//! line-numbering and squeeze decision; reading files and writing bytes to
//! standard output stay with the execution layer.

/// Number of columns the C `%6d` line-number field is right-justified into.
const LINE_FIELD_WIDTH: usize = 6;

/// Stateful transform for the `cat` cook path. Feed every input byte through
/// [`CatCook::push`]; it emits the cooked bytes through the caller's sink in
/// the same order they reach standard output.
#[derive(Debug, Clone, Copy)]
pub struct CatCook {
    number: bool,
    number_nonblank: bool,
    squeeze: bool,
    /// Previous byte; starts as a newline so the first byte is a line start.
    prev: u8,
    /// Next line number to emit (C pre-increments `++line`).
    line: u64,
    /// Whether the current blank run has already emitted its one newline.
    gobble: bool,
}

impl CatCook {
    /// Build a transform for the given flags. `-b` implies `-n` (C `cat.c:89`),
    /// so passing `number_nonblank` turns numbering on.
    pub fn new(number: bool, number_nonblank: bool, squeeze: bool) -> Self {
        CatCook {
            number: number || number_nonblank,
            number_nonblank,
            squeeze,
            prev: b'\n',
            line: 0,
            gobble: false,
        }
    }

    /// Feed one input byte, emitting cooked bytes through `emit`. Mirrors the
    /// `cook_buf` loop body for the numbering and squeeze flags.
    pub fn push(&mut self, ch: u8, emit: &mut dyn FnMut(u8)) {
        if self.prev == b'\n' {
            // At the start of a line: squeeze then numbering decisions.
            let mut dropped = false;
            if self.squeeze {
                if ch == b'\n' {
                    if self.gobble {
                        dropped = true;
                    } else {
                        self.gobble = true;
                    }
                } else {
                    self.gobble = false;
                }
            }
            if !dropped && self.number {
                // `-b` skips the number on a blank line.
                if !self.number_nonblank || ch != b'\n' {
                    self.line += 1;
                    emit_line_number(self.line, emit);
                }
            }
            if dropped {
                self.prev = ch;
                return;
            }
        }

        // The character itself. Control/tab/newline visualization is not part
        // of this subset, so every kept byte is written verbatim.
        emit(ch);
        self.prev = ch;
    }
}

/// Emit a right-justified `%6d`-style line number followed by a tab.
fn emit_line_number(line: u64, emit: &mut dyn FnMut(u8)) {
    let mut digits = [0u8; 20];
    let mut len = 0;
    let mut value = line;
    if value == 0 {
        digits[0] = b'0';
        len = 1;
    } else {
        while value > 0 {
            digits[len] = b'0' + (value % 10) as u8;
            value /= 10;
            len += 1;
        }
    }
    // Leading spaces pad out to the field width; a wider number just grows.
    let mut pad = LINE_FIELD_WIDTH.saturating_sub(len);
    while pad > 0 {
        emit(b' ');
        pad -= 1;
    }
    let mut index = len;
    while index > 0 {
        index -= 1;
        emit(digits[index]);
    }
    emit(b'\t');
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn run(bytes: &[u8], number: bool, number_nonblank: bool, squeeze: bool) -> Vec<u8> {
        let mut cook = CatCook::new(number, number_nonblank, squeeze);
        let mut out = Vec::new();
        for &b in bytes {
            cook.push(b, &mut |e| out.push(e));
        }
        out
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    #[test]
    fn test_plain_passthrough_without_flags() {
        let input = b"a\n\nb\n";
        assert_eq!(text(&run(input, false, false, false)), "a\n\nb\n");
    }

    #[test]
    fn test_number_every_line() {
        let out = run(b"ab\ncd\n", true, false, false);
        assert_eq!(text(&out), "     1\tab\n     2\tcd\n");
    }

    #[test]
    fn test_number_nonblank_skips_blank_lines() {
        // "a", blank, "b": with -b the blank line gets no number but the blank
        // newline is still emitted.
        let out = run(b"a\n\nb\n", false, true, false);
        assert_eq!(text(&out), "     1\ta\n\n     2\tb\n");
    }

    #[test]
    fn test_squeeze_collapses_consecutive_blanks() {
        // Two blank lines collapse to one; a single blank stays.
        let out = run(b"a\n\n\nb\n", false, false, true);
        assert_eq!(text(&out), "a\n\nb\n");
        // A lone blank between content is kept verbatim.
        let out2 = run(b"a\n\nb\n", false, false, true);
        assert_eq!(text(&out2), "a\n\nb\n");
    }

    #[test]
    fn test_squeeze_then_number_line_numbers_survive_squeeze() {
        // Combined -n -s: numbered lines, blanks squeezed. The blank run after
        // "a" is one newline kept (numbered 2), then squeezed.
        let out = run(b"a\n\n\nb\n", true, false, true);
        assert_eq!(text(&out), "     1\ta\n     2\t\n     3\tb\n");
    }

    #[test]
    fn test_no_trailing_newline_last_line_still_numbered() {
        // Input without a final newline: the last content line is still numbered.
        let out = run(b"x", true, false, false);
        assert_eq!(text(&out), "     1\tx");
    }

    #[test]
    fn test_line_number_field_widens_past_six_digits() {
        // Line 1234567 exceeds the 6-wide field: no truncation, no leading pad.
        let mut out = Vec::new();
        emit_line_number(1_234_567, &mut |b| out.push(b));
        assert_eq!(text(&out), "1234567\t");
    }
}
