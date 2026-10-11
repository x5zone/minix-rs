//! `getopt(3)` — the BSD option parser behind the `getopt` command.
//!
//! Ground truth: `minix3/lib/libc/stdlib/getopt.c` (the NetBSD classic,
//! non-permuting parser). This is a faithful port of that one function's
//! state machine: the C globals `optind`/`optarg`/`opterr`/`optreset` become
//! struct fields, and the `static const char *place` that persists across
//! calls becomes a byte slice borrowed from the argument vector (`place`
//! empty means "pointing at the NUL / `EMSG`"). `opterr` is folded to *on*
//! (the value `getopt(1)` runs with); the two stderr messages that C prints
//! inside the parser are returned to the caller instead, so this module stays
//! free of I/O and `no_std`-pure like the rest of the crate.
//!
//! The parser is deliberately the *classic* one: it stops at the first
//! non-option argument (no operand permutation) and honours only the single
//! `getopt` spellings — a bare letter (flag), `letter:` (required argument),
//! `letter::` (GNU optional argument), a leading `:` in the option string
//! (missing-argument reported as a colon), and `-` as a literal option when
//! the option string lists it. `getopt_long` (`--word`) is out of scope, as
//! in this file.

/// One result of a parser step, mirroring the `int` that C `getopt` returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome<'a> {
    /// A recognised option letter (C returned the letter). `argument` is the
    /// option's value when the option string asked for one (`letter:` took an
    /// attached or following argument, or `letter::` took an attached one);
    /// `None` for a plain flag, and also for `letter::` with nothing attached.
    Found {
        /// The option letter.
        letter: u8,
        /// The argument, if the option carried one.
        argument: Option<&'a [u8]>,
    },
    /// An unknown option character (C returned `?`). The caller prints
    /// `progname: unknown option -- X` unless the option string begins with
    /// `:` (then C prints nothing), and fails like `getopt(1)`.
    Unknown {
        /// The offending character.
        letter: u8,
    },
    /// An option requiring an argument had none. C returns `:` when the option
    /// string begins with `:` (`colon_mode`), otherwise `?` after printing
    /// `progname: option requires an argument -- X`.
    MissingArgument {
        /// The option letter that wanted an argument.
        letter: u8,
        /// Whether the option string began with `:` (colon reporting mode).
        colon_mode: bool,
    },
    /// End of options (C returned `-1`). Operands begin at `next_index` in the
    /// argument vector.
    End {
        /// Index of the first operand (`optind`).
        next_index: usize,
    },
}

/// The parser state, threaded across calls like C's globals. Borrowed over
/// `'a` so `place` points into the caller's argument vector — no unsafe.
#[derive(Debug, Clone, Copy)]
pub struct Getopt<'a> {
    optind: usize,
    place: &'a [u8],
}

/// Read the byte at `place`, treating a slice exhausted as the NUL C sees.
fn peek(place: &[u8]) -> u8 {
    place.first().copied().unwrap_or(0)
}

/// Advance `place` by one byte (`*place++`); a no-op once at the NUL.
fn bump(place: &[u8]) -> &[u8] {
    &place[1.min(place.len())..]
}

impl<'a> Getopt<'a> {
    /// A parser positioned at `optind` (C sets this to 2 in `getopt(1)`, past
    /// the program name and the option letters). `place` starts empty (`EMSG`).
    pub fn new(optind: usize) -> Self {
        Getopt { optind, place: b"" }
    }

    /// The current `optind`: where operands begin once [`Getopt::next`]
    /// returns [`Outcome::End`].
    pub fn optind(&self) -> usize {
        self.optind
    }

