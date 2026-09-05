//! The process table trait: one interface, one implementation per stage.
//!
//! The C `ps` reads processes through kernel memory and system controls
//! (`minix3/bin/ps/ps.c` includes `kvm.h` and `sysctl.h` at lines 89 to
//! 92); its keyword table (`keyword.c`: `VAR3`/`VAR4`/`PID` macros naming
//! each column) decides what to print, not where processes come from. The
//! Rust side splits the same way: [`ProcessTable`] answers "which
//! processes exist and what are their numbers", the future column layer
//! formats them. Two implementations ship: [`EmptyTable`] (no processes —
//! the honest starting point) and [`SliceTable`] (an in memory row list
//! for tests).

use crate::ProcError;

/// One process row: identity numbers only. Owners, times, states, and
/// command lines are column layer concerns built on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessRow {
    /// Process identifier.
    pub pid: u32,
    /// Parent process identifier.
    pub ppid: u32,
    /// Real user identifier of the owner.
    pub uid: u32,
}

/// Read only process listing used by the process tools.
pub trait ProcessTable {
    /// Fill `out` with up to its length in rows, returning the count.
    /// Rows beyond capacity are silently left out (the caller sizes the
    /// buffer for its display; `ps` never needs more than a screenful
    /// plus a margin, and truncation here is paging, not data loss —
    /// the count return keeps it honest).
    fn list(&self, out: &mut [ProcessRow]) -> usize;
    /// Find one process by identifier, or `None`.
    fn find(&self, pid: u32) -> Option<ProcessRow>;
}

/// A table with no processes: every listing is empty, every lookup
/// misses with the search error number. Until the kernel table binding
/// lands, "no processes" is the only honest answer.
pub struct EmptyTable;

impl ProcessTable for EmptyTable {
    fn list(&self, _out: &mut [ProcessRow]) -> usize {
        0
    }

    fn find(&self, _pid: u32) -> Option<ProcessRow> {
        None
    }
}

/// A table over an in memory row list (up to 32 rows).
pub struct SliceTable<'a> {
    /// Process rows searched in order; the first identifier match wins.
    pub rows: &'a [ProcessRow],
}

impl ProcessTable for SliceTable<'_> {
    fn list(&self, out: &mut [ProcessRow]) -> usize {
        let count = self.rows.len().min(out.len());
        out[..count].copy_from_slice(&self.rows[..count]);
        count
    }

    fn find(&self, pid: u32) -> Option<ProcessRow> {
        self.rows.iter().find(|row| row.pid == pid).copied()
    }
}

/// Look up one process, translating the miss into the search error.
pub fn lookup<T: ProcessTable>(table: &T, pid: u32) -> Result<ProcessRow, ProcError> {
    table.find(pid).ok_or(ProcError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: [ProcessRow; 3] = [
        ProcessRow { pid: 1, ppid: 0, uid: 0 },
        ProcessRow { pid: 100, ppid: 1, uid: 0 },
        ProcessRow { pid: 101, ppid: 100, uid: 1001 },
    ];

    #[test]
    fn test_empty_table_lists_nothing() {
        let table = EmptyTable;
        let mut out = [ProcessRow { pid: 0, ppid: 0, uid: 0 }; 4];
        assert_eq!(table.list(&mut out), 0);
        assert_eq!(table.find(1), None);
    }

    #[test]
    fn test_slice_lists_and_finds() {
        let table = SliceTable { rows: &ROWS };
        let mut out = [ProcessRow { pid: 0, ppid: 0, uid: 0 }; 4];
        assert_eq!(table.list(&mut out), 3);
        assert_eq!(out[2].uid, 1001);
        assert_eq!(table.find(100).unwrap().ppid, 1);
        assert_eq!(table.find(999), None);
    }

    #[test]
    fn test_lookup_maps_miss() {
        let table = SliceTable { rows: &ROWS };
        assert_eq!(lookup(&table, 999), Err(ProcError::NotFound));
        assert_eq!(lookup(&table, 999).unwrap_err().as_errno(), 3);
    }

    #[test]
    fn test_tables_share_the_trait() {
        let empty = EmptyTable;
        let slice = SliceTable { rows: &ROWS };
        let tables: [&dyn ProcessTable; 2] = [&empty, &slice];
        let counts: [usize; 2] = {
            let mut a = [ProcessRow { pid: 0, ppid: 0, uid: 0 }; 4];
            let mut b = a;
            [tables[0].list(&mut a), tables[1].list(&mut b)]
        };
        assert_eq!(counts, [0, 3]);
    }
}
