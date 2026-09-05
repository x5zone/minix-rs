//! Name comparison with optional case folding (`name.c`).
//!
//! Case-insensitive mounts compare folded names everywhere (hashing and
//! equality alike); sensitive mounts compare bytes. Folding is plain
//! lowercase over bytes, matching the C `tolower` loop for the names
//! this filesystem accepts.

/// Fold one name for hashing when the mount is insensitive
/// (`normalize_name`, `name.c:18-34`).
pub fn normalize(name: &[u8], case_insensitive: bool) -> alloc::vec::Vec<u8> {
    if !case_insensitive {
        return name.to_vec();
    }
    name.iter().map(|byte| byte.to_ascii_lowercase()).collect()
}

/// Whether two names name the same entry (`compare_name`,
/// `name.c:39-51`).
pub fn equivalent(first: &[u8], second: &[u8], case_insensitive: bool) -> bool {
    if case_insensitive {
        first.eq_ignore_ascii_case(second)
    } else {
        first == second
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_folds_only_when_set() {
        assert_eq!(normalize(b"AbC", false), b"AbC");
        assert_eq!(normalize(b"AbC", true), b"abc");
    }

    #[test]
    fn test_equivalent_both_modes() {
        assert!(equivalent(b"file", b"file", false));
        assert!(!equivalent(b"File", b"file", false));
        assert!(equivalent(b"File", b"file", true));
        assert!(!equivalent(b"file", b"files", true));
    }
}
