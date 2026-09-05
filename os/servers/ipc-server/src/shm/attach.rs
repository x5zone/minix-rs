//! Attach, detach lookup, and control commands for shared memory.
//!
//! C: `do_shmat` / `do_shmdt` (lookup half) / `do_shmctl`
//! (shm.c:130-170/:209-242/:261-371).
//! Document `08-ipc-shm-attach.md` §3 (decisions D1/D4/D5).
//!
//! Mapping and unmapping stay at the boundary: this module judges
//! addresses and commands, the service layer moves the pages. Detach of
//! an unknown address succeeds (C logs and returns `OK` — shm.c:236-241),
//! so the miss path is a value, not an error.

use alloc::vec::Vec;

use minix_types::{
    IPC_INFO, IPC_R, IPC_RMID, IPC_SET, IPC_STAT, IPC_W, SHM_INFO, SHM_RND, SHM_STAT, SHMMNI,
};

use super::ShmError;
use super::segment::ShmTable;
use crate::perms::{Identity, IpcPermSysctl, ShmctlAccess, check_perm, is_owner_or_root};

// ============================================================================
// Address alignment
// ============================================================================

/// Page size for alignment (C: `PAGE_SIZE`).
pub const PAGE_SIZE: u64 = 4096;

/// Align a requested attach address: aligned passes through, unaligned
/// rounds down with the round flag, otherwise rejected.
///
/// C: `do_shmat` head (shm.c:141-146). Bitwise test keeps the function
/// `const` (`is_multiple_of` is not const yet); `PAGE_SIZE` is a power of
/// two, so the mask test is exact.
pub const fn align_addr(addr: u64, flag: u32) -> Result<u64, ShmError> {
    if addr & (PAGE_SIZE - 1) == 0 {
        Ok(addr)
    } else if flag & (SHM_RND as u32) != 0 {
        Ok(addr - addr % PAGE_SIZE)
    } else {
        Err(ShmError::Invalid)
    }
}

// ============================================================================
// Detach lookup
// ============================================================================

/// Locate a segment by the physical address behind a caller address.
///
/// C: the scan in `do_shmdt` (shm.c:221-235): up to the mark, skip free
/// slots, match the stored physical snapshot. The physical query itself
/// (`vm_getphys`) stays at the boundary; this is the pure comparison.
pub fn find_by_phys(table: &ShmTable, phys: u64) -> Option<usize> {
    for index in 0..table.live_count() {
        if let Some(seg) = table.get(index)
            && seg.backing.phys == phys
        {
            return Some(index);
        }
    }
    None
}

// ============================================================================
// Control commands
// ============================================================================

/// The six shared-memory control commands.
///
/// C: the two `switch (cmd)` ladders in `do_shmctl`
/// (shm.c:283-299/:301-369). One enum keeps them consistent (document 08
/// §3 D5), mirroring `sem::SemctlCommand`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShmctlCommand {
    /// Remove identifier (marks destroy-on-last-detach). C: `IPC_RMID 0`.
    Remove,
    /// Set options. C: `IPC_SET 1`.
    Set,
    /// Get options. C: `IPC_STAT 2`.
    Stat,
    /// Read by slot index. C: `SHM_STAT 13`.
    StatBySlot,
    /// Read module summary. C: `IPC_INFO 500`.
    Info,
    /// Read module aggregate. C: `SHM_INFO 14`.
    Aggregate,
}

impl ShmctlCommand {
    /// Decode a raw command number. `None` is rejected with `EINVAL`
    /// (shm.c:367-368) before any permission check runs.
    pub const fn from_raw(cmd: i32) -> Option<Self> {
        match cmd {
            IPC_RMID => Some(Self::Remove),
            IPC_SET => Some(Self::Set),
            IPC_STAT => Some(Self::Stat),
            SHM_STAT => Some(Self::StatBySlot),
            IPC_INFO => Some(Self::Info),
            SHM_INFO => Some(Self::Aggregate),
            _ => None,
        }
    }

    /// Raw command number.
    pub const fn to_raw(self) -> i32 {
        match self {
            Self::Remove => IPC_RMID,
            Self::Set => IPC_SET,
            Self::Stat => IPC_STAT,
            Self::StatBySlot => SHM_STAT,
            Self::Info => IPC_INFO,
            Self::Aggregate => SHM_INFO,
        }
    }
}

/// Enforce the permission ladder for one command on one segment.
///
/// C: `do_shmctl` permission arms (shm.c:303-337): read bit for the two
/// status reads, owner identity for set/remove, free for the two
/// information commands. Uses the shared 04 verdicts via `ShmctlAccess`.
pub fn authorize(
    perm: &crate::perms::IpcPerm,
    caller: Identity,
    cmd: ShmctlCommand,
) -> Result<(), ShmError> {
    match crate::perms::resolve_shmctl_access(cmd.to_raw()) {
        ShmctlAccess::CheckOwner => {
            if is_owner_or_root(perm, caller.uid) {
                Ok(())
            } else {
                Err(ShmError::Ownership)
            }
        }
        ShmctlAccess::Free => Ok(()),
        ShmctlAccess::CheckRead => {
            if check_perm(perm, caller, IPC_R) {
                Ok(())
            } else {
                Err(ShmError::Access)
            }
        }
    }
}

