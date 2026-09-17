//! diff — differential file comparison (minix3/minix/usr.bin/diff/diffreg.c).
//!
//! Deciding half: the 4.4BSD two-file differ over in-memory line
//! vectors. The pipeline mirrors the C exactly — line values (dense
//! equivalence classes standing in for the Sedgewick byte hash, which
//! also removes the `jackpot` re-verification path), common
//! prefix/suffix pruning, sort+equiv+unsort to build class/member,
//! the stone() candidate search with its isqrt try bound, unravel()
//! into the J match table, then the output walk feeding change() per
//! hunk. Output formats: normal, ed script (`-e`, reverse walk with
//! the bare-dot escape), forward ed (`-f`), RCS (`-n`), brief (`-q`)
//! and unified (`-u`) with context merging. The `-c` context format,
//! `#ifdef` output, ignore flags (`-b/-w/-i`), `\ No newline at end
//! of file` (needs the file-end fact from the doing half) and
//! directory recursion are registered corners.

use crate::floatfmt::parse_float_prefix;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::{vec, vec::Vec};

/// Output shapes (the C's diff_format values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFormat {
    /// Default `XcY` / `<` / `---` / `>` script.
    Normal,
    /// `-e`: ed script, reverse walk, raw lines, dot escaping.
    Ed,
    /// `-f`: forward ed (D_REVERSE).
    ForwardEd,
    /// `-n`: RCS form (`dR N` / `aR N`).
    Rcs,
    /// `-q`: report only, no hunks.
    Brief,
    /// `-u`: unified hunks with `@@` headers.
    Unified,
}

#[derive(Debug, Clone)]
pub struct DiffOptions {
    pub format: DiffFormat,
    /// Context lines around unified hunks (the C's diff_context).
    pub context: usize,
    /// `-d`-adjacent minimal search bound (D_MINIMAL: unbounded tries).
    pub minimal: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        DiffOptions { format: DiffFormat::Normal, context: 3, minimal: false }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DiffStatus {
    Same,
    Differ,
}

#[derive(Debug, PartialEq)]
pub struct DiffResult {
    pub status: DiffStatus,
    /// The rendered script (empty when identical or brief).
    pub text: String,
}

#[derive(Clone, Copy, Default)]
struct LineRec {
    value: u32,
    serial: usize,
}

#[derive(Clone, Copy, Default)]
struct Cand {
    x: usize,
    y: i64,
    pred: usize,
}

#[derive(Clone, Copy)]
struct ContextVec {
    a: usize,
    b: usize,
    c: usize,
    d: usize,
}

struct Engine<'a> {
    a: &'a [&'a str],
    b: &'a [&'a str],
    len: [usize; 2],
    pref: usize,
    suff: usize,
    j: Vec<usize>,
    options: DiffOptions,
}

