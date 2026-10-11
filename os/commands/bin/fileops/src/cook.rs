//! `cat` cooked output (line numbering, blank-line squeezing, visible characters).
//!
//! Ground truth: `minix3/bin/cat/cat.c` `cook_buf`. The cook path is taken
//! when a line-oriented flag (`-n`, `-b`, `-s`) or a visible-character flag
//! (`-e`, `-t`, `-v`) is set; it walks the input byte by byte and, at the
//! start of each line (`prev == '\n'`):
//!
//! - `-s` (squeeze): a blank line (a `'\n'` at line start) that immediately
//!   follows another blank line is dropped — the first blank of a run is kept,
//!   consecutive blanks are swallowed. A non-blank line resets the squeeze
//!   state so the next blank run starts fresh.
//! - `-n` (number): a `%6d\t` right-justified line number is emitted before the
//!   line. `-b` (number non-blank) suppresses the number on blank lines; under
//!   `-e` the suppressed slot still prints a six-space field then a tab (C's
//!   `fprintf("%6s\t", "")`).
//!
//! Then the byte itself is rendered:
//! - `'\n'` — under `-e`, a `$` is written first, then the newline.
//! - `'\t'` — under `-t`, written as `^I` (and the raw tab is not written).
//! - any other byte under `-v`: a high byte becomes `M-` followed by its
//!   low-ASCII form; a control byte (including the just-stripped one) becomes
//!   `^` plus `ch | 0x40` (`^?` for 0x7f). A byte that is neither control nor
//!   high after the `M-` prefix is written verbatim.
//!
//! `-e` and `-t` each imply `-v` (C `cat.c:92,107`), so any `$` or `^I` also
//! turns on the control/high rendering. This module owns the pure line and byte
//! decision; reading files and writing bytes to standard output stay with the
//! execution layer. The `-B`/`-l`/`-f` flags (no-op / line-buffered / `rf` open)
//! are handled by the caller, not here.

/// The line-oriented and visible-character flags of the `cat` cook path.
#[derive(Debug, Clone, Copy, Default)]
pub struct CookFlags {
    /// `-n`: number every line.
    pub number: bool,
    /// `-b`: number only non-blank lines (implies `number`).
    pub number_nonblank: bool,
    /// `-s`: squeeze consecutive blank lines into one.
    pub squeeze: bool,
    /// `-e`: end each line with `$` (implies `visible`).
    pub dollar: bool,
    /// `-t`: show tabs as `^I` (implies `visible`).
    pub show_tabs: bool,
    /// `-v`: render non-printing bytes as `^X` / `M-x`.
    pub visible: bool,
}

/// Number of columns the C `%6d` line-number field is right-justified into.
const LINE_FIELD_WIDTH: usize = 6;

/// DEL (`0x7f`), rendered `^?` rather than `^` plus `DEL | 0x40`.
const DEL: u8 = 0x7f;

/// Stateful transform for the `cat` cook path. Feed every input byte through
/// [`CatCook::push`]; it emits the cooked bytes through the caller's sink in
/// the order they reach standard output.
#[derive(Debug, Clone, Copy)]
pub struct CatCook {
    flags: CookFlags,
    /// Previous byte; starts as a newline so the first byte is a line start.
    prev: u8,
    /// Next line number to emit (C pre-increments `++line`).
    line: u64,
    /// Whether the current blank run has already emitted its one newline.
    gobble: bool,
}

impl CatCook {
    /// Build a transform. `-b` implies `-n` and `-e`/`-t` each imply `-v`, so
    /// those are folded here rather than at every call site.
    pub fn new(flags: CookFlags) -> Self {
        let number = flags.number || flags.number_nonblank;
        let visible = flags.visible || flags.dollar || flags.show_tabs;
        CatCook {
            flags: CookFlags {
                number,
                number_nonblank: flags.number_nonblank,
                squeeze: flags.squeeze,
                dollar: flags.dollar,
                show_tabs: flags.show_tabs,
                visible,
            },
            prev: b'\n',
            line: 0,
            gobble: false,
        }
    }

