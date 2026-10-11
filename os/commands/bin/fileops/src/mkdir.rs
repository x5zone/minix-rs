//! Directory creation decisions.
//!
//! Ground truth: `minix3/bin/mkdir/mkdir.c` (NetBSD `mkdir.c` 1.38). The
//! command accepts `-p` (create every missing parent component) and `-m mode`
//! (set the mode with `setmode`/`getmode`). The default mode is `0777`, which
//! the `mkdir(2)` system call masks with the process umask. For `-p`,
//! `mkpath` walks the path slash by slash, creating each leading component
//! with an intermediate mode of `mode | u+wx` (so the walk can always descend)
//! and the final component with the requested mode; when a component cannot be
//! created it `stat`s it to tell "already a directory" (keep going) apart from
//! "exists but is not a directory" (`ENOTDIR`, stop). A freshly created final
//! component whose mode carries bits outside `0777` (sticky, set-user-id,
//! set-group-id) is re-applied with `chmod`, because `mkdir` and `umask`
//! honour only the nine permission bits (`mkdir.c` "So chmod()").
//!
//! This module owns the pure shape of that walk: how a path splits into the
//! prefixes to create, which mode each one gets, and whether the extra-bit
//! `chmod` fix-up is needed. The filesystem actions (`mkdir`, `stat`, `chmod`)
//! stay with the execution layer.

use crate::mode::apply_mode;
use crate::FileOpError;

/// The nine permission bits (`S_IRWXU | S_IRWXG | S_IRWXO`), C `0777`.
pub const ALL_PERMISSIONS: u16 = 0o777;

/// Default creation mode when `-m` is absent; `mkdir(2)` still masks umask.
pub const DEFAULT_MODE: u16 = ALL_PERMISSIONS;

/// User write and execute bits (`S_IWUSR | S_IXUSR`), C `0300`.
const USER_WX: u16 = 0o300;

/// Mode for an intermediate `-p` component: the requested mode with `u+wx`
/// forced on so the walk can always create the child underneath it.
pub fn intermediate_mode(mode: u16) -> u16 {
    mode | USER_WX
}

/// True when `mode` carries bits outside the nine permission bits — the
/// sticky, set-user-id, or set-group-id bits that `mkdir(2)` loses and
/// `mkdir.c` therefore re-applies with a follow-up `chmod`.
pub fn needs_extra_bit_fixup(mode: u16) -> bool {
    mode & !ALL_PERMISSIONS != 0
}

/// Resolve a `-m` mode string. An octal or symbolic spec is applied to a
/// `0777` base, matching C `getmode(setmode(optarg), S_IRWXU|S_IRWXG|S_IRWXO)`;
/// the shared [`apply_mode`] already ported BSD `setmode`/`getmode`.
pub fn parse_mode_spec(spec: &str) -> Result<u16, FileOpError> {
    apply_mode(ALL_PERMISSIONS, spec)
}

/// Walk the `-p` prefix sequence of `path`, calling `visit` once per component
/// from the outermost inward, with `is_last` true only for the final
/// component. Trailing slashes are ignored, so the last real component is the
/// terminal one. A path made only of slashes (or empty) yields no components.
///
/// This mirrors `mkpath`'s `strspn`/`strcspn` scan; unlike C, which mutates the
/// buffer in place, this borrows slices of the input.
pub fn for_each_parent<'a>(path: &'a str, visit: &mut dyn FnMut(&'a str, bool)) {
    let bytes = path.as_bytes();
    // The terminal prefix ends just past the final non-slash byte.
    let mut final_end = bytes.len();
    while final_end > 0 && bytes[final_end - 1] == b'/' {
        final_end -= 1;
    }
    let mut end = 0;
    loop {
        while end < bytes.len() && bytes[end] == b'/' {
            end += 1;
        }
        if end >= bytes.len() {
            break;
        }
        while end < bytes.len() && bytes[end] != b'/' {
            end += 1;
        }
        visit(&path[..end], end == final_end);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn prefixes(path: &str) -> Vec<(&str, bool)> {
        let mut out = Vec::new();
        for_each_parent(path, &mut |prefix, is_last| out.push((prefix, is_last)));
        out
    }

    #[test]
    fn test_absolute_path_prefixes() {
        assert_eq!(
            prefixes("/a/b/c"),
            vec![("/a", false), ("/a/b", false), ("/a/b/c", true)]
        );
    }

    #[test]
    fn test_relative_path_prefixes() {
        assert_eq!(prefixes("a/b"), vec![("a", false), ("a/b", true)]);
        assert_eq!(prefixes("a"), vec![("a", true)]);
    }

    #[test]
    fn test_leading_and_trailing_slashes() {
        // A bare root or slashes-only path has nothing to create.
        assert!(prefixes("/").is_empty());
        // Trailing slashes still terminal at the last real component.
        assert_eq!(prefixes("/a/"), vec![("/a", true)]);
        assert_eq!(prefixes("/a//b//"), vec![("/a", false), ("/a//b", true)]);
    }

    #[test]
    fn test_intermediate_mode_forces_user_wx() {
        // An intermediate component is always traversable (u+wx).
        assert_eq!(intermediate_mode(0o500), 0o700);
        assert_eq!(intermediate_mode(0o777), 0o777);
    }

    #[test]
    fn test_extra_bit_fixup() {
        assert!(needs_extra_bit_fixup(0o2755)); // setgid present
        assert!(needs_extra_bit_fixup(0o4700)); // setuid present
        assert!(!needs_extra_bit_fixup(0o755)); // plain permissions only
    }

    #[test]
    fn test_parse_mode_spec_octal_and_symbolic() {
        // Octal `-m 2755` keeps the setgid bit for the follow-up chmod.
        assert_eq!(parse_mode_spec("2755").unwrap(), 0o2755);
        assert_eq!(parse_mode_spec("755").unwrap(), 0o755);
        // Symbolic applies to the 0777 base: `go-rwx` clears group and other,
        // leaving user's 0700; the follow-up `u+x` is already present (no-op).
        assert_eq!(parse_mode_spec("go-rwx,u+x").unwrap(), 0o700);
        assert_eq!(parse_mode_spec(""), Err(FileOpError::InvalidArgument));
    }
}
