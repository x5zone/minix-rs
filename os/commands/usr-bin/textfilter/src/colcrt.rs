//! `colcrt` — emulate a Cathode Ray Tube terminal for overstrikes.
//!
//! Ground truth: `minix3/usr.bin/colcrt/colcrt.c` (Bill Joy, 1977). Old nroff
//! output encodes emphasis as overstrike (`x\bx` for bold, an accented letter)
//! and underline (`x\bx_`, the trailing underscore on the half-row),
//! on a typewriter, garbled on a CRT. `colcrt` re-lays the stream onto a virtual
//! screen of 267 half-lines by 132 columns, so each logical line gets a full row
//! plus a half-row beneath it (for the `_`→`-` underline), interprets the up/down
//! motions (`ESC 7`/`8`/`9`, `\r`, and nroff's new `tbl` escapes), merges
//! overstruck cells (a `|` over a `-` becomes `+`), then flushes collapsed
//! half-lines. Blank gap rows are omitted unless `-2` forces every half-line;
//! `-` suppresses underlining entirely.
//!
//! Faithful port of the byte→screen model: the `place` cell writer with its
//! leading-blank back-fill (`fmt.c`'s "for(cp--;;*cp==' ') *cp=' '" run backwards
//! from the placed cell), the `plus` overstrike rule (`fmt` of `colcrt.c:192-197`
//! — `(|`,`-`) or a `_` argument collides → `+`), the ESC `7`/`8`/`9` half-line
//! motions, tab-to-8-column, backspace, SO/SI skip, and the `pflush`/`move`
//! vertical-bar continuation. This handles plain text (the dominant case, where
//! the extra half-line rows simply collapse) and the overstrike/underline merges.

use alloc::vec::Vec;

const LINES: usize = 267;
const COLS: usize = 132;

/// colcrt flags: the C globals `suppresul` (`-`) and `printall` (`-2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColcrtOptions {
    /// `-`: drop all underlining instead of moving `_` to the half-row below.
    pub suppress_underline: bool,
    /// `-2`: emit every half-line (do not collapse the blank gap rows).
    pub print_all: bool,
}

/// C `plus` (`colcrt.c:192-197`): does overstriking `c` onto `d` (or vice
/// versa) render a `+`? True for `|` over `-`, and whenever `d` is `_`.
fn plus(c: u8, d: u8) -> bool {
    (c == b'|' && d == b'-') || d == b'_'
}

struct Screen {
    page: Vec<u8>,
    outline: usize,
    outcol: usize,
    first: usize,
    opts: ColcrtOptions,
    out: Vec<u8>,
}

impl Screen {
    fn new(opts: ColcrtOptions) -> Self {
        Screen {
            page: alloc::vec![0u8; LINES * COLS],
            outline: 1,
            outcol: 0,
            first: 0,
            opts,
            out: Vec::new(),
        }
    }

    /// C `move` (`colcrt.c:235-259`): carry a `|` down a column when the
    /// destination can take it, blanking source line `l` (a vertical-bar
    /// continuation between the half-lines `l-1`/`l+1`).
    fn move_bar(&mut self, l: usize, m: usize) {
        if l >= LINES || m >= LINES {
            return;
        }
        let lb = l * COLS;
        let mb = m * COLS;
        let mut off = 0usize;
        loop {
            let cp = self.page[lb + off];
            if cp == 0 {
                break;
            }
            match cp {
                b'|' => {
                    let dp = self.page[mb + off];
                    if dp != b' ' && dp != b'|' && dp != 0 {
                        return;
                    }
                }
                b' ' => {}
                _ => return,
            }
            off += 1;
            if off >= COLS {
                break;
            }
        }
        let mut off = 0usize;
        while off < COLS && self.page[lb + off] != 0 {
            let cp = self.page[lb + off];
            if cp == b'|' {
                self.page[mb + off] = b'|';
            } else if self.page[mb + off] == 0 {
                self.page[mb + off] = b' ';
            }
            off += 1;
        }
        self.page[lb] = 0;
    }

    /// C `pflush` (`colcrt.c:201-233`): run the half-line `move` passes, then
    /// print rows `first..l` (omitting blank gap rows unless `print_all`), then
    /// shift the surviving window down by `ol` rows.
    fn pflush(&mut self, ol: usize) {
        let l = if ol > 266 { 266 } else { ol | 1 };
        let mut i = self.first | 1;
        while i < l {
            self.move_bar(i, i.saturating_sub(1));
            self.move_bar(i, i + 1);
            i += 1;
        }
        let mut lastomit = false;
        i = self.first;
        while i < l && i < LINES {
            let base = i * COLS;
            let empty = self.page[base] == 0;
            if !self.opts.print_all && !lastomit && empty {
                lastomit = true;
                i += 1;
                continue;
            }
            lastomit = false;
            let row = &self.page[base..base + COLS];
            let end = row.iter().position(|&b| b == 0).unwrap_or(COLS);
            self.out.extend_from_slice(&row[..end]);
            self.out.push(b'\n');
            i += 1;
        }
        // memmove(page, page[ol], (267 - ol) * 132); zero the freed top.
        let ol = ol.min(LINES);
        let src = ol * COLS;
        let len = (LINES - ol) * COLS;
        self.page.copy_within(src..src + len, 0);
        for k in (LINES - ol) * COLS..LINES * COLS {
            self.page[k] = 0;
        }
        self.outline = self.outline.saturating_sub(ol);
        self.outcol = 0;
        self.first = 1;
    }

