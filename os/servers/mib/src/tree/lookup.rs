//! Tree lookup: one array hop, then a sorted walk.
//!
//! Mirrors `mib_find` (`tree.c:34-81`). The arena (static arrays,
//! dynamic lists) lands with 04's follow-up/08; this module owns the
//! *decisions*: which lane, hit or miss, and where a miss would insert.
//! The early-stop position doubles as 08's sorted-insert point —
//! measured once, used twice.
//!
//! 05-mib-tree-lookup.md.

/// Lookup outcome: hit lane, or miss with advice.
///
/// C: `mib_find` return + `prevpp` out-param (`tree.c:34-81`). Static
/// hits carry no removal link (`*prevpp = NULL`, :53-54 — static nodes
/// are never removed); dynamic hits carry the link position (:73-74);
/// misses carry nothing in C, but the scan already knows the insert
/// point, so this type reports it (08 consumes it; C re-walks in
/// `mib_add` — one walk saved, same order kept).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// Static hit at this array index. C: `:48-56`.
    FoundStatic {
        /// Array index (= the id). C: `&parent->node_scptr[id]`.
        index: usize,
    },
    /// Dynamic hit at this sorted position. C: `:71-75`.
    FoundDynamic {
        /// Position in the id-sorted list. C: link `dynp`.
        pos: usize,
    },
    /// Miss. C: `return NULL` (`:41`, `:81`).
    Missing {
        /// Where a dynamic node would insert to keep the sort.
        insert_at: usize,
    },
}

/// Static-lane verdict: id in range and slot alive.
///
/// C: `:48-57`. Negative ids never reach here (`:40-41` filters first —
/// the unsigned `IS_STATIC_ID` would otherwise wrap them huge, 03);
/// in-range but zeroed slots are vacant, not misses-through (the walk
/// continues to the dynamic lane — a vacant static slot does not stop
/// the search).
pub const fn find_static(csize: u32, flags: &[u32], id: i32) -> Option<usize> {
    if id < 0 || (id as u32) >= csize {
        return None;
    }
    let idx = id as usize;
    if idx < flags.len() && flags[idx] != 0 {
        Some(idx)
    } else {
        None
    }
}

/// Dynamic-lane scan: sorted walk with early stop.
///
/// C: `:69-78`. The list is sorted by id *because* userland picks ids at
/// creation (a dense array is impossible — :60-67), and the sort buys
/// the early break (`dynode_id > id` → stop, :76-77). Returns the hit
/// position or the insert point that preserves the sort.
pub const fn scan_dynamic(ids: &[i32], id: i32) -> Lookup {
    let mut pos = 0;
    while pos < ids.len() {
        if ids[pos] == id {
            return Lookup::FoundDynamic { pos };
        } else if ids[pos] > id {
            break;
        }
        pos += 1;
    }
    Lookup::Missing { insert_at: pos }
}

/// Full lookup: negative guard, static lane, dynamic lane.
///
/// C: `mib_find` whole body (`:40-81`). Negative ids miss outright
/// (`:40-41`); static hits short-circuit (`:48-56`); everything else
/// walks the sorted list (`:69-78`).
pub const fn find(csize: u32, flags: &[u32], dyn_ids: &[i32], id: i32) -> Lookup {
    if id < 0 {
        return Lookup::Missing { insert_at: 0 };
    }
    if let Some(index) = find_static(csize, flags, id) {
        return Lookup::FoundStatic { index };
    }
    scan_dynamic(dyn_ids, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_hit_and_vacant() {
        // Array lookup is O(1); zeroed slots are vacant (tree.c:48-57).
        let flags = [0x100, 0, 0x100];
        assert_eq!(find_static(3, &flags, 0), Some(0));
        assert_eq!(find_static(3, &flags, 1), None);
        assert_eq!(find_static(3, &flags, 2), Some(2));
        // Out of range and negative miss before touching the array.
        assert_eq!(find_static(3, &flags, 3), None);
        assert_eq!(find_static(3, &flags, -1), None);
    }

    #[test]
    fn test_dynamic_sorted_early_stop() {
        // Sorted list, early break past the target (tree.c:69-78).
        let ids = [1024, 1030, 1041];
        assert_eq!(scan_dynamic(&ids, 1030), Lookup::FoundDynamic { pos: 1 });
        // Misses report the insert point that keeps the sort.
        assert_eq!(scan_dynamic(&ids, 1025), Lookup::Missing { insert_at: 1 });
        assert_eq!(scan_dynamic(&ids, 1000), Lookup::Missing { insert_at: 0 });
        assert_eq!(scan_dynamic(&ids, 9999), Lookup::Missing { insert_at: 3 });
        assert_eq!(scan_dynamic(&[], 5), Lookup::Missing { insert_at: 0 });
    }

    #[test]
    fn test_find_prefers_static() {
        // Static hit short-circuits the list (tree.c:48-56 before :69).
        let flags = [0x100, 0x100];
        let dyn_ids = [0, 1];
        assert_eq!(
            find(2, &flags, &dyn_ids, 1),
            Lookup::FoundStatic { index: 1 }
        );
        // Vacant static slot falls through to the dynamic lane.
        assert_eq!(
            find(3, &[0x100, 0, 0], &[1], 1),
            Lookup::FoundDynamic { pos: 0 }
        );
        // Negative ids miss outright (tree.c:40-41).
        assert_eq!(
            find(2, &flags, &dyn_ids, -2),
            Lookup::Missing { insert_at: 0 }
        );
    }
}
