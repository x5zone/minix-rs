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

use crate::pattern::{Captures, Pattern};

/// Maximum pattern plus replacement bytes handled in one command.
pub const MAX_SUBST_TEXT: usize = 256;

/// The largest occurrence number C accepts in an `s` flag: `s->n` is an `int`
/// (`compile.c:601-607`'s `nval > INT_MAX`), written out so the bound stays a
/// number rather than a cast.
const INT_MAX_FLAG: u32 = 2_147_483_647;

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

/// Why one `s` command could not be read. C's arms are four separate `errx`
/// sentences, so this type names them one by one instead of folding them into a
/// single "bad pattern" answer — the shell can only print what C prints if the
/// failure says which one it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubstError {
    /// The byte after `s` ends the line or is a backslash
    /// (`compile.c:324-326`).
    BadDelimiter,
    /// A delimiter never closed before the text ran out (`compile.c:330-331`).
    Unterminated,
    /// A flag byte outside `g`, `p`, the case letters, `w`, or `1`-`9`
    /// (`compile_flags`' `default:`, `compile.c:636`). A leading `0` lands here
    /// too, because C's digit arm only accepts `1`-`9`.
    BadFlag(u8),
    /// The occurrence number overflowed an `int` (`compile.c:601-607`'s
    /// `errno == ERANGE || nval > INT_MAX`).
    NumberOverflow,
    /// Pattern plus replacement past [`MAX_SUBST_TEXT`]. C has no such cap (it
    /// reads into fixed `_POSIX2_LINE_MAX` buffers), so this arm has no C
    /// sentence to borrow.
    TooLong,
}

