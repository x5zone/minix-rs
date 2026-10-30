//! `realpath(3)` port: folds `.`, `..` and symbolic links in a path segment
//! by segment into an absolute path.
//!
//! The ground truth is `minix3/lib/libc/gen/getcwd.c:64-215` (a BSD-family
//! implementation; `realpath` and `getcwd` live in the same file). This layer
//! collects everything that leg needs into one trait [`PathProbe`] (`lstat`,
//! `readlink`, `getcwd` — three actions), so the folding algorithm itself can
//! be tested branch by branch on the host; the real system-call side
//! implements the trait in the thin shell.
//!
//! Three semantics worth stating up front; all are C's shapes, not inventions
//! of this layer:
//! - **A relative path first gets the cwd prefixed** (`getcwd.c:112-119`). C
//!   does not give up when fetching the cwd fails; it writes `resolved` as `"."`
//!   and continues — the error is only returned on the `goto out` branch.
//! - **An absolute symlink target restarts everything** (`getcwd.c:203-205`:
//!   `p = resolved`), discarding all accumulated segments; a relative target is
//!   appended after the current segment.
//! - **The last segment may not exist**: only a failing `lstat` is an error, and
//!   a regular file returns after its last segment (`getcwd.c:174-176` puts the
//!   failure exit on `lstat`), while "an intermediate segment is not a
//!   directory" has its own `ENOTDIR` branch (`getcwd.c:207-209`, with the
//!   criterion that a `/` follows the segment).

use alloc::string::String;

/// `PATH_MAX` from `minix3/include/limits.h`; this tree already fixed the
/// on-the-wire transfer limit at the same value in
/// `os/libs/minix-sys/src/lib.rs:269-270`, and this reuses it.
pub const MAXPATHLEN: usize = 1024;

/// `MAXSYMLINKS` from `minix3/sys/sys/param.h:358`.
pub const MAXSYMLINKS: u32 = 32;

/// Why folding failed. The names follow the `errno` C sets before `goto out`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealPathError {
    /// Empty path (`getcwd.c:104-107`).
    NoEntry,
    /// The accumulated path exceeds `MAXPATHLEN` (`getcwd.c:159-165`).
    NameTooLong,
    /// Symlink depth exceeds `MAXSYMLINKS` (`getcwd.c:180-183`).
    Loop,
    /// An intermediate segment is not a directory and more content follows
    /// (`getcwd.c:207-209`).
    NotDir,
    /// `lstat` failed. C returns that errno directly; this layer keeps the
    /// error code so the caller can report it only in a warning, as C does.
    StatFailed,
    /// `readlink` failed (`getcwd.c:185-191`; a return of 0 is treated as
    /// `ENOENT` too).
    ReadLinkFailed,
    /// Relative path, but the cwd cannot be obtained (`getcwd.c:112-118`).
    NoWorkingDirectory,
}

/// What folding a path must ask the file system.
pub trait PathProbe {
    /// `lstat`'s `st_mode`; `None` on failure.
    fn lstat_mode(&self, path: &str) -> Option<u32>;
    /// `readlink`'s content; `None` on failure or an empty string (C also
    /// treats `n == 0` as an error).
    fn read_link(&self, path: &str) -> Option<String>;
    /// The current working directory.
    fn working_directory(&self) -> Option<String>;
}

/// The mode bits needed by the `S_ISLNK`/`S_IFDIR`/`S_IFMT` checks,
/// `minix3/sys/sys/stat.h:138-158` (this tree already defined the same set of
/// values in `cp.rs:29-36`).
const S_IFMT: u32 = 0o170_000;
const S_IFLNK: u32 = 0o120_000;
const S_IFDIR: u32 = 0o040_000;

