//! Semaphore control commands: the thirteen `semctl` branches.
//!
//! C: `do_semctl` / `fill_seminfo` / `get_sem_mib_info`
//! (sem.c:469-648/:430-463/:787-848).
//! Document `05-ipc-sem-table.md` §3 (decisions D5-D7).
//!
//! Kernel data copies stay at the boundary: these functions take and
//! return plain values and buffers. The service layer moves the bytes.

use alloc::vec::Vec;

use minix_types::{
    GETALL, GETNCNT, GETPID, GETVAL, GETZCNT, IPC_INFO, IPC_RMID, IPC_SET, IPC_STAT, IPC_W,
    SEM_INFO, SEM_STAT, SEMMNI, SEMMSL, SEMVMX, SETALL, SETVAL,
};

use super::SemError;
use super::table::{SemaphoreTable, encode_id};
use crate::perms::{
    Identity, IpcPermSysctl, SemctlAccess, check_perm, is_owner_or_root, resolve_semctl_mask,
};

// ============================================================================
// Command enum
// ============================================================================

/// The thirteen control commands plus the two information commands.
///
/// C: the three `switch (cmd)` ladders in `do_semctl`
/// (sem.c:491/:516/:540). One enum keeps the ladders consistent: adding a
/// command forces all three sites to handle it (document 05 §3 D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemctlCommand {
    /// Remove identifier. C: `IPC_RMID 0`.
    Remove,
    /// Set options. C: `IPC_SET 1`.
    Set,
    /// Get options. C: `IPC_STAT 2`.
    Stat,
    /// Read by slot index. C: `SEM_STAT 18`.
    StatBySlot,
    /// Read module summary. C: `IPC_INFO 500`.
    Info,
    /// Read module detail. C: `SEM_INFO 19`.
    SemInfo,
    /// Return wait-for-increase count. C: `GETNCNT 3`.
    GetRaiseWaiters,
    /// Return last-operation process. C: `GETPID 4`.
    GetLastPid,
    /// Return value. C: `GETVAL 5`.
    GetValue,
    /// Return all values. C: `GETALL 6`.
    GetAll,
    /// Return wait-for-zero count. C: `GETZCNT 7`.
    GetZeroWaiters,
    /// Set value. C: `SETVAL 8`.
    SetValue,
    /// Set all values. C: `SETALL 9`.
    SetAll,
}

impl SemctlCommand {
    /// Decode a raw command number. `None` means the dispatch rejects it
    /// with `EINVAL` (sem.c:643-644) before any permission check runs.
    pub const fn from_raw(cmd: i32) -> Option<Self> {
        match cmd {
            IPC_RMID => Some(Self::Remove),
            IPC_SET => Some(Self::Set),
            IPC_STAT => Some(Self::Stat),
            SEM_STAT => Some(Self::StatBySlot),
            IPC_INFO => Some(Self::Info),
            SEM_INFO => Some(Self::SemInfo),
            GETNCNT => Some(Self::GetRaiseWaiters),
            GETPID => Some(Self::GetLastPid),
            GETVAL => Some(Self::GetValue),
            GETALL => Some(Self::GetAll),
            GETZCNT => Some(Self::GetZeroWaiters),
            SETVAL => Some(Self::SetValue),
            SETALL => Some(Self::SetAll),
            _ => None,
        }
    }

    /// Raw command number.
    pub const fn to_raw(self) -> i32 {
        match self {
            Self::Remove => IPC_RMID,
            Self::Set => IPC_SET,
            Self::Stat => IPC_STAT,
            Self::StatBySlot => SEM_STAT,
            Self::Info => IPC_INFO,
            Self::SemInfo => SEM_INFO,
            Self::GetRaiseWaiters => GETNCNT,
            Self::GetLastPid => GETPID,
            Self::GetValue => GETVAL,
            Self::GetAll => GETALL,
            Self::GetZeroWaiters => GETZCNT,
            Self::SetValue => SETVAL,
            Self::SetAll => SETALL,
        }
    }
}

// ============================================================================
// Authorization (document 04 matrix, semctl column)
// ============================================================================

