//! The `whatis` database: `apropos` and `whatis` queries.
//!
//! Ground truth: `minix3/libexec/makewhatis/makewhatis.c` (1174 lines)
//! builds the database by scanning manual pages for their `NAME` sections;
//! `minix3/usr.bin/apropos/` and `minix3/usr.bin/whatis/` query it. Each
//! database row has the shape `name(section) - description` (several names
//! may share one row, comma separated). `whatis` matches names exactly,
//! `apropos` matches names and descriptions by keyword.

use crate::DocError;

/// Maximum names sharing one database row.
pub const MAX_NAMES: usize = 8;

/// One parsed database row, borrowed from the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhatisEntry<'a> {
    /// Command names on this row.
    pub names: [&'a str; MAX_NAMES],
    /// How many of `names` are used.
    pub name_count: usize,
    /// Manual section (`1`, `8`, `3`, ...).
    pub section: &'a str,
    /// One line description.
    pub description: &'a str,
}

impl<'a> WhatisEntry<'a> {
    /// The names as a slice.
    pub fn name_list(&self) -> &[&'a str] {
        &self.names[..self.name_count]
    }
}

/// Parse one database line: `name[, name...](section) - description`.
///
/// The dash separator must be surrounded by blanks (` - `); names split on
/// commas with surrounding blanks trimmed. A row needs at least one name,
/// a section, and a non empty description.
pub fn parse_whatis_line<'a>(line: &'a str) -> Result<Option<WhatisEntry<'a>>, DocError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let (head, description) = trimmed.split_once(" - ").ok_or(DocError::InvalidArgument)?;
    if description.trim().is_empty() {
        return Err(DocError::InvalidArgument);
    }
    let open = head.find('(').ok_or(DocError::InvalidArgument)?;
    let close = head.find(')').ok_or(DocError::InvalidArgument)?;
    if close < open {
        return Err(DocError::InvalidArgument);
    }
    let section = head[open + 1..close].trim();
    if section.is_empty() {
        return Err(DocError::InvalidArgument);
    }
    let mut entry = WhatisEntry {
        names: [" "; MAX_NAMES],
        name_count: 0,
        section,
        description: description.trim(),
    };
    for name in head[..open].split(',') {
        let name = name.trim();
        if name.is_empty() || entry.name_count >= MAX_NAMES {
            return Err(DocError::InvalidArgument);
        }
        entry.names[entry.name_count] = name;
        entry.name_count += 1;
    }
    if entry.name_count == 0 {
        return Err(DocError::InvalidArgument);
    }
    Ok(Some(entry))
}

/// Read only manual database lookup.
pub trait ManDb {
    /// Find rows whose name list contains `name` exactly (`whatis`
    /// semantics): first match wins, or `None`.
    fn lookup_exact(&self, name: &str) -> Option<WhatisEntry<'_>>;
    /// True when any row's names or description contain `keyword`
    /// (`apropos` semantics).
    fn search_keyword(&self, keyword: &str) -> bool;
}

/// A database that knows nothing: every query misses. The honest starting
/// point until the database builder lands.
pub struct EmptyManDb;

impl ManDb for EmptyManDb {
    fn lookup_exact(&self, _name: &str) -> Option<WhatisEntry<'_>> {
        None
    }

    fn search_keyword(&self, _keyword: &str) -> bool {
        false
    }
}

/// A database over in memory `whatis` text lines.
///
/// Corrupt lines are skipped, never fatal: one bad row must not hide the
/// rest of the database.
pub struct SliceManDb<'a> {
    /// Raw database lines searched in order; the first name match wins.
    pub lines: &'a [&'a str],
}

impl ManDb for SliceManDb<'_> {
    fn lookup_exact(&self, name: &str) -> Option<WhatisEntry<'_>> {
        self.lines.iter().find_map(|line| {
            let entry = parse_whatis_line(line).ok()??;
            entry.name_list().contains(&name).then_some(entry)
        })
    }

    fn search_keyword(&self, keyword: &str) -> bool {
        if keyword.is_empty() {
            return false;
        }
        self.lines.iter().any(|line| {
            let Ok(Some(entry)) = parse_whatis_line(line) else {
                return false;
            };
            entry.name_list().iter().any(|name| name.contains(keyword))
                || entry.description.contains(keyword)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DB: [&str; 3] = [
        "ls(1) - list directory contents",
        "cp, mv(1) - copy and move files",
        "# not a real row, skipped",
    ];

    #[test]
    fn test_single_name_row() {
        let entry = parse_whatis_line(DB[0]).unwrap().unwrap();
        assert_eq!(entry.name_list(), &["ls"]);
        assert_eq!(entry.section, "1");
        assert_eq!(entry.description, "list directory contents");
    }

    #[test]
    fn test_shared_row() {
        let entry = parse_whatis_line(DB[1]).unwrap().unwrap();
        assert_eq!(entry.name_list(), &["cp", "mv"]);
    }

    #[test]
    fn test_blank_skipped() {
        assert_eq!(parse_whatis_line(""), Ok(None));
    }

    #[test]
    fn test_missing_separator_rejected() {
        assert_eq!(
            parse_whatis_line("ls(1) list files"),
            Err(DocError::InvalidArgument)
        );
        assert_eq!(
            parse_whatis_line("(1) - no name"),
            Err(DocError::InvalidArgument)
        );
        assert_eq!(
            parse_whatis_line("ls() - no section"),
            Err(DocError::InvalidArgument)
        );
    }

    #[test]
    fn test_exact_lookup() {
        let db = SliceManDb { lines: &DB };
        assert_eq!(db.lookup_exact("mv").unwrap().section, "1");
        assert_eq!(db.lookup_exact("ghost"), None);
    }

    #[test]
    fn test_keyword_search() {
        let db = SliceManDb { lines: &DB };
        assert!(db.search_keyword("directory"));
        assert!(db.search_keyword("mv"));
        assert!(!db.search_keyword("ghost"));
        assert!(!db.search_keyword(""));
    }

    #[test]
    fn test_empty_db_misses() {
        let db = EmptyManDb;
        assert_eq!(db.lookup_exact("ls"), None);
        assert!(!db.search_keyword("ls"));
    }
}
