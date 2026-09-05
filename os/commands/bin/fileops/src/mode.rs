//! Permission bit arithmetic behind `chmod`.
//!
//! Ground truth: `minix3/bin/chmod/chmod.c` (`setmode` at line 166 turns
//! the mode text into an edit script, `getmode` at line 216 applies it to
//! the file's current bits). Two spellings:
//!
//! - Octal (`755`): the twelve permission bits written out directly.
//! - Symbolic (`u+x,go-w`, `a=rw`, `+x`): who (`u` user, `g` group, `o`
//!   others, `a` all), how (`+` add, `-` remove, `=` set exactly), what
//!   (`rwxXst` plus `u`/`g`/`o` copying one class from another).
//!
//! Special bits (set user identifier, set group identifier, sticky) live
//! above the nine `rwx` bits and are addressed by the same grammar.

use crate::FileOpError;

/// User, group, and others read/write/execute bits plus the three special
/// bits, in classic octal positions.
pub const SET_USER_ID: u16 = 0o4000;
/// Set group identifier on execution.
pub const SET_GROUP_ID: u16 = 0o2000;
/// Sticky bit (restricted deletion).
pub const STICKY: u16 = 0o1000;

/// Apply `spec` (octal or symbolic) to `base` permission bits.
///
/// An all digit spec is octal and replaces the twelve low bits outright
/// (higher bits of `base`, such as the file type, are preserved). Anything
/// else is a comma separated list of symbolic clauses applied left to
/// right. Empty clauses (from `u+x,,go-w`) are rejected, matching the C
/// library refusing the whole spec.
pub fn apply_mode(base: u16, spec: &str) -> Result<u16, FileOpError> {
    if spec.is_empty() {
        return Err(FileOpError::InvalidArgument);
    }
    if spec.bytes().all(|b| b.is_ascii_digit()) {
        return apply_octal(base, spec);
    }
    let mut mode = base;
    for clause in spec.split(',') {
        mode = apply_clause(mode, clause)?;
    }
    Ok(mode)
}

fn apply_octal(base: u16, spec: &str) -> Result<u16, FileOpError> {
    // Digits 8 and 9 are not octal; more than four digits overflows the
    // twelve permission bits.
    if spec.len() > 4 || spec.bytes().any(|b| !(b'0'..=b'7').contains(&b)) {
        return Err(FileOpError::InvalidArgument);
    }
    let mut value: u16 = 0;
    for byte in spec.bytes() {
        value = value * 8 + (byte - b'0') as u16;
    }
    Ok((base & 0xF000) | (value & 0x0FFF))
}

/// Who mask: which of the three classes a clause addresses.
fn who_mask(who: &str) -> Result<u16, FileOpError> {
    if who.is_empty() {
        // No who means "all, honoring the creation mask" — the creation
        // mask lives with the executor, so the library reports all bits
        // and documents the handoff.
        return Ok(0o7777);
    }
    let mut mask = 0;
    for byte in who.bytes() {
        match byte {
            b'u' => mask |= 0o4700,
            b'g' => mask |= 0o2070,
            b'o' => mask |= 0o1007,
            b'a' => mask |= 0o7777,
            _ => return Err(FileOpError::InvalidArgument),
        }
    }
    Ok(mask)
}

/// Permission bits named by one `what` character, resolved against the
/// addressed classes (`X` means execute only when the target already
/// executes somewhere or is a directory — directory knowledge lives with
/// the executor, so `X` behaves as `x` here and the difference is
/// documented).
fn what_bits(what: u8, who: u16, base: u16) -> Result<u16, FileOpError> {
    let bits;
    match what {
        b'r' => bits = 0o444,
        b'w' => bits = 0o222,
        b'x' | b'X' => bits = 0o111,
        b's' => bits = SET_USER_ID | SET_GROUP_ID,
        b't' => bits = STICKY,
        b'u' => bits = ((base >> 6) & 0o7) * 0o111,
        b'g' => bits = ((base >> 3) & 0o7) * 0o111,
        b'o' => bits = (base & 0o7) * 0o111,
        _ => return Err(FileOpError::InvalidArgument),
    }
    Ok(bits & who)
}

