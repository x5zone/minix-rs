//! Atomic trial execution and queue retry for semaphore operations.
//!
//! C: `try_semop` / `check_set` plus the validation head of `do_semop`
//! (sem.c:294-373/:381-423/:667-739).
//! Document `06-ipc-semop.md` §3 (decisions D2/D3/D5/D7).
//!
//! The core reshape: C modifies the live values optimistically and rolls
//! back with inverse arithmetic on failure. Here trials run on a scratch
//! copy and commit only on success — same observable behaviour (all or
//! nothing), no rollback path to get wrong. Suspension counts and waiter
//! slots live in `waiter.rs` / `table.rs`; this module only judges values.

use alloc::vec::Vec;

use minix_types::{EAGAIN, IPC_NOWAIT, IPC_W, SEM_UNDO, SEMMSL, SEMOPM, SEMVMX};

use super::SemError;
use super::table::SemSet;
use super::waiter::WaiterTable;
use crate::perms::check_perm;

// ============================================================================
// Operation item and trial outcome
// ============================================================================

/// One semaphore operation: which semaphore, what change, which flags.
///
/// C: `struct sembuf` — sys/sem.h:71-75 (`sem_num`/`sem_op`/`sem_flg`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemOp {
    /// Semaphore index within the set. C: `sem_num`.
    pub num: u16,
    /// Operation: positive adds, negative subtracts, zero waits-for-zero.
    /// C: `sem_op`.
    pub op: i16,
    /// Flags (`IPC_NOWAIT`, `SEM_UNDO`). C: `sem_flg`.
    pub flag: u16,
}

/// Outcome of one trial run.
///
/// C: `try_semop` returns an integer code and reports the blocking
/// operation through an output pointer (`blkop`). The enum names the three
/// exits (document 06 §3 D3); the blocking point is an array index, never
/// a pointer into a reallocatable buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryOutcome {
    /// All operations applied.
    Done,
    /// Blocked at this operation index; the caller parks the waiter.
    Suspend {
        /// Index into the operation array that blocked.
        blocked_on: usize,
    },
    /// Failed outright (nothing applied).
    Failed(SemError),
}

/// What an operation array needs, permission-wise.
///
/// C: the mask loop in `do_semop` (sem.c:697-703): any non-zero operation
/// wants the write bit, all-zero wants the read bit. `Nothing` is the
/// empty array (`nsops == 0 → OK`, sem.c:670-671).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpNeed {
    /// Empty array: nothing to do, succeeds immediately.
    Nothing,
    /// All operations wait-for-zero: needs the read bit.
    Read,
    /// Some operation changes a value: needs the write bit.
    Write,
}

// ============================================================================
// Entry validation (pure checks of do_semop's head)
// ============================================================================

/// Validate an operation array: size, numbers, undo exclusion.
///
/// C: `do_semop` head (sem.c:670-739) minus transport (allocation, copy)
/// and the permission call itself (the caller checks the returned need
/// with `check_perm`, document 04). The undo scan rejects every
/// `SEM_UNDO` flag with `EINVAL` (the `SHRT_MAX` magic in C only skips the
/// warning print, never the rejection — sem.c:730-739).
pub fn validate_ops(ops: &[SemOp], set_count: usize) -> Result<OpNeed, SemError> {
    if ops.is_empty() {
        return Ok(OpNeed::Nothing);
    }
    if ops.len() > SEMOPM {
        return Err(SemError::TooManyOps);
    }
    let mut need = OpNeed::Read;
    for op in ops {
        if op.num as usize >= set_count {
            return Err(SemError::BadNumber);
        }
        if op.flag as i32 & SEM_UNDO != 0 {
            return Err(SemError::Invalid);
        }
        if op.op != 0 {
            need = OpNeed::Write;
        }
    }
    Ok(need)
}

/// Wanted permission bit for a validated need (`None` = nothing wanted).
pub const fn need_mask(need: OpNeed) -> Option<u32> {
    match need {
        OpNeed::Nothing => None,
        OpNeed::Read => Some(minix_types::IPC_R),
        OpNeed::Write => Some(IPC_W),
    }
}

// ============================================================================
// Trial execution on a scratch copy
// ============================================================================