/// Attach mask for a flag word: read bit for read-only attaches,
/// read-plus-write otherwise.
///
/// C: `do_shmat` (shm.c:151-155). Thin wrapper over the shared 04 mask so
/// the attach path reads alike.
pub const fn attach_mask(flag: u32) -> u32 {
    crate::perms::resolve_shmat_mask(flag)
}

// ============================================================================
// Information assembly
// ============================================================================

/// Module summary counters.
///
/// C: `struct shminfo` — sys/shm.h:136-142.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShmSummary {
    /// Max segment size (unlimited). C: `shmmax` (-1).
    pub max: u64,
    /// Min segment size. C: `shmmin` (1).
    pub min: u32,
    /// Max identifiers. C: `shmmni` (1024).
    pub identifiers: u32,
    /// Max segments per process (unlimited). C: `shmseg` (-1).
    pub per_process: u64,
    /// Max pages (unlimited). C: `shmall` (-1).
    pub pages: u64,
}

/// Fill the summary (all constants — C: `fill_shminfo`, shm.c:248-258).
pub const fn fill_summary() -> ShmSummary {
    ShmSummary {
        max: u64::MAX,
        min: 1,
        identifiers: SHMMNI as u32,
        per_process: u64::MAX,
        pages: u64::MAX,
    }
}

/// Module aggregate counters.
///
/// C: `struct shm_info` — sys/shm.h:208-216.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ShmInfoAgg {
    /// Segments in use. C: `used_ids`.
    pub used_ids: i32,
    /// Total pages. C: `shm_tot`.
    pub total_pages: u64,
    /// Resident pages (== total: no swapping). C: `shm_rss`.
    pub resident_pages: u64,
    /// Swapped pages (always zero). C: `shm_swp`.
    pub swapped_pages: u64,
    /// Swap attempts (always zero). C: `swap_attempts`.
    pub swap_attempts: u64,
    /// Swap successes (always zero). C: `swap_successes`.
    pub swap_successes: u64,
}

/// Aggregate live segments: count plus page totals.
///
/// C: `SHM_INFO` arm (shm.c:348-366). Page counts divide by the page size
/// (`PAGE_SIZE`); resident echoes total (no swapping exists).
pub fn aggregate(table: &ShmTable) -> ShmInfoAgg {
    let mut agg = ShmInfoAgg::default();
    for index in 0..table.live_count() {
        if let Some(seg) = table.get(index) {
            agg.used_ids += 1;
            agg.total_pages += seg.size_bytes / super::segment::PAGE_SIZE;
        }
    }
    agg.resident_pages = agg.total_pages;
    agg
}

/// One row of the management-information listing.
///
/// C: `struct shmid_ds_sysctl` — sys/shm.h:145-154 (permission snapshot
/// plus sizes, processes, times, attach count; the private pointer is
/// dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShmIdView {
    /// Permission snapshot. C: `shm_perm` (via `prepare_mib_perm`).
    pub perm: IpcPermSysctl,
    /// Segment size in bytes. C: `shm_segsz`.
    pub size: u64,
    /// Last-operation process. C: `shm_lpid`.
    pub last_pid: i32,
    /// Creator process. C: `shm_cpid`.
    pub creator_pid: i32,
    /// Last attach time. C: `shm_atime`.
    pub attach_time: u64,
    /// Last detach time. C: `shm_dtime`.
    pub detach_time: u64,
    /// Last change time. C: `shm_ctime`.
    pub change_time: u64,
    /// Attach count. C: `shm_nattch`.
    pub attached: u16,
}

/// Assemble the full listing: one row per slot, always 1024.
///
/// C: `get_shm_mib_info` minus the copy-out loop (shm.c:379-444). Same
/// contract as the semaphore twin: `ipcs` sizes its buffer from the
/// summary, so free rows are zeroed placeholders (shm.c:399-404).
pub fn assemble_mib_info(table: &ShmTable) -> Vec<ShmIdView> {
    let mut rows = Vec::with_capacity(SHMMNI);
    for index in 0..SHMMNI {
        match table.get(index) {
            Some(seg) => rows.push(ShmIdView {
                perm: IpcPermSysctl::from_perm(&seg.perm),
                size: seg.size_bytes,
                last_pid: seg.last_pid,
                creator_pid: seg.creator_pid,
                attach_time: seg.attach_time,
                detach_time: seg.detach_time,
                change_time: seg.change_time,
                attached: seg.attached,
            }),
            None => rows.push(ShmIdView {
                perm: IpcPermSysctl::from_perm(&crate::perms::IpcPerm {
                    key: 0,
                    uid: 0,
                    gid: 0,
                    creator_uid: 0,
                    creator_gid: 0,
                    mode: 0,
                    seq: 0,
                }),
                size: 0,
                last_pid: 0,
                creator_pid: 0,
                attach_time: 0,
                detach_time: 0,
                change_time: 0,
                attached: 0,
            }),
        }
    }
    rows
}

