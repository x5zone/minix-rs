//! Dynamic node lifecycle verdicts: names, scans, flags, guards.
//!
//! Mirrors `mib_check_name` / `mib_scan` (`tree.c:247-359`), the
//! `mib_create` validation chain (`:499-665`), `mib_remove` /
//! `mib_destroy` guards (`:785-915`), and the allocation-size math
//! (`:679-684`). Allocation, linking, copying, and freeing are arena
//! effects owned by 08's follow-up; every *decision* is judged here.
//!
//! 08-mib-dynamic-nodes.md.

use minix_types::{
    CREATE_BASE, CTLFLAG_IMMEDIATE, CTLFLAG_OWNDATA, CTLFLAG_PERMANENT, CTLFLAG_UNSIGNED,
    CTLTYPE_BOOL, CTLTYPE_INT, CTLTYPE_NODE, CTLTYPE_QUAD, CTLTYPE_STRING, CTLTYPE_STRUCT, EBUSY,
    EINVAL, ENOTEMPTY, EPERM, SYSCTL_TYPEMASK, SYSCTL_USERFLAGS, sysctl_flags,
};

use super::flag::{CTLFLAG_PARENT, CTLFLAG_REMOTE};

/// Validate a node name: C-symbol style, nonempty, terminated in-buf.
///
/// C: `mib_check_name` — tree.c:247-266. Letters, digits (not first),
/// underscore only; empty or unterminated-within-`buf` fails. Returns
/// the length *excluding* the terminator; `None` is C's zero (failure).
/// The `CTL_MAXNAME` component cap is 01's business (name *arrays*),
/// this is the *string* rule — the two stack, neither subsumes.
pub fn check_name(buf: &[u8]) -> Option<usize> {
    let mut len = 0;
    for &c in buf {
        if c == 0 {
            break;
        }
        let ok = c.is_ascii_alphabetic() || c == b'_' || (c.is_ascii_digit() && len > 0);
        if !ok {
            return None;
        }
        len += 1;
    }
    if len == 0 || len == buf.len() {
        return None;
    }
    Some(len)
}

/// Where a scan stopped: which collision, or a free slot.
///
/// C: `mib_scan` outcomes — tree.c:305-358. Static collisions report the
/// static id; dynamic collisions the dynamic id; success reports the id
/// *and* the sorted insert position (05's `Missing{insert_at}`, reused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanOutcome {
    /// Static id or name taken. C: `:310-314`.
    StaticClash {
        /// Conflicting static id. C: `*idp = id`.
        id: i32,
    },
    /// Dynamic id taken (explicit request) or name taken anywhere.
    /// C: `:330-342, :349-353`.
    DynClash {
        /// Conflicting dynamic id. C: `*idp`.
        id: i32,
    },
    /// Free to create. C: `:356-358`.
    Free {
        /// Chosen id (requested, or first free past the base).
        id: i32,
        /// Sorted insert position in the dynamic list.
        insert_at: usize,
    },
}