    /// Feed one input byte, emitting cooked bytes through `emit`. Mirrors the
    /// `cook_buf` loop body for the numbering, squeeze, and visible flags.
    pub fn push(&mut self, ch: u8, emit: &mut dyn FnMut(u8)) {
        if self.prev == b'\n' {
            // At the start of a line: squeeze then numbering decisions.
            let mut dropped = false;
            if self.flags.squeeze {
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
            if !dropped && self.flags.number {
                if !self.flags.number_nonblank || ch != b'\n' {
                    self.line += 1;
                    emit_line_number(self.line, emit);
                } else if self.flags.dollar {
                    // `-be`: a blank line keeps the number column as blanks.
                    emit_blank_field(emit);
                }
            }
            if dropped {
                self.prev = ch;
                return;
            }
        }

        self.render(ch, emit);
        self.prev = ch;
    }

    /// Write one byte per the `cook_buf` char branches (line-end `$`, tab `^I`,
    /// control/high `^X`/`M-x`, otherwise verbatim).
    fn render(&self, ch: u8, emit: &mut dyn FnMut(u8)) {
        if ch == b'\n' {
            if self.flags.dollar {
                emit(b'$');
            }
            emit(b'\n');
            return;
        }
        if ch == b'\t' {
            if self.flags.show_tabs {
                emit(b'^');
                emit(b'I');
                return;
            }
            emit(ch);
            return;
        }
        if self.flags.visible {
            let mut byte = ch;
            if byte >= 0x80 {
                emit(b'M');
                emit(b'-');
                byte &= 0x7f; // C toascii
            }
            if byte < 0x20 || byte == DEL {
                emit(b'^');
                emit(if byte == DEL { b'?' } else { byte | 0x40 });
                return;
            }
            emit(byte);
            return;
        }
        emit(ch);
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

/// Emit the `%6s\t`-of-an-empty-string field: six spaces then a tab.
fn emit_blank_field(emit: &mut dyn FnMut(u8)) {
    for _ in 0..LINE_FIELD_WIDTH {
        emit(b' ');
    }
    emit(b'\t');
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec::Vec;

    fn run(bytes: &[u8], flags: CookFlags) -> Vec<u8> {
        let mut cook = CatCook::new(flags);
        let mut out = Vec::new();
        for &b in bytes {
            cook.push(b, &mut |e| out.push(e));
        }
        out
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    const fn no_flags() -> CookFlags {
        CookFlags {
            number: false,
            number_nonblank: false,
            squeeze: false,
            dollar: false,
            show_tabs: false,
            visible: false,
        }
    }

    #[test]
    fn test_plain_passthrough_without_flags() {
        assert_eq!(text(&run(b"a\n\nb\n", no_flags())), "a\n\nb\n");
    }

    #[test]
    fn test_number_every_line() {
        let flags = CookFlags {
            number: true,
            ..no_flags()
        };
        assert_eq!(text(&run(b"ab\ncd\n", flags)), "     1\tab\n     2\tcd\n");
    }

    #[test]
    fn test_number_nonblank_skips_blank_lines() {
        let flags = CookFlags {
            number_nonblank: true,
            ..no_flags()
        };
        assert_eq!(text(&run(b"a\n\nb\n", flags)), "     1\ta\n\n     2\tb\n");
    }

    #[test]
    fn test_squeeze_collapses_consecutive_blanks() {
        let flags = CookFlags {
            squeeze: true,
            ..no_flags()
        };
        assert_eq!(text(&run(b"a\n\n\nb\n", flags)), "a\n\nb\n");
        assert_eq!(text(&run(b"a\n\nb\n", flags)), "a\n\nb\n");
    }

    #[test]
    fn test_squeeze_then_number_line_numbers_survive_squeeze() {
        let flags = CookFlags {
            number: true,
            squeeze: true,
            ..no_flags()
        };
        assert_eq!(
            text(&run(b"a\n\n\nb\n", flags)),
            "     1\ta\n     2\t\n     3\tb\n"
        );
    }

    #[test]
    fn test_no_trailing_newline_last_line_still_numbered() {
        let flags = CookFlags {
            number: true,
            ..no_flags()
        };
        assert_eq!(text(&run(b"x", flags)), "     1\tx");
    }

    #[test]
    fn test_line_number_field_widens_past_six_digits() {
        let mut out = Vec::new();
        emit_line_number(1_234_567, &mut |b| out.push(b));
        assert_eq!(text(&out), "1234567\t");
    }

    #[test]
    fn test_dollar_marks_line_end_and_implies_visible() {
        // -e: each newline gains a leading '$'.
        let flags = CookFlags {
            dollar: true,
            ..no_flags()
        };
        assert_eq!(text(&run(b"ab\ncd\n", flags)), "ab$\ncd$\n");
    }

    #[test]
    fn test_show_tabs_renders_caret_upper_i() {
        let flags = CookFlags {
            show_tabs: true,
            ..no_flags()
        };
        assert_eq!(text(&run(b"a\tb\n", flags)), "a^Ib\n");
    }

    #[test]
    fn test_visible_control_and_high_bytes() {
        let flags = CookFlags {
            visible: true,
            ..no_flags()
        };
        // ESC (0x1b) -> ^[, DEL (0x7f) -> ^?, 0x80 -> M-<0x00=^@>, 0xE1 -> M-a.
        let out = run(&[0x1b, 0x7f, b'\n', 0x80, b'\n', 0xe1], flags);
        assert_eq!(text(&out), "^[^?\nM-^@\nM-a");
    }

    #[test]
    fn test_be_blank_line_keeps_blank_number_field() {
        // -b -e: blank line has no number but a 6-space field then tab, then $ .
        let flags = CookFlags {
            number_nonblank: true,
            dollar: true,
            ..no_flags()
        };
        let out = run(b"a\n\nb\n", flags);
        assert_eq!(text(&out), "     1\ta$\n      \t$\n     2\tb$\n");
    }
}