    /// Parse one option from `argv` against `optstring`, advancing the state.
    /// Mirrors `getopt.c:62-142` call-for-call.
    pub fn next(&mut self, argv: &[&'a [u8]], optstring: &[u8]) -> Outcome<'a> {
        // `optreset` is never set here, so the branch runs on `*place == 0`.
        let optopt: u8 = if peek(self.place) == 0 {
            // Update the scanning pointer: grab the next argv entry.
            self.place = b"";
            if self.optind >= argv.len() || argv[self.optind].first() != Some(&b'-') {
                return Outcome::End {
                    next_index: self.optind,
                };
            }
            let arg = argv[self.optind];
            self.place = &arg[1..]; // `place = nargv[optind]`, past the '-'
            let optopt = peek(self.place);
            self.place = bump(self.place); // `optopt = *place++`
            if optopt == b'-' && peek(self.place) == 0 {
                // `--` ends option processing.
                self.optind += 1;
                self.place = b"";
                return Outcome::End {
                    next_index: self.optind,
                };
            }
            if optopt == 0 {
                // Solitary `-`: a literal `-` option only if the string lists
                // it, otherwise end of options.
                self.place = b"";
                if !optstring.contains(&b'-') {
                    return Outcome::End {
                        next_index: self.optind,
                    };
                }
                b'-'
            } else {
                optopt
            }
        } else {
            // Mid-cluster: take the next letter of the current argument.
            let optopt = peek(self.place);
            self.place = bump(self.place);
            optopt
        };

        let colon_mode = optstring.first() == Some(&b':');

        // `optopt == ':'` or not present in the option string -> unknown.
        let oli = match optstring.iter().position(|&c| c == optopt) {
            Some(i) if optopt != b':' => i,
            _ => {
                if peek(self.place) == 0 {
                    self.optind += 1;
                }
                // C prints the message unless the string begins with `:`; the
                // caller decides rendering.
                return Outcome::Unknown { letter: optopt };
            }
        };

        // Does this option take an argument? `oli[1] == ':'`.
        if optstring.get(oli + 1) != Some(&b':') {
            if peek(self.place) == 0 {
                self.optind += 1;
            }
            return Outcome::Found {
                letter: optopt,
                argument: None,
            };
        }

        // An argument: the rest of this argument, the next argument, or (for
        // `letter::`) nothing.
        if peek(self.place) != 0 {
            let value = self.place;
            self.place = b"";
            self.optind += 1;
            Outcome::Found {
                letter: optopt,
                argument: Some(value),
            }
        } else if optstring.get(oli + 2) == Some(&b':') {
            // GNU optional argument with nothing present -> no argument.
            self.place = b"";
            self.optind += 1;
            Outcome::Found {
                letter: optopt,
                argument: None,
            }
        } else {
            // The value would be the entire next argument. C's
            // `nargc > ++optind` pre-increments `optind` even when the
            // comparison then fails, so a missing argument still advances
            // the cursor — a spin-free faithful port (without the increment
            // the classic `getopt(1)` demo would loop forever on `-x` with no
            // value).
            self.optind += 1;
            if self.optind < argv.len() {
                let value = argv[self.optind];
                self.optind += 1; // the trailing `++optind` after `optarg=`
                self.place = b"";
                return Outcome::Found {
                    letter: optopt,
                    argument: Some(value),
                };
            }
            self.place = b"";
            Outcome::MissingArgument {
                letter: optopt,
                colon_mode,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Collect every step of a parse into a Vec for readable assertions. The
    // argument vector is passed as byte slices (matching C's raw argv).
    fn run<'a>(argv: &[&'a [u8]], optstring: &[u8], optind: usize) -> Vec<Outcome<'a>> {
        let mut g = Getopt::new(optind);
        let mut out = Vec::new();
        loop {
            let step = g.next(argv, optstring);
            let done = matches!(step, Outcome::End { .. });
            out.push(step);
            if done {
                break;
            }
        }
        out
    }

    #[test]
    fn plain_flags_in_one_cluster() {
        // `-ab` with optstring "ab" yields two flags, no operands.
        let steps = run(&[&b"getopt"[..], &b"ab"[..], &b"-ab"[..]], b"ab", 2);
        assert_eq!(
            steps,
            vec![
                Outcome::Found {
                    letter: b'a',
                    argument: None
                },
                Outcome::Found {
                    letter: b'b',
                    argument: None
                },
                Outcome::End { next_index: 3 },
            ]
        );
    }

    #[test]
    fn flag_with_attached_argument() {
        // `-nVALUE`: optstring "n:"; the rest of the token is the value.
        let steps = run(&[&b"getopt"[..], &b"n:"[..], &b"-nfoo"[..]], b"n:", 2);
        assert_eq!(
            steps,
            vec![
                Outcome::Found {
                    letter: b'n',
                    argument: Some(&b"foo"[..])
                },
                Outcome::End { next_index: 3 },
            ]
        );
    }

    #[test]
    fn flag_with_separate_argument() {
        // `-n foo`: value is the following argument.
        let steps = run(
            &[&b"getopt"[..], &b"n:"[..], &b"-n"[..], &b"foo"[..]],
            b"n:",
            2,
        );
        assert_eq!(
            steps,
            vec![
                Outcome::Found {
                    letter: b'n',
                    argument: Some(&b"foo"[..])
                },
                Outcome::End { next_index: 4 },
            ]
        );
    }

    #[test]
    fn stops_at_first_operand_no_permutation() {
        // The classic parser stops at `foo`; a later `-x` is an operand, not
        // re-ordered into options.
        let steps = run(
            &[&b"getopt"[..], &b"x"[..], &b"foo"[..], &b"-x"[..]],
            b"x",
            2,
        );
        assert_eq!(steps, vec![Outcome::End { next_index: 2 }]);
    }

    #[test]
    fn double_dash_ends_options_and_operands_follow() {
        let steps = run(
            &[&b"getopt"[..], &b"x"[..], &b"--"[..], &b"a"[..], &b"b"[..]],
            b"x",
            2,
        );
        assert_eq!(steps, vec![Outcome::End { next_index: 3 }]);
    }

    #[test]
    fn unknown_option_reported() {
        // `-z` not in optstring "a" -> Unknown, still advances past it.
        let steps = run(&[&b"getopt"[..], &b"a"[..], &b"-z"[..]], b"a", 2);
        assert_eq!(
            steps,
            vec![
                Outcome::Unknown { letter: b'z' },
                Outcome::End { next_index: 3 }
            ]
        );
    }

    #[test]
    fn missing_required_argument_not_colon_mode() {
        // optstring "a:" with `-a` last and nothing after -> MissingArgument,
        // colon_mode false (C would print "option requires an argument").
        let steps = run(&[&b"getopt"[..], &b"a:"[..], &b"-a"[..]], b"a:", 2);
        assert_eq!(
            steps,
            vec![
                Outcome::MissingArgument {
                    letter: b'a',
                    colon_mode: false
                },
                Outcome::End { next_index: 3 },
            ]
        );
    }

    #[test]
    fn colon_mode_reports_missing_argument_with_colon() {
        // optstring ":a:" (leading colon) -> colon_mode true.
        let steps = run(&[&b"getopt"[..], &b":a:"[..], &b"-a"[..]], b":a:", 2);
        assert_eq!(
            steps,
            vec![
                Outcome::MissingArgument {
                    letter: b'a',
                    colon_mode: true
                },
                Outcome::End { next_index: 3 },
            ]
        );
    }

    #[test]
    fn gnu_optional_argument_present_and_absent() {
        // "o::": `-oval` -> Some(val); bare `-o` -> None (optional).
        let attached = run(&[&b"getopt"[..], &b"o::"[..], &b"-oX"[..]], b"o::", 2);
        assert_eq!(
            attached[0],
            Outcome::Found {
                letter: b'o',
                argument: Some(&b"X"[..])
            }
        );
        let bare = run(&[&b"getopt"[..], &b"o::"[..], &b"-o"[..]], b"o::", 2);
        assert_eq!(
            bare[0],
            Outcome::Found {
                letter: b'o',
                argument: None
            }
        );
    }

    #[test]
    fn literal_dash_option_when_listed() {
        // optstring "a-" lists '-', so a bare `-` parses as option '-'.
        let steps = run(&[&b"getopt"[..], &b"a-"[..], &b"-"[..]], b"a-", 2);
        assert_eq!(
            steps[0],
            Outcome::Found {
                letter: b'-',
                argument: None
            }
        );
    }

    #[test]
    fn mixed_flags_and_argument() {
        // "abc:" (a, b flags; c takes an argument): `-abcVALUE` -> a flag,
        // b flag, c with "VALUE".
        let steps = run(
            &[&b"getopt"[..], &b"abc:"[..], &b"-abcVALUE"[..]],
            b"abc:",
            2,
        );
        assert_eq!(
            steps,
            vec![
                Outcome::Found {
                    letter: b'a',
                    argument: None
                },
                Outcome::Found {
                    letter: b'b',
                    argument: None
                },
                Outcome::Found {
                    letter: b'c',
                    argument: Some(&b"VALUE"[..])
                },
                Outcome::End { next_index: 3 },
            ]
        );
    }
}