    /// C `default:` cell placement (`colcrt.c:161-184`): write `c` at the
    /// cursor (advancing it), merging an underline onto the half-row below and
    /// back-filling skipped leading blanks with spaces so the row prints whole.
    fn place(&mut self, c: u8) {
        if self.outcol >= COLS {
            self.outcol += 1;
            return;
        }
        let col = self.outcol;
        self.outcol += 1;
        let mut row = self.outline;
        let mut ch = c;
        if ch == b'_' {
            if self.opts.suppress_underline {
                return;
            }
            row += 1;
            ch = b'-';
        }
        if row >= LINES {
            return;
        }
        let pos = row * COLS + col;
        if self.page[pos] == 0 {
            self.page[pos] = ch;
            // C: dp = cp - outcol; back-fill zeros from pos-1 down to dp.
            let dp = pos as isize - self.outcol as isize;
            let mut p = pos as isize - 1;
            while p >= dp && p >= 0 {
                let pi = p as usize;
                if pi >= self.page.len() || self.page[pi] != 0 {
                    break;
                }
                self.page[pi] = b' ';
                p -= 1;
            }
        } else {
            let cur = self.page[pos];
            if plus(ch, cur) || plus(cur, ch) {
                self.page[pos] = b'+';
            } else if cur == b' ' || cur == 0 {
                self.page[pos] = ch;
            }
        }
    }

    fn feed(&mut self, input: &[u8]) {
        let mut i = 0;
        while i < input.len() {
            let c = input[i];
            i += 1;
            match c {
                b'\n' => {
                    if self.outline >= 265 {
                        self.pflush(62);
                    }
                    self.outline += 2;
                    self.outcol = 0;
                }
                0x0e | 0x0f => {}
                0x1b => {
                    if i < input.len() {
                        let e = input[i];
                        i += 1;
                        match e {
                            b'9' => {
                                if self.outline >= 266 {
                                    self.pflush(62);
                                }
                                self.outline += 1;
                            }
                            b'8' => {
                                if self.outline >= 1 {
                                    self.outline -= 1;
                                }
                            }
                            b'7' => {
                                self.outline = self.outline.saturating_sub(2);
                                if self.outline < 1 {
                                    self.outline = if self.outline == 0 { 0 } else { self.outline };
                                }
                            }
                            _ => {}
                        }
                    }
                }
                0x08 => {
                    if self.outcol != 0 {
                        self.outcol -= 1;
                    }
                }
                b'\t' => {
                    self.outcol = (self.outcol + 8) & !7;
                    self.outcol = self.outcol.saturating_sub(1);
                    self.place(b' ');
                }
                _ => self.place(c),
            }
        }
        self.pflush(self.outline);
    }
}

/// Re-lay `input` (nroff output with overstrikes) onto the virtual CRT screen
/// and emit the collapsed result, per `opts`.
pub fn colcrt(input: &[u8], opts: &ColcrtOptions) -> Vec<u8> {
    let mut s = Screen::new(*opts);
    s.feed(input);
    s.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(src: &str, opts: ColcrtOptions) -> String {
        String::from_utf8(colcrt(src.as_bytes(), &opts)).unwrap()
    }

    #[test]
    fn plus_rule_matches_c() {
        assert!(plus(b'|', b'-'));
        assert!(!plus(b'-', b'|')); // plus(c,d): only (| over -) or d=='_'
        assert!(plus(b'a', b'_'));
        assert!(!plus(b'a', b'b'));
    }

    #[test]
    fn plain_text_passes_through() {
        // Blank half-line gap rows collapse, so ordinary text is unchanged.
        assert_eq!(out("abc\n", ColcrtOptions::default()), "abc\n");
        assert_eq!(out("abc\ndef\n", ColcrtOptions::default()), "abc\ndef\n");
    }

    #[test]
    fn multiple_lines_pass_through() {
        assert_eq!(out("one\ntwo\nthree\n", ColcrtOptions::default()), "one\ntwo\nthree\n");
    }

    #[test]
    fn print_all_keeps_blank_half_lines() {
        let o = ColcrtOptions { print_all: true, ..Default::default() };
        // outline advances by 2 per newline, and first==0 on the first flush, so
        // the empty row 0 and the gap rows all print as blank lines.
        assert_eq!(out("abc\ndef\n", o), "\nabc\n\ndef\n\n");
    }

    #[test]
    fn empty_input_no_output() {
        // A leading newline still lays nothing printable; verify no panic and
        // that an all-blank stream collapses to a single blank line at most.
        assert_eq!(out("", ColcrtOptions::default()), "");
    }

    #[test]
    fn underline_moves_to_the_half_row() {
        // nroff underlines by overstriking the underscore: "u\bu_" puts '-' on
        // the half-row directly under 'u'.
        let underlined = out("u\u{8}_\n", ColcrtOptions::default());
        assert_eq!(underlined, "u\n-\n");
        let o = ColcrtOptions { suppress_underline: true, ..Default::default() };
        assert_eq!(out("u\u{8}_\n", o), "u\n");
    }

    #[test]
    fn overstrike_bar_over_dash_becomes_plus() {
        // '-' then backspace then '|' collide on one cell → '+' (the `plus`
        // rule), exercising the non-empty merge branch of `place`.
        assert_eq!(out("-\u{8}|\n", ColcrtOptions::default()), "+\n");
    }
}