/// Fold `path` segment by segment. Semantics match `realpath(3)`: an absolute
/// path starts at the root and a relative one at the cwd; `.` is dropped, `..`
/// backs up one segment, and a symlink splices its target before the unresolved
/// part.
pub fn real_path<T: PathProbe + ?Sized>(probe: &T, path: &str) -> Result<String, RealPathError> {
    if path.is_empty() {
        // `getcwd.c:104-107`: an empty string is `ENOENT`, not `EINVAL`.
        return Err(RealPathError::NoEntry);
    }

    let mut resolved = String::new();
    if !path.starts_with('/') {
        let cwd = probe
            .working_directory()
            .ok_or(RealPathError::NoWorkingDirectory)?;
        resolved.push_str(cwd.trim_end_matches('/'));
    }

    let mut unresolved = String::from(path);
    let mut links_seen = 0u32;
    // C alternates between two buffers holding "link target + remaining
    // path"; this layer swaps Strings instead, with the same effect: the
    // resolved target is spliced before the unresolved part and the loop
    // continues.
    loop {
        while unresolved.starts_with('/') {
            unresolved.remove(0);
        }
        if unresolved.is_empty() {
            if resolved.is_empty() {
                resolved.push('/');
            }
            return Ok(resolved);
        }
        let head_end = unresolved.find('/').unwrap_or(unresolved.len());
        let component = &unresolved[..head_end];
        let rest = &unresolved[head_end..];

        if component == "." {
            unresolved = String::from(rest);
            continue;
        }
        if component == ".." {
            trim_last(&mut resolved);
            unresolved = String::from(rest);
            continue;
        }

        let next_len = resolved.len() + 1 + component.len() + 1;
        if next_len > MAXPATHLEN {
            return Err(RealPathError::NameTooLong);
        }
        // In C, `p` stops on the `/` just written, and the link branch
        // overwrites the link name by not advancing it.
        let prefix_len = resolved.len();
        let mut candidate = resolved.clone();
        candidate.push('/');
        candidate.push_str(component);

        let mode = probe
            .lstat_mode(&candidate)
            .ok_or(RealPathError::StatFailed)?;
        if mode & S_IFMT == S_IFLNK {
            links_seen += 1;
            if links_seen > MAXSYMLINKS {
                return Err(RealPathError::Loop);
            }
            let mut target = probe
                .read_link(&candidate)
                .ok_or(RealPathError::ReadLinkFailed)?;
            if target.is_empty() {
                return Err(RealPathError::NoEntry);
            }
            // "Splice the unresolved part after the link target, then switch
            // to it" — `getcwd.c:193-200`.
            target.push_str(rest);
            unresolved = target;
            // C does not advance `p` on the link branch (`getcwd.c:174-206`
            // finishes and does `goto loop`; the advancing line is at `:212`),
            // so the next segment **overwrites** the link name itself. An
            // absolute target also pulls `p` back to the start of the string
            // (`getcwd.c:203-205`), discarding the whole prefix.
            resolved.truncate(prefix_len);
            if unresolved.starts_with('/') {
                resolved = String::new();
            }
            continue;
        }
        // If content follows this segment and it is not a directory, that is
        // `ENOTDIR` (`getcwd.c:207-209`).
        if !rest.is_empty() && mode & S_IFMT != S_IFDIR {
            return Err(RealPathError::NotDir);
        }
        resolved = candidate;
        unresolved = String::from(rest);
    }
}

/// Drop the last segment. C writes `while (*--p != '/')` (`getcwd.c:150-152`),
/// i.e. at least one `/` must remain at the root; at the root already
/// (`p == resolved`) it does not move.
fn trim_last(resolved: &mut String) {
    if resolved.is_empty() {
        return;
    }
    match resolved.rfind('/') {
        // Nothing to back out above the root: `/..` is still `/`.
        Some(0) => resolved.clear(),
        Some(at) => {
            resolved.truncate(at);
        }
        // Only absolute paths carrying a segment in the cwd reach here; a
        // relative start does not back out.
        None => resolved.clear(),
    }
}

/// A minimal file-system view used only in host tests: path -> (mode bits,
/// link target).
#[cfg(test)]
pub(crate) struct Fixture {
    pub entries: Vec<(String, u32, Option<String>)>,
    pub cwd: String,
}

#[cfg(test)]
impl PathProbe for Fixture {
    fn lstat_mode(&self, path: &str) -> Option<u32> {
        self.entries
            .iter()
            .find(|(p, _, _)| p == path)
            .map(|(_, mode, _)| *mode)
    }

