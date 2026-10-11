//! `truncate` — set a file's length: the pure size grammar and decision layer.
//!
//! Ground truth: `minix3/minix/commands/truncate/truncate.c`. The option scan is
//! `getopt(argc, argv, "cr:s:")` at line 57, the rule that exactly one of
//! `-s`/`-r` was given is at line 89, and `parselength` at lines 150 to 198 turns
//! `[+-]?[0-9]+[GMK]` into an `off_t`. The filesystem legs — `stat`, `open`,
//! `fstat`, `ftruncate`, `close` — live in the thin program `src/bin/truncate.rs`.
//!
//! The size grammar is the interesting part, and two of its quirks are load
//! bearing: the multiplier `switch` falls through (`G` multiplies by 1024 three
//! times, so `1G` is a gibibyte rather than `1 * 1024`), and the "nothing may
//! follow the multiplier" test lives in the `K` arm (`truncate.c:187-188`), which
//! means it also gates `G` and `M` — `1KB` and `1Gx` are both refused. A leading
//! `+` or `-` in the text is what makes the size relative (`truncate.c:72`), and
//! the negative half of that is only meaningful because the resulting length is
//! clamped to zero at `truncate.c:131`.

use crate::cp;
use alloc::string::String;
use alloc::vec::Vec;

/// `truncate.c:106`: the mode a missing file is created with — `S_IRUSR |
/// S_IWUSR | S_IRGRP | S_IROTH`, i.e. `0644`. There is no execute bit anywhere,
/// and the caller's creation mask still applies on top.
pub const CREATE_MODE: u32 = 0o644;

/// `truncate.c:102-105`: `-c` drops `O_CREAT`; without it a missing operand is
/// created.
pub fn open_flags(no_create: bool) -> i32 {
    if no_create {
        cp::O_WRONLY
    } else {
        cp::O_WRONLY | cp::O_CREAT
    }
}

/// Why a `-s` argument could not be turned into a length. C has one answer for
/// both shapes — `parselength` returns -1 and the caller prints
/// "invalid size argument \`%s'" (`truncate.c:67-71`) — so the two variants exist
/// to keep the two failure shapes visible in the tests, not because the program
/// treats them differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthError {
    /// A character the grammar does not allow: a non-digit that is not one of
    /// the six multiplier letters, or a multiplier with anything after it.
    Malformed,
    /// The digit string or one of the multiplications overflowed. C guards each
    /// step with `ASSIGN_CHK_OFLOW` (`truncate.c:166`, applied at `:172-173` and
    /// `:179-190`).
    Overflow,
}

/// What the length comes from (`truncate.c:89-100`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SizeSource {
    /// `-s SIZE` without a sign: every operand gets exactly this length.
    Absolute(i64),
    /// `-s +SIZE` or `-s -SIZE`: each operand's own length is the start point.
    Relative(i64),
    /// `-r RFILE`: the length is that file's `st_size`, read once up front.
    Reference(String),
}

/// The parsed command line, ready for the filesystem leg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruncatePlan {
    /// `-c`: do not create a missing operand (`truncate.c:59-61`).
    pub no_create: bool,
    pub source: SizeSource,
    /// The files to truncate, in the order given (`truncate.c:108`).
    pub files: Vec<String>,
}

/// Why the command line is not a `truncate` invocation. The two shapes differ
/// because C prints them differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TruncateArgError {
    /// Usage and exit 1 — no operand file, `-s` or `-r` without a value, both
    /// `-s` and `-r` together, neither of them, or an unknown letter
    /// (`truncate.c:89-90` with `:76-78` and `:201-208`).
    Usage,
    /// `-s` whose value `parselength` refused. C prints "invalid size argument
    /// \`%s'" and exits at once, before looking at any file (`truncate.c:67-71`).
    InvalidSize(String),
}

