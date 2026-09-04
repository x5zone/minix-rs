//! Mount verdicts: path policy, flag windows, target kinds, restore.
//!
//! Mirrors the pure halves of `mib_mount` / `mib_unmount`
//! (`tree.c:1543-1842`). Walking the path, fetching the remote root's
//! name/description, allocating, and linking are walker/arena effects;
//! every *gate* — path length, flag window, per-node policy, target
//! kind, restore math — is judged here.
//!
//! 12-mib-remote-subtrees.md.

use minix_types::{
    CTLFLAG_HIDDEN, CTLFLAG_PERMANENT, CTLFLAG_PRIVATE, CTLFLAG_READONLY, CTLFLAG_READWRITE,
    CTLTYPE_NODE, EINVAL, EPERM, SYSCTL_FLAGMASK, SYSCTL_VERSION, sysctl_flags, sysctl_type,
    sysctl_vers,
};

use super::flag::{CTLFLAG_PARENT, CTLFLAG_REMOTE};

/// Mount policy outcomes for the request head.
///
/// C: `mib_mount` parameter gates — tree.c:1572-1598.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountHead {
    /// Proceed to the path walk.
    Proceed,
    /// Top-level mount forbidden. C: `miblen < 2` → `EPERM` (:1572-1576 —
    /// the only security-like restriction: no taking over `kern` whole).
    TooShort,
    /// Flag window violated. C: `:1583-1591` → `EINVAL`.
    BadFlags,
    /// Child window impossible. C: `:1593-1598` → `EINVAL`.
    BadWindow,
}

/// Judge the mount request head: path length, flag window, child window.
///
/// Flag window (`:1583-1586`): version must be `SYSCTL_VERSION`, type
/// `NODE`, and no bits outside `READONLY|READWRITE|PERMANENT|HIDDEN`.
/// Child window: `csize` fits the 12-bit remote lanes and `clen <= csize`
/// (`:1593` — wider would not survive the `RemotePack`, 03).
pub const fn check_head(miblen: u32, flags: u32, csize: u32, clen: u32) -> MountHead {
    if miblen < 2 {
        return MountHead::TooShort;
    }
    if sysctl_vers(flags) != SYSCTL_VERSION
        || sysctl_type(flags) != CTLTYPE_NODE
        || sysctl_flags(flags)
            & !(CTLFLAG_READONLY | CTLFLAG_READWRITE | CTLFLAG_PERMANENT | CTLFLAG_HIDDEN)
            != 0
    {
        return MountHead::BadFlags;
    }
    if csize > 4096 || clen > csize {
        return MountHead::BadWindow;
    }
    MountHead::Proceed
}

/// Head refusal codes, in C order. C: `:1575`, `:1590`, `:1597`.
pub const fn head_code(v: MountHead) -> Result<(), i32> {
    match v {
        MountHead::Proceed => Ok(()),
        MountHead::TooShort => Err(EPERM),
        MountHead::BadFlags => Err(EINVAL),
        MountHead::BadWindow => Err(EINVAL),
    }
}

/// Whether one path node may be walked through.
///
/// C: `:1629-1637` → `EPERM` otherwise. Every node up to the parent must
/// be a real local non-private node-type node: real (`PARENT`), local
/// (not `REMOTE`), non-private (a service must not intercept writes to
/// privileged nodes, :1604-1607), node-typed. Meta-ids in paths refuse
/// earlier with `EINVAL` (`:1613-1619` — checked per component before
/// lookup; the mount-point slot itself at `:1643-1647`).
pub const fn path_node_ok(flags: u32) -> bool {
    sysctl_type(flags) == CTLTYPE_NODE
        && flags & CTLFLAG_PARENT != 0
        && flags & (CTLFLAG_REMOTE | CTLFLAG_PRIVATE) == 0
}

/// Whether a path component id is admissible (no meta-ids).
/// C: `:1613-1619`, `:1643-1647` → `EINVAL`.
pub const fn path_id_ok(id: i32) -> bool {
    id >= 0
}

/// Mount target kind after the walk: cover or create.
///
/// C: `:1654-1767`. An existing node that passes the match becomes an
/// obscuring mount (`TargetVerdict::Obscure`); a miss becomes a temporary
/// node (name/description fetched from the service first, :1702-1708).
/// The kind rides inside [`TargetVerdict`], not a parallel enum — one
/// decision, one type.
///
/// Judge an existing target node for obscuring.
///
/// C: `:1659-1678`. Flags must match exactly (`FLAGS | PARENT`,
/// `:1660-1661` — the request carries no `PARENT` bit of its own, the
/// target supplies it); dynamic children present block the mount
/// (`EBUSY`, :1673-1678 — unmount could not restore them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetVerdict {
    /// Mount over it.
    Obscure,
    /// Flag mismatch. C: `:1666` → `EPERM`.
    Mismatch,
    /// Dynamic children present. C: `:1677` → `EBUSY`.
    Busy,
}

/// Judge the existing target (`node_size` vs `node_csize` detects dynamic
/// children — static-only tables have them equal).
pub const fn check_target(
    node_flags: u32,
    req_flags: u32,
    node_size: u32,
    node_csize: u32,
) -> TargetVerdict {
    if sysctl_type(node_flags) != CTLTYPE_NODE
        || (node_flags & SYSCTL_FLAGMASK) != ((req_flags & SYSCTL_FLAGMASK) | CTLFLAG_PARENT)
    {
        return TargetVerdict::Mismatch;
    }
    if node_size != node_csize {
        return TargetVerdict::Busy;
    }
    TargetVerdict::Obscure
}

