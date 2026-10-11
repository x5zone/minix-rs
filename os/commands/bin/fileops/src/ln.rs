//! `ln` — hard/symbolic link creation, the pure argument layer.
//!
//! Ground truth: `minix3/bin/ln/ln.c` (BSD). The command resolves its option
//! flags and operand shapes here; the filesystem syscalls (`lstat`/`stat`/
//! `unlink`/`link`/`symlink`) and the interactive prompt live in the thin bin
//! `src/bin/ln.rs`, mirroring how `ln.c` separates `main`/`linkit`'s argument
//! bookkeeping from its `linkf()`/`unlink()` calls.
//!
//! [`LnFlags::parse`] reproduces `getopt(argc, argv, "fhinsv")`: `-f` (force,
//! clears interactive), `-i` (interactive, clears force), `-h`/`-n` (treat a
//! symlinked destination as a file, don't follow into it), `-s` (symbolic),
//! `-v` (verbose). After `--` or the first non-option, the remainder are the
//! operands. `plan_operands` encodes the argc shape (1 → link into `.`,
//! 2 → explicit source, N → directory form), and [`basename`] is C's
//! `strrchr(target, '/')`-based name used for the directory form.

use alloc::vec::Vec;

/// The resolved option set of `ln`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LnFlags {
    /// `-f`: unlink an existing destination instead of failing.
    pub force: bool,
    /// `-i`: prompt before replacing an existing destination.
    pub interactive: bool,
    /// `-h`/`-n`: do not follow a symlinked destination directory.
    pub no_follow: bool,
    /// `-s`: make a symbolic link rather than a hard link.
    pub symbolic: bool,
    /// `-v`: print each created link.
    pub verbose: bool,
}

/// The shape `ln` inferred from its operand list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperandPlan<'a> {
    /// No usable operands — the caller prints usage and exits 1.
    Usage,
    /// `ln target`: link into the current directory (source `"."`).
    ImplicitCwd { target: &'a str },
    /// `ln target source`: one explicit link path.
    Pair { target: &'a str, source: &'a str },
    /// `ln target… directory`: link each target into the directory by its
    /// basename. The trailing operand is the directory (not a target).
    ToDirectory { targets: Vec<&'a str>, dir: &'a str },
}

impl LnFlags {
    /// Parse the option string `"fhinsv"` from the leading args. Returns the
    /// flags and the operand slice (everything after option processing). A
    /// bad option is reported as `None` so the bin can print usage + exit 1.
    pub fn parse<'a>(args: &[&'a str]) -> Option<(LnFlags, Vec<&'a str>)> {
        let mut f = LnFlags::default();
        let mut i = 0;
        let mut done = false;
        while i < args.len() {
            let a = args[i];
            if done {
                break;
            }
            if a == "--" {
                done = true;
                i += 1;
                continue;
            }
            if a.len() >= 2 && a.starts_with('-') {
                for &byte in a.as_bytes()[1..].iter() {
                    match byte {
                        b'f' => {
                            f.force = true;
                            f.interactive = false;
                        }
                        b'i' => {
                            f.interactive = true;
                            f.force = false;
                        }
                        b'h' | b'n' => f.no_follow = true,
                        b's' => f.symbolic = true,
                        b'v' => f.verbose = true,
                        _ => return None,
                    }
                }
                i += 1;
                continue;
            }
            // First non-option ends option processing (no permutation, as in
            // the classic getopt BSD uses here).
            break;
        }
        Some((f, args[i..].to_vec()))
    }
}

/// Classify the operand list into the `ln` invocation shape.
pub fn plan_operands<'a>(operands: &[&'a str]) -> OperandPlan<'a> {
    match operands.len() {
        0 => OperandPlan::Usage,
        1 => OperandPlan::ImplicitCwd {
            target: operands[0],
        },
        2 => OperandPlan::Pair {
            target: operands[0],
            source: operands[1],
        },
        n => OperandPlan::ToDirectory {
            targets: operands[..n - 1].to_vec(),
            dir: operands[n - 1],
        },
    }
}

/// C `strrchr(target, '/')` + 1, else `target` — the last path component.
pub fn basename(target: &str) -> &str {
    match target.rfind('/') {
        Some(i) => &target[i + 1..],
        None => target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_parse_known_letters() {
        let (f, rest) = LnFlags::parse(&["-s", "-f", "-v", "a", "b"]).unwrap();
        assert!(f.symbolic && f.force && f.verbose);
        assert!(!f.interactive, "-f clears -i");
        assert_eq!(rest, ["a", "b"]);
    }

    #[test]
    fn combined_cluster_flags() {
        let (f, rest) = LnFlags::parse(&["-sv", "x"]).unwrap();
        assert!(f.symbolic && f.verbose);
        assert_eq!(rest, ["x"]);
    }

    #[test]
    fn f_and_i_last_wins_both_ways() {
        // -i after -f: interactive set, force cleared (mirror ln.c's switch).
        let (f, _) = LnFlags::parse(&["-f", "-i"]).unwrap();
        assert!(f.interactive && !f.force);
        let (f, _) = LnFlags::parse(&["-i", "-f"]).unwrap();
        assert!(f.force && !f.interactive);
    }

    #[test]
    fn unknown_option_is_error() {
        assert!(LnFlags::parse(&["-z"]).is_none());
    }

    #[test]
    fn double_dash_ends_options() {
        let (_, rest) = LnFlags::parse(&["--", "-notanoption"]).unwrap();
        assert_eq!(rest, ["-notanoption"]);
    }

    #[test]
    fn operand_shapes() {
        assert_eq!(plan_operands(&[]), OperandPlan::Usage);
        assert_eq!(
            plan_operands(&["t"]),
            OperandPlan::ImplicitCwd { target: "t" }
        );
        assert_eq!(
            plan_operands(&["t", "s"]),
            OperandPlan::Pair {
                target: "t",
                source: "s"
            }
        );
        assert_eq!(
            plan_operands(&["a", "b", "d"]),
            OperandPlan::ToDirectory {
                targets: vec!["a", "b"],
                dir: "d"
            }
        );
    }

    #[test]
    fn basename_strips_dir() {
        assert_eq!(basename("/etc/passwd"), "passwd");
        assert_eq!(basename("plain"), "plain");
        assert_eq!(basename("trailing/"), "");
    }
}