/// Enforce the permission ladder for one command on one set.
///
/// C: sem.c:516-538. Write-bit for the two setters, owner identity for
/// remove/change-owner, free for the two information commands, read-bit
/// for everything else.
pub fn authorize(
    perm: &crate::perms::IpcPerm,
    caller: Identity,
    cmd: SemctlCommand,
) -> Result<(), SemError> {
    match resolve_semctl_mask(cmd.to_raw()) {
        SemctlAccess::CheckWrite => {
            if check_perm(perm, caller, IPC_W) {
                Ok(())
            } else {
                Err(SemError::Access)
            }
        }
        SemctlAccess::CheckOwner => {
            if is_owner_or_root(perm, caller.uid) {
                Ok(())
            } else {
                Err(SemError::Ownership)
            }
        }
        SemctlAccess::Free => Ok(()),
        SemctlAccess::CheckRead => {
            if check_perm(perm, caller, minix_types::IPC_R) {
                Ok(())
            } else {
                Err(SemError::Access)
            }
        }
    }
}

// ============================================================================
// Value queries and updates
// ============================================================================

/// Read one scalar field (the four indexed getters).
///
/// C: `GETVAL`/`GETPID`/`GETNCNT`/`GETZCNT` (sem.c:590-609). Out-of-range
/// semaphore numbers fail with `EINVAL`.
pub fn query_scalar(
    table: &SemaphoreTable,
    index: usize,
    cmd: SemctlCommand,
    num: i32,
) -> Result<i32, SemError> {
    let set = table.get(index).ok_or(SemError::Invalid)?;
    if num < 0 || num as usize >= set.count {
        return Err(SemError::Invalid);
    }
    let sem = &set.sems[num as usize];
    Ok(match cmd {
        SemctlCommand::GetValue => sem.value as i32,
        SemctlCommand::GetLastPid => sem.last_pid,
        SemctlCommand::GetRaiseWaiters => sem.raise_waiters as i32,
        SemctlCommand::GetZeroWaiters => sem.zero_waiters as i32,
        _ => return Err(SemError::Invalid),
    })
}

/// Read all values into a fixed buffer (caller copies it out).
///
/// C: `GETALL` (sem.c:581-589) via the static `valbuf`.
pub fn read_all(table: &SemaphoreTable, index: usize) -> Result<[u16; SEMMSL], SemError> {
    let set = table.get(index).ok_or(SemError::Invalid)?;
    let mut out = [0u16; SEMMSL];
    for (i, sem) in set.sems.iter().enumerate().take(set.count) {
        out[i] = sem.value;
    }
    Ok(out)
}

/// Write all values from a caller-supplied buffer.
///
/// C: `SETALL` (sem.c:610-628). Every value is range-checked *before* any
/// is stored; returns whether the waiter queue needs a retry (`check_set`,
/// sem.c:627 — the service layer runs it).
pub fn write_all(
    table: &mut SemaphoreTable,
    index: usize,
    values: &[u16],
    now: u64,
) -> Result<bool, SemError> {
    let set = table.get(index).ok_or(SemError::Invalid)?;
    if values.len() < set.count {
        return Err(SemError::Invalid);
    }
    for &v in values.iter().take(set.count) {
        if v as u32 > SEMVMX {
            return Err(SemError::Range);
        }
    }
    let set = table.get_mut(index).expect("checked live above");
    for (i, &v) in values.iter().enumerate().take(set.count) {
        set.sems[i].value = v;
    }
    set.change_time = now;
    Ok(true)
}

/// Write one value.
///
/// C: `SETVAL` (sem.c:629-642). Returns whether the waiter queue needs a
/// retry, like [`write_all`].
pub fn write_value(
    table: &mut SemaphoreTable,
    index: usize,
    num: i32,
    value: i32,
    now: u64,
) -> Result<bool, SemError> {
    let set = table.get(index).ok_or(SemError::Invalid)?;
    if num < 0 || num as usize >= set.count {
        return Err(SemError::Invalid);
    }
    if value < 0 || value as u32 > SEMVMX {
        return Err(SemError::Range);
    }
    let set = table.get_mut(index).expect("checked live above");
    set.sems[num as usize].value = value as u16;
    set.change_time = now;
    Ok(true)
}

// ============================================================================
// Information assembly
// ============================================================================

/// Module summary or detail counters.
///
/// C: `struct seminfo` — sys/sem.h:123-134 (ten 32-bit fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemInfo {
    /// Entries in the semaphore map. C: `semmap` (always `SEMMNI`).
    pub map: i32,
    /// Semaphore identifiers. C: `semmni` (always `SEMMNI`).
    pub identifiers: i32,
    /// Semaphores system-wide. C: `semmns` (`SEMMNI * SEMMSL`).
    pub total: i32,
    /// Undo structures (unsupported, always zero). C: `semmnu`.
    pub undo_structures: i32,
    /// Max semaphores per id. C: `semmsl`.
    pub per_id: i32,
    /// Max operations per call. C: `semopm`.
    pub max_ops: i32,
    /// Max undo entries (unsupported, always zero). C: `semume`.
    pub undo_entries: i32,
    /// Undo size / live-set count. C: `semusz` (see below).
    pub live_or_undo: i32,
    /// Maximum value. C: `semvmx`.
    pub max_value: i32,
    /// Exit-adjust max / allocated total. C: `semaem` (see below).
    pub allocated_or_exit: i32,
}

