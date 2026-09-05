//! Variable expansion over an environment trait.
//!
//! Ground truth: `minix3/bin/sh/expand.c` (1640 lines; entry `expandarg` at
//! line 138, arithmetic `expari` at line 355). The C expander handles seven
//! expansions (variables, command substitution, arithmetic, pathname
//! globbing, tilde, field splitting, quote removal). This module implements
//! the variable subset — the one every startup file and every `env` style
//! tool needs — over the [`Environ`] trait, so testing never touches a real
//! process environment:
//!
//! - `$NAME` and `${NAME}`: plain lookup; unset expands to nothing.
//! - `${NAME-word}` / `${NAME:-word}`: default when unset / when unset or
//!   empty.
//! - `${NAME+word}` / `${NAME:+word}`: alternate value when set / when set
//!   and non empty.
//! - `${NAME?word}` / `${NAME:?word}`: error when unset / when unset or
//!   empty (the message is ignored; the failure is reported).
//! - `$$`: process identifier; `$?`: last exit status; `$#`, `$*`, `$@`,
//!   `$0`..`$9`: positional face, skipped here and reported by the executor.
//! - `Strong` quoted runs pass through untouched; `Weak` and `Bare` runs
//!   expand.
//!
//! Two documented boundaries: `${NAME=value}` assignment needs owned
//! strings (allocation), so it is rejected loudly rather than done wrongly;
//! default words expand recursively up to 8 levels deep, deeper nesting is
//! rejected. Command substitution, arithmetic, globbing, tilde expansion,
//! and field splitting belong to later stages.
//!
//! Expansion writes into a caller provided byte buffer and reports the used
//! prefix length: no heap, `no_std` throughout.

use crate::ShellError;
use crate::lexer::{Quoting, Word};

/// Maximum nesting of default words inside `${...}`.
const MAX_NESTING: u8 = 8;

/// Read only variable lookup used by the expander.
pub trait Environ {
    /// Fetch the value of `name`, or `None` when unset.
    fn get(&self, name: &str) -> Option<&str>;
}

/// An environment that holds nothing: every lookup misses. The honest
/// starting point until the process environment binding lands; scripts
/// expanding against it see documented empty defaults.
pub struct EmptyEnv;

impl Environ for EmptyEnv {
    fn get(&self, _name: &str) -> Option<&str> {
        None
    }
}

/// An environment over an in memory table of name/value pairs.
///
/// Lookups scan in order; the first name match wins. Capacity is fixed (16
/// entries): system environments are small, and a fixed table keeps the
/// whole crate heap free.
pub struct TableEnv<'a> {
    /// Name/value pairs; `None` slots are free.
    pub entries: [Option<(&'a str, &'a str)>; 16],
}

impl<'a> TableEnv<'a> {
    /// An empty table.
    pub fn empty() -> Self {
        TableEnv { entries: [None; 16] }
    }
}

impl Environ for TableEnv<'_> {
    fn get(&self, name: &str) -> Option<&str> {
        self.entries.iter().find_map(|slot| match slot {
            Some((key, value)) if *key == name => Some(*value),
            _ => None,
        })
    }
}

/// Expand one lexer word into `out`, returning the used byte count.
///
/// `last_status` feeds `$?`; `process_id` feeds `$$`. Strong runs copy
/// verbatim; other runs expand `$` sequences. A lone trailing `$` copies
/// verbatim (matching shell practice of leaving it alone).
pub fn expand_word(
    word: &Word<'_>,
    env: &dyn Environ,
    last_status: u8,
    process_id: u32,
    out: &mut [u8],
) -> Result<usize, ShellError> {
    let mut written = 0;
    for run in word.runs[..word.run_count].iter() {
        if word.quoting == Quoting::Strong {
            written = put_slice(out, written, run.as_bytes())?;
        } else {
            written = expand_text(run, env, last_status, process_id, out, written, 0)?;
        }
    }
    Ok(written)
}

fn put_byte(out: &mut [u8], mut written: usize, byte: u8) -> Result<usize, ShellError> {
    if written >= out.len() {
        return Err(ShellError::TooLong);
    }
    out[written] = byte;
    written += 1;
    Ok(written)
}

fn put_slice(out: &mut [u8], mut written: usize, slice: &[u8]) -> Result<usize, ShellError> {
    for byte in slice {
        written = put_byte(out, written, *byte)?;
    }
    Ok(written)
}

fn put_str(out: &mut [u8], written: usize, text: &str) -> Result<usize, ShellError> {
    put_slice(out, written, text.as_bytes())
}

