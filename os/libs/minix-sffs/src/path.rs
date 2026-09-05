//! Host-path building: prefix joins plus component push and pop
//! (`path.c`: `make_path`, `push_path`, `pop_path`).
//!
//! A guest node is a chain of names from the root; its host path is the
//! mount prefix plus those names joined with single slashes. Building
//! runs right-to-left in C (names prepend into a scratch buffer); here
//! the chain arrives as a slice and joins left-to-right, which is the
//! same string without the scratch dance. Every join checks the bound
//! first: overlong paths refuse with overlong, never truncate.

use minix_types::{EINVAL, ENAMETOOLONG, Errno};

/// Why path building failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// Bad node (detached chain reaching no root).
    Invalid,
    /// Result would not fit (`ENAMETOOLONG`).
    NameTooLong,
}

impl PathError {
    /// The wire error code.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::Invalid => Errno::from_i32(EINVAL),
            Self::NameTooLong => Errno::from_i32(ENAMETOOLONG),
        }
    }
}

/// Join a prefix with a chain of names (`make_path`, `path.c:17-65`).
/// Leading slashes leave the prefix (they add no information), an empty
/// prefix yields a relative path starting at the first name, and a
/// detached chain (marked by the caller with `rooted = false`) reports
/// missing instead of a half path (`path.c:48-52`).
pub fn join(prefix: &str, chain: &[&str], rooted: bool, max: usize) -> Result<alloc::string::String, PathError> {
    use alloc::string::String;
    if !rooted {
        return Err(PathError::Invalid);
    }
    let trimmed = prefix.trim_start_matches('/');
    let mut total = trimmed.len();
    for name in chain {
        total += name.len() + 1;
    }
    if total >= max {
        return Err(PathError::NameTooLong);
    }
    let mut out = String::new();
    out.push_str(trimmed);
    for name in chain {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(name);
    }
    Ok(out)
}

/// Append one component (`push_path`, `path.c:70-87`): a separator goes
/// in only when something precedes it, and the bound counts the
/// separator before writing anything.
pub fn push(path: &mut alloc::string::String, name: &str, max: usize) -> Result<(), PathError> {
    let mut add = name.len();
    if !path.is_empty() {
        add += 1;
    }
    if path.len() + add >= max {
        return Err(PathError::NameTooLong);
    }
    if !path.is_empty() {
        path.push('/');
    }
    path.push_str(name);
    Ok(())
}

/// Drop the last component (`pop_path`, `path.c:92-108`): cut at the
/// final slash, or clear a slash-free path to empty (popping the root
/// itself is a caller bug, and the empty result shows it).
pub fn pop(path: &mut alloc::string::String) {
    match path.rfind('/') {
        Some(at) => path.truncate(at),
        None => path.clear(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec::Vec;

    #[test]
    fn test_join_prefix_and_chain() {
        assert_eq!(join("/share", &["a", "b"], true, 4096).unwrap(), "share/a/b");
        // Empty prefix yields a relative path.
        assert_eq!(join("", &["a"], true, 4096).unwrap(), "a");
        // Detached chains refuse.
        assert_eq!(join("/share", &["a"], false, 4096).unwrap_err(), PathError::Invalid);
        // Bounds count before writing: seven bytes do not fit seven.
        assert_eq!(join("/share", &["a"], true, 7).unwrap_err(), PathError::NameTooLong);
    }

    #[test]
    fn test_push_separator_rules() {
        let mut path = "a".to_string();
        push(&mut path, "b", 4096).unwrap();
        assert_eq!(path, "a/b");
        let mut empty = "".to_string();
        push(&mut empty, "b", 4096).unwrap();
        assert_eq!(empty, "b");
        let mut full = "a".to_string();
        assert_eq!(push(&mut full, "bcdef", 6).unwrap_err(), PathError::NameTooLong);
        assert_eq!(full, "a");
    }

    #[test]
    fn test_pop_last_component() {
        let mut path = "share/a/b".to_string();
        pop(&mut path);
        assert_eq!(path, "share/a");
        pop(&mut path);
        assert_eq!(path, "share");
        // No slash left: popping clears to empty (the root itself has
        // no parent to name).
        pop(&mut path);
        assert_eq!(path, "");
        let _ = Vec::<u8>::new();
    }
}
