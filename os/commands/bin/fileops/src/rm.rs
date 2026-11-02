//! `rm` — file removal, the pure argument layer.
//!
//! Ground truth: `minix3/bin/rm/rm.c` (BSD, NetBSD 1.53). This module owns
//! the deciding half — the option set, the `NONEXISTENT` errno triage behind
//! `-f`, and the POSIX `.`/`..` operand rejection — mirroring how `rm.c`
//! separates its argument bookkeeping (`main`'s `getopt` loop at lines 108 to
//! 142, `checkdot` at lines 589 to 618, the `NONEXISTENT` macro at lines 89 to
//! 90) from the filesystem syscalls (`rm_file`'s `lstat`/`unlink`/`rmdir`),
//! which live in the thin bin `src/bin/rm.rs`.
//!
//! Only the non-recursive default path is wired. The recursive hierarchy walk
//! (`rm_tree`, `rm.c:167-287`) hinges on `fts`(3), the secure overwrite
//! (`rm_overwrite`, `rm.c:410-539`) on `arc4random`/`O_NOFOLLOW`, and whiteout
//! removal (`-W`, `undelete`) on facilities not wrapped in this tree — those
//! are declared residuals the bin refuses up front, exactly as `chmod` treats
//! `-R`/`-h` (`bin/chmod.rs`), not a semantics choice.

use alloc::string::String;
use alloc::vec::Vec;
use minix_sys::Errno;

/// The resolved option set of `rm`, mirroring the file-scope globals of
/// `rm.c:63-64` (`dflag`, `fflag`, `iflag`, `Pflag`, `rflag`, `vflag`,
/// `Wflag`, `xflag`). `eval` (the running exit status) is not a flag and is
/// tracked by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RmFlags {
    /// `-d`: allow removal of the directory entry itself (via `rmdir`).
    pub dflag: bool,
    /// `-f`: ignore nonexistent operands and force removal (clears `-i`).
    pub force: bool,
    /// `-i`: prompt before every removal (clears `-f`).
    pub interactive: bool,
    /// `-P`: overwrite before unlinking (`rm_overwrite`, `rm.c:361-451` —
    /// the DSS-matrix three-pass pattern with a verify leg).
    pub overwrite: bool,
    /// `-r`/`-R`: recursive hierarchy walk (`rm.c:167-281` `rm_tree`), wired
    /// through this crate's own directory decoder rather than fts(3).
    pub recursive: bool,
    /// `-v`: print each removed operand.
    pub verbose: bool,
    /// `-W`: remove whiteouts (`undelete`; not wired).
    pub whiteout: bool,
    /// `-x`: do not cross mount points during the walk (`FTS_XDEV`,
    /// `rm.c:193-195`). The mount-point directory itself is never visited
    /// with its cross-device children, so the walk skips it silently and the
    /// parent's `rmdir` answers ENOTEMPTY, exactly as fts pruning leaves it.
    pub cross_fs: bool,
}

impl RmFlags {
    /// Parse the option string `"dfiPRrvWx"` from the leading args. Returns the
    /// flags and the operand slice (everything after option processing), or
    /// `None` on an unknown option so the bin can print usage and exit 1.
    ///
    /// Faithful corners from `rm.c:108-140`: `-f` sets `force` and clears
    /// `interactive`; `-i` sets `interactive` and clears `force` (so the last
    /// of `-f`/`-i` wins, per `case 'f'`/`case 'i'`); `-R` and `-r` both set
    /// `recursive`. After `--`, or at the first non-option, the remainder are
    /// operands (this is the classic BSD getopt, no argument permutation).
    pub fn parse<'a>(args: &[&'a str]) -> Option<(RmFlags, Vec<&'a str>)> {
        let mut f = RmFlags::default();
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
                        b'd' => f.dflag = true,
                        b'f' => {
                            f.force = true;
                            f.interactive = false;
                        }
                        b'i' => {
                            f.interactive = true;
                            f.force = false;
                        }
                        b'P' => f.overwrite = true,
                        b'R' | b'r' => f.recursive = true,
                        b'v' => f.verbose = true,
                        b'W' => f.whiteout = true,
                        b'x' => f.cross_fs = true,
                        _ => return None,
                    }
                }
                i += 1;
                continue;
            }
            // First non-option ends option processing (no permutation).
            break;
        }
        // Everything from `i` on (after `--` or the first operand) is operands.
        Some((f, args[i..].to_vec()))
    }

    /// Whether any requested option still needs an unwrapped facility: `-P`
    /// overwrite (`arc4random` plus `O_NOFOLLOW`), `-W` whiteout `undelete`,
    /// `-x` cross-device pruning (`FTS_XDEV`). The bin refuses those up front
    /// rather than silently doing less than the flags promise. The recursive
    /// walk itself is now wired (the bin's `rm_tree`, walking with
    /// `crate::dents`), so `-r`/`-R` no longer count here.
    pub fn needs_unwired(&self) -> bool {
        // `-P` and `-x` left this set (the overwrite pass and the device
        // boundary); `-W` (whiteout `undelete`) is the only one left.
        self.whiteout
    }
}

