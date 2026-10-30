//! Head/tail line windows behind one trait.
//!
//! Ground truth: `head` keeps the first N lines, `tail` the last N. Both
//! answer "which lines survive?" from opposite ends, so both implement
//! [`LineWindow`]. The caller feeds lines in order and drains the survivors
//! at the end; the window never holds more than N borrowed lines.
//!
//! N is the caller's count, with no ceiling of its own: C `head.c` counts down
//! from it and C `forward.c` + `RLINES` walks back through the mapped file for
//! any of them. The storage therefore grows as lines arrive and stops at N, so a
//! `tail -n 1000000` on a twenty-line file holds twenty lines, not a million
//! slots — the count cannot be turned into an allocation bomb.

use crate::TextError;

use alloc::vec::Vec;

/// One line: the content (**as bytes**) plus whether the input actually had that
/// newline.
///
/// C's line legs return the trailing newline together with the content and never add
/// one on the input's behalf: `fgets` at `nl.c:288` reads the newline into the buffer
/// and `printf("%s%s", sep, buffer)` at `:340` prints it unchanged; `head.c:169-172`
/// copies character by character with `getc`/`putchar`; the line ranges `reverse.c`
/// records at `:133-144` include that newline too. So "the last line has no newline"
/// is a **property of the input**, not a gap the program fills.
///
/// Why the content is `&[u8]` and not `&str`: these C line legs interpret not a single
/// byte — `fgetln`, `fgets` and `getc` return bytes. Once a line becomes a `&str`, an
/// illegal UTF-8 byte forces a decode, and whichever way that decode is resolved it
/// changes the input: failing on the whole input yields nothing at all, failing per
/// line replaces that line with something else. Bytes have no such problem, so the
/// reading side (splitting columns, comparing, counting) and the writing side work on
/// the same bytes.
///
/// An interior line always carries its newline; only the input's last line may lack
/// one. Making the termination state part of the line data means the writing side
/// only looks at [`Line::eol`], and nobody has to ask "is the line I am writing the
/// input's last line" — `tail -r` moves the last line to the front of the output,
/// where asking by line number would get the wrong answer. (Two exceptions: `cut` and
/// `rev` **do** normalize by adding a newline in C.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line<'a> {
    bytes: &'a [u8],
    terminated: bool,
}

impl<'a> Line<'a> {
    /// The content (without the newline) plus "the input had that newline".
    pub fn new(bytes: &'a [u8]) -> Line<'a> {
        Line {
            bytes,
            terminated: true,
        }
    }

    /// Returns a line as the input had it: `terminated` is false when the last line
    /// lacks a newline.
    pub fn raw(bytes: &'a [u8], terminated: bool) -> Line<'a> {
        Line { bytes, terminated }
    }

    /// The line's content, as bytes.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    /// The bytes to append when writing this line: what the input had, and **nothing**
    /// when it had none.
    pub fn eol(&self) -> &'static [u8] {
        if self.terminated { b"\n" } else { b"" }
    }
    /// Whether this line carried a newline in the input.
    pub fn terminated(&self) -> bool {
        self.terminated
    }
}

impl<'a> From<&'a str> for Line<'a> {
    /// Borrows a piece of text as "a line with a newline": the path tests and
    /// content-only callers take.
    fn from(text: &'a str) -> Line<'a> {
        Line::new(text.as_bytes())
    }
}

/// Whether the input's last line carried that newline — a one-line predicate. Legs
/// that split lines themselves byte-wise (the `nl` numbering leg has to tolerate
/// non-UTF-8 and does not use [`Line`]) use it, and `lines_of` uses it to set the last
/// line's `terminated`; both come from here.
/// Empty input has no lines and returns `true` (neither adding nor not adding can
/// reach a line).
pub fn final_line_terminated(input: &[u8]) -> bool {
    input.is_empty() || input.ends_with(b"\n")
}

/// Splits the input byte-wise and returns every line together with its termination
/// state in the input — **no decoding at all**.
///
/// Empty `input` returns an empty table (no lines). When the input ends with a newline,
/// the extra empty piece `split` returns is what comes after the newline, not a line;
/// the empty piece between two adjacent newlines is a real line.
///
/// Why not decode the whole input into a `&str` first and then split: one decode
/// demands "the whole input is valid UTF-8", and a single illegal byte would turn the
/// whole input into an empty string, making every line disappear — while the reference
/// build still returns line by line for the same input (a line containing `\xff` is
/// printed with its bytes). Byte-wise splitting makes the lines the input's own
/// division, independent of whether the content is UTF-8.
pub fn lines_of(input: &[u8]) -> Vec<Line<'_>> {
    if input.is_empty() {
        return Vec::new();
    }
    let mut pieces: Vec<Line<'_>> = input.split(|byte| *byte == b'\n').map(Line::new).collect();
    if final_line_terminated(input) {
        // When the input ends with a newline, `split` returns one extra empty piece:
        // it is what comes **after** the newline, not a line; the interior empty piece
        // (between two adjacent newlines) is still a real line.
        pieces.pop();
    } else if let Some(last) = pieces.last_mut() {
        // The last line has no newline in the input: record that on the line so the
        // writing legs will not add one.
        *last = Line::raw(last.bytes(), false);
    }
    pieces
}

/// One line selection strategy: feed lines in order, drain survivors.
pub trait LineWindow<'a> {
    /// Offer one line (1 based number for diagnostics); survivors stay
    /// inside the window.
    fn push(&mut self, line: Line<'a>);
    /// Survivors in original order; call once after the last push.
    fn drain(&mut self, out: &mut [Line<'a>]) -> usize;
    /// How many lines were offered in total.
    fn seen(&self) -> u64;
}

/// Keep the first N lines (`head -n`).
pub struct HeadWindow<'a> {
    lines: Vec<Line<'a>>,
    kept: usize,
    limit: usize,
    seen: u64,
}

impl<'a> HeadWindow<'a> {
    /// A window keeping the first `n` lines. Only `0` is refused: `head.c`'s
    /// counter never starts, which the callers treat as "print nothing" rather
    /// than as a window request.
    pub fn new(n: usize) -> Result<Self, TextError> {
        if n == 0 {
            return Err(TextError::InvalidArgument);
        }
        Ok(HeadWindow {
            lines: Vec::new(),
            kept: 0,
            limit: n,
            seen: 0,
        })
    }
}

impl<'a> LineWindow<'a> for HeadWindow<'a> {
    fn push(&mut self, line: Line<'a>) {
        self.seen += 1;
        if self.kept < self.limit {
            self.lines.push(line);
            self.kept += 1;
        }
    }

    fn drain(&mut self, out: &mut [Line<'a>]) -> usize {
        let count = self.kept.min(out.len());
        out[..count].copy_from_slice(&self.lines[..count]);
        count
    }

    fn seen(&self) -> u64 {
        self.seen
    }
}

/// Keep the last N lines (`tail -n`): a ring holding the most recent N.
pub struct TailWindow<'a> {
    ring: Vec<Line<'a>>,
    limit: usize,
    seen: u64,
}

impl<'a> TailWindow<'a> {
    /// A window keeping the last `n` lines. `0` is refused (C `forward.c` seeks
    /// to the end of the file in that case and prints nothing, so no window is
    /// needed); there is no upper bound, matching `RLINES`' backward walk.
    pub fn new(n: usize) -> Result<Self, TextError> {
        if n == 0 {
            return Err(TextError::InvalidArgument);
        }
        Ok(TailWindow {
            ring: Vec::new(),
            limit: n,
            seen: 0,
        })
    }
}

impl<'a> LineWindow<'a> for TailWindow<'a> {
    fn push(&mut self, line: Line<'a>) {
        // Fill before wrapping, so a count larger than the input costs the
        // input's size and not the count.
        if self.ring.len() < self.limit {
            self.ring.push(line);
        } else {
            self.ring[(self.seen as usize) % self.limit] = line;
        }
        self.seen += 1;
    }

    fn drain(&mut self, out: &mut [Line<'a>]) -> usize {
        let kept = (self.seen as usize).min(self.limit).min(out.len());
        if kept == 0 {
            return 0;
        }
        // Oldest survivor first: it sits right past the write cursor when
        // the ring wrapped, else at slot zero.
        let start = if (self.seen as usize) > self.limit {
            (self.seen as usize) % self.limit
        } else {
            0
        };
        for index in 0..kept {
            out[index] = self.ring[(start + index) % self.limit];
        }
        kept
    }

    fn seen(&self) -> u64 {
        self.seen
    }
}

/// Parsed `head` options and file operands — the deciding half's front door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadArgs {
    /// `-n` line count, or the default ten.
    pub count: usize,
    /// `-c` byte count; `0` (the default) selects line mode.
    pub bytecnt: usize,
    /// `-q`: never print per-file banners.
    pub qflag: bool,
    /// `-v`: always print per-file banners.
    pub vflag: bool,
    /// File operands in order (a lone `-` names a file, unlike `cut`).
    /// What comes back is the **raw bytes** from argv: the banner leg is assembled by
    /// the shell itself (`printf("%s==> %s <==\\n", …, *argv)` at `head.c:130-131`), so
    /// it can echo byte for byte; only opening the file and the `warn("%s", *argv)`
    /// message go through `&str` and need one lossy fold.
    pub operands: Vec<Vec<u8>>,
}

/// `head` argument-parsing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadArgError {
    /// An option letter outside the set, or a value option missing its value. `head`
    /// has no bare-usage leg of its own (C has exactly one `usage()`, in the
    /// `case '?'`/`default:` arms at `head.c:115-117`), and that arm is preceded by a
    /// line libc prints itself (`getopt.c:101-104`, `:132-136`), so this type keeps a
    /// single exit and adds no `Usage` variant.
    Getopt(crate::GetoptReport),
    /// A `-c`/`-n` value that is not a strictly positive integer: trailing
    /// garbage, `<= 0`, or overflow (C `strtoimax` reject, `head.c:90-103`).
    IllegalCount {
        /// `true` for a `-c` (byte) count, `false` for `-n` (line).
        byte: bool,
        /// The offending value **as the argv bytes**, echoed into C's
        /// `errx(1, "illegal … count -- %s", optarg)` (`head.c:96-97`, `:102-103`).
        value: Vec<u8>,
    },
}

