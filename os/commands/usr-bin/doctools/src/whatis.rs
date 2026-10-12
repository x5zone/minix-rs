//! The `whatis` database: `apropos` and `whatis` queries.
//!
//! Ground truth: `minix3/libexec/makewhatis/makewhatis.c` (1174 lines)
//! builds the database by scanning manual pages for their `NAME` sections;
//! `minix3/usr.bin/apropos/` and `minix3/usr.bin/whatis/` query it. Each
//! database row has the shape `name(section) - description` (several names
//! may share one row, comma separated). `whatis` matches names exactly,
//! `apropos` matches names and descriptions by keyword.

use crate::DocError;

use alloc::vec::Vec;

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

impl SliceManDb<'_> {
    /// Keyword face of `apropos`: every row carrying the keyword (ASCII
    /// case folded, no allocation), handed to `visit` in order. Returns
    /// the row count printed.
    pub fn for_each_matching(&self, keyword: &str, mut visit: impl FnMut(&str)) -> usize {
        let needle = keyword.as_bytes();
        if needle.is_empty() {
            return 0;
        }
        let mut hits = 0;
        for line in self.lines {
            let hay = line.as_bytes();
            if hay.len() < needle.len() {
                continue;
            }
            let mut hit = false;
            for start in 0..=hay.len() - needle.len() {
                if hay[start..start + needle.len()].eq_ignore_ascii_case(needle) {
                    hit = true;
                    break;
                }
            }
            if hit {
                hits += 1;
                visit(line);
            }
        }
        hits
    }
}

/// Argument-splitting failure for `whatis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhatisArgError {
    /// An option letter outside the `C M m P` set, or a value option with no
    /// argument (C `getopt` returns `?`/`:` → usage, `whatis.c:93-95`).
    Usage,
}

/// `whatis`'s parsed argv tail: the database-path override and the query
/// names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhatisArgs<'a> {
    /// `-M`/`-P` database path (the last one wins, as both set C's `p_path`);
    /// `None` means the caller's compiled-in default.
    pub db_path: Option<&'a str>,
    /// Query names in order, each already trimmed to its final path component
    /// (C trims `strrchr('/')`, `whatis.c:107-109`).
    pub names: Vec<&'a str>,
}

/// Split `whatis`'s argv tail (no program name) into [`WhatisArgs`], faithful
/// to C's `getopt(argc, argv, "C:M:m:P:")` (`whatis.c:81`): all four options
/// take a value, values cluster or attach (`-Mpath`) or follow (`-M path`),
/// `--` ends option scanning, a lone `-` is a name, and any other option letter
/// is [`WhatisArgError::Usage`] — never a leaked search name. `-M`/`-P` set the
/// database path; `-C` (config file) and `-m` (manpath augment) are accepted
/// and their values consumed but otherwise unused, because this model reads a
/// single classic text database (the multi-path resolution `makewhatis` builds
/// is the declared gap of 10-doc-man-tools.md §3.4). Operands are path-trimmed
/// exactly as C does before the lookup.
pub fn parse_whatis_args<'a>(args: &'a [&'a str]) -> Result<WhatisArgs<'a>, WhatisArgError> {
    let mut db_path: Option<&str> = None;
    let mut names: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let token = args[index];
        if token == "--" {
            names.extend(args[index + 1..].iter().map(|name| trim_path(name)));
            break;
        }
        let bytes = token.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            let letter = bytes[1];
            // Every `whatis` option takes a value: the rest of the token when
            // attached, else the following argument.
            let value = if bytes.len() > 2 {
                &token[2..]
            } else {
                index += 1;
                if index >= args.len() {
                    return Err(WhatisArgError::Usage);
                }
                args[index]
            };
            match letter {
                b'C' | b'm' => {} // config file / manpath augment: not modelled
                b'M' | b'P' => db_path = Some(value),
                _ => return Err(WhatisArgError::Usage),
            }
        } else {
            names.push(trim_path(token));
        }
        index += 1;
    }
    Ok(WhatisArgs { db_path, names })
}

/// `whatis.c:107-109`'s operand trim: a full path collapses to its final
/// component (`/usr/bin/foo` → `foo`); a bare name is unchanged.
fn trim_path(name: &str) -> &str {
    match name.rfind('/') {
        Some(at) => &name[at + 1..],
        None => name,
    }
}

/// `apropos`'s parsed argv tail: the database-path override and the keywords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AproposArgs<'a> {
    /// `-M`/`-P` database path (the last one wins); `None` means the caller's
    /// compiled-in default.
    pub db_path: Option<&'a str>,
    /// Keywords in order.
    pub keywords: Vec<&'a str>,
}