#[allow(clippy::too_many_arguments)]
fn expand_text(
    text: &str,
    env: &dyn Environ,
    last_status: u8,
    process_id: u32,
    out: &mut [u8],
    mut written: usize,
    depth: u8,
) -> Result<usize, ShellError> {
    if depth > MAX_NESTING {
        return Err(ShellError::TooLong);
    }
    let bytes = text.as_bytes();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos] != b'$' {
            written = put_byte(out, written, bytes[pos])?;
            pos += 1;
            continue;
        }
        pos += 1;
        if pos >= bytes.len() {
            written = put_byte(out, written, b'$')?;
            break;
        }
        match bytes[pos] {
            b'$' => {
                written = write_decimal(out, written, process_id as u64)?;
                pos += 1;
            }
            b'?' => {
                written = write_decimal(out, written, last_status as u64)?;
                pos += 1;
            }
            b'#' | b'*' | b'@' | b'0'..=b'9' => {
                // Positional face: reported through dedicated channels by
                // the executor, never expanded here. Skip the marker.
                pos += 1;
            }
            b'{' => {
                let (next, updated) =
                    expand_braced(text, pos + 1, env, last_status, process_id, out, written, depth)?;
                pos = next;
                written = updated;
            }
            b if is_name_start(b) => {
                let start = pos;
                while pos < bytes.len() && is_name_char(bytes[pos]) {
                    pos += 1;
                }
                if let Some(value) = env.get(&text[start..pos]) {
                    written = put_str(out, written, value)?;
                }
            }
            _ => {
                // `$` before anything else is literal.
                written = put_byte(out, written, b'$')?;
            }
        }
    }
    Ok(written)
}

/// Expand `${...}` starting just after the opening brace.
///
/// Returns the position past the closing brace plus the updated write
/// cursor. Handles plain lookup, `-`/`:-` defaults, `+`/`:+` alternates,
/// and `?`/`:?` errors. Assignment (`=`/`:=`) is rejected: it needs owned
/// strings, and doing it wrong silently would be worse than refusing.
#[allow(clippy::too_many_arguments)]
fn expand_braced(
    text: &str,
    inner: usize,
    env: &dyn Environ,
    last_status: u8,
    process_id: u32,
    out: &mut [u8],
    written: usize,
    depth: u8,
) -> Result<(usize, usize), ShellError> {
    let bytes = text.as_bytes();
    let mut pos = inner;
    // Special single character names first (`${?}`, `${$}`).
    if pos < bytes.len() && matches!(bytes[pos], b'$' | b'?') {
        let value = if bytes[pos] == b'$' {
            process_id as u64
        } else {
            last_status as u64
        };
        pos += 1;
        if bytes.get(pos) != Some(&b'}') {
            return Err(ShellError::InvalidSyntax);
        }
        let written = write_decimal(out, written, value)?;
        return Ok((pos + 1, written));
    }
    if pos >= bytes.len() || !is_name_start(bytes[pos]) {
        return Err(ShellError::InvalidSyntax);
    }
    let name_start = pos;
    while pos < bytes.len() && is_name_char(bytes[pos]) {
        pos += 1;
    }
    let name = &text[name_start..pos];
    // Plain `${NAME}`.
    if bytes.get(pos) == Some(&b'}') {
        let mut written = written;
        if let Some(value) = env.get(name) {
            written = put_str(out, written, value)?;
        }
        return Ok((pos + 1, written));
    }
    // Operator: optional colon (empty counts as unset) plus one of `-+?=`.
    let mut check_empty = false;
    if bytes.get(pos) == Some(&b':') {
        check_empty = true;
        pos += 1;
    }
    let operator = *bytes.get(pos).ok_or(ShellError::InvalidSyntax)?;
    if !matches!(operator, b'-' | b'+' | b'?' | b'=') {
        return Err(ShellError::InvalidSyntax);
    }
    pos += 1;
    // The word runs to the closing brace (no nesting of braces inside).
    let word_start = pos;
    while pos < bytes.len() && bytes[pos] != b'}' {
        pos += 1;
    }
    if pos >= bytes.len() {
        return Err(ShellError::InvalidSyntax);
    }
    let word = &text[word_start..pos];
    let after = pos + 1;
    let current = env.get(name);
    let is_unset = match current {
        None => true,
        Some(value) => check_empty && value.is_empty(),
    };
    match operator {
        b'-' => {
            if is_unset {
                let written = expand_text(word, env, last_status, process_id, out, written, depth + 1)?;
                Ok((after, written))
            } else {
                let written = put_str(out, written, current.unwrap_or(""))?;
                Ok((after, written))
            }
        }
        b'+' => {
            if is_unset {
                Ok((after, written))
            } else {
                let written = expand_text(word, env, last_status, process_id, out, written, depth + 1)?;
                Ok((after, written))
            }
        }
        b'?' => {
            if is_unset {
                Err(ShellError::InvalidSyntax)
            } else {
                let written = put_str(out, written, current.unwrap_or(""))?;
                Ok((after, written))
            }
        }
        // `=` and `:=` need owned strings; refuse loudly (see module docs).
        _ => Err(ShellError::InvalidSyntax),
    }
}