/// Parse one `s` command (the leading `s` already consumed).
///
/// The byte after `s` is the delimiter. Returns the substitution plus how
/// many bytes of `text` it consumed, so the caller can continue with
/// chained commands (`s/a/b/;s/c/d/`).
pub fn parse_subst<'a>(text: &'a str) -> Result<(Subst<'a>, usize), SubstError> {
    let bytes = text.as_bytes();
    // C checks the delimiter before reading anything: a line-ending or
    // backslash delimiter is refused on the spot (`compile.c:323-326`).
    let delim = match bytes.first() {
        None => return Err(SubstError::BadDelimiter),
        Some(&b'\n') | Some(&b'\\') => return Err(SubstError::BadDelimiter),
        Some(&byte) => byte,
    };
    let mut pattern_text: Option<&'a str> = None;
    let mut replacement: Option<&'a str> = None;
    let mut pos = 1;
    for slot in [&mut pattern_text, &mut replacement].into_iter() {
        let start = pos;
        loop {
            if pos >= bytes.len() {
                return Err(SubstError::Unterminated);
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
        return Err(SubstError::Unterminated);
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
            b'0' => return Err(SubstError::BadFlag(b'0')),
            b'1'..=b'9' => {
                let mut number: u32 = 0;
                while consumed < bytes.len() && bytes[consumed].is_ascii_digit() {
                    let digit = u32::from(bytes[consumed] - b'0');
                    // C's gate is `errno == ERANGE || nval > INT_MAX`, and `s->n`
                    // is an `int`, so anything past that bound is the overflow
                    // sentence rather than a huge count (`compile.c:601-607`).
                    if number > (INT_MAX_FLAG - digit) / 10 {
                        return Err(SubstError::NumberOverflow);
                    }
                    number = number * 10 + digit;
                    consumed += 1;
                }
                scope = SubstScope::Nth(number);
            }
            b';' | b'\n' => break,
            // C's flag loop runs `EATSPACE()` at the **top** of every iteration
            // (`compile.c:576`, marked EXTENSION there), so blanks between flags
            // and before a separator are skipped and `s/a/b/g p` still sets the
            // `p` flag — stopping here instead would turn that into a refusal.
            b' ' | b'\t' => consumed += 1,
            other => return Err(SubstError::BadFlag(other)),
        }
    }
    if pattern_text.len() + replacement.len() > MAX_SUBST_TEXT {
        return Err(SubstError::TooLong);
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

/// Where an `s` command gets its regular expression.
///
/// C keeps the last expression it ran in `defpreg` and hands that one to a
/// command whose pattern is empty, rather than compiling anything
/// (`process.c:668-671`: `if (preg == NULL) { if (defpreg == NULL) errx(1,
/// "first RE may not be empty"); } else defpreg = preg;`). An empty pattern is
/// therefore **not** "the empty regular expression": this engine refuses an
/// empty pattern outright (`pattern.rs:fn compile`), and reaching that arm would
/// turn `s//x/` into a match-every-position rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternChoice {
    /// The command brings the expression compiled at this index.
    Own(usize),
    /// Empty pattern: run against the expression recorded at this index.
    Reuse(usize),
    /// Empty pattern with nothing recorded yet: C's
    /// `errx(1, "first RE may not be empty")`, which carries no line number or
    /// file name and so has an exact one-line shape.
    NoPrevious,
}

/// Decide which expression one `s` command runs.
///
/// `own` is the index of the expression the command compiled for itself, `None`
/// when its pattern is empty; `recorded` is C's `defpreg` as it stands at the
/// moment the command runs. The second argument is why this is a **runtime**
/// decision: a script text alone does not say which expression an empty pattern
/// will land on, and a command the run never reaches (`d` placed before it) never
/// records or reads anything.
#[must_use]
pub fn pattern_choice(own: Option<usize>, recorded: Option<usize>) -> PatternChoice {
    match (own, recorded) {
        (Some(index), _) => PatternChoice::Own(index),
        (None, Some(index)) => PatternChoice::Reuse(index),
        (None, None) => PatternChoice::NoPrevious,
    }
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

/// One command plus the script line it started on.
///
/// The line travels with the command because C prints it in two different
/// places: the compile-time arms use the `linenum` current while reading
/// (`compile.c:219`, `:264`, `:320`, `:331`, `:366`, `:601`, `:636`), and the
/// execution-time arm restores the command's own saved line before printing
/// (`process.c:387`'s `linenum = cp->u.s->linenum`). A caller that wants C's line
/// has to know which line each command came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scripted<'a> {
    /// The command itself.
    pub command: Command<'a>,
    /// Its 1-based script line.
    pub line: u64,
}

/// Errors while splitting a script into commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptError {
    /// An address prefix (`/re`, a line number, `$`), a verb C has but this
    /// build does not carry, or more than `MAX_SCRIPT_COMMANDS` commands. C
    /// accepts all of these, so there is no C sentence to print and the shell
    /// gives its own.
    NotModelled,
    /// A byte that is not one of C's command codes at all:
    /// `invalid command code %c` (`compile.c:320`). The payload is C's `%c`
    /// argument, a raw byte.
    InvalidCode(u8),
    /// Text left after a verb that takes none, as C's `EMPTY` case `default:`
    /// arm reports it (`compile.c:264-265`). The payload is the **verb**, because
    /// that is what C prints (`cmd->code`), not the leftover byte.
    ExtraChars(u8),
    /// A failure inside one `s` command, which carries its own arms.
    Subst(SubstError),
}

/// A refusal plus the script line it happened on.
///
/// C's compile-time `errx` calls all print `"%lu: %s: <sentence>"` with
/// `linenum` — the script line just read, counted by `cu_fgets`
/// (`main.c:274`, `:292`, `:306`, `:315`) — and `fname`, the script string in
/// quotes. A caller that wants to print what C prints therefore needs the line
/// number with the reason, not just the reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseFailure {
    /// Which refusal.
    pub error: ScriptError,
    /// The 1-based script line the failing command started on.
    pub line: u64,
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
/// After a bare verb (`d`, `p`, `=`) blanks are eaten and a separator, a line
/// end, or the end of the script must follow — C's `EMPTY` case `switch`
/// (`compile.c:251-266`) accepts exactly those, and its `default:` arm is the
/// fatal `extra characters at the end of %c command`. An `s` command cannot reach
/// that check: its own reader ends at a separator, a line end, or the end of the
/// text, and any other byte there is a flag position, which C refuses with
/// `bad flag in substitute command` (`compile.c:636`).
pub fn parse_script<'a>(script: &'a str, out: &mut [Scripted<'a>]) -> Result<usize, ParseFailure> {
    let bytes = script.as_bytes();
    let mut index = 0usize;
    let mut count = 0usize;
    // C's `linenum`, which `cu_fgets` raises once per script line before the
    // command on it is even looked at (`main.c:274`, `:292`, `:306`, `:315`).
    let mut line = 1u64;
    while index < bytes.len() {
        while matches!(bytes.get(index), Some(&byte) if matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        {
            if bytes[index] == b'\n' {
                line += 1;
            }
            index += 1;
        }
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
        // The line this command started on, kept for the refusal messages.
        let command_line = line;
        let command = match bytes[index] {
            b's' => {
                let (subst, consumed) = parse_subst(&script[index + 1..])
                    .map_err(|error| fail(command_line, ScriptError::Subst(error)))?;
                index += 1 + consumed;
                Command::Subst(subst)
            }
            byte @ (b'd' | b'p' | b'=') => {
                index += 1;
                index = skip_spaces(bytes, index);
                if index < bytes.len() && !matches!(bytes[index], b';' | b'\n') {
                    return Err(fail(command_line, ScriptError::ExtraChars(byte)));
                }
                match byte {
                    b'd' => Command::Delete,
                    b'p' => Command::Print,
                    _ => Command::PrintLineNum,
                }
            }
            // `/re`, a digit, `\`, `$` are address starts (`compile.c:193`'s
            // `addrchar`); the rest split by whether C's own table knows them.
            other => {
                return Err(fail(
                    command_line,
                    if is_command_code(other) || matches!(other, b'/' | b'\\' | b'$' | b'0'..=b'9')
                    {
                        ScriptError::NotModelled
                    } else {
                        ScriptError::InvalidCode(other)
                    },
                ));
            }
        };
        *out.get_mut(count)
            .ok_or_else(|| fail(command_line, ScriptError::NotModelled))? =
            Scripted {
                command,
                line: command_line,
            };
        count += 1;
    }
    Ok(count)
}

/// One refusal at a known script line.
fn fail(line: u64, error: ScriptError) -> ParseFailure {
    ParseFailure { error, line }
}

/// Whether C's command table knows this byte (`compile.c:102-131`'s `cmd_fmts`).
/// The distinction is what decides the message: an unknown byte is C's own
/// `invalid command code`, while a known verb this build does not carry has no C
/// error to copy.
fn is_command_code(byte: u8) -> bool {
    matches!(
        byte,
        b'{' | b'}'
            | b'a'..=b'd'
            | b'D'
            | b'g'
            | b'G'
            | b'h'
            | b'H'
            | b'i'
            | b'l'
            | b'n'
            | b'N'
            | b'p'
            | b'P'
            | b'q'
            | b'r'
            | b's'
            | b't'
            | b'w'
            | b'x'
            | b'y'
            | b'!'
            | b':'
    )
}

/// The blanks C eats between a bare verb and its separator (`compile.c:253`'s
/// `EATSPACE()`), restricted to what can sit on one line: in C each script line
/// arrives as its own buffer, so a line end always means "this line is over".
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

/// The back-reference numbers a replacement asks for, in the order C reads them.
///
/// `compile_subst` walks the replacement and treats a backslash followed by
/// `1`-`9` as a reference — that is the only place a number enters
/// `s->maxbref`, so `\\1` (an escaped backslash then a `1`) and `\&` are not
/// references (`compile.c:501-531`). A backslash before anything else yields
/// that character.
fn visit_backrefs(replacement: &str, visit: &mut dyn FnMut(u8)) {
    let bytes = replacement.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] != b'\\' || index + 1 >= bytes.len() {
            index += 1;
            continue;
        }
        let next = bytes[index + 1];
        if matches!(next, b'1'..=b'9') {
            visit(next - b'0');
        }
        // Both arms below consume the pair, so a doubled backslash shields the
        // digit after it exactly as C's `sawesc`-free stepping does.
        index += 2;
    }
}

