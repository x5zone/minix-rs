//! `renice` — reset the scheduling priority of running processes, the pure
//! argument layer.
//!
//! Ground truth: `minix3/usr.bin/renice/renice.c` (NetBSD 1.18). The program
//! walks its arguments (`main`, lines 62-113), parses the priority and the id
//! list with `getnum` (lines 117-132), decides process / process-group / user
//! scope, and for each target `donice` (lines 136-156) reads the old nice
//! value, optionally treats the priority as an increment, clamps to
//! `[PRIO_MIN, PRIO_MAX]`, and sets it. The `getpriority`/`setpriority`
//! syscalls and the printing live in the thin bin `src/bin/renice.rs`.
//!
//! [`getnum`] reproduces C's `strtol(str, &ep, 0)` base-selection: an optional
//! sign, a `0x`/`0X` hex prefix, a leading `0` meaning octal, otherwise
//! decimal — and, like `getnum`, it rejects any string with trailing
//! unconsumed characters (`*ep`) or a range error, while accepting the empty
//! string as `0` (strtol consumes nothing and `*ep` is `'\0'`).
//!
//! [`clamp_prio`] mirrors `donice`: with the `-n` increment flag the priority
//! is added to the current value before clamping.

use alloc::string::String;
use alloc::vec::Vec;

/// `sys/sys/resource.h:46-48` — the `which` codes for get/setpriority.
pub const PRIO_PROCESS: i32 = 0;
/// `sys/sys/resource.h:47`.
pub const PRIO_PGRP: i32 = 1;
/// `sys/sys/resource.h:48`.
pub const PRIO_USER: i32 = 2;
/// `sys/sys/resource.h:43` (lowest niceness).
pub const PRIO_MIN: i32 = -20;
/// `sys/sys/resource.h:44` (highest niceness).
pub const PRIO_MAX: i32 = 20;

/// The scope selector implied by the `-p`/`-g`/`-u` switches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Process,
    Pgrp,
    User,
}

impl Scope {
    pub fn code(self) -> i32 {
        match self {
            Scope::Process => PRIO_PROCESS,
            Scope::Pgrp => PRIO_PGRP,
            Scope::User => PRIO_USER,
        }
    }
}

/// One resolved operand after the priority. `Numeric` is an explicit id in the
/// current scope; `User` needs `getpwnam` (deferred to the bin); `Bad` is a
/// value that failed `getnum` or a negative id, which C reports and skips
/// (contributing to the error count rather than the action list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Numeric(Scope, i32),
    User(String),
    Bad(String),
}

/// A parsed `renice` invocation: the increment flag, the parsed priority, and
/// the target operands in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub incr: bool,
    pub prio: i32,
    pub targets: Vec<Target>,
}

/// Why a top-level argument list was rejected. `Usage` is C's `argc < 2`
/// (nothing to act on); `BadPriority` is a failing `getnum` on the priority
/// operand (which C reports with a distinct message but also exits 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    Usage,
    /// The priority token failed `getnum`; carries the token for C's
    /// `warnx("Bad %s argument: %s", "priority", str)` line.
    BadPriority(String),
}

/// C `getnum` (`renice.c:117-132`): parse `s` as a `strtol(..., 0)` integer.
/// Returns `None` when the parse leaves trailing characters (`*ep != '\0'`) or
/// overflows (the `LONG_MIN`/`LONG_MAX`+`ERANGE` check); an empty string yields
/// `Some(0)`, matching strtol consuming nothing with `ep == str`.
pub fn getnum(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let mut i = 0usize;
    let neg = if i < b.len() && b[i] == b'-' {
        i += 1;
        true
    } else if i < b.len() && b[i] == b'+' {
        i += 1;
        false
    } else {
        false
    };
    // Choose the base from the (post-sign) prefix, exactly as strtol base 0.
    let (radix, digit_start) =
        if i + 1 < b.len() && b[i] == b'0' && (b[i + 1] == b'x' || b[i + 1] == b'X') {
            (16, i + 2)
        } else if i < b.len() && b[i] == b'0' {
            // Leading '0' selects octal; the digits run from the '0' onward.
            (8, i)
        } else {
            (10, i)
        };
    let mut j = digit_start;
    while j < b.len() && (b[j] as char).is_digit(radix) {
        j += 1;
    }
    // strtol consumed [digit_start, j); it is an error unless the whole string
    // was digits (i.e. j reached the end) and at least one digit was eaten —
    // except the empty input, which strtol reports as 0 with ep == str.
    if s.is_empty() {
        return Some(0);
    }
    if j == digit_start || j != b.len() {
        return None;
    }
    let digits = &s[digit_start..j];
    let v = i64::from_str_radix(digits, radix).ok()?;
    Some(if neg { -v } else { v })
}

/// C `donice`'s value computation (`renice.c:142-149`): when `incr` (the `-n`
/// form) the parsed priority is added to the current nice value; the result is
/// clamped to `[PRIO_MIN, PRIO_MAX]`.
pub fn clamp_prio(old: i32, prio: i32, incr: bool) -> i32 {
    let v = if incr { old.saturating_add(prio) } else { prio };
    v.clamp(PRIO_MIN, PRIO_MAX)
}

