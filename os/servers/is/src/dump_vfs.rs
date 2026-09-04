//! VFS dump domain (07-is-dump-vfs.md §4.1).
//!
//! C: `minix3/minix/servers/is/dmp_fs.c` (83 lines). Two dumps over two
//! tables (`SI_PROC_TAB` + `SI_DMAP_TAB`): fd counting, session/revived
//! bits, blocked-on endpoints, driver mapping. Bodies await the A-6 output
//! channel; this module delivers snapshots, pure logic, and format
//! constants.
//!
//! `[ARCH: A-4]`: snapshots are wire-contract proposals (`#[repr(C)]`,
//! used-fields subsets); the VFS-side GETSYSINFO producer aligns to them
//! (pending VFS-crate work, explicit TODO below).

// TODO(P1): [code] [factual] Align VFS-crate GETSYSINFO producers with the
// snapshots below — see 07 §4. (current state: IS-side wire proposals)
// (fix: VFS `do_getsysinfo` SI_PROC_TAB/SI_DMAP_TAB payloads match; A-4).

/// VFS process snapshot (used fields only).
///
/// C: `struct fproc` — `minix3/minix/servers/vfs/fproc.h` (subset).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct FProcSnap {
    /// C: `fp_pid` (fproc.h:18).
    pub fp_pid: i32,
    /// C: `fp_tty` device number (fproc.h:27).
    pub fp_tty: i32,
    /// C: `fp_umask` (fproc.h:69).
    pub fp_umask: u32,
    /// C: `fp_realuid` (fproc.h:63).
    pub fp_realuid: u32,
    /// C: `fp_effuid` + `fp_realgid` + `fp_effgid` (fproc.h:63-65 area).
    pub fp_effuid: u32,
    /// C: `fp_realgid`.
    pub fp_realgid: u32,
    /// C: `fp_effgid`.
    pub fp_effgid: u32,
    /// C: `fp_flags` (fproc.h:16).
    pub fp_flags: u32,
    /// C: `fp_blocked_on` (fproc.h:29).
    pub fp_blocked_on: i32,
}

/// Max open files per process. C: `OPEN_MAX 255` — sys/syslimits.h:38.
pub const OPEN_MAX_FD: usize = 255;

/// Session-leader bit. C: `FP_SESLDR 0004` — fproc.h:96.
pub const FP_SESLDR: u32 = 0o4;
/// Revived bit. C: `FP_REVIVED 0002` — fproc.h:94.
pub const FP_REVIVED: u32 = 0o2;

/// Blocked-on reason.
///
/// C: `FP_BLOCKED_ON_*` — `minix3/minix/servers/vfs/const.h:19-25`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedOn {
    None,
    Pipe,
    Flock,
    Popen,
    Select,
    Cdev,
    Sdev,
}

impl BlockedOn {
    /// Wire value.
    pub const fn code(self) -> i32 {
        match self {
            BlockedOn::None => 0,
            BlockedOn::Pipe => 1,
            BlockedOn::Flock => 2,
            BlockedOn::Popen => 3,
            BlockedOn::Select => 4,
            BlockedOn::Cdev => 5,
            BlockedOn::Sdev => 6,
        }
    }

    /// Decodes a wire value; unknown values are `None` (forward-compatible,
    /// cf. C's open `int` comparison chain).
    pub const fn decode(v: i32) -> Option<Self> {
        match v {
            0 => Some(BlockedOn::None),
            1 => Some(BlockedOn::Pipe),
            2 => Some(BlockedOn::Flock),
            3 => Some(BlockedOn::Popen),
            4 => Some(BlockedOn::Select),
            5 => Some(BlockedOn::Cdev),
            6 => Some(BlockedOn::Sdev),
            _ => None,
        }
    }
}

/// Resolves the proc column for a blocked process.
///
/// C: `fp_blocked_on == FP_BLOCKED_ON_CDEV → fp_cdev.endpt`, else `" nil"`
/// (dmp_fs.c:60-63), with the source comment
/// `/* TODO: for FP_BLOCKED_ON_SDEV we do not have the endpoint.. */`.
/// The SDEV gap is inherited explicitly as `None` (not this doc's debt).
pub const fn blocked_endpoint(on: BlockedOn, cdev_endpt: i32) -> Option<i32> {
    match on {
        BlockedOn::Cdev => Some(cdev_endpt),
        _ => None,
    }
}

/// Counts open file descriptors.
///
/// C: the `for (j...) if (fp_filp[j] != NULL) nfds++` loop — dmp_fs.c:45-47.
/// Slice version (callers pass the occupancy window; full width is
/// `OPEN_MAX_FD`).
pub fn count_fds(slots_used: &[bool]) -> u32 {
    let mut n = 0u32;
    let mut i = 0;
    while i < slots_used.len() {
        if slots_used[i] {
            n += 1;
        }
        i += 1;
    }
    n
}

/// Whether an fproc row is skipped.
///
/// C: `if (fp->fp_pid <= 0) continue` — dmp_fs.c:43. Note the difference
/// from PM (`== 0` + slot-0 exception): VFS has no exception.
pub const fn fproc_skipped(pid: i32) -> bool {
    pid <= 0
}

