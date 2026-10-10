//! `printenv` — read environment variables.
//!
//! Ground truth: `minix3/usr.bin/printenv/printenv.c`. With no operand it
//! prints every `KEY=VALUE` environment string; with exactly one name it prints
//! that variable's value (or exits 1 if unset); more than one name, or a name
//! containing `=`, is a usage/argument error. The environment is handed in as a
//! slice of `KEY=VALUE` byte strings (the C `environ` walk), so the lookup is
//! pure and host-testable; reading the real environment and the output bytes
//! stay with the caller.
//!
//! `lookup` mirrors the C matcher (`printenv.c:78-87`): a candidate matches
//! only if it begins with the name *and* the byte after the name is `=` or the
//! end of the string — so a query `FO` does not falsely hit `FOO=1`. The
//! returned value is the bytes after `=`; a bare `NAME` entry (no `=`) yields an
//! empty value, matching C printing `*cp` when the terminator stands where the
//! `=` would be.

/// True when `name` contains `=`, which `printenv` rejects as an argument error
/// (`printenv.c:73-75`).
pub fn name_is_invalid(name: &[u8]) -> bool {
    name.contains(&b'=')
}

/// Value of the environment variable `name` among `env` entries, or `None` if
/// unset. The first matching entry wins, as C exits on its first hit.
pub fn lookup<'a>(env: &[&'a [u8]], name: &[u8]) -> Option<&'a [u8]> {
    let len = name.len();
    for &entry in env {
        if entry.len() < len || &entry[..len] != name {
            continue;
        }
        let rest = &entry[len..];
        if rest.is_empty() {
            return Some(&[][..]); // entry is exactly the name, no value
        }
        if rest[0] == b'=' {
            return Some(&rest[1..]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV: &[&[u8]] = &[b"PATH=/bin:/usr/bin", b"HOME=/root", b"EMPTY=", b"BARE"];

    #[test]
    fn finds_exact_variable() {
        assert_eq!(lookup(ENV, b"PATH"), Some(&b"/bin:/usr/bin"[..]));
        assert_eq!(lookup(ENV, b"HOME"), Some(&b"/root"[..]));
    }

    #[test]
    fn unset_returns_none() {
        assert_eq!(lookup(ENV, b"MISSING"), None);
    }

    #[test]
    fn name_prefix_does_not_match_longer_name() {
        // "PAT" is a prefix of "PATH" but the byte after it is 'H', not '=' → no
        // false hit (the C memcmp+boundary guard).
        assert_eq!(lookup(ENV, b"PAT"), None);
    }

    #[test]
    fn empty_and_bare_values() {
        assert_eq!(lookup(ENV, b"EMPTY"), Some(&b""[..]));
        assert_eq!(lookup(ENV, b"BARE"), Some(&b""[..]));
    }

    #[test]
    fn invalid_name_has_equals() {
        assert!(name_is_invalid(b"A=B"));
        assert!(!name_is_invalid(b"AB"));
    }

    #[test]
    fn first_match_wins() {
        let env: &[&[u8]] = &[b"X=first", b"X=second"];
        assert_eq!(lookup(env, b"X"), Some(&b"first"[..]));
    }
}