/// Diffs two line vectors. Labels appear in unified headers.
pub fn diff_lines(a: &[&str], b: &[&str], label_a: &str, label_b: &str, options: &DiffOptions) -> DiffResult {
    // Dense equivalence classes: every distinct line gets a value.
    // (The C hashes bytes and later re-verifies matches in check();
    // exact classes make that pass unnecessary.)
    let mut ids: Vec<&str> = Vec::new();
    fn value_of<'a>(ids: &mut Vec<&'a str>, line: &'a str) -> u32 {
        if let Some(pos) = ids.iter().position(|l| *l == line) {
            return pos as u32 + 1;
        }
        ids.push(line);
        ids.len() as u32
    }
    let mut va: Vec<u32> = Vec::new();
    let mut vb: Vec<u32> = Vec::new();
    for l in a {
        va.push(value_of(&mut ids, l));
    }
    for l in b {
        vb.push(value_of(&mut ids, l));
    }

    let len = [a.len(), b.len()];
    let mut engine = Engine { a, b, len, pref: 0, suff: 0, j: Vec::new(), options: options.clone() };

    // prune(): common prefix and suffix.
    while engine.pref < len[0]
        && engine.pref < len[1]
        && va[engine.pref] == vb[engine.pref]
    {
        engine.pref += 1;
    }
    while engine.suff < len[0] - engine.pref
        && engine.suff < len[1] - engine.pref
        && va[len[0] - engine.suff - 1] == vb[len[1] - engine.suff - 1]
    {
        engine.suff += 1;
    }
    let slen = [
        len[0] - engine.pref - engine.suff,
        len[1] - engine.pref - engine.suff,
    ];

    // The middle sections with 1-based serials (C convention).
    let mut s: [Vec<LineRec>; 2] = [Vec::new(), Vec::new()];
    for side in 0..2 {
        let values = if side == 0 { &va } else { &vb };
        let base = engine.pref;
        let mut recs: Vec<LineRec> = Vec::new();
        recs.push(LineRec::default());
        for i in 0..slen[side] {
            recs.push(LineRec { value: values[base + i], serial: i + 1 });
        }
        s[side] = recs;
    }
    // sort(): by value, then serial.
    for side in 0..2 {
        s[side][1..].sort_by_key(|r| (r.value, r.serial));
    }

    // equiv(): class[serial-in-s0] = first matching position in sorted
    // s1 (0 = unmatched).
    let mut class: Vec<i64> = vec![0; slen[0] + 2];
    {
        let (s0, s1) = (&s[0], &s[1]);
        let (mut i, mut j) = (1usize, 1usize);
        while i <= slen[0] && j <= slen[1] {
            if s0[i].value < s1[j].value {
                class[s0[i].serial] = 0;
                i += 1;
            } else if s0[i].value == s1[j].value {
                class[s0[i].serial] = j as i64;
                i += 1;
            } else {
                j += 1;
            }
        }
        while i <= slen[0] {
            class[s0[i].serial] = 0;
            i += 1;
        }
    }
    // member: run heads carry negative serials (the candidate line),
    // continuations positive (skipped by stone's loop guard).
    let mut member: Vec<i64> = vec![0; slen[1] + 2];
    {
        let s1 = &s[1];
        let mut j = 1usize;
        while j <= slen[1] {
            member[j] = -(s1[j].serial as i64);
            let mut k = j + 1;
            while k <= slen[1] && s1[k].value == s1[j].value {
                member[k] = s1[k].serial as i64;
                k += 1;
            }
            j = k;
        }
        member[slen[1] + 1] = -1;
    }

    // stone(): the candidate search.
    let mut clist: Vec<Cand> = Vec::new();
    let mut klist: Vec<usize> = vec![0; slen[0] + 2];
    let bound = if options.minimal {
        usize::MAX
    } else {
        let n = slen[0];
        core::cmp::max(256, isqrt(n))
    };
    let mut k: usize = 0;
    clist.push(Cand { x: 0, y: 0, pred: 0 });
    for i in 1..=slen[0] {
        let jj = class[i];
        if jj == 0 {
            continue;
        }
        let mut j = jj as usize;
        let mut y = -member[j];
        let mut oldl = 0usize;
        let mut oldc = 0usize;
        let mut numtries = 0usize;
        loop {
            if y <= clist[oldc].y {
                // continue in the do-while
            } else {
                let l = search(&clist, &klist, k, y);
                if l != oldl + 1 {
                    oldc = klist[l - 1];
                }
                if l <= k {
                    if clist[klist[l]].y <= y {
                        // try next occurrence
                    } else {
                        let tc = klist[l];
                        klist[l] = clist.len();
                        clist.push(Cand { x: i, y, pred: oldc });
                        oldc = tc;
                        oldl = l;
                        numtries += 1;
                    }
                } else {
                    klist[l] = clist.len();
                    clist.push(Cand { x: i, y, pred: oldc });
                    k += 1;
                    break;
                }
            }
            j += 1;
            y = -member[j];
            if y <= 0 || numtries >= bound {
                break;
            }
        }
    }

    // unravel(): J over the full file lengths.
    let len0 = len[0];
    let len1 = len[1];
    let mut jtable: Vec<usize> = vec![0; len0 + 2];
    for i in 0..=len0 {
        jtable[i] = if i <= engine.pref {
            i
        } else if i > len0 - engine.suff {
            i + len1 - len0
        } else {
            0
        };
    }
    let mut p = klist[k];
    while clist[p].y != 0 {
        jtable[clist[p].x + engine.pref] = clist[p].y as usize + engine.pref;
        p = clist[p].pred;
    }
    // check()'s core duty (diffreg.c:728-830): every matched pair is
    // re-verified against the actual lines. The C needs this because
    // hashing can confound and the member sentinel can offer a
    // spurious y=1 candidate; line equality makes the check exact.
    for i in engine.pref + 1..=len0 - engine.suff {
        let jj = jtable[i];
        if jj != 0 {
            let matched = a[i - 1] == b[jj - 1];
            if !matched {
                jtable[i] = 0;
            }
        }
    }
    engine.j = jtable;

    // output(): walk the runs and render.
    let mut out = String::new();
    let mut anychange = false;
    let mut ctx: Vec<ContextVec> = Vec::new();
    if options.format != DiffFormat::Ed {
        let m = len0;
        engine.j[0] = 0;
        engine.j[m + 1] = len1 + 1;
        let mut i0 = 1usize;
        while i0 <= m {
            while i0 <= m && engine.j[i0] == engine.j[i0 - 1] + 1 {
                i0 += 1;
            }
            let j0 = engine.j[i0 - 1] + 1;
            let mut i1 = i0 - 1;
            while i1 < m && engine.j[i1 + 1] == 0 {
                i1 += 1;
            }
            let j1 = engine.j[i1 + 1] - 1;
            engine.j[i1] = j1;
            engine.change(i0, i1, j0, j1, &mut out, &mut anychange, &mut ctx, label_a, label_b);
            i0 = i1 + 1;
        }
    } else {
        // D_EDIT: reverse walk so earlier commands do not shift the
        // line numbers of later ones.
        let m = len0;
        engine.j[0] = 0;
        engine.j[m + 1] = len1 + 1;
        let mut i0 = m;
        while i0 >= 1 {
            while i0 >= 1 && engine.j[i0] == engine.j[i0 + 1] - 1 && engine.j[i0] != 0 {
                i0 -= 1;
            }
            let j0 = engine.j[i0 + 1] - 1;
            let mut i1 = i0 + 1;
            while i1 > 1 && engine.j[i1 - 1] == 0 {
                i1 -= 1;
            }
            let j1 = engine.j[i1 - 1] + 1;
            engine.j[i1] = j1;
            engine.change(i1, i0, j1, j0, &mut out, &mut anychange, &mut ctx, label_a, label_b);
            if i1 == 0 {
                break;
            }
            i0 = i1 - 1;
        }
    }
    if len0 == 0 {
        engine.change(1, 0, 1, len1, &mut out, &mut anychange, &mut ctx, label_a, label_b);
    }
    if options.format == DiffFormat::Unified && anychange {
        dump_unified(a, b, &mut ctx, options.context, &mut out);
    }

    let status = if anychange { DiffStatus::Differ } else { DiffStatus::Same };
    DiffResult { status, text: out }
}