    fn read_link(&self, path: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|(p, _, _)| p == path)
            .and_then(|(_, _, target)| target.clone())
    }

    fn working_directory(&self) -> Option<String> {
        Some(self.cwd.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const REG: u32 = 0o100_644;
    const DIR: u32 = S_IFDIR | 0o755;
    const LINK: u32 = S_IFLNK | 0o777;

    fn fixture(cwd: &str, entries: &[(&str, u32, Option<&str>)]) -> Fixture {
        let mut owned = Vec::new();
        for (path, mode, target) in entries {
            owned.push((String::from(*path), *mode, (*target).map(String::from)));
        }
        Fixture {
            entries: owned,
            cwd: String::from(cwd),
        }
    }

    #[test]
    fn an_absolute_path_of_plain_directories_comes_back_cleaned() {
        let f = fixture("/", &[("/a", DIR, None), ("/a/b", DIR, None)]);
        assert_eq!(real_path(&f, "/a/b"), Ok(String::from("/a/b")));
        // Repeated slashes and a trailing slash are both removed
        // (`getcwd.c:133-135`, `:137-141`).
        assert_eq!(real_path(&f, "///a//b/"), Ok(String::from("/a/b")));
        assert_eq!(real_path(&f, "/a/b/"), Ok(String::from("/a/b")));
    }

    #[test]
    fn dot_and_dotdot_are_folded_without_asking_the_file_system() {
        let f = fixture("/", &[("/a", DIR, None), ("/a/b", DIR, None)]);
        assert_eq!(real_path(&f, "/a/./b"), Ok(String::from("/a/b")));
        assert_eq!(real_path(&f, "/a/b/../b"), Ok(String::from("/a/b")));
        // Nothing to back out above the root.
        assert_eq!(real_path(&f, "/../../a"), Ok(String::from("/a")));
    }

    #[test]
    fn a_missing_component_is_the_stat_failure() {
        let f = fixture("/", &[("/a", DIR, None)]);
        assert_eq!(real_path(&f, "/a/nope"), Err(RealPathError::StatFailed));
        // An empty string takes the ENOENT branch (`getcwd.c:104-107`).
        assert_eq!(real_path(&f, ""), Err(RealPathError::NoEntry));
    }

    #[test]
    fn a_relative_path_starts_from_the_working_directory() {
        let f = fixture("/tmp", &[("/tmp", DIR, None), ("/tmp/x", REG, None)]);
        assert_eq!(real_path(&f, "x"), Ok(String::from("/tmp/x")));
        assert_eq!(real_path(&f, "./x"), Ok(String::from("/tmp/x")));
    }

    #[test]
    fn an_absolute_symlink_restarts_the_accumulated_prefix() {
        // `/l -> /a/b`: jumping to the absolute target discards the already
        // accumulated `/x` entirely.
        let f = fixture(
            "/",
            &[
                ("/x", DIR, None),
                ("/x/l", LINK, Some("/a/b")),
                ("/a", DIR, None),
                ("/a/b", DIR, None),
            ],
        );
        assert_eq!(real_path(&f, "/x/l"), Ok(String::from("/a/b")));
    }

    #[test]
    fn a_relative_symlink_lands_inside_its_own_directory() {
        // The relative target overwrites the link name itself: C does not
        // advance `p` on the link branch (see the note inside `real_path`), so
        // `/d/l` with `l -> e` answers `/d/e`, not `/d/l/e`.
        let f = fixture(
            "/",
            &[
                ("/d", DIR, None),
                ("/d/l", LINK, Some("e")),
                ("/d/e", REG, None),
            ],
        );
        assert_eq!(real_path(&f, "/d/l"), Ok(String::from("/d/e")));
    }

    #[test]
    fn a_symlink_chain_is_followed_to_the_end() {
        let f = fixture(
            "/",
            &[
                ("/l1", LINK, Some("/l2")),
                ("/l2", LINK, Some("/l3")),
                ("/l3", REG, None),
            ],
        );
        assert_eq!(real_path(&f, "/l1"), Ok(String::from("/l3")));
    }

    #[test]
    fn too_many_symlink_hops_is_a_loop() {
        let mut entries = Vec::new();
        for i in 0..=MAXSYMLINKS {
            entries.push((
                alloc::format!("/l{}", i),
                LINK,
                Some(alloc::format!("/l{}", i + 1)),
            ));
        }
        entries.push((alloc::format!("/l{}", MAXSYMLINKS + 1), REG, None));
        let f = Fixture {
            entries,
            cwd: String::from("/"),
        };
        // The loop is only detected on hop MAXSYMLINKS+1 (C's
        // `nlnk++ >= MAXSYMLINKS`, `getcwd.c:180-183`).
        assert_eq!(real_path(&f, "/l0"), Err(RealPathError::Loop));
    }

    #[test]
    fn a_non_directory_in_the_middle_is_enotdir() {
        let f = fixture("/", &[("/f", REG, None)]);
        assert_eq!(real_path(&f, "/f/sub"), Err(RealPathError::NotDir));
        // A regular last segment is legitimate (`getcwd.c:207`'s criterion
        // includes "a `/` follows").
        assert_eq!(real_path(&f, "/f"), Ok(String::from("/f")));
    }

    #[test]
    fn a_path_beyond_the_buffer_is_enametooolong() {
        let long = alloc::format!("/{}", "a".repeat(MAXPATHLEN));
        let f = fixture("/", &[]);
        assert_eq!(real_path(&f, &long), Err(RealPathError::NameTooLong));
    }

    #[test]
    fn a_broken_link_target_reports_through_the_readlink_leg() {
        // `lstat` says it is a link but `readlink` yields no content: both
        // error legs must be reportable.
        let f = Fixture {
            entries: alloc::vec![
                (String::from("/l"), LINK, None),
                (String::from("/e"), LINK, Some(String::new())),
            ],
            cwd: String::from("/"),
        };
        assert_eq!(real_path(&f, "/l"), Err(RealPathError::ReadLinkFailed));
        assert_eq!(real_path(&f, "/e"), Err(RealPathError::NoEntry));
    }

    #[test]
    fn the_root_alone_resolves_to_itself() {
        let f = fixture("/", &[("/", DIR, None)]);
        assert_eq!(real_path(&f, "/"), Ok(String::from("/")));
        assert_eq!(real_path(&f, "//"), Ok(String::from("/")));
        assert_eq!(real_path(&f, "/.."), Ok(String::from("/")));
    }
}
