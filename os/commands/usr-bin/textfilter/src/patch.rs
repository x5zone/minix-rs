//! patch — apply a diff file to an original (minix3/usr.bin/patch/).
//!
//! Deciding half: unified and normal hunk parsing (pch.c another_hunk,
//! the UNI_DIFF and NORMAL_DIFF branches) plus the application engine
//! (patch.c locate_hunk / patch_match / apply_hunk): first guess at
//! `pch_first + last_offset`, alternating forward/backward offset
//! search, then context fuzz (leading/trailing context lines dropped
//! from the match, default maximum 2). Applied, offset and fuzz
//! reporting mirror the C's state machine. Context-format (`***`)
//! hunks, ed-script piping, `similar()` whitespace folding, reverse
//! (`-R`) and the `\ No newline` bookkeeping are registered corners.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// One parsed hunk: old/new ranges plus tagged lines in patch order
/// (the C's p_line/p_char arrays).
#[derive(Debug, Clone, PartialEq)]
pub struct Hunk {
    /// pch_first(): 1-based first pattern line in the old file.
    pub old_first: usize,
    /// pch_ptrn_lines(): pattern (old-side) line count.
    pub ptrn_lines: usize,
    /// pch_newfirst(): first replacement line in the new file.
    pub new_first: usize,
    /// pch_repl_lines().
    pub repl_lines: usize,
    /// Tagged lines: ' ' context, '-' old-only, '+' new-only. For
    /// normal diffs the C also rewrites changes into '!' runs — here
    /// they keep the normal-diff '-'/'+' split, which applies the
    /// same.
    pub lines: Vec<(char, String)>,
}

#[derive(Debug, Clone)]
pub struct PatchOptions {
    /// -F: maximum context fuzz (the C's max_fuzz, default 2).
    pub max_fuzz: usize,
}

impl Default for PatchOptions {
    fn default() -> Self {
        PatchOptions { max_fuzz: 2 }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct PatchOutcome {
    /// The patched text (lines joined with newlines, trailing newline
    /// preserved as in the input shape).
    pub output: String,
    /// Hunks applied cleanly or with offset/fuzz.
    pub applied: usize,
    /// (hunk number, detected position) per failure.
    pub failed: Vec<(usize, usize)>,
    /// Last offset reached (the C's last_offset chain).
    pub last_offset: i64,
}

#[derive(Debug, PartialEq)]
pub enum PatchError {
    Malformed(usize, &'static str),
}

impl core::fmt::Display for PatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PatchError::Malformed(line, why) => write!(f, "malformed patch at line {}: {}", line, why),
        }
    }
}

/// Parses all hunks from patch text (lines without trailing newlines).
pub fn parse_patch(text: &str) -> Result<Vec<Hunk>, PatchError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut hunks = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        if let Some(rest) = line.strip_prefix("@@ -") {
            i += 1;
            let (hunk, consumed) = parse_unified(rest, &lines, i)?;
            hunks.push(hunk);
            i = consumed;
        } else if let Some((old_first, kind, new_first, new_count)) = parse_normal_header(line) {
            i += 1;
            let (hunk, consumed) = parse_normal(old_first, kind, new_first, new_count, &lines, i)?;
            hunks.push(hunk);
            i = consumed;
        } else {
            i += 1;
        }
    }
    Ok(hunks)
}

