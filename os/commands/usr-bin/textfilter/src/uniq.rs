//! Adjacent duplicate handling behind `uniq`.
//!
//! Ground truth: `minix3/usr.bin/uniq/uniq.c` (257 lines; flag variables
//! `cflag`/`dflag`/`uflag` around line 56, the skip decision at line 188,
//! the count print at line 190). Rules:
//!
//! - Only *adjacent* equal lines collapse (the input is expected sorted;
//!   `uniq` never reorders).
//! - Default prints one copy of each run.
//! - `-c` prefixes each printed line with its run count (`C` prints `"%4d %s"`,
//!   so the count is right-justified in four columns).
//! - `-d` prints only lines from runs longer than one.
//! - `-u` prints only lines from runs of exactly one.
//!
//! The three flags are independent in C (`uniq.c` keeps `cflag`, `dflag` and
//! `uflag` as three ints), so combinations are legal and each has its own
//! meaning: `-c -d` counts the repeated runs only, `-c -u` counts the unique
//! ones, and `-d -u` suppresses every run and prints nothing. `show()`'s guard
//! says it in one line — skip when `dflag && repeats == 0` or `uflag && repeats
//! > 0` — and [`UniqFlags::keeps`] is that guard written the same way.
//!
//! The handler is a small state machine fed one line at a time: it reports
//! completed runs through a callback, so arbitrarily long inputs stream
//! without buffering.

use crate::TextError;

/// C `uniq`'s three flag variables (`cflag`, `dflag`, `uflag`), kept separate
/// because C composes them in `show()` rather than collapsing them into one
/// mode. A fourth shape is not needed: with no flag set every run is printed
/// once, which is what the default `false` triple already does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UniqFlags {
    /// `-c`: prefix each printed line with its run count.
    pub count: bool,
    /// `-d`: print only runs of more than one line.
    pub repeats_only: bool,
    /// `-u`: print only runs of exactly one line.
    pub uniques_only: bool,
}

impl UniqFlags {
    /// C `show()`'s guard (`uniq.c`): a run is skipped when `-d` is set and it
    /// never repeated, or when `-u` is set and it did. `repeats` in C is
    /// `count - 1`, so "never repeated" is `count == 1`. Written as one
    /// negated disjunction because that is literally C's shape:
    /// `if ((dflag && repeats == 0) || (uflag && repeats > 0)) return;`.
    pub fn keeps(&self, count: u64) -> bool {
        !((self.repeats_only && count == 1) || (self.uniques_only && count > 1))
    }
}

/// One completed run handed to the reporter: the line plus its count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run<'a> {
    /// The repeated line（内容加它在输入里的那枚换行——C `uniq` 印的是**一个 run 的
    /// 第一行**，而只有输入的末行可能没有换行，所以这个状态必须跟着行走：
    /// `printf 'x\nx'` 里被印出的是第一行 `x\n`，不是末行那个 `x`）。
    pub line: crate::window::Line<'a>,
    /// How many adjacent copies formed the run.
    pub count: u64,
}

/// C `uniq`'s `skip()` (`uniq.c`): the comparison key of a line under
/// `-f fields` and `-s chars`. Skip `numfields` whitespace-delimited fields —
/// leading blanks are not a boundary, and the blank that ends a field is
/// consumed — then `numchars` bytes, and return the rest. Operates on bytes
/// because C is byte-oriented (`isspace` on `unsigned char`).
pub fn comparison_key(line: &[u8], numfields: usize, numchars: usize) -> &[u8] {
    let mut i = 0usize;
    let mut nfields = numfields;
    let mut infield = false;
    while nfields > 0 && i < line.len() {
        let b = line[i];
        if b == b' ' || (0x09..=0x0d).contains(&b) {
            if infield {
                infield = false;
                nfields -= 1;
            }
        } else if !infield {
            infield = true;
        }
        i += 1;
    }
    let mut nchars = numchars;
    while nchars > 0 && i < line.len() {
        i += 1;
        nchars -= 1;
    }
    &line[i..]
}

/// Adjacent duplicate handler: feed lines in order, collect runs.
pub struct Uniq<'a> {
    flags: UniqFlags,
    skip_fields: usize,
    skip_chars: usize,
    previous: Option<crate::window::Line<'a>>,
    previous_key: Option<&'a [u8]>,
    count: u64,
}

impl<'a> Uniq<'a> {
    /// A new handler under `flags` (no field/character skip).
    pub fn new(flags: UniqFlags) -> Self {
        Uniq::new_with_skip(flags, 0, 0)
    }