/// Split `head`'s argv tail into [`HeadArgs`], faithful to C's
/// `obsolete(argv)` pre-pass (`head.c:84`, defined `head.c:178-195`) plus
/// `getopt(argc, argv, "c:n:qv")` (`head.c:87`):
/// - `obsolete` rewrites only the **leading run** of `-<digit>…` tokens into
///   `-n<digit>…`; its `while` returns at the first token that is not one (a
///   file, `--`, or a non-digit flag). So `head -5 f` becomes `head -n5 f`,
///   while `head f -5` is not rewritten at all.
/// - `getopt` **does not permute**: it looks at one token per call and, when
///   that token is absent or does not start with `-`, ends option scanning right
///   there (`getopt.c:69-75` returns `-1` with `optind` left in place). The
///   caller then takes `argv + optind` as its operands (`head.c:118-119`), so
///   **every token from the first non-option on is an operand** — a later `-q`
///   never sets `qflag`, a later `-x` never reaches the unknown-letter arm, a
///   later `-n 3` never changes the count, and a later `--` is a file named
///   `--`. Only inside a leading option cluster does `getopt` keep consuming:
///   letters cluster (`-qv` ≡ `-q -v`), a colon option's value attaches to its
///   own token (`-n5`) or is the next token (`-n 5`), a bare `--` as the leading
///   token ends scanning so everything after it is an operand, and a solitary
///   `-` is treated as a non-option (`getopt.c:96-104`: `head` does not list `-`
///   in its option string) and therefore becomes an operand too.
/// - `-q`/`-v` are mutually exclusive with the last one winning
///   (`head.c:105-113`); any other letter inside a leading option token is
///   [`HeadArgError::Getopt`] with that letter, because libc prints its own line
///   before `usage()` runs.
pub fn parse_head_args(args: &[&[u8]]) -> Result<HeadArgs, HeadArgError> {
    // Phase 1 — obsolete(): rewrite the leading `-<digit>…` run, stop at the
    // first non-matching token and copy the remainder verbatim.
    let mut scanned: Vec<Vec<u8>> = Vec::new();
    let mut still_obsolete = true;
    for arg in args {
        if still_obsolete
            && arg.len() >= 2
            && arg[0] == b'-'
            && arg[1] != b'-'
            && arg[1].is_ascii_digit()
        {
            // C's `obsolete()` assembles a character array (`-`, `n` plus
            // `strcpy(ap + 2, *argv + 1)` at `head.c:188-192`), which here is "two
            // prefix bytes plus every byte of the original after the dash" — a
            // non-UTF-8 byte in the tail must travel unchanged, because `strtoimax`
            // then rejects it and it is **echoed into the message** (`illegal ... -- %s`
            // at `head.c:96-97`).
            let mut rewritten: Vec<u8> = b"-n".to_vec();
            rewritten.extend_from_slice(&arg[1..]);
            scanned.push(rewritten);
        } else {
            still_obsolete = false;
            scanned.push((*arg).to_vec());
        }
    }

    // Phase 2 — getopt("c:n:qv").
    let mut result = HeadArgs {
        count: 10,
        bytecnt: 0,
        qflag: false,
        vflag: false,
        operands: Vec::new(),
    };
    let mut index = 0;
    while index < scanned.len() {
        let token: &[u8] = &scanned[index];
        if token == b"--" {
            result.operands.extend_from_slice(&scanned[index + 1..]);
            break;
        }
        let bytes = token;
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'c' | b'n' => {
                        let byte_opt = bytes[pos] == b'c';
                        let value = if pos + 1 < bytes.len() {
                            token[pos + 1..].to_vec()
                        } else {
                            index += 1;
                            if index >= scanned.len() {
                                return Err(HeadArgError::Getopt(
                                    crate::GetoptReport::MissingValue(char::from(bytes[pos])),
                                ));
                            }
                            scanned[index].clone()
                        };
                        // The numeric leg still goes to `str::parse::<i64>`: text that
                        // parses as i64 is necessarily ASCII, so a `from_utf8` failure
                        // and "this is not a number" are the same thing — decoding is
                        // used as a **pure check**, and a non-UTF-8 value takes C's
                        // `illegal ... count` sentence and is **echoed as raw bytes**.
                        match core::str::from_utf8(&value)
                            .ok()
                            .and_then(|text| text.parse::<i64>().ok())
                        {
                            Some(parsed) if parsed > 0 => {
                                if byte_opt {
                                    result.bytecnt = parsed as usize;
                                } else {
                                    result.count = parsed as usize;
                                }
                            }
                            _ => {
                                return Err(HeadArgError::IllegalCount {
                                    byte: byte_opt,
                                    value,
                                });
                            }
                        }
                        pos = bytes.len(); // the value consumed the token's tail
                    }
                    b'q' => {
                        result.qflag = true;
                        result.vflag = false;
                        pos += 1;
                    }
                    b'v' => {
                        result.qflag = false;
                        result.vflag = true;
                        pos += 1;
                    }
                    _ => {
                        return Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                            char::from(bytes[pos]),
                        )));
                    }
                }
            }
        } else {
            // C's `getopt` returns -1 here and does not advance `optind`
            // (`getopt.c:71-74`), so `argv += optind` at `head.c:118-119` takes this
            // token **and everything after it** as operands: a later `-q` never sets
            // the flag, `-x` never reaches the unknown-letter sentence, `-n 3` never
            // changes the count, and `--` is no longer "end of options" but a file
            // named `--`.
            result.operands.extend_from_slice(&scanned[index..]);
            break;
        }
        index += 1;
    }
    Ok(result)
}

/// Parsed `tail` options and file operands — the deciding half's front door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailArgs {
    /// C's `(style, off)` plus `rflag`, already folded (`tail.c:135-164`), so
    /// the execution half only has to pick a printing shape.
    pub style: TailStyle,
    /// C's `fflag` (`extern.h:51`, set by `-f`/`-F` at `tail.c:110-121`).
    pub follow: Follow,
    /// File operands in order; `-` is a literal file name (`tail` opens operands
    /// through `fopen`, unlike `cut`). Bytes, because an operand is both a
    /// `fopen` argument and the text of the `==> … <==` banner (`tail.c:173`).
    pub operands: Vec<Vec<u8>>,
}

/// `tail`'s three follow states, mirroring C's integer global `fflag` (`extern.h:51`):
/// `-f` sets 1, `-F` sets 2, and no flag is 0 (`tail.c:110-121`). All three values are
/// used in different predicates by the ground truth, so they cannot be collapsed into
/// one boolean: 1 only reads new bytes, 2 also recognizes a swapped file by name
/// (`forward.c:255`), and 0 does not even sleep (`:201-202`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow {
    /// No follow option: exit after printing.
    Off,
    /// `-f` (`fflag == 1`): follow the **already-open file** for new bytes and never
    /// reopen by name.
    ByHandle,
    /// `-F` (`fflag == 2`): besides reading new bytes, check the operand's identity by
    /// name each round and reopen when a swap is detected.
    ByName,
}

impl Follow {
    /// Whether following is active at all (C's `if (fflag)`, `if (fflag && argc > 1)`
    /// and `if (rflag) { if (fflag) usage(); }` all ask only this one thing).
    #[must_use]
    pub fn active(self) -> bool {
        self != Follow::Off
    }

    /// Whether this round checks identity by name (C `forward.c:255`'s
    /// `fflag == 2 && fp != stdin`). The second condition blocks one arm: standard
    /// input has no name to check (`tail.c:186` stores the literal `"stdin"` in
    /// `fname`, and `stat`ing that yields only a fictitious path).
    #[must_use]
    pub fn probes_identity(self, is_stdin: bool) -> bool {
        self == Follow::ByName && !is_stdin
    }

    /// C `tail.c:197-201`: with no operand, first probe whether standard input can be
    /// positioned; an `lseek` reporting `ESPIPE` (a pipe) clears `fflag`, with the C
    /// comment "POSIX.2 requires this". So `cat x | tail -f` prints and exits instead
    /// of waiting forever.
    #[must_use]
    pub fn kept_for_stdin(self, stdin_is_pipe: bool) -> Follow {
        if stdin_is_pipe { Follow::Off } else { self }
    }
}

/// How `tail` selects what to print — C's `enum STYLE` (`extern.h:38`) carried
/// with its `off`, folded the way C's `main()` folds it before dispatching to
/// `forward()` or `reverse()` (`tail.c:135-164`). Keeping mode and count in one
/// variant is what lets the `+N` forms express `RBYTES`/`RLINES` with `off == 0`
/// (which C prints as *nothing*, `reverse.c:79-80`) instead of as "no option".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailStyle {
    /// `forward()` + `RLINES`: the last `off` lines in order (default `off = 10`).
    LastLines { off: usize },
    /// `forward()` + `RBYTES`: the trailing `off` bytes (`-b` folded to 512×N).
    LastBytes { off: usize },
    /// `forward()` + `FLINES`: drop `off` newlines, print everything after them.
    SkipLines { off: usize },
    /// `forward()` + `FBYTES`: drop the first `off` bytes, print the remainder.
    SkipBytes { off: usize },
    /// `reverse()` + `RLINES`: the last `off` lines bottom-up.
    ReverseLines { off: usize },
    /// `reverse()` + `RBYTES`: the lines of the trailing `off`-byte window,
    /// bottom-up.
    ReverseBytes { off: usize },
    /// `reverse()` + `REVERSE`: every line bottom-up (`-r` with no number).
    ReverseAll,
}

