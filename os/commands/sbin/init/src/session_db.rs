//! Session database: pid → session index.
//!
//! Covers `minix3/sbin/init/init.c:1021-1096`. Berkeley DB in-memory
//! hash (`dbopen(NULL, HASH)`) is replaced by `HashMap` (ARCH A-1:
//! same behaviour, no libdb dependency).
//! Design contract: `.design/08-design.v1.md §1.1-§1.2`.

use std::collections::HashMap;

/// Database error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbError {
    OpenFailed,
}

/// Session database boundary.
pub trait SessionDb {
    fn open(&mut self) -> Result<(), DbError>;
    fn insert(&mut self, pid: i32, session_index: usize);
    fn remove(&mut self, pid: i32) -> bool;
    fn find(&self, pid: i32) -> Option<usize>;
    fn is_open(&self) -> bool;
}

/// Live `HashMap` implementation.
#[derive(Debug, Default)]
pub struct HashMapDb {
    open: bool,
    map: HashMap<i32, usize>,
}

impl SessionDb for HashMapDb {
    fn open(&mut self) -> Result<(), DbError> {
        // C: close old table, open fresh memory hash (init.c:1025-1030).
        self.map.clear();
        self.open = true;
        Ok(())
    }

    fn insert(&mut self, pid: i32, session_index: usize) {
        if !self.open {
            return;
        }
        self.map.insert(pid, session_index);
    }

    fn remove(&mut self, pid: i32) -> bool {
        self.map.remove(&pid).is_some()
    }

    fn find(&self, pid: i32) -> Option<usize> {
        if !self.open {
            return None;
        }
        self.map.get(&pid).copied()
    }

    fn is_open(&self) -> bool {
        self.open
    }
}

/// Fake database with scripted open failures for tests.
#[derive(Debug, Default)]
pub struct FakeDb {
    pub fail_open: bool,
    inner: HashMapDb,
}

impl SessionDb for FakeDb {
    fn open(&mut self) -> Result<(), DbError> {
        if self.fail_open {
            return Err(DbError::OpenFailed);
        }
        self.inner.open()
    }

    fn insert(&mut self, pid: i32, session_index: usize) {
        self.inner.insert(pid, session_index);
    }

    fn remove(&mut self, pid: i32) -> bool {
        self.inner.remove(pid)
    }

    fn find(&self, pid: i32) -> Option<usize> {
        self.inner.find(pid)
    }

    fn is_open(&self) -> bool {
        self.inner.is_open()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_insert_find() {
        let mut db = HashMapDb::default();
        db.open().unwrap();
        db.insert(42, 1);
        assert_eq!(db.find(42), Some(1));
    }

    #[test]
    fn test_find_missing_none() {
        let mut db = HashMapDb::default();
        db.open().unwrap();
        assert_eq!(db.find(99), None);
    }

    #[test]
    fn test_remove_deletes() {
        let mut db = HashMapDb::default();
        db.open().unwrap();
        db.insert(7, 0);
        assert!(db.remove(7));
        assert_eq!(db.find(7), None);
    }

    #[test]
    fn test_reopen_clears() {
        let mut db = HashMapDb::default();
        db.open().unwrap();
        db.insert(1, 0);
        db.open().unwrap();
        assert_eq!(db.find(1), None);
    }

    #[test]
    fn test_null_db_fake() {
        let mut db = FakeDb {
            fail_open: true,
            ..Default::default()
        };
        assert_eq!(db.open(), Err(DbError::OpenFailed));
    }
}