    /// A handler that compares `-f fields`/`-s chars` skipped keys while still
    /// reporting whole lines (`skip_fields`/`skip_chars` mirror C `uniq`'s
    /// `numfields`/`numchars`).
    pub fn new_with_skip(flags: UniqFlags, skip_fields: usize, skip_chars: usize) -> Self {
        Uniq {
            flags,
            skip_fields,
            skip_chars,
            previous: None,
            previous_key: None,
            count: 0,
        }
    }

    /// Offer one line; returns a completed run when the run breaks.
    pub fn push(&mut self, line: impl Into<crate::window::Line<'a>>) -> Option<Run<'a>> {
        let line = line.into();
        let key = comparison_key(line.text().as_bytes(), self.skip_fields, self.skip_chars);
        match self.previous_key {
            Some(previous_key) if previous_key == key => {
                self.count += 1;
                None
            }
            _ => {
                let finished = self.previous.map(|previous| Run {
                    line: previous,
                    count: self.count,
                });
                self.previous = Some(line);
                self.previous_key = Some(key);
                self.count = 1;
                finished
            }
        }
    }

    /// The trailing run after the last push, if any input arrived.
    pub fn finish(&mut self) -> Option<Run<'a>> {
        let finished = self.previous.map(|previous| Run {
            line: previous,
            count: self.count,
        });
        self.previous = None;
        self.previous_key = None;
        self.count = 0;
        finished
    }

    /// Decide whether `run` is reported: C's `show()` guard, flags composed.
    pub fn keep(&self, run: &Run<'_>) -> bool {
        self.flags.keeps(run.count)
    }

    /// Format one kept run into `out`, returning the used byte count. With `-c`
    /// C writes `"%4d %s"` (`uniq.c`'s `show()`), so the count is right-justified
    /// in four columns before the single separating blank — `1` becomes three
    /// spaces and a digit, and a five-digit count simply overflows the field.
    pub fn format(&self, run: &Run<'_>, out: &mut [u8]) -> Result<usize, TextError> {
        let mut written = 0;
        if self.flags.count {
            let digits = decimal_digits(run.count);
            for _ in digits..4 {
                written = write_byte(out, written, b' ')?;
            }
            written = write_decimal(run.count, out, written)?;
            written = write_byte(out, written, b' ')?;
        }
        for byte in run.line.text().as_bytes() {
            written = write_byte(out, written, *byte)?;
        }
        Ok(written)
    }
}

fn write_byte(out: &mut [u8], mut written: usize, byte: u8) -> Result<usize, TextError> {
    if written >= out.len() {
        return Err(TextError::TooLong);
    }
    out[written] = byte;
    written += 1;
    Ok(written)
}

/// How many columns C's `%4d` fills with `value`'s digits — zero is one digit,
/// not none (this is the blank-padding decision, kept apart from the digit
/// writing below so both stay readable).
fn decimal_digits(mut value: u64) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