impl Plan {
    /// Parse the operand list (`argv[1..]`, without the program name),
    /// reproducing `renice.c`'s structure: optional leading `-n`, then the
    /// priority, then the id operands each of which may be preceded by a
    /// `-p`/`-g`/`-u` scope switch. A negative id or one that fails `getnum`
    /// becomes [`Target::Bad`] so the bin can count it as C does.
    pub fn parse(args: &[&str]) -> Result<Plan, PlanError> {
        // C requires at least a priority and one target (argc < 2 -> usage).
        if args.len() < 2 {
            return Err(PlanError::Usage);
        }
        let mut i = 0;
        let mut incr = false;
        if args[0] == "-n" {
            incr = true;
            i = 1;
            if i >= args.len() {
                return Err(PlanError::Usage);
            }
        }
        let prio =
            getnum(args[i]).ok_or_else(|| PlanError::BadPriority(String::from(args[i])))? as i32;
        i += 1;

        let mut scope = Scope::Process;
        let mut targets = Vec::new();
        while i < args.len() {
            let a = args[i];
            i += 1;
            match a {
                "-g" => scope = Scope::Pgrp,
                "-u" => scope = Scope::User,
                "-p" => scope = Scope::Process,
                _ => {
                    if scope == Scope::User {
                        targets.push(Target::User(String::from(a)));
                    } else {
                        match getnum(a) {
                            Some(v) if v >= 0 && v <= i32::MAX as i64 => {
                                targets.push(Target::Numeric(scope, v as i32));
                            }
                            _ => targets.push(Target::Bad(String::from(a))),
                        }
                    }
                }
            }
        }
        Ok(Plan {
            incr,
            prio,
            targets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getnum_decimal_and_sign() {
        assert_eq!(getnum("10"), Some(10));
        assert_eq!(getnum("-5"), Some(-5));
        assert_eq!(getnum("+7"), Some(7));
        assert_eq!(getnum("0"), Some(0));
    }

    #[test]
    fn getnum_hex_and_octal_base_zero() {
        assert_eq!(getnum("0x1f"), Some(31));
        assert_eq!(getnum("0X20"), Some(32));
        assert_eq!(getnum("-0x10"), Some(-16));
        assert_eq!(getnum("010"), Some(8)); // leading 0 -> octal
        assert_eq!(getnum("0777"), Some(511));
    }

    #[test]
    fn getnum_rejects_trailing_and_invalid() {
        assert_eq!(getnum("12abc"), None); // trailing junk -> *ep != '\0'
        assert_eq!(getnum("abc"), None); // no digits consumed
        assert_eq!(getnum("078"), None); // '8' not octal, stops before end
        assert_eq!(getnum(""), Some(0)); // strtol: empty -> 0, ep == str
        assert_eq!(getnum("0x"), None); // hex prefix, no digits
    }

    #[test]
    fn getnum_overflow_rejected() {
        assert_eq!(getnum("99999999999999999999999"), None);
    }

    #[test]
    fn clamp_abs_and_incremented() {
        assert_eq!(clamp_prio(5, 10, false), 10); // absolute, ignores old
        assert_eq!(clamp_prio(5, 3, true), 8); // old 5 + incr 3
    }

    #[test]
    fn clamp_bounds() {
        assert_eq!(clamp_prio(15, 10, true), PRIO_MAX); // 25 -> 20
        assert_eq!(clamp_prio(-15, -10, true), PRIO_MIN); // -25 -> -20
        assert_eq!(clamp_prio(0, 100, false), PRIO_MAX);
        assert_eq!(clamp_prio(0, -100, false), PRIO_MIN);
    }

    #[test]
    fn parse_basic_priority_and_pids() {
        let p = Plan::parse(&["5", "100", "200"]).unwrap();
        assert!(!p.incr);
        assert_eq!(p.prio, 5);
        assert_eq!(
            p.targets,
            [
                Target::Numeric(Scope::Process, 100),
                Target::Numeric(Scope::Process, 200)
            ]
        );
    }

    #[test]
    fn parse_n_increment_flag() {
        let p = Plan::parse(&["-n", "3", "42"]).unwrap();
        assert!(p.incr);
        assert_eq!(p.prio, 3);
        assert_eq!(p.targets, [Target::Numeric(Scope::Process, 42)]);
    }

    #[test]
    fn parse_scope_switches_interspersed() {
        let p = Plan::parse(&["-n", "2", "-g", "7", "-p", "9", "-u", "root"]).unwrap();
        assert_eq!(
            p.targets,
            [
                Target::Numeric(Scope::Pgrp, 7),
                Target::Numeric(Scope::Process, 9),
                Target::User(String::from("root")),
            ]
        );
    }

    #[test]
    fn parse_bad_and_negative_ids() {
        let p = Plan::parse(&["1", "abc", "-3"]).unwrap();
        assert_eq!(
            p.targets,
            [
                Target::Bad(String::from("abc")),
                Target::Bad(String::from("-3"))
            ]
        );
    }

    #[test]
    fn parse_errors() {
        assert_eq!(Plan::parse(&["5"]), Err(PlanError::Usage)); // no target
        assert_eq!(Plan::parse(&[]), Err(PlanError::Usage));
        assert_eq!(Plan::parse(&["-n"]), Err(PlanError::Usage)); // -n but no priority
        assert_eq!(
            Plan::parse(&["bad", "1"]),
            Err(PlanError::BadPriority(String::from("bad")))
        );
    }
}