/// The largest back-reference a replacement asks for: C's `s->maxbref`
/// (`compile.c:529-530`), which is the number C prints when a *reused*
/// expression cannot fill it (`process.c:386-390`).
#[must_use]
pub fn max_backref(replacement: &str) -> u8 {
    let mut largest = 0u8;
    visit_backrefs(replacement, &mut |ref_number| {
        largest = largest.max(ref_number);
    });
    largest
}

/// The first back-reference the expression with `groups` captures cannot fill.
///
/// C tests each digit as it reads the replacement, so it names **that** digit,
/// not the largest one later in the string: `s/\(a\)/\2\3/` complains about
/// `\2` (`compile.c:526-528`).
#[must_use]
pub fn first_undefined_backref(replacement: &str, groups: u8) -> Option<u8> {
    let mut offender: Option<u8> = None;
    visit_backrefs(replacement, &mut |ref_number| {
        if offender.is_none() && ref_number > groups {
            offender = Some(ref_number);
        }
    });
    offender
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
        // C's `compile_delimited` returns NULL and the caller prints
        // `unterminated substitute pattern` (`compile.c:329-331`).
        assert_eq!(parse_subst("/ab"), Err(SubstError::Unterminated));
    }

    #[test]
    fn test_zero_nth_rejected() {
        // C's digit arm accepts `1`-`9` only, so a `0` in the flag run falls to
        // its `default:` — a bad flag, not a bad number (`compile.c:597-636`).
        assert_eq!(parse_subst("/a/b/0"), Err(SubstError::BadFlag(b'0')));
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
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("s/a/b/", &mut cmds).unwrap(), 1);
        assert!(matches!(cmds[0].command, Command::Subst(_)));
        assert_eq!(parse_script("s/a/b/g;d;p;=", &mut cmds).unwrap(), 4);
        assert!(matches!(cmds[1].command, Command::Delete));
        assert!(matches!(cmds[2].command, Command::Print));
        assert!(matches!(cmds[3].command, Command::PrintLineNum));
        // Line separators and a trailing separator: empty segments are skipped.
        assert_eq!(parse_script("p;\n", &mut cmds).unwrap(), 1);
        // Leading whitespace is not an address (judged after trim_start).
        assert_eq!(parse_script("  d", &mut cmds).unwrap(), 1);
    }

    #[test]
    fn test_parse_script_splits_c_own_errors_from_ours() {
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        // Addresses are something C carries, so a refusal here has no C sentence:
        // ours. The four address-start bytes come from `compile.c:193`'s
        // `addrchar("0123456789/\\$")`.
        for script in ["1d", "/a/d", "$d", "\\ad"] {
            assert_eq!(
                parse_script(script, &mut cmds),
                Err(fail(1, ScriptError::NotModelled)),
                "{script:?}"
            );
        }
        // Likewise a verb C's table knows and this build does not carry.
        assert_eq!(
            parse_script("q", &mut cmds),
            Err(fail(1, ScriptError::NotModelled))
        );
        // A byte outside C's table is C's own `invalid command code`
        // (`compile.c:318-320`), carrying the byte for C's `%c`.
        assert_eq!(
            parse_script("Z", &mut cmds),
            Err(fail(1, ScriptError::InvalidCode(b'Z')))
        );
        // Text after a bare verb names the **verb**, as C's `EMPTY` case does
        // (`compile.c:264-265`).
        assert_eq!(
            parse_script("dx", &mut cmds),
            Err(fail(1, ScriptError::ExtraChars(b'd')))
        );
        assert_eq!(
            parse_script("p ; p x", &mut cmds),
            Err(fail(1, ScriptError::ExtraChars(b'p')))
        );
        // C prints the script line the command sat on, so the scan has to carry it:
        // `cu_fgets` raises `linenum` per line (`main.c:274`, `:292`, `:306`).
        assert_eq!(
            parse_script("p\np\n d x", &mut cmds),
            Err(fail(3, ScriptError::ExtraChars(b'd')))
        );
        assert_eq!(
            parse_script("s/a/b/;\nZ", &mut cmds),
            Err(fail(2, ScriptError::InvalidCode(b'Z')))
        );
        // A comment that runs to the end of its own line does not move the count
        // past that line.
        assert_eq!(
            parse_script("#c\ns/a/b/x", &mut cmds),
            Err(fail(2, ScriptError::Subst(SubstError::BadFlag(b'x'))))
        );
    }

    #[test]
    fn test_subst_error_arms() {
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        // A flag letter outside C's set (`compile.c:635-637`).
        assert_eq!(
            parse_script("s/a/b/x", &mut cmds),
            Err(fail(1, ScriptError::Subst(SubstError::BadFlag(b'x'))))
        );
        // C's digit arm takes `1`-`9`, so a leading `0` is a bad flag, not a bad
        // number.
        assert_eq!(
            parse_script("s/a/b/0", &mut cmds),
            Err(fail(1, ScriptError::Subst(SubstError::BadFlag(b'0'))))
        );
        // Past the bound C puts on `s->n` (`compile.c:601-607`).
        assert_eq!(
            parse_script("s/a/b/2147483648", &mut cmds),
            Err(fail(1, ScriptError::Subst(SubstError::NumberOverflow)))
        );
        // Right at the bound is still a count.
        assert_eq!(parse_script("s/a/b/2147483647", &mut cmds).unwrap(), 1);
        // A backslash delimiter (`compile.c:323-326`) and a delimiter that never
        // closes (`compile.c:329-331`).
        assert_eq!(
            parse_script("s\\a/b/", &mut cmds),
            Err(fail(1, ScriptError::Subst(SubstError::BadDelimiter)))
        );
        assert_eq!(
            parse_script("s/a/b", &mut cmds),
            Err(fail(1, ScriptError::Subst(SubstError::Unterminated)))
        );
    }

    #[test]
    fn test_semicolon_inside_a_substitution_does_not_split() {
        // C's `compile_subst` only stops at the delimiter, so the `;` here is
        // pattern text; a scanner that cut the script on `;` first would see two
        // broken commands instead of one.
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("s/a;b/x/", &mut cmds).unwrap(), 1);
        assert!(matches!(cmds[0].command, Command::Subst(_)));
        // And the separator that follows the finished command still works.
        assert_eq!(parse_script("s/a;b/x/;d", &mut cmds).unwrap(), 2);
        assert!(matches!(cmds[1].command, Command::Delete));
    }

    #[test]
    fn test_separator_chain_swallows_blanks_and_doubled_separators() {
        // `semicolon:` eats blanks, then a `;`, then goes back for more of the
        // same (`compile.c:177-184`), and a command may be followed by blanks
        // before its separator (`compile.c:253`, `:347`).
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script(" s/a/b/ ; p ;; d", &mut cmds).unwrap(), 3);
        assert!(matches!(cmds[1].command, Command::Print));
        assert!(matches!(cmds[2].command, Command::Delete));
        // Trailing separators and line ends give no command of their own.
        assert_eq!(parse_script("p;\n", &mut cmds).unwrap(), 1);
    }

    #[test]
    fn test_comment_ends_the_line_it_sits_on() {
        // C's `if (*p == '#') continue;` reads the **next line**, so a `;` after
        // the sign never reaches the parser (`compile.c:178-180`).
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("p;#x;d", &mut cmds).unwrap(), 1);
        // A comment drops the rest of **its own** line, and the command on the
        // next line still arrives. Writing `"p\n#x;d"` would expect two and be
        // wrong: that `d` sits after the sign, so C's `continue` has already
        // abandoned it.
        assert_eq!(parse_script("p\n#x;\nd", &mut cmds).unwrap(), 2);
        assert!(matches!(cmds[1].command, Command::Delete));
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
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("#n;p", &mut cmds).unwrap(), 0);
    }

    #[test]
    fn test_step_carries_the_c_print_rules() {
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("s/a/b/p", &mut cmds).unwrap(), 1);
        let with_flag = cmds[0].command;
        assert_eq!(parse_script("s/a/b/", &mut cmds).unwrap(), 1);
        let without_flag = cmds[0].command;
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
    fn test_pattern_choice_reuses_or_refuses_an_empty_pattern() {
        // Three arms: a pattern of its own, an empty pattern with something
        // recorded, and an empty pattern with nothing recorded yet.
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("s/a/A/;s//B/", &mut cmds).unwrap(), 2);
        let own_pattern = match cmds[0].command {
            Command::Subst(subst) => subst,
            _ => panic!("第一条该是 s"),
        };
        let empty_pattern = match cmds[1].command {
            Command::Subst(subst) => subst,
            _ => panic!("第二条该是 s"),
        };
        assert_eq!(own_pattern.pattern_text, "a");
        assert_eq!(empty_pattern.pattern_text, "");
        // A command that brings its own expression keeps it, whatever is
        // recorded: `s/a/A/;s/b/B/` runs `b` even though `a` was recorded first.
        assert_eq!(pattern_choice(Some(0), None), PatternChoice::Own(0));
        assert_eq!(pattern_choice(Some(3), Some(1)), PatternChoice::Own(3));
        // An empty pattern takes the **last recorded** expression, and the
        // argument is the run's state, not the script text: this is why
        // `s/a/A/;s/b/B/;s//C/` reuses `b` while a `d` before the whole script
        // leaves the run with nothing recorded at all.
        assert_eq!(pattern_choice(None, Some(0)), PatternChoice::Reuse(0));
        assert_eq!(pattern_choice(None, Some(1)), PatternChoice::Reuse(1));
        assert_eq!(pattern_choice(None, None), PatternChoice::NoPrevious);
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
    fn test_backref_scan_follows_the_c_escape_rules() {
        // Only a backslash followed by `1`-`9` is a reference, and each pair is
        // consumed whole, so a doubled backslash shields the digit after it
        // (`compile.c:501-531`).
        assert_eq!(max_backref(r"\1"), 1);
        assert_eq!(max_backref(r"\\1"), 0);
        assert_eq!(max_backref(r"\1\\2"), 1);
        assert_eq!(max_backref(r"\1\2\1"), 2);
        assert_eq!(max_backref(r"\11"), 1);
        // `&` is the whole match, `\0` is not a reference slot at all.
        assert_eq!(max_backref("&"), 0);
        assert_eq!(max_backref(r"\0"), 0);
        assert_eq!(max_backref(""), 0);
        // The offender C names is the **first** digit over the bound, not the
        // largest: `\2\3` against one group reports `\2`.
        assert_eq!(first_undefined_backref(r"\2\3", 1), Some(2));
        assert_eq!(first_undefined_backref(r"\3\2", 1), Some(3));
        assert_eq!(first_undefined_backref(r"\1\2", 2), None);
        assert_eq!(first_undefined_backref(r"\\1", 0), None);
        assert_eq!(first_undefined_backref("", 0), None);
    }

    #[test]
    fn test_blanks_between_flags_still_belong_to_the_flags() {
        // C's `compile_flags` runs `EATSPACE()` at the top of its loop (the
        // extension marked as such at `compile.c:576`), so a blank before a flag
        // letter does not end the flag run: `s/a/b/ g` is `g`, `s/a/b/ p` is the
        // print flag. A shell that stopped the run at the blank would refuse both.
        let mut cmds = [Scripted {
            command: Command::Delete,
            line: 1,
        }; MAX_SCRIPT_COMMANDS];
        assert_eq!(parse_script("s/a/b/ g", &mut cmds).unwrap(), 1);
        let Command::Subst(subst) = cmds[0].command else {
            panic!("一条 s 命令都读不出来");
        };
        assert_eq!(subst.scope, SubstScope::Global);
        assert!(!subst.print);
        assert_eq!(parse_script("s/a/b/ p;d", &mut cmds).unwrap(), 2);
        let Command::Subst(subst) = cmds[0].command else {
            panic!("第一条该是 s");
        };
        assert!(subst.print);
        assert!(matches!(cmds[1].command, Command::Delete));
        // The same blank before a separator is only a separator.
        assert_eq!(parse_script("s/a/b/ ;p", &mut cmds).unwrap(), 2);
        // Trailing blanks after the flags run out are nothing at all.
        assert_eq!(parse_script("s/a/b/  ", &mut cmds).unwrap(), 1);
    }
}