fn write_decimal(mut value: u64, out: &mut [u8], mut written: usize) -> Result<usize, TextError> {
    if value == 0 {
        return write_byte(out, written, b'0');
    }
    let mut digits = [0u8; 20];
    let mut count = 0;
    while value > 0 {
        digits[count] = b'0' + (value % 10) as u8;
        value /= 10;
        count += 1;
    }
    while count > 0 {
        count -= 1;
        written = write_byte(out, written, digits[count])?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use crate::window::Line;
    use super::*;

    fn collect(flags: UniqFlags, lines: &[&str]) -> Vec<(String, u64)> {
        let mut handler = Uniq::new(flags);
        let mut runs = Vec::new();
        for line in lines {
            if let Some(run) = handler.push(Line::from(*line)) {
                if handler.keep(&run) {
                    runs.push((run.line.text().to_string(), run.count));
                }
            }
        }
        if let Some(run) = handler.finish() {
            if handler.keep(&run) {
                runs.push((run.line.text().to_string(), run.count));
            }
        }
        runs
    }

    const NONE: UniqFlags = UniqFlags {
        count: false,
        repeats_only: false,
        uniques_only: false,
    };
    const COUNTED: UniqFlags = UniqFlags {
        count: true,
        ..NONE
    };
    const REPEATED: UniqFlags = UniqFlags {
        repeats_only: true,
        ..NONE
    };
    const UNIQUE: UniqFlags = UniqFlags {
        uniques_only: true,
        ..NONE
    };

    /// One run of the line `a` with `count` copies, for the format tests.
    fn run_of(count: u64) -> Run<'static> {
        Run {
            line: Line::new("a"),
            count,
        }
    }

    #[test]
    fn test_collapses_adjacent() {
        assert_eq!(
            collect(NONE, &["a", "a", "b", "a"]),
            [
                ("a".to_string(), 2),
                ("b".to_string(), 1),
                ("a".to_string(), 1)
            ]
        );
    }

    #[test]
    fn test_repeated_and_unique_modes() {
        let lines = ["a", "a", "b"];
        assert_eq!(collect(REPEATED, &lines), [("a".to_string(), 2)]);
        assert_eq!(collect(UNIQUE, &lines), [("b".to_string(), 1)]);
    }

    #[test]
    fn test_flag_combinations_follow_the_show_guard() {
        // C keeps three independent ints, so combinations are legal and each
        // falls out of `show()`'s guard rather than being rejected as a clash:
        // `-d -u` suppresses every run, `-c -d` counts the repeated ones, `-c -u`
        // the unique ones.
        let lines = ["a", "a", "b"];
        let both = UniqFlags {
            repeats_only: true,
            uniques_only: true,
            ..NONE
        };
        assert!(collect(both, &lines).is_empty());
        let count_repeated = UniqFlags {
            count: true,
            repeats_only: true,
            ..NONE
        };
        assert_eq!(collect(count_repeated, &lines), [("a".to_string(), 2)]);
        let count_unique = UniqFlags {
            count: true,
            uniques_only: true,
            ..NONE
        };
        assert_eq!(collect(count_unique, &lines), [("b".to_string(), 1)]);
        // `-c` alone changes only the prefix, never which runs are kept.
        assert_eq!(collect(COUNTED, &lines), collect(NONE, &lines));
    }

    #[test]
    fn test_counted_format() {
        // C prints `"%4d %s"`: the count right-justified in four columns, then
        // one blank, then the line.
        let handler = Uniq::new(COUNTED);
        let mut out = [0u8; 16];
        let len = handler.format(&run_of(12), &mut out).unwrap();
        assert_eq!(&out[..len], b"  12 a");
    }

    #[test]
    fn test_counted_format_pads_to_four_columns() {
        let handler = Uniq::new(COUNTED);
        let mut out = [0u8; 16];
        let len = handler.format(&run_of(1), &mut out).unwrap();
        assert_eq!(&out[..len], b"   1 a");
        let len = handler.format(&run_of(9999), &mut out).unwrap();
        assert_eq!(&out[..len], b"9999 a");
        // A count wider than the field overflows it instead of truncating, the
        // way a `printf` width never cuts digits.
        let len = handler.format(&run_of(12345), &mut out).unwrap();
        assert_eq!(&out[..len], b"12345 a");
    }

    #[test]
    fn test_empty_input_no_runs() {
        let mut handler = Uniq::new(NONE);
        assert_eq!(handler.finish(), None);
    }

    #[test]
    fn test_comparison_key_field_skip() {
        // Skip N whitespace-delimited fields; the blank ending each is consumed.
        assert_eq!(comparison_key(b"aa bb cc", 1, 0), b"bb cc");
        assert_eq!(comparison_key(b"aa bb cc", 2, 0), b"cc");
        assert_eq!(comparison_key(b"  lead blanks", 1, 0), b"blanks");
    }

    #[test]
    fn test_comparison_key_char_skip() {
        // With no field skip, `-s N` advances N bytes from the start.
        assert_eq!(comparison_key(b"abcdef", 0, 2), b"cdef");
        assert_eq!(comparison_key(b"abcdef", 0, 0), b"abcdef");
    }

    #[test]
    fn test_skip_fields_collapses_on_key_not_full_line() {
        // -f 1: "foo SAME" and "bar SAME" share the key "SAME", so they form one
        // run reported as the first full line (uniq.c compares skip(), shows all).
        let mut handler = Uniq::new_with_skip(NONE, 1, 0);
        let mut runs: Vec<(String, u64)> = Vec::new();
        for line in ["foo SAME", "bar SAME", "baz OTHER"] {
            if let Some(run) = handler.push(Line::from(line)) {
                runs.push((run.line.text().to_string(), run.count));
            }
        }
        if let Some(run) = handler.finish() {
            runs.push((run.line.text().to_string(), run.count));
        }
        assert_eq!(
            runs,
            [("foo SAME".to_string(), 2), ("baz OTHER".to_string(), 1)]
        );
    }
}
