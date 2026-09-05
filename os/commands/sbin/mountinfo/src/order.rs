//! Check scheduling from pass numbers, plus the mount table trait.
//!
//! Ground truth: `minix3/sbin/fsck/fsck.c:254` (`fs_passno == 0` means
//! skip). Scheduling rules: pass 1 (the root) goes first and alone; higher
//! passes follow in ascending order; pass 0 never runs. Rows sharing a
//! pass number keep table order (stable: the administrator's listing order
//! is meaningful). The [`MountTable`] trait offers table lookup with an
//! empty and a slice implementation, the stage's standing shape for
//! read only databases.

use crate::MountError;
use crate::fstab::FstabEntry;

/// Maximum rows scheduled in one plan.
pub const MAX_ROWS: usize = 16;

/// One scheduled check: which row, in which pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckJob<'a> {
    /// The table row to check.
    pub entry: FstabEntry<'a>,
}

/// Plan the check order for up to 16 rows: ascending pass numbers, pass 0
/// skipped, table order kept within a pass. Returns the jobs plus how
/// many are used.
pub fn plan_checks<'a>(
    rows: &[FstabEntry<'a>],
) -> Result<([CheckJob<'a>; MAX_ROWS], usize), MountError> {
    if rows.len() > MAX_ROWS {
        return Err(MountError::InvalidArgument);
    }
    let mut jobs: [CheckJob<'a>; MAX_ROWS] = [CheckJob {
        entry: FstabEntry {
            device: "",
            mount_point: "",
            fs_type: "",
            options: "",
            dump: 0,
            pass: 0,
        },
    }; MAX_ROWS];
    let mut count = 0;
    // Pass numbers in ascending order; rows keep table order (insertion
    // sort over at most 16 rows is trivially fast and obviously stable).
    let mut order: [usize; MAX_ROWS] = [0; MAX_ROWS];
    for (index, _) in rows.iter().enumerate() {
        order[index] = index;
    }
    for i in 1..rows.len() {
        let mut j = i;
        while j > 0 && rows[order[j]].pass < rows[order[j - 1]].pass {
            order.swap(j, j - 1);
            j -= 1;
        }
    }
    for index in order[..rows.len()].iter() {
        let entry = rows[*index];
        if entry.pass == 0 {
            continue;
        }
        jobs[count] = CheckJob { entry };
        count += 1;
    }
    Ok((jobs, count))
}

/// Read only mount table lookup by mount point.
pub trait MountTable<'a> {
    /// Find the row mounted at `point`, or `None`.
    fn lookup(&self, point: &str) -> Option<FstabEntry<'a>>;
}

/// A table with no rows: every lookup misses. The honest starting point
/// until the table reader lands.
pub struct EmptyMountTable;

impl<'a> MountTable<'a> for EmptyMountTable {
    fn lookup(&self, _point: &str) -> Option<FstabEntry<'a>> {
        None
    }
}

/// A table over in memory rows. Corrupt rows never reach it (parsing
/// filters them); the first mount point match wins.
pub struct SliceMountTable<'a> {
    /// Table rows searched in order.
    pub rows: &'a [FstabEntry<'a>],
}

impl<'a> MountTable<'a> for SliceMountTable<'a> {
    fn lookup(&self, point: &str) -> Option<FstabEntry<'a>> {
        self.rows
            .iter()
            .find(|entry| entry.mount_point == point)
            .copied()
    }
}

/// Look up one mount point, translating the miss.
pub fn lookup_mount<'a, T: MountTable<'a>>(table: &'a T, point: &str) -> Result<FstabEntry<'a>, MountError> {
    if point.is_empty() {
        return Err(MountError::InvalidArgument);
    }
    table.lookup(point).ok_or(MountError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fstab::parse_fstab_line;

    fn rows() -> [FstabEntry<'static>; 4] {
        let lines = [
            "/dev/a /usr mfs rw 1 2",
            "/dev/r / mfs rw 1 1",
            "procfs /proc procfs ro 0 0",
            "/dev/b /home mfs rw 1 2",
        ];
        let mut rows = [FstabEntry {
            device: "",
            mount_point: "",
            fs_type: "",
            options: "",
            dump: 0,
            pass: 0,
        }; 4];
        for (index, line) in lines.iter().enumerate() {
            rows[index] = parse_fstab_line(line).unwrap().unwrap();
        }
        rows
    }

    #[test]
    fn test_root_first_then_ascending() {
        let rows = rows();
        let (jobs, count) = plan_checks(&rows).unwrap();
        assert_eq!(count, 3);
        assert_eq!(jobs[0].entry.mount_point, "/");
        assert_eq!(jobs[1].entry.mount_point, "/usr");
        assert_eq!(jobs[2].entry.mount_point, "/home");
    }

    #[test]
    fn test_pass_zero_skipped() {
        let rows = rows();
        let (jobs, count) = plan_checks(&rows).unwrap();
        assert!((0..count).all(|i| jobs[i].entry.pass != 0));
    }

    #[test]
    fn test_lookup_by_point() {
        let rows = rows();
        let table = SliceMountTable { rows: &rows };
        assert_eq!(lookup_mount(&table, "/usr").unwrap().device, "/dev/a");
        assert_eq!(lookup_mount(&table, "/ghost"), Err(MountError::NotFound));
        assert_eq!(lookup_mount(&table, ""), Err(MountError::InvalidArgument));
    }

    #[test]
    fn test_empty_table_misses() {
        let table = EmptyMountTable;
        assert_eq!(lookup_mount(&table, "/"), Err(MountError::NotFound));
    }
}
