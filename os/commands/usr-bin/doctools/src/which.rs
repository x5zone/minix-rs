//! `which` — locate a command on the search path, the pure argument layer.
//!
//! Ground truth: `minix3/usr.bin/whereis/whereis.c` (NetBSD 1.21). That one
//! binary is both `whereis` and `which`, choosing its personality by
//! `strcmp(getprogname(), "which")` at line 66. This module owns the deciding
//! half shared by both: the option scan (`getopt(argc, argv, "ap")`,
//! `whereis.c:78-92`), the search-path splitting that turns one `$PATH`-style
//! string into candidate file paths (`whereis.c:126-136`), the absolute-name
//! test (`whereis.c:115`), and the exit-status rule (`whereis.c:157`). The
//! `stat`/`access` syscalls and the actual printing live in the thin bin
//! `src/bin/which.rs`.
//!
//! The faithful corner is the empty-path-element handling: `whereis.c:128-134`
//! replaces an empty element (a leading, doubled, or trailing `:`) with `.` so
//! that `:foo` searches the current directory, then builds `"%s/%s"`. So every
//! element — empty or not — yields `element/name`, with `""` folded to `"./name"`.

use alloc::string::String;
use alloc::vec::Vec;

/// The resolved option set of `which`/`whereis`, from `whereis.c:78-92`.
/// `-a` clears the `which` personality (show every match, not just the first);
/// `-p` selects the environment `$PATH` as the search source. Under the
/// `which` personality `-p` is already the default (C sets `useenvpath = which`
/// at line 67), so it is accepted but a no-op here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WhichFlags {
    /// `-a`: report every matching file across the path, not just the first.
    pub show_all: bool,
    /// `-p`: use the environment `PATH` (default under `which`).
    pub use_env_path: bool,
}

impl WhichFlags {
    /// Parse the option string `"ap"`. Returns the flags and the operand slice
    /// (everything after option processing), or `None` on an unknown option so
    /// the bin prints usage and exits 1. Classic BSD getopt: no permutation —
    /// the first non-option ends the scan, and `--` ends it too.
    pub fn parse<'a>(args: &[&'a str]) -> Option<(WhichFlags, Vec<&'a str>)> {
        let mut f = WhichFlags::default();
        let mut i = 0;
        let mut done = false;
        while i < args.len() && !done {
            let a = args[i];
            if a == "--" {
                done = true;
                i += 1;
                continue;
            }
            if a.len() >= 2 && a.starts_with('-') {
                for &byte in a.as_bytes()[1..].iter() {
                    match byte {
                        b'a' => f.show_all = true,
                        b'p' => f.use_env_path = true,
                        _ => return None,
                    }
                }
                i += 1;
                continue;
            }
            break; // first non-option ends the option scan
        }
        Some((f, args[i..].to_vec()))
    }
}

/// Whether an operand is an absolute path (checked directly, not searched) —
/// C's `if (**argv == '/')` at `whereis.c:115`.
pub fn is_absolute(name: &str) -> bool {
    name.starts_with('/')
}

/// Split a `:`-separated search path into candidate file paths for `name`,
/// reproducing `whereis.c:126-136`: an empty element (leading, doubled, or
/// trailing `:`) becomes `.`, and each element `dir` contributes `dir/name`
/// exactly as C's `snprintf("%s/%s", dir, name)` does — so element `/` yields
/// `//name` and empty yields `./name`.
pub fn path_candidates(search_path: &str, name: &str) -> Vec<String> {
    search_path
        .split(':')
        .map(|dir| {
            let dir = if dir.is_empty() { "." } else { dir };
            String::from(dir) + "/" + name
        })
        .collect()
}

/// The exit status of `whereis.c:157`: `3` when nothing was found, `0` when
/// every operand produced at least one match (`found >= argc`), else `2`.
/// `found` counts printed matches; `argc` is the operand count.
pub fn compute_status(found: usize, argc: usize) -> i32 {
    if found == 0 {
        3
    } else if found >= argc {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_parse_known_letters() {
        let (f, rest) = WhichFlags::parse(&["-a", "-p", "ls"]).unwrap();
        assert!(f.show_all && f.use_env_path);
        assert_eq!(rest, ["ls"]);
    }

    #[test]
    fn combined_cluster_flags() {
        let (f, rest) = WhichFlags::parse(&["-ap", "ls", "cp"]).unwrap();
        assert!(f.show_all && f.use_env_path);
        assert_eq!(rest, ["ls", "cp"]);
    }

    #[test]
    fn unknown_option_is_error() {
        assert!(WhichFlags::parse(&["-z"]).is_none());
    }

    #[test]
    fn double_dash_ends_options() {
        let (f, rest) = WhichFlags::parse(&["--", "-a"]).unwrap();
        assert!(!f.show_all, "-a after -- is an operand, not a flag");
        assert_eq!(rest, ["-a"]);
    }

    #[test]
    fn first_operand_ends_option_scan() {
        // No permutation: a flag after the first operand is an operand.
        let (f, rest) = WhichFlags::parse(&["ls", "-a"]).unwrap();
        assert!(!f.show_all);
        assert_eq!(rest, ["ls", "-a"]);
    }

    #[test]
    fn candidates_basic() {
        let c = path_candidates("/bin:/usr/bin", "ls");
        assert_eq!(c, ["/bin/ls", "/usr/bin/ls"]);
    }

    #[test]
    fn candidates_empty_element_becomes_dot_dir() {
        // Leading, doubled, and trailing ':' each fold to ".".
        assert_eq!(path_candidates(":a:b:", "x"), ["./x", "a/x", "b/x", "./x"]);
        assert_eq!(path_candidates("a::b", "x"), ["a/x", "./x", "b/x"]);
    }

    #[test]
    fn candidates_single_and_empty() {
        assert_eq!(path_candidates("/bin", "ls"), ["/bin/ls"]);
        // An entirely empty path is one empty element -> "./name".
        assert_eq!(path_candidates("", "ls"), ["./ls"]);
    }

    #[test]
    fn candidates_keep_element_slash_like_c() {
        // C builds "%s/%s" verbatim, so element "/" yields "//name".
        assert_eq!(path_candidates("/", "ls"), ["//ls"]);
    }

    #[test]
    fn absolute_detection() {
        assert!(is_absolute("/usr/bin/ls"));
        assert!(!is_absolute("ls"));
        assert!(!is_absolute("./ls"));
    }

    #[test]
    fn status_rule() {
        assert_eq!(compute_status(0, 3), 3); // nothing found
        assert_eq!(compute_status(3, 3), 0); // every operand matched
        assert_eq!(compute_status(5, 3), 0); // -a can exceed argc, still 0
        assert_eq!(compute_status(1, 3), 2); // some operands missing
        assert_eq!(compute_status(2, 3), 2);
    }
}