/// `parselength` (`truncate.c:150-198`): `[+-]?[0-9]+[GMKgmk]?` to a length in
/// bytes, sign included. The multiplier arms fall through on purpose, so `K` is
/// one factor of 1024, `M` two and `G` three, and the trailing-character test
/// written in the `K` arm therefore also gates `G` and `M`.
pub fn parse_length(text: &str) -> Result<i64, LengthError> {
    let bytes = text.as_bytes();
    let mut pos = 0;
    let mut sign: i64 = 1;
    // C's switch has no `break` between `-` and `+` (`truncate.c:159-164`), so a
    // leading `-` steps over the sign as well; a second sign character then fails
    // the digit scan and the multiplier switch below refuses it.
    if pos < bytes.len() && (bytes[pos] == b'-' || bytes[pos] == b'+') {
        if bytes[pos] == b'-' {
            sign = -1;
        }
        pos += 1;
    }

    let mut length: i64 = 0;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        let digit = (bytes[pos] - b'0') as i64;
        length = length
            .checked_mul(10)
            .and_then(|v| v.checked_add(digit))
            .ok_or(LengthError::Overflow)?;
        pos += 1;
    }

    // The multiplier arms, in the order the C `switch` falls through them
    // (`truncate.c:176-195`). An empty digit string is legal and means zero: the
    // digit loop may simply not run (`:171-174`).
    let factors: u32 = match bytes.get(pos) {
        Some(b'G') | Some(b'g') => 3,
        Some(b'M') | Some(b'm') => 2,
        Some(b'K') | Some(b'k') => 1,
        None => 0,
        Some(_) => return Err(LengthError::Malformed),
    };
    if factors > 0 && pos + 1 < bytes.len() {
        // `truncate.c:187-188`: `if (ls[1] != '\0') return -1;`, reached from the
        // `G` and `M` arms by fall-through too.
        return Err(LengthError::Malformed);
    }
    for _ in 0..factors {
        length = length.checked_mul(1024).ok_or(LengthError::Overflow)?;
    }
    Ok(length * sign)
}

/// `truncate.c:57-82`: BSD `getopt` with `"cr:s:"`. `-r` and `-s` each take the
/// next argument as their value; the first non-option ends the scan and `--` ends
/// it too. Two `-s` letters let the last one win, because `got_size` is a flag
/// and not a counter (`truncate.c:74`), while `-s` together with `-r` is the
/// usage error C states as `do_refer + got_size != 1` (`truncate.c:89`).
pub fn parse_args(args: &[&str]) -> Result<TruncatePlan, TruncateArgError> {
    let mut no_create = false;
    let mut got_size = false;
    let mut do_refer = false;
    let mut size: Option<SizeSource> = None;
    let mut reference: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        if a == "--" {
            i += 1;
            break;
        }
        let bytes = a.as_bytes();
        // A bare "-", and anything not starting with one, is an operand: BSD
        // getopt stops scanning at the first non-option.
        if bytes.len() < 2 || bytes[0] != b'-' {
            break;
        }
        if !bytes[1..].iter().all(|b| matches!(b, b'c' | b'r' | b's')) {
            return Err(TruncateArgError::Usage);
        }
        let mut k = 1;
        while k < bytes.len() {
            let letter = bytes[k];
            if letter == b'c' {
                no_create = true;
                k += 1;
                continue;
            }
            // `r` and `s` both want a value. C's getopt only reads it from the
            // following argv entry (`optarg`), so a glued form (`-s10`) is
            // refused rather than guessed at.
            if bytes.len() != k + 1 {
                return Err(TruncateArgError::Usage);
            }
            let value = match args.get(i + 1) {
                Some(v) => *v,
                None => return Err(TruncateArgError::Usage),
            };
            if letter == b'r' {
                do_refer = true;
                reference = Some(String::from(value));
            } else {
                let sz = match parse_length(value) {
                    Ok(sz) => sz,
                    Err(_) => {
                        return Err(TruncateArgError::InvalidSize(String::from(value)));
                    }
                };
                let v = value.as_bytes();
                got_size = true;
                size = Some(if v.first() == Some(&b'+') || v.first() == Some(&b'-') {
                    // `truncate.c:72-74`: the sign in the text sets
                    // `do_relative`; the value itself is already signed.
                    SizeSource::Relative(sz)
                } else {
                    SizeSource::Absolute(sz)
                });
            }
            i += 1; // consume the value together with the option letter
            k += 1;
        }
        i += 1;
    }

    let files: Vec<String> = args[i..].iter().map(|s| String::from(*s)).collect();
    // `truncate.c:89`: exactly one source and at least one operand file.
    if (got_size && do_refer) || (!got_size && !do_refer) || files.is_empty() {
        return Err(TruncateArgError::Usage);
    }
    let source = if got_size {
        size.expect("-s set the size")
    } else {
        SizeSource::Reference(reference.expect("-r set the reference"))
    };
    Ok(TruncatePlan {
        no_create,
        source,
        files,
    })
}