/// Parses `a[,b] +c[,d] @@` after the "@@ -" marker.
fn parse_unified(
    rest: &str,
    lines: &[&str],
    mut i: usize,
) -> Result<(Hunk, usize), PatchError> {
    fn bad(i: usize, why: &'static str) -> PatchError {
        PatchError::Malformed(i + 1, why)
    }
    let (old_first, ptrn_lines, rest) =
        range_pair(rest).ok_or_else(|| bad(i, "malformed patch"))?;
    let rest = rest.trim_start_matches(' ');
    let rest = rest.strip_prefix('+').ok_or_else(|| bad(i, "malformed patch"))?;
    let (new_first, repl_lines, _) = range_pair(rest).ok_or_else(|| bad(i, "malformed patch"))?;
    // `if (!p_ptrn_lines) p_first++` — pure append.
    let (old_first, ptrn_lines) = if ptrn_lines == 0 {
        (old_first + 1, 0)
    } else {
        (old_first, ptrn_lines)
    };
    let mut hunk = Hunk {
        old_first,
        ptrn_lines,
        new_first,
        repl_lines,
        lines: Vec::new(),
    };
    let mut old_left = ptrn_lines;
    let mut new_left = repl_lines;
    while old_left > 0 || new_left > 0 {
        let line = *lines.get(i).ok_or(PatchError::Malformed(i + 1, "unexpected end of file in patch"))?;
        i += 1;
        let (ch, text) = match line.chars().next() {
            Some(c @ ('-' | '+' | ' ')) => (c, &line[1..]),
            // A leading tab or empty line: the space got eaten.
            Some('\t') => (' ', &line[1..]),
            Some('\n') | None => (' ', ""),
            _ => return Err(bad(i, "malformed patch")),
        };
        match ch {
            '-' => {
                if old_left == 0 {
                    return Err(bad(i, "malformed patch"));
                }
                old_left -= 1;
            }
            '+' => {
                if new_left == 0 {
                    return Err(bad(i, "malformed patch"));
                }
                new_left -= 1;
            }
            _ => {
                // ' ' fills one pattern slot and one replacement slot.
                old_left = old_left.saturating_sub(1);
                new_left = new_left.saturating_sub(1);
            }
        }
        hunk.lines.push((ch, text.to_string()));
    }
    Ok((hunk, i))
}

/// `a[,n]` → (a, n-or-1, rest): unified headers carry start and line
/// COUNT directly (unlike normal headers' interval ends).
fn range_pair(s: &str) -> Option<(usize, usize, &str)> {
    let digits: usize = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let a: usize = s[..digits].parse().ok()?;
    let rest = &s[digits..];
    if let Some(after) = rest.strip_prefix(',') {
        let n: usize = after.bytes().take_while(|b| b.is_ascii_digit()).count();
        if n == 0 {
            return None;
        }
        let count: usize = after[..n].parse().ok()?;
        Some((a, count, &after[n..]))
    } else {
        Some((a, 1, rest))
    }
}

/// Recognizes `XaY`, `XdY`, `XcY[,Z]` normal hunk headers.
fn parse_normal_header(line: &str) -> Option<(usize, char, usize, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let old_first: usize = line[..i].parse().ok()?;
    if i >= bytes.len() {
        return None;
    }
    let kind = bytes[i] as char;
    if !matches!(kind, 'a' | 'd' | 'c') {
        return None;
    }
    i += 1;
    let ds = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == ds {
        return None;
    }
    let new_first: usize = line[ds..i].parse().ok()?;
    let new_count = if i < bytes.len() && bytes[i] == b',' {
        i += 1;
        let ds = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == ds {
            return None;
        }
        line[ds..i].parse::<usize>().ok()?.saturating_sub(new_first - 1)
    } else {
        1
    };
    if i != bytes.len() {
        return None;
    }
    Some((old_first, kind, new_first, new_count))
}