/// Highest in-use slot number, or zero when empty (the `IPC_INFO` /
/// `SHM_INFO` reply slot).
///
/// C: `shm_list_nr - 1`, or 0 (shm.c:343-346/:362-365).
pub fn highest_slot_reply(table: &ShmTable) -> i32 {
    if table.live_count() > 0 {
        table.live_count() as i32 - 1
    } else {
        0
    }
}

/// Write mask used when the attach is not read-only.
pub const WRITE_MASK: u32 = IPC_R | IPC_W;
/// Read mask used for read-only attaches and status reads.
pub const READ_MASK: u32 = IPC_R;
/// Write bit (re-export for attach-path readers).
pub const WRITE_BIT: u32 = IPC_W;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::Identity;
    use crate::shm::segment::{Backing, CreateParams, ShmTable};

    fn caller() -> Identity {
        Identity { uid: 100, gid: 200 }
    }

    fn one_segment() -> ShmTable {
        let mut table = ShmTable::new();
        table
            .create(CreateParams {
                key: 1,
                size: 5000,
                flag: 0o1000 | 0o600,
                caller: caller(),
                backing: Backing {
                    local: 0x4000_0000,
                    phys: 0x0010_0000,
                },
                now: 111,
                cpid: 42,
            })
            .unwrap();
        table
    }

    #[test]
    fn align_rounds_down_with_flag() {
        // C: shm.c:142-143 — unaligned with the round flag rounds down.
        assert_eq!(align_addr(0x1000, 0), Ok(0x1000));
        assert_eq!(
            align_addr(0x1234, SHM_RND as u32),
            Ok(0x1000),
            "rounds down to the page"
        );
    }

    #[test]
    fn align_rejects_without_flag() {
        // C: shm.c:144-145 — unaligned without the flag fails.
        assert_eq!(align_addr(0x1234, 0), Err(ShmError::Invalid));
    }

    #[test]
    fn find_by_phys_matches() {
        // C: shm.c:221-235 — physical snapshot equality, free slots skipped.
        let table = one_segment();
        assert_eq!(find_by_phys(&table, 0x0010_0000), Some(0));
        assert_eq!(find_by_phys(&table, 0x0020_0000), None);
        assert_eq!(find_by_phys(&ShmTable::new(), 0x0010_0000), None);
    }

    #[test]
    fn detach_missing_is_ok() {
        // C: shm.c:236-241 — an unmatched detach logs and still succeeds.
        // Service-layer note: the miss path carries no error, so there is
        // no test asserting failure — this test pins the lookup half
        // (miss → None) that the service maps to success.
        let table = one_segment();
        assert_eq!(find_by_phys(&table, 0x9999_0000), None);
    }

    #[test]
    fn shmctl_commands_roundtrip() {
        // Six commands decode; anything else is rejected up front.
        let cmds = [
            (0, ShmctlCommand::Remove),
            (1, ShmctlCommand::Set),
            (2, ShmctlCommand::Stat),
            (13, ShmctlCommand::StatBySlot),
            (500, ShmctlCommand::Info),
            (14, ShmctlCommand::Aggregate),
        ];
        for (raw, cmd) in cmds {
            assert_eq!(ShmctlCommand::from_raw(raw), Some(cmd));
            assert_eq!(cmd.to_raw(), raw);
        }
        assert_eq!(ShmctlCommand::from_raw(3), None);
        // Authorization follows the 04 matrix: stranger reads fail,
        // owner removes pass.
        let table = one_segment();
        let seg = table.get(0).unwrap();
        let stranger = Identity { uid: 777, gid: 777 };
        assert_eq!(
            authorize(&seg.perm, stranger, ShmctlCommand::Stat),
            Err(ShmError::Access)
        );
        assert_eq!(
            authorize(&seg.perm, caller(), ShmctlCommand::Remove),
            Ok(())
        );
        assert_eq!(
            authorize(&seg.perm, stranger, ShmctlCommand::Remove),
            Err(ShmError::Ownership)
        );
        assert_eq!(authorize(&seg.perm, stranger, ShmctlCommand::Info), Ok(()));
    }

    #[test]
    fn mib_rows_always_full() {
        // C: shm.c:399-430 — always SHMMNI rows; live rows carry data.
        let table = one_segment();
        let rows = assemble_mib_info(&table);
        assert_eq!(rows.len(), SHMMNI);
        assert_eq!((rows[0].size, rows[0].creator_pid), (5000, 42));
        assert_eq!(rows[1].size, 0, "free rows are zeroed placeholders");
        let summary = fill_summary();
        assert_eq!((summary.min, summary.identifiers), (1, SHMMNI as u32));
        let agg = aggregate(&table);
        assert_eq!(agg.used_ids, 1);
        assert_eq!(agg.resident_pages, agg.total_pages);
        assert_eq!(agg.swapped_pages, 0);
    }
}