impl TailStyle {
    /// Whether this selection prints nothing at all. C has two early exits that
    /// both come down to "a backward count of zero selects nothing":
    /// `forward.c`'s `RLINES`/`RBYTES` arms seek to the end of the file when
    /// `off` is zero (and for a pipe just drain it), and `reverse.c` returns
    /// before touching the input when `style != REVERSE && off == 0`. So
    /// `tail -n 0`, `tail -c 0`, `tail -b 0` and the folded `tail -r -n +1` all
    /// print nothing while still exiting successfully — and none of them is the
    /// `REVERSE` shape, which prints the whole file bottom-up.
    pub fn selects_nothing(self) -> bool {
        matches!(
            self,
            TailStyle::LastLines { off: 0 }
                | TailStyle::LastBytes { off: 0 }
                | TailStyle::ReverseLines { off: 0 }
                | TailStyle::ReverseBytes { off: 0 }
        )
    }
}

/// `tail` argument-parsing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailError {
    /// A usage error without getopt's preceding message: C's `ARG` macro opens with
    /// `if (style) usage()` (`tail.c:86-87`, for either numeric option whether repeated
    /// or mixed) and `-r` combined with follow (`:143-145`) both go straight to usage,
    /// with no extra getopt line in front.
    Usage,
    /// An option letter outside the set, or a value option missing its value: libc's
    /// getopt prints that line itself, and the command's usage follows, so the ground
    /// truth is **two lines**. The sentence lives in [`GetoptReport`] (shared by the
    /// first three commands of the family).
    Getopt(crate::GetoptReport),
    /// `-f`/`-F` with more than one operand: C exits fatally after parsing options
    /// and before opening any file (`xerrx(1, "-f and -F options only appropriate for
    /// a single file")` at `tail.c:135-137`). That sentence has no `errno` suffix
    /// (`xerrx` goes through `vwarnx`, `misc.c:78-89`), and it precedes the `-r` usage
    /// check.
    FollowMultiFile,
    /// A `-b`/`-c`/`-n` count that is not a decimal integer (C `ARG`'s `*p` test
    /// on the leftover of `strtoll`, `tail.c:89-90`); carries the bytes, because
    /// C prints `optarg` itself. A zero count is *not* this error: C accepts it
    /// and selects nothing (see [`TailStyle::selects_nothing`]).
    IllegalCount(Vec<u8>),
    /// An old-style `-<digit>…` token whose trailing unit letter is none of
    /// `b`/`c`/`l`/digit — C `obsolete()`'s `default:` fatal arm
    /// (`xerrx(1, "illegal option -- %s", *argv)`, `tail.c:269`); carries the
    /// whole token as the bytes `argv` handed over.
    IllegalOption(Vec<u8>),
}

/// One token's outcome in C's `obsolete()` pre-pass.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ObsoleteStep {
    /// The token is an old-style option and was rewritten for `getopt`. The
    /// rewrite is a **byte** token: C builds it with `malloc` + `strcpy` out of
    /// `argv` bytes (`tail.c:238-273`), so an illegible byte in the operand stays
    /// in the token that later reaches the diagnostics.
    Rewritten(Vec<u8>),
    /// A recognised letter option (`f`, `r`, or `b`/`c`/`n` with its value
    /// attached): kept verbatim, and the scan continues.
    Verbatim,
    /// `b`/`c`/`n` written as its own token: the *next* token is its value and is
    /// not scanned at all (C's `if (!ap[1]) ++argv`).
    SkipsValue,
    /// C `return`s here: this token and everything after it stay verbatim.
    StopScan,
    /// C's `default:` inside the rewrite — fatal, `xerrx(1, "illegal option")`.
    IllegalOption,
}

/// Classify and rewrite one argv token the way C's `obsolete()` walks `argv`
/// (`tail.c`). A token starting with `-` or `+` whose second char is a digit is
/// rewritten: a trailing `f`/`r` is lifted to the front as its own letter; the
/// (possibly new) last char picks the unit letter — `b` and `c` stay, `l` and a
/// digit become `n` — and is dropped from the digits; the token's own first char
/// is then re-inserted before the digits, so the number arrives as the option's
/// value *with its sign*: `-3c` becomes `-c-3`, which `ARG` reads back as three
/// bytes because its `case '-'` negates the parsed number. `tail +10` is the same
/// machine with a `+` first char: it rewrites to `-n+10`, i.e. the forward form
/// starting at line 10.
///
/// The walk is what the two `Verbatim`-shaped steps model: C does **not** stop at
/// the first ordinary option, it keeps scanning past `f`/`r` and past the letter
/// of a value-taking option (skipping only that option's value). So `tail -r -3c`
/// rewrites `-3c`, while `tail f -3c` does not — an operand ends the pre-pass.
fn obsolete_step(arg: &[u8]) -> ObsoleteStep {
    let bytes = arg;
    // C: `ap[0] != '-' && ap[0] != '+'` returns, and so does `ap[1] == '-'`
    // (`--`). A lone `-`/`+` reads the terminator and falls to `default:`.
    if bytes.first() != Some(&b'-') && bytes.first() != Some(&b'+') {
        return ObsoleteStep::StopScan;
    }
    let Some(second) = bytes.get(1).copied() else {
        return ObsoleteStep::StopScan;
    };
    if second == b'-' {
        return ObsoleteStep::StopScan;
    }
    if second.is_ascii_digit() {
        let mut body = &bytes[1..];
        let mut trail: &[u8] = b"";
        match body.last() {
            Some(&b'f') | Some(&b'r') => {
                trail = &body[body.len() - 1..];
                body = &body[..body.len() - 1];
            }
            _ => {}
        }
        let unit = match body.last() {
            Some(b'b') => {
                body = &body[..body.len() - 1];
                b'b'
            }
            Some(b'c') => {
                body = &body[..body.len() - 1];
                b'c'
            }
            Some(b'l') => {
                body = &body[..body.len() - 1];
                b'n'
            }
            Some(c) if c.is_ascii_digit() => b'n',
            // C `default: xerrx(1, "illegal option -- %s", *argv)` — fatal.
            Some(_) | None => return ObsoleteStep::IllegalOption,
        };
        // The five pieces C assembles are, in order, `-`, the `f`/`r` moved earlier,
        // the unit letter, the original token's first character (the sign position),
        // and only then the remaining digits (`*t`/`*t--` write `'\0'` over the unit and
        // trailing letter in place, so `strcpy(p, ap)` carries only the digits).
        return ObsoleteStep::Rewritten(
            [b"-".as_slice(), trail, &[unit], &bytes[0..1], body].concat(),
        );
    }
    match second {
        b'b' | b'c' | b'n' => {
            if bytes.len() == 2 {
                ObsoleteStep::SkipsValue
            } else {
                ObsoleteStep::Verbatim
            }
        }
        b'f' | b'r' => ObsoleteStep::Verbatim,
        // Any other letter: C returns and lets `getopt` reject it (`case '?'` →
        // usage), so the pre-pass stops without raising its own error.
        _ => ObsoleteStep::StopScan,
    }
}

/// A decimal digit run folded one byte at a time: empty, any non-digit byte, or a
/// run that no longer fits `usize` all give `None`, which is exactly the accept
/// set `str::parse::<usize>()` had, so moving the value leg onto bytes does not
/// change what is rejected. C does **not** reject an over-wide run: `strtoll`
/// saturates (`tail.c:88`) and the huge offset goes on to select or to fail an
/// allocation — that divergence is a separate range, registered as
/// `TAIL-OFFSET-SATURATION-OUTCOME` with its readings.
fn digits_to_usize(digits: &[u8]) -> Option<usize> {
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value = 0usize;
    for &byte in digits {
        value = value
            .checked_mul(10)?
            .checked_add(usize::from(byte - b'0'))?;
    }
    Some(value)
}