/// Scan for collisions and a free id.
///
/// Pure form of `mib_scan` (`:305-358`) over slices: static flags/names,
/// dynamic ids/names (both sorted by id). `given_id >= 0` pins the id
/// (taken → clash); negative auto-picks from `max(CREATE_BASE, size)`
/// (`:324`, well clear of static range — :293-297) bumping past taken
/// ids (`:336`). Name clashes are checked in *both* lanes, including
/// past the insert point (`:345-354` — stopping early would miss a
/// same-name node further down).
#[allow(clippy::too_many_arguments)]
pub fn scan(
    parent_size: u32,
    static_flags: &[u32],
    static_names: &[&str],
    dyn_ids: &[i32],
    dyn_names: &[&str],
    given_id: i32,
    name: &str,
) -> ScanOutcome {
    for (id, (flags, sname)) in static_flags.iter().zip(static_names).enumerate() {
        if *flags == 0 {
            continue;
        }
        if id as i32 == given_id || *sname == name {
            return ScanOutcome::StaticClash { id: id as i32 };
        }
    }
    let mut id = if given_id >= 0 {
        given_id
    } else {
        CREATE_BASE.max(parent_size as i32)
    };
    // First pass: walk to the insert point, checking id and name clashes
    // on the way. Bumping `id` past taken neighbours keeps the walk sorted
    // without restarting.
    let mut insert_at = dyn_ids.len();
    let mut i = 0;
    while i < dyn_ids.len() {
        if dyn_ids[i] > id {
            insert_at = i;
            break;
        }
        if dyn_ids[i] == id {
            if given_id >= 0 {
                return ScanOutcome::DynClash { id };
            }
            id += 1;
            insert_at = dyn_ids.len();
        }
        if dyn_names[i] == name {
            return ScanOutcome::DynClash { id: dyn_ids[i] };
        }
        i += 1;
    }
    // Second pass: names past the insert point still collide (:345-354).
    for (did, dname) in dyn_ids.iter().zip(dyn_names).skip(i) {
        debug_assert!(*did > id);
        if *dname == name {
            return ScanOutcome::DynClash { id: *did };
        }
    }
    ScanOutcome::Free { id, insert_at }
}

/// Whether create flags are user-settable.
///
/// C: `SYSCTL_FLAGS & ~(USERFLAGS | UNSIGNED)` must be zero —
/// tree.c:552-554. `UNSIGNED` is allowed with interpretation left to
/// userland (:549-551).
pub const fn valid_create_flags(flags: u32) -> bool {
    sysctl_flags(flags) & !(SYSCTL_USERFLAGS | CTLFLAG_UNSIGNED) == 0
}

/// Sanitize the multi-bit `READWRITE` field: any write bit → full mask.
///
/// C: `if (flags & READWRITE) flags |= READWRITE` — tree.c:575-576.
/// `READWRITE` is `0x70` (three bits, 02); a caller setting one bit gets
/// the documented whole — partial write masks are not a thing.
pub const fn sanitize_rw(flags: u32) -> u32 {
    use minix_types::CTLFLAG_READWRITE;
    if flags & CTLFLAG_READWRITE != 0 {
        flags | CTLFLAG_READWRITE
    } else {
        flags
    }
}

/// Immediate/owndata/data-pointer combination rule.
///
/// C: tree.c:556-572 (create) + :623-624 (node-type ban). Without
/// `IMMEDIATE`, non-node types must own their data (`OWNDATA` forced —
/// kernel addresses as data pointers are meaningless on MINIX3, :564-567);
/// `IMMEDIATE + OWNDATA` together is contradictory (`:571-572`);
/// node-type nodes take neither (`:623-624`).
pub const fn data_combo_ok(flags: u32, has_data_ptr: bool) -> bool {
    let ty = flags & SYSCTL_TYPEMASK;
    let immediate = flags & CTLFLAG_IMMEDIATE != 0;
    let owndata = flags & CTLFLAG_OWNDATA != 0;
    if ty == CTLTYPE_NODE {
        return !immediate && !owndata;
    }
    if immediate {
        return !owndata;
    }
    owndata || !has_data_ptr
}

/// Type/size validation for a create request.
///
/// C: tree.c:579-631. Bool/int/quad demand exact `sizeof`; strings take
/// any nonzero size (zero + data means "measure it", :597-603 — the
/// measuring itself is 06's `copyin_str`); structs any nonzero size;
/// node-types demand zero size, no immediate/owndata, and empty
/// child-spec (`:625-627`); anything else fails.
pub const fn type_size_ok(ty: u32, size: u64, child_spec: bool) -> bool {
    match ty {
        CTLTYPE_BOOL => size == 1,
        CTLTYPE_INT => size == 4,
        CTLTYPE_QUAD => size == 8,
        CTLTYPE_STRING | CTLTYPE_STRUCT => size != 0,
        CTLTYPE_NODE => size == 0 && !child_spec,
        _ => false,
    }
}

