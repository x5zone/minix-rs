//! Substitution command parsing and application for `sed`.
//!
//! Ground truth: `minix3/usr.bin/sed/compile.c` (the `s` command dispatch
//! at line 122, address parsing from line 189, `compile_subst` at line 480)
//! and `minix3/usr.bin/sed/process.c` (substitution execution around lines
//! 403 to 432, including the empty match advance that keeps global
//! substitution from looping forever).
//!
//! A substitution has the shape `s<delim>pattern<delim>replacement<delim>flags`
//! where the delimiter is any character (the classic slash is convention,
//! not law) and flags mix `g` (replace every match, not just the first),
//! `p` (print the line when a replacement happened), and a decimal number
//! (replace only the nth match). Backslash escapes the delimiter inside
//! either half. The replacement replays the whole match for `&` and group
//! `n` for `\n` (a backslash before any other character yields that
//! character).

use crate::RegexError;
use crate::pattern::{Captures, Pattern};

/// Maximum pattern plus replacement bytes handled in one command.
pub const MAX_SUBST_TEXT: usize = 256;

/// Which matches a substitution replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstScope {
    /// Replace the first match only (no flag).
    First,
    /// Replace every match (`g`).
    Global,
    /// Replace only the nth match (1 based).
    Nth(u32),
}

/// One parsed `s` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subst<'a> {
    /// The pattern text between the first two delimiters.
    pub pattern_text: &'a str,
    /// The replacement text between the last two delimiters.
    pub replacement: &'a str,
    /// Which matches to replace.
    pub scope: SubstScope,
    /// The `p` flag: print the line when a replacement happened.
    pub print: bool,
}

/// Parse one `s` command (the leading `s` already consumed).
///
/// The byte after `s` is the delimiter. Returns the substitution plus how
/// many bytes of `text` it consumed, so the caller can continue with
/// chained commands (`s/a/b/;s/c/d/`).
pub fn parse_subst<'a>(text: &'a str) -> Result<(Subst<'a>, usize), RegexError> {
    let bytes = text.as_bytes();
    let delim = *bytes.first().ok_or(RegexError::InvalidPattern)?;
    let mut pattern_text: Option<&'a str> = None;
    let mut replacement: Option<&'a str> = None;
    let mut pos = 1;
    for slot in [&mut pattern_text, &mut replacement].into_iter() {
        let start = pos;
        loop {
            if pos >= bytes.len() {
                return Err(RegexError::InvalidPattern);
            }
            if bytes[pos] == b'\\' {
                pos += 2;
                continue;
            }
            if bytes[pos] == delim {
                *slot = Some(&text[start..pos]);
                pos += 1;
                break;
            }
            pos += 1;
        }
    }
    let (Some(pattern_text), Some(replacement)) = (pattern_text, replacement) else {
        return Err(RegexError::InvalidPattern);
    };
    let mut scope = SubstScope::First;
    let mut print = false;
    let mut consumed = pos;
    while consumed < bytes.len() {
        match bytes[consumed] {
            b'g' => {
                scope = SubstScope::Global;
                consumed += 1;
            }
            b'p' => {
                print = true;
                consumed += 1;
            }
            b'0'..=b'9' => {
                let mut number: u32 = 0;
                while consumed < bytes.len() && bytes[consumed].is_ascii_digit() {
                    number = number
                        .checked_mul(10)
                        .and_then(|v| v.checked_add((bytes[consumed] - b'0') as u32))
                        .ok_or(RegexError::InvalidPattern)?;
                    consumed += 1;
                }
                if number == 0 {
                    return Err(RegexError::InvalidPattern);
                }
                scope = SubstScope::Nth(number);
            }
            b';' | b'\n' | b' ' | b'\t' => break,
            _ => return Err(RegexError::InvalidPattern),
        }
    }
    if pattern_text.len() + replacement.len() > MAX_SUBST_TEXT {
        return Err(RegexError::TooComplex);
    }
    Ok((
        Subst {
            pattern_text,
            replacement,
            scope,
            print,
        },
        consumed,
    ))
}