/// Device-mapping snapshot.
///
/// C: `struct dmap` — `minix3/minix/servers/vfs/dmap.h:16-18` (subset:
/// label + driver only).
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct DmapSnap {
    /// C: `dmap_driver` (dmap.h:17; `NONE` = empty slot).
    pub dmap_driver: i32,
    /// C: `dmap_label[LABEL_MAX]`, `LABEL_MAX 16` (vfs/const.h:34).
    pub dmap_label: [u8; 16],
}

/// Whether a dmap row is skipped. C: `dmap[i].dmap_driver == NONE` —
/// dmp_fs.c:78.
pub const fn dmap_skipped(driver: i32, none: i32) -> bool {
    driver == none
}

/// VFS page cursor: isomorphic to 06's `PmCursor` (22-bound, `\r`, wrap).
/// A fresh type (not an alias): 06 is CONVERGED and its `Pm`-prefixed name
/// would lie here. Logic intentionally duplicated, noted here.
#[derive(Debug, Clone, Copy, Default)]
pub struct VfsCursor {
    next: usize,
    n: u32,
}

impl VfsCursor {
    pub const fn new() -> Self {
        Self { next: 0, n: 0 }
    }

    pub const fn next(&self) -> usize {
        self.next
    }

    /// C: `if (fp->fp_pid <= 0) continue; if (++n > 22) break;` — dmp_fs.c:43-44.
    pub const fn push(&mut self, pid: i32, idx: usize) -> bool {
        if fproc_skipped(pid) {
            return false; // skipped, uncounted
        }
        self.n += 1;
        if self.n > 22 {
            self.next = idx;
            return false; // break: row NOT printed
        }
        true // printed
    }

    /// C: `if (i >= NR_PROCS) i = 0; else printf("--more--\r"); prev_i = i;`
    pub const fn finish(&mut self, exhausted: bool) {
        if exhausted {
            self.next = 0;
        }
    }
}

/// C: `"File System (FS) process table dump\n"` — dmp_fs.c:50.
pub const FPROC_TITLE: &str = "File System (FS) process table dump\n";
/// C: dmp_fs.c:51.
pub const FPROC_COLUMNS: &str =
    "-nr- -pid- -tty- -umask- --uid-- --gid-- -ldr-fds-sus-rev-proc-\n";
/// C: `"File System (FS) device <-> driver mappings\n"` — dmp_fs.c:73.
pub const DMAP_TITLE: &str = "File System (FS) device <-> driver mappings\n";
/// C: dmp_fs.c:74-75.
pub const DMAP_COLUMNS: &str = "    Label     Major Driver ept\n------------- ----- ----------\n";
/// C: `" nil\n"` non-cdev branch — dmp_fs.c:63.
pub const NIL_ENDPOINT: &str = " nil\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_fds() {
        // C: dmp_fs.c:45-47.
        assert_eq!(count_fds(&[]), 0);
        assert_eq!(count_fds(&[false, true, false, true]), 2);
        assert_eq!(OPEN_MAX_FD, 255);
    }

    #[test]
    fn test_skip_rule_no_exception() {
        // C: `fp_pid <= 0` — dmp_fs.c:43 (vs PM's ==0+exception).
        assert!(fproc_skipped(0));
        assert!(fproc_skipped(-3));
        assert!(!fproc_skipped(1));
    }

    #[test]
    fn test_fp_bits() {
        // C: fproc.h:94-96 (octal 0002/0004).
        assert_eq!((FP_SESLDR, FP_REVIVED), (0o4, 0o2));
        assert_eq!(0x10u32 & FP_SESLDR, 0);
        assert_ne!(0x04u32 & FP_SESLDR, 0);
    }

    #[test]
    fn test_blocked_on_codes_and_endpoint() {
        // C: const.h:19-25 + dmp_fs.c:60-63 (+ SDEV TODO).
        assert_eq!(BlockedOn::decode(5), Some(BlockedOn::Cdev));
        assert_eq!(BlockedOn::decode(6), Some(BlockedOn::Sdev));
        assert_eq!(BlockedOn::decode(99), None);
        assert_eq!(blocked_endpoint(BlockedOn::Cdev, 7), Some(7));
        assert_eq!(blocked_endpoint(BlockedOn::Sdev, 7), None);
        assert_eq!(blocked_endpoint(BlockedOn::Pipe, 7), None);
    }

    #[test]
    fn test_vfs_cursor_pages_like_pm() {
        let mut c = VfsCursor::new();
        for i in 0..22 {
            assert!(c.push(1, i));
        }
        assert!(!c.push(1, 22));
        assert_eq!(c.next(), 22);
        assert!(!c.push(0, 23)); // skipped rows don't count or break
        c.finish(true);
        assert_eq!(c.next(), 0);
    }

    #[test]
    fn test_dmap_skip_and_columns() {
        assert!(dmap_skipped(-99, -99));
        assert!(!dmap_skipped(5, -99));
        assert!(DMAP_COLUMNS.contains("Label     Major Driver ept"));
        assert_eq!(NIL_ENDPOINT, " nil\n");
    }
}
