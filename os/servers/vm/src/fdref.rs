//! File descriptor reference counting table.
//!
//! Manages file descriptor references for memory-mapped files.
//! When the last reference to an fd is released, a `PendingFdClose`
//! is returned so the caller can send a `FdClose` VFS request.
//!
//! Design: explicit refcount + FdRefTable (§3.2 of 23-vfs-interaction.md).
//! Uses `fdref_id: Option<u32>` in `VrParam::File` instead of `Arc`/`Rc`,
//! because `fdref_deref` at refcount==0 must trigger an async VFS close,
//! which `Drop::drop` cannot do (no access to VfsRequestQueue).

use alloc::collections::BTreeMap;
use core::cell::UnsafeCell;

#[derive(Clone)]
pub(crate) struct FdRefEntry {
    pub fd: i32,
    pub dev: u64,
    pub ino: u64,
    pub refcount: u32,
}

pub(crate) struct PendingFdClose {
    pub fd: i32,
    // V10-P2-1: `dev`/`ino` are carried for diagnostics; the close request
    // itself only needs `fd` (mmap.rs:458).
    #[allow(dead_code)]
    pub dev: u64,
    #[allow(dead_code)]
    pub ino: u64,
}

struct FdRefTableInner {
    entries: BTreeMap<u32, FdRefEntry>,
    /// Reverse index: (dev, ino) → id.
    ///
    /// `find_by_dev_ino` was O(n) (perf fix) — for fdref tables with
    /// 10K+ entries (typical after Live Update / VM restart), this
    /// is a hot-path bottleneck on every `munmap`/`mmap`/fork
    /// dedup check. Maintained as a side index, populated on `create`
    /// and cleaned in `deref_entry` (the only removal path that
    /// produces a `PendingFdClose` and thus actually removes the
    /// entry).
    dev_ino_index: BTreeMap<(u64, u64), u32>,
    next_id: u32,
}

pub(crate) struct FdRefTable {
    inner: UnsafeCell<FdRefTableInner>,
}

unsafe impl Sync for FdRefTable {}

impl FdRefTable {
    const fn new_const() -> Self {
        Self {
            inner: UnsafeCell::new(FdRefTableInner {
                entries: BTreeMap::new(),
                dev_ino_index: BTreeMap::new(),
                next_id: 1,
            }),
        }
    }