/// Apply one substitution to `haystack`, writing the result into `out`.
///
/// Returns `(bytes_written, replacement_happened)`. Global substitution
/// advances past empty matches by one character, mirroring
/// `process.c:418-422`, so `s/x*/-/g` terminates. Output truncates (never
/// overflows) when the result exceeds `out`.
///
/// The haystack is **bytes**, not a `&str`, because a chained script feeds one
/// command's output straight into the next: C carries the pattern space as a
/// byte buffer (`process.c` `SPACE` with `ps`/`psl`), and re-decoding between
/// commands would turn any split multibyte sequence into a question mark or
/// drop the line. Matching itself was already byte-oriented
/// ([`Pattern::find_bytes`]); this only stops the carrier from pretending
/// otherwise.
pub fn apply(
    pattern: &Pattern,
    subst: &Subst<'_>,
    haystack: &[u8],
    out: &mut [u8],
) -> (usize, bool) {
    let bytes = haystack;
    let mut written = 0usize;
    let mut emit = |slice: &[u8]| {
        for byte in slice {
            if written >= out.len() {
                break;
            }
            out[written] = *byte;
            written += 1;
        }
    };
    let mut pos = 0;
    let mut ordinal = 0u32;
    let mut replaced = false;
    while pos <= bytes.len() {
        let Some((start, end, caps)) = pattern.find_bytes(bytes, pos) else {
            emit(&bytes[pos..]);
            break;
        };
        ordinal += 1;
        let wanted = match subst.scope {
            SubstScope::First => ordinal == 1,
            SubstScope::Global => true,
            SubstScope::Nth(n) => ordinal == n,
        };
        emit(&bytes[pos..start]);
        if wanted {
            replaced = true;
            splice_replacement(subst.replacement, &caps, bytes, &mut emit);
        } else {
            emit(&bytes[start..end]);
        }
        if end == start {
            // Empty match: copy one character raw so a global walk always
            // makes progress (process.c:418-422).
            if start < bytes.len() {
                let advance = char_len_at(bytes, start);
                emit(&bytes[start..start + advance]);
                pos = start + advance;
            } else {
                pos = start + 1;
            }
        } else {
            pos = end;
        }
        if wanted && !matches!(subst.scope, SubstScope::Global) {
            emit(&bytes[pos..]);
            break;
        }
    }
    (written, replaced)
}

/// Copy `replacement` to `emit`, replaying `&` (whole match) and `\n`
/// (group n) from `caps` against `haystack`. Unset groups expand to
/// nothing; a backslash before a non digit yields that character.
fn splice_replacement(
    replacement: &str,
    caps: &Captures,
    haystack: &[u8],
    emit: &mut dyn FnMut(&[u8]),
) {
    let bytes = replacement.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'&' => {
                if let Some((start, end)) = caps.spans[0] {
                    emit(&haystack[start as usize..end as usize]);
                }
                index += 1;
            }
            b'\\' if index + 1 < bytes.len() => {
                let next = bytes[index + 1];
                if next.is_ascii_digit() {
                    let slot = (next - b'0') as usize;
                    if let Some(Some((start, end))) = caps.spans.get(slot) {
                        emit(&haystack[*start as usize..*end as usize]);
                    }
                    index += 2;
                } else {
                    emit(&bytes[index + 1..index + 2]);
                    index += 2;
                }
            }
            _ => {
                emit(&bytes[index..index + 1]);
                index += 1;
            }
        }
    }
}

/// Length of the character at `pos` (multibyte aware, never zero).
fn char_len_at(bytes: &[u8], pos: usize) -> usize {
    if pos >= bytes.len() {
        return 1;
    }
    let lead = bytes[pos];
    if lead < 0x80 {
        1
    } else if lead >> 5 == 0b110 {
        2.min(bytes.len() - pos)
    } else if lead >> 4 == 0b1110 {
        3.min(bytes.len() - pos)
    } else if lead >> 3 == 0b11110 {
        4.min(bytes.len() - pos)
    } else {
        1
    }
}

/// Cap on commands one script may fill — the crate stays heap-free (fixed
/// arrays, no `alloc`), mirroring the engine's other size limits.
pub const MAX_SCRIPT_COMMANDS: usize = 16;

