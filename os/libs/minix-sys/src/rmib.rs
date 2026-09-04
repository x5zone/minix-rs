//! Remote MIB (RMIB) client: pure bookkeeping for mounted subtrees.
//!
//! A service that owns part of the sysctl name space (IPC, the network
//! stacks) keeps its subtree at home and lets the MIB service forward
//! matching queries. This module holds the parts of that contract that
//! need no IPC: the subtree slot table, the sorted sparse-node lookup,
//! the caller context, and the stack-buffer update policy. Sending the
//! registration, answering forwarded calls, and copying through grants
//! stay with the transport layer: they need `asynsend3`/`sendrec` and
//! the grant machinery, none of which exists here yet.
//!
//! C: `minix/lib/libsys/rmib.c` (1089 lines) and
//! `minix/include/minix/rmib.h` (188 lines).
//!
//! 22-mib-rmib-client.md.

use minix_types::CTLFLAG_ROOT;

/// How many subtrees one service can mount.
///
/// C: `RMIB_MAX_SUBTREES 16` — rmib.c:47. Raising it is safe, says the
/// comment; sixteen is plenty in practice. A root id is an index into
/// this table.
pub const RMIB_MAX_SUBTREES: usize = 16;

/// Stack buffer size for field updates.
///
/// C: `RMIB_STACKBUF 257` — rmib.c:36-44. Updates that fit go on the
/// stack; the extra byte leaves room for a missing string terminator.
pub const RMIB_STACKBUF: usize = 257;

/// Caller has superuser privileges.
///
/// C: `RMIB_FLAG_AUTH 0x1` — rmib.h:39. The header admits this flag
/// travels on the wire but has no shared definition with the MIB
/// service yet (:32-38); one flag only, so not urgent.
pub const RMIB_FLAG_AUTH: u32 = 0x1;

/// Sparse-node marker, borrowed from an unused NetBSD flag.
///
/// C: `CTLFLAG_SPARSE` is `CTLFLAG_ROOT` — rmib.h:57. Sparse nodes
/// trade lookup speed for memory: instead of a dense child array they
/// keep `{id, child}` pairs, sorted ascending, searched linearly.
pub const CTLFLAG_SPARSE: u32 = CTLFLAG_ROOT;

/// Whether the caller runs privileged.
///
/// C: `call_flags & RMIB_FLAG_AUTH` — rmib.c request handling. The MIB
/// service snapshots the bit into the forwarded call; the handler
/// trusts it.
pub const fn is_authed(call_flags: u32) -> bool {
    call_flags & RMIB_FLAG_AUTH != 0
}

/// Whether a field update may use the stack buffer.
///
/// C: by policy, non-root users may not update fields past the stack
/// buffer at all — rmib.c:36-44. Root may always proceed (heap past
/// the buffer); anyone else is limited to the buffer.
pub const fn stack_update_allowed(is_root: bool, field_size: usize) -> bool {
    is_root || field_size <= RMIB_STACKBUF
}

/// Find a child id in a sorted sparse list.
///
/// C: sparse (`SNODE`) lookup — rmib.h:48-56. Linear scan over ids
/// sorted ascending; duplicates, null children, and zero-flagged nodes
/// are forbidden by construction (the caller upholds the invariant,
/// see [`validate_sparse`]).
pub fn sparse_find(ids: &[u32], want: u32) -> Option<usize> {
    ids.iter().position(|&id| id == want)
}

/// Check a sparse list: ascending, unique, and all flagged.
///
/// C: the construction rules — rmib.h:52-55. Returns the first problem:
/// out-of-order (or duplicate) ids, or a node without flags. An empty
/// list is valid.
pub fn validate_sparse(ids: &[u32], has_flags: &[bool]) -> Result<(), SparseError> {
    if ids.len() != has_flags.len() {
        return Err(SparseError::LengthMismatch);
    }
    let mut prev: Option<u32> = None;
    for (i, &id) in ids.iter().enumerate() {
        if let Some(p) = prev {
            if id <= p {
                return Err(SparseError::NotSorted);
            }
        }
        if !has_flags[i] {
            return Err(SparseError::Unflagged);
        }
        prev = Some(id);
    }
    Ok(())
}

/// What is wrong with a sparse list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseError {
    /// Id and flag slices differ in length.
    LengthMismatch,
    /// Ids not strictly ascending (duplicates included).
    NotSorted,
    /// A node carries no flags.
    Unflagged,
}

/// Which subtrees this service has mounted.
///
/// C: `rnodes[]` — rmib.c:52-57. Slot index doubles as the root id on
/// the wire. Registering claims the first free slot; deregistering
/// frees it; a MIB restart needs re-registration of every live slot
/// (`rmib_reregister`, :973-983); `rmib_reset` clears everything
/// without talking to anyone and exists for tests only (:986-994).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountTable {
    used: [bool; RMIB_MAX_SUBTREES],
}

