//! Head/tail line windows behind one trait.
//!
//! Ground truth: `head` keeps the first N lines, `tail` the last N. Both
//! answer "which lines survive?" from opposite ends, so both implement
//! [`LineWindow`]. The caller feeds lines in order and drains the survivors
//! at the end; the window never holds more than N borrowed lines.

use crate::TextError;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// One line selection strategy: feed lines in order, drain survivors.
pub trait LineWindow<'a> {
    /// Offer one line (1 based number for diagnostics); survivors stay
    /// inside the window.
    fn push(&mut self, line: &'a str);
    /// Survivors in original order; call once after the last push.
    fn drain(&mut self, out: &mut [&'a str]) -> usize;
    /// How many lines were offered in total.
    fn seen(&self) -> u64;
}

/// Keep the first N lines (`head -n`).
pub struct HeadWindow<'a> {
    lines: [&'a str; 32],
    kept: usize,
    limit: usize,
    seen: u64,
}

impl<'a> HeadWindow<'a> {
    /// A window keeping the first `n` lines (at most 32).
    pub fn new(n: usize) -> Result<Self, TextError> {
        if n == 0 || n > 32 {
            return Err(TextError::InvalidArgument);
        }
        Ok(HeadWindow {
            lines: [""; 32],
            kept: 0,
            limit: n,
            seen: 0,
        })
    }
}

impl<'a> LineWindow<'a> for HeadWindow<'a> {
    fn push(&mut self, line: &'a str) {
        self.seen += 1;
        if self.kept < self.limit {
            self.lines[self.kept] = line;
            self.kept += 1;
        }
    }

    fn drain(&mut self, out: &mut [&'a str]) -> usize {
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
    ring: [&'a str; 32],
    limit: usize,
    seen: u64,
}

impl<'a> TailWindow<'a> {
    /// A window keeping the last `n` lines (at most 32).
    pub fn new(n: usize) -> Result<Self, TextError> {
        if n == 0 || n > 32 {
            return Err(TextError::InvalidArgument);
        }
        Ok(TailWindow {
            ring: [""; 32],
            limit: n,
            seen: 0,
        })
    }
}

impl<'a> LineWindow<'a> for TailWindow<'a> {
    fn push(&mut self, line: &'a str) {
        self.ring[(self.seen as usize) % self.limit] = line;
        self.seen += 1;
    }

    fn drain(&mut self, out: &mut [&'a str]) -> usize {
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
    pub operands: Vec<String>,
}

/// `head` argument-parsing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadArgError {
    /// An unknown option letter, a missing `-c`/`-n` value, or a bare `-`
    /// where an option value belonged (C `getopt` `?`/`:`/default → usage,
    /// `head.c:115-117`).
    Usage,
    /// A `-c`/`-n` value that is not a strictly positive integer: trailing
    /// garbage, `<= 0`, or overflow (C `strtoimax` reject, `head.c:90-103`).
    IllegalCount {
        /// `true` for a `-c` (byte) count, `false` for `-n` (line).
        byte: bool,
        /// The offending value text, echoed into the C error message.
        value: String,
    },
}

/// Split `head`'s argv tail into [`HeadArgs`], faithful to C's
/// `obsolete(argv)` pre-pass (`head.c:84`, defined `head.c:178-195`) plus
/// `getopt(argc, argv, "c:n:qv")` (`head.c:87`):
/// - `obsolete` rewrites only the **leading run** of `-<digit>…` tokens into
///   `-n<digit>…`; its `while` returns at the first token that is not one (a
///   file, `--`, or a non-digit flag). So `head -5 f` becomes `head -n5 f`,
///   while `head f -5` is *not* rewritten and stays a usage error.
/// - `getopt` permutes options among operands: letters cluster (`-qv` ≡ `-q -v`),
///   a colon option's value attaches to its token (`-n5`) or follows it
///   (`-n 5`), `--` ends option scanning so everything after is an operand, and
///   a lone `-` is itself an operand.
/// - `-q`/`-v` are mutually exclusive with the last one winning
///   (`head.c:105-113`); any other letter is [`HeadArgError::Usage`].
pub fn parse_head_args(args: &[&str]) -> Result<HeadArgs, HeadArgError> {
    // Phase 1 — obsolete(): rewrite the leading `-<digit>…` run, stop at the
    // first non-matching token and copy the remainder verbatim.
    let mut scanned: Vec<String> = Vec::new();
    let mut still_obsolete = true;
    for arg in args {
        let bytes = arg.as_bytes();
        if still_obsolete
            && bytes.len() >= 2
            && bytes[0] == b'-'
            && bytes[1] != b'-'
            && bytes[1].is_ascii_digit()
        {
            let mut rewritten = String::from("-n");
            rewritten.push_str(&arg[1..]);
            scanned.push(rewritten);
        } else {
            still_obsolete = false;
            scanned.push(String::from(*arg));
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
        let token = scanned[index].as_str();
        if token == "--" {
            result.operands.extend_from_slice(&scanned[index + 1..]);
            break;
        }
        let bytes = token.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'c' | b'n' => {
                        let byte_opt = bytes[pos] == b'c';
                        let value = if pos + 1 < bytes.len() {
                            String::from(&token[pos + 1..])
                        } else {
                            index += 1;
                            if index >= scanned.len() {
                                return Err(HeadArgError::Usage);
                            }
                            scanned[index].clone()
                        };
                        match value.parse::<i64>() {
                            Ok(parsed) if parsed > 0 => {
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
                    _ => return Err(HeadArgError::Usage),
                }
            }
        } else {
            result.operands.push(String::from(token));
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
    /// File operands in order; `-` is a literal file name (`tail` opens operands
    /// through `fopen`, unlike `cut`).
    pub operands: Vec<String>,
}

/// How `tail` selects what to print — C's `enum STYLE` (`extern.h:36`) carried
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

/// `tail` argument-parsing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailError {
    /// An unknown option letter or a missing `-b`/`-c`/`-n` value (C `getopt`
    /// `?`/default → usage, `tail.c:128-130`).
    Usage,
    /// `-f`/`-F` follow mode — not modelled (line-based ring, forward only).
    NotModelledFollow,
    /// A `-b`/`-c`/`-n` count that is not a decimal integer, or a zero byte or
    /// block count (C `strtol` reject, `tail.c:88-90`); carries the text.
    IllegalCount(String),
    /// An old-style `-<digit>…` token whose trailing unit letter is none of
    /// `b`/`c`/`l`/digit — C `obsolete()`'s `default:` fatal arm
    /// (`xerrx(1, "illegal option -- %s", *argv)`, `tail.c`); carries the token.
    IllegalOption(String),
}

/// One token's outcome in C's `obsolete()` pre-pass.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ObsoleteStep {
    /// The token is an old-style option and was rewritten for `getopt`.
    Rewritten(String),
    /// C `return`s here: this token and everything after it stay verbatim.
    Verbatim,
    /// C's `default:` inside the rewrite — fatal, `xerrx(1, "illegal option")`.
    IllegalOption,
}

/// C `obsolete()`'s rewrite of one argv token (`tail.c`): a token starting with
/// `-` or `+` whose second char is a digit becomes a `getopt`-shaped option.
/// A trailing `f`/`r` is lifted to the front as its own letter; the (possibly
/// new) last char picks the unit letter — `b` and `c` stay, `l` and a digit
/// become `n` — and is dropped from the digits; the token's own first char is
/// then re-inserted before the digits, so the number arrives as the option's
/// value *with its sign*: `-3c` becomes `-c-3`, which `ARG` reads back as three
/// bytes because its `case '-'` negates the parsed number.
///
/// `tail +10` is the same machine with a `+` first char: it rewrites to `-n+10`,
/// i.e. the forward form starting at line 10.
fn obsolete_arg(arg: &str) -> ObsoleteStep {
    let bytes = arg.as_bytes();
    let signed = bytes.first() == Some(&b'-') || bytes.first() == Some(&b'+');
    // C: a bare operand stops the scan (`ap[0] != '-' && ap[0] != '+'`), `--`
    // stops it too (`ap[1] == '-'`), and so does a second char that is not a
    // digit — that token is left for `getopt` to reject or treat as an operand.
    // A lone `-` is the empty-value case of the same test: `*++ap` reads the
    // terminator, so C falls through to `default: return`.
    if !signed || bytes.len() < 2 || (bytes[0] == b'-' && bytes[1] == b'-') {
        return ObsoleteStep::Verbatim;
    }
    if !bytes[1].is_ascii_digit() {
        return ObsoleteStep::Verbatim;
    }
    let mut body = &arg[1..];
    let mut trail = "";
    match body.as_bytes().last() {
        Some(&b'f') | Some(&b'r') => {
            trail = &body[body.len() - 1..];
            body = &body[..body.len() - 1];
        }
        _ => {}
    }
    let unit = match body.as_bytes().last() {
        Some(b'b') => {
            body = &body[..body.len() - 1];
            'b'
        }
        Some(b'c') => {
            body = &body[..body.len() - 1];
            'c'
        }
        Some(b'l') => {
            body = &body[..body.len() - 1];
            'n'
        }
        Some(c) if c.is_ascii_digit() => 'n',
        None => return ObsoleteStep::IllegalOption,
        Some(_) => return ObsoleteStep::IllegalOption,
    };
    ObsoleteStep::Rewritten(format!("-{trail}{unit}{}{body}", &arg[0..1]))
}

/// Split `tail`'s argv tail into [`TailArgs`], faithful to C's `obsolete(argv)`
/// pre-pass (`tail.c:106`) plus `getopt(argc, argv, "Fb:c:fn:r")` (`tail.c:108`):
/// old-style `-<digit>…` / `+<digit>…` tokens are rewritten into getopt-shaped
/// options (see [`obsolete_arg`]); option letters cluster (`-rn` ≡ `-r -n`); a
/// colon option's value attaches to its token (`-n10`, `-c100`) or follows it
/// (`-n 10`); `--` ends option scanning; a lone `-` is an operand. `-f`/`-F` are
/// reported, never silently leaked as an operand (as the old whole-token matcher
/// did with `-n10`).
///
/// A `+N` value takes the `forward` arm of C's `ARG` macro (`tail.c:91-96`),
/// which is a *different* style, not a negated count: `tail -n +20` starts at
/// line 20, and because the macro's number options are 1-based it decrements
/// `off` by one unit first, so `+1` skips nothing and prints the whole file. A
/// `-N` value takes the other arm: `ARG` negates the parsed number and keeps the
/// backward style, so `-c -3` means the same as `-c 3`.
pub fn parse_tail_args(args: &[&str]) -> Result<TailArgs, TailError> {
    // Phase 1 — obsolete(): rewrite each leading old-style token while the run
    // lasts, stop rewriting at the first token C would `return` on, and hand that
    // token to `getopt` unchanged.
    let mut scanned: Vec<String> = Vec::new();
    let mut still_obsolete = true;
    for arg in args {
        let step = if still_obsolete {
            obsolete_arg(arg)
        } else {
            ObsoleteStep::Verbatim
        };
        match step {
            ObsoleteStep::Rewritten(token) => scanned.push(token),
            ObsoleteStep::Verbatim => {
                still_obsolete = false;
                scanned.push(String::from(*arg));
            }
            ObsoleteStep::IllegalOption => {
                return Err(TailError::IllegalOption(String::from(*arg)));
            }
        }
    }

    // Phase 2 — getopt("Fb:c:fn:r"). C keeps `style` and `off` as two locals and
    // only fills them in when a number option appears (`NOTSET` until then,
    // `tail.c:107`); the default and the `-r` folding happen after the loop.
    let mut seen: Option<(RawStyle, usize)> = None;
    let mut rflag = false;
    let mut operands: Vec<String> = Vec::new();
    let mut index = 0;
    while index < scanned.len() {
        let token = scanned[index].as_str();
        if token == "--" {
            operands.extend_from_slice(&scanned[index + 1..]);
            break;
        }
        let bytes = token.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            let mut pos = 1usize;
            while pos < bytes.len() {
                match bytes[pos] {
                    b'f' | b'F' => return Err(TailError::NotModelledFollow),
                    b'r' => {
                        rflag = true;
                        pos += 1;
                    }
                    b'n' | b'c' | b'b' => {
                        let letter = bytes[pos];
                        // Value attaches to this token (`-n10`) or is the next
                        // word (`-n 10`).
                        let value = if pos + 1 < bytes.len() {
                            String::from(&token[pos + 1..])
                        } else {
                            index += 1;
                            if index >= scanned.len() {
                                return Err(TailError::Usage);
                            }
                            scanned[index].clone()
                        };
                        // C `ARG(units, forward, backward)` (`tail.c:85-104`):
                        // `off = strtoll(optarg, &p, 10) * units`, a leading `+`
                        // selects the forward style and gives back one unit
                        // (only when `off` is non-zero), anything else is the
                        // backward style. `-b` is `ARG(512, FBYTES, RBYTES)`, so
                        // it shares `-c`'s two styles at 512-byte units.
                        let forward = value.starts_with('+');
                        // C `ARG`'s `case '-'`: `off = -off`, which lands on the
                        // same backward style as an unsigned value, so only the
                        // sign prefix is stripped here.
                        let backward_negated = !forward && value.starts_with('-');
                        let digits = &value[if forward || backward_negated { 1 } else { 0 }..];
                        let (units, fwd, bwd) = match letter {
                            b'n' => (1usize, RawStyle::SkipLines, RawStyle::LastLines),
                            b'c' => (1, RawStyle::SkipBytes, RawStyle::LastBytes),
                            _ /* b */ => (512, RawStyle::SkipBytes, RawStyle::LastBytes),
                        };
                        let parsed = digits
                            .parse::<usize>()
                            .map_err(|_| TailError::IllegalCount(value.clone()))?;
                        if !forward && matches!(letter, b'c' | b'b') && parsed == 0 {
                            return Err(TailError::IllegalCount(value));
                        }
                        let mut off = parsed.saturating_mul(units);
                        if forward && off != 0 {
                            off -= units;
                        }
                        seen = Some((if forward { fwd } else { bwd }, off));
                        pos = bytes.len(); // the value consumed the token's tail
                    }
                    _ => return Err(TailError::Usage),
                }
            }
        } else {
            operands.push(String::from(token));
        }
        index += 1;
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
    Ok(TailArgs { style, operands })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn feed<'a, W: LineWindow<'a>>(window: &mut W, lines: &[&'a str]) -> [&'a str; 8] {
        for line in lines {
            window.push(line);
        }
        let mut out: [&'a str; 8] = [""; 8];
        let count = window.drain(&mut out);
        assert!(count <= 8);
        out
    }

    #[test]
    fn test_head_keeps_first() {
        let mut window = HeadWindow::new(2).unwrap();
        let out = feed(&mut window, &["a", "b", "c"]);
        assert_eq!(&out[..2], &["a", "b"]);
        assert_eq!(window.seen(), 3);
    }

    #[test]
    fn test_tail_keeps_last() {
        let mut window = TailWindow::new(2).unwrap();
        let out = feed(&mut window, &["a", "b", "c", "d"]);
        assert_eq!(&out[..2], &["c", "d"]);
        assert_eq!(window.seen(), 4);
    }

    #[test]
    fn test_tail_short_input() {
        let mut window = TailWindow::new(5).unwrap();
        let out = feed(&mut window, &["a", "b"]);
        assert_eq!(&out[..2], &["a", "b"]);
    }

    #[test]
    fn test_zero_or_huge_rejected() {
        assert_eq!(
            HeadWindow::new(0).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
        assert_eq!(
            TailWindow::new(33).map(|_| ()),
            Err(TextError::InvalidArgument)
        );
    }

    #[test]
    fn test_windows_share_the_trait() {
        let mut head = HeadWindow::new(1).unwrap();
        let mut tail = TailWindow::new(1).unwrap();
        {
            let windows: [&mut dyn LineWindow<'_>; 2] = [&mut head, &mut tail];
            for window in windows {
                window.push("x");
                window.push("y");
            }
        }
        let mut out = [""; 2];
        assert_eq!(head.drain(&mut out), 1);
        assert_eq!(out[0], "x");
        assert_eq!(tail.drain(&mut out), 1);
        assert_eq!(out[0], "y");
    }

    fn operands(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| String::from(*w)).collect()
    }

    #[test]
    fn test_parse_head_args_faithful_getopt() {
        use alloc::string::String;
        use alloc::vec;
        // Defaults: no args → ten lines, no operands.
        assert_eq!(
            parse_head_args(&[]),
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
            parse_head_args(&["-n", "5", "f"]),
            Ok(HeadArgs {
                count: 5,
                operands: operands(&["f"]),
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
            parse_head_args(&["-n5", "f"]),
            Ok(HeadArgs {
                count: 5,
                operands: operands(&["f"]),
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
            parse_head_args(&["-5", "f"]).map(|a| (a.count, a.operands)),
            Ok((5, operands(&["f"])))
        );
        // `-c` byte count (attached or separate).
        assert_eq!(parse_head_args(&["-c", "10"]).map(|a| a.bytecnt), Ok(10));
        // clustered `-q`/`-v`, last one wins.
        assert_eq!(
            parse_head_args(&["-qv", "f"]).map(|a| (a.qflag, a.vflag)),
            Ok((false, true))
        );
        assert_eq!(
            parse_head_args(&["-vq"]).map(|a| (a.qflag, a.vflag)),
            Ok((true, false))
        );
        // `--` ends options; a lone `-` is a file operand.
        assert_eq!(
            parse_head_args(&["--", "-5", "f"]).map(|a| a.operands),
            Ok(operands(&["-5", "f"]))
        );
        assert_eq!(
            parse_head_args(&["-"]).map(|a| a.operands),
            Ok(operands(&["-"]))
        );
    }

    #[test]
    fn test_parse_head_args_errors() {
        use alloc::string::String;
        // The regression: an unknown option no longer leaks as a file operand.
        assert_eq!(parse_head_args(&["-z", "f"]), Err(HeadArgError::Usage));
        // obsolete() only rewrites the LEADING run, so `f -5` is not `-n5`;
        // `-5` after a file is an unknown option letter '5' → usage.
        assert_eq!(parse_head_args(&["f", "-5"]), Err(HeadArgError::Usage));
        // A colon option with no value argument → usage (C getopt ':' arm).
        assert_eq!(parse_head_args(&["-n"]), Err(HeadArgError::Usage));
        // Illegal counts (<= 0, trailing garbage) → the byte/line reject arm.
        assert_eq!(
            parse_head_args(&["-n", "0"]),
            Err(HeadArgError::IllegalCount {
                byte: false,
                value: String::from("0")
            })
        );
        assert_eq!(
            parse_head_args(&["-c", "x"]),
            Err(HeadArgError::IllegalCount {
                byte: true,
                value: String::from("x")
            })
        );
    }

    #[test]
    fn test_parse_tail_args_faithful_getopt() {
        use alloc::string::String;
        use alloc::vec;
        // Defaults: no args → ten lines, no operands (`tail.c:161-162`).
        assert_eq!(
            parse_tail_args(&[]),
            Ok(TailArgs {
                style: TailStyle::LastLines { off: 10 },
                operands: Vec::new(),
            })
        );
        // `-n` attached (regression: the old matcher leaked `-n10` to operands)
        // and next-word forms both select `RLINES` with that count.
        assert_eq!(
            parse_tail_args(&["-n10", "f"]),
            Ok(TailArgs {
                style: TailStyle::LastLines { off: 10 },
                operands: operands(&["f"]),
            })
        );
        assert_eq!(
            parse_tail_args(&["-n", "10", "f"]).unwrap().style,
            TailStyle::LastLines { off: 10 }
        );
        // obsolete leading digit `-N` rewrites to `-n<N>`.
        assert_eq!(
            parse_tail_args(&["-5", "f"]).unwrap().style,
            TailStyle::LastLines { off: 5 }
        );
        // `-c`/`-b` attached values reach the byte style (`-b` counts 512-byte
        // blocks, C `ARG(512, FBYTES, RBYTES)`).
        assert_eq!(
            parse_tail_args(&["-c100", "f"]).unwrap().style,
            TailStyle::LastBytes { off: 100 }
        );
        assert_eq!(
            parse_tail_args(&["-b3", "f"]).unwrap().style,
            TailStyle::LastBytes { off: 1536 }
        );
        // clustering: `-r` then an attached `-n` is C `RLINES` under `reverse()`.
        assert_eq!(
            parse_tail_args(&["-rn3", "f"]).unwrap().style,
            TailStyle::ReverseLines { off: 3 }
        );
        // bare `-r` is C `REVERSE` (whole file, `tail.c:157-159`), and bare `-r`
        // with `-c`/`-b` is `RBYTES`.
        assert_eq!(
            parse_tail_args(&["-r", "f"]).unwrap().style,
            TailStyle::ReverseAll
        );
        assert_eq!(
            parse_tail_args(&["-rc", "8"]).unwrap().style,
            TailStyle::ReverseBytes { off: 8 }
        );
        // `--` ends scanning; a bare `-` is a literal operand.
        assert_eq!(
            parse_tail_args(&["--", "-n"]).unwrap().operands,
            vec![String::from("-n")]
        );
        assert_eq!(
            parse_tail_args(&["-"]).unwrap().operands,
            vec![String::from("-")]
        );
        // error paths
        assert_eq!(
            parse_tail_args(&["-f"]).err(),
            Some(TailError::NotModelledFollow)
        );
        assert_eq!(
            parse_tail_args(&["-F", "f"]).err(),
            Some(TailError::NotModelledFollow)
        );
        assert_eq!(parse_tail_args(&["-x", "f"]).err(), Some(TailError::Usage));
        assert_eq!(parse_tail_args(&["-n"]).err(), Some(TailError::Usage));
        assert_eq!(
            parse_tail_args(&["-n", "x"]).err(),
            Some(TailError::IllegalCount(String::from("x")))
        );
        assert_eq!(
            parse_tail_args(&["-c", "0"]).err(),
            Some(TailError::IllegalCount(String::from("0")))
        );
    }

    #[test]
    fn test_parse_tail_args_forward_offset_is_one_based() {
        // C `ARG` (`tail.c:91-96`): a `+` value takes the forward style and,
        // when non-zero, gives back one unit — so `+1` skips nothing and `+N`
        // starts *at* line/byte N.
        assert_eq!(
            parse_tail_args(&["-n", "+1"]).unwrap().style,
            TailStyle::SkipLines { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&["-n", "+20"]).unwrap().style,
            TailStyle::SkipLines { off: 19 }
        );
        assert_eq!(
            parse_tail_args(&["-c", "+3"]).unwrap().style,
            TailStyle::SkipBytes { off: 2 }
        );
        // The attached form (`-c+3`, and the clustered `-rc+3`) goes through the
        // same `ARG` path as the next-word form.
        assert_eq!(
            parse_tail_args(&["-c+3"]).unwrap().style,
            TailStyle::SkipBytes { off: 2 }
        );
        // `-b` counts 512-byte blocks, so `+2` starts at byte 512.
        assert_eq!(
            parse_tail_args(&["-b", "+2"]).unwrap().style,
            TailStyle::SkipBytes { off: 512 }
        );
        // `+0` keeps `off` at zero (C skips the decrement when `off` is zero).
        assert_eq!(
            parse_tail_args(&["-n", "+0"]).unwrap().style,
            TailStyle::SkipLines { off: 0 }
        );
        // A count past the ring's declared cap is not the ring's business: C
        // `forward()` + `FLINES` walks the file with `getc` and needs no buffer.
        assert_eq!(
            parse_tail_args(&["-n", "+40"]).unwrap().style,
            TailStyle::SkipLines { off: 39 }
        );
    }

    #[test]
    fn test_parse_tail_args_reverse_folds_forward_offset() {
        // C `tail.c:146-149`: under `-r` `FBYTES` becomes `RBYTES` and `FLINES`
        // becomes `RLINES`, `off` unchanged — `tail -r -n +5` is the last four
        // lines bottom-up, not a forward walk.
        assert_eq!(
            parse_tail_args(&["-r", "-n", "+5"]).unwrap().style,
            TailStyle::ReverseLines { off: 4 }
        );
        assert_eq!(
            parse_tail_args(&["-r", "-c", "+9"]).unwrap().style,
            TailStyle::ReverseBytes { off: 8 }
        );
        // `-r -n +1` folds to `RLINES` with `off == 0`, which C prints as
        // nothing (`reverse.c:79-80`) — the shape must not collapse into
        // `ReverseAll`, which prints the whole file.
        assert_eq!(
            parse_tail_args(&["-r", "-n", "+1"]).unwrap().style,
            TailStyle::ReverseLines { off: 0 }
        );
        assert_eq!(
            parse_tail_args(&["-r", "-c", "+1"]).unwrap().style,
            TailStyle::ReverseBytes { off: 0 }
        );
    }

    #[test]
    fn test_parse_tail_args_forward_offset_rejects_non_numeric() {
        // C `if (*p) xerrx(1, "illegal offset -- %s", optarg)` (`tail.c:89-90`)
        // applies to the `+` form too: a bare `+` is not a number.
        assert_eq!(
            parse_tail_args(&["-n", "+"]).err(),
            Some(TailError::IllegalCount(String::from("+")))
        );
        assert_eq!(
            parse_tail_args(&["-c", "+x"]).err(),
            Some(TailError::IllegalCount(String::from("+x")))
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_unit_letters() {
        // C `obsolete()`: a trailing unit letter selects the option and the digits
        // come back as a *negated* value, which `ARG`'s `case '-'` reads as a
        // backward count. So `-3c` is `tail -c 3`, `-3b` three 512-byte blocks,
        // `-3l` and a bare `-3` are lines.
        assert_eq!(
            parse_tail_args(&["-3c"]).unwrap().style,
            TailStyle::LastBytes { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-3b"]).unwrap().style,
            TailStyle::LastBytes { off: 1536 }
        );
        assert_eq!(
            parse_tail_args(&["-3l"]).unwrap().style,
            TailStyle::LastLines { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-3"]).unwrap().style,
            TailStyle::LastLines { off: 3 }
        );
        // A trailing `r` is lifted to the front, so `-3r` ≡ `-r -n 3` and
        // `-3cr` ≡ `-r -c 3`.
        assert_eq!(
            parse_tail_args(&["-3r"]).unwrap().style,
            TailStyle::ReverseLines { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-3cr"]).unwrap().style,
            TailStyle::ReverseBytes { off: 3 }
        );
        // The `+` first char runs through the same machine: `tail +10` ≡
        // `tail -n +10`, and `+4c` ≡ `tail -c +4`.
        assert_eq!(
            parse_tail_args(&["+10"]).unwrap().style,
            TailStyle::SkipLines { off: 9 }
        );
        assert_eq!(
            parse_tail_args(&["+4c"]).unwrap().style,
            TailStyle::SkipBytes { off: 3 }
        );
        // A trailing `f` lifts too, then meets this shell's honest follow
        // rejection instead of leaking away as an operand.
        assert_eq!(
            parse_tail_args(&["-3f"]).err(),
            Some(TailError::NotModelledFollow)
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_stops_at_first_non_match() {
        // C `obsolete()` returns from the whole pre-pass at the first token it
        // does not recognise, so a later old-style token is *not* rewritten. A
        // `+<digit>` token then reaches `getopt` as a plain operand, exactly as C
        // permutes it.
        let frozen = parse_tail_args(&["f", "+10"]).unwrap();
        assert_eq!(frozen.style, TailStyle::LastLines { off: 10 });
        assert_eq!(
            frozen.operands,
            alloc::vec![String::from("f"), String::from("+10")]
        );
        // A frozen `-3c` keeps its dash, so `getopt` sees the unknown letter `3`
        // and takes C's `case '?'` → usage (`tail.c:128-130`).
        assert_eq!(parse_tail_args(&["f", "-3c"]).err(), Some(TailError::Usage));
        // Before the freeze point the rewrite does apply, and the operand list is
        // just the file name.
        let live = parse_tail_args(&["-3c", "f"]).unwrap();
        assert_eq!(live.style, TailStyle::LastBytes { off: 3 });
        assert_eq!(live.operands, alloc::vec![String::from("f")]);
        // `--` is the other stop signal, and it still ends option scanning.
        assert_eq!(
            parse_tail_args(&["-3c", "--", "f"]).unwrap().operands,
            alloc::vec![String::from("f")]
        );
        // A lone `-` is an operand, not a rewrite — and must not index past the
        // token's end while deciding that.
        assert_eq!(
            parse_tail_args(&["-"]).unwrap().operands,
            alloc::vec![String::from("-")]
        );
    }

    #[test]
    fn test_parse_tail_args_obsolete_illegal_unit_is_fatal() {
        // C's `default:` inside the rewrite is fatal and quotes the whole token:
        // `xerrx(1, "illegal option -- %s", *argv)`.
        assert_eq!(
            parse_tail_args(&["-3x"]).err(),
            Some(TailError::IllegalOption(String::from("-3x")))
        );
        // Only one trailing letter is lifted, so a second one lands on the same
        // arm (`-3ff` → after lifting `f`, the unit char is `f`).
        assert_eq!(
            parse_tail_args(&["-3ff"]).err(),
            Some(TailError::IllegalOption(String::from("-3ff")))
        );
    }

    #[test]
    fn test_parse_tail_args_negated_value_is_backward() {
        // C `ARG`'s `case '-'` (`tail.c:97-102`): `off = -off` and the backward
        // style, so `-c -3` means the same as `-c 3`.
        assert_eq!(
            parse_tail_args(&["-c", "-3"]).unwrap().style,
            TailStyle::LastBytes { off: 3 }
        );
        assert_eq!(
            parse_tail_args(&["-n", "-7"]).unwrap().style,
            TailStyle::LastLines { off: 7 }
        );
        // A lone `-` after the sign is not a number; C's `*p` test rejects it and
        // the message carries the whole option argument.
        assert_eq!(
            parse_tail_args(&["-n", "-"]).err(),
            Some(TailError::IllegalCount(String::from("-")))
        );
    }
}