/// Fill the summary (`IPC_INFO`) or the detail (`SEM_INFO`) counters.
///
/// C: `fill_seminfo` — sem.c:430-463. The two differ in exactly two
/// fields: summary reports zeros where undo support would go, detail
/// reports the live-set count and the allocated-semaphore total.
pub fn fill_info(table: &SemaphoreTable, detail: bool) -> SemInfo {
    let mut info = SemInfo {
        map: SEMMNI as i32,
        identifiers: SEMMNI as i32,
        total: (SEMMNI * SEMMSL) as i32,
        undo_structures: 0,
        per_id: SEMMSL as i32,
        max_ops: minix_types::SEMOPM as i32,
        undo_entries: 0,
        live_or_undo: 0,
        max_value: SEMVMX as i32,
        allocated_or_exit: 0,
    };
    if detail {
        info.live_or_undo = table.live_count() as i32;
        info.allocated_or_exit = table_slots_used(table) as i32;
    }
    info
}

/// Total allocated semaphores across live sets (the `SEM_INFO` total).
fn table_slots_used(table: &SemaphoreTable) -> usize {
    let mut total = 0;
    let mut index = 0;
    while index < SEMMNI {
        if let Some(set) = table.get(index) {
            total += set.count;
        }
        index += 1;
    }
    total
}

/// One row of the management-information listing.
///
/// C: `struct semid_ds_sysctl` — sys/sem.h:137-144 (permission snapshot
/// plus counts and times; the private base pointer is dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemIdView {
    /// Permission snapshot. C: `sem_perm` (via `prepare_mib_perm`).
    pub perm: IpcPermSysctl,
    /// Semaphores in this set. C: `sem_nsems`.
    pub count: u16,
    /// Last operation time. C: `sem_otime`.
    pub op_time: u64,
    /// Last change time. C: `sem_ctime`.
    pub change_time: u64,
}

/// Assemble the full listing: summary plus one row per slot, always ten.
///
/// C: `get_sem_mib_info` — sem.c:787-848 minus the copy-out loop. The row
/// count is *always* `SEMMNI` (live rows carry snapshots, free rows carry
/// zeroes) because `ipcs` sizes its buffer from the summary (sem.c:807-812
/// — the fixed-length array is a contract, document 05 §3 D7).
pub fn assemble_mib_info(table: &SemaphoreTable) -> (SemInfo, Vec<SemIdView>) {
    let info = fill_info(table, false);
    let mut rows = Vec::with_capacity(SEMMNI);
    for index in 0..SEMMNI {
        match table.get(index) {
            Some(set) => rows.push(SemIdView {
                perm: IpcPermSysctl::from_perm(&set.perm),
                count: set.count as u16,
                op_time: set.op_time,
                change_time: set.change_time,
            }),
            None => rows.push(SemIdView {
                perm: IpcPermSysctl::from_perm(&crate::perms::IpcPerm {
                    key: 0,
                    uid: 0,
                    gid: 0,
                    creator_uid: 0,
                    creator_gid: 0,
                    mode: 0,
                    seq: 0,
                }),
                count: 0,
                op_time: 0,
                change_time: 0,
            }),
        }
    }
    (info, rows)
}

/// Highest in-use slot number, or zero when empty (the `IPC_INFO` /
/// `SEM_INFO` reply slot).
///
/// C: `sem_list_nr - 1`, or 0 (sem.c:576-579).
pub fn highest_slot_reply(table: &SemaphoreTable) -> i32 {
    if table.live_count() > 0 {
        table.live_count() as i32 - 1
    } else {
        0
    }
}

/// Identifier to return for a slot-index read (`SEM_STAT`).
///
/// C: `IXSEQ_TO_IPCID(id, ...)` — sem.c:547-548.
pub fn stat_reply_id(table: &SemaphoreTable, slot: usize) -> Result<i32, SemError> {
    let set = table.get(slot).ok_or(SemError::Invalid)?;
    Ok(encode_id(slot, set.perm.seq))
}