/// A claimed slot: the root id for this subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot(pub u8);

impl MountTable {
    /// Empty table: nothing mounted.
    pub const fn new() -> MountTable {
        MountTable {
            used: [false; RMIB_MAX_SUBTREES],
        }
    }

    /// Claim the first free slot, if any.
    pub fn claim(&mut self) -> Option<Slot> {
        for (i, u) in self.used.iter_mut().enumerate() {
            if !*u {
                *u = true;
                return Some(Slot(i as u8));
            }
        }
        None
    }

    /// Free a claimed slot. Unknown ids are ignored: deregistration
    /// answers silence, never errors.
    pub fn release(&mut self, slot: Slot) {
        if (slot.0 as usize) < RMIB_MAX_SUBTREES {
            self.used[slot.0 as usize] = false;
        }
    }

    /// Whether this slot is currently claimed.
    pub const fn is_claimed(&self, slot: Slot) -> bool {
        if (slot.0 as usize) >= RMIB_MAX_SUBTREES {
            return false;
        }
        self.used[slot.0 as usize]
    }

    /// Drop every registration without telling anyone.
    ///
    /// C: `rmib_reset` — rmib.c:986-994, test-only helper.
    pub fn clear(&mut self) {
        self.used = [false; RMIB_MAX_SUBTREES];
    }

    /// Live slots, in order: what re-registration re-sends.
    ///
    /// C: `rmib_reregister` walks the table and re-sends every live
    /// root — rmib.c:973-983.
    pub fn live_slots(&self) -> [Option<Slot>; RMIB_MAX_SUBTREES] {
        let mut out = [None; RMIB_MAX_SUBTREES];
        for (i, u) in self.used.iter().enumerate() {
            if *u {
                out[i] = Some(Slot(i as u8));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_table_limits() {
        // Sixteen slots, stack buffer 257, auth bit 0x1.
        assert_eq!(RMIB_MAX_SUBTREES, 16);
        assert_eq!(RMIB_STACKBUF, 257);
        assert_eq!(RMIB_FLAG_AUTH, 0x1);
        assert_eq!(CTLFLAG_SPARSE, CTLFLAG_ROOT);
        assert!(is_authed(0x1));
        assert!(!is_authed(0x0));
        // Root may update anything; others stop at the buffer.
        assert!(stack_update_allowed(true, 10_000));
        assert!(stack_update_allowed(false, 257));
        assert!(!stack_update_allowed(false, 258));
    }

    #[test]
    fn test_sparse_lists() {
        // Sorted lookup finds, misses miss.
        let ids = [3u32, 7, 42];
        assert_eq!(sparse_find(&ids, 7), Some(1));
        assert_eq!(sparse_find(&ids, 8), None);
        assert_eq!(sparse_find(&[], 1), None);
        // Construction rules: ascending, unique, flagged.
        assert_eq!(validate_sparse(&[3, 7, 42], &[true, true, true]), Ok(()));
        assert_eq!(validate_sparse(&[], &[]), Ok(()));
        assert_eq!(
            validate_sparse(&[3, 3, 42], &[true, true, true]),
            Err(SparseError::NotSorted)
        );
        assert_eq!(
            validate_sparse(&[7, 3], &[true, true]),
            Err(SparseError::NotSorted)
        );
        assert_eq!(
            validate_sparse(&[3, 7], &[true, false]),
            Err(SparseError::Unflagged)
        );
        assert_eq!(
            validate_sparse(&[3], &[true, true]),
            Err(SparseError::LengthMismatch)
        );
    }

    #[test]
    fn test_mount_table() {
        let mut t = MountTable::new();
        // First claim is slot zero (the root id on the wire).
        assert_eq!(t.claim(), Some(Slot(0)));
        assert_eq!(t.claim(), Some(Slot(1)));
        assert!(t.is_claimed(Slot(0)));
        assert!(!t.is_claimed(Slot(2)));
        // Release and reclaim reuse the slot.
        t.release(Slot(0));
        assert!(!t.is_claimed(Slot(0)));
        assert_eq!(t.claim(), Some(Slot(0)));
        // Unknown releases are ignored, never errors.
        t.release(Slot(200));
        // Fill the table: the seventeenth claim fails.
        for _ in 2..RMIB_MAX_SUBTREES {
            assert!(t.claim().is_some());
        }
        assert_eq!(t.claim(), None);
        // Re-registration re-sends every live slot, in order.
        let live = t.live_slots();
        assert!(live.iter().all(|s| s.is_some()));
        // Test reset clears everything.
        t.clear();
        assert!(live_slots_empty(&t));
    }

    fn live_slots_empty(t: &MountTable) -> bool {
        t.live_slots().iter().all(|s| s.is_none())
    }
}
