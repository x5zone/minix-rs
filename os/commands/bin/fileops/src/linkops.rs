//! `link` / `unlink` — the hard-link create/remove commands' argument layer.
//!
//! Ground truth: `minix3/usr.sbin/link/link.c` (71 lines) and
//! `minix3/usr.sbin/unlink/unlink.c` (70 lines). Each is one system call behind
//! a POSIX argument-count check — no options, no `getopt`: `link` needs exactly
//! two operands (`link(2)`'s `oldpath`, `newpath`), `unlink` exactly one
//! (`unlink(2)`'s `pathname`); any other count is a usage error. That shape
//! check is the deciding half here; the `link`/`unlink` syscalls and the error
//! printing live in the thin bins `src/bin/link.rs` and `src/bin/unlink.rs`.
//!
//! (These are distinct from `ln`, which is the full BSD multi-operand link
//! tool with `-s`/`-f`/`-i`/`-n`; `link`/`unlink` are the single-call POSIX
//! utilities the standard defines.)

/// `link.c:52` `if (argc != 3) usage()`: exactly two operands. Returns them as
/// `(oldpath, newpath)`, or `None` when the count is wrong.
pub fn link_operands<'a>(operands: &[&'a str]) -> Option<(&'a str, &'a str)> {
    match operands {
        [old, new] => Some((*old, *new)),
        _ => None,
    }
}

/// `unlink.c:52` `if (argc != 2) usage()`: exactly one operand. Returns it, or
/// `None` when the count is wrong.
pub fn unlink_operand<'a>(operands: &[&'a str]) -> Option<&'a str> {
    match operands {
        [path] => Some(*path),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_needs_exactly_two() {
        assert_eq!(link_operands(&["a", "b"]), Some(("a", "b")));
        assert_eq!(link_operands(&[]), None);
        assert_eq!(link_operands(&["a"]), None);
        assert_eq!(link_operands(&["a", "b", "c"]), None);
    }

    #[test]
    fn unlink_needs_exactly_one() {
        assert_eq!(unlink_operand(&["f"]), Some("f"));
        assert_eq!(unlink_operand(&[]), None);
        assert_eq!(unlink_operand(&["a", "b"]), None);
    }

    #[test]
    fn link_operands_preserve_paths_verbatim() {
        // No basename/dir handling — the raw operands go straight to link(2).
        let (old, new) = link_operands(&["/etc/passwd", "backup"]).unwrap();
        assert_eq!(old, "/etc/passwd");
        assert_eq!(new, "backup");
    }
}
