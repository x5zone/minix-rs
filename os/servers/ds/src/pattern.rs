//! DS subscription pattern engine: anchored POSIX ERE, no allocation.
//!
//! Mirrors the `regcomp("^" + pattern + "$", REG_EXTENDED)` +
//! `regexec` pair the server applies to every subscription
//! (`minix3/minix/servers/ds/store.c:487-498, 186-193`).
//! 10-ds-subscribe-check.md. [ARCH A-2]: C stores the *compiled* formula
//! in the subscription seat and anchors it at subscribe time; Rust
//! stores the raw pattern text (`subscription.rs`) and this engine
//! applies the anchoring semantically — every match is a *full* match,
//! which is exactly what the `^…$` anchors state. A literal pattern
//! behaves byte-for-byte like C's; the extended syntax (`.`, `*`, `+`,
//! `?`, `[class]`, `(group)`, `|`, escapes, `^`/`$` anchors) is decided
//! here instead of refused with `BadPattern`.
//!
//! The engine is a backtracking matcher over the NUL-stopped lane
//! content (the `strcmp` stop rule, 04), with a step budget so a
//! pathological pattern cannot spin. No allocation anywhere: the crate
//! has no allocator in production builds.
//!
//! Known corners, stated honestly: `{m,n}` intervals are not ERE here —
//! `{` binds as a literal byte (glibc's own fallback for malformed
//! intervals; no known client subscribes with intervals); a quantifier
//! on an anchor (`^*`) refuses at [`EreMatcher::check`], and a
//! zero-width repetition (`()*`-family) only matches through its
//! zero-width arm where glibc may match empty — every real subscriber
//! pattern sits far from these corners.

use crate::subscribe::SubscribeReject;
use core::cell::Cell;
use minix_types::DS_MAX_KEYLEN;

/// The anchored ERE engine (`regcomp`/`regexec` counterpart, A-2).
///
/// Unit struct: the engine holds no state. C compiles once at subscribe
/// time and pays `regexec` per entry; Rust re-parses per match over the
/// stored text — on an 80-byte lane and a 128-seat table the difference
/// is noise, and the payoff is an engine with zero allocation and zero
/// cached-state lifecycle (nothing to regfree, 04's release hook stays
/// a no-op).
#[derive(Debug, Clone, Copy, Default)]
pub struct EreMatcher;

impl EreMatcher {
    /// Full-match `pattern` against `key` (both NUL-stopped lanes).
    pub fn matches(&self, pattern: &[u8; DS_MAX_KEYLEN], key: &[u8; DS_MAX_KEYLEN]) -> bool {
        let pat = cstr(pattern);
        let text = cstr(key);
        let steps = Cell::new(0u32);
        // Anchored full match: the top continuation accepts only the
        // empty remainder — the whole pattern must consume the whole key.
        match_expr(pat, text, text.len(), &mut |rest| rest.is_empty(), &steps)
    }

    /// Compile-check `pattern`: `Ok` means subscribable (`regcomp`
    /// counterpart; failures surface as `BadPattern`/`EINVAL`).
    pub fn check(&self, pattern: &[u8; DS_MAX_KEYLEN]) -> Result<(), SubscribeReject> {
        if valid_expr(cstr(pattern)) {
            Ok(())
        } else {
            Err(SubscribeReject::BadPattern)
        }
    }
}

/// Step budget for one match call. A validated pattern over an 80-byte
/// lane finishes in the hundreds; the bound exists so an unvalidated
/// pattern cannot spin the sweep.
const STEP_BUDGET: u32 = 20_000;

/// NUL-stopped content of a lane (the `strcmp` stop rule, 04).
fn cstr(lane: &[u8; DS_MAX_KEYLEN]) -> &[u8] {
    let len = lane.iter().position(|&b| b == 0).unwrap_or(lane.len());
    &lane[..len]
}

fn spend(steps: &Cell<u32>) -> bool {
    let n = steps.get() + 1;
    steps.set(n);
    n <= STEP_BUDGET
}

