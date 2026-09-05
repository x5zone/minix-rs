//! Directory hierarchy specification lines (`mtree` format).
//!
//! Ground truth: `minix3/etc/mtree` (BSD `mtree` specification: one path
//! per line followed by whitespace separated `key=value` attributes such
//! as `type=dir mode=0755`) consumed by the `mtree` verification command in
//! `minix3/usr.sbin/mtree/`.
//!
//! Each line states what one path in the installed system must look like.
//! This module parses the line into a path plus raw attributes; comparing
//! attributes against the live file system stays with the caller.

use crate::DevDbError;

/// Maximum attributes kept per line; more is a malformed line.
pub const MAX_ATTRIBUTES: usize = 16;

/// One parsed `mtree` row, borrowed from the line it was parsed from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtreeEntry<'a> {
    /// Path the line describes, for example `./etc`.
    pub path: &'a str,
    /// Raw `key=value` attribute texts.
    pub attributes: [&'a str; MAX_ATTRIBUTES],
    /// How many of `attributes` are used.
    pub attribute_count: usize,
}

impl<'a> MtreeEntry<'a> {
    /// The attributes as a slice.
    pub fn attribute_list(&self) -> &[&'a str] {
        &self.attributes[..self.attribute_count]
    }

    /// Fetch the value of attribute `key` (`None` when absent or when the
    /// attribute has no `=` separator).
    pub fn attribute(&self, key: &str) -> Option<&'a str> {
        self.attribute_list().iter().find_map(|attribute| {
            let (name, value) = attribute.split_once('=')?;
            (name == key).then_some(value)
        })
    }
}

/// Parse one `mtree` specification line.
///
/// Comment lines (`#` first) and blank lines yield `Ok(None)`. Directives
/// (`/set`, `/unset`) are not entries and yield `Ok(None)` as well: they
/// adjust defaults for following lines, which is the verifier's job, not
/// the parser's. Every other word after the path must contain `=`.
pub fn parse_mtree_line<'a>(line: &'a str) -> Result<Option<MtreeEntry<'a>>, DevDbError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let mut words = trimmed.split_whitespace();
    let path = words.next().ok_or(DevDbError::InvalidArgument)?;
    if path.starts_with('/') && (path == "/set" || path == "/unset") {
        return Ok(None);
    }
    let mut entry = MtreeEntry {
        path,
        attributes: [""; MAX_ATTRIBUTES],
        attribute_count: 0,
    };
    for word in words {
        if !word.contains('=') {
            return Err(DevDbError::InvalidArgument);
        }
        if entry.attribute_count >= MAX_ATTRIBUTES {
            return Err(DevDbError::InvalidArgument);
        }
        entry.attributes[entry.attribute_count] = word;
        entry.attribute_count += 1;
    }
    Ok(Some(entry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directory_entry() {
        let entry = parse_mtree_line("./etc type=dir mode=0755")
            .unwrap()
            .unwrap();
        assert_eq!(entry.path, "./etc");
        assert_eq!(entry.attribute("type"), Some("dir"));
        assert_eq!(entry.attribute("mode"), Some("0755"));
        assert_eq!(entry.attribute("owner"), None);
    }

    #[test]
    fn test_directives_skipped() {
        assert_eq!(parse_mtree_line("/set type=file mode=0644"), Ok(None));
        assert_eq!(parse_mtree_line("/unset mode"), Ok(None));
    }

    #[test]
    fn test_comment_and_blank_skipped() {
        assert_eq!(parse_mtree_line("# mtree spec"), Ok(None));
        assert_eq!(parse_mtree_line(""), Ok(None));
    }

    #[test]
    fn test_bare_word_rejected() {
        assert_eq!(
            parse_mtree_line("./etc type=dir extra"),
            Err(DevDbError::InvalidArgument)
        );
    }

    #[test]
    fn test_empty_line_rejected() {
        assert_eq!(parse_mtree_line("   "), Ok(None));
    }
}