/// Control reply: either a scalar for the message reply slot or bytes the
/// service layer copies out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtlReply {
    /// Fill the message reply slot with this value.
    Scalar(i32),
    /// Caller must copy structured bytes out (stat/info/get-all paths).
    CopyOut,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::Identity;

    fn caller() -> Identity {
        Identity { uid: 100, gid: 200 }
    }

    fn one_set() -> SemaphoreTable {
        let mut table = SemaphoreTable::new();
        table.create(1, 3, 0o1000 | 0o600, caller(), 111).unwrap();
        table
    }

    #[test]
    fn command_roundtrip() {
        // Thirteen commands decode; anything else is rejected up front.
        let cmds = [
            (0, SemctlCommand::Remove),
            (1, SemctlCommand::Set),
            (2, SemctlCommand::Stat),
            (18, SemctlCommand::StatBySlot),
            (500, SemctlCommand::Info),
            (19, SemctlCommand::SemInfo),
            (3, SemctlCommand::GetRaiseWaiters),
            (4, SemctlCommand::GetLastPid),
            (5, SemctlCommand::GetValue),
            (6, SemctlCommand::GetAll),
            (7, SemctlCommand::GetZeroWaiters),
            (8, SemctlCommand::SetValue),
            (9, SemctlCommand::SetAll),
        ];
        for (raw, cmd) in cmds {
            assert_eq!(SemctlCommand::from_raw(raw), Some(cmd));
            assert_eq!(cmd.to_raw(), raw);
        }
        assert_eq!(SemctlCommand::from_raw(10), None);
        assert_eq!(SemctlCommand::from_raw(-1), None);
    }

    #[test]
    fn ctl_get_set_roundtrip() {
        // C: sem.c:581-642 — set then read back; out-of-range rejected.
        let mut table = one_set();
        assert!(write_value(&mut table, 0, 1, 42, 222).unwrap());
        assert_eq!(query_scalar(&table, 0, SemctlCommand::GetValue, 1), Ok(42));
        assert_eq!(
            query_scalar(&table, 0, SemctlCommand::GetValue, 3),
            Err(SemError::Invalid)
        );
        assert_eq!(
            write_value(&mut table, 0, 1, SEMVMX as i32 + 1, 0),
            Err(SemError::Range)
        );
        assert_eq!(write_value(&mut table, 0, 9, 1, 0), Err(SemError::Invalid));
        // Whole-array path agrees with the scalar path.
        let all = read_all(&table, 0).unwrap();
        assert_eq!((all[0], all[1], all[2]), (0, 42, 0));
        let mut vals = [7u16; SEMMSL];
        vals[0] = 1;
        assert!(write_all(&mut table, 0, &vals, 333).unwrap());
        assert_eq!(query_scalar(&table, 0, SemctlCommand::GetValue, 0), Ok(1));
        assert_eq!(
            write_all(&mut table, 0, &[SEMVMX as u16 + 1], 0),
            Err(SemError::Invalid),
            "short buffer cannot cover the set"
        );
    }

    #[test]
    fn ctl_rmid_removes() {
        // Removal itself lives in table.rs; here the command resolves and
        // the stranger is stopped at the permission ladder first.
        assert_eq!(
            SemctlCommand::from_raw(IPC_RMID),
            Some(SemctlCommand::Remove)
        );
        let table = one_set();
        let set = table.get(0).unwrap();
        let stranger = Identity { uid: 777, gid: 777 };
        assert_eq!(
            authorize(&set.perm, stranger, SemctlCommand::Remove),
            Err(SemError::Ownership)
        );
        assert_eq!(
            authorize(&set.perm, caller(), SemctlCommand::Remove),
            Ok(())
        );
    }

    #[test]
    fn ctl_info_differs() {
        // C: sem.c:445-462 — summary zeroes, detail counts.
        let table = one_set();
        let summary = fill_info(&table, false);
        assert_eq!((summary.live_or_undo, summary.allocated_or_exit), (0, 0));
        let detail = fill_info(&table, true);
        assert_eq!((detail.live_or_undo, detail.allocated_or_exit), (1, 3));
        assert_eq!(
            (summary.map, summary.identifiers, summary.max_value),
            (10, 10, 32767)
        );
        assert_eq!(highest_slot_reply(&table), 0);
        let empty = SemaphoreTable::new();
        assert_eq!(highest_slot_reply(&empty), 0);
    }

    #[test]
    fn mib_array_always_ten() {
        // C: sem.c:807-834 — always SEMMNI rows; live rows carry data.
        let table = one_set();
        let (info, rows) = assemble_mib_info(&table);
        assert_eq!(rows.len(), SEMMNI);
        assert_eq!(rows[0].count, 3);
        assert_eq!(rows[0].change_time, 111);
        assert_eq!(rows[1].count, 0, "free rows are zeroed placeholders");
        assert_eq!(info.identifiers, SEMMNI as i32);
    }
}