/// Try all operations against a scratch value copy.
///
/// `scratch` starts as a copy of the live values; on [`TryOutcome::Done`]
/// the caller commits it (see [`commit`]). Array order is semantic (C
/// comment sem.c:302-311): later operations see earlier ones, so
/// increase-if-zero style combinations work.
///
/// Rules, line for line with `try_semop` (sem.c:314-342): positive
/// operations fail with `Range` past `SEMVMX`; negative operations with
/// insufficient value suspend (or fail with `Again` under `IPC_NOWAIT`);
/// zero operations suspend on non-zero values the same way. Negative
/// underflow is allowed on purpose (sem.c:325-328).
pub fn try_ops(scratch: &mut [u16], ops: &[SemOp]) -> TryOutcome {
    for (i, op) in ops.iter().enumerate() {
        let value = scratch[op.num as usize];
        if op.op > 0 {
            // C: sem.c:319 — int arithmetic, like the original.
            if (SEMVMX as i32 - value as i32) < op.op as i32 {
                return TryOutcome::Failed(SemError::Range);
            }
            scratch[op.num as usize] = value.wrapping_add(op.op as u16);
        } else if op.op < 0 {
            if (value as i32) < -(op.op as i32) {
                return if op.flag as i32 & IPC_NOWAIT != 0 {
                    TryOutcome::Failed(SemError::Again)
                } else {
                    TryOutcome::Suspend { blocked_on: i }
                };
            }
            scratch[op.num as usize] = value.wrapping_add(op.op as u16);
        } else if value != 0 {
            return if op.flag as i32 & IPC_NOWAIT != 0 {
                TryOutcome::Failed(SemError::Again)
            } else {
                TryOutcome::Suspend { blocked_on: i }
            };
        }
    }
    TryOutcome::Done
}

/// Commit a successful trial: copy values back, stamp actors and time.
///
/// C: the landing loop (sem.c:367-370): every touched semaphore records
/// the acting process, the set records the operation time.
pub fn commit(set: &mut SemSet, scratch: &[u16], ops: &[SemOp], pid: i32, now: u64) {
    for (i, &v) in scratch.iter().enumerate().take(set.count) {
        set.sems[i].value = v;
    }
    for op in ops {
        set.sems[op.num as usize].last_pid = pid;
    }
    set.op_time = now;
}

// ============================================================================
// Queue retry
// ============================================================================

/// Retry every waiter in queue order until a full pass wakes nobody.
///
/// C: `check_set` (sem.c:381-423): first-in-first-out for fairness, the
/// outer loop repeats while progress happens (a wake-up may unblock the
/// next waiter). Successful or failed waiters leave with a [`Wake`];
/// still-blocked waiters whose blocking point moved migrate their
/// suspension count. Returns the wake-ups in completion order.
pub fn retry(
    table: &mut WaiterTable,
    set: &mut SemSet,
    set_index: usize,
    now: u64,
) -> Vec<super::table::Wakeup> {
    use super::table::Wakeup;
    let mut wakes = Vec::new();
    let mut scratch = [0u16; SEMMSL];
    loop {
        let mut progressed = false;
        let mut cursor = 0;
        while cursor < table.queue_len(set_index) {
            let slot = table.queue_slot(set_index, cursor);
            // Copy the trial inputs out first: completion below needs a
            // mutable table borrow that cannot coexist with the view.
            let (ops, pid, endpoint, blocked_on) = {
                let (ops, pid, endpoint, blocked_on) = table.waiter_view(slot);
                (ops.to_vec(), pid, endpoint, blocked_on)
            };
            for (i, sem) in set.sems.iter().enumerate().take(set.count) {
                scratch[i] = sem.value;
            }
            match try_ops(&mut scratch, &ops) {
                TryOutcome::Done => {
                    commit(set, &scratch, &ops, pid, now);
                    table.complete_slot(slot, set);
                    wakes.push(Wakeup { endpoint, code: 0 });
                    progressed = true;
                    // Do not advance the cursor: removal shifted the tail.
                }
                TryOutcome::Failed(error) => {
                    table.complete_slot(slot, set);
                    wakes.push(Wakeup {
                        endpoint,
                        code: error.to_errno(),
                    });
                }
                TryOutcome::Suspend { blocked_on: now_at } => {
                    if now_at != blocked_on {
                        table.migrate_block(slot, set, blocked_on, now_at);
                    }
                    cursor += 1;
                }
            }
        }
        if !progressed {
            break;
        }
    }
    wakes
}

/// Permission pre-check shared by the entry path: the wanted bit must hold.
///
/// Thin wrapper so the entry sequence reads as validate → authorize → try.
pub fn authorize_ops(
    perm: &crate::perms::IpcPerm,
    caller: crate::perms::Identity,
    need: OpNeed,
) -> Result<(), SemError> {
    match need_mask(need) {
        None => Ok(()),
        Some(mask) => {
            if check_perm(perm, caller, mask) {
                Ok(())
            } else {
                Err(SemError::Access)
            }
        }
    }
}