/// Normal hunk body: `< ` old lines, optional `---`, `> ` new lines.
fn parse_normal(
    old_first: usize,
    kind: char,
    new_first: usize,
    new_count: usize,
    lines: &[&str],
    mut i: usize,
) -> Result<(Hunk, usize), PatchError> {
    fn bad(i: usize, why: &'static str) -> PatchError {
        PatchError::Malformed(i + 1, why)
    }
    // The C's conversions: 'a' increments p_first (append), 'd'
    // increments min (the new-side start).
    let (old_first, kind) = if kind == 'a' { (old_first + 1, kind) } else { (old_first, kind) };
    let ptrn_lines = match kind {
        'a' => 0,
        _ => {
            let mut n = 0usize;
            let mut j = i;
            while j < lines.len() && lines[j].starts_with("< ") {
                n += 1;
                j += 1;
            }
            n
        }
    };
    let (new_first, repl_lines) = if kind == 'd' {
        (new_first + 1, new_count.saturating_sub(1))
    } else {
        (new_first, new_count)
    };
    let mut hunk = Hunk {
        old_first,
        ptrn_lines,
        new_first,
        repl_lines,
        lines: Vec::new(),
    };
    for _ in 0..ptrn_lines {
        let line = *lines.get(i).ok_or(bad(i, "unexpected end of file in patch"))?;
        if !line.starts_with("< ") {
            return Err(bad(i, "< expected"));
        }
        hunk.lines.push(('-', line[2..].to_string()));
        i += 1;
    }
    if kind == 'c' {
        let line = lines.get(i).ok_or(bad(i, "unexpected end of file in patch"))?;
        if !line.starts_with("---") {
            return Err(bad(i, "--- expected"));
        }
        i += 1;
    }
    for _ in 0..repl_lines {
        let line = *lines.get(i).ok_or(bad(i, "unexpected end of file in patch"))?;
        if !line.starts_with("> ") {
            return Err(bad(i, "> expected"));
        }
        hunk.lines.push(('+', line[2..].to_string()));
        i += 1;
    }
    Ok((hunk, i))
}

/// patch_match: pattern lines 1+fuzz..=ptrn_lines-fuzz against the
/// input starting at base+offset+fuzz (1-based).
fn patch_match(input: &[&str], hunk: &Hunk, base: usize, offset: i64, fuzz: usize) -> bool {
    if base as i64 + offset < 1 || base as i64 + offset > input.len() as i64 {
        return false;
    }
    // At least one pattern line must remain verified.
    if hunk.ptrn_lines > 0 && hunk.ptrn_lines <= 2 * fuzz {
        return false;
    }
    let pat_lines = hunk.ptrn_lines.saturating_sub(fuzz);
    // pfetch(pline) walks the old-side view (' ' and '-' lines).
    let pattern: Vec<&str> = hunk
        .lines
        .iter()
        .filter(|(c, _)| *c != '+')
        .map(|(_, t)| t.as_str())
        .collect();
    let mut pline = 1 + fuzz;
    let mut iline = (base as i64 + offset + fuzz as i64) as usize;
    while pline <= pat_lines {
        if iline > input.len() {
            return false;
        }
        if input[iline - 1] != pattern[pline - 1] {
            return false;
        }
        pline += 1;
        iline += 1;
    }
    true
}

/// locate_hunk: try first_guess, then alternating ±offset; 0 = miss.
fn locate_hunk(input: &[&str], hunk: &Hunk, last_offset: i64, fuzz: usize) -> (usize, i64) {
    let first_guess = hunk.old_first as i64 + last_offset;
    let pat_lines = hunk.ptrn_lines;
    if pat_lines == 0 {
        // Null range matches always.
        return (first_guess.max(0) as usize, last_offset);
    }
    if first_guess <= 0 {
        return (0, last_offset);
    }
    let first_guess = first_guess as usize;
    if first_guess <= input.len() && patch_match(input, hunk, first_guess, 0, fuzz) {
        return (first_guess, last_offset);
    }
    let max_pos_offset = input.len() as i64 - first_guess as i64 - pat_lines as i64 + 1;
    let mut offset = 1i64;
    loop {
        let check_after = offset <= max_pos_offset;
        // max_neg_offset guards against reaching above the frozen top;
        // the line-vector model needs no lower bound beyond 1.
        let check_before = first_guess as i64 - offset >= 1;
        if check_after && patch_match(input, hunk, first_guess, offset, fuzz) {
            return (first_guess + offset as usize, offset);
        } else if check_before && patch_match(input, hunk, first_guess, -offset, fuzz) {
            return ((first_guess as i64 - offset) as usize, -offset);
        } else if !check_before && !check_after {
            return (0, last_offset);
        }
        offset += 1;
    }
}

