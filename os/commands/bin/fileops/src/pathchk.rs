//! Portability checking for path names behind `pathchk -p`.
//!
//! Ground truth: `minix3/usr.bin/pathchk/pathchk.c` (NetBSD 1.2). The
//! `-p` mode is pure: every `/`-separated component is at most
//! [`POSIX_NAME_MAX`] bytes (`_POSIX_NAME_MAX`, `minix3/include/limits.h:52`),
//! the whole path is shorter than [`POSIX_PATH_MAX`] (`_POSIX_PATH_MAX`,
//! `minix3/include/limits.h:55`; the C compares `strlen >= pathmax`, so
//! the usable length is one less), a component may not start with `-`
//! (`portable`, pathchk.c:174-191), and every byte must come from the
//! portable filename set `A-Za-z0-9._-`. The C's default mode probes the
//! live system through `pathconf` and `stat` (pathchk.c:103-107,
//! 124-128); that face is not available here, so the doing half serves
//! `-p` only and rejects the default mode explicitly.

/// Portable component limit (C: `_POSIX_NAME_MAX`).
pub const POSIX_NAME_MAX: usize = 14;
/// Portable whole-path limit (C: `_POSIX_PATH_MAX`; the comparison is
/// `strlen >= pathmax`).
pub const POSIX_PATH_MAX: usize = 256;

/// Why a path failed the `-p` check.
///
/// The C renders one `warnx` line per failure (pathchk.c:119, 131, 159);
/// the doing half formats these from the fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFault<'a> {
    /// A component longer than [`POSIX_NAME_MAX`].
    ComponentTooLong {
        /// The offending component.
        component: &'a str,
    },
    /// A component starting with `-`, or carrying a byte outside the
    /// portable set; the offending byte rides along.
    NonPortableByte {
        /// The offending component.
        component: &'a str,
        /// The first byte that is not portable.
        byte: u8,
    },
    /// The whole path reaches [`POSIX_PATH_MAX`].
    PathTooLong,
}

/// Runs the `-p` check over one path, returning the first fault.
///
/// Faithful to `check` + `portable` for the `-p` branch (pathchk.c:103,
/// 108-109, 111-149, 151-161): components are the `/`-separated runs
/// (empty components pass, so `/a//b` and a trailing slash are fine), the
/// length check runs before the character check, an empty path passes,
/// and the whole-path length uses `strlen >= POSIX_PATH_MAX`.
pub fn check_portable(path: &str) -> Result<(), PathFault<'_>> {
    let bytes = path.as_bytes();
    let mut start = 0;
    loop {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'/' {
            end += 1;
        }
        let component = &path[start..end];
        if component.len() > POSIX_NAME_MAX {
            return Err(PathFault::ComponentTooLong { component });
        }
        if let Some(byte) = first_non_portable(component) {
            return Err(PathFault::NonPortableByte {
                component,
                byte,
            });
        }
        if end == bytes.len() {
            break;
        }
        start = end;
    }
    if bytes.len() >= POSIX_PATH_MAX {
        return Err(PathFault::PathTooLong);
    }
    Ok(())
}

/// The first byte of `component` that fails portability: a leading `-` is
/// itself non-portable (C: `portable`, pathchk.c:183-184), then any byte
/// outside `A-Za-z0-9._-` (pathchk.c:177-180).
fn first_non_portable(component: &str) -> Option<u8> {
    for (index, &byte) in component.as_bytes().iter().enumerate() {
        let portable = matches!(byte,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-')
            && !(index == 0 && byte == b'-');
        if !portable {
            return Some(byte);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portable_paths_pass() {
        assert_eq!(check_portable("usr/bin/grep"), Ok(()));
        assert_eq!(check_portable("a.b_c-d"), Ok(()));
        assert_eq!(check_portable("/lead/slash"), Ok(()));
        assert_eq!(check_portable("double//slash"), Ok(()));
        assert_eq!(check_portable("trailing/"), Ok(()));
    }

    #[test]
    fn test_empty_path_passes_like_the_c() {
        // pathchk.c's loop never faults an empty path: the single empty
        // component passes both checks and strlen 0 is below the max.
        assert_eq!(check_portable(""), Ok(()));
    }

    #[test]
    fn test_leading_hyphen_rejected() {
        assert_eq!(
            check_portable("ok/-bad"),
            Err(PathFault::NonPortableByte {
                component: "-bad",
                byte: b'-',
            })
        );
    }

    #[test]
    fn test_non_portable_character_rejected() {
        assert_eq!(
            check_portable("bad+name"),
            Err(PathFault::NonPortableByte {
                component: "bad+name",
                byte: b'+',
            })
        );
        assert_eq!(
            check_portable("with space"),
            Err(PathFault::NonPortableByte {
                component: "with space",
                byte: b' ',
            })
        );
    }

    #[test]
    fn test_component_length_limit_is_fourteen() {
        let fourteen = "abcdefghijklmn";
        assert_eq!(fourteen.len(), POSIX_NAME_MAX);
        assert_eq!(check_portable(fourteen), Ok(()));
        let fifteen = "abcdefghijklmno";
        assert_eq!(
            check_portable(fifteen),
            Err(PathFault::ComponentTooLong {
                component: fifteen,
            })
        );
    }

    #[test]
    fn test_path_length_limit_is_256() {
        // 255 bytes of 7-byte components fits; one more byte trips the
        // whole-path check while every component is still fine.
        let ok = "abcdef/".repeat(36) + "abc";
        assert_eq!(ok.len(), POSIX_PATH_MAX - 1);
        assert_eq!(check_portable(&ok), Ok(()));
        let too_long = format!("{ok}x");
        assert_eq!(too_long.len(), POSIX_PATH_MAX);
        assert_eq!(check_portable(&too_long), Err(PathFault::PathTooLong));
    }

    #[test]
    fn test_component_check_precedes_the_path_length_check() {
        // The C checks each component inside the loop and the whole path
        // only after it, so a path that is both over the name limit and
        // over the path limit reports the component fault first.
        let long_bad = format!("/{}", "abcdefghijklmno".repeat(20));
        assert!(matches!(
            check_portable(&long_bad),
            Err(PathFault::ComponentTooLong { .. })
        ));
    }
}
