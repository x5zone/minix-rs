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

pub(crate) struct FdRefEntry {
    pub fd: i32,
    pub dev: u64,
    pub ino: u64,
    pub may_close: bool,
    pub refcount: u32,
}

pub(crate) struct PendingFdClose {
    pub fd: i32,
    pub dev: u64,
    pub ino: u64,
}

struct FdRefTableInner {
    entries: BTreeMap<u32, FdRefEntry>,
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
                next_id: 1,
            }),
        }
    }

    pub(crate) fn get_global() -> &'static FdRefTable {
        static FDREF_TABLE: FdRefTable = FdRefTable::new_const();
        &FDREF_TABLE
    }

    fn inner(&self) -> &mut FdRefTableInner {
        // SAFETY: single-threaded event loop model; no concurrent access.
        unsafe { &mut *self.inner.get() }
    }

    pub(crate) fn create(
        &self,
        fd: i32,
        dev: u64,
        ino: u64,
        may_close: bool,
    ) -> u32 {
        let inner = self.inner();
        let id = inner.next_id;
        inner.next_id += 1;
        inner.entries.insert(id, FdRefEntry {
            fd,
            dev,
            ino,
            may_close,
            refcount: 0,
        });
        id
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
            if entry.may_close {
                Some(PendingFdClose {
                    fd: entry.fd,
                    dev: entry.dev,
                    ino: entry.ino,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(crate) fn find_by_dev_ino(&self, dev: u64, ino: u64) -> Option<u32> {
        self.inner().entries.iter()
            .find(|(_, e)| e.dev == dev && e.ino == ino)
            .map(|(id, _)| *id)
    }

    pub(crate) fn get(&self, id: u32) -> Option<&FdRefEntry> {
        // SAFETY: returning a shared reference; the UnsafeCell borrow is
        // short-lived and the returned reference borrows from the inner map.
        // Under the single-threaded event loop model this is safe because
        // no mutable access occurs while the reference is live.
        unsafe { (*self.inner.get()).entries.get(&id) }
    }

    pub(crate) fn len(&self) -> usize {
        self.inner().entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner().entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_test_table() -> &'static FdRefTable {
        static TEST_TABLE: FdRefTable = FdRefTable::new_const();
        &TEST_TABLE
    }

    #[test]
    fn test_fdref_create_and_get() {
        let table = new_test_table();
        let id = table.create(3, 100, 200, true);

        let entry = table.get(id).unwrap();
        assert_eq!(entry.fd, 3);
        assert_eq!(entry.dev, 100);
        assert_eq!(entry.ino, 200);
        assert!(entry.may_close);
        assert_eq!(entry.refcount, 0);
    }

    #[test]
    fn test_fdref_ref_deref_cycle() {
        let table = new_test_table();
        let id = table.create(3, 100, 200, true);

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
    fn test_fdref_no_may_close() {
        let table = new_test_table();
        let id = table.create(3, 100, 200, false);

        table.ref_entry(id);
        let result = table.deref_entry(id);
        assert!(result.is_none());
        assert!(table.get(id).is_none());
    }

    #[test]
    fn test_fdref_dedup() {
        let table = new_test_table();
        let id1 = table.create(3, 100, 200, true);

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
}