/// search(): binary search over the klist for the first y >= target.
fn search(clist: &[Cand], klist: &[usize], k: usize, y: i64) -> usize {
    if clist[klist[k]].y < y {
        return k + 1;
    }
    let (mut i, mut j) = (0usize, k + 1);
    let mut l;
    loop {
        l = (i + j) / 2;
        if l <= i {
            break;
        }
        let t = clist[klist[l]].y;
        if t > y {
            j = l;
        } else if t < y {
            i = l;
        } else {
            return l;
        }
    }
    l + 1
}

fn isqrt(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut x = 1usize;
    loop {
        let y = x;
        x = n / x;
        x += y;
        x /= 2;
        let d = x as isize - y as isize;
        if d <= 1 && d >= -1 {
            break;
        }
    }
    x
}

impl<'a> Engine<'a> {
    fn a_line(&self, i: usize) -> &'a str {
        self.a[i - 1]
    }

    fn b_line(&self, i: usize) -> &'a str {
        self.b[i - 1]
    }

    /// change(): one hunk in the configured format (diffreg.c:1017-1147).
    fn change(
        &mut self,
        a: usize,
        b: usize,
        c: usize,
        d: usize,
        out: &mut String,
        anychange: &mut bool,
        ctx: &mut Vec<ContextVec>,
        label_a: &str,
        label_b: &str,
    ) {
        if a > b && c > d {
            return;
        }
        match self.options.format {
            DiffFormat::Unified => {
                // Accumulate; dump when the gap exceeds 2*context+1.
                if *anychange
                    && a > ctx.last().map(|v| v.b).unwrap_or(0) + 2 * self.options.context + 1
                    && c > ctx.last().map(|v| v.d).unwrap_or(0) + 2 * self.options.context + 1
                {
                    dump_unified(self.a, self.b, ctx, self.options.context, out);
                }
                if !*anychange {
                    out.push_str(&format!("--- {}\n+++ {}\n", label_a, label_b));
                    *anychange = true;
                }
                ctx.push(ContextVec { a, b, c, d });
                return;
            }
            DiffFormat::Brief => {
                *anychange = true;
                return;
            }
            _ => {}
        }
        *anychange = true;
        match self.options.format {
            DiffFormat::Normal | DiffFormat::Ed => {
                range(a, b, ",", out);
                out.push(if a > b {
                    'a'
                } else if c > d {
                    'd'
                } else {
                    'c'
                });
                if self.options.format == DiffFormat::Normal {
                    range(c, d, ",", out);
                }
                out.push('\n');
            }
            DiffFormat::ForwardEd => {
                out.push(if a > b { 'a' } else if c > d { 'd' } else { 'c' });
                range(a, b, " ", out);
                out.push('\n');
            }
            DiffFormat::Rcs => {
                if a > b {
                    out.push_str(&format!("a{} {}\n", b, d - c + 1));
                } else {
                    out.push_str(&format!("d{} {}\n", a, b - a + 1));
                    if c <= d {
                        out.push_str(&format!("a{} {}\n", b, d - c + 1));
                    }
                }
            }
            _ => {}
        }
        if self.options.format == DiffFormat::Normal {
            for i in a..=b {
                out.push_str("< ");
                out.push_str(self.a_line(i));
                out.push('\n');
            }
            if a <= b && c <= d {
                out.push_str("---\n");
            }
        }
        // New-side lines: raw in every form but Normal (the C passes
        // '\0' as the prefix for all non-NORMAL formats).
        let raw = !matches!(self.options.format, DiffFormat::Normal);
        if c <= d {
            for i in c..=d {
                if !raw {
                    out.push_str("> ");
                }
                out.push_str(self.b_line(i));
                out.push('\n');
            }
        }
        if matches!(self.options.format, DiffFormat::Ed | DiffFormat::ForwardEd) && c <= d {
            out.push_str(".\n");
        }
    }
}