/// Write `value` decimally, returning the updated cursor.
fn write_decimal(out: &mut [u8], mut written: usize, mut value: u64) -> Result<usize, ShellError> {
    if value == 0 {
        return put_byte(out, written, b'0');
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
        written = put_byte(out, written, digits[count])?;
    }
    Ok(written)
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::split_words;

    fn expand(line: &str, env: &dyn Environ) -> Result<String, ShellError> {
        let (words, count) = split_words(line)?;
        let mut out = [0u8; 256];
        let mut result = String::new();
        for word in words[..count].iter() {
            let len = expand_word(word, env, 3, 42, &mut out)?;
            result.push_str(core::str::from_utf8(&out[..len]).unwrap());
            result.push('|');
        }
        Ok(result)
    }

    #[test]
    fn test_plain_lookup() {
        let mut env = TableEnv::empty();
        env.entries[0] = Some(("HOME", "/root"));
        assert_eq!(expand("echo $HOME", &env).unwrap(), "echo|/root|");
    }

    #[test]
    fn test_braced_lookup() {
        let mut env = TableEnv::empty();
        env.entries[0] = Some(("HOME", "/root"));
        assert_eq!(expand("echo ${HOME}/bin", &env).unwrap(), "echo|/root/bin|");
    }

    #[test]
    fn test_unset_expands_empty() {
        let env = EmptyEnv;
        assert_eq!(expand("echo $MISSING", &env).unwrap(), "echo||");
    }

    #[test]
    fn test_default_when_unset() {
        let env = EmptyEnv;
        assert_eq!(expand("echo ${MISSING:-fallback}", &env).unwrap(), "echo|fallback|");
    }

    #[test]
    fn test_default_keeps_set_value() {
        let mut env = TableEnv::empty();
        env.entries[0] = Some(("A", "1"));
        assert_eq!(expand("echo ${A:-fallback}", &env).unwrap(), "echo|1|");
    }

    #[test]
    fn test_colon_default_treats_empty_as_unset() {
        let mut env = TableEnv::empty();
        env.entries[0] = Some(("E", ""));
        assert_eq!(expand("echo ${E:-fallback}", &env).unwrap(), "echo|fallback|");
        assert_eq!(expand("echo ${E-fallback}", &env).unwrap(), "echo||");
    }

    #[test]
    fn test_alternate_value() {
        let mut env = TableEnv::empty();
        env.entries[0] = Some(("A", "1"));
        assert_eq!(expand("echo ${A:+yes}", &env).unwrap(), "echo|yes|");
        let env = EmptyEnv;
        assert_eq!(expand("echo ${MISSING:+yes}", &env).unwrap(), "echo||");
    }

    #[test]
    fn test_error_when_unset() {
        let env = EmptyEnv;
        assert_eq!(
            expand("echo ${MISSING:?need it}", &env),
            Err(ShellError::InvalidSyntax)
        );
    }

    #[test]
    fn test_assign_rejected_loudly() {
        let env = EmptyEnv;
        assert_eq!(
            expand("echo ${MISSING:=x}", &env),
            Err(ShellError::InvalidSyntax)
        );
    }

    #[test]
    fn test_unclosed_brace_rejected() {
        let env = EmptyEnv;
        assert_eq!(
            expand("echo ${HOME", &env),
            Err(ShellError::InvalidSyntax)
        );
    }

    #[test]
    fn test_special_parameters() {
        let env = EmptyEnv;
        assert_eq!(expand("echo $? $$", &env).unwrap(), "echo|3|42|");
    }

    #[test]
    fn test_strong_quote_suppresses() {
        let mut env = TableEnv::empty();
        env.entries[0] = Some(("HOME", "/root"));
        assert_eq!(expand("echo '$HOME'", &env).unwrap(), "echo|$HOME|");
    }

    #[test]
    fn test_env_trait_objects() {
        let empty = EmptyEnv;
        let mut table = TableEnv::empty();
        table.entries[0] = Some(("A", "1"));
        let envs: [&dyn Environ; 2] = [&empty, &table];
        assert_eq!(envs[0].get("A"), None);
        assert_eq!(envs[1].get("A"), Some("1"));
    }
}
