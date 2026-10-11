//! `env` — run in a modified environment, the pure argument layer.
//!
//! Ground truth: `minix3/usr.bin/env/env.c` (99 lines, BSD). The program's
//! observable behaviour splits cleanly: `main`'s `getopt(argc, argv, "-i")`
//! (`env.c:59-73`) decides whether the environment starts clean, the loop
//! `for (argv += optind; *argv && strchr(*argv, '='); ++argv) putenv(*argv)`
//! (`env.c:75-76`) merges the leading `NAME=VALUE` operands into that
//! environment with `putenv` semantics, and the tail decides between execing a
//! command (`env.c:78-85`) or printing the whole environment (`env.c:87-88`).
//! The first two stages are pure and owned here; the syscall legs (reading the
//! real environment, `execvp`) live in the thin bin `src/bin/env.rs`.
//!
//! [`parse_flags`] reproduces the leading `-i` option scan. Because the
//! option string starts with `-`, BSD `getopt` does not permute: it stops at
//! the first non-option argument, so `env FOO=bar -i` treats `-i` as the
//! command's tail, not as a flag. An option-looking argument (`-x`) that is
//! not `i` is a usage error, mirroring `default: usage()`.
//!
//! [`command_start`] marks where the leading `NAME=VALUE` operands run out
//! (C's `strchr(*argv, '=')`), and [`build_env`] renders `putenv`'s
//! replace-in-place / append semantics: a variable keeps the position of its
//! first appearance but takes the value of the last assignment to it, and a
//! brand-new key is appended in assignment order.

use alloc::vec::Vec;

/// Parse the leading `-i` option cluster. Returns `(clean, first_operand)`
/// where `clean` is set by `-i` (or repeated `-i`), and `first_operand` is the
/// index at which operands begin — the scan stops at the first non-option
/// argument (including a bare `-`), exactly as BSD `getopt` with an option
/// string that begins with `-`. A `-` followed by any character other than `i`
/// is reported as `None` so the bin can print usage and exit 1.
pub fn parse_flags(args: &[&str]) -> Option<(bool, usize)> {
    let mut clean = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        // An option argument: starts with '-', is longer than the bare '-',
        // and every body character is a listed option (`i`).
        if a.len() >= 2 && a.starts_with('-') {
            for &c in a.as_bytes()[1..].iter() {
                if c == b'i' {
                    clean = true;
                } else {
                    return None; // getopt returns '?' → usage
                }
            }
            i += 1;
        } else {
            // Non-option (or bare "-"): option processing stops here.
            break;
        }
    }
    Some((clean, i))
}

/// Index at which the command would begin: the count of leading operands that
/// contain `=`, matching C's `strchr(*argv, '=')` guard in the `putenv` loop
/// (`env.c:75`). The operands before this index are assignments; from it
/// onward is the command (empty when absent, i.e. print the environment).
pub fn command_start(operands: &[&str]) -> usize {
    let mut k = 0;
    while k < operands.len() && operands[k].contains('=') {
        k += 1;
    }
    k
}

/// The variable name of an environment entry: everything before the first
/// `=`. A byte string with no `=` yields the whole slice (a malformed entry;
/// `putenv` of such removes the variable, but `env` never feeds one because
/// the `putenv` loop is gated on `strchr(arg, '=')`).
pub fn entry_key(entry: &[u8]) -> &[u8] {
    match entry.iter().position(|&b| b == b'=') {
        Some(i) => &entry[..i],
        None => entry,
    }
}

