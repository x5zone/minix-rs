//! Build configuration vocabulary.
//!
//! Ground truth: `minix3/etc/mk.conf`. The file is five lines: a default
//! assignment (`MAKEVERBOSE?= 1`) and one conditional block guarding the
//! package builder variable. The build system reads these lines; this module
//! owns the assignment shapes so tools can validate the file without running
//! the builder.

use crate::PkgError;

/// Assignment operators understood in configuration files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    /// `=`: plain assignment.
    Plain,
    /// `?=`: default assignment (only when unset).
    Default,
    /// `+=`: append assignment.
    Append,
}

/// Parse an assignment operator word (`=`, `?=`, `+=`).
pub fn parse_assign_op(word: &str) -> Result<AssignOp, PkgError> {
    match word {
        "=" => Ok(AssignOp::Plain),
        "?=" => Ok(AssignOp::Default),
        "+=" => Ok(AssignOp::Append),
        _ => Err(PkgError::InvalidArgument),
    }
}

/// One configuration assignment (`name operator value`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MkAssign<'a> {
    /// Variable name.
    pub name: &'a str,
    /// Assignment operator.
    pub op: AssignOp,
    /// Assigned text (may be empty).
    pub value: &'a str,
}

/// Parse one configuration line.
///
/// Comment lines (leading `#`) and blank lines are not assignments and report
/// not found so callers can skip them. Conditional lines (leading `.`) are
/// malformed for this parser (the builder owns them).
pub fn parse_mk_line(line: &str) -> Result<MkAssign<'_>, PkgError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Err(PkgError::NotFound);
    }
    if trimmed.starts_with('.') {
        return Err(PkgError::InvalidArgument);
    }
    for op_word in ["?=", "+=", "="] {
        if let Some((name, value)) = trimmed.split_once(op_word) {
            let name = name.trim();
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            {
                return Err(PkgError::InvalidArgument);
            }
            let op = parse_assign_op(op_word)?;
            return Ok(MkAssign {
                name,
                op,
                value: value.trim(),
            });
        }
    }
    Err(PkgError::InvalidArgument)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_assignment_parses() {
        let parsed = parse_mk_line("MAKEVERBOSE?= 1").unwrap();
        assert_eq!(parsed.name, "MAKEVERBOSE");
        assert_eq!(parsed.op, AssignOp::Default);
        assert_eq!(parsed.value, "1");
    }

    #[test]
    fn test_plain_and_append_parse() {
        assert_eq!(parse_mk_line("A = b").unwrap().op, AssignOp::Plain);
        assert_eq!(parse_mk_line("A += b").unwrap().op, AssignOp::Append);
    }

    #[test]
    fn test_comments_and_blanks_skipped() {
        assert_eq!(parse_mk_line("# comment"), Err(PkgError::NotFound));
        assert_eq!(parse_mk_line("   "), Err(PkgError::NotFound));
    }

    #[test]
    fn test_conditionals_rejected() {
        assert_eq!(
            parse_mk_line(".if defined(X)"),
            Err(PkgError::InvalidArgument)
        );
    }

    #[test]
    fn test_bad_names_rejected() {
        assert_eq!(parse_mk_line("has-dash = 1"), Err(PkgError::InvalidArgument));
        assert_eq!(parse_mk_line("= 1"), Err(PkgError::InvalidArgument));
        assert_eq!(parse_mk_line("NOOPERATOR"), Err(PkgError::InvalidArgument));
    }
}