/// One pattern element: a consuming single byte, an anchor, or a group.
enum Atom<'p> {
    /// Literal byte, `.`, or `[class]` — distinguished by kind.
    Single(SingleKind<'p>),
    /// `^`: assert at the text's start.
    Start,
    /// `$`: assert at the text's end.
    End,
    /// `(...)`: a grouped alternation expression (bytes between parens).
    Group(&'p [u8]),
}

/// The consuming single-byte forms.
enum SingleKind<'p> {
    /// One exact byte (literal or `\`-escaped).
    Byte(u8),
    /// `.`: any byte of the lane content.
    Any,
    /// `[...]`: the class span (`pat[..= end]`, `pat[0] == '['`).
    Class { pat: &'p [u8], end: usize },
}

/// Match one byte against a [`SingleKind`].
fn match_single(kind: &SingleKind<'_>, b: u8) -> bool {
    match kind {
        SingleKind::Byte(want) => b == *want,
        SingleKind::Any => true,
        SingleKind::Class { pat, end } => class_matches(&pat[..=*end], b),
    }
}

/// Does `rest` begin with a quantifier? `*`/`+`/`?` as (min, max);
/// `{m,n}` intervals are unsupported (`{` binds as a literal — module
/// docs).
fn quantifier(rest: &[u8]) -> Option<(usize, Option<usize>)> {
    match rest.first()? {
        b'*' => Some((0, None)),
        b'+' => Some((1, None)),
        b'?' => Some((0, Some(1))),
        _ => None,
    }
}

/// Match `pat` (an alternation expression) against `t`, handing each
/// possible remainder to the continuation `k` — which decides whether
/// the rest of the sequence can still match. `t0` is the full text
/// length (the `^` anchor's reference point).
fn match_expr(
    pat: &[u8],
    t: &[u8],
    t0: usize,
    k: &mut dyn FnMut(&[u8]) -> bool,
    steps: &Cell<u32>,
) -> bool {
    let mut start = 0usize;
    loop {
        let end = next_alt_cut(pat, start).unwrap_or(pat.len());
        if match_seq(&pat[start..end], t, t0, k, steps) {
            return true;
        }
        if end == pat.len() {
            return false;
        }
        start = end + 1;
        if !spend(steps) {
            return false;
        }
    }
}

/// Offset of the next depth-0 `|` at or after `from` (escapes and class
/// spans are transparent); `None` when this is the last branch.
fn next_alt_cut(pat: &[u8], from: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_class = false;
    let mut escaped = false;
    for (i, &b) in pat[from..].iter().enumerate() {
        let i = from + i;
        if escaped {
            escaped = false;
            continue;
        }
        if in_class {
            if b == b']' {
                in_class = false;
            }
            continue;
        }
        match b {
            b'\\' => escaped = true,
            b'[' => in_class = true,
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b'|' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Match one alternation branch as a sequence of elements.
fn match_seq(
    pat: &[u8],
    t: &[u8],
    t0: usize,
    k: &mut dyn FnMut(&[u8]) -> bool,
    steps: &Cell<u32>,
) -> bool {
    if !spend(steps) {
        return false;
    }
    if pat.is_empty() {
        return k(t);
    }
    let (atom, rest) = match parse_atom(pat) {
        Some(parsed) => parsed,
        None => return false,
    };
    if let Some(quant) = quantifier(rest) {
        let rest = &rest[1..];
        return match atom {
            Atom::Single(kind) => rep_single(&kind, quant, rest, t, t0, k, steps),
            Atom::Group(content) => rep_group(content, quant, rest, t, t0, k, steps),
            // A quantified anchor: refused at `check`; never matches here.
            Atom::Start | Atom::End => false,
        };
    }
    match atom {
        Atom::Single(kind) => match t.first() {
            Some(&c) if match_single(&kind, c) => match_seq(rest, &t[1..], t0, k, steps),
            _ => false,
        },
        Atom::Start => t.len() == t0 && match_seq(rest, t, t0, k, steps),
        Atom::End => t.is_empty() && match_seq(rest, t, t0, k, steps),
        Atom::Group(content) => {
            let k = &mut *k;
            match_expr(
                content,
                t,
                t0,
                &mut |t2| match_seq(rest, t2, t0, k, steps),
                steps,
            )
        }
    }
}

/// Repeat a single-byte atom greedily with backtracking: consume as many
/// as `max` admits, yield back down to `min`, then continue the sequence.
fn rep_single(
    kind: &SingleKind<'_>,
    quant: (usize, Option<usize>),
    rest: &[u8],
    t: &[u8],
    t0: usize,
    k: &mut dyn FnMut(&[u8]) -> bool,
    steps: &Cell<u32>,
) -> bool {
    let (min, max) = quant;
    if !spend(steps) {
        return false;
    }
    if max.map(|m| m > 0).unwrap_or(true)
        && let Some(&c) = t.first()
        && match_single(kind, c)
        && rep_single(
            kind,
            (min.saturating_sub(1), max.map(|m| m - 1)),
            rest,
            &t[1..],
            t0,
            k,
            steps,
        )
    {
        return true;
    }
    if min == 0 {
        return match_seq(rest, t, t0, k, steps);
    }
    false
}

/// Repeat a grouped atom greedily with backtracking. The zero-width guard
/// (a repetition extends only through matches that consume something)
/// keeps the walk bounded.
fn rep_group(
    content: &[u8],
    quant: (usize, Option<usize>),
    rest: &[u8],
    t: &[u8],
    t0: usize,
    k: &mut dyn FnMut(&[u8]) -> bool,
    steps: &Cell<u32>,
) -> bool {
    let (min, max) = quant;
    if !spend(steps) {
        return false;
    }
    if max.map(|m| m > 0).unwrap_or(true) {
        let k = &mut *k;
        let extended = match_expr(
            content,
            t,
            t0,
            &mut |t2| {
                t2.len() < t.len()
                    && rep_group(
                        content,
                        (min.saturating_sub(1), max.map(|m| m - 1)),
                        rest,
                        t2,
                        t0,
                        k,
                        steps,
                    )
            },
            steps,
        );
        if extended {
            return true;
        }
    }
    if min == 0 {
        return match_seq(rest, t, t0, k, steps);
    }
    false
}

/// Parse one element off the front of `pat`: `(atom, rest)`. `None` is a
/// compile error (stray quantifier or `)`, unbalanced group, trailing
/// escape, unterminated class).
fn parse_atom(pat: &[u8]) -> Option<(Atom<'_>, &[u8])> {
    Some(match *pat.first()? {
        b'\\' => {
            let c = *pat.get(1)?;
            (Atom::Single(SingleKind::Byte(c)), &pat[2..])
        }
        b'.' => (Atom::Single(SingleKind::Any), &pat[1..]),
        b'^' => (Atom::Start, &pat[1..]),
        b'$' => (Atom::End, &pat[1..]),
        b'*' | b'+' | b'?' | b')' => return None,
        b'[' => {
            let end = class_end(pat)?;
            (
                Atom::Single(SingleKind::Class { pat, end }),
                &pat[end + 1..],
            )
        }
        b'(' => {
            let end = group_end(pat)?;
            (Atom::Group(&pat[1..end]), &pat[end + 1..])
        }
        c => (Atom::Single(SingleKind::Byte(c)), &pat[1..]),
    })
}

/// Byte offset of the `]` closing the class whose `[` is `pat[0]`.
/// POSIX rules: a leading `^` negates; a `]` in first-item position is a
/// literal; `\` is an ordinary byte inside a class.
fn class_end(pat: &[u8]) -> Option<usize> {
    let mut i = 1usize;
    if pat.get(i) == Some(&b'^') {
        i += 1;
    }
    if pat.get(i) == Some(&b']') {
        i += 1;
    }
    while i < pat.len() {
        if pat[i] == b']' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Membership test for the class span `class_ = "[...]"` against `b`.
fn class_matches(class_: &[u8], b: u8) -> bool {
    let end = match class_end(class_) {
        Some(e) => e,
        None => return false,
    };
    let body = &class_[1..end];
    let (neg, items) = match body.first() {
        Some(&b'^') => (true, &body[1..]),
        Some(_) => (false, body),
        None => return false,
    };
    let mut hit = false;
    let mut i = 0usize;
    if items.first() == Some(&b']') {
        // Leading `]` is a literal member, not the class terminator.
        if b == b']' {
            hit = true;
        }
        i = 1;
    }
    while i < items.len() {
        if i + 2 < items.len() && items[i + 1] == b'-' {
            // Range item `a-z` (a `-` at either end falls through to the
            // literal arm).
            if items[i] <= b && b <= items[i + 2] {
                hit = true;
            }
            i += 3;
        } else {
            if items[i] == b {
                hit = true;
            }
            i += 1;
        }
    }
    hit != neg
}

/// Byte offset of the `)` closing the group whose `(` is `pat[0]`.
/// Escapes and class spans are transparent to the depth count.
fn group_end(pat: &[u8]) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_class = false;
    let mut escaped = false;
    for (i, &b) in pat.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_class {
            if b == b']' {
                in_class = false;
            }
            continue;
        }
        match b {
            b'\\' => escaped = true,
            b'[' => in_class = true,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Compile validation (`regcomp`): a structural walk with no text.
/// `Ok` at `check` means every later `matches` on this pattern is
/// error-free.
fn valid_expr(pat: &[u8]) -> bool {
    let mut start = 0usize;
    loop {
        let end = next_alt_cut(pat, start).unwrap_or(pat.len());
        if !valid_seq(&pat[start..end]) {
            return false;
        }
        if end == pat.len() {
            return true;
        }
        start = end + 1;
    }
}

fn valid_seq(pat: &[u8]) -> bool {
    let mut rest = pat;
    while !rest.is_empty() {
        let (atom, after) = match parse_atom(rest) {
            Some(parsed) => parsed,
            None => return false,
        };
        if quantifier(after).is_some() {
            match atom {
                Atom::Single(_) | Atom::Group(_) => {}
                // Quantified anchor (`^*`, `$?`): regcomp-grade error.
                Atom::Start | Atom::End => return false,
            }
        }
        if let Atom::Group(content) = atom {
            if !valid_expr(content) {
                return false;
            }
        }
        rest = if quantifier(after).is_some() {
            &after[1..]
        } else {
            after
        };
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lane helper: stuff `s` into an 80-byte lane.
    fn lane(s: &str) -> [u8; DS_MAX_KEYLEN] {
        let mut lane = [0u8; DS_MAX_KEYLEN];
        lane[..s.len()].copy_from_slice(s.as_bytes());
        lane
    }

    fn m(pat: &str, key: &str) -> bool {
        EreMatcher.matches(&lane(pat), &lane(key))
    }

    fn ok(pat: &str) -> bool {
        EreMatcher.check(&lane(pat)).is_ok()
    }

    #[test]
    fn test_real_client_patterns_match() {
        // The live Minix3 subscribers, verbatim (VFS main.c:441,
        // input input.c:665, filter main.c:385) — the roads A-2's
        // deferral used to block.
        assert!(m("drv\\.inp\\..*", "drv.inp.kbd"));
        assert!(m("drv\\.inp\\..*", "drv.inp.mouse.x"));
        assert!(!m("drv\\.inp\\..*", "drv.inp"));
        assert!(!m("drv\\.inp\\..*", "drvinp.x"));
        assert!(m("drv\\.[bc]..\\..*", "drv.bcm.mouse"));
        assert!(!m("drv\\.[bc]..\\..*", "drv.icm.mouse"));
        assert!(!m("drv\\.[bc]..\\..*", "drv.bc.x"));
        assert!(m("drv\\.blk\\..*", "drv.blk.sda"));
        assert!(!m("drv\\.blk\\..*", "drv.nblk.sda"));
    }

    #[test]
    fn test_literal_is_anchored_exact() {
        // A literal pattern is C's anchored literal: one key, whole.
        assert!(m("clk", "clk"));
        assert!(!m("clk", "cl"));
        assert!(!m("clk", "ckl"));
        assert!(!m("clk", "xclk"));
        // NUL stop: residue past the terminator never joins (04).
        let mut pat = lane("clk");
        pat[4] = b'x'; // "clk\0x" — residue behind the NUL is invisible
        let key = lane("clk");
        assert!(EreMatcher.matches(&pat, &key));
    }

    #[test]
    fn test_dot_star_plus_question() {
        assert!(m("a.*b", "ab"));
        assert!(m("a.*b", "axb"));
        assert!(m("a.*b", "axxb"));
        assert!(!m("a.*b", "axxby")); // trailing `y` unconsumed
        assert!(!m("a.*b", "a"));
        assert!(m("a+b", "ab"));
        assert!(m("a+b", "aaab"));
        assert!(!m("a+b", "b"));
        assert!(m("a?b", "b"));
        assert!(m("a?b", "ab"));
        assert!(!m("a?b", "aab"));
    }

    #[test]
    fn test_classes() {
        assert!(m("[abc]x", "bx"));
        assert!(!m("[abc]x", "dx"));
        assert!(m("[^abc]x", "dx"));
        assert!(!m("[^abc]x", "ax"));
        assert!(m("[a-z]+x", "qrx"));
        assert!(!m("[a-z]+x", "qRx"));
        // `-` at either end is a literal; `]` first in class is literal.
        assert!(m("[-a]x", "-x"));
        assert!(m("[]x]", "]"));
        assert!(m("[]x]", "x"));
        assert!(!m("[]x]", "["));
    }

    #[test]
    fn test_groups_alternation() {
        assert!(m("(a|b)+c", "abac"));
        assert!(!m("(a|b)+c", "abcx"));
        assert!(m("(ab|a)b", "abb"));
        assert!(m("(ab|a)b", "ab"));
        assert!(m("x(a|b|)y", "xy"));
        assert!(m("x(a|b|)y", "xay"));
        assert!(!m("x(a|b|)y", "xcy"));
        assert!(m("(dev)?disk\\..*", "disk.0"));
        assert!(m("(dev)?disk\\..*", "devdisk.0"));
    }

    #[test]
    fn test_anchors_and_escapes() {
        // The engine anchors by construction; ^ and $ are the assertions
        // C's added anchors state (redundant here, harmless as in C).
        assert!(m("^clk", "clk"));
        assert!(m("clk$", "clk"));
        assert!(!m("a^b", "ab")); // mid-anchor: unreachable, as in glibc
        assert!(m("a\\.b", "a.b"));
        assert!(!m("a\\.b", "axb"));
        assert!(m("a\\*b", "a*b"));
        assert!(!m("a\\*b", "aab"));
    }

    #[test]
    fn test_check_rejects_compile_errors() {
        // regcomp's EINVAL roads (store.c:493-498), mirrored.
        assert!(!ok("a**")); // nested quantifier
        assert!(!ok("*a")); // quantifier with no atom
        assert!(!ok("(ab")); // unbalanced group
        assert!(!ok("ab)")); // stray close
        assert!(!ok("[ab")); // unterminated class
        assert!(!ok("a\\")); // trailing escape
        assert!(!ok("^*")); // quantified anchor
        // Well-formed patterns compile — including empty branches and the
        // empty pattern (C: "^$" matches only the empty key).
        assert!(ok(""));
        assert!(ok("(a|)"));
        assert!(ok("a?"));
    }

    #[test]
    fn test_step_budget_terminates_pathological() {
        // An adversarial pattern must return false, not spin (the budget
        // is a defensive bound; subscribed patterns are validated first).
        let mut pattern = lane("(a|a)*b");
        let mut key = lane("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"); // 31 a's, no b
        assert!(!EreMatcher.matches(&mut pattern, &key));
    }
}
