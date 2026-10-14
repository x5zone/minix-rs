//! Portability checking for path names behind `pathchk -p`.
//!
//! Ground truth: `minix3/usr.bin/pathchk/pathchk.c` (NetBSD 1.2). The
//! `-p` mode is pure: every `/`-separated component is at most
//! [`POSIX_NAME_MAX`] bytes (`_POSIX_NAME_MAX`, `minix3/include/limits.h:52`),
//! the whole path is shorter than [`POSIX_PATH_MAX`] (`_POSIX_PATH_MAX`,
//! `minix3/include/limits.h:55`; the C compares `strlen >= pathmax`, so
//! the usable length is one less), a component may not start with `-`
//! (`portable`, pathchk.c:174-191), and every byte must come from the
//! portable filename set `A-Za-z0-9._-`. The default mode instead asks the live
//! system two `pathconf` questions and a `stat` per component
//! (pathchk.c:103-161); [`check_system`] owns that logic.

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
            return Err(PathFault::NonPortableByte { component, byte });
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

/// What `pathconf` answers when it fails with an errno set: C falls back to
/// `NAME_MAX` (`pathchk.c:107`, `:144`; `minix3/sys/sys/syslimits.h:57`).
pub const SYSTEM_NAME_MAX: i64 = 511;
/// The same fallback for `_PC_PATH_MAX` (`pathchk.c:155`;
/// `minix3/sys/sys/syslimits.h:64`).
pub const SYSTEM_PATH_MAX: i64 = 1024;

/// The three questions C's default mode asks the system.
///
/// `Ok(-1)` is C's `-1` with `errno` left zero — POSIX's "no limit", which
/// switches the corresponding check off (`pathchk.c:118`, `:158`). An `Err` is
/// C's `-1` with an errno set, which takes the documented fallback instead.
pub trait PathProbe {
    /// `pathconf(path, _PC_NAME_MAX)` (`pathchk.c:105`, `:142`).
    fn name_max(&mut self, path: &str) -> Result<i64, minix_sys::Errno>;
    /// `pathconf(path, _PC_PATH_MAX)` (`pathchk.c:153`).
    fn path_max(&mut self, path: &str) -> Result<i64, minix_sys::Errno>;
    /// `stat(path)`: only the failure and its number matter — C ignores the
    /// buffer and tests `errno != ENOENT` (`pathchk.c:124-126`).
    fn stat(&mut self, path: &str) -> Result<(), minix_sys::Errno>;
}

/// Why C's default mode refuses a path. The fields are pieces of the original
/// operand, so the shell can render C's `warnx`/`warn` text exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemFault<'a> {
    /// `complen > namemax` (`pathchk.c:118-121`).
    ComponentTooLong { component: &'a str, limit: i64 },
    /// `stat` failed with something other than `ENOENT` on the copy truncated
    /// after this component (`pathchk.c:124-128`). C's `%.*s` prints
    /// `strlen(pathd) - complen - 1` bytes — everything before the component
    /// minus one more — so `prefix` is that text. A component starting at index
    /// zero makes that count negative, and a negative precision means "no
    /// precision" to C's printf; `whole` marks that case (it is unreachable on
    /// a real tree, where the parent of a first component is `.`).
    StatFailed {
        prefix: &'a str,
        whole: bool,
        errno: minix_sys::Errno,
    },
    /// `strlen(path) >= pathmax` (`pathchk.c:158-160`); `limit` is the
    /// `pathmax - 1` C prints.
    PathTooLong { limit: i64 },
}