/// Render `putenv` over `base` for each assignment (in order), returning the
/// resulting environment. For a `NAME=VALUE` assignment whose name already
/// appears in the environment, the existing entry is replaced in place (same
/// position, new value); otherwise the assignment is appended. Repeated
/// assignments to a new key collapse to one entry at its first position taking
/// the last value — which is exactly C's sequential `putenv` calls.
///
/// Assignments are `&str` (they come from `argv`); `base` entries are byte
/// slices (the raw `environ`). A name comparison is byte-exact and stops at
/// the first `=`, so `A=1` and `AB=2` never collide.
pub fn build_env(base: &[&[u8]], assignments: &[&str]) -> Vec<Vec<u8>> {
    let mut result: Vec<Vec<u8>> = base.iter().map(|e| e.to_vec()).collect();
    for a in assignments {
        let key = entry_key(a.as_bytes());
        if let Some(pos) = result.iter().position(|e| entry_key(e) == key) {
            result[pos] = a.as_bytes().to_vec();
        } else {
            result.push(a.as_bytes().to_vec());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(env: &[Vec<u8>]) -> Vec<String> {
        env.iter()
            .map(|e| String::from_utf8(e.clone()).unwrap())
            .collect()
    }

    #[test]
    fn flag_i_sets_clean() {
        assert_eq!(parse_flags(&["-i", "A=1"]), Some((true, 1)));
        assert_eq!(parse_flags(&["-i"]), Some((true, 1)));
        assert_eq!(parse_flags(&[]), Some((false, 0)));
    }

    #[test]
    fn repeated_i_is_idempotent() {
        assert_eq!(parse_flags(&["-i", "-i", "x"]), Some((true, 2)));
    }

    #[test]
    fn cluster_of_i_still_valid() {
        // "-ii" is two `i` options; getopt accepts both, clean set.
        assert_eq!(parse_flags(&["-ii", "x"]), Some((true, 1)));
    }

    #[test]
    fn no_leading_option_is_not_clean() {
        assert_eq!(parse_flags(&["A=1", "-i"]), Some((false, 0)));
        // Option processing stops at the first non-option; "-i" is an operand.
    }

    #[test]
    fn unknown_option_is_usage_error() {
        assert_eq!(parse_flags(&["-x"]), None);
        assert_eq!(parse_flags(&["-i", "-x"]), None); // still in option region
    }

    #[test]
    fn bare_dash_is_nonoption_stop() {
        // A lone "-" is a non-option argument under the leading-"-" convention.
        assert_eq!(parse_flags(&["-", "A=1"]), Some((false, 0)));
    }

    #[test]
    fn command_start_counts_leading_assignments() {
        assert_eq!(command_start(&["A=1", "B=2", "cmd", "-x"]), 2);
        assert_eq!(command_start(&["cmd", "A=1"]), 0); // stops at first non-assignment
        assert_eq!(command_start(&["A=1", "B=2"]), 2); // all assignments, no command
        assert_eq!(command_start(&[]), 0);
    }

    #[test]
    fn entry_key_stops_at_first_equals() {
        assert_eq!(entry_key(b"AB=1"), &b"AB"[..]);
        assert_eq!(entry_key(b"=v"), &b""[..]);
        assert_eq!(entry_key(b"novalue"), &b"novalue"[..]);
    }

    #[test]
    fn build_env_replaces_in_place() {
        let base: Vec<&[u8]> = vec![b"A=1".as_slice(), b"B=2".as_slice()];
        assert_eq!(strs(&build_env(&base, &["A=9"])), ["A=9", "B=2"]);
    }

    #[test]
    fn build_env_appends_new_key_at_end() {
        let base: Vec<&[u8]> = vec![b"A=1".as_slice()];
        assert_eq!(strs(&build_env(&base, &["C=3"])), ["A=1", "C=3"]);
    }

    #[test]
    fn build_env_keeps_position_and_takes_last_value() {
        // "C" is new (appended), "A" replaced in place at position 0.
        let base: Vec<&[u8]> = vec![b"A=1".as_slice(), b"B=2".as_slice()];
        assert_eq!(
            strs(&build_env(&base, &["C=3", "A=9"])),
            ["A=9", "B=2", "C=3"]
        );
    }

    #[test]
    fn build_env_collapses_repeated_new_key() {
        // putenv: first X appends, second replaces that entry → one X=2.
        assert_eq!(strs(&build_env(&[], &["X=1", "X=2"])), ["X=2"]);
    }

    #[test]
    fn build_env_empty_value_and_clean_base() {
        let base: Vec<&[u8]> = vec![b"A=1".as_slice()];
        assert_eq!(strs(&build_env(&base, &["A="])), ["A="]);
        // -i gives an empty base; assignments alone form the environment.
        assert_eq!(strs(&build_env(&[], &["A=1", "B=2"])), ["A=1", "B=2"]);
    }

    #[test]
    fn build_env_prefix_keys_distinct() {
        let base: Vec<&[u8]> = vec![b"AB=1".as_slice()];
        // "A=2" must not clobber "AB=1" — key stops at '='.
        assert_eq!(strs(&build_env(&base, &["A=2"])), ["AB=1", "A=2"]);
    }
}