/// Again error code (re-export for entry-path readers).
pub const AGAIN: i32 = EAGAIN;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perms::Identity;
    use crate::sem::table::SemaphoreTable;
    use crate::sem::waiter::Waiter;
    use minix_types::Endpoint;

    fn values(v: &[u16]) -> [u16; SEMMSL] {
        let mut scratch = [0u16; SEMMSL];
        scratch[..v.len()].copy_from_slice(v);
        scratch
    }

    fn op(num: u16, delta: i16, flag: u16) -> SemOp {
        SemOp {
            num,
            op: delta,
            flag,
        }
    }

    #[test]
    fn try_all_done() {
        // C: sem.c:362-372 — everything applies. Array order is semantic:
        // the third operation sees the first one's effect (5-3=2, then
        // 2-2=0), so the combination "subtract then drain" works atomically.
        let mut scratch = values(&[5, 0]);
        let ops = [op(0, -3, 0), op(1, 2, 0), op(0, -2, 0)];
        assert_eq!(try_ops(&mut scratch, &ops), TryOutcome::Done);
        assert_eq!((scratch[0], scratch[1]), (0, 2));
    }

    #[test]
    fn try_negative_short_suspends() {
        // C: sem.c:329-333 — insufficient value suspends at that index,
        // with the earlier walk visible in the scratch copy.
        let mut scratch = values(&[2]);
        let ops = [op(0, 1, 0), op(0, -5, 0)];
        assert_eq!(
            try_ops(&mut scratch, &ops),
            TryOutcome::Suspend { blocked_on: 1 }
        );
        assert_eq!(scratch[0], 3);
    }

    #[test]
    fn try_nowait_returns_again() {
        // C: sem.c:330/:337 — IPC_NOWAIT turns suspension into EAGAIN.
        let mut scratch = values(&[1]);
        assert_eq!(
            try_ops(&mut scratch, &[op(0, -3, IPC_NOWAIT as u16)]),
            TryOutcome::Failed(SemError::Again)
        );
        let mut scratch = values(&[1]);
        assert_eq!(
            try_ops(&mut scratch, &[op(0, 0, IPC_NOWAIT as u16)]),
            TryOutcome::Failed(SemError::Again)
        );
    }

    #[test]
    fn try_overflow_returns_range() {
        // C: sem.c:319-322 — adding past SEMVMX fails.
        let mut scratch = values(&[SEMVMX as u16]);
        assert_eq!(
            try_ops(&mut scratch, &[op(0, 1, 0)]),
            TryOutcome::Failed(SemError::Range)
        );
    }

    #[test]
    fn try_rollback_leaves_zero() {
        // C: sem.c:350-360 — an unwalked tail leaves no trace. With scratch
        // semantics there is simply no commit; the live set (a separate
        // copy) is untouched while the scratch shows the partial walk.
        let live = values(&[10, 5]);
        let mut scratch = live;
        let ops = [op(0, -4, 0), op(1, 0, 0)];
        assert_eq!(
            try_ops(&mut scratch, &ops),
            TryOutcome::Suspend { blocked_on: 1 }
        );
        assert_eq!(live, values(&[10, 5]), "live copy untouched");
    }

    #[test]
    fn validate_rejects_bad_index() {
        // C: sem.c:709-712 — EFBIG past the set size.
        assert_eq!(validate_ops(&[op(5, 1, 0)], 2), Err(SemError::BadNumber));
        assert_eq!(validate_ops(&[], 2), Ok(OpNeed::Nothing));
        assert_eq!(validate_ops(&[op(0, 0, 0)], 2), Ok(OpNeed::Read));
        assert_eq!(validate_ops(&[op(0, 1, 0)], 2), Ok(OpNeed::Write));
    }

    #[test]
    fn validate_rejects_undo() {
        // C: sem.c:730-739 — any SEM_UNDO flag fails (the SHRT_MAX magic
        // only skips the warning print, never the rejection).
        assert_eq!(
            validate_ops(&[op(0, 1, minix_types::SEM_UNDO as u16)], 2),
            Err(SemError::Invalid)
        );
        // Too many operations fail first.
        let many = alloc::vec![op(0, 0, 0); SEMOPM + 1];
        assert_eq!(validate_ops(&many, 2), Err(SemError::TooManyOps));
    }

    #[test]
    fn retry_wakes_fifo() {
        // C: sem.c:393-422 — queue order, chained progress.
        let mut ttable = SemaphoreTable::new();
        ttable
            .create(1, 1, 0o1000 | 0o600, Identity { uid: 1, gid: 1 }, 0)
            .unwrap();
        {
            let set = ttable.get_mut(0).unwrap();
            set.sems[0].value = 0;
        }
        let mut waiters = WaiterTable::new();
        // First waiter needs 1 (blocked); second waits-for-zero (passes
        // right away at value 0).
        for (endpoint, pid, ops) in [
            (10, 11, alloc::vec![op(0, -1, 0)]),
            (20, 21, alloc::vec![op(0, 0, 0)]),
        ] {
            let set = ttable.get_mut(0).unwrap();
            waiters.park(
                set,
                0,
                Waiter {
                    endpoint: Endpoint(endpoint),
                    pid,
                    ops,
                    blocked_on: 0,
                    set_index: 0,
                },
            );
        }
        let set = ttable.get_mut(0).unwrap();
        let wakes = retry(&mut waiters, set, 0, 777);
        // The zero-waiter completes; the other stays queued.
        assert_eq!(wakes.len(), 1);
        assert_eq!(wakes[0].code, 0);
        assert_eq!(wakes[0].endpoint, Endpoint(20));
        assert_eq!(waiters.queue_len(0), 1);
        // The completed semaphore records actor and time.
        assert_eq!(set.sems[0].last_pid, 21);
        assert_eq!(set.op_time, 777);
    }
}