/// Split `tail`'s argv tail into [`TailArgs`], faithful to C's `obsolete(argv)`
/// pre-pass (`tail.c:106`) plus `getopt(argc, argv, "Fb:c:fn:r")` (`tail.c:108`):
/// old-style `-<digit>…` / `+<digit>…` tokens are rewritten into getopt-shaped
/// options (see [`obsolete_arg`]); option letters cluster (`-rn` ≡ `-r -n`); a
/// colon option's value attaches to its token (`-n10`, `-c100`) or follows it
/// (`-n 10`); `--` ends option scanning; a lone `-` is an operand. `-f`/`-F` are
/// reported, never silently leaked as an operand (as the old whole-token matcher
/// did with `-n10`). Like `head`, `getopt` here **does not permute** (see
/// [`parse_head_args`]): the first non-option token ends option scanning and
/// every token from there on is an operand, so `tail f -r` reads two files and
/// never reverses (the reference build's `tail f -r` measures
/// `tail: -r: No such file or directory`).
///
/// A `+N` value takes the `forward` arm of C's `ARG` macro (`tail.c:91-96`),
/// which is a *different* style, not a negated count: `tail -n +20` starts at
/// line 20, and because the macro's number options are 1-based it decrements
/// `off` by one unit first, so `+1` skips nothing and prints the whole file. A
/// `-N` value takes the other arm: `ARG` negates the parsed number and keeps the
/// backward style, so `-c -3` means the same as `-c 3`.
pub fn parse_tail_args(args: &[&[u8]]) -> Result<TailArgs, TailError> {
    // Phase 1 — obsolete(): walk the tokens the way C does. The walk outlives
    // ordinary options and skips only a value-taking option's value; it stops at
    // the first token C returns on, and everything from there on is handed to
    // `getopt` unchanged. Tokens are owned byte vectors because the rewrite
    // mints new ones (`tail.c:238-273`), exactly like C's `malloc`ed replacement.
    let mut scanned: Vec<Vec<u8>> = Vec::new();
    let mut rest = args;
    let mut frozen = false;
    while let Some((head, tail)) = rest.split_first() {
        rest = tail;
        if frozen {
            scanned.push((*head).to_vec());
            continue;
        }
        match obsolete_step(head) {
            ObsoleteStep::Rewritten(token) => scanned.push(token),
            ObsoleteStep::Verbatim => scanned.push((*head).to_vec()),
            ObsoleteStep::SkipsValue => {
                scanned.push((*head).to_vec());
                // C `if (!ap[1]) ++argv`: the value word is consumed by the
                // option, so it is copied through without being scanned.
                if let Some((value, tail2)) = rest.split_first() {
                    scanned.push((*value).to_vec());
                    rest = tail2;
                }
            }
            ObsoleteStep::StopScan => {
                frozen = true;
                scanned.push((*head).to_vec());
            }
            ObsoleteStep::IllegalOption => {
                return Err(TailError::IllegalOption((*head).to_vec()));
            }
        }
    }

    // Phase 2 — getopt("Fb:c:fn:r"). C keeps `style` and `off` as two locals and
    // only fills them in when a number option appears (`NOTSET` until then,
    // `tail.c:107`); the default and the `-r` folding happen after the loop.
    let mut seen: Option<(RawStyle, usize)> = None;
    let mut rflag = false;
    // C's `fflag` is a 0/1/2 value rather than a boolean (`-f` sets 1, `-F` sets 2,
    // `tail.c:110-121`), and the later one overwrites the earlier: `tail -f -F` is
    // `fflag = 2` in C, so this is an assignment too.
    let mut follow = Follow::Off;
    let mut operands: Vec<Vec<u8>> = Vec::new();
    let mut index = 0;
    while index < scanned.len() {
        let token = scanned[index].as_slice();
        if token == b"--" {
            operands.extend_from_slice(&scanned[index + 1..]);
            break;
        }
        let bytes = token;
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'f' => {
                        follow = Follow::ByHandle;
                        pos += 1;
                    }
                    b'F' => {
                        follow = Follow::ByName;
                        pos += 1;
                    }
                    b'r' => {
                        rflag = true;
                        pos += 1;
                    }
                    b'n' | b'c' | b'b' => {
                        let letter = bytes[pos];
                        // C `ARG`'s first statement: `if (style) usage()` — a
                        // second number option of any kind is a usage error, not
                        // a silent override (`tail.c:86-87`).
                        if seen.is_some() {
                            return Err(TailError::Usage);
                        }
                        // Value attaches to this token (`-n10`) or is the next
                        // word (`-n 10`).
                        let value = if pos + 1 < bytes.len() {
                            bytes[pos + 1..].to_vec()
                        } else {
                            index += 1;
                            if index >= scanned.len() {
                                return Err(TailError::Getopt(crate::GetoptReport::MissingValue(
                                    char::from(letter),
                                )));
                            }
                            scanned[index].clone()
                        };
                        // C `ARG(units, forward, backward)` (`tail.c:85-104`):
                        // `off = strtoll(optarg, &p, 10) * units`, a leading `+`
                        // selects the forward style and gives back one unit
                        // (only when `off` is non-zero), anything else is the
                        // backward style. `-b` is `ARG(512, FBYTES, RBYTES)`, so
                        // it shares `-c`'s two styles at 512-byte units.
                        let forward = value.first() == Some(&b'+');
                        // C `ARG`'s `case '-'`: `off = -off`, which lands on the
                        // same backward style as an unsigned value, so only the
                        // sign prefix is stripped here.
                        let backward_negated = !forward && value.first() == Some(&b'-');
                        let digits = &value[if forward || backward_negated { 1 } else { 0 }..];
                        let (units, fwd, bwd) = match letter {
                            b'n' => (1usize, RawStyle::SkipLines, RawStyle::LastLines),
                            b'c' => (1, RawStyle::SkipBytes, RawStyle::LastBytes),
                            _ /* b */ => (512, RawStyle::SkipBytes, RawStyle::LastBytes),
                        };
                        let parsed = digits_to_usize(digits)
                            .ok_or_else(|| TailError::IllegalCount(value.clone()))?;
                        // C `ARG` does not reject zero: `off = 0 * units` stays
                        // zero and the style is set anyway, which is the
                        // "selects nothing" shape of [`TailStyle::selects_nothing`].
                        let mut off = parsed.saturating_mul(units);
                        if forward && off != 0 {
                            off -= units;
                        }
                        seen = Some((if forward { fwd } else { bwd }, off));
                        pos = bytes.len(); // the value consumed the token's tail
                    }
                    _ => {
                        return Err(TailError::Getopt(crate::GetoptReport::UnknownOption(
                            char::from(bytes[pos]),
                        )));
                    }
                }
            }
        } else {
            // The same predicate as in `head`: `getopt` returns -1 on the first
            // non-option token without advancing `optind` (`getopt.c:71-74`), so
            // `argv += optind` at `tail.c:110` takes this token and everything after it
            // as operands — the reference build's `tail a.txt -r` prints only
            // `tail: -r: No such file or directory` and does **not** reverse.
            operands.extend_from_slice(&scanned[index..]);
            break;
        }
        index += 1;
    }

    // C's two gates are ordered the way `tail.c` has them: the single-file gate at
    // `:135-137` (right after `argc -= optind`), the reverse-versus-follow exclusion at
    // `:143-145`. So `tail -r -f a b` reports the single-file sentence rather than
    // usage — reversing the order would swallow a diagnostic already in hand.
    if follow.active() && operands.len() > 1 {
        return Err(TailError::FollowMultiFile);
    }
    if rflag && follow.active() {
        return Err(TailError::Usage);
    }

    // C `tail.c:135-164`: when no number option was given the default is the
    // whole file for `-r` (`REVERSE`), else the last ten lines (`RLINES`).
    // Under `-r` a forward style becomes the same-unit backward one with `off`
    // kept, which is how `tail -r -n +5` ends up the last four lines bottom-up.
    let style = match (rflag, seen) {
        (false, None) => TailStyle::LastLines { off: 10 },
        (true, None) => TailStyle::ReverseAll,
        (false, Some((RawStyle::LastLines, off))) => TailStyle::LastLines { off },
        (false, Some((RawStyle::LastBytes, off))) => TailStyle::LastBytes { off },
        (false, Some((RawStyle::SkipLines, off))) => TailStyle::SkipLines { off },
        (false, Some((RawStyle::SkipBytes, off))) => TailStyle::SkipBytes { off },
        (true, Some((kind, off))) => match kind {
            RawStyle::LastLines | RawStyle::SkipLines => TailStyle::ReverseLines { off },
            RawStyle::LastBytes | RawStyle::SkipBytes => TailStyle::ReverseBytes { off },
        },
    };
    Ok(TailArgs {
        style,
        follow,
        operands,
    })
}

/// The `NOTSET`-resolved pair C holds while parsing: a style without its count,
/// so the post-loop fold can tell "no number option" from "`off == 0`".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawStyle {
    LastLines,
    LastBytes,
    SkipLines,
    SkipBytes,
}

/// The identity `tail -f`/`-F` compares on each decision round.
///
/// It mirrors the `USE_SLEEP` arm of C `forward.c` (`:248-272`) — the kqueue half is
/// excluded on Minix by `#if !defined(__minix)` (`:207-246`), so the ground truth's
/// follow is "sleep one round plus check identity by name". The fields are exactly the
/// four C compares: `st_size` is not in the predicate (the ground truth never compares
/// it; a size change shows up through the handle's read position), and timestamps are
/// not either — neither enters this record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatIdentity {
    /// `struct stat::st_ino`: C uses it to recognize that the file was replaced.
    pub ino: u64,
    /// `st_dev`.
    pub dev: u64,
    /// `st_rdev` (C compares this too; a device file repointed counts as a swap).
    pub rdev: u64,
    /// `st_nlink`: 0 means the last link was removed.
    pub nlink: u64,
}

