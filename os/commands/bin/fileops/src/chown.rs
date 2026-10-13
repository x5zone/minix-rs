//! `chown` / `chgrp` — the owner-spec argument layer, the pure deciding half.
//!
//! Ground truth: `minix3/sbin/chown/chown.c` (a single source shared by both
//! names; `ischown = myname[2] == 'o'` at line 94 selects `chown` vs `chgrp`).
//! This module owns C's argument bookkeeping — the `owner[:group]` /
//! `owner.group` split (`chown.c:163-176`, the `.` arm gated by `SUPPORT_DOT`
//! which `sbin/chown/Makefile:5` defines), the empty-part early returns
//! (`a_uid`/`a_gid` `*s == '\0'` at `:264/:273`), and the numeric `id()`
//! parser (`:281-296`: optional `#`, `strtoul` base 10, reject `errno`
//! overflow and any trailing byte). The syscall leg (`lstat`/`chown`) lives in
//! the thin bin, exactly as `rm`'s removal syscalls live apart from its
//! `RmFlags`/triage layer.
//!
//! Scope of what can be *decided* here: a purely numeric owner or group
//! (`100`, `#100`, `100:200`, `:200`, `100.200`) is resolved without any
//! name service. A **named** user or group (`root`, `wheel`) needs the
//! `getpwnam`/`getgrnam` password DB (`chown.c:252`, `uid_from_user`), which
//! this tree does not yet wrap — that leg is a registered residual (`E5`/
//! the `chown` line in `18-stage-commands`), so this layer reports it as
//! [`ChownError::NamedUnsupported`] rather than inventing a resolution. This
//! mirrors how `chmod` refuses `-R` up front and `find` refuses its missing
//! legs by name: the decision half stays honest about what the caller must
//! not pretend to support. The `--reference=rfile` form (`chown.c:181-189`,
//! which reads uid/gid off an existing file rather than a spec) likewise needs
//! the syscall leg and is the thin bin's concern, not this parser's.

use alloc::format;
use alloc::string::String;

/// The owner/group a spec resolves to, mirroring C's file-scope `uid`/`gid`
/// (`chown.c:70-71`), both initialised to `(uid_t)-1`/`(gid_t)-1` (`:158-159`)
/// meaning "leave unchanged". `None` is that `-1`; `Some(n)` is a concrete id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerIds {
    /// The user half (`a_uid`), or `None` to keep the current owner.
    pub uid: Option<u32>,
    /// The group half (`a_gid`), or `None` to keep the current group.
    pub gid: Option<u32>,
}

impl OwnerIds {
    /// Both halves unset — C's `uid = gid = -1` before any `a_*` runs.
    const NONE: OwnerIds = OwnerIds { uid: None, gid: None };
}

/// Why a spec could not be decided to a numeric pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChownError {
    /// A malformed numeric token: empty after `#`, a non-digit, trailing
    /// bytes, or overflow — C's `id()` `*ep != '\0'` / `errno` failures
    /// (`chown.c:288-293`), which `err`/`errx` turn into exit.
    Invalid(String),
    /// A user or group named rather than numbered, which needs the password
    /// DB this tree has not wired. Carries the offending name so the bin can
    /// echo it (`chown.c:293` `"%s: invalid %s name"`).
    NamedUnsupported(String),
}

/// Parse a `chown` owner spec into the (user, group) pair.
///
/// `ischown` selects the C branch: `true` for `chown` (a `[user][:group]`
/// spec, `chown.c:161-176`), `false` for `chgrp` (the whole argument is a
/// group, `chown.c:176-177`). When `false`, any `:`/`.` split is skipped and
/// the whole token goes through the group parser.
pub fn parse_owner_spec(spec: &str, ischown: bool) -> Result<OwnerIds, ChownError> {
    if !ischown {
        // chgrp: a_gid(*argv) alone (`chown.c:177`).
        return Ok(OwnerIds { uid: None, gid: parse_group(spec)? });
    }

    let mut ids = OwnerIds::NONE;

    // chown.c:163-176 — a `:` splits user and group; under SUPPORT_DOT a `.`
    // splits them too, but only when the whole token before the last `.` is
    // not a known user. Without a name service every `.`-bearing token that
    // is not a bare number falls to the `.` split, matching C's outcome for
    // numeric users (`100.200`) and deferring named users to `NamedUnsupported`.
    let (user_part, group_part): (&str, Option<&str>) = if let Some(idx) = spec.find(':') {
        (&spec[..idx], Some(&spec[idx + 1..]))
    } else if let Some(idx) = spec.rfind('.') {
        (&spec[..idx], Some(&spec[idx + 1..]))
    } else {
        (spec, None)
    };

    // a_uid then a_gid, preserving C's order: the `:`/`.` arm runs a_gid
    // first (`:163-165`), a_uid after (`:172`); the two are independent here
    // since we do not share C's mutable `uid`/`gid` globals.
    ids.uid = parse_user(user_part)?;
    if let Some(group) = group_part {
        ids.gid = parse_group(group)?;
    }

    // C never errors when a spec resolves to no change, but a bare separator
    // (`:` / `.` with both sides empty) leaves uid and gid both unset, which
    // `main` treats as "change nothing"; the bin reports it, the parser does
    // not reject it.
    Ok(ids)
}

/// `a_uid` (`chown.c:270-277`): empty means "leave owner"; else `#`-prefixed
/// or an unresolved user name falls through to the numeric `id()`. With no
/// name service, a non-numeric token is an unresolved name.
fn parse_user(s: &str) -> Result<Option<u32>, ChownError> {
    if s.is_empty() {
        return Ok(None); // "Argument was \"[:.]gid\"." (`chown.c:272`)
    }
    match parse_num(s, "user") {
        Ok(n) => Ok(Some(n)),
        // A non-numeric token is one `uid_from_user` could not resolve and
        // `id()` also rejected — but the real reason it is unusable here is
        // the missing password DB, not malformed digits, so surface it as the
        // named residual unless it is outright malformed.
        Err(ChownError::Invalid(_)) if looks_like_name(s) => {
            Err(ChownError::NamedUnsupported(String::from(s)))
        }
        Err(e) => Err(e),
    }
}

