//! `basename` and `dirname` path splitting.
//!
//! The two commands answer complementary questions about a path: the file
//! name at the end (`basename`) and the directory holding it (`dirname`).
//! Both are pure string surgery with edge cases the standard pins down:
//! trailing slashes are stripped first (except the root's own), an empty
//! result means the current directory (`.`) for `dirname` and the last
//! component for `basename`, and `basename` drops a matching suffix
//! (`basename /a/b.c .c` gives `b`).

use crate::FileOpError;

/// Split off the last component of `path`.
///
/// `a/b/c` gives `c`; `a/b/c/` gives `c`; `/` gives `/`; `a` gives `a`.
/// When `suffix` is non empty, equal to neither the whole name, and the
/// name ends with it, the suffix is dropped (`b.c` with `.c` gives `b`).
pub fn basename<'a>(path: &'a str, suffix: &str) -> Result<&'a str, FileOpError> {
    if path.is_empty() {
        return Err(FileOpError::InvalidArgument);
    }
    let stripped = path.trim_end_matches('/');
    let name = match stripped.rfind('/') {
        Some(index) => &stripped[index + 1..],
        None => stripped,
    };
    // The path was all slashes: the name is the root.
    let name = if name.is_empty() { "/" } else { name };
    if !suffix.is_empty() && name != suffix && name.ends_with(suffix) {
        Ok(&name[..name.len() - suffix.len()])
    } else {
        Ok(name)
    }
}

/// Split off everything after the last slash of `path`.
///
/// `a/b/c` gives `a/b`; `a/b/` gives `a`; `/a` gives `/`; `a` gives `.`;
/// `/` gives `/`. Trailing slashes are stripped first (except the root).
pub fn dirname(path: &str) -> Result<&str, FileOpError> {
    if path.is_empty() {
        return Err(FileOpError::InvalidArgument);
    }
    let stripped = path.trim_end_matches('/');
    if stripped.is_empty() {
        return Ok("/");
    }
    match stripped.rfind('/') {
        None => Ok("."),
        Some(0) => Ok("/"),
        Some(index) => {
            let dir = stripped[..index].trim_end_matches('/');
            Ok(if dir.is_empty() { "/" } else { dir })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basename_common() {
        assert_eq!(basename("a/b/c", ""), Ok("c"));
        assert_eq!(basename("a/b/c/", ""), Ok("c"));
        assert_eq!(basename("/", ""), Ok("/"));
        assert_eq!(basename("a", ""), Ok("a"));
    }

    #[test]
    fn test_basename_suffix() {
        assert_eq!(basename("/a/b.c", ".c"), Ok("b"));
        // A suffix equal to the whole name stays.
        assert_eq!(basename("b.c", "b.c"), Ok("b.c"));
        assert_eq!(basename("b.c", ".o"), Ok("b.c"));
    }

    #[test]
    fn test_dirname_common() {
        assert_eq!(dirname("a/b/c"), Ok("a/b"));
        assert_eq!(dirname("a/b/"), Ok("a"));
        assert_eq!(dirname("/a"), Ok("/"));
        assert_eq!(dirname("a"), Ok("."));
        assert_eq!(dirname("/"), Ok("/"));
        assert_eq!(dirname("//"), Ok("/"));
    }

    #[test]
    fn test_empty_path_rejected() {
        assert_eq!(basename("", ""), Err(FileOpError::InvalidArgument));
        assert_eq!(dirname(""), Err(FileOpError::InvalidArgument));
    }
}