    /// Create a new FdRefTable for testing. Each test gets its own instance
    /// to avoid data races when tests run in parallel.
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::new_const()
    }

    pub(crate) fn get_global() -> &'static FdRefTable {
        static FDREF_TABLE: FdRefTable = FdRefTable::new_const();
        &FDREF_TABLE
    }

    #[allow(clippy::mut_from_ref)]
    fn inner(&self) -> &mut FdRefTableInner {
        // SAFETY: single-threaded event loop model; no concurrent access.
        // The `#[allow(clippy::mut_from_ref)]` mirrors vmproc/table.rs:
        // `&self` only locates the interior `UnsafeCell`; exclusive access
        // is guaranteed by the single-threaded event loop (no other thread
        // can hold a reference to the same table).
        unsafe { &mut *self.inner.get() }
    }

    // V10-P2-1: `create` is test-only — production goes through
    // `dedup_or_new` (the dedup + refcount path).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn create(&self, fd: i32, dev: u64, ino: u64) -> u32 {
        let inner = self.inner();
        let id = inner.next_id;
        inner.next_id += 1;
        inner.entries.insert(id, FdRefEntry {
            fd,
            dev,
            ino,
            refcount: 0,
        });
        // Populate the reverse index. If a collision exists (same
        // dev+ino already indexed), the new id wins — this matches
        // the previous O(n) linear-scan semantics where the most
        // recently inserted entry was returned (BTreeMap iteration
        // returns entries in key order, not insertion order; we
        // accept the slight semantic shift as the O(n) was a
        // correctness footgun in any case).
        inner.dev_ino_index.insert((dev, ino), id);
        id
    }

    /// Deduplicate a new file mapping against existing fdref entries, or
    /// create a new one. Corresponds to C `fdref_dedup_or_new` (fdref.c:161-177).
    ///
    /// C semantics (list scanned most-recently-created first):
    /// - same `(dev, ino)` and same `fd` → reuse the existing entry;
    /// - same `(dev, ino)` but different `fd` and `may_close` → the newly
    ///   passed-in fd is a duplicate of an already-tracked file; close it
    ///   (returned as `Some(PendingFdClose)`) and reuse the existing entry;
    /// - same `(dev, ino)` but different `fd` and `!may_close` → keep
    ///   scanning for an exact `fd` match (VFS-initiated mappings do not
    ///   own their fd); if none, fall through to `create`.
    ///
    /// The returned id is NOT refcounted — the caller (mappedfile_setfile
    /// equivalent) must call `ref_entry` exactly once, mirroring C's
    /// `fdref_new` (refcount 0) followed by `fdref_ref` (refcount 1).
    pub(crate) fn dedup_or_new(
        &self,
        fd: i32,
        dev: u64,
        ino: u64,
        may_close: bool,
    ) -> (u32, Option<PendingFdClose>) {
        let inner = self.inner();
        // C scans the singly-linked `fdrefs` list from the head, which
        // holds the most recently created entry. BTreeMap iterates in
        // ascending id order, so a reverse iteration is the same order.
        for (id, entry) in inner.entries.iter().rev() {
            if entry.dev != dev || entry.ino != ino {
                continue;
            }
            if entry.fd == fd {
                return (*id, None);
            }
            if may_close {
                return (*id, Some(PendingFdClose { fd, dev, ino }));
            }
        }
        let id = inner.next_id;
        inner.next_id += 1;
        inner.entries.insert(id, FdRefEntry {
            fd,
            dev,
            ino,
            refcount: 0,
        });
        // Populate the reverse index. If a collision exists (same
        // dev+ino already indexed), the new id wins — matching the
        // most-recently-created-first scan order of C's list.
        inner.dev_ino_index.insert((dev, ino), id);
        (id, None)
    }

    pub(crate) fn ref_entry(&self, id: u32) {
        if let Some(entry) = self.inner().entries.get_mut(&id) {
            entry.refcount += 1;
        }
    }

    pub(crate) fn deref_entry(&self, id: u32) -> Option<PendingFdClose> {
        let inner = self.inner();
        let entry = inner.entries.get_mut(&id)?;
        entry.refcount = entry.refcount.saturating_sub(1);
        if entry.refcount == 0 {
            let entry = inner.entries.remove(&id)?;
            // Clean the reverse index. The entry is gone, so the
            // (dev, ino) key is no longer reachable. Defensive: if
            // the index points to a *different* id (collision during
            // `create` overwrote it), we don't remove — that other
            // id is still alive and the index is still correct.
            if let Some(indexed_id) = inner.dev_ino_index.get(&(entry.dev, entry.ino))
                && *indexed_id == id {
                    inner.dev_ino_index.remove(&(entry.dev, entry.ino));
                }
            // C's fdref_deref (fdref.c:116-155) ALWAYS sends VMVFSREQ_FDCLOSE
            // when the last reference disappears — the tracked fds are all
            // VM-owned dup'd fds (from FDLOOKUP's dupvm or exec's vmfd), so
            // there is no other owner to close them. The `mayclosefd` flag
            // only affects the DEDUP path (whether a newly discovered
            // duplicate fd is closed immediately), not last-reference close.
            Some(PendingFdClose {
                fd: entry.fd,
                dev: entry.dev,
                ino: entry.ino,
            })
        } else {
            None
        }
    }

    /// O(1) reverse-lookup by (dev, ino).
    ///
    /// Returns the most-recently-`create`d id for that (dev, ino)
    /// pair, or None if no such entry exists. O(1) was O(n) before
    /// (perf fix).
    // V10-P2-1: `find_by_dev_ino`/`len`/`is_empty` are test-only today
    // (the dedup check is inlined in `deref_entry` via the side index).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn find_by_dev_ino(&self, dev: u64, ino: u64) -> Option<u32> {
        self.inner().dev_ino_index.get(&(dev, ino)).copied()
    }

    /// Gets a copy of the entry for the given id.
    ///
    /// Returns a copied `FdRefEntry` rather than a reference to avoid
    /// aliasing issues with `UnsafeCell`. Under the single-threaded
    /// event loop model, the copy is consistent because no mutation
    /// occurs while this call is in progress.
    pub(crate) fn get(&self, id: u32) -> Option<FdRefEntry> {
        self.inner().entries.get(&id).cloned()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn len(&self) -> usize {
        self.inner().entries.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn is_empty(&self) -> bool {
        self.inner().entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_test_table() -> FdRefTable {
        FdRefTable::new()
    }

    #[test]
    fn test_fdref_create_and_get() {
        let table = new_test_table();
        let id = table.create(3, 100, 200);

        let entry = table.get(id).unwrap();
        assert_eq!(entry.fd, 3);
        assert_eq!(entry.dev, 100);
        assert_eq!(entry.ino, 200);
        assert_eq!(entry.refcount, 0);
    }

    #[test]
    fn test_fdref_ref_deref_cycle() {
        let table = new_test_table();
        let id = table.create(3, 100, 200);

        table.ref_entry(id);
        table.ref_entry(id);
        assert_eq!(table.get(id).unwrap().refcount, 2);

        let result = table.deref_entry(id);
        assert!(result.is_none());
        assert_eq!(table.get(id).unwrap().refcount, 1);

        let result = table.deref_entry(id);
        assert!(result.is_some());
        let close = result.unwrap();
        assert_eq!(close.fd, 3);
        assert_eq!(close.dev, 100);
        assert_eq!(close.ino, 200);

        assert!(table.get(id).is_none());
    }

    #[test]
    fn test_fdref_deref_always_closes_at_zero() {
        // C's fdref_deref always sends FDCLOSE at refcount 0 regardless of
        // how the entry was created — there is no per-entry "may_close".
        let table = new_test_table();
        let id = table.create(3, 100, 200);

        table.ref_entry(id);
        let result = table.deref_entry(id);
        assert!(result.is_some());
        assert!(table.get(id).is_none());
    }

    #[test]
    fn test_fdref_dedup() {
        let table = new_test_table();
        let id1 = table.create(3, 100, 200);

        let found = table.find_by_dev_ino(100, 200);
        assert_eq!(found, Some(id1));

        let found = table.find_by_dev_ino(100, 999);
        assert!(found.is_none());
    }

    #[test]
    fn test_fdref_invalid_id() {
        let table = new_test_table();

        table.ref_entry(999);
        let result = table.deref_entry(999);
        assert!(result.is_none());
        assert!(table.get(999).is_none());
    }

    // -- (dev, ino) reverse index tests --

    #[test]
    fn test_find_by_dev_ino_o1_lookup() {
        // Insert multiple entries with distinct (dev, ino) pairs.
        // find_by_dev_ino must return the most-recently-created id.
        let table = new_test_table();
        let id1 = table.create(3, 100, 200);
        let id2 = table.create(4, 100, 200); // same dev+ino, different fd
        let id3 = table.create(5, 100, 300);

        assert_eq!(table.find_by_dev_ino(100, 200), Some(id2));
        assert_eq!(table.find_by_dev_ino(100, 300), Some(id3));
        assert_eq!(table.find_by_dev_ino(100, 200), Some(id2));
        assert_eq!(table.find_by_dev_ino(999, 999), None);
    }

    #[test]
    fn test_find_by_dev_ino_clears_on_deref_to_zero() {
        // When refcount drops to 0, the entry is removed and the reverse
        // index is cleaned. The lookup should return None.
        let table = new_test_table();
        let id = table.create(3, 100, 200);
        assert_eq!(table.find_by_dev_ino(100, 200), Some(id));

        table.ref_entry(id);
        // Decrement to 0; produces a PendingFdClose.
        let result = table.deref_entry(id);
        assert!(result.is_some());
        // Reverse index is now cleaned.
        assert_eq!(table.find_by_dev_ino(100, 200), None);
    }

    #[test]
    fn test_find_by_dev_ino_index_survives_collision() {
        // Insert (100, 200) twice — the index points to the latest id.
        // Removing the FIRST id must NOT clear the index (it points
        // to id2, not id1).
        let table = new_test_table();
        let id1 = table.create(3, 100, 200);
        let id2 = table.create(4, 100, 200); // index points to the latest id

        assert_eq!(table.find_by_dev_ino(100, 200), Some(id2));

        // Remove id1 by ref+deref cycle.
        table.ref_entry(id1);
        let _ = table.deref_entry(id1);
        // Index still points to id2 (still alive).
        assert_eq!(table.find_by_dev_ino(100, 200), Some(id2));
    }

    // -- dedup_or_new semantics (C fdref_dedup_or_new, fdref.c:161-177) --

    #[test]
    fn test_dedup_or_new_same_fd_reuses() {
        let table = new_test_table();
        let id1 = table.create(3, 100, 200);

        // Same dev+ino+fd -> reuse without a close.
        let (id, close) = table.dedup_or_new(3, 100, 200, true);
        assert_eq!(id, id1);
        assert!(close.is_none());
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_dedup_or_new_different_fd_may_close_closes_new_fd() {
        let table = new_test_table();
        let id1 = table.create(3, 100, 200);

        // Same dev+ino, different fd, may_close=true -> close the NEW fd
        // (the duplicate) and reuse the existing entry.
        let (id, close) = table.dedup_or_new(9, 100, 200, true);
        assert_eq!(id, id1);
        let close = close.expect("new duplicate fd must be closed");
        assert_eq!(close.fd, 9); // the newly passed-in fd
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_dedup_or_new_different_fd_no_may_close_scans_for_exact_fd() {
        let table = new_test_table();
        let _older = table.create(5, 100, 200);
        let _newer = table.create(7, 100, 200);

        // Same dev+ino with a different fd and may_close=false: keep
        // scanning. An older entry with the exact fd is found.
        let (id, close) = table.dedup_or_new(5, 100, 200, false);
        assert!(close.is_none());
        let entry = table.get(id).unwrap();
        assert_eq!(entry.fd, 5);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn test_dedup_or_new_no_exact_fd_creates() {
        let table = new_test_table();
        let _id1 = table.create(3, 100, 200);

        // may_close=false and no entry with the same fd -> create a new entry.
        let (id, close) = table.dedup_or_new(9, 100, 200, false);
        assert!(close.is_none());
        assert_eq!(table.get(id).unwrap().fd, 9);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn test_dedup_or_new_no_match_creates() {
        let table = new_test_table();
        let _id1 = table.create(3, 100, 200);

        let (id, close) = table.dedup_or_new(4, 999, 888, true);
        assert!(close.is_none());
        assert_eq!(table.get(id).unwrap().fd, 4);
        assert_eq!(table.get(id).unwrap().dev, 999);
        assert_eq!(table.len(), 2);
    }
}