/// `range()`: `a` or `a,b` (when a < b, ascending first).
fn range(a: usize, b: usize, separator: &str, out: &mut String) {
    use core::cmp;
    out.push_str(&format!("{}", cmp::min(a, b)));
    if a < b {
        out.push_str(separator);
        out.push_str(&format!("{}", b));
    }
}

/// `uni_range()`: start,length with the C's empty-range shape.
fn uni_range(a: usize, b: usize, out: &mut String) {
    if a < b {
        out.push_str(&format!("{},{}", a, b - a + 1));
    } else if a == b {
        out.push_str(&format!("{}", b));
    } else {
        out.push_str(&format!("{},0", b));
    }
}

/// dump_unified_vec(): one `@@` hunk with merged context.
fn dump_unified(
    a: &[&str],
    b: &[&str],
    ctx: &mut Vec<ContextVec>,
    context: usize,
    out: &mut String,
) {
    use core::cmp::{max, min};
    if ctx.is_empty() {
        ctx.clear();
        return;
    }
    let len0 = a.len();
    let len1 = b.len();
    let lowa = max(1, ctx[0].a.saturating_sub(context));
    let upb = min(len0, ctx.last().unwrap().b + context);
    let lowc = max(1, ctx[0].c.saturating_sub(context));
    let upd = min(len1, ctx.last().unwrap().d + context);

    out.push_str("@@ -");
    uni_range(lowa, upb, out);
    out.push_str(" +");
    uni_range(lowc, upd, out);
    out.push_str(" @@\n");

    let mut cur_lowa = lowa;
    let mut cur_lowc = lowc;
    for cv in ctx.iter() {
        let (cv_a, cv_b, cv_c, cv_d) = (cv.a, cv.b, cv.c, cv.d);
        if cv_a <= cv_b && cv_c <= cv_d {
            fetch(a, cur_lowa, cv_a - 1, " ", out);
            fetch(a, cv_a, cv_b, "-", out);
            fetch(b, cv_c, cv_d, "+", out);
        } else if cv_a <= cv_b {
            fetch(a, cur_lowa, cv_a - 1, " ", out);
            fetch(a, cv_a, cv_b, "-", out);
        } else {
            fetch(b, cur_lowc, cv_c - 1, " ", out);
            fetch(b, cv_c, cv_d, "+", out);
        }
        cur_lowa = cv_b + 1;
        cur_lowc = cv_d + 1;
    }
    fetch(b, ctx.last().unwrap().d + 1, upd, " ", out);
    ctx.clear();
}

