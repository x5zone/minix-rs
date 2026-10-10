//! `sleep` — pause for a given number of seconds.
//!
//! Ground truth: `minix3/bin/sleep/sleep.c`. The interesting part is how the
//! single operand is turned into a `timespec`. C first scans the string for any
//! non-digit byte (`sleep.c:104-108`): if there is one it takes the *floating*
//! path (`atof`, then `floor`/fraction split, rejecting `val <= 0` as a usage
//! error), otherwise the *integer* path (`atol`, where `<= 0` means "no sleep at
//! all, exit success"). The asymmetry is real and reproduced here: `"0"` is an
//! integer so it sleeps zero and exits 0, whereas `"0.0"` has a `.` so it takes
//! the float path, sees `val <= 0`, and errors.
//!
//! The plan is returned as data so the caller owns the actual `nanosleep`
//! (and, in C, the `SIGINFO` progress report and the EINTR-remaining resume,
//! which need signal handling this build does not wire — see the `sleep` bin).
//!
//! Declared residual: the float path is a plain decimal parser (optional leading
//! space, optional sign, digits, one `.`), so it does not read exponents or the
//! other `atof` forms — `1e3` parses as `1`, not `1000`. Fractional digits past
//! the nanosecond place are truncated, matching C's `(int)` cast rather than a
//! rounding.

/// What the operand asks `sleep` to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepPlan {
    /// The duration is non-positive on the integer path: do nothing, succeed.
    NoSleep,
    /// Pause for `secs` whole seconds plus `nanos` nanoseconds.
    Sleep { secs: i64, nanos: i64 },
    /// A malformed or non-positive operand on the float path: a usage error.
    Bad,
}

/// C's `atof` restricted to a plain decimal: leading spaces, an optional sign,
/// an integer part, one optional fractional part. Stops at the first byte it
/// cannot use (so `1e3` yields `1`, a declared residual).
fn atof(arg: &[u8]) -> f64 {
    let mut i = 0;
    while i < arg.len() && (arg[i] == b' ' || arg[i] == b'\t') {
        i += 1;
    }
    let neg = matches!(arg.get(i), Some(b'-'));
    if matches!(arg.get(i), Some(b'-') | Some(b'+')) {
        i += 1;
    }
    let mut val = 0f64;
    while i < arg.len() && arg[i].is_ascii_digit() {
        val = val * 10.0 + (arg[i] - b'0') as f64;
        i += 1;
    }
    if arg.get(i) == Some(&b'.') {
        i += 1;
        let mut scale = 0.1;
        while i < arg.len() && arg[i].is_ascii_digit() {
            val += (arg[i] - b'0') as f64 * scale;
            scale *= 0.1;
            i += 1;
        }
    }
    if neg { -val } else { val }
}

/// Turn the raw operand bytes into a [`SleepPlan`], mirroring `sleep.c:104-124`.
pub fn plan(arg: &[u8]) -> SleepPlan {
    let has_nondigit = arg.iter().any(|&c| !c.is_ascii_digit());
    if has_nondigit {
        let val = atof(arg);
        if val <= 0.0 {
            return SleepPlan::Bad;
        }
        let ival = val as i64; // val > 0, so the cast floors
        let nanos = ((val - ival as f64) * 1_000_000_000.0) as i64;
        SleepPlan::Sleep { secs: ival, nanos }
    } else {
        let mut secs: i64 = 0;
        for &c in arg {
            secs = secs.saturating_mul(10).saturating_add((c - b'0') as i64);
        }
        if secs <= 0 {
            SleepPlan::NoSleep
        } else {
            SleepPlan::Sleep { secs, nanos: 0 }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_seconds() {
        assert_eq!(plan(b"5"), SleepPlan::Sleep { secs: 5, nanos: 0 });
        assert_eq!(plan(b"01.5"), SleepPlan::Sleep { secs: 1, nanos: 500_000_000 });
    }

    #[test]
    fn zero_integer_does_not_sleep() {
        assert_eq!(plan(b"0"), SleepPlan::NoSleep);
        assert_eq!(plan(b"000"), SleepPlan::NoSleep);
        assert_eq!(plan(b""), SleepPlan::NoSleep);
    }

    #[test]
    fn fractional_split() {
        assert_eq!(plan(b"1.5"), SleepPlan::Sleep { secs: 1, nanos: 500_000_000 });
        assert_eq!(plan(b"2.0"), SleepPlan::Sleep { secs: 2, nanos: 0 });
        assert_eq!(plan(b"0.25"), SleepPlan::Sleep { secs: 0, nanos: 250_000_000 });
    }

    #[test]
    fn float_path_rejects_non_positive() {
        assert_eq!(plan(b"0.0"), SleepPlan::Bad);
        assert_eq!(plan(b"-3"), SleepPlan::Bad);
        assert_eq!(plan(b"abc"), SleepPlan::Bad);
    }

    #[test]
    fn leading_space_and_sign_take_float_path() {
        // " 5" has a non-digit (space) → atof → positive → integer-valued sleep.
        assert_eq!(plan(b" 5"), SleepPlan::Sleep { secs: 5, nanos: 0 });
        assert_eq!(plan(b"+2.5"), SleepPlan::Sleep { secs: 2, nanos: 500_000_000 });
    }

    #[test]
    fn nanosecond_digits_truncate() {
        // >9 fractional digits truncate at the ns place (C's (int) cast).
        assert_eq!(plan(b"1.9999999999"), SleepPlan::Sleep { secs: 1, nanos: 999_999_999 });
    }
}
