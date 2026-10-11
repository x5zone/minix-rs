//! Directory removal decisions.
//!
//! Ground truth: `minix3/bin/rmdir/rmdir.c` (NetBSD `rmdir.c` 1.26). The
//! command takes `-p`. Without it each operand is removed with `rmdir(2)`
//! (which the kernel rejects for a non-empty directory — "you may empty the
//! basket, you may not dump the筐"). With `-p`, after the operand itself is
//! removed, `rm_path` walks up: it truncates the path at each trailing `/`
//! and removes each shorter ancestor, stopping at the first failure and never
//! removing the root (truncating past the leading `/` yields an empty path,
//! which the loop treats as the top level and breaks on).
//!
//! This module owns the pure ancestor walk; the `rmdir(2)` calls and the error
//! reporting stay with the execution layer.

/// Walk the ancestors of `path` from the innermost outward — exactly the order
/// `rm_path` removes them in after the leaf has gone — calling `visit` once per
/// ancestor. This replicates `rm_path`'s `strrchr`-and-truncate loop in place:
/// each step cuts at the last `/` and, if what remains is non-empty, removes
/// it; an empty remainder (truncating past the leading `/`) is the root and
/// stops the walk. Like C, it does not pre-strip trailing slashes, so an
/// operand ending in `/` re-offers its own leaf as the first ancestor (the C
/// behaviour), rather than being normalised away.
pub fn for_each_ancestor<'a>(path: &'a str, visit: &mut dyn FnMut(&'a str)) {
    let bytes = path.as_bytes();
    let mut end = bytes.len();
    loop {
        // Walk back to the `/` at or just before `end`, mirroring `strrchr`.
        let mut i = end;
        while i > 0 && bytes[i - 1] != b'/' {
            i -= 1;
        }
        if i == 0 {
            // No `/` above this component: we are at the top level.
            break;
        }
        // The separator sits at index `i - 1`; the ancestor is everything before it.
        let ancestor = &path[..i - 1];
        if ancestor.is_empty() {
            // That separator was the leading slash: the remainder is the root.
            break;
        }
        visit(ancestor);
        end = i - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn ancestors(path: &str) -> Vec<&str> {
        let mut out = Vec::new();
        for_each_ancestor(path, &mut |a| out.push(a));
        out
    }

    #[test]
    fn test_relative_walks_innermost_first() {
        assert_eq!(ancestors("a/b/c"), vec!["a/b", "a"]);
    }

    #[test]
    fn test_absolute_stops_before_root() {
        // "/x/y": remove "/x", then the next truncation is the empty root.
        assert_eq!(ancestors("/x/y"), vec!["/x"]);
        assert_eq!(ancestors("/a/b/c"), vec!["/a/b", "/a"]);
    }

    #[test]
    fn test_single_component_has_no_ancestors() {
        assert!(ancestors("a").is_empty());
        assert!(ancestors("/x").is_empty());
    }

    #[test]
    fn test_trailing_slash_reoffers_leaf_like_c() {
        // Faithful to `rm_path`: it does not pre-strip the trailing `/`, so the
        // first truncation turns "a/b/" back into "a/b" (the leaf, already
        // removed by main) and the walk continues to "a".
        assert_eq!(ancestors("a/b/"), vec!["a/b", "a"]);
        assert_eq!(ancestors("/x/y//"), vec!["/x/y/", "/x/y", "/x"]);
    }

    #[test]
    fn test_reverse_of_parent_prefixes() {
        // The ancestor sequence is for_each_parent minus its terminal leaf,
        // reversed — the cross-check that both walkers agree on the components.
        let mut parents = Vec::new();
        crate::mkdir::for_each_parent("a/b/c", &mut |p, _| parents.push(p));
        let mut got = Vec::new();
        for_each_ancestor("a/b/c", &mut |a| got.push(a));
        let mut expected = parents;
        expected.pop(); // drop the leaf
        expected.reverse();
        assert_eq!(got, expected);
    }
}