/// fetch(): lines a..=b, each headed by an exact prefix string
/// (unified markers carry no trailing space; normal's "< " does).
fn fetch(lines: &[&str], a: usize, b: usize, prefix: &str, out: &mut String) {
    if a > b {
        return;
    }
    for i in a..=b {
        out.push_str(prefix);
        out.push_str(lines[i - 1]);
        out.push('\n');
    }
}

/// Splits file text into lines for the deciding half; the trailing
/// newline fact is the doing half's concern (registered corner).
pub fn split_text(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0usize;
    for i in 0..bytes.len() {
        if bytes[i] == b'\n' {
            lines.push(&text[start..i]);
            start = i + 1;
        }
    }
    if start < bytes.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// strtod-shaped number check reused by the diff bin's -U parser.
pub fn parse_number(s: &str) -> Option<usize> {
    parse_float_prefix(s).and_then(|(v, used)| {
        if used == s.len() && v >= 0.0 {
            Some(v as usize)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal(a: &str, b: &str) -> String {
        let la = split_text(a);
        let lb = split_text(b);
        diff_lines(&la, &lb, "d1", "d2", &DiffOptions::default()).text
    }

    fn with_format(a: &str, b: &str, format: DiffFormat) -> DiffResult {
        let la = split_text(a);
        let lb = split_text(b);
        diff_lines(
            &la,
            &lb,
            "d1",
            "d2",
            &DiffOptions { format, ..DiffOptions::default() },
        )
    }

    #[test]
    fn test_identical_files_report_same() {
        let r = with_format("a\nb\n", "a\nb\n", DiffFormat::Normal);
        assert_eq!(r.status, DiffStatus::Same);
        assert_eq!(r.text, "");
    }

    #[test]
    fn test_change_hunk_normal_format() {
        // Host reference: diff d1 d2 → "2c2 / < b / --- / > x".
        assert_eq!(normal("a\nb\nc\n", "a\nx\nc\nd\n"), "2c2\n< b\n---\n> x\n3a4\n> d\n");
    }

    #[test]
    fn test_delete_and_pure_delete_hunks() {
        // Host reference: diff e1 e2 → "2d1 / < 2 / 4d2 / < 4".
        assert_eq!(normal("1\n2\n3\n4\n5\n", "1\n3\n5\n"), "2d1\n< 2\n4d2\n< 4\n");
        // Deleting everything from a one-line file.
        assert_eq!(normal("x\n", ""), "1d0\n< x\n");
    }

    #[test]
    fn test_insert_into_empty_file() {
        // m == 0: the C emits change(1, 0, 1, len1).
        assert_eq!(normal("", "a\nb\n"), "0a1,2\n> a\n> b\n");
    }

    #[test]
    fn test_append_at_end() {
        assert_eq!(normal("a\nb\nc\n", "a\nb\nc\nd\n"), "3a4\n> d\n");
    }

    #[test]
    fn test_ed_script_reverse_order() {
        // Host reference: diff -e e1 e2 → "4d\n2d\n".
        let r = with_format("1\n2\n3\n4\n5\n", "1\n3\n5\n", DiffFormat::Ed);
        assert_eq!(r.text, "4d\n2d\n");
        // Host reference: diff -e d1 d2 → "3a\nd\n.\n2c\nx\n.\n." —
        // the last "." of each change closes the command.
        let r = with_format("a\nb\nc\n", "a\nx\nc\nd\n", DiffFormat::Ed);
        assert_eq!(r.text, "3a\nd\n.\n2c\nx\n.\n");
    }

    #[test]
    fn test_forward_ed_shape() {
        // -f: letter first then "a b" range; new lines raw; "." after.
        let r = with_format("a\nb\nc\n", "a\nx\nc\nd\n", DiffFormat::ForwardEd);
        assert_eq!(r.text, "c2\nx\n.\na3\nd\n.\n");
    }

    #[test]
    fn test_rcs_format() {
        // Host reference: diff -n d1 d2 → "d2 1 / a2 1 / x / a3 1 / d".
        let r = with_format("a\nb\nc\n", "a\nx\nc\nd\n", DiffFormat::Rcs);
        assert_eq!(r.text, "d2 1\na2 1\nx\na3 1\nd\n");
    }

    #[test]
    fn test_brief_reports_without_text() {
        let r = with_format("a\n", "b\n", DiffFormat::Brief);
        assert_eq!(r.status, DiffStatus::Differ);
        assert_eq!(r.text, "");
    }

    #[test]
    fn test_unified_hunk_with_context() {
        // Host reference: diff -u d1 d2 (headers stripped).
        let la = split_text("a\nb\nc\n");
        let lb = split_text("a\nx\nc\nd\n");
        let r = diff_lines(&la, &lb, "d1", "d2", &DiffOptions { format: DiffFormat::Unified, ..Default::default() });
        assert_eq!(
            r.text,
            "--- d1\n+++ d2\n@@ -1,3 +1,4 @@\n a\n-b\n+x\n c\n+d\n"
        );
    }

    #[test]
    fn test_unified_context_merges_close_hunks() {
        // Two changes one line apart merge into one hunk.
        let la = split_text("1\n2\n3\n4\n5\n6\n7\n8\n9\n");
        let lb = split_text("1\nx\n3\n4\n5\n6\n7\ny\n9\n");
        let r = diff_lines(&la, &lb, "a", "b", &DiffOptions { format: DiffFormat::Unified, ..Default::default() });
        assert_eq!(
            r.text,
            "--- a\n+++ b\n@@ -1,9 +1,9 @@\n 1\n-2\n+x\n 3\n 4\n 5\n 6\n 7\n-8\n+y\n 9\n"
        );
    }

    #[test]
    fn test_prefix_suffix_pruning_correctness() {
        // Long identical head and tail with a small middle change.
        let head = "h1\nh2\nh3\n";
        let tail = "t1\nt2\nt3\n";
        let a = format!("{}m\n{}", head, tail);
        let b = format!("{}n\n{}", head, tail);
        let r = normal(&a, &b);
        assert_eq!(r, "4c4\n< m\n---\n> n\n");
    }

    #[test]
    fn test_minimal_flag_still_minimal_for_simple_case() {
        let la = split_text("a\nb\nc\n");
        let lb = split_text("b\nc\nd\n");
        let r = diff_lines(&la, &lb, "a", "b", &DiffOptions { minimal: true, ..Default::default() });
        assert_eq!(r.text, "1d0\n< a\n3a3\n> d\n");
    }

    #[test]
    fn test_split_text_keeps_final_fragment() {
        assert_eq!(split_text("a\nb"), vec!["a", "b"]);
        assert_eq!(split_text("a\nb\n"), vec!["a", "b"]);
        assert_eq!(split_text(""), Vec::<&str>::new());
    }

    #[test]
    fn test_parse_number() {
        assert_eq!(parse_number("5"), Some(5));
        assert_eq!(parse_number("5x"), None);
        assert_eq!(parse_number("-1"), None);
    }

    #[test]
    fn test_multi_hunk_vectors_from_host() {
        // 12 lines, changes at 5 and 10 (host: seq 1..12 with two
        // substitutions).
        let a = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"];
        let b = ["1", "2", "3", "4", "five", "6", "7", "8", "9", "ten", "11", "12"];
        let r = diff_lines(&a, &b, "l1", "l2", &DiffOptions::default());
        assert_eq!(r.text, "5c5\n< 5\n---\n> five\n10c10\n< 10\n---\n> ten\n");
        // -e: reverse order, each change closed with a dot.
        let r = diff_lines(&a, &b, "l1", "l2", &DiffOptions { format: DiffFormat::Ed, ..Default::default() });
        assert_eq!(r.text, "10c\nten\n.\n5c\nfive\n.\n");
        // -U2: the two hunks adjoin (5 + 2*2 + 1 >= 10) and merge.
        let r = diff_lines(&a, &b, "l1", "l2", &DiffOptions { format: DiffFormat::Unified, context: 2, ..Default::default() });
        assert_eq!(
            r.text,
            "--- l1\n+++ l2\n@@ -3,10 +3,10 @@\n 3\n 4\n-5\n+five\n 6\n 7\n 8\n 9\n-10\n+ten\n 11\n 12\n"
        );
    }
}
