//! `fmt` — reflow prose onto lines near a goal width.
//!
//! Ground truth: `minix3/usr.bin/fmt/fmt.c` (the NetBSD/BSD formatter). Its
//! `fmt()` loop collects a physical line doing `^H` (backspace) processing and
//! dropping non-printable bytes, expands tabs to 8-column stops, strips trailing
//! blanks, then hands the line to `prefix()` (`fmt.c:324-370`), which measures
//! the leading-blank indent and, on an indent change, flushes the current output
//! line. Body lines are broken into "words" by `split()` (`fmt.c:378-420`): each
//! word is a maximal non-blank run with its following blanks attached, and a
//! physical line end guarantees one space (two after `.`/`:`/`!` — the sentence
//! break). `pack()` (`fmt.c:445-475`) greedily appends a word when it still fits
//! the goal, or — under the BSD "closer to goal" rule — when the line would stay
//! within the max length *and* including the word lands nearer the goal than
//! excluding it. `oflush()`/`tabulate()` (`fmt.c:481-528`) strip the output
//! line's trailing blanks and re-encode its leading blanks back into tabs.
//!
//! Wired: the whole non-centered pipeline — `^H`/non-printable handling, tab
//! expansion, indent-flush, blank-line passthrough, the sentence double-space,
//! the goal/max greedy packer and the leading-blank→tab re-encoding — driven by
//! `FmtOptions { goal, max, raw }`. `raw` reproduces `fmt -r`: it drops the
//! special handling of an `.`-prefixed (nroff control) line, which the default
//! (`-r` absent) keeps on a line of its own.
//!
//! Declared residual: the *mail-header* heuristic. The default path also calls
//! `ishead()` (`minix3/usr.bin/mail/head.c:266`, an mbox `From `+RFC-822-date
//! probe that drags in the mail `parse`/`isdate` machinery) and a `To`/`Subject`
//!/`Cc` near-a-detected-head rule. Those are a different stage's dependency and
//! are not wired here, so a `From ` header line is reflowed as ordinary text.
//! For non-mail input `ishead` returns 0 and the `To/Subject/Cc` rule never
//! fires, so this formatter is byte-identical to the C for ordinary prose. The
//! `-C` center mode is likewise out of scope (rejected by the `fmt` bin).

use alloc::vec::Vec;

/// Formatting knobs (the C globals `goal_length`, `max_length`, `raw`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FmtOptions {
    /// Target output line length (C `GOAL_LENGTH`, 65).
    pub goal: usize,
    /// Maximum output line length (C `MAX_LENGTH`, 75).
    pub max: usize,
    /// Raw mode: do not keep an `.`-prefixed nroff line on its own (C `-r`).
    pub raw: bool,
}

impl Default for FmtOptions {
    fn default() -> Self {
        FmtOptions { goal: 65, max: 75, raw: false }
    }
}

/// Is `c` a byte the C collector keeps on its way to the line buffer? It is
/// either a tab (expanded later) or a printable — `isprint` in the C locale
/// spans `0x20..=0x7e` — or a high-bit byte (`c >= 160`, treated as printable).
fn keep_byte(c: u8) -> bool {
    c == b'\t' || (0x20..=0x7e).contains(&c) || c >= 160
}

/// The running formatter state that C keeps in file-scope globals (`pfx`,
/// `outbuf`, the output stream).
struct Formatter {
    goal: usize,
    max: usize,
    raw: bool,
    pfx: usize,
    /// The output line under construction (C `outbuf`).
    pending: Vec<u8>,
    /// The finished output stream.
    out: Vec<u8>,
}

impl Formatter {
    fn new(opts: &FmtOptions) -> Self {
        Formatter { goal: opts.goal, max: opts.max, raw: opts.raw, pfx: 0, pending: Vec::new(), out: Vec::new() }
    }

    /// C `leadin` (`fmt.c:534-543`): start a fresh output line with `pfx` blanks.
    fn leadin(&mut self) {
        self.pending.clear();
        for _ in 0..self.pfx {
            self.pending.push(b' ');
        }
    }

