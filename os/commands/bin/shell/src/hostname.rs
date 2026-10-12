//! `hostname` option parsing and display trimming.
//!
//! Ground truth: `minix3/bin/hostname/hostname.c` — the `getopt(argc,
//! argv, "s")` scan around line 64 (the switch's `case 's'` sets the
//! flag, `case '?'` reaches `usage()`); `if (argc > 1) usage()` around
//! line 76; the `if (*argv)` split at line 78 dispatches to
//! `sethostname(*argv, strlen(*argv))` (line 80) or the read arm
//! (`gethostname(hostname, sizeof hostname)` line 83, the
//! `hostname[sizeof(hostname) - 1] = '\0'` clamp line 85, the
//! `strchr(hostname, '.')` trim line 86, and the `printf("%s\n",
//! hostname)` line 88). The sysctl round trip itself rides
//! [`minix_sys::gethostname`] / [`minix_sys::sethostname`] — the two
//! `{CTL_KERN, KERN_HOSTNAME}` wrappers in `libc/gen/gethostname.c` and
//! `libc/gen/sethostname.c` — so this module owns only the shape
//! decision: which arm, which flag, how to trim.
//!
//! Like [`crate::lexer`] and [`crate::redir`], the parsing here is pure
//! and unit-tested without a service or a syscall.

use crate::ShellError;

/// What `hostname` decided to do, parsed out of the argument vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan<'a> {
    /// No operand: read the current hostname and print it. `short`
    /// mirrors the `-s` flag (drop the domain suffix before display).
    Read { short: bool },
    /// One operand: publish `name` as the new hostname. The C code
    /// ignores `-s` on this arm (the trim lives only inside the read
    /// path, so `hostname -s foo.example.com` writes the full string,
    /// matching `hostname.c:78-81`).
    Write { name: &'a str },
}

/// Parse the argv slice (already stripped of `argv[0]`) into a [`Plan`].
///
/// C `getopt(argc, argv, "s")` walks option clusters byte by byte and
/// exits with usage on `?`/unknown; everything after `optind` is an
/// operand, and `if (argc > 1) usage()` refuses more than one. This
/// walker keeps that shape: a leading `-s` sets the short flag (repeat
/// occurrences stay idempotent, matching the C `case 's': sflag = 1`),
/// a bare `--` ends option scanning (the NetBSD `getopt` convention),
/// any other dash-prefixed token (including clusters like `-sx`) is
/// [`ShellError::InvalidSyntax`], and more than one operand likewise
/// refuses.
pub fn parse_plan<'a>(argv: &'a [&'a str]) -> Result<Plan<'a>, ShellError> {
    let mut short = false;
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i];
        if arg == "--" {
            i += 1;
            break;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            // The C switch has exactly one `case 's'`; every other
            // option byte — an unknown letter, a cluster tail after
            // `-s`, or a bare `-x` — falls through to `case '?'` and
            // usage. Refusing the whole cluster here is the same
            // answer at a coarser granularity.
            if arg == "-s" {
                short = true;
                i += 1;
                continue;
            }
            return Err(ShellError::InvalidSyntax);
        }
        break;
    }
    let operands = &argv[i..];
    match operands {
        [] => Ok(Plan::Read { short }),
        [name] => Ok(Plan::Write { name }),
        _ => Err(ShellError::InvalidSyntax),
    }
}

/// Trim the domain suffix from a hostname buffer (C `hostname.c:86-87`:
/// `if (sflag && (p = strchr(hostname, '.'))) *p = '\0';`).
///
/// Returns everything up to (but not including) the first `.`; when no
/// `.` is present the slice is returned unchanged. A leading `.` yields
/// the empty slice — exactly what C prints (an empty line), so callers
/// do not need a special case for it.
pub fn strip_domain(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|b| *b == b'.') {
        Some(dot) => &bytes[..dot],
        None => bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_read_long() {
        assert_eq!(parse_plan(&[]), Ok(Plan::Read { short: false }));
    }

    #[test]
    fn test_short_flag_sets_read_short() {
        assert_eq!(parse_plan(&["-s"]), Ok(Plan::Read { short: true }));
        // C's `case 's': sflag = 1` is idempotent under repetition.
        assert_eq!(parse_plan(&["-s", "-s"]), Ok(Plan::Read { short: true }));
    }

    #[test]
    fn test_operand_after_flag_is_write() {
        assert_eq!(
            parse_plan(&["-s", "minix.example.com"]),
            Ok(Plan::Write {
                name: "minix.example.com"
            }),
        );
        assert_eq!(parse_plan(&["myhost"]), Ok(Plan::Write { name: "myhost" }));
    }

    #[test]
    fn test_dash_dash_ends_options() {
        // A `--` barrier makes a following `-s` an operand, exactly as
        // NetBSD getopt handles it.
        assert_eq!(parse_plan(&["--", "-s"]), Ok(Plan::Write { name: "-s" }));
        assert_eq!(parse_plan(&["--"]), Ok(Plan::Read { short: false }));
    }

    #[test]
    fn test_two_operands_is_usage() {
        assert_eq!(
            parse_plan(&["host1", "host2"]).unwrap_err(),
            ShellError::InvalidSyntax,
        );
    }

    #[test]
    fn test_unknown_flag_is_usage() {
        assert_eq!(parse_plan(&["-x"]).unwrap_err(), ShellError::InvalidSyntax,);
        // Clustered options outside `-s` refuse (C's `case '?'` fires
        // on the `x` byte).
        assert_eq!(parse_plan(&["-sx"]).unwrap_err(), ShellError::InvalidSyntax,);
    }

    #[test]
    fn test_strip_domain_cuts_at_first_dot() {
        assert_eq!(strip_domain(b"minix.example.com"), &b"minix"[..]);
    }

    #[test]
    fn test_strip_domain_without_dot_is_identity() {
        assert_eq!(strip_domain(b"minix"), &b"minix"[..]);
        assert_eq!(strip_domain(b""), &b""[..]);
    }

    #[test]
    fn test_strip_domain_uses_first_dot_only() {
        assert_eq!(strip_domain(b"a.b.c"), &b"a"[..]);
    }

    #[test]
    fn test_strip_domain_leading_dot_yields_empty() {
        // C's `strchr` matches at index 0 and `*p = '\0'` truncates the
        // whole buffer, so `printf("%s\n")` prints an empty line. Pin
        // the shape here so future rewrites keep the answer instead of
        // inventing a "reject leading dot" behavior.
        assert_eq!(strip_domain(b".minix"), &b""[..]);
    }
}