/// Split `apropos`'s argv tail into [`AproposArgs`], faithful to C's
/// `getopt(argc, argv, "C:M:m:P:S:s:")` (`apropos.c:85`) — whatis's face plus
/// `-S` (machine/arch) and `-s` (section), every option taking a value. All
/// four of whatis's rules apply: values cluster or attach (`-Mpath`) or follow
/// (`-M path`), `--` ends option scanning, a lone `-` is a keyword, and any
/// other letter is [`WhatisArgError::Usage`] — never a leaked keyword. `-M`/`-P`
/// set the database path; `-C`/`-m`/`-S`/`-s` are accepted and their values
/// consumed but unused (single classic text db; section/arch selection is a
/// declared gap). Unlike `whatis`, `apropos.c:120` lower-cases its operands
/// rather than path-trimming them; here the matching itself is case-folded
/// (see [`SliceManDb::for_each_matching`]), so the keywords are kept verbatim
/// and borrowed — the outcome is identical, without an allocation.
pub fn parse_apropos_args<'a>(args: &'a [&'a str]) -> Result<AproposArgs<'a>, WhatisArgError> {
    let mut db_path: Option<&str> = None;
    let mut keywords: Vec<&str> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let token = args[index];
        if token == "--" {
            keywords.extend_from_slice(&args[index + 1..]);
            break;
        }
        let bytes = token.as_bytes();
        if bytes.len() > 1 && bytes[0] == b'-' {
            let letter = bytes[1];
            // Every `apropos` option takes a value: attached, else next arg.
            let value = if bytes.len() > 2 {
                &token[2..]
            } else {
                index += 1;
                if index >= args.len() {
                    return Err(WhatisArgError::Usage);
                }
                args[index]
            };
            match letter {
                b'C' | b'm' | b'S' | b's' => {} // not modelled: single text db
                b'M' | b'P' => db_path = Some(value),
                _ => return Err(WhatisArgError::Usage),
            }
        } else {
            keywords.push(token);
        }
        index += 1;
    }
    Ok(AproposArgs { db_path, keywords })
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

    #[test]
    fn test_parse_whatis_operands_and_trim() {
        use alloc::vec;
        assert_eq!(
            parse_whatis_args(&["ls"]),
            Ok(WhatisArgs {
                db_path: None,
                names: vec!["ls"]
            })
        );
        // Full-path operands collapse to their last component (`whatis.c:107`).
        assert_eq!(
            parse_whatis_args(&["/usr/bin/foo", "bar/"]).map(|a| a.names),
            Ok(vec!["foo", ""])
        );
        // A lone `-` is a name, not an option.
        assert_eq!(parse_whatis_args(&["-"]).map(|a| a.names), Ok(vec!["-"]));
    }

    #[test]
    fn test_parse_whatis_db_path_options() {
        use alloc::vec;
        // `-M` value separate, and attached.
        assert_eq!(
            parse_whatis_args(&["-M", "/db", "ls"]),
            Ok(WhatisArgs {
                db_path: Some("/db"),
                names: vec!["ls"]
            })
        );
        assert_eq!(
            parse_whatis_args(&["-M/db", "ls"]).map(|a| a.db_path),
            Ok(Some("/db"))
        );
        // `-P` also sets the path; the last of `-M`/`-P` wins.
        assert_eq!(
            parse_whatis_args(&["-M", "/a", "-P", "/b", "ls"]).map(|a| a.db_path),
            Ok(Some("/b"))
        );
    }

    #[test]
    fn test_parse_whatis_value_options_are_not_leaked() {
        use alloc::vec;
        // The regression: `-m <path>` is consumed (its value is not a name), so
        // the old catch-all never searches for "-m" or "/augment" as man pages.
        assert_eq!(
            parse_whatis_args(&["-m", "/augment", "ls"]),
            Ok(WhatisArgs {
                db_path: None,
                names: vec!["ls"]
            })
        );
        assert_eq!(
            parse_whatis_args(&["-C", "/conf", "ls"]).map(|a| a.names),
            Ok(vec!["ls"])
        );
        // An unknown option letter is a usage error, never a name.
        assert_eq!(parse_whatis_args(&["-z", "ls"]), Err(WhatisArgError::Usage));
        // A value option with no argument is a usage error.
        assert_eq!(parse_whatis_args(&["-M"]), Err(WhatisArgError::Usage));
    }

    #[test]
    fn test_parse_whatis_double_dash() {
        use alloc::vec;
        // After `--` everything is a name, even a dash-leading token.
        assert_eq!(
            parse_whatis_args(&["--", "-M", "x"]),
            Ok(WhatisArgs {
                db_path: None,
                names: vec!["-M", "x"]
            })
        );
    }

    #[test]
    fn test_parse_apropos_keywords_and_no_trim() {
        use alloc::vec;
        assert_eq!(
            parse_apropos_args(&["ls"]),
            Ok(AproposArgs {
                db_path: None,
                keywords: vec!["ls"]
            })
        );
        // Unlike whatis, apropos does NOT path-trim operands (`apropos.c:120`
        // lower-cases; the fold lives in the matcher, keywords stay verbatim).
        assert_eq!(
            parse_apropos_args(&["/usr/bin/foo"]).map(|a| a.keywords),
            Ok(vec!["/usr/bin/foo"])
        );
        assert_eq!(
            parse_apropos_args(&["-M", "/db", "-M/db2", "k"]).map(|a| a.db_path),
            Ok(Some("/db2"))
        );
    }

    #[test]
    fn test_parse_apropos_value_options_are_not_leaked() {
        use alloc::vec;
        // The regression: `-m`/`-S`/`-s`/`-C` values are consumed, never keywords.
        assert_eq!(
            parse_apropos_args(&["-m", "/augment", "-S", "sun3", "-s", "1", "cp"]),
            Ok(AproposArgs {
                db_path: None,
                keywords: vec!["cp"]
            })
        );
        // Unknown option letter → usage, never a keyword.
        assert_eq!(
            parse_apropos_args(&["-z", "cp"]),
            Err(WhatisArgError::Usage)
        );
        // A value option with no argument → usage.
        assert_eq!(parse_apropos_args(&["-s"]), Err(WhatisArgError::Usage));
        // `--` ends options; dash-leading tokens after it are keywords.
        assert_eq!(
            parse_apropos_args(&["--", "-s", "1"]).map(|a| a.keywords),
            Ok(vec!["-s", "1"])
        );
    }
}