    /// C `oflush` + `tabulate` (`fmt.c:481-528`): send the current line out with
    /// trailing blanks tossed and leading blanks re-encoded as tabs.
    fn oflush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        // Toss trailing blanks.
        let body_end = self.pending.iter().rposition(|&b| b != b' ').map(|p| p + 1).unwrap_or(0);
        let body = &self.pending[..body_end];
        // Count leading blank space and tabulate.
        let lead = body.iter().take_while(|&&b| b == b' ').count();
        let rest = &body[lead..];
        for _ in 0..lead / 8 {
            self.out.push(b'\t');
        }
        for _ in 0..lead % 8 {
            self.out.push(b' ');
        }
        self.out.extend_from_slice(rest);
        self.out.push(b'\n');
        self.pending.clear();
    }

    /// C `pack` (`fmt.c:445-475`): append a word (its following blanks already
    /// joined onto `token`) to the current line, flushing and starting a new one
    /// when the BSD goal/max rule rejects it.
    fn pack(&mut self, token: &[u8], wlen: usize) {
        if self.pending.is_empty() {
            self.leadin();
        }
        let s = self.pending.len();
        let t = wlen + s;
        let take = t <= self.goal
            || (t <= self.max && s <= self.goal && t.saturating_sub(self.goal) <= self.goal.saturating_sub(s));
        if take {
            self.pending.extend_from_slice(token);
            return;
        }
        if s > self.pfx {
            self.oflush();
            self.leadin();
        }
        self.pending.extend_from_slice(token);
    }

    /// C `split` (`fmt.c:378-420`): break a line into words with their trailing
    /// blanks attached, guaranteeing a separator (two after sentence punctuation)
    /// where the physical line ended and more input follows.
    fn split(&mut self, line: &[u8], add_space: bool) {
        let mut i = 0;
        while i < line.len() {
            let mut token = Vec::new();
            let mut wlen = 0usize;
            // Collect a word, letting `\<space>` escape the blank.
            while i < line.len() && line[i] != b' ' {
                if line[i] == b'\\' && i + 1 < line.len() && line[i + 1] == b' ' {
                    token.push(line[i]);
                    i += 1;
                }
                token.push(line[i]);
                i += 1;
                wlen += 1;
            }
            // End of the physical line: guarantee a space, two after .:!
            if i == line.len() && add_space {
                token.push(b' ');
                if line.last().is_some_and(|&last| last == b'.' || last == b':' || last == b'!') {
                    token.push(b' ');
                }
            }
            // Attach the run of blanks that separated this word from the next.
            while i < line.len() && line[i] == b' ' {
                token.push(line[i]);
                i += 1;
            }
            self.pack(&token, wlen);
        }
    }

    /// C `prefix` (`fmt.c:324-370`): classify a cleaned line by its leading
    /// blanks, flush the current output line on an indent change or before an
    /// nroff `.` control line, then feed the body to `split`/`pack`.
    fn prefix(&mut self, line: &[u8], add_space: bool) {
        if line.is_empty() {
            self.oflush();
            self.out.push(b'\n');
            return;
        }
        let np = line.iter().take_while(|&&b| b == b' ').count();
        let cp = &line[np..];
        // "The following horrible expression attempts to avoid linebreaks when
        // the indent changes due to a paragraph."
        if np != self.pfx && (np > self.pfx || self.pfx.abs_diff(np) > 8) {
            self.oflush();
        }
        let h = !self.raw && cp[0] == b'.';
        self.pfx = np;
        if h {
            self.pack(cp, cp.len());
            self.oflush();
        } else {
            self.split(cp, add_space);
        }
    }

    /// Expand tabs to 8-column stops (C `fmt`'s tab swipe, `fmt.c:286-299`).
    fn expand_tabs(&self, line: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut col = 0usize;
        for &c in line {
            if c != b'\t' {
                out.push(c);
                col += 1;
            } else {
                loop {
                    out.push(b' ');
                    col += 1;
                    if col.is_multiple_of(8) {
                        break;
                    }
                }
            }
        }
        out
    }

    /// Drive one input byte stream the way C's `fmt(FILE*)` does: collect a line
    /// (backspace and non-printable processing), expand tabs, strip trailing
    /// blanks, and hand it to `prefix`. A line that ended on a newline (rather
    /// than the file's end) carries `add_space`.
    fn run(&mut self, input: &[u8]) {
        // C reads the first byte before the loop (`c = getc(fi)`); an empty
        // stream yields EOF immediately and the loop never runs, so no line —
        // and critically no phantom trailing blank line — is processed.
        if input.is_empty() {
            return;
        }
        let mut i = 0;
        loop {
            let mut raw = Vec::new();
            while i < input.len() && input[i] != b'\n' {
                let c = input[i];
                i += 1;
                if c == 0x08 {
                    raw.pop();
                    continue;
                }
                if keep_byte(c) {
                    raw.push(c);
                }
            }
            // The line ended on a newline (`add_space`) or on the file's end.
            let add_space = i < input.len();
            if add_space {
                i += 1; // consume the newline
            }
            let expanded = self.expand_tabs(&raw);
            let end = expanded.iter().rposition(|&b| b != b' ').map(|p| p + 1).unwrap_or(0);
            self.prefix(&expanded[..end], add_space);
            // C re-reads the next byte; on EOF the outer `while` exits without
            // forming another (empty) line.
            if i >= input.len() {
                break;
            }
        }
        self.oflush();
    }
}