/// One command in a `;`/newline-separated sed script (C `compile.c` reads
/// commands one at a time; separators `:165`/`:205`). The no-address common
/// verbs are modelled; an address prefix or any other verb yields
/// `ScriptError::NotModelled` so callers fail loudly rather than silently
/// misbehave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command<'a> {
    /// `s<delim>re<delim>rep<delim>flags`.
    Subst(Subst<'a>),
    /// `d` — pattern space deleted, no auto-print this cycle.
    Delete,
    /// `p` — print the current pattern space.
    Print,
    /// `=` — print the current line number on its own line.
    PrintLineNum,
}

/// Errors while splitting a script into commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptError {
    /// An address prefix (`/re/`, a line number, `$`), a verb beyond
    /// `s`/`d`/`p`/`=`, trailing bytes, or more than `MAX_SCRIPT_COMMANDS`
    /// commands — not modelled by this build.
    NotModelled,
    /// A malformed `s` command (bad delimiters/escapes).
    BadSubst,
}

/// Split a sed script into [`Command`]s written into the caller's `out` slice
/// (heap-free, like the rest of the crate), returning the count. Purely the
/// no-address subset: `s…`, `d`, `p`, `=`.
///
/// The walk follows C's `compile_stream` **one command at a time** rather than
/// cutting the text on `;` first. `compile.c:177-184`'s `semicolon:` label eats
/// blanks, swallows a doubled separator, treats `#` as "this line is finished",
/// and only then reads a command; each command's own reader consumes its
/// delimiters (`compile_subst` stops only at the delimiter), so a `;` inside
/// `s/a;b/x/` is pattern text and must not split anything. Cutting the whole
/// script up front would read `s/a;b/x/` as two broken commands.
///
/// After a command, blanks and tabs are eaten and a separator, a line end, or
/// the end of the script must follow — C's `compile_flags` return points and the
/// `EMPTY` case's `switch` (`compile.c:251-266`) accept exactly those, and its
/// `default` arm is the fatal `extra characters at the end of %c command`. Any
/// other byte here is refused as `ScriptError::NotModelled`, the same loud
/// rejection this shell gives for an address or an unmodelled verb.
pub fn parse_script<'a>(script: &'a str, out: &mut [Command<'a>]) -> Result<usize, ScriptError> {
    let bytes = script.as_bytes();
    let mut index = 0usize;
    let mut count = 0usize;
    while index < bytes.len() {
        index = skip_blanks(bytes, index);
        if index >= bytes.len() {
            break;
        }
        match bytes[index] {
            b';' => {
                index += 1;
                continue;
            }
            b'#' => {
                // C's `if (*p == '#') continue;` goes back for the **next line**,
                // so everything after the sign on this one is dropped — including
                // any separator that would have started another command.
                let Some(at) = bytes[index..].iter().position(|&byte| byte == b'\n') else {
                    break;
                };
                index += at;
                continue;
            }
            _ => {}
        }
        let command = match bytes[index] {
            b's' => {
                let (subst, consumed) =
                    parse_subst(&script[index + 1..]).map_err(|_| ScriptError::BadSubst)?;
                index += 1 + consumed;
                Command::Subst(subst)
            }
            byte @ (b'd' | b'p' | b'=') => {
                index += 1;
                match byte {
                    b'd' => Command::Delete,
                    b'p' => Command::Print,
                    _ => Command::PrintLineNum,
                }
            }
            // `/re/`, a digit, `$` → address; any other verb → unmodelled.
            _ => return Err(ScriptError::NotModelled),
        };
        index = skip_spaces(bytes, index);
        if index < bytes.len() && !matches!(bytes[index], b';' | b'\n') {
            return Err(ScriptError::NotModelled);
        }
        *out.get_mut(count).ok_or(ScriptError::NotModelled)? = command;
        count += 1;
    }
    Ok(count)
}

/// Blanks that separate commands within a line, C's `isspace` under the C
/// locale (`compile.c:152-156`'s `EATSPACE`). A newline is **not** eaten here:
/// in C each script line arrives as its own buffer, so a line end always means
/// "this line is over", never "keep looking on the next one".
fn skip_blanks(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(&byte) if matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
    {
        index += 1;
    }
    index
}