/// C `rm.c:89-90` `NONEXISTENT(x)`: `(x) == ENOENT || (x) == ENAMETOOLONG ||
/// (x) == ENOTDIR`. `-f` suppresses the diagnostic for an operand whose
/// removal failed *because the file (or a path component) was not there*;
/// every other error is still reported.
pub fn nonexistent(e: Errno) -> bool {
    e == Errno::ENOENT || e == Errno::ENAMETOOLONG || e == Errno::ENOTDIR
}

/// Which question C's `check` (`rm.c:541-578`) asks, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckFlavor {
    /// Nothing is asked: the operand is removed straight away (`rm.c:557-560`
    /// returns "proceed" — its `1` is the caller's "yes, go ahead").
    None,
    /// `-i`: `remove '<path>'? ` (`rm.c:551-552`).
    Remove,
    /// No `-i`, but a terminal, a non-symlink whose write access is genuinely
    /// denied: `override <perms><sep><user>:<group> for '<path>'? `
    /// (`rm.c:561-572`).
    Override,
}

/// The ask decision of `rm.c:551-560`.
///
/// The comment there is explicit about the exclusions: a symbolic link is not
/// asked about "because their permissions are meaningless", a non-terminal
/// standard input never asks, and the `access(name, W_OK)` test treats
/// `ETXTBSY` (a running binary: writable but in use) as "not a permission
/// problem", so it does not ask either. `unwritable` is the truth value of
/// `access(...)` returning `-1`; `busy` says that failure was `ETXTBSY`.
pub fn check_flavor(
    interactive: bool,
    stdin_is_tty: bool,
    is_symlink: bool,
    unwritable: bool,
    busy: bool,
) -> CheckFlavor {
    if interactive {
        return CheckFlavor::Remove;
    }
    if !stdin_is_tty || is_symlink || !unwritable || busy {
        return CheckFlavor::None;
    }
    CheckFlavor::Override
}

/// C `rm.c:588` `ISDOT(a)`: `(a)[0] == '.' && (!(a)[1] || ((a)[1] == '.' &&
/// !(a)[2]))` — the basename is exactly `"."` or exactly `".."`.
pub fn is_dot(name: &str) -> bool {
    name == "." || name == ".."
}