/// C's default `check()` (`pathchk.c:91-167`) for one path, against a probe.
///
/// Faithful in the parts a reader tends to smooth away:
///
/// * the first `_PC_NAME_MAX` question is about `"/"` for an absolute path and
///   about `"."` otherwise, asked once before the loop (`:103-109`);
/// * the component-length check runs **before** the `stat`, and the limit in
///   force is the one learned for the directory just crossed (`:139-145`) — a
///   failed reprobe keeps the previous answer instead of resetting it;
/// * `-1` and "error" are two different answers (see [`PathProbe`]);
/// * `ENOENT` is not a refusal at all — `pathchk /does/not/exist` exits 0;
/// * the whole-path question is asked only after every component passed.
pub fn check_system<'a>(path: &'a str, probe: &mut impl PathProbe) -> Result<(), SystemFault<'a>> {
    let bytes = path.as_bytes();
    let root = if bytes.first() == Some(&b'/') {
        "/"
    } else {
        "."
    };
    let mut namemax = probe.name_max(root).unwrap_or(SYSTEM_NAME_MAX);
    let mut start = 0usize;
    loop {
        while start < bytes.len() && bytes[start] == b'/' {
            start += 1;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'/' {
            end += 1;
        }
        let component = &path[start..end];
        if namemax != -1 && (end - start) as i64 > namemax {
            return Err(SystemFault::ComponentTooLong {
                component,
                limit: namemax,
            });
        }
        // C stats its copy with the string ended right after this component, which
        // is the prefix `path[0..end]`; `ENOENT` is not a refusal at all.
        match probe.stat(&path[..end]) {
            Err(errno) if errno != minix_sys::Errno::ENOENT => {
                return Err(SystemFault::StatFailed {
                    prefix: &path[..start.saturating_sub(1)],
                    whole: start == 0,
                    errno,
                });
            }
            _ => {}
        }
        if end == bytes.len() {
            break;
        }
        namemax = probe.name_max(&path[..end]).unwrap_or(namemax);
        start = end;
    }
    let pathmax = probe.path_max(path).unwrap_or(SYSTEM_PATH_MAX);
    if pathmax != -1 && bytes.len() as i64 >= pathmax {
        return Err(SystemFault::PathTooLong { limit: pathmax - 1 });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    /// C's `ENOTDIR` (`minix3/sys/errno.h` through `errno.rs:34`).
    const ENOTDIR: i32 = 20;

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
            Err(PathFault::ComponentTooLong { component: fifteen })
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

    /// Answers the three probe questions from canned data. A `name_max` or
    /// `path_max` answer of [`Self::FAILS`] stands for C's `-1` with an errno set;
    /// `-1` itself stays the distinct "no limit" answer.
    struct Fake {
        name_maxes: Vec<i64>,
        path_max: i64,
        missing: Vec<&'static str>,
        broken: Vec<(&'static str, i32)>,
        seen: Vec<String>,
    }

    const FAILS: i64 = i64::MIN;

    impl Default for Fake {
        fn default() -> Self {
            Fake {
                name_maxes: Vec::new(),
                path_max: -1,
                missing: Vec::new(),
                broken: Vec::new(),
                seen: Vec::new(),
            }
        }
    }

    impl PathProbe for Fake {
        fn name_max(&mut self, path: &str) -> Result<i64, minix_sys::Errno> {
            self.seen.push(String::from(path));
            match self.name_maxes.remove(0) {
                FAILS => Err(minix_sys::Errno::EINVAL),
                value => Ok(value),
            }
        }
        fn path_max(&mut self, path: &str) -> Result<i64, minix_sys::Errno> {
            self.seen.push(String::from(path));
            if self.path_max == FAILS {
                Err(minix_sys::Errno::EINVAL)
            } else {
                Ok(self.path_max)
            }
        }
        fn stat(&mut self, path: &str) -> Result<(), minix_sys::Errno> {
            if self.missing.contains(&path) {
                return Err(minix_sys::Errno::ENOENT);
            }
            if let Some((_, number)) = self.broken.iter().find(|(bad, _)| *bad == path) {
                return Err(minix_sys::Errno::from_i32(*number));
            }
            Ok(())
        }
    }

    #[test]
    fn the_first_question_is_about_the_root_or_the_working_directory() {
        // `pathconf(*p == '/' ? "/" : ".", _PC_NAME_MAX)` (pathchk.c:105).
        let mut absolute = Fake {
            name_maxes: vec![FAILS, -1],
            ..Fake::default()
        };
        assert_eq!(check_system("/usr/bin", &mut absolute), Ok(()));
        assert_eq!(&absolute.seen[0], "/");
        let mut relative = Fake {
            name_maxes: vec![FAILS, -1],
            ..Fake::default()
        };
        assert_eq!(check_system("usr/bin", &mut relative), Ok(()));
        assert_eq!(&relative.seen[0], ".");
    }

    #[test]
    fn a_failed_probe_falls_back_to_name_max_while_minus_one_means_no_limit() {
        let long_component = "a".repeat(512);
        let probe_fails = Fake {
            name_maxes: vec![FAILS, FAILS],
            ..Fake::default()
        };
        let mut probe = probe_fails;
        assert_eq!(
            check_system(&long_component, &mut probe),
            Err(SystemFault::ComponentTooLong {
                component: &long_component,
                limit: SYSTEM_NAME_MAX,
            })
        );
        // The same operand against a system that answers "no limit" is fine, and so
        // is one exactly at the fallback limit.
        let mut unlimited = Fake {
            name_maxes: vec![-1],
            ..Fake::default()
        };
        assert_eq!(check_system(&long_component, &mut unlimited), Ok(()));
        let mut at_limit = Fake {
            name_maxes: vec![511],
            ..Fake::default()
        };
        assert_eq!(check_system(&"a".repeat(511), &mut at_limit), Ok(()));
    }

    #[test]
    fn the_limit_in_force_is_the_one_for_the_directory_just_crossed() {
        // `svnamemax` at pathchk.c:141-144: a successful reprobe replaces the answer,
        // a failed one keeps the previous value rather than resetting it.
        let mut widening = Fake {
            name_maxes: vec![3, 30, 30],
            ..Fake::default()
        };
        assert_eq!(check_system("abc/abcdefghijkl", &mut widening), Ok(()));
        assert_eq!(&widening.seen[1], "abc");
        let mut stuck = Fake {
            name_maxes: vec![3, FAILS, 3],
            ..Fake::default()
        };
        assert_eq!(
            check_system("abc/abcdefghijkl", &mut stuck),
            Err(SystemFault::ComponentTooLong {
                component: "abcdefghijkl",
                limit: 3,
            })
        );
    }

    #[test]
    fn a_stat_failure_reports_the_prefix_c_would_print() {
        // The reference binary answers `/etc/hostname/x` with
        // `pathchk: /etc/hostname/x: /etc/hostname: Not a directory`: the printed
        // prefix is `strlen(pathd) - complen - 1` bytes, i.e. up to the component
        // minus one more (pathchk.c:125-127).
        let mut probe = Fake {
            name_maxes: vec![-1, -1, -1],
            broken: vec![("/etc/hostname/x", ENOTDIR)],
            ..Fake::default()
        };
        assert_eq!(
            check_system("/etc/hostname/x", &mut probe),
            Err(SystemFault::StatFailed {
                prefix: "/etc/hostname",
                whole: false,
                errno: minix_sys::Errno::from_i32(ENOTDIR),
            })
        );
    }

    #[test]
    fn a_missing_file_is_not_a_failure_at_all() {
        // `errno != ENOENT` is the test (pathchk.c:124); every component of a deep,
        // absent path is therefore waved through, and only the limits remain.
        let mut probe = Fake {
            name_maxes: vec![-1, -1, -1],
            missing: vec!["/does", "/does/not", "/does/not/exist"],
            ..Fake::default()
        };
        assert_eq!(check_system("/does/not/exist", &mut probe), Ok(()));
    }

    #[test]
    fn the_path_limit_is_asked_last_and_reported_one_lower() {
        let path = "/".to_string() + &"ab/".repeat(512); // far past 1024 bytes
        assert!(path.len() > SYSTEM_PATH_MAX as usize);
        let mut probed = Fake {
            name_maxes: vec![-1; 600],
            path_max: SYSTEM_PATH_MAX,
            ..Fake::default()
        };
        assert_eq!(
            check_system(&path, &mut probed),
            Err(SystemFault::PathTooLong {
                limit: SYSTEM_PATH_MAX - 1
            })
        );
        // An unlimited answer switches the check off; a failed probe takes the
        // documented PATH_MAX fallback instead.
        let mut unlimited = Fake {
            name_maxes: vec![-1; 600],
            path_max: -1,
            ..Fake::default()
        };
        assert_eq!(check_system(&path, &mut unlimited), Ok(()));
        let mut failed = Fake {
            name_maxes: vec![-1; 600],
            path_max: FAILS,
            ..Fake::default()
        };
        assert_eq!(
            check_system(&path, &mut failed),
            Err(SystemFault::PathTooLong {
                limit: SYSTEM_PATH_MAX - 1
            })
        );
    }
}