/// Reflow `input` into formatted output bytes, faithful to `fmt.c`'s non-center
/// path for the given `opts`.
pub fn fmt_bytes(input: &[u8], opts: &FmtOptions) -> Vec<u8> {
    let mut f = Formatter::new(opts);
    f.run(input);
    f.out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str, opts: &FmtOptions) -> String {
        String::from_utf8(fmt_bytes(src.as_bytes(), opts)).unwrap()
    }

    #[test]
    fn short_line_fits() {
        assert_eq!(fmt("hello world\n", &FmtOptions::default()), "hello world\n");
    }

    #[test]
    fn blank_line_passes_through() {
        assert_eq!(fmt("a\n\nb\n", &FmtOptions::default()), "a\n\nb\n");
    }

    #[test]
    fn greedy_goal_max_break() {
        // goal 10, max 15 over words aa bb cc dd ee ff (see the pack walkthrough).
        let o = FmtOptions { goal: 10, max: 15, raw: false };
        assert_eq!(fmt("aa bb cc dd ee ff\n", &o), "aa bb cc dd\nee ff\n");
    }

    #[test]
    fn sentence_double_space_preserved() {
        // The line-ending period earns a second space, kept when both fit.
        assert_eq!(fmt("hi there.\nnext line\n", &FmtOptions::default()), "hi there.  next line\n");
    }

    #[test]
    fn tabs_expand_to_spaces() {
        assert_eq!(fmt("x\ty\n", &FmtOptions::default()), "x       y\n");
    }

    #[test]
    fn backspace_deletes_preceding() {
        assert_eq!(fmt("ab\u{8}c\n", &FmtOptions::default()), "ac\n");
    }

    #[test]
    fn non_printable_dropped() {
        assert_eq!(fmt("a\x01b\n", &FmtOptions::default()), "ab\n");
    }

    #[test]
    fn leading_blank_reencodes_as_tab() {
        // 8 leading spaces survive the round trip as one tab (tabulate).
        let o = FmtOptions { goal: 30, max: 40, raw: false };
        assert_eq!(fmt("        deep text here\n", &o), "\tdeep text here\n");
    }

    #[test]
    fn nroff_dot_line_kept_alone() {
        assert_eq!(fmt(".x y\nz\n", &FmtOptions::default()), ".x y\nz\n");
    }

    #[test]
    fn raw_disables_dot_handling() {
        let o = FmtOptions { goal: 65, max: 75, raw: true };
        assert_eq!(fmt(".x y\nz\n", &o), ".x y z\n");
    }

    #[test]
    fn empty_input_yields_no_output() {
        // C reads the first byte before the loop; an empty stream emits nothing.
        assert_eq!(fmt("", &FmtOptions::default()), "");
    }

    #[test]
    fn increase_indent_flushes_paragraph() {
        // A deeper indent (0 -> 2) forces a line break; the flush check in
        // `prefix` (`fmt.c:345`) is `np > pfx`.
        assert_eq!(fmt("aaa\n  bbb\n", &FmtOptions::default()), "aaa\n  bbb\n");
    }

    #[test]
    fn escaped_space_is_not_a_break() {
        // `\<space>` keeps the backslash and glues the blank into the word, so
        // "a\ b" stays on one line even past the goal; the next word wraps.
        let o = FmtOptions { goal: 3, max: 6, raw: false };
        assert_eq!(fmt("a\\ b c\n", &o), "a\\ b\nc\n");
    }
}