fn apply_clause(mut mode: u16, clause: &str) -> Result<u16, FileOpError> {
    let bytes = clause.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() && matches!(bytes[pos], b'u' | b'g' | b'o' | b'a') {
        pos += 1;
    }
    let who = who_mask(&clause[..pos])?;
    if pos >= bytes.len() {
        return Err(FileOpError::InvalidArgument);
    }
    // One clause may chain several `how what` actions (`g+w-x`),
    // applied strictly left to right; each `=` clears the addressed
    // classes first (matching setmode chaining semantics).
    while pos < bytes.len() {
        let how = bytes[pos];
        if !matches!(how, b'+' | b'-' | b'=') {
            return Err(FileOpError::InvalidArgument);
        }
        pos += 1;
        let mut bits = 0;
        let mut any = false;
        while pos < bytes.len() && !matches!(bytes[pos], b'+' | b'-' | b'=') {
            bits |= what_bits(bytes[pos], who, mode)?;
            any = true;
            pos += 1;
        }
        // `=` with an empty what clears the addressed classes (`u=`).
        if !any && how != b'=' {
            return Err(FileOpError::InvalidArgument);
        }
        match how {
            b'+' => mode |= bits,
            b'-' => mode &= !bits,
            b'=' => {
                mode &= !who;
                mode |= bits;
            }
            _ => return Err(FileOpError::InvalidArgument),
        }
    }
    Ok(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_octal_replaces_low_bits() {
        assert_eq!(apply_mode(0o644, "755").unwrap(), 0o755);
        // File type bits survive.
        assert_eq!(apply_mode(0o100644, "755").unwrap(), 0o100755);
    }

    #[test]
    fn test_bad_octal_rejected() {
        assert_eq!(apply_mode(0, "888"), Err(FileOpError::InvalidArgument));
        assert_eq!(apply_mode(0, "12345"), Err(FileOpError::InvalidArgument));
        assert_eq!(apply_mode(0, ""), Err(FileOpError::InvalidArgument));
    }

    #[test]
    fn test_add_and_remove() {
        assert_eq!(apply_mode(0o644, "u+x").unwrap(), 0o744);
        assert_eq!(apply_mode(0o755, "go-w").unwrap(), 0o755);
        assert_eq!(apply_mode(0o777, "go-w").unwrap(), 0o755);
    }

    #[test]
    fn test_set_exactly() {
        assert_eq!(apply_mode(0o777, "u=rw").unwrap(), 0o677);
        assert_eq!(apply_mode(0o777, "a=rx").unwrap(), 0o555);
        assert_eq!(apply_mode(0o777, "o=").unwrap(), 0o770);
    }

    #[test]
    fn test_special_bits() {
        assert_eq!(apply_mode(0o755, "u+s").unwrap(), 0o4755);
        assert_eq!(apply_mode(0o777, "+t").unwrap(), 0o1777);
    }

    #[test]
    fn test_copy_between_classes() {
        // `g=u` copies the user triplet onto the group triplet.
        assert_eq!(apply_mode(0o700, "g=u").unwrap(), 0o770);
    }

    #[test]
    fn test_chained_clauses() {
        assert_eq!(apply_mode(0o000, "u+rw,go+r").unwrap(), 0o644);
    }

    #[test]
    fn test_bad_clause_rejected() {
        assert_eq!(apply_mode(0o644, "z+x"), Err(FileOpError::InvalidArgument));
        assert_eq!(apply_mode(0o644, "u"), Err(FileOpError::InvalidArgument));
        assert_eq!(apply_mode(0o644, "u+"), Err(FileOpError::InvalidArgument));
    }
}