/// Allocation size for the dynode block: header + name + optional data.
///
/// C: `sizeof(*dynode) + namelen (+ size unless immediate)` —
/// tree.c:679-681. One `malloc` holds node + name + data (03 §2.2);
/// failure speaks `EINVAL`, never `ENOMEM` (`:684`, 02 §2.7).
/// `header` is the caller's `size_of` its dynode header.
pub const fn create_alloc_size(header: usize, namelen: usize, data: u64, immediate: bool) -> usize {
    if immediate {
        header + namelen
    } else {
        header + namelen + data as usize
    }
}

/// Destroy guards for the target node.
///
/// C: `mib_destroy` target checks — tree.c:869-899. Permanent nodes
/// refuse (`EPERM`); remote mount points are busy (`EBUSY` — unmount
/// first, 12); function nodes (node-type without `PARENT`) refuse
/// (`EPERM`); non-empty parents refuse (`ENOTEMPTY`). Version/name match
/// failures speak `EINVAL` even where NetBSD says `ENOENT` (`:890,898` —
/// deliberate deviation, commented in C).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestroyRefusal {
    /// Permanent node. C: `:870-871` → `EPERM`.
    Permanent,
    /// Remote mount point: unmount first. C: `:876-877` → `EBUSY`.
    MountBusy,
    /// Node-type with a function (no `PARENT`). C: `:880-881` → `EPERM`.
    HasFunction,
    /// Parent still has children. C: `:884-885` → `ENOTEMPTY`.
    NotEmpty,
    /// Staged version mismatch. C: `:889-890` → `EINVAL`.
    VersionStale,
    /// Staged name mismatch. C: `:893-899` → `EINVAL`.
    NameMismatch,
}

/// Judge whether the target may be destroyed (`Ok`) or why not.
pub const fn check_destroy(
    flags: u32,
    child_count: u32,
    staged_ver: u32,
    node_ver: u32,
    name_ok: bool,
) -> Result<(), (DestroyRefusal, i32)> {
    if flags & CTLFLAG_PERMANENT != 0 {
        return Err((DestroyRefusal::Permanent, EPERM));
    }
    let ty = flags & SYSCTL_TYPEMASK;
    if ty == CTLTYPE_NODE {
        if flags & CTLFLAG_REMOTE != 0 {
            return Err((DestroyRefusal::MountBusy, EBUSY));
        }
        if flags & CTLFLAG_PARENT == 0 {
            return Err((DestroyRefusal::HasFunction, EPERM));
        }
        if child_count != 0 {
            return Err((DestroyRefusal::NotEmpty, ENOTEMPTY));
        }
    }
    if staged_ver != 0 && staged_ver != node_ver {
        return Err((DestroyRefusal::VersionStale, EINVAL));
    }
    if !name_ok {
        return Err((DestroyRefusal::NameMismatch, EINVAL));
    }
    Ok(())
}

/// Counter deltas for a removal.
///
/// C: `mib_remove` — tree.c:793-829. Descriptions free one object when
/// owned (`:794-797` — data never frees separately, `:799-804`
/// *forbids* consulting `OWNDATA` here); dynamic nodes free the dynode
/// and shrink `csize` (`:814-823`), static nodes zero in place (`:825`);
/// both shrink `clen`, drop one node, and bump the path version (`:827-832`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveDelta {
    /// Free the `strdup`ed description. C: `:794-797`.
    pub free_desc: bool,
    /// Free the dynode block (false = zero the static node in place).
    pub free_dynode: bool,
    /// Shrink the static window (dynamic only). C: `:823`.
    pub csize_dec: u32,
    /// Objects freed (desc + dynode, each at most one).
    pub objects_dec: u32,
}

