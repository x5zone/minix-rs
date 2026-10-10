//! `pwd` — report the current working directory.
//!
//! Ground truth: `minix3/bin/pwd/pwd.c`. `pwd` defaults to the *physical* path
//! (`getcwd`); with `-L` it first trusts the shell's cached `$PWD` — but only if
//! `getcwd_logical` approves it. That function's cheap string pre-checks are the
//! pure part reproduced here: `$PWD` must be absolute and contain no `/./` or
//! `/../` component (a cache built through symlinks or dot-components would not
//! name the real directory). The remaining gate — that `$PWD` and `.` resolve to
//! the same `st_dev`/`st_ino` — needs `stat` and stays with the caller.

/// BSD `getcwd_logical`'s string pre-checks on the `PWD` value (the value with
/// its `PWD=` prefix already stripped): non-empty and starting with `/`, and no
/// `/./` or `/../` substring. The dev/inode identity check is the caller's.
pub fn env_pwd_usable(pwd: &[u8]) -> bool {
    pwd.first() == Some(&b'/') && !contains(pwd, b"/./") && !contains(pwd, b"/../")
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.len() >= needle.len() && hay.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_clean_path_is_usable() {
        assert!(env_pwd_usable(b"/home/user/proj"));
        assert!(env_pwd_usable(b"/"));
    }

    #[test]
    fn relative_or_empty_is_rejected() {
        assert!(!env_pwd_usable(b""));
        assert!(!env_pwd_usable(b"relative/path"));
        assert!(!env_pwd_usable(b"./x"));
    }

    #[test]
    fn dot_components_are_rejected() {
        assert!(!env_pwd_usable(b"/a/./b"));
        assert!(!env_pwd_usable(b"/a/../b"));
    }

    #[test]
    fn trailing_dot_without_slash_is_allowed() {
        // The C checks look for "/./" and "/../" (trailing slash), so a bare
        // trailing "/." is not caught by the string stage.
        assert!(env_pwd_usable(b"/a/b/."));
    }
}