/// Temp-node allocation size: header + name + description + NUL.
///
/// C: `sizeof(*dynode) + namelen + desclen + 1` — tree.c:1739. Unlike
/// user-created nodes (08: description set later), temp mounts embed the
/// fetched description in the same block (03 §2.2 shape).
/// Allocation failure here speaks **`ENOMEM`** (`:1741-1744`) — the one
/// allocation in MIB allowed to (mount runs before any old-length
/// contract exists, so no `ENOMEM` confusion is possible, 02 §2.7).
pub const fn temp_alloc_size(header: usize, namelen: usize, desclen: usize) -> usize {
    header + namelen + desclen + 1
}

/// Restore math for an unmounted obscuring node.
///
/// C: `mib_unmount` restore arm — tree.c:1804-1823. `REMOTE` clears,
/// `csize` resets to the static size, `clen` recounts live static slots
/// (dynamic children cannot exist — mount refused them, :1673-1678),
/// the dynamic list drops (`NULL`), versions bump up the path.
pub const fn recount_clen(static_flags: &[u32], csize: u32) -> u32 {
    let mut clen = 0;
    let mut id = 0;
    while id < csize {
        if (id as usize) < static_flags.len() && static_flags[id as usize] != 0 {
            clen += 1;
        }
        id += 1;
    }
    clen
}

/// Whether an unmount restores (obscuring) or frees (temporary).
///
/// C: `:1804` — `PARENT` set means a preexisting node hides underneath.
pub const fn is_obscuring(flags: u32) -> bool {
    flags & CTLFLAG_PARENT != 0
}

/// Whether a remote-role flag word is well-formed for unmount entry.
///
/// C: `:1796-1797` asserts (`NODE` type, `REMOTE` set).
pub const fn unmount_entry_ok(flags: u32) -> bool {
    sysctl_type(flags) == CTLTYPE_NODE && flags & CTLFLAG_REMOTE != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{CTLFLAG_ANYWRITE, SYSCTL_VERS_1};

    #[test]
    fn test_check_head() {
        // Too short takes over nothing (:1572-1576).
        assert_eq!(head_code(check_head(1, 0x0100_0001, 4, 2)), Err(EPERM));
        // Flag window: version + NODE + subset (:1583-1591).
        let good = SYSCTL_VERS_1 | CTLTYPE_NODE | CTLFLAG_READONLY;
        assert_eq!(head_code(check_head(2, good, 4, 2)), Ok(()));
        assert_eq!(head_code(check_head(2, CTLTYPE_NODE, 4, 2)), Err(EINVAL));
        assert_eq!(
            head_code(check_head(2, good | CTLFLAG_ANYWRITE, 4, 2)),
            Err(EINVAL)
        );
        // Window: 12-bit lanes, clen <= csize (:1593-1598).
        assert_eq!(head_code(check_head(2, good, 4097, 0)), Err(EINVAL));
        assert_eq!(head_code(check_head(2, good, 4, 5)), Err(EINVAL));
        assert_eq!(head_code(check_head(2, good, 4096, 4096)), Ok(()));
    }

    #[test]
    fn test_path_policy() {
        // Real local non-private nodes only (:1629-1637).
        let ok = CTLTYPE_NODE | CTLFLAG_PARENT | CTLFLAG_READONLY;
        assert!(path_node_ok(ok));
        assert!(!path_node_ok(CTLTYPE_NODE | CTLFLAG_READONLY));
        assert!(!path_node_ok(ok | CTLFLAG_REMOTE));
        assert!(!path_node_ok(ok | minix_types::CTLFLAG_PRIVATE));
        assert!(!path_node_ok(minix_types::CTLTYPE_INT | CTLFLAG_PARENT));
        // No meta-ids in paths (:1613-1619, :1643-1647).
        assert!(path_id_ok(0));
        assert!(!path_id_ok(-2));
    }

    #[test]
    fn test_check_target() {
        // Exact flag match + no dynamic children (:1659-1678).
        let req = SYSCTL_VERS_1 | CTLTYPE_NODE | CTLFLAG_READONLY;
        let node = CTLTYPE_NODE | CTLFLAG_PARENT | CTLFLAG_READONLY;
        assert_eq!(check_target(node, req, 7, 7), TargetVerdict::Obscure);
        // Flag mismatch: node carries READWRITE the request lacks (:1660-1661
        // compares FLAGS words — PARENT supplied by the target side).
        assert_eq!(
            check_target(node | CTLFLAG_READWRITE, req, 7, 7),
            TargetVerdict::Mismatch
        );
        assert_eq!(check_target(node, req, 7, 9), TargetVerdict::Busy);
        // Temp allocation embeds the description (:1739); ENOMEM allowed here.
        assert_eq!(temp_alloc_size(48, 4, 10), 63);
    }

    #[test]
    fn test_unmount_restore() {
        // Obscuring restores (PARENT set); temp frees (:1804).
        let obsc = CTLTYPE_NODE | CTLFLAG_PARENT | CTLFLAG_REMOTE;
        assert!(is_obscuring(obsc));
        assert!(!is_obscuring(CTLTYPE_NODE | CTLFLAG_REMOTE));
        // clen recounts live static slots (:1813-1818).
        assert_eq!(recount_clen(&[1, 0, 3, 0, 5], 5), 3);
        assert_eq!(recount_clen(&[1, 0, 3, 0, 5], 3), 2);
        assert!(unmount_entry_ok(obsc));
        assert!(!unmount_entry_ok(CTLTYPE_NODE));
    }
}