/// The blanks C eats between a command and its separator (`compile.c:253` and
/// `:347`'s `EATSPACE()`), restricted to what can sit on one line.
fn skip_spaces(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(&b' ' | &b'\t')) {
        index += 1;
    }
    index
}

/// Whether the script asks for quiet through C's `#n` line. `cu_fgets` tests the
/// first two bytes of each compilation unit (`main.c:286-287`,
/// `if (linenum == 0 && s[0] == '#' && s[1] == 'n') nflag = 1;`), so the line is
/// both a comment **and** an option switch — a scanner that skipped `#` lines
/// without this test would silently lose the switch.
pub fn script_quiet_directive(script: &str) -> bool {
    script.starts_with("#n")
}

/// What one command contributes to the cycle a line is going through. C keeps
/// these decisions inside `process()`'s `switch` with a `pd` flag ("pattern
/// space deleted") that the print statements read; pulling them out into one
/// pure step lets the rules be gated without a process, and the execution layer
/// only has to carry `pd` and the pattern space across the steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycleStep {
    /// Emit the line number on its own line: C's `case '='` (`process.c:264-265`), which checks **neither** `nflag` nor `pd`.
    pub print_number: bool,
    /// Emit the pattern space as it stands after this command: C's `OUT()` in
    /// `substitute()` for the `p` flag (`process.c:466-467`, reached only when a
    /// replacement happened) and `case 'p'` (`process.c:191-194`).
    pub print_line: bool,
    /// The pattern space is deleted, which ends the cycle at `new:` and skips
    /// the automatic print: C's `case 'd': pd = 1; goto new;`
    /// (`process.c:142-144`).
    pub ends_cycle: bool,
}

/// Decide one step. `replaced` is what C's `substitute()` returned — the `p`
/// flag prints only when a replacement actually happened, and the empty-pattern
/// walk that found nothing prints nothing even with `-n` off.
#[must_use]
pub fn step(command: &Command, replaced: bool) -> CycleStep {
    match command {
        Command::Subst(subst) => CycleStep {
            print_number: false,
            print_line: subst.print && replaced,
            ends_cycle: false,
        },
        Command::Delete => CycleStep {
            print_number: false,
            print_line: false,
            ends_cycle: true,
        },
        Command::Print => CycleStep {
            print_number: false,
            print_line: true,
            ends_cycle: false,
        },
        Command::PrintLineNum => CycleStep {
            print_number: true,
            print_line: false,
            ends_cycle: false,
        },
    }
}