/// `a_gid` (`chown.c:250-262`): empty means "leave group"; a `#` prefix skips
/// `getgrnam` and forces numeric; otherwise a group name is looked up (not
/// wired) and non-numeric tokens fall to `id()`.
fn parse_group(s: &str) -> Result<Option<u32>, ChownError> {
    if s.is_empty() {
        return Ok(None); // "Argument was \"uid[.]:\"." (`chown.c:253`)
    }
    match parse_num(s, "group") {
        Ok(n) => Ok(Some(n)),
        Err(ChownError::Invalid(_)) if looks_like_name(s) => {
            Err(ChownError::NamedUnsupported(String::from(s)))
        }
        Err(e) => Err(e),
    }
}

/// `id()` (`chown.c:281-296`): strip a leading `#`, `strtoul` base 10, then
/// reject `errno` (overflow) and any unconsumed trailing byte.
fn parse_num(name: &str, kind: &str) -> Result<u32, ChownError> {
    let digits = name.strip_prefix('#').unwrap_or(name);
    if digits.is_empty() {
        return Err(ChownError::Invalid(String::from(kind)));
    }
    let mut acc: u64 = 0;
    for byte in digits.bytes() {
        if !byte.is_ascii_digit() {
            // A non-digit here is a trailing byte (`*ep != '\0'`) in a token
            // that started with digits, or the first byte of a name.
            return Err(ChownError::Invalid(format!("{name}: invalid {kind} name")));
        }
        acc = acc
            .saturating_mul(10)
            .saturating_add((byte - b'0') as u64);
        if acc > u32::MAX as u64 {
            return Err(ChownError::Invalid(format!("{name}: invalid {kind} name")));
        }
    }
    Ok(acc as u32)
}

/// Whether a rejected token reads as a name (starts with an ASCII letter)
/// rather than as malformed digits — the distinction between the `chown`
/// named-user residual and a genuine syntax error worth surfacing verbatim.
fn looks_like_name(s: &str) -> bool {
    s.strip_prefix('#')
        .unwrap_or(s)
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_user_only() {
        assert_eq!(parse_owner_spec("100", true).unwrap(), OwnerIds { uid: Some(100), gid: None });
    }

    #[test]
    fn hash_forces_numeric() {
        assert_eq!(parse_owner_spec("#0", true).unwrap(), OwnerIds { uid: Some(0), gid: None });
    }

    #[test]
    fn colon_separates_user_and_group() {
        assert_eq!(
            parse_owner_spec("100:200", true).unwrap(),
            OwnerIds { uid: Some(100), gid: Some(200) }
        );
    }

    #[test]
    fn dot_separator_supported() {
        // SUPPORT_DOT is defined (`sbin/chown/Makefile:5`).
        assert_eq!(
            parse_owner_spec("100.200", true).unwrap(),
            OwnerIds { uid: Some(100), gid: Some(200) }
        );
    }

    #[test]
    fn leading_colon_is_group_only() {
        // ":200" — a_uid("") leaves owner, a_gid("200") sets group.
        assert_eq!(parse_owner_spec(":200", true).unwrap(), OwnerIds { uid: None, gid: Some(200) });
    }

    #[test]
    fn trailing_colon_is_user_only() {
        // "100:" — a_uid("100") sets owner, a_gid("") leaves group.
        assert_eq!(parse_owner_spec("100:", true).unwrap(), OwnerIds { uid: Some(100), gid: None });
    }

    #[test]
    fn chgrp_reads_the_whole_token_as_group() {
        // ischown == false: no split, the whole spec goes to the group parser.
        assert_eq!(parse_owner_spec("200", false).unwrap(), OwnerIds { uid: None, gid: Some(200) });
    }

    #[test]
    fn empty_spec_changes_nothing() {
        assert_eq!(parse_owner_spec("", true).unwrap(), OwnerIds::NONE);
        assert_eq!(parse_owner_spec(":", true).unwrap(), OwnerIds::NONE);
    }

    #[test]
    fn trailing_junk_is_invalid() {
        // "10x" — id() would hit `*ep != '\0'`; starts with a digit so it is
        // malformed, not a name.
        assert!(matches!(parse_owner_spec("10x", true), Err(ChownError::Invalid(_))));
    }

    #[test]
    fn overflow_is_invalid() {
        assert!(matches!(parse_owner_spec("99999999999", true), Err(ChownError::Invalid(_))));
    }

    #[test]
    fn bare_hash_is_invalid() {
        assert!(matches!(parse_owner_spec("#", true), Err(ChownError::Invalid(_))));
    }

    #[test]
    fn named_user_is_the_password_db_residual() {
        // `root` is not numeric and not malformed — it needs getpwnam, which
        // this tree has not wired. Report it as the named residual, not an
        // invented id.
        assert_eq!(
            parse_owner_spec("root", true),
            Err(ChownError::NamedUnsupported(String::from("root")))
        );
        assert_eq!(
            parse_owner_spec("100:wheel", true),
            Err(ChownError::NamedUnsupported(String::from("wheel")))
        );
    }

    #[test]
    fn numeric_group_with_named_user_still_refuses_named_half() {
        // Mixed "user:100" — the group parses, but the user half is a name.
        assert_eq!(
            parse_owner_spec("alice:100", true),
            Err(ChownError::NamedUnsupported(String::from("alice")))
        );
    }
}