/// Whether the file in hand and the one the name now points to are the same (C
/// `forward.c:257-260`'s four comparisons: any change of `st_ino`/`st_dev`/`st_rdev`,
/// or `st_nlink == 0`).
///
/// This does **not ask `fflag` again**: "only `-F` probes" is the job of the gate at
/// `:255`, already carried by [`Follow::probes_identity`]. If the same fact were
/// repeated in both places, flattening either one would not change behavior and a
/// mutation negative control could not catch it.
/// After reopening, the caller takes over the new identity and keeps comparing (C's
/// `*sbp = statbuf`, `:266`) and resets the read position to the start of the new file
/// (C's `freopen`, `:261`).
#[must_use]
pub fn reopen_needed(opened: &StatIdentity, current: &StatIdentity) -> bool {
    current.ino != opened.ino
        || current.dev != opened.dev
        || current.rdev != opened.rdev
        || current.nlink == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed<'a, W: LineWindow<'a>>(window: &mut W, lines: &[&'a str]) -> [Line<'a>; 8] {
        for line in lines {
            window.push(Line::from(*line));
        }
        let mut out: [Line<'a>; 8] = [Line::from(""); 8];
        let count = window.drain(&mut out);
        assert!(count <= 8);
        out
    }

    /// Comparing content looks at text only — the termination state is pinned by two
    /// other doors and is not repeated here. Lines are bytes now, and this file's
    /// fixtures are all ASCII, so this expects directly: non-ASCII would mean the
    /// fixture is wrong.
    fn texts<'a>(lines: &[Line<'a>]) -> Vec<&'a str> {
        lines
            .iter()
            .map(|line| core::str::from_utf8(line.bytes()).expect("测试夹具都是 ASCII"))
            .collect()
    }

    /// The four fields of the swap comparison and the split from "does not ask fflag"
    /// (mirroring `forward.c:255` and `:257-260`).
    #[test]
    fn follow_reopen_needs_a_changed_identity() {
        let opened = StatIdentity {
            ino: 7,
            dev: 1,
            rdev: 0,
            nlink: 1,
        };
        // Three shapes are recognized: ino changed, dev changed, last link removed
        // (nlink == 0)
        let rotated = StatIdentity { ino: 8, ..opened };
        assert!(reopen_needed(&opened, &rotated));
        assert!(reopen_needed(&opened, &StatIdentity { dev: 2, ..opened }));
        assert!(reopen_needed(&opened, &StatIdentity { nlink: 0, ..opened }));
        // `st_rdev` counts too: a device file repointed is a swapped file
        assert!(reopen_needed(&opened, &StatIdentity { rdev: 5, ..opened }));
        // All four fields equal means no reopen (size does not enter this record, so it
        // takes no part in the comparison either)
        assert!(!reopen_needed(&opened, &opened));
    }

    /// The identity-probe gate and the two standard-input exceptions
    /// (`forward.c:255` and `tail.c:197-201`).
    #[test]
    fn follow_probes_only_a_named_file_and_a_pipe_stdin_gives_it_up() {
        // "Who probes" all lives here: `-F` probes only a real file, `-f` does not, and
        // standard input does not either (it has no name to check). The comparison step
        // does not repeat that decision.
        assert!(Follow::ByName.probes_identity(false));
        assert!(!Follow::ByName.probes_identity(true));
        assert!(!Follow::ByHandle.probes_identity(false));
        assert!(!Follow::Off.probes_identity(false));
        // When standard input is a pipe, C clears fflag (POSIX.2), so even `-F` gives up
        assert_eq!(Follow::ByName.kept_for_stdin(true), Follow::Off);
        assert_eq!(Follow::ByHandle.kept_for_stdin(true), Follow::Off);
        assert_eq!(Follow::ByName.kept_for_stdin(false), Follow::ByName);
        assert_eq!(Follow::Off.kept_for_stdin(true), Follow::Off);
    }

    #[test]
    fn test_head_keeps_first() {
        let mut window = HeadWindow::new(2).unwrap();
        let out = feed(&mut window, &["a", "b", "c"]);
        assert_eq!(texts(&out[..2]), ["a", "b"]);
        assert_eq!(window.seen(), 3);
    }

    #[test]
    fn test_tail_keeps_last() {
        let mut window = TailWindow::new(2).unwrap();
        let out = feed(&mut window, &["a", "b", "c", "d"]);
        assert_eq!(texts(&out[..2]), ["c", "d"]);
        assert_eq!(window.seen(), 4);
    }

    #[test]
    fn test_tail_short_input() {
        let mut window = TailWindow::new(5).unwrap();
        let out = feed(&mut window, &["a", "b"]);
        assert_eq!(texts(&out[..2]), ["a", "b"]);
    }

    #[test]
    fn test_zero_count_is_refused_and_large_counts_are_not() {
        assert_eq!(
            HeadWindow::new(0).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
        assert_eq!(
            TailWindow::new(0).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
        // No ceiling: C `forward.c` + `RLINES` walks back through the mapped
        // file for any count, so a count above the old 32 must survive.
        assert!(TailWindow::new(33).is_ok());
        assert!(HeadWindow::new(33).is_ok());
        let mut tail = TailWindow::new(1000).unwrap();
        for line in ["a", "b"] {
            tail.push(Line::from(line));
        }
        // A count far above the input yields the whole input, in order.
        let mut out: [Line; 2] = [Line::from(""); 2];
        assert_eq!(tail.drain(&mut out), 2);
        assert_eq!(texts(&out), ["a", "b"]);
    }

    #[test]
    fn test_windows_share_the_trait() {
        let mut head = HeadWindow::new(1).unwrap();
        let mut tail = TailWindow::new(1).unwrap();
        {
            let windows: [&mut dyn LineWindow<'_>; 2] = [&mut head, &mut tail];
            for window in windows {
                window.push(Line::from("x"));
                window.push(Line::from("y"));
            }
        }
        let mut out = [Line::from(""); 2];
        assert_eq!(head.drain(&mut out), 1);
        assert_eq!(out[0].bytes(), b"x");
        assert_eq!(tail.drain(&mut out), 1);
        assert_eq!(out[0].bytes(), b"y");
    }

    /// The fixtures still write `"…"` literals for readability, and this layer hands
    /// them back as `&[u8]` — `parse_head_args` under test takes raw argv byte slices.
    fn argv<'a>(words: &'a [&'a str]) -> Vec<&'a [u8]> {
        words.iter().map(|w| w.as_bytes()).collect()
    }

    /// The expected head operands: `HeadArgs.operands` holds raw argv bytes, so each
    /// word is converted to bytes here.
    fn operands_bytes(words: &[&str]) -> Vec<Vec<u8>> {
        words.iter().map(|w| w.as_bytes().to_vec()).collect()
    }

    #[test]
    fn test_parse_head_args_faithful_getopt() {
        // Defaults: no args → ten lines, no operands.
        assert_eq!(
            parse_head_args(&[] as &[&[u8]]),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: Vec::new(),
            })
        );
        // `-n` value as the next token or attached.
        assert_eq!(
            parse_head_args(&argv(&["-n", "5", "f"])),
            Ok(HeadArgs {
                count: 5,
                operands: operands_bytes(&["f"]),
                ..HeadArgs {
                    count: 10,
                    bytecnt: 0,
                    qflag: false,
                    vflag: false,
                    operands: Vec::new(),
                }
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["-n5", "f"])),
            Ok(HeadArgs {
                count: 5,
                operands: operands_bytes(&["f"]),
                ..HeadArgs {
                    count: 10,
                    bytecnt: 0,
                    qflag: false,
                    vflag: false,
                    operands: Vec::new(),
                }
            })
        );
        // obsolete(): the leading `-<digit>` run rewrites to `-n<digit>`.
        assert_eq!(
            parse_head_args(&argv(&["-5", "f"])).map(|a| (a.count, a.operands)),
            Ok((5, operands_bytes(&["f"])))
        );
        // `-c` byte count (attached or separate).
        assert_eq!(
            parse_head_args(&argv(&["-c", "10"])).map(|a| a.bytecnt),
            Ok(10)
        );
        // clustered `-q`/`-v`, last one wins.
        assert_eq!(
            parse_head_args(&argv(&["-qv", "f"])).map(|a| (a.qflag, a.vflag)),
            Ok((false, true))
        );
        assert_eq!(
            parse_head_args(&argv(&["-vq"])).map(|a| (a.qflag, a.vflag)),
            Ok((true, false))
        );
        // `--` ends options; a lone `-` is a file operand.
        assert_eq!(
            parse_head_args(&argv(&["--", "-5", "f"])).map(|a| a.operands),
            Ok(operands_bytes(&["-5", "f"]))
        );
        assert_eq!(
            parse_head_args(&argv(&["-"])).map(|a| a.operands),
            Ok(operands_bytes(&["-"]))
        );
    }

    /// `tail`'s ring likewise returns the **first byte** after the dash
    /// (`bytes[pos] as char`; `optopt = *place++` at `getopt.c:79` takes one byte at a
    /// time). The option string is `"Fb:c:fn:r"` (`tail.c:108`), and `b`/`c`/`n` all
    /// take a value, so in the negative case `-né` the `é` is a **value** and the line
    /// is `tail: illegal offset -- é`. This test pins the reading that is already in
    /// place; the byte-for-byte door lives in `tests/tail_face.rs`.
    #[test]
    fn the_tail_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_tail_args(&argv(&["-\u{e9}"])).err(),
            Some(TailError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00c3}'
            ))),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // `r` is a legal, value-less letter in the option string, so the cluster keeps
        // consuming through to the first byte.
        assert_eq!(
            parse_tail_args(&argv(&["-r\u{e9}"])).err(),
            Some(TailError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00c3}'
            ))),
            "-r\u{e9}"
        );
        // The ring stops at the first bad letter.
        assert_eq!(
            parse_tail_args(&argv(&["-Z\u{e9}"])).err(),
            Some(TailError::Getopt(crate::GetoptReport::UnknownOption('Z'))),
            "-Z\u{e9}"
        );
        // The negative case: `n` takes a value, so the bytes after it are consumed
        // entirely as the value and never reach the letter position.
        assert_eq!(
            parse_tail_args(&argv(&["-n\u{e9}"])).err(),
            Some(TailError::IllegalCount(b"\xc3\xa9".to_vec())),
            "-n\u{e9} 走的是 `illegal offset` 那一句"
        );
    }

    /// The value position, the whole-token position and the operand position all
    /// return argv's **raw bytes**. Those three C sentences print argv's content
    /// directly: `ARG`'s `xerrx(1, "illegal offset -- %s", optarg)` (`tail.c:90`,
    /// printing **optarg itself**, not the number parsed out of it), `obsolete()`'s
    /// `xerrx(1, "illegal option -- %s", *argv)` (`:269`, printing the **whole token**),
    /// and an operand enters both `fopen` (`:167`) and the `==> %s <==` banner
    /// (`:173`). `tail` also has a leg other commands lack: the legacy form is a string
    /// C **assembles on the spot** with `malloc` + `strcpy` (`:238-273`), so a high byte
    /// travels into the new token and all the way into the message — the `-1<ff>2` case
    /// below pins that leg (C assembles `-n-1<ff>2`, `ARG`'s `*p` stops on `ff`, so the
    /// echo carries the value **after** the rewrite).
    #[test]
    fn the_tail_value_and_operand_legs_hand_back_the_argv_bytes() {
        assert_eq!(
            parse_tail_args(&[b"-n\xff"]).err(),
            Some(TailError::IllegalCount(vec![0xff])),
            "附值形的高字节原样进回声"
        );
        assert_eq!(
            parse_tail_args(&[b"-n", b"\xff"]).err(),
            Some(TailError::IllegalCount(vec![0xff])),
            "分词形的值同样是原字节"
        );
        assert_eq!(
            parse_tail_args(&[b"-20\xffx"]).err(),
            Some(TailError::IllegalOption(b"-20\xffx".to_vec())),
            "遗留形的回声带的是整段 token"
        );
        assert_eq!(
            parse_tail_args(&[b"-1\xff2"]).err(),
            Some(TailError::IllegalCount(b"-1\xff2".to_vec())),
            "改写之后的 token 里那枚 ff 仍按原字节走到回声位"
        );
        assert_eq!(
            parse_tail_args(&[b"in\xff", b"-r"]).unwrap().operands,
            vec![b"in\xff".to_vec(), b"-r".to_vec()],
            "操作数是文件名，也是横幅里的字"
        );
    }

    /// The outside-the-set letter returns the **first byte** after the dash, not the
    /// first character: `-\u{e9}` (`c3 a9`) is U+00C3 in the `char` domain. The ring
    /// side already reads `bytes[pos] as char` (`optopt = *place++` at `getopt.c:79`
    /// takes one byte at a time), so this pins the reading that is already in place,
    /// keeping a future change back to character-wise reading from turning only the
    /// face red with no clue which half broke; the byte-for-byte door lives in
    /// `tests/head_face.rs`.
    #[test]
    fn the_unknown_letter_is_the_first_byte_not_the_first_character() {
        assert_eq!(
            parse_head_args(&argv(&["-\u{e9}"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00c3}'
            ))),
            "`é` 是 c3 a9 两字节，libc 报 c3 而不是整个字符"
        );
        // `q` is a legal, value-less letter in the option string (`"c:n:qv"` at
        // `head.c:87`), so the cluster keeps consuming through to the first byte.
        assert_eq!(
            parse_head_args(&argv(&["-q\u{e9}"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                '\u{00c3}'
            ))),
            "-q\u{e9}"
        );
        // The ring stops at the first bad letter.
        assert_eq!(
            parse_head_args(&argv(&["-Z\u{e9}"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                'Z'
            ))),
            "-Z\u{e9}"
        );
        // The negative case: a value-taking letter consumes the following bytes as its
        // value, so the letter position is never reached (`head: illegal byte count --
        // é`, a single line with no usage appended).
        assert_eq!(
            parse_head_args(&argv(&["-c\u{e9}"])),
            Err(HeadArgError::IllegalCount {
                byte: true,
                value: "é".as_bytes().to_vec()
            })
        );
    }

    /// A `-c`/`-n` value is **echoed** in C (`errx(1, "illegal byte count -- %s",
    /// optarg)` at `head.c:96-97`, `:102-103`), so what comes back must be raw argv
    /// bytes; operands are likewise returned as bytes (the banner leg is assembled by
    /// the shell and never goes through `&str`). A `&[&str]` signature could not even
    /// express the first two shapes — a bare `ff` is no longer itself before it reaches
    /// the ring. The reference build measures three shapes: `-c<ff>` →
    /// `head: illegal byte count -- <ff>`, `-c 5<ff>` → the same sentence with the
    /// trailing byte, and the obsolete form `-5<ff>` → `illegal line count -- 5<ff>`
    /// (`obsolete()` assembles `-n` plus **all** bytes of the original after the dash,
    /// `head.c:188-192`).
    #[test]
    fn an_illegal_count_value_and_the_operands_carry_the_argv_bytes() {
        let bad: &[&[u8]] = &[b"-c\xff"];
        assert_eq!(
            parse_head_args(bad).err(),
            Some(HeadArgError::IllegalCount {
                byte: true,
                value: vec![0xff],
            }),
            "`-c<ff>` 的值交回的是那一枚裸字节"
        );
        let tail: &[&[u8]] = &[b"-c", b"5\xff"];
        assert_eq!(
            parse_head_args(tail).err(),
            Some(HeadArgError::IllegalCount {
                byte: true,
                value: vec![b'5', 0xff],
            }),
            "尾巴带裸字节也整串交回（C 的 `%s` 打的是 optarg 全体）"
        );
        let obsolete: &[&[u8]] = &[b"-5\xff", b"f"];
        assert_eq!(
            parse_head_args(obsolete).err(),
            Some(HeadArgError::IllegalCount {
                byte: false,
                value: vec![b'5', 0xff],
            }),
            "obsolete 重写之后，值仍是那一串原字节"
        );
        let names: &[&[u8]] = &[b"a\xffb", b"c"];
        assert_eq!(
            parse_head_args(names).map(|a| a.operands),
            Ok(vec![b"a\xffb".to_vec(), b"c".to_vec()]),
            "操数按原字节交回"
        );
    }

    #[test]
    fn test_parse_head_args_errors() {
        // The regression: an unknown option no longer leaks as a file operand.
        // In both cases libc prints a line first and usage follows, so the letter is
        // carried along.
        assert_eq!(
            parse_head_args(&argv(&["-z", "f"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::UnknownOption(
                'z'
            )))
        );
        // obsolete() rewrites only the leading run, so `f -5` does not become `-n5`;
        // and `getopt` does not permute, so the first operand ends the ring
        // (`getopt.c:71-74`) and `-5` becomes an operand **as it stands** — the
        // reference build's `head a.txt -5` measures `==> a.txt <==` plus ten lines,
        // then `head: -5: No such file or directory` with exit 1, and no letter is
        // reported at all.
        assert_eq!(
            parse_head_args(&argv(&["f", "-5"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-5".as_bytes().to_vec()],
            })
        );
        // Flags after an operand take no effect either: `-q` here is a file named
        // `-q` (the reference build reports `head: -q: No such file or directory`), and
        // the same goes for an outside-the-set letter (the `unknown option` line is
        // gone) and for a value option, which lands in the operands together with its
        // value.
        assert_eq!(
            parse_head_args(&argv(&["f", "-q"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-q".as_bytes().to_vec()],
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["f", "-x"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-x".as_bytes().to_vec()],
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["f", "-n", "3"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec![
                    "f".as_bytes().to_vec(),
                    "-n".as_bytes().to_vec(),
                    "3".as_bytes().to_vec()
                ],
            })
        );
        // Options before the operand still take effect; the ring simply never comes
        // back once it hits an operand (the reference build's `head -n 3 a.txt -5`:
        // three lines plus the `-5` line).
        assert_eq!(
            parse_head_args(&argv(&["-n", "3", "f", "-5"])),
            Ok(HeadArgs {
                count: 3,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-5".as_bytes().to_vec()],
            })
        );
        // After an operand, `--` is not "end of options" but a file name (C's ring
        // never reaches it); a lone `-` gets -1 at the same point, so it and everything
        // after it are operands.
        assert_eq!(
            parse_head_args(&argv(&["f", "--", "-5"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec![
                    "f".as_bytes().to_vec(),
                    "--".as_bytes().to_vec(),
                    "-5".as_bytes().to_vec()
                ],
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["f", "-"])),
            Ok(HeadArgs {
                count: 10,
                bytecnt: 0,
                qflag: false,
                vflag: false,
                operands: vec!["f".as_bytes().to_vec(), "-".as_bytes().to_vec()],
            })
        );
        // A colon option with no value argument → the libc missing-value line.
        assert_eq!(
            parse_head_args(&argv(&["-n"])),
            Err(HeadArgError::Getopt(crate::GetoptReport::MissingValue('n')))
        );
        // Illegal counts (<= 0, trailing garbage) → the byte/line reject arm.
        assert_eq!(
            parse_head_args(&argv(&["-n", "0"])),
            Err(HeadArgError::IllegalCount {
                byte: false,
                value: "0".as_bytes().to_vec()
            })
        );
        assert_eq!(
            parse_head_args(&argv(&["-c", "x"])),
            Err(HeadArgError::IllegalCount {
                byte: true,
                value: "x".as_bytes().to_vec()
            })
        );
    }

    #[test]
    fn test_parse_tail_args_faithful_getopt() {
        use alloc::vec;
        // Defaults: no args → ten lines, no operands (`tail.c:161-162`).
        assert_eq!(
            parse_tail_args(&argv(&[])),
            Ok(TailArgs {
                style: TailStyle::LastLines { off: 10 },
                follow: Follow::Off,
                operands: Vec::new(),
            })
        );
        // `getopt` does not permute: a `-r` after the first operand is a file name, not
        // a flag (the reference build's `tail a.txt -r` measures
        // `tail: -r: No such file or directory`, with the output still the last ten
        // lines in forward order).
        assert_eq!(
            parse_tail_args(&argv(&["f", "-r"])),
            Ok(TailArgs {
                style: TailStyle::LastLines { off: 10 },
                follow: Follow::Off,
                operands: vec![b"f".to_vec(), b"-r".to_vec()],
            })
        );
        assert_eq!(
            parse_tail_args(&argv(&["-r", "f", "-n", "3"])),
            Ok(TailArgs {
                style: TailStyle::ReverseAll,
                follow: Follow::Off,
                operands: vec![b"f".to_vec(), b"-n".to_vec(), b"3".to_vec()],
            })
        );
        // `-n` attached (regression: the old matcher leaked `-n10` to operands)
        // and next-word forms both select `RLINES` with that count.
        assert_eq!(
            parse_tail_args(&argv(&["-n10", "f"])),
            Ok(TailArgs {
                style: TailStyle::LastLines { off: 10 },
                follow: Follow::Off,
                operands: operands_bytes(&["f"]),
            })
        );
        assert_eq!(
            parse_tail_args(&argv(&["-n", "10", "f"])).unwrap().style,
            TailStyle::LastLines { off: 10 }
        );
        // obsolete leading digit `-N` rewrites to `-n<N>`.
        assert_eq!(
            parse_tail_args(&argv(&["-5", "f"])).unwrap().style,
            TailStyle::LastLines { off: 5 }
        );
        // `-c`/`-b` attached values reach the byte style (`-b` counts 512-byte
        // blocks, C `ARG(512, FBYTES, RBYTES)`).
        assert_eq!(
            parse_tail_args(&argv(&["-c100", "f"])).unwrap().style,
            TailStyle::LastBytes { off: 100 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-b3", "f"])).unwrap().style,
            TailStyle::LastBytes { off: 1536 }
        );
        // clustering: `-r` then an attached `-n` is C `RLINES` under `reverse()`.
        assert_eq!(
            parse_tail_args(&argv(&["-rn3", "f"])).unwrap().style,
            TailStyle::ReverseLines { off: 3 }
        );
        // bare `-r` is C `REVERSE` (whole file, `tail.c:157-159`), and bare `-r`
        // with `-c`/`-b` is `RBYTES`.
        assert_eq!(
            parse_tail_args(&argv(&["-r", "f"])).unwrap().style,
            TailStyle::ReverseAll
        );
        assert_eq!(
            parse_tail_args(&argv(&["-rc", "8"])).unwrap().style,
            TailStyle::ReverseBytes { off: 8 }
        );
        // `--` ends scanning; a bare `-` is a literal operand.
        assert_eq!(
            parse_tail_args(&argv(&["--", "-n"])).unwrap().operands,
            vec![b"-n".to_vec()]
        );
        assert_eq!(
            parse_tail_args(&argv(&["-"])).unwrap().operands,
            vec![b"-".to_vec()]
        );
        // error paths
        // Both an outside-the-set letter and a missing value go through libc's line
        // (the sentence has a single source in `GetoptReport`).
        assert_eq!(
            parse_tail_args(&argv(&["-x", "f"])).err(),
            Some(TailError::Getopt(crate::GetoptReport::UnknownOption('x')))
        );
        assert_eq!(
            parse_tail_args(&argv(&["-n"])).err(),
            Some(TailError::Getopt(crate::GetoptReport::MissingValue('n')))
        );
        assert_eq!(
            parse_tail_args(&argv(&["-c"])).err(),
            Some(TailError::Getopt(crate::GetoptReport::MissingValue('c')))
        );
        assert_eq!(
            parse_tail_args(&argv(&["-n", "x"])).err(),
            Some(TailError::IllegalCount(b"x".to_vec()))
        );
        // C `ARG` does not reject a zero count — it selects nothing. Refusing
        // `-c 0`/`-b 0` as `illegal offset` is not C behavior.
        assert_eq!(
            parse_tail_args(&argv(&["-c", "0"])).unwrap().style,
            TailStyle::LastBytes { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-b", "0"])).unwrap().style,
            TailStyle::LastBytes { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-n", "0"])).unwrap().style,
            TailStyle::LastLines { off: 0 }
        );
    }

    #[test]
    fn test_tail_style_selects_nothing_covers_the_zero_arms() {
        // C's two early exits: `forward.c` seeks to the end for `RLINES`/`RBYTES`
        // with `off == 0`, `reverse.c` returns for any non-`REVERSE` style with
        // `off == 0`.
        assert!(TailStyle::LastLines { off: 0 }.selects_nothing());
        assert!(TailStyle::LastBytes { off: 0 }.selects_nothing());
        assert!(TailStyle::ReverseLines { off: 0 }.selects_nothing());
        assert!(TailStyle::ReverseBytes { off: 0 }.selects_nothing());
        // Bare `-r` is `REVERSE`: it prints the whole file, and a forward offset
        // of `+1` is `off == 0` in the *skip* sense — everything after byte/line
        // one, i.e. the whole file.
        assert!(!TailStyle::ReverseAll.selects_nothing());
        assert!(!TailStyle::SkipLines { off: 0 }.selects_nothing());
        assert!(!TailStyle::SkipBytes { off: 0 }.selects_nothing());
        assert!(!TailStyle::LastLines { off: 10 }.selects_nothing());
        assert!(!TailStyle::ReverseBytes { off: 8 }.selects_nothing());
    }

    #[test]
    fn test_parse_tail_args_forward_offset_is_one_based() {
        // C `ARG` (`tail.c:91-96`): a `+` value takes the forward style and,
        // when non-zero, gives back one unit — so `+1` skips nothing and `+N`
        // starts *at* line/byte N.
        assert_eq!(
            parse_tail_args(&argv(&["-n", "+1"])).unwrap().style,
            TailStyle::SkipLines { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-n", "+20"])).unwrap().style,
            TailStyle::SkipLines { off: 19 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-c", "+3"])).unwrap().style,
            TailStyle::SkipBytes { off: 2 }
        );
        // The attached form (`-c+3`, and the clustered `-rc+3`) goes through the
        // same `ARG` path as the next-word form.
        assert_eq!(
            parse_tail_args(&argv(&["-c+3"])).unwrap().style,
            TailStyle::SkipBytes { off: 2 }
        );
        // `-b` counts 512-byte blocks, so `+2` starts at byte 512.
        assert_eq!(
            parse_tail_args(&argv(&["-b", "+2"])).unwrap().style,
            TailStyle::SkipBytes { off: 512 }
        );
        // `+0` keeps `off` at zero (C skips the decrement when `off` is zero).
        assert_eq!(
            parse_tail_args(&argv(&["-n", "+0"])).unwrap().style,
            TailStyle::SkipLines { off: 0 }
        );
        // A count past the ring's declared cap is not the ring's business: C
        // `forward()` + `FLINES` walks the file with `getc` and needs no buffer.
        assert_eq!(
            parse_tail_args(&argv(&["-n", "+40"])).unwrap().style,
            TailStyle::SkipLines { off: 39 }
        );
    }

    #[test]
    fn test_parse_tail_args_reverse_folds_forward_offset() {
        // C `tail.c:146-149`: under `-r` `FBYTES` becomes `RBYTES` and `FLINES`
        // becomes `RLINES`, `off` unchanged — `tail -r -n +5` is the last four
        // lines bottom-up, not a forward walk.
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-n", "+5"])).unwrap().style,
            TailStyle::ReverseLines { off: 4 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-c", "+9"])).unwrap().style,
            TailStyle::ReverseBytes { off: 8 }
        );
        // `-r -n +1` folds to `RLINES` with `off == 0`, which C prints as
        // nothing (`reverse.c:79-80`) — the shape must not collapse into
        // `ReverseAll`, which prints the whole file.
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-n", "+1"])).unwrap().style,
            TailStyle::ReverseLines { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-c", "+1"])).unwrap().style,
            TailStyle::ReverseBytes { off: 0 }
        );
    }

    #[test]
    fn test_parse_tail_args_forward_offset_rejects_non_numeric() {
        // C `if (*p) xerrx(1, "illegal offset -- %s", optarg)` (`tail.c:89-90`)
        // applies to the `+` form too: a bare `+` is not a number.
        assert_eq!(
            parse_tail_args(&argv(&["-n", "+"])).err(),
            Some(TailError::IllegalCount(b"+".to_vec()))
        );
        assert_eq!(
            parse_tail_args(&argv(&["-c", "+x"])).err(),
            Some(TailError::IllegalCount(b"+x".to_vec()))
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_unit_letters() {
        // C `obsolete()`: a trailing unit letter selects the option and the digits
        // come back as a *negated* value, which `ARG`'s `case '-'` reads as a
        // backward count. So `-3c` is `tail -c 3`, `-3b` three 512-byte blocks,
        // `-3l` and a bare `-3` are lines.
        assert_eq!(
            parse_tail_args(&argv(&["-3c"])).unwrap().style,
            TailStyle::LastBytes { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-3b"])).unwrap().style,
            TailStyle::LastBytes { off: 1536 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-3l"])).unwrap().style,
            TailStyle::LastLines { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-3"])).unwrap().style,
            TailStyle::LastLines { off: 3 }
        );
        // A trailing `r` is lifted to the front, so `-3r` ≡ `-r -n 3` and
        // `-3cr` ≡ `-r -c 3`.
        assert_eq!(
            parse_tail_args(&argv(&["-3r"])).unwrap().style,
            TailStyle::ReverseLines { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-3cr"])).unwrap().style,
            TailStyle::ReverseBytes { off: 3 }
        );
        // The `+` first char runs through the same machine: `tail +10` ≡
        // `tail -n +10`, and `+4c` ≡ `tail -c +4`.
        assert_eq!(
            parse_tail_args(&argv(&["+10"])).unwrap().style,
            TailStyle::SkipLines { off: 9 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["+4c"])).unwrap().style,
            TailStyle::SkipBytes { off: 3 }
        );
        // A trailing `f` lifts to the front of the rewritten token (`-3f` becomes
        // `-fn-3`, `tail.c:247-251`), so the old-style form sets the follow flag
        // and the count together instead of leaking away as an operand.
        let oldstyle = parse_tail_args(&argv(&["-3f"])).unwrap();
        assert_eq!(oldstyle.style, TailStyle::LastLines { off: 3 });
        assert_eq!(oldstyle.follow, Follow::ByHandle);
    }

    /// The values, clustering, overwriting and C's two gates for the two follow forms
    /// (`tail.c:110-121`, `:135-137`, `:143-145`).
    #[test]
    fn test_parse_tail_args_follow_flags_and_gates() {
        // `-f` and `-F` are two different values, not one boolean
        assert_eq!(
            parse_tail_args(&argv(&["-f", "f"])).unwrap().follow,
            Follow::ByHandle
        );
        assert_eq!(
            parse_tail_args(&argv(&["-F", "f"])).unwrap().follow,
            Follow::ByName
        );
        // C's switch assigns for both letters, so the later one overwrites the earlier
        assert_eq!(
            parse_tail_args(&argv(&["-f", "-F", "f"])).unwrap().follow,
            Follow::ByName
        );
        // Clustering inside one token is recognized too (`-Ff` is two value-less options)
        assert_eq!(
            parse_tail_args(&argv(&["-Ff", "f"])).unwrap().follow,
            Follow::ByHandle
        );
        // The single-file gate looks only at the operand count; neither 0 (reading
        // standard input) nor 1 is blocked
        assert_eq!(
            parse_tail_args(&argv(&["-f"])).unwrap().follow,
            Follow::ByHandle
        );
        assert_eq!(
            parse_tail_args(&argv(&["-f", "a"])).unwrap().operands.len(),
            1
        );
        assert_eq!(
            parse_tail_args(&argv(&["-f", "a", "b"])).err(),
            Some(TailError::FollowMultiFile)
        );
        assert_eq!(
            parse_tail_args(&argv(&["-F", "a", "b"])).err(),
            Some(TailError::FollowMultiFile)
        );
        // The single-file gate precedes the `-r` usage check, so `-r -f a b` reports the
        // former sentence
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-f", "a", "b"])).err(),
            Some(TailError::FollowMultiFile)
        );
        // `-r` and follow are mutually exclusive, with or without a count
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-f", "a"])).err(),
            Some(TailError::Usage)
        );
        // The exclusion is checked only in the `rflag` arm (`tail.c:143-145`), so a
        // count together with `-F` reports no error
        let counted = parse_tail_args(&argv(&["-n", "3", "-F", "a"])).unwrap();
        assert_eq!(counted.style, TailStyle::LastLines { off: 3 });
        assert_eq!(counted.follow, Follow::ByName);
        assert_eq!(
            parse_tail_args(&argv(&["-rF", "a"])).err(),
            Some(TailError::Usage)
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_stops_at_first_non_match() {
        // C `obsolete()` returns from the whole pre-pass at the first token it
        // does not recognise, so a later old-style token is *not* rewritten. A
        // `+<digit>` token then reaches the tail as a plain operand, and because
        // `getopt` does not permute (`getopt.c:71-74`) the operand run ends
        // option scanning there — everything after it is a file name too.
        let frozen = parse_tail_args(&argv(&["f", "+10"])).unwrap();
        assert_eq!(frozen.style, TailStyle::LastLines { off: 10 });
        assert_eq!(frozen.operands, alloc::vec![b"f".to_vec(), b"+10".to_vec()]);
        // A `-3c` frozen past the operand is no longer "unknown letter 3" either: the
        // reference build's `tail f -3c` measures `tail: -3c: No such file or
        // directory` with exit 1, reporting no option error at all.
        let after = parse_tail_args(&argv(&["f", "-3c"])).unwrap();
        assert_eq!(after.style, TailStyle::LastLines { off: 10 });
        assert_eq!(after.operands, alloc::vec![b"f".to_vec(), b"-3c".to_vec()]);
        // Before the freeze point the rewrite does apply, and the operand list is
        // just the file name.
        let live = parse_tail_args(&argv(&["-3c", "f"])).unwrap();
        assert_eq!(live.style, TailStyle::LastBytes { off: 3 });
        assert_eq!(live.operands, alloc::vec![b"f".to_vec()]);
        // `--` is the other stop signal, and it still ends option scanning.
        assert_eq!(
            parse_tail_args(&argv(&["-3c", "--", "f"]))
                .unwrap()
                .operands,
            alloc::vec![b"f".to_vec()]
        );
        // A lone `-` is an operand, not a rewrite — and must not index past the
        // token's end while deciding that.
        assert_eq!(
            parse_tail_args(&argv(&["-"])).unwrap().operands,
            alloc::vec![b"-".to_vec()]
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_illegal_unit_is_fatal() {
        // C's `default:` inside the rewrite is fatal and quotes the whole token:
        // `xerrx(1, "illegal option -- %s", *argv)`.
        assert_eq!(
            parse_tail_args(&argv(&["-3x"])).err(),
            Some(TailError::IllegalOption(b"-3x".to_vec()))
        );
        // Only one trailing letter is lifted, so a second one lands on the same
        // arm (`-3ff` → after lifting `f`, the unit char is `f`).
        assert_eq!(
            parse_tail_args(&argv(&["-3ff"])).err(),
            Some(TailError::IllegalOption(b"-3ff".to_vec()))
        );
    }

    #[test]
    fn test_parse_tail_args_negated_value_is_backward() {
        // C `ARG`'s `case '-'` (`tail.c:97-102`): `off = -off` and the backward
        // style, so `-c -3` means the same as `-c 3`.
        assert_eq!(
            parse_tail_args(&argv(&["-c", "-3"])).unwrap().style,
            TailStyle::LastBytes { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-n", "-7"])).unwrap().style,
            TailStyle::LastLines { off: 7 }
        );
        // A value that is not a plain number still reaches `ARG`'s `*p` test with
        // its sign attached: `tail -n -3c` takes `-3c` as the number, and C
        // reports `illegal offset -- -3c`.
        assert_eq!(
            parse_tail_args(&argv(&["-n", "-3c"])).err(),
            Some(TailError::IllegalCount(b"-3c".to_vec()))
        );
        // A lone `-` after the sign is not a number either; the message carries
        // the whole option argument.
        assert_eq!(
            parse_tail_args(&argv(&["-n", "-"])).err(),
            Some(TailError::IllegalCount(b"-".to_vec()))
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_scan_survives_letter_options() {
        // C's `obsolete()` walk does not stop at `f`/`r`, and for `b`/`c`/`n`
        // written as its own token it steps over the *value* and keeps scanning.
        // So `-r -3c` rewrites: `-c-3` under `rflag` is the three trailing bytes
        // bottom-up (`RBYTES`), not a usage error.
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-3c"])).unwrap().style,
            TailStyle::ReverseBytes { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-5"])).unwrap().style,
            TailStyle::ReverseLines { off: 5 }
        );
        // The value word is consumed by its option, so an old-style number after
        // `-n` is *not* rewritten: `-n -3c` above is the proof, and here the
        // operand is untouched.
        assert_eq!(
            parse_tail_args(&argv(&["-n", "3", "file"]))
                .unwrap()
                .operands,
            alloc::vec![b"file".to_vec()]
        );
    }

    #[test]
    fn test_parse_tail_args_second_number_option_is_usage_error() {
        // C `ARG` opens with `if (style) usage()` (`tail.c:86-87`): two number
        // options of any kind — repeated or mixed — are a usage error, never a
        // silent override.
        assert_eq!(
            parse_tail_args(&argv(&["-n", "3", "-n", "5"])).err(),
            Some(TailError::Usage)
        );
        assert_eq!(
            parse_tail_args(&argv(&["-c10", "-b2"])).err(),
            Some(TailError::Usage)
        );
        assert_eq!(
            parse_tail_args(&argv(&["-n", "3", "-c", "5"])).err(),
            Some(TailError::Usage)
        );
        // One number option alongside `-r` is still fine.
        assert_eq!(
            parse_tail_args(&argv(&["-r", "-n", "3"])).unwrap().style,
            TailStyle::ReverseLines { off: 3 }
        );
    }

    /// The termination state of the line legs: an interior line always carries the
    /// input's newline, and only the input's last line may lack one. The predicate
    /// comes from the reference builds' readings of newline-less tails for all four
    /// shells `head`/`tail`/`nl`/`uniq`; C's line legs never add a newline on the
    /// input's behalf.
    #[test]
    fn lines_of_marks_only_the_final_line_as_unterminated() {
        let marked = lines_of(b"alpha\nbeta\ngamma");
        assert_eq!(
            marked
                .iter()
                .map(|line| (line.bytes(), line.terminated()))
                .collect::<Vec<_>>(),
            vec![
                (b"alpha".as_slice(), true),
                (b"beta".as_slice(), true),
                (b"gamma".as_slice(), false)
            ]
        );
        // The extra empty piece returned for a trailing newline is not a line; the
        // piece between two adjacent newlines is a real line.
        assert_eq!(lines_of(b"alpha\n").len(), 1);
        let blanks = lines_of(b"a\n\nb");
        assert_eq!(
            blanks
                .iter()
                .map(|line| (line.bytes(), line.terminated()))
                .collect::<Vec<_>>(),
            vec![
                (b"a".as_slice(), true),
                (b"".as_slice(), true),
                (b"b".as_slice(), false)
            ]
        );
        assert!(lines_of(b"").is_empty());
    }

    /// A byte that does not decode as UTF-8 does not mean "this line is gone", let
    /// alone "this input is gone". C's line legs (`fgets` at `nl.c:288`, `getc` at
    /// `head.c:169-172`) read bytes, and the reference builds return the same input
    /// with its bytes. Once a line is decoded before splitting, a single illegal byte
    /// turns the whole input into an empty string, and this would return zero lines.
    #[test]
    fn an_undecodable_byte_is_still_a_line() {
        let marked = lines_of(b"a\xffb\nc\n");
        assert_eq!(
            marked
                .iter()
                .map(|line| (line.bytes(), line.terminated()))
                .collect::<Vec<_>>(),
            vec![(b"a\xffb".as_slice(), true), (b"c".as_slice(), true)],
            "含非法字节的行必须留在行里，且两行都带着输入里那枚换行"
        );
        // The last line is a single illegal byte with no newline: the line stays, and
        // the termination state must still be recorded truthfully.
        let tail = lines_of(b"\xff");
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].bytes(), b"\xff");
        assert!(!tail[0].terminated());
    }

    /// `eol` is the writing side's only place to fetch a byte: the newline when the
    /// line had one, empty otherwise.
    #[test]
    fn a_line_writes_back_the_newline_it_had() {
        assert_eq!(Line::from("a").eol(), b"\n");
        assert_eq!(Line::raw(b"a", false).eol(), b"");
        assert!(final_line_terminated(b""));
        assert!(final_line_terminated(b"a\n"));
        assert!(!final_line_terminated(b"a"));
    }
}