/// The automatic print at the end of a cycle: C's `new: if (!nflag && !pd) OUT();`
/// (`process.c:270`). Both conditions are load-bearing: `-n` turns the automatic
/// print off for the whole run, and a deleted pattern space prints nothing even
/// without `-n`.
#[must_use]
pub fn auto_print(quiet: bool, deleted: bool) -> bool {
    !quiet && !deleted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::{compile_basic, compile_extended};

    fn run(pattern_text: &str, extended: bool, command: &str, line: &str) -> (String, bool) {
        let pattern = if extended {
            compile_extended(pattern_text).unwrap()
        } else {
            compile_basic(pattern_text).unwrap()
        };
        let (subst, _) = parse_subst(command).unwrap();
        let mut out = [0u8; 512];
        let (len, replaced) = apply(&pattern, &subst, line.as_bytes(), &mut out);
        (String::from_utf8_lossy(&out[..len]).into_owned(), replaced)
    }

    #[test]
    fn test_first_only_by_default() {
        let (text, replaced) = run("o", false, "/o/0/", "foo boo");
        assert_eq!(text, "f0o boo");
        assert!(replaced);
    }

    #[test]
    fn test_global_flag() {
        let (text, _) = run("o", false, "/o/0/g", "foo boo");
        assert_eq!(text, "f00 b00");
    }

    #[test]
    fn test_nth_match_only() {
        // Matches sit at 1, 2, 5; replacing the second one turns
        // "foo boo" into "fo0 boo".
        let (text, _) = run("o", false, "/o/0/2", "foo boo");
        assert_eq!(text, "fo0 boo");
    }

    #[test]
    fn test_ampersand_replays_match() {
        let (text, _) = run("o+", true, "/o+/<&>/g", "foo boo");
        assert_eq!(text, "f<oo> b<oo>");
    }

    #[test]
    fn test_group_reference() {
        // Group 1 holds the user, group 2 the host; the replacement swaps
        // them around a dot.
        let (text, _) = run("\\([a-z]*\\)@\\([a-z]*\\)", false, "/@/...\\2.\\1/", "u@h");
        assert_eq!(text, "...h.u");
    }

    #[test]
    fn test_custom_delimiter() {
        let (subst, consumed) = parse_subst("#a#b#g").unwrap();
        assert_eq!(subst.pattern_text, "a");
        assert_eq!(subst.replacement, "b");
        assert_eq!(subst.scope, SubstScope::Global);
        assert_eq!(consumed, 6);
    }

    #[test]
    fn test_print_flag() {
        let (subst, _) = parse_subst("/a/b/p").unwrap();
        assert!(subst.print);
        assert_eq!(subst.scope, SubstScope::First);
    }

    #[test]
    fn test_missing_delimiter_rejected() {
        assert_eq!(parse_subst("/ab"), Err(RegexError::InvalidPattern));
    }

    #[test]
    fn test_zero_nth_rejected() {
        assert_eq!(parse_subst("/a/b/0"), Err(RegexError::InvalidPattern));
    }

    #[test]
    fn test_no_match_copies_input() {
        let (text, replaced) = run("z", false, "/z/q/", "abc");
        assert_eq!(text, "abc");
        assert!(!replaced);
    }

    #[test]
    fn test_empty_match_walk_terminates() {
        let (text, _) = run("x*", false, "/x*/-/g", "ab");
        assert_eq!(text, "-a-b-");
    }

    #[test]
    fn test_parse_script_chains_and_skips_empty() {
        let mut cmds = [Command::Delete; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("s/a/b/", &mut cmds).unwrap(), 1);
        assert!(matches!(cmds[0], Command::Subst(_)));
        assert_eq!(parse_script("s/a/b/g;d;p;=", &mut cmds).unwrap(), 4);
        assert!(matches!(cmds[1], Command::Delete));
        assert!(matches!(cmds[2], Command::Print));
        assert!(matches!(cmds[3], Command::PrintLineNum));
        // Line separators and a trailing separator: empty segments are skipped.
        assert_eq!(parse_script("p;\n", &mut cmds).unwrap(), 1);
        // Leading whitespace is not an address (judged after trim_start).
        assert_eq!(parse_script("  d", &mut cmds).unwrap(), 1);
    }

    #[test]
    fn test_parse_script_rejects_addresses_and_junk() {
        let mut cmds = [Command::Delete; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("1d", &mut cmds), Err(ScriptError::NotModelled));
        assert_eq!(
            parse_script("/a/d", &mut cmds),
            Err(ScriptError::NotModelled)
        );
        assert_eq!(parse_script("$d", &mut cmds), Err(ScriptError::NotModelled));
        assert_eq!(parse_script("q", &mut cmds), Err(ScriptError::NotModelled));
        assert_eq!(parse_script("dx", &mut cmds), Err(ScriptError::NotModelled));
        assert_eq!(
            parse_script("s/a/b/x", &mut cmds),
            Err(ScriptError::BadSubst)
        );
    }

    #[test]
    fn test_semicolon_inside_a_substitution_does_not_split() {
        // C's `compile_subst` only stops at the delimiter, so the `;` here is
        // pattern text; a scanner that cut the script on `;` first would see two
        // broken commands instead of one.
        let mut cmds = [Command::Delete; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("s/a;b/x/", &mut cmds).unwrap(), 1);
        assert!(matches!(cmds[0], Command::Subst(_)));
        // And the separator that follows the finished command still works.
        assert_eq!(parse_script("s/a;b/x/;d", &mut cmds).unwrap(), 2);
        assert!(matches!(cmds[1], Command::Delete));
    }

    #[test]
    fn test_separator_chain_swallows_blanks_and_doubled_separators() {
        // `semicolon:` eats blanks, then a `;`, then goes back for more of the
        // same (`compile.c:177-184`), and a command may be followed by blanks
        // before its separator (`compile.c:253`, `:347`).
        let mut cmds = [Command::Delete; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script(" s/a/b/ ; p ;; d", &mut cmds).unwrap(), 3);
        assert!(matches!(cmds[1], Command::Print));
        assert!(matches!(cmds[2], Command::Delete));
        // Trailing separators and line ends give no command of their own.
        assert_eq!(parse_script("p;\n", &mut cmds).unwrap(), 1);
    }

    #[test]
    fn test_comment_ends_the_line_it_sits_on() {
        // C's `if (*p == '#') continue;` reads the **next line**, so a `;` after
        // the sign never reaches the parser (`compile.c:178-180`).
        let mut cmds = [Command::Delete; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("p;#x;d", &mut cmds).unwrap(), 1);
        // A comment drops the rest of **its own** line, and the command on the
        // next line still arrives. Writing `"p\n#x;d"` would expect two and be
        // wrong: that `d` sits after the sign, so C's `continue` has already
        // abandoned it.
        assert_eq!(parse_script("p\n#x;\nd", &mut cmds).unwrap(), 2);
        assert!(matches!(cmds[1], Command::Delete));
    }

    #[test]
    fn test_quiet_directive_reads_only_the_script_start() {
        // `main.c:286-287` tests the first two bytes of the unit, and the
        // comparison is literal, so `#N` is just a comment.
        assert!(script_quiet_directive("#n"));
        assert!(script_quiet_directive("#n;p"));
        assert!(!script_quiet_directive("p;#n"));
        assert!(!script_quiet_directive("#N"));
        assert!(!script_quiet_directive(" n"));
        // The same text as a script is one comment line: no command at all.
        let mut cmds = [Command::Delete; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("#n;p", &mut cmds).unwrap(), 0);
    }

    #[test]
    fn test_step_carries_the_c_print_rules() {
        let mut cmds = [Command::Delete; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("s/a/b/p", &mut cmds).unwrap(), 1);
        let with_flag = cmds[0];
        assert_eq!(parse_script("s/a/b/", &mut cmds).unwrap(), 1);
        let without_flag = cmds[0];
        // The `p` flag prints only on a replacement (`substitute()` reaches its
        // `OUT()` only after one happened); nothing else about the command prints.
        assert_eq!(
            step(&with_flag, true),
            CycleStep {
                print_number: false,
                print_line: true,
                ends_cycle: false,
            }
        );
        assert_eq!(
            step(&with_flag, false),
            CycleStep {
                print_number: false,
                print_line: false,
                ends_cycle: false,
            }
        );
        assert!(!step(&without_flag, true).print_line);
        // A bare `p` prints whatever stands in the space, and `=` prints the
        // number with no condition on quiet or deleted; `d` ends the cycle.
        assert!(step(&Command::Print, false).print_line);
        assert!(step(&Command::PrintLineNum, false).print_number);
        assert!(step(&Command::Delete, false).ends_cycle);
    }

    #[test]
    fn test_auto_print_needs_both_conditions() {
        // `new: if (!nflag && !pd) OUT();`
        assert!(auto_print(false, false));
        assert!(!auto_print(true, false));
        assert!(!auto_print(false, true));
        assert!(!auto_print(true, true));
    }

    #[test]
    fn test_flag_after_a_space_is_a_known_divergence() {
        // C's `compile_flags` runs `EATSPACE()` at the top of its loop (an
        // extension marked as such at `compile.c:576`), so `s/a/b/ p` sets the
        // `p` flag there. This build stops the flag run at the space and then
        // refuses the trailing text, registered as `SED-FLAG-SPACE-EXTENSION`.
        let mut cmds = [Command::Delete; MAX_SCRIPT_COMMANDS];
        assert_eq!(
            parse_script("s/a/b/ p", &mut cmds),
            Err(ScriptError::NotModelled)
        );
        // Without the letter the blank is only a separator, which is accepted.
        assert_eq!(parse_script("s/a/b/ ;p", &mut cmds).unwrap(), 2);
    }
}
