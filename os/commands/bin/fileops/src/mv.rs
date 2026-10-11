//! `mv` — move / rename files, the pure argument layer.
//!
//! Ground truth: `minix3/bin/mv/mv.c` (BSD). The option scan (`getopt(argc,
//! argv, "ifv")`, `mv.c:88-110`) and the operand-shape decision (`mv.c:113-159`)
//! live here; the filesystem legs — `rename` for a same-filesystem move and the
//! interactive prompt — live in the thin bin `src/bin/mv.rs`.
//!
//! [`MvFlags::parse`] mirrors the `-f`/`-i` switch pair (the last one seen
//! wins, as in `rm`/`ln`) plus `-v`. [`plan_operands`] encodes the two shapes:
//! `mv name target` (a rename) and `mv name ... directory` (each name moved
//! into the directory under its basename); fewer than two operands is a usage
//! error. [`dest_name`] is the trailing-slash-stripped final component C uses
//! to build the in-directory target (`mv.c:144-149`).

use alloc::vec::Vec;

/// The resolved option set of `mv` (`mv.c:88-110`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MvFlags {
    /// `-f`: do not prompt before overwriting (clears `-i`).
    pub force: bool,
    /// `-i`: prompt before overwriting an existing target (clears `-f`).
    pub interactive: bool,
    /// `-v`: print each rename as it happens.
    pub verbose: bool,
}

impl MvFlags {
    /// Parse the option string `"ifv"`. Returns the flags and the operand
    /// slice, or `None` on an unknown option (usage + exit 1). BSD getopt: the
    /// first non-option ends the scan; `--` ends it too.
    pub fn parse<'a>(args: &[&'a str]) -> Option<(MvFlags, Vec<&'a str>)> {
        let mut f = MvFlags::default();
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
                        b'f' => {
                            f.force = true;
                            f.interactive = false;
                        }
                        b'i' => {
                            f.interactive = true;
                            f.force = false;
                        }
                        b'v' => f.verbose = true,
                        _ => return None,
                    }
                }
                i += 1;
                continue;
            }
            break; // first non-option ends option processing (no permutation)
        }
        Some((f, args[i..].to_vec()))
    }
}

/// The shape `mv` inferred from its operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MvPlan<'a> {
    /// Fewer than two operands — the caller prints usage and exits 1.
    Usage,
    /// `mv name target`: a single rename (target may be a file or a dir; the
    /// bin decides — if the target is a directory, `name` lands under its
    /// basename, matching `mv.c:113`).
    Pair { source: &'a str, target: &'a str },
    /// `mv name ... directory`: move each source into the directory under its
    /// basename. Requires the trailing operand to actually be a directory (the
    /// bin checks; otherwise it is a usage error like `mv.c:118`).
    ToDirectory { sources: Vec<&'a str>, dir: &'a str },
}

/// Encode the operand list into an `mv` invocation shape (`mv.c:113-134`).
pub fn plan_operands<'a>(operands: &[&'a str]) -> MvPlan<'a> {
    let n = operands.len();
    if n < 2 {
        return MvPlan::Usage;
    }
    if n == 2 {
        return MvPlan::Pair {
            source: operands[0],
            target: operands[1],
        };
    }
    MvPlan::ToDirectory {
        sources: operands[..n - 1].to_vec(),
        dir: operands[n - 1],
    }
}

/// The basename C uses when moving a source into a directory: strip trailing
/// slashes (never to empty, `mv.c:144-146`), then take the part after the last
/// `/` (`mv.c:147-149`). `dir/name` → `name`, `dir/name/` → `name`, `name` →
/// `/` → `` (the root has no final component, matching `ln::basename`).
pub fn dest_name(path: &str) -> &str {
    let mut s = path;
    while s.len() > 1 && s.ends_with('/') {
        s = &s[..s.len() - 1];
    }
    match s.rfind('/') {
        Some(i) => &s[i + 1..],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_parse_known_letters() {
        let (f, rest) = MvFlags::parse(&["-f", "-v", "a", "b"]).unwrap();
        assert!(f.force && f.verbose);
        assert!(!f.interactive, "-f clears -i");
        assert_eq!(rest, ["a", "b"]);
    }

    #[test]
    fn f_and_i_last_wins_both_ways() {
        let (f, _) = MvFlags::parse(&["-f", "-i"]).unwrap();
        assert!(f.interactive && !f.force);
        let (f, _) = MvFlags::parse(&["-i", "-f"]).unwrap();
        assert!(f.force && !f.interactive);
    }

    #[test]
    fn combined_cluster_and_double_dash() {
        let (f, rest) = MvFlags::parse(&["-iv", "x"]).unwrap();
        assert!(f.interactive && f.verbose);
        assert_eq!(rest, ["x"]);
        let (_, rest) = MvFlags::parse(&["--", "-f"]).unwrap();
        assert_eq!(rest, ["-f"], "-f after -- is an operand");
    }

    #[test]
    fn unknown_option_is_error() {
        assert!(MvFlags::parse(&["-z"]).is_none());
    }

    #[test]
    fn operand_shapes() {
        assert_eq!(plan_operands(&[]), MvPlan::Usage);
        assert_eq!(plan_operands(&["a"]), MvPlan::Usage);
        assert_eq!(
            plan_operands(&["a", "b"]),
            MvPlan::Pair {
                source: "a",
                target: "b"
            }
        );
        assert_eq!(
            plan_operands(&["a", "b", "d"]),
            MvPlan::ToDirectory {
                sources: vec!["a", "b"],
                dir: "d"
            }
        );
    }

    #[test]
    fn dest_name_strips_trailing_slash_and_dir() {
        assert_eq!(dest_name("dir/name"), "name");
        assert_eq!(dest_name("dir/name/"), "name");
        assert_eq!(dest_name("name"), "name");
        assert_eq!(dest_name("/"), ""); // root has no final component (as ln::basename)
        assert_eq!(dest_name("a//b//"), "b");
    }
}