/// Applies hunks to `input` with offset chaining and fuzz fallback
/// (apply_hunk + copy_till: context lines are copied lazily, deletions
/// skip their aligned input line, insertions emit in place).
pub fn apply_patch(input: &[&str], hunks: &[Hunk], options: &PatchOptions) -> PatchOutcome {
    let mut out: Vec<String> = Vec::new();
    let mut frozen = 0usize; // 1-based: input lines already emitted
    let mut last_offset = 0i64;
    let mut outcome = PatchOutcome::default();

    let mut copy_till = |out: &mut Vec<String>, frozen: &mut usize, n: usize| {
        while *frozen < n && *frozen < input.len() {
            out.push(input[*frozen].to_string());
            *frozen += 1;
        }
    };

    for (no, hunk) in hunks.iter().enumerate() {
        let mut where_line = 0usize;
        let mut fuzz = 0usize;
        loop {
            let (pos, offset) = locate_hunk(input, hunk, last_offset, fuzz);
            if pos != 0 {
                where_line = pos;
                last_offset = offset;
                if fuzz > 0 {
                    // Reported like the C's "with fuzz" state.
                }
                break;
            }
            if fuzz >= options.max_fuzz {
                break;
            }
            fuzz += 1;
        }
        if where_line == 0 {
            outcome.failed.push((no + 1, 0));
            continue;
        }
        outcome.applied += 1;
        // The C's apply_hunk starts with where--.
        let base = where_line as i64 - 1;
        // Pattern view (' ' and '-') and new view (' ' and '+').
        let oldv: Vec<&(char, String)> =
            hunk.lines.iter().filter(|(c, _)| *c != '+').collect();
        let newv: Vec<&(char, String)> =
            hunk.lines.iter().filter(|(c, _)| *c != '-').collect();
        let mut o = 0usize;
        let mut n = 0usize;
        let aligned = |o: usize| (base + o as i64 - 1).max(0) as usize;
        while o < oldv.len() {
            if oldv[o].0 == '-' {
                copy_till(&mut out, &mut frozen, aligned(o + 1));
                frozen += 1; // the aligned line is deleted
                o += 1;
            } else if n < newv.len() && newv[n].0 == '+' {
                copy_till(&mut out, &mut frozen, aligned(o + 1));
                out.push(newv[n].1.clone());
                n += 1;
            } else {
                // Context on both sides: copied lazily by later
                // copy_tills (the C defers it the same way).
                o += 1;
                n += 1;
            }
        }
        // Trailing insertions (pure appends end here too).
        if n < newv.len() && newv[n].0 == '+' {
            copy_till(&mut out, &mut frozen, aligned(oldv.len() + 1));
            while n < newv.len() && newv[n].0 == '+' {
                out.push(newv[n].1.clone());
                n += 1;
            }
        }
    }
    // Copy any trailing input after the last hunk.
    while frozen < input.len() {
        out.push(input[frozen].to_string());
        frozen += 1;
    }
    outcome.output = out.join("\n");
    if !out.is_empty() {
        outcome.output.push('\n');
    }
    outcome.last_offset = last_offset;
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(input: &str, patch: &str) -> PatchOutcome {
        let lines: Vec<&str> = input.lines().collect();
        let hunks = parse_patch(patch).unwrap();
        apply_patch(&lines, &hunks, &PatchOptions::default())
    }

    #[test]
    fn test_fuzz_rescues_mismatched_leading_context() {
        // The leading context line does not match; at fuzz 1 the
        // interior lines (delete b, insert x) still line up.
        let out = apply("a\nb\nc\nd\n", "--- f\n+++ g\n@@ -1,4 +1,4 @@\n q\n-b\n+x\n c\n d\n");
        assert_eq!(out.applied, 1);
        assert_eq!(out.output, "a\nx\nc\nd\n");
    }

    #[test]
    fn test_unified_exact_apply() {
        let out = apply("a\nb\nc\nd\n", "--- f\n+++ g\n@@ -1,4 +1,4 @@\n a\n-b\n+x\n c\n d\n");
        assert_eq!(out.output, "a\nx\nc\nd\n");
        assert_eq!(out.applied, 1);
        assert!(out.failed.is_empty());
    }

    #[test]
    fn test_unified_append_hunk() {
        // Pure append: "@@ -3,0 +4,2 @@" inserts two lines after 3.
        let out = apply("1\n2\n3\n", "--- f\n+++ g\n@@ -3,0 +4,2 @@\n+4\n+5\n");
        assert_eq!(out.output, "1\n2\n3\n4\n5\n");
    }

    #[test]
    fn test_offset_search_finds_moved_context() {
        // The hunk says line 1 but the content sits at line 3.
        let out = apply("p\nq\nr\ns\nt\n", "--- f\n+++ g\n@@ -1,2 +1,1 @@\n-s\n-t\n+u\n");
        assert_eq!(out.applied, 1);
        assert_eq!(out.last_offset, 3);
        assert_eq!(out.output, "p\nq\nr\nu\n");
    }

    #[test]
    fn test_normal_hunk_change() {
        let out = apply("a\nb\nc\n", "--- f\n+++ g\n2c2\n< b\n---\n> x\n");
        assert_eq!(out.output, "a\nx\nc\n");
    }

    #[test]
    fn test_normal_hunk_delete_and_append() {
        let out = apply("1\n2\n3\n4\n5\n", "--- f\n+++ g\n2d1\n< 2\n4a4\n> 4.5\n");
        assert_eq!(out.output, "1\n3\n4\n4.5\n5\n");
    }

    #[test]
    fn test_failed_hunk_reported() {
        // The hunk expects content that nowhere appears, and the
        // single-line pattern leaves nothing for fuzz to verify.
        let out = apply("a\nb\n", "--- f\n+++ g\n@@ -5,1 +5,1 @@\n-x\n+y\n");
        assert_eq!(out.applied, 0);
        assert_eq!(out.failed, vec![(1, 0)]);
        assert_eq!(out.output, "a\nb\n");
    }

    #[test]
    fn test_multiple_hunks_with_offset_chain() {
        // Second hunk's position shifts by the first hunk's growth.
        let input = "1\n2\n3\n4\n5\n6\n";
        let patch = "--- f\n+++ g\n@@ -1,2 +1,3 @@\n 1\n-2\n+2a\n+2b\n 3\n@@ -5,2 +6,2 @@\n-5\n+five\n 6\n";
        let out = apply(input, patch);
        assert_eq!(out.output, "1\n2a\n2b\n3\n4\nfive\n6\n");
        assert_eq!(out.applied, 2);
    }

    #[test]
    fn test_parse_normal_header_shapes() {
        assert_eq!(parse_normal_header("2c2"), Some((2, 'c', 2, 1)));
        assert_eq!(parse_normal_header("3a4"), Some((3, 'a', 4, 1)));
        assert_eq!(parse_normal_header("4a4,6"), Some((4, 'a', 4, 3)));
        assert_eq!(parse_normal_header("2d1"), Some((2, 'd', 1, 1)));
        assert_eq!(parse_normal_header("junk"), None);
    }

    #[test]
    fn test_malformed_hunks_rejected() {
        assert!(parse_patch("@@ -1,0 +1,1 @@\n+x\n").is_ok());
        assert!(parse_patch("@@ - + @@\n").is_err());
        // Unknown headers are garbage to skip, not errors.
        assert_eq!(parse_patch("1e2\n").unwrap().len(), 0);
    }
}