/// `truncate.c:117-131` with `:98`: fold a relative size onto the length the
/// operand already has, then clamp. C's `EFBIG` attempt at line 123 compares
/// `oflow` against the very expression it was just assigned from, so the branch
/// cannot fire; the guard that does work is the negative clamp eight lines later.
/// Saturating arithmetic replaces the wrapped add without changing that clamp's
/// answer for the wraps that land on the negative side.
pub fn relative_target(current: i64, delta: i64) -> i64 {
    clamp(current.saturating_add(delta))
}

/// `truncate.c:131-132`: `if (tsize < 0) tsize = 0;`. The clamp runs for every
/// path — absolute, relative and referenced alike.
pub fn clamp(size: i64) -> i64 {
    size.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // ── the size grammar ─────────────────────────────────────────────────

    #[test]
    fn plain_digits_are_bytes() {
        assert_eq!(parse_length("0"), Ok(0));
        assert_eq!(parse_length("512"), Ok(512));
        assert_eq!(parse_length("1048576"), Ok(1_048_576));
    }

    #[test]
    fn multipliers_fall_through_as_c_counts_them() {
        // truncate.c:176-190: K one factor, M two, G three — `1G` is a gibibyte.
        assert_eq!(parse_length("1K"), Ok(1024));
        assert_eq!(parse_length("1k"), Ok(1024));
        assert_eq!(parse_length("2M"), Ok(2 * 1024 * 1024));
        assert_eq!(parse_length("3m"), Ok(3 * 1024 * 1024));
        assert_eq!(parse_length("1G"), Ok(1024 * 1024 * 1024));
        assert_eq!(parse_length("1g"), Ok(1024 * 1024 * 1024));
    }

    #[test]
    fn a_sign_makes_the_length_negative() {
        assert_eq!(parse_length("+8"), Ok(8));
        assert_eq!(parse_length("-8"), Ok(-8));
        assert_eq!(parse_length("-2K"), Ok(-2048));
        assert_eq!(parse_length("-+5"), Err(LengthError::Malformed));
    }

    #[test]
    fn anything_after_a_multiplier_is_refused() {
        // The `ls[1] != '\0'` test sits in the K arm (truncate.c:187) and the G
        // and M arms fall through it, so all three spellings are covered.
        assert_eq!(parse_length("1KB"), Err(LengthError::Malformed));
        assert_eq!(parse_length("1Gx"), Err(LengthError::Malformed));
        assert_eq!(parse_length("1M "), Err(LengthError::Malformed));
        assert_eq!(parse_length("1X"), Err(LengthError::Malformed));
        assert_eq!(parse_length("0x10"), Err(LengthError::Malformed));
        assert_eq!(parse_length(""), Ok(0), "an empty digit string is zero");
    }

    #[test]
    fn overflow_fails_instead_of_wrapping() {
        // ASSIGN_CHK_OFLOW guards each digit step and each multiplier step
        // (truncate.c:166 with :172-173).
        assert_eq!(
            parse_length("99999999999999999999999"),
            Err(LengthError::Overflow)
        );
        assert_eq!(
            parse_length("9999999999G"),
            Err(LengthError::Overflow),
            "the multiplication by 1024 cubed cannot fit"
        );
    }

    // ── the option face ──────────────────────────────────────────────────

    #[test]
    fn exactly_one_size_source_with_a_file() {
        assert_eq!(
            parse_args(&["-s", "100", "f"]),
            Ok(TruncatePlan {
                no_create: false,
                source: SizeSource::Absolute(100),
                files: vec![String::from("f")],
            })
        );
        assert_eq!(
            parse_args(&["-r", "ref", "f"]),
            Ok(TruncatePlan {
                no_create: false,
                source: SizeSource::Reference(String::from("ref")),
                files: vec![String::from("f")],
            })
        );
        // Both sources, neither source, and a missing operand are all usage.
        assert_eq!(
            parse_args(&["-s", "1", "-r", "ref", "f"]),
            Err(TruncateArgError::Usage),
            "truncate.c:89 wants do_refer + got_size == 1"
        );
        assert_eq!(parse_args(&["f"]), Err(TruncateArgError::Usage));
        assert_eq!(
            parse_args(&["-s", "1", "-c"]),
            Err(TruncateArgError::Usage),
            "a size with no operand file is not an invocation"
        );
    }

    #[test]
    fn the_last_size_letter_wins_as_c_flags_it() {
        // got_size is a flag, not a counter (truncate.c:74), so `-s 1 -s 2`
        // keeps 2.
        let p = parse_args(&["-s", "1", "-s", "2", "f"]).unwrap();
        assert_eq!(p.source, SizeSource::Absolute(2));
    }

    #[test]
    fn a_signed_size_is_relative() {
        assert_eq!(
            parse_args(&["-s", "-5", "f"]).unwrap().source,
            SizeSource::Relative(-5)
        );
        assert_eq!(
            parse_args(&["-s", "+5", "f"]).unwrap().source,
            SizeSource::Relative(5)
        );
        assert_eq!(
            parse_args(&["-s", "5", "f"]).unwrap().source,
            SizeSource::Absolute(5)
        );
    }

    #[test]
    fn c_flag_and_clusters_and_double_dash() {
        let p = parse_args(&["-cs", "10", "f"]).unwrap();
        assert!(p.no_create);
        assert_eq!(p.source, SizeSource::Absolute(10));
        let p = parse_args(&["-c", "-c", "-s", "10", "f"]).unwrap();
        assert!(p.no_create, "-c twice is still -c");
        assert_eq!(
            parse_args(&["--", "-s", "10", "f"]),
            Err(TruncateArgError::Usage),
            "no source survives --"
        );
        let p = parse_args(&["-s", "10", "--", "-weird"]).unwrap();
        assert_eq!(p.files, [String::from("-weird")]);
    }

    #[test]
    fn a_bad_size_is_reported_with_the_text_that_failed() {
        assert_eq!(
            parse_args(&["-s", "1KB", "f"]),
            Err(TruncateArgError::InvalidSize(String::from("1KB")))
        );
        // An empty -s value is the length 0, not a failure: parselength's digit
        // loop may run zero times (truncate.c:171-174).
        assert_eq!(
            parse_args(&["-s", "", "f"]).unwrap().source,
            SizeSource::Absolute(0)
        );
    }

    #[test]
    fn an_unknown_letter_and_a_missing_option_argument_are_usage() {
        assert_eq!(
            parse_args(&["-z", "-s", "1", "f"]),
            Err(TruncateArgError::Usage)
        );
        assert_eq!(
            parse_args(&["-s"]),
            Err(TruncateArgError::Usage),
            "-s needs a value"
        );
        assert_eq!(
            parse_args(&["-r"]),
            Err(TruncateArgError::Usage),
            "-r needs a value"
        );
        assert_eq!(
            parse_args(&["-s10", "f"]),
            Err(TruncateArgError::Usage),
            "a glued value is not the shape C's getopt reads here"
        );
    }

    #[test]
    fn open_flags_and_create_mode_follow_the_c_flag() {
        assert_eq!(open_flags(false), cp::O_WRONLY | cp::O_CREAT);
        assert_eq!(open_flags(true), cp::O_WRONLY, "-c drops O_CREAT");
        assert_eq!(CREATE_MODE, 0o644, "S_IRUSR|S_IWUSR|S_IRGRP|S_IROTH");
    }

    // ── the arithmetic ───────────────────────────────────────────────────

    #[test]
    fn relative_sizes_land_on_the_clamped_sum() {
        assert_eq!(relative_target(100, -30), 70);
        assert_eq!(relative_target(30, -100), 0, "truncate.c:131-132 clamps");
        assert_eq!(relative_target(30, 0), 30);
        // Saturating add keeps C's clamp answer for a negative-side wrap and
        // never reaches the EFBIG branch C cannot trigger (truncate.c:123).
        assert_eq!(relative_target(i64::MAX, 5), i64::MAX);
        assert_eq!(relative_target(i64::MIN / 2, i64::MIN / 2), 0);
        assert_eq!(clamp(-1), 0);
        assert_eq!(clamp(4096), 4096);
    }
}
