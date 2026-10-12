//! Head/tail line windows behind one trait.
//!
//! Ground truth: `head` keeps the first N lines, `tail` the last N. Both
//! answer "which lines survive?" from opposite ends, so both implement
//! [`LineWindow`]. The caller feeds lines in order and drains the survivors
//! at the end; the window never holds more than N borrowed lines.

use crate::TextError;

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
                                })
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
        assert_eq!(HeadWindow::new(0).map(|_| ()), Err(TextError::InvalidArgument));
        assert_eq!(TailWindow::new(33).map(|_| ()), Err(TextError::InvalidArgument));
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
}