/// Compute the removal deltas. `dynamic` = linked (vs static zeroing);
/// `owndesc` = description owned.
pub const fn remove_delta(dynamic: bool, owndesc: bool) -> RemoveDelta {
    RemoveDelta {
        free_desc: owndesc,
        free_dynode: dynamic,
        csize_dec: if dynamic { 1 } else { 0 },
        objects_dec: (if owndesc { 1 } else { 0 }) + (if dynamic { 1 } else { 0 }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{CTLFLAG_PERMANENT, CTLFLAG_READWRITE};

    #[test]
    fn test_check_name_symbol_style() {
        // C: mib_check_name (tree.c:253-263) — letters/digits(non-first)/_.
        assert_eq!(check_name(b"kern\0"), Some(4));
        assert_eq!(check_name(b"a9_\0rest"), Some(3));
        assert_eq!(check_name(b"_\0"), Some(1));
        assert_eq!(check_name(b"9a\0"), None);
        assert_eq!(check_name(b"a-b\0"), None);
        assert_eq!(check_name(b"a b\0"), None);
        assert_eq!(check_name(b"\0"), None);
        assert_eq!(check_name(b"abc"), None);
        assert_eq!(check_name(b""), None);
    }

    #[test]
    fn test_scan_clashes() {
        let sf = [0x100u32, 0];
        let sn = ["kern", ""];
        // Static id clash (tree.c:310-314).
        assert_eq!(
            scan(2, &sf, &sn, &[], &[], 0, "new"),
            ScanOutcome::StaticClash { id: 0 }
        );
        // Static name clash.
        assert_eq!(
            scan(2, &sf, &sn, &[], &[], 5, "kern"),
            ScanOutcome::StaticClash { id: 0 }
        );
        // Dynamic id clash on explicit request (:330-334).
        assert_eq!(
            scan(2, &sf, &sn, &[1030], &["x"], 1030, "new"),
            ScanOutcome::DynClash { id: 1030 }
        );
        // Dynamic name clash anywhere (:338-342).
        assert_eq!(
            scan(2, &sf, &sn, &[1030], &["x"], 5, "x"),
            ScanOutcome::DynClash { id: 1030 }
        );
    }

    #[test]
    fn test_scan_auto_id() {
        let sf = [0x100u32];
        let sn = ["kern"];
        // Auto-pick starts at max(CREATE_BASE, size) (tree.c:324).
        assert_eq!(
            scan(1, &sf, &sn, &[], &[], -1, "new"),
            ScanOutcome::Free {
                id: 1024,
                insert_at: 0
            }
        );
        // Bumps past taken ids (:336).
        assert_eq!(
            scan(1, &sf, &sn, &[1024], &["a"], -1, "new"),
            ScanOutcome::Free {
                id: 1025,
                insert_at: 1
            }
        );
        // Big static tables push the base up.
        assert_eq!(
            scan(2000, &sf, &sn, &[], &[], -1, "new"),
            ScanOutcome::Free {
                id: 2000,
                insert_at: 0
            }
        );
    }

    #[test]
    fn test_flag_combos() {
        // Only USERFLAGS + UNSIGNED pass (:552-554).
        assert!(valid_create_flags(CTLFLAG_READWRITE));
        assert!(valid_create_flags(CTLFLAG_UNSIGNED));
        assert!(!valid_create_flags(minix_types::CTLFLAG_ROOT));
        // RW sanitize fills the mask (:575-576).
        assert_eq!(sanitize_rw(0x10), CTLFLAG_READWRITE);
        assert_eq!(sanitize_rw(0), 0);
        // Immediate/owndata combos (:556-572, :623-624).
        let node = CTLTYPE_NODE;
        assert!(data_combo_ok(node, false));
        assert!(!data_combo_ok(node | CTLFLAG_IMMEDIATE, false));
        assert!(!data_combo_ok(node | CTLFLAG_OWNDATA, false));
        assert!(data_combo_ok(CTLTYPE_INT | CTLFLAG_IMMEDIATE, false));
        assert!(!data_combo_ok(
            CTLTYPE_INT | CTLFLAG_IMMEDIATE | CTLFLAG_OWNDATA,
            false
        ));
        assert!(data_combo_ok(CTLTYPE_INT | CTLFLAG_OWNDATA, false));
        assert!(!data_combo_ok(CTLTYPE_STRING, true));
    }

    #[test]
    fn test_type_sizes() {
        // Exact sizes for scalars (:581-591); nonzero for string/struct.
        assert!(type_size_ok(CTLTYPE_BOOL, 1, false));
        assert!(!type_size_ok(CTLTYPE_BOOL, 2, false));
        assert!(type_size_ok(CTLTYPE_INT, 4, false));
        assert!(type_size_ok(CTLTYPE_QUAD, 8, false));
        assert!(type_size_ok(CTLTYPE_STRING, 9, false));
        assert!(!type_size_ok(CTLTYPE_STRING, 0, false));
        assert!(type_size_ok(CTLTYPE_STRUCT, 3, false));
        // Node-type: zero size, no child spec (:621-627).
        assert!(type_size_ok(CTLTYPE_NODE, 0, false));
        assert!(!type_size_ok(CTLTYPE_NODE, 1, false));
        assert!(!type_size_ok(CTLTYPE_NODE, 0, true));
        assert!(!type_size_ok(99, 4, false));
    }

    #[test]
    fn test_create_alloc_math() {
        // header + name (+ data unless immediate) (:679-684).
        assert_eq!(create_alloc_size(48, 4, 16, false), 68);
        assert_eq!(create_alloc_size(48, 4, 16, true), 52);
    }

    #[test]
    fn test_destroy_guards() {
        use minix_types::{EBUSY, EINVAL, ENOTEMPTY, EPERM};
        let leaf = CTLTYPE_INT | CTLFLAG_READWRITE;
        assert_eq!(check_destroy(leaf, 0, 0, 5, true), Ok(()));
        // Permanent refuses (tree.c:870-871).
        assert_eq!(
            check_destroy(leaf | CTLFLAG_PERMANENT, 0, 0, 5, true),
            Err((DestroyRefusal::Permanent, EPERM))
        );
        // Mount points are busy (:876-877).
        let remote =
            CTLTYPE_NODE | super::super::flag::CTLFLAG_PARENT | super::super::flag::CTLFLAG_REMOTE;
        assert_eq!(
            check_destroy(remote, 0, 0, 5, true),
            Err((DestroyRefusal::MountBusy, EBUSY))
        );
        // Function nodes refuse (:880-881).
        assert_eq!(
            check_destroy(CTLTYPE_NODE, 0, 0, 5, true),
            Err((DestroyRefusal::HasFunction, EPERM))
        );
        // Non-empty parents refuse (:884-885).
        let parent = CTLTYPE_NODE | super::super::flag::CTLFLAG_PARENT;
        assert_eq!(
            check_destroy(parent, 2, 0, 5, true),
            Err((DestroyRefusal::NotEmpty, ENOTEMPTY))
        );
        // Stale version / wrong name speak EINVAL like NetBSD won't (:889-898).
        assert_eq!(
            check_destroy(leaf, 0, 6, 5, true),
            Err((DestroyRefusal::VersionStale, EINVAL))
        );
        assert_eq!(
            check_destroy(leaf, 0, 0, 5, false),
            Err((DestroyRefusal::NameMismatch, EINVAL))
        );
    }

    #[test]
    fn test_remove_deltas() {
        // Dynamic + owned desc frees two objects and shrinks csize (:793-829).
        assert_eq!(
            remove_delta(true, true),
            RemoveDelta {
                free_desc: true,
                free_dynode: true,
                csize_dec: 1,
                objects_dec: 2
            }
        );
        // Static zeroing frees nothing but the desc.
        assert_eq!(
            remove_delta(false, true),
            RemoveDelta {
                free_desc: true,
                free_dynode: false,
                csize_dec: 0,
                objects_dec: 1
            }
        );
        assert_eq!(
            remove_delta(false, false),
            RemoveDelta {
                free_desc: false,
                free_dynode: false,
                csize_dec: 0,
                objects_dec: 0
            }
        );
    }
}
