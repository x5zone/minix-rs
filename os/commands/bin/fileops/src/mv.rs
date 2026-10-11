//! `mv` — move / rename files, the pure argument layer.
//!
//! Ground truth: `minix3/bin/mv/mv.c` (BSD). The option scan (`getopt(argc,
//! argv, "ifv")`, `mv.c:88-110`) and the operand-shape decision (`mv.c:113-159`)
//! live here, together with the cross-device decisions (`mv.c:236`, `mv.c:251`,
//! `mv.c:272`); the filesystem legs — `rename` for a same-filesystem move, the
//! `EXDEV` copy, the interactive prompt — live in the thin bin `src/bin/mv.rs`.
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

/// The cross-device form `mv.c:251-252` picks once `rename` failed with
/// `EXDEV` and the destination has been cleared out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossDevice {
    /// A plain file: the C program copies it inline with `fastcopy`
    /// (`mv.c:256-354`) and then unlinks the source.
    InlineCopy,
    /// Anything else (directory, symlink, device, FIFO): `copy` (`mv.c:357-399`)
    /// does not copy by hand — it runs `/bin/cp -PRp` and then `/bin/rm -rf`
    /// on the source. That needs the exec delivery chain, so the program
    /// reports it instead of pretending.
    NeedsSubprocess,
}

/// `mv.c:251`: the split is on the source's file type as recorded by `lstat`
/// (`mv.c:246`), so a symlink is never the inline case.
pub fn cross_device_plan(src_mode: u32) -> CrossDevice {
    if crate::cp::is_reg(src_mode) {
        CrossDevice::InlineCopy
    } else {
        CrossDevice::NeedsSubprocess
    }
}

/// `mv.c:236`: the call that clears a destination which survived the `EXDEV`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalCall {
    /// The destination is a directory → `rmdir`.
    Rmdir,
    /// Anything else → `unlink`.
    Unlink,
}

/// `mv.c:236`: the destination that survived the `EXDEV` is removed first, and
/// the call chosen depends on what it is — a directory needs `rmdir`, anything
/// else needs `unlink`.
pub fn existing_removal(dst_mode: u32) -> RemovalCall {
    if crate::cp::is_dir(dst_mode) {
        RemovalCall::Rmdir
    } else {
        RemovalCall::Unlink
    }
}

/// `mv.c:272`: `fastcopy` hands the source's whole `st_mode` to `open` when it
/// creates the destination — unlike `cp`, which strips the set-user-id and
/// set-group-id bits (`cp::create_mode`). The kernel masks the type bits off;
/// the set-id bits are deliberately kept, because the very next thing
/// `fastcopy` does is re-apply the source's mode and owner anyway.
pub fn move_create_mode(from_mode: u32) -> u32 {
    from_mode
}

/// Which question `mv` asks before clobbering an existing target
/// (`mv.c:172-203`). C's ladder is: the whole block only runs when `-f` is
/// absent and the target exists; inside it `-i` asks "overwrite", otherwise a
/// terminal standard input plus a target we cannot write (and can still
/// `stat`) asks "override", and anything else skips the question. Both asking
/// branches first check the source is reachable at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// No question: `-f`, a missing target, or the fall-through branch.
    None,
    /// The asking branch ran and `access(from, F_OK)` failed: C reports
    /// `rename <source>` and counts it as an error (`mv.c:177-180`, `:183-186`).
    SourceUnreachable,
    /// `-i` on an existing target: `overwrite <target>? ` (`mv.c:181`).
    Overwrite,
    /// No flags, standard input is a terminal, the target is not writable and
    /// can be stat'd: `override <perms> <user>/<group> for <target>? `
    /// (`mv.c:182-191`).
    Override,
}

