//! Manual configuration file directives (`man.conf`).
//!
//! Ground truth: `minix3/etc/man.conf` parsed by
//! `minix3/usr.bin/man/manconf.c` (272 lines). Directives start with an
//! underscore keyword:
//!
//! - `_whatdb path`: where the `whatis` database lives.
//! - `_subdir list`: section directories searched in order (`man1 man8
//!   ...` — order decides which page wins when several sections hold the
//!   same name).
//! - `_suffix ext`: default page file extension.
//! - `_build pattern command`: how to render a page (suffix patterns with
//!   `%s` for the file, piped through decompressors and formatters).
//! - `_version`, `_machine`, `_build_filters`, ...: single value settings
//!   this module reports opaquely (parsed, not interpreted).
//!
//! Comment lines (`#` first) and blank lines are skipped. Unknown
//! directives are an error (a typo must not silently change where pages
//! come from).

use crate::DocError;

/// Maximum subsection names kept from one `_subdir` line.
pub const MAX_SUBDIRS: usize = 32;
/// Maximum build rules kept.
pub const MAX_BUILDS: usize = 16;

/// One parsed configuration directive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Directive<'a> {
    /// `_whatdb path`.
    WhatDb(&'a str),
    /// `_subdir` names in search order.
    Subdirs([&'a str; MAX_SUBDIRS], usize),
    /// `_suffix ext`.
    Suffix(&'a str),
    /// `_build pattern command`.
    Build(&'a str, &'a str),
    /// Any other `_keyword value...`: reported with its words for the
    /// caller to interpret or ignore.
    Other(&'a str, [&'a str; 8], usize),
}

/// Parse one configuration line; comments and blanks yield `Ok(None)`.
pub fn parse_directive<'a>(line: &'a str) -> Result<Option<Directive<'a>>, DocError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return Ok(None);
    }
    let mut words = trimmed.split_whitespace();
    let keyword = words.next().ok_or(DocError::InvalidArgument)?;
    if !keyword.starts_with('_') {
        return Err(DocError::InvalidArgument);
    }
    match keyword {
        "_whatdb" => {
            let path = words.next().ok_or(DocError::InvalidArgument)?;
            Ok(Some(Directive::WhatDb(path)))
        }
        "_subdir" => {
            let mut names: [&'a str; MAX_SUBDIRS] = [""; MAX_SUBDIRS];
            let mut count = 0;
            for word in words {
                if count >= MAX_SUBDIRS {
                    return Err(DocError::InvalidArgument);
                }
                names[count] = word;
                count += 1;
            }
            if count == 0 {
                return Err(DocError::InvalidArgument);
            }
            Ok(Some(Directive::Subdirs(names, count)))
        }
        "_suffix" => {
            let ext = words.next().ok_or(DocError::InvalidArgument)?;
            Ok(Some(Directive::Suffix(ext)))
        }
        "_build" => {
            let pattern = words.next().ok_or(DocError::InvalidArgument)?;
            if words.next().is_none() {
                return Err(DocError::InvalidArgument);
            }
            // Re-slice the command from the original line so inner spacing
            // survives exactly as written.
            let command = command_slice(trimmed, pattern);
            Ok(Some(Directive::Build(pattern, command)))
        }
        _ => {
            let mut rest: [&'a str; 8] = [""; 8];
            let mut count = 0;
            for word in words {
                if count >= rest.len() {
                    return Err(DocError::InvalidArgument);
                }
                rest[count] = word;
                count += 1;
            }
            Ok(Some(Directive::Other(keyword, rest, count)))
        }
    }
}

/// Recover the build command: everything after the pattern word in `line`.
fn command_slice<'a>(line: &'a str, pattern: &str) -> &'a str {
    let pos = line.find(pattern).unwrap_or(0) + pattern.len();
    line[pos..].trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_whatdb_from_live_conf() {
        let directive = parse_directive("_whatdb /usr/man/whatis.db").unwrap().unwrap();
        assert_eq!(directive, Directive::WhatDb("/usr/man/whatis.db"));
    }

    #[test]
    fn test_subdir_order_preserved() {
        let directive = parse_directive("_subdir\t\tcat1 man1 cat8 man8").unwrap().unwrap();
        match directive {
            Directive::Subdirs(names, count) => {
                assert_eq!(count, 4);
                assert_eq!(&names[..count], &["cat1", "man1", "cat8", "man8"]);
            }
            _ => panic!("wrong directive"),
        }
    }

    #[test]
    fn test_build_keeps_pipeline() {
        let directive = parse_directive("_build\t\t.[1-9ln].gz\t\t/usr/bin/zcat %s | /usr/bin/mandoc")
            .unwrap()
            .unwrap();
        match directive {
            Directive::Build(pattern, command) => {
                assert_eq!(pattern, ".[1-9ln].gz");
                assert_eq!(command, "/usr/bin/zcat %s | /usr/bin/mandoc");
            }
            _ => panic!("wrong directive"),
        }
    }

    #[test]
    fn test_comment_and_blank_skipped() {
        assert_eq!(parse_directive("# comment"), Ok(None));
        assert_eq!(parse_directive("   "), Ok(None));
    }

    #[test]
    fn test_bare_keyword_rejected() {
        assert_eq!(
            parse_directive("_whatdb"),
            Err(DocError::InvalidArgument)
        );
        assert_eq!(
            parse_directive("_build .gz"),
            Err(DocError::InvalidArgument)
        );
    }

    #[test]
    fn test_non_directive_rejected() {
        assert_eq!(
            parse_directive("man1 man8"),
            Err(DocError::InvalidArgument)
        );
    }
}