/// C `checkdot` (`rm.c:589-618`): the POSIX requirement that an operand whose
/// (trailing-slash-stripped) basename is `"."` or `".."` is diagnosed and
/// dropped rather than removed. Returns the surviving operands, each with
/// trailing slashes stripped (C overwrites them in place), and whether any
/// operand was rejected (the running `eval` becomes 1). The `"."`/`".."`
/// diagnostic itself is printed once by the caller (C's `complained` guard).
///
/// Trailing-slash stripping mirrors `while (--p > *t && *p == '/') *p = '\0'`
/// (`rm.c:598-600`): it never empties the string, so `"/"` and `"//"` survive
/// to the removal stage (where they become "is a directory").
pub fn strip_dots(operands: &[&str]) -> (Vec<String>, bool) {
    let mut kept: Vec<String> = Vec::new();
    let mut eval = false;
    for &raw in operands {
        // Strip trailing slashes, keeping at least one character.
        let mut s = String::from(raw);
        while s.len() > 1 && s.ends_with('/') {
            s.pop();
        }
        // Extract the basename after the last remaining '/'.
        let base = match s.rfind('/') {
            Some(idx) => &s[idx + 1..],
            None => s.as_str(),
        };
        if is_dot(base) {
            eval = true;
            continue;
        }
        kept.push(s);
    }
    (kept, eval)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_parse_known_letters() {
        let (f, rest) = RmFlags::parse(&["-d", "-f", "-v", "a", "b"]).unwrap();
        assert!(f.dflag && f.force && f.verbose);
        assert!(!f.interactive, "-f clears -i");
        assert_eq!(rest, ["a", "b"]);
    }

    #[test]
    fn combined_cluster_flags() {
        let (f, rest) = RmFlags::parse(&["-dfv", "x"]).unwrap();
        assert!(f.dflag && f.force && f.verbose);
        assert_eq!(rest, ["x"]);
    }

    #[test]
    fn f_and_i_last_wins_both_ways() {
        // rm.c `case 'f'`/`case 'i'`: whichever comes last clears the other.
        let (f, _) = RmFlags::parse(&["-f", "-i"]).unwrap();
        assert!(f.interactive && !f.force);
        let (f, _) = RmFlags::parse(&["-i", "-f"]).unwrap();
        assert!(f.force && !f.interactive);
    }

    #[test]
    fn r_lower_and_upper_both_set_recursive() {
        assert!(RmFlags::parse(&["-r"]).unwrap().0.recursive);
        assert!(RmFlags::parse(&["-R"]).unwrap().0.recursive);
    }

    #[test]
    fn unwired_flags_flagged() {
        assert!(RmFlags::parse(&["-W", "f"]).unwrap().0.needs_unwired());
        // `-x` left the unwired set when the walk grew the device boundary.
        assert!(!RmFlags::parse(&["-x", "f"]).unwrap().0.needs_unwired());
        assert!(!RmFlags::parse(&["-P", "f"]).unwrap().0.needs_unwired());
        // -r/-R are wired since the recursive walk stopped needing fts(3):
        // the bin's `rm_tree` walks directories with `crate::dents` instead.
        assert!(!RmFlags::parse(&["-r", "f"]).unwrap().0.needs_unwired());
        assert!(!RmFlags::parse(&["-R", "f"]).unwrap().0.needs_unwired());
        // The wired default subset needs nothing unwired.
        let wired = RmFlags::parse(&["-f", "-d", "-v", "f"]).unwrap().0;
        assert!(!wired.needs_unwired());
    }

    #[test]
    fn unknown_option_is_error() {
        assert!(RmFlags::parse(&["-z"]).is_none());
    }

    #[test]
    fn double_dash_ends_options() {
        let (_, rest) = RmFlags::parse(&["--", "-notanoption"]).unwrap();
        assert_eq!(rest, ["-notanoption"]);
    }

    #[test]
    fn first_operand_ends_option_scan() {
        // No permutation: a flag after the first operand is an operand.
        let (f, rest) = RmFlags::parse(&["a", "-f"]).unwrap();
        assert!(!f.force);
        assert_eq!(rest, ["a", "-f"]);
    }

    #[test]
    fn nonexistent_triage() {
        assert!(nonexistent(Errno::ENOENT));
        assert!(nonexistent(Errno::ENAMETOOLONG));
        assert!(nonexistent(Errno::ENOTDIR));
        // A permission failure is real and must still be reported under -f.
        assert!(!nonexistent(Errno::EPERM));
        assert!(!nonexistent(Errno::EINVAL));
    }

    #[test]
    fn check_flavor_minus_i_wins_without_consulting_anything() {
        // rm.c:551: "Check -i first" — the terminal, the type and the access
        // bits are not even looked at.
        assert_eq!(
            check_flavor(true, false, false, false, false),
            CheckFlavor::Remove
        );
        assert_eq!(
            check_flavor(true, true, true, true, true),
            CheckFlavor::Remove
        );
    }

    #[test]
    fn check_flavor_needs_terminal_writable_and_type() {
        // The one shape that asks: rm.c:557-560 all three guards pass.
        assert_eq!(
            check_flavor(false, true, false, true, false),
            CheckFlavor::Override,
            "a terminal, a non-symlink, and access(W_OK) denied"
        );
        assert_eq!(
            check_flavor(false, false, false, true, false),
            CheckFlavor::None,
            "!stdin_ok"
        );
        assert_eq!(
            check_flavor(false, true, true, true, false),
            CheckFlavor::None,
            "symlink: mode is meaningless"
        );
        assert_eq!(
            check_flavor(false, true, false, false, false),
            CheckFlavor::None,
            "writable"
        );
        assert_eq!(
            check_flavor(false, true, false, true, true),
            CheckFlavor::None,
            "ETXTBSY is a denial, but not a permission one (rm.c:558)"
        );
    }

    #[test]
    fn is_dot_exact() {
        assert!(is_dot("."));
        assert!(is_dot(".."));
        assert!(!is_dot("..."));
        assert!(!is_dot("a"));
        assert!(!is_dot(""));
        assert!(!is_dot("./"));
    }

    #[test]
    fn strip_dots_rejects_dot_and_dotdot() {
        assert_eq!(strip_dots(&["."]), (Vec::new(), true));
        assert_eq!(strip_dots(&[".."]), (Vec::new(), true));
        let (kept, eval) = strip_dots(&["a", ".", "b", ".."]);
        assert_eq!(kept, ["a", "b"]);
        assert!(eval);
    }

    #[test]
    fn strip_dots_basenames_through_dirs() {
        // "./." and "dir/.." have "." / ".." as basename → rejected.
        assert_eq!(strip_dots(&["./."]), (Vec::new(), true));
        assert_eq!(strip_dots(&["dir/.."]), (Vec::new(), true));
        // ".." with trailing slashes: strip them, basename ".." → rejected.
        assert_eq!(strip_dots(&["..//"]), (Vec::new(), true));
    }

    #[test]
    fn strip_dots_strips_trailing_slash_but_keeps_root() {
        // Trailing slashes stripped on surviving operands (C overwrites in place).
        assert_eq!(strip_dots(&["a/"]), (vec![String::from("a")], false));
        assert_eq!(strip_dots(&["foo//"]), (vec![String::from("foo")], false));
        // "/" and "//" never empty to nothing: they survive to "is a directory".
        assert_eq!(strip_dots(&["/"]), (vec![String::from("/")], false));
    }

    #[test]
    fn strip_dots_keeps_plain_operands_in_order() {
        let (kept, eval) = strip_dots(&["x", "y", "z"]);
        assert_eq!(kept, ["x", "y", "z"]);
        assert!(!eval);
    }
}