/// The ask ladder of `mv.c:172-203`, as a pure decision.
///
/// Note `-i` does not consult the terminal here: unlike `cp.c:148`, `mv.c:92`
/// sets `iflg = 1` unconditionally, so an interactive flag asks even with a
/// pipe for standard input (the answer then reads as end-of-file, which C
/// treats as "no").
#[allow(clippy::too_many_arguments)]
pub fn ask_about(
    force: bool,
    interactive: bool,
    target_exists: bool,
    stdin_is_tty: bool,
    target_writable: bool,
    target_statable: bool,
    source_exists: bool,
) -> Ask {
    if force || !target_exists {
        return Ask::None;
    }
    if interactive {
        return if source_exists {
            Ask::Overwrite
        } else {
            Ask::SourceUnreachable
        };
    }
    if stdin_is_tty && !target_writable && target_statable {
        return if source_exists {
            Ask::Override
        } else {
            Ask::SourceUnreachable
        };
    }
    Ask::None
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

    #[test]
    fn cross_device_split_is_on_the_source_type() {
        assert_eq!(cross_device_plan(0o100644), CrossDevice::InlineCopy);
        assert_eq!(
            cross_device_plan(crate::cp::S_IFDIR | 0o755),
            CrossDevice::NeedsSubprocess
        );
        assert_eq!(
            cross_device_plan(crate::cp::S_IFLNK | 0o777),
            CrossDevice::NeedsSubprocess,
            "mv.c:246 lstats the source, so a symlink never takes the inline leg"
        );
    }

    #[test]
    fn destination_removal_follows_its_type() {
        assert_eq!(
            existing_removal(crate::cp::S_IFDIR | 0o755),
            RemovalCall::Rmdir
        );
        assert_eq!(existing_removal(0o100644), RemovalCall::Unlink);
        assert_eq!(
            existing_removal(crate::cp::S_IFLNK | 0o777),
            RemovalCall::Unlink,
            "a symlink destination is unlinked, not rmdir'd (mv.c:236)"
        );
    }

    #[test]
    fn move_create_mode_keeps_what_cp_create_mode_strips() {
        // mv.c:272 hands the whole st_mode to open(); cp's create_mode
        // (utils.c:146) drops the set-user-id and set-group-id bits. The two
        // engines differ here on purpose, so the difference is pinned.
        assert_eq!(move_create_mode(0o4755), 0o4755);
        assert_eq!(crate::cp::create_mode(0o4755), 0o0755);
        assert_ne!(move_create_mode(0o4755), crate::cp::create_mode(0o4755));
    }

    #[test]
    fn no_force_or_missing_target_means_no_question() {
        // mv.c:172: `if (!fflg && !access(to, F_OK))` gates the whole block.
        assert_eq!(
            ask_about(true, true, true, true, false, true, true),
            Ask::None,
            "-f wins over -i in the gate, not just in the flags"
        );
        assert_eq!(
            ask_about(false, true, false, true, false, true, true),
            Ask::None,
            "nothing to clobber, nothing to ask"
        );
    }

    #[test]
    fn interactive_asks_overwrite_and_checks_the_source_first() {
        assert_eq!(
            ask_about(false, true, true, false, true, true, true),
            Ask::Overwrite
        );
        assert_eq!(
            ask_about(false, true, true, false, true, true, false),
            Ask::SourceUnreachable,
            "mv.c:177-180 checks access(from, F_OK) before printing the prompt"
        );
        // mv.c:92 sets iflg unconditionally: a pipe does not disable the ask.
        assert_eq!(
            ask_about(false, true, true, false, false, true, true),
            Ask::Overwrite,
            "unlike cp, mv does not require a terminal for -i"
        );
    }

    #[test]
    fn override_needs_terminal_unwritable_and_statable() {
        assert_eq!(
            ask_about(false, false, true, true, false, true, true),
            Ask::Override
        );
        assert_eq!(
            ask_about(false, false, true, false, false, true, true),
            Ask::None,
            "not a terminal (mv.c:182 stdin_ok)"
        );
        assert_eq!(
            ask_about(false, false, true, true, true, true, true),
            Ask::None,
            "we can write it, no need to ask"
        );
        assert_eq!(
            ask_about(false, false, true, true, false, false, true),
            Ask::None,
            "stat failed: mv.c:182 requires !stat(to, &sb)"
        );
        assert_eq!(
            ask_about(false, false, true, true, false, true, false),
            Ask::SourceUnreachable
        );
    }

    #[test]
    fn interactive_branch_precedes_the_override_branch() {
        // Both conditions hold; mv.c:176 tests iflg first, so the question is
        // "overwrite", not the long "override" form.
        assert_eq!(
            ask_about(false, true, true, true, false, true, true),
            Ask::Overwrite
        );
    }
}
