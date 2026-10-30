//! The PM process table: slot allocation/release, endpoint validation, PID
//! lookup.
//!
//! C correspondence: `minix3/minix/servers/pm/{glo.h, utility.c, forkexit.c}`
//! plus the kernel's generation semantics
//! (`minix3/minix/kernel/system/do_fork.c:69-72`).
//! Doc: `rewrite-notes/04-stage-pm/03-mproc-table.md`
//!
//! # The table's three identities
//!
//! - **slot**: `[0, NR_PROCS)`, the physical index shared with the kernel, VM
//!   and VFS tables;
//! - **endpoint**: `generation << 15 + slot` (the cross-service IPC identity,
//!   `minix-types` [`Endpoint`]); the kernel increments `generation` when a slot
//!   is reused (fork) (`do_fork.c:69-72`), and **PM only validates it, never
//!   creates it**;
//! - **pid**: the POSIX-visible identity, `[INIT_PID+1, NR_PIDS]`, allocated by
//!   PM through [`PidGenerator`].
//!
//! # Single-threaded model
//!
//! PM is a user-space server (a single-threaded event loop). `Cell<usize>`
//! counters and the round-robin pointer are safe under one thread; sharing
//! across threads would require `AtomicUsize`.
//!
//! # Minix3 correspondence
//!
//! - `mproc[NR_PROCS]` (mproc.h:83) + `procs_in_use` (glo.h:9) → this structure
//! - `pm_isokendpt` (utility.c:108-121) → [`ProcTable::pm_isokendpt`]
//! - `find_proc` (utility.c:76-85) → [`ProcTable::find_proc`]
//! - `cleanup` (forkexit.c:795-806) → [`ProcTable::release_slot`]
//!   (**does not bump generation**, see 03-mproc-table.md §3.3)
//! - the `do_fork` capacity check (forkexit.c:60-62) →
//!   [`ProcTable::can_alloc_for_user`]

use core::cell::Cell;
use minix_types::{Endpoint, Errno, NR_PROCS, LAST_FEW, Pid, UserSlot};
use crate::mproc::{Process, PidGenerator};

/// The PM process table.
///
/// An aggregation of four scattered C globals: `mproc[NR_PROCS]` (mproc.h:83),
/// `procs_in_use` (glo.h:9), `do_fork`'s `static next_child` (forkexit.c:51)
/// and `get_free_pid`'s `static next_pid` (utility.c:36)
/// ([ARCH: globals become `ProcTable`/`PmContext` state]).
#[derive(Debug)]
pub struct ProcTable {
    /// The process array (256 slots, about 120 KB; `size_of::<Process>()`
    /// measures 480 B per slot).
    ///
    /// A slot index is the slot part of an endpoint and the coordinate the four
    /// tables (kernel/VM/VFS/PM) share.
    pub procs: [Process; NR_PROCS],
    /// Count of live processes.
    ///
    /// C: `procs_in_use` (glo.h:9). Always equal to the number of `is_in_use()`
    /// slots in `procs[]`.
    pub procs_in_use: Cell<usize>,
    /// The next child slot (the round-robin allocation pointer).
    ///
    /// C: `static unsigned int next_child` inside `do_fork` (forkexit.c:51).
    pub next_child: Cell<usize>,
    /// The PID generator.
    ///
    /// C: `static pid_t next_pid` inside `get_free_pid` (utility.c:36).
    pub pid_generator: PidGenerator,
    /// The clock frequency (ticks/second). C: the `system_hz` global (glo.h;
    /// main.c:238 `system_hz = sys_hz()`), the basis for ticks↔timeval
    /// conversion ([ARCH: the global moves into the table]).
    pub system_hz: u32,
}

impl ProcTable {
    /// Create an empty process table: every slot `Unused`, count 0, round-robin
    /// pointer 0, PID generator ready (`next_pid = INIT_PID + 1`).
    ///
    /// C: step 1 (main.c:146-152), the mproc table initialization plus the
    /// global zero initialization.
    pub fn new() -> Self {
        Self {
            procs: core::array::from_fn(|_| Process::default()),
            procs_in_use: Cell::new(0),
            next_child: Cell::new(0),
            // C: the compile-time `HZ` preset (60 on the ibm-pc branch);
            // `init_fresh` overwrites it with the `sys_hz()` result
            // (main.c:238).
            system_hz: 60,
            pid_generator: PidGenerator::new(),
        }
    }

    /// Get a reference to the process in a slot.
    pub fn get(&self, index: usize) -> Option<&Process> {
        self.procs.get(index)
    }

    /// Get a mutable reference to the process in a slot.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Process> {
        self.procs.get_mut(index)
    }

    /// Count of live processes.
    ///
    /// C: `procs_in_use`.
    pub fn count(&self) -> usize {
        self.procs_in_use.get()
    }

    /// Whether the table is full (no empty slot).
    pub fn is_full(&self) -> bool {
        self.procs_in_use.get() >= NR_PROCS
    }

    /// Iterate over all live processes.
    ///
    /// Yields only live processes (`is_in_use()`), for table-wide scans such as
    /// PID collision detection.
    pub fn iter_active(&self) -> impl Iterator<Item = &Process> {
        self.procs.iter().filter(|p| p.is_in_use())
    }

    /// Whether a non-root user may still allocate a slot.
    ///
    /// C: the `do_fork` capacity check (forkexit.c:60-62):
    /// ```c
    /// if ((procs_in_use == NR_PROCS) ||
    ///     (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0))
    ///     return(EAGAIN);
    /// ```
    ///
    /// By C semantics `is_root` means effective uid == 0 (`mp_effuid`, see
    /// `PmContext::is_root`).
    pub fn can_alloc_for_user(&self, is_root: bool) -> bool {
        let count = self.procs_in_use.get();
        if count >= NR_PROCS {
            return false;
        }
        if count >= NR_PROCS - LAST_FEW && !is_root {
            return false;
        }
        true
    }

    /// Find an empty slot (round-robin scan).
    ///
    /// C: the `do_fork` slot scan (forkexit.c:68-74):
    /// ```c
    /// do {
    ///     next_child = (next_child+1) % NR_PROCS;
    ///     n++;
    /// } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
    /// ```
    ///
    /// Increment `next_child` before testing, as C does; `next_child` holds
    /// "the slot checked last". A full table returns `None` (the unreachable C
    /// path `panic("do_fork can't find child slot")` — the capacity check
    /// already guarantees an empty slot exists).
    pub fn find_free_slot(&self) -> Option<usize> {
        for _ in 0..NR_PROCS {
            let next = (self.next_child.get() + 1) % NR_PROCS;
            self.next_child.set(next);
            if !self.procs[next].is_in_use() {
                return Some(next);
            }
        }
        None
    }

    /// Allocate a slot: find an empty one and bump the live count.
    ///
    /// It only claims the slot and counts it; the slot contents
    /// (endpoint/pid/state) are filled in by the caller (the fork/init paths).
    /// A full table returns `None`.
    pub fn alloc_slot(&self) -> Option<usize> {
        let slot = self.find_free_slot()?;
        self.procs_in_use.set(self.procs_in_use.get() + 1);
        Some(slot)
    }

    /// Release a slot: reset the slot and decrement the live count.
    ///
    /// C: `cleanup` (forkexit.c:795-806) — it clears only
    /// `mp_pid`/`mp_flags`/child times and does `procs_in_use--`.
    ///
    /// # Deviation from C ([ARCH], 03-mproc-table.md §3.3)
    ///
    /// C keeps a stale `mp_endpoint` and blocks stale references through the
    /// `!IN_USE` test (EDEADEPT); Rust resets the whole slot to
    /// `Process::default()` (endpoint → `Endpoint::NONE`), so a stale reference
    /// fails right at the endpoint comparison — the errno contract is the same
    /// (EDEADEPT).
    ///
    /// **Incrementing generation belongs to the kernel** (`do_fork.c:69-72`:
    /// +1 on slot reuse, passed back to PM through VM), so PM must not bump it —
    /// this method does not touch the endpoint generation.
    pub fn release_slot(&mut self, index: usize) {
        if index >= NR_PROCS || self.procs_in_use.get() == 0 {
            return;
        }
        self.procs[index] = Process::default();
        self.procs_in_use.set(self.procs_in_use.get() - 1);
    }

    /// Validate an endpoint and return its slot.
    ///
    /// C: `pm_isokendpt` (utility.c:108-121):
    /// ```c
    /// *proc = _ENDPOINT_P(endpoint);
    /// if (*proc < 0 || *proc >= NR_PROCS)
    ///     return EINVAL;
    /// if (endpoint != mproc[*proc].mp_endpoint)
    ///     return EDEADEPT;
    /// if (!(mproc[*proc].mp_flags & IN_USE))
    ///     return EDEADEPT;
    /// return OK;
    /// ```
    ///
    /// The three checks run in C's order: slot range (EINVAL) → endpoint
    /// generation (EDEADEPT) → liveness (EDEADEPT). A negative slot (a kernel
    /// task) and the special ANY/NONE/SELF values all fall out at the range
    /// check → EINVAL.
    pub fn pm_isokendpt(&self, endpoint: Endpoint) -> Result<UserSlot, EndpointError> {
        let slot = endpoint.slot();
        if slot < 0 || slot as usize >= NR_PROCS {
            return Err(EndpointError::InvalidSlot);
        }
        let idx = slot as usize;
        if self.procs[idx].endpoint() != endpoint {
            return Err(EndpointError::DeadEndpoint);
        }
        if !self.procs[idx].is_in_use() {
            return Err(EndpointError::DeadEndpoint);
        }
        Ok(UserSlot::new(idx))
    }

    /// Find a process slot by pid.
    ///
    /// C: `find_proc` (utility.c:76-85) — scan for the first slot with
    /// `(mp_flags & IN_USE) && mp_pid == lpid`; no match returns `None` (the
    /// caller maps that to ESRCH).
    ///
    /// The slot is returned rather than `&Process`: a caller (trace/misc) that
    /// needs mutable access calls `get_mut()` on demand, and the table stays the
    /// authority on indexes.
    pub fn find_proc(&self, pid: Pid) -> Option<UserSlot> {
        self.procs
            .iter()
            .enumerate()
            .find(|(_, p)| p.is_in_use() && p.pid() == pid)
            .map(|(idx, _)| UserSlot::new(idx))
    }
}

impl Default for ProcTable {
    fn default() -> Self {
        Self::new()
    }
}

/// An endpoint validation error.
///
/// C: the two failure errnos of `pm_isokendpt` (utility.c:108-121):
/// - `InvalidSlot` → EINVAL (slot out of range: negative / >= NR_PROCS /
///   special endpoint);
/// - `DeadEndpoint` → EDEADEPT (endpoint generation mismatch or slot unused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointError {
    /// Slot out of range (C: EINVAL, sys/errno.h:64).
    InvalidSlot,
    /// Stale endpoint or unused slot (C: EDEADEPT, sys/errno.h:211).
    DeadEndpoint,
}

impl EndpointError {
    /// Map to a Minix3 errno.
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::InvalidSlot => Errno::EINVAL,
            Self::DeadEndpoint => Errno::EDEADEPT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::Lifecycle;

    #[test]
    fn test_proc_table_new() {
        let table = ProcTable::new();
        assert_eq!(table.count(), 0);
        assert!(!table.is_full());
        assert!(!table.procs[0].is_in_use());
    }

    #[test]
    fn test_find_free_slot_round_robin() {
        let mut table = ProcTable::new();
        // First round: the check starts at slot 1 (`next_child` is incremented
        // first, as at C forkexit.c:69).
        let slot = table.find_free_slot().unwrap();
        assert_eq!(slot, 1);
        assert_eq!(table.next_child.get(), 1);

        // Round-robin: an occupied slot is skipped.
        table.procs[2].state.lifecycle = Lifecycle::Running;
        let slot = table.find_free_slot().unwrap();
        assert_eq!(slot, 3);
        assert_eq!(table.next_child.get(), 3);
    }

    #[test]
    fn test_find_free_slot_full() {
        let mut table = ProcTable::new();
        for i in 0..NR_PROCS {
            table.procs[i].state.lifecycle = Lifecycle::Running;
        }
        assert_eq!(table.find_free_slot(), None);
    }

    #[test]
    fn test_alloc_slot() {
        let table = ProcTable::new();
        let slot = table.alloc_slot().unwrap();
        assert!(slot < NR_PROCS);
        assert_eq!(table.count(), 1);
    }

    #[test]
    fn test_alloc_slot_full_returns_none() {
        let mut table = ProcTable::new();
        for i in 0..NR_PROCS {
            table.procs[i].state.lifecycle = Lifecycle::Running;
        }
        table.procs_in_use.set(NR_PROCS);
        assert_eq!(table.alloc_slot(), None);
    }

    #[test]
    fn test_release_slot() {
        let mut table = ProcTable::new();
        let slot = table.alloc_slot().unwrap();
        assert_eq!(table.count(), 1);
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(2, slot as i32);

        table.release_slot(slot);
        assert_eq!(table.count(), 0);
        assert!(!table.procs[slot].is_in_use());
        // C's cleanup does not bump generation (forkexit.c:795-806); Rust
        // resets to default (endpoint → NONE). A new endpoint is created by the
        // kernel when the slot is reused.
        assert_eq!(table.procs[slot].endpoint(), Endpoint::NONE);
    }

    #[test]
    fn test_release_slot_keeps_count() {
        let mut table = ProcTable::new();
        table.release_slot(0); // release without allocating: the count stays
        assert_eq!(table.count(), 0);
    }

    #[test]
    fn test_can_alloc_for_user() {
        let table = ProcTable::new();
        assert!(table.can_alloc_for_user(false));
        assert!(table.can_alloc_for_user(true));

        // Reaching the reserved area: non-root is refused, root may allocate.
        table.procs_in_use.set(NR_PROCS - LAST_FEW);
        assert!(!table.can_alloc_for_user(false));
        assert!(table.can_alloc_for_user(true));

        // Completely full: root is refused too.
        table.procs_in_use.set(NR_PROCS);
        assert!(!table.can_alloc_for_user(true));
    }

    #[test]
    fn test_pm_isokendpt_valid() {
        let mut table = ProcTable::new();
        table.procs[5].state.lifecycle = Lifecycle::Running;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(3, 5);
        let ep = table.procs[5].endpoint();
        assert_eq!(table.pm_isokendpt(ep), Ok(UserSlot::new(5)));
    }

    #[test]
    fn test_pm_isokendpt_slot_out_of_range() {
        let table = ProcTable::new();
        // A negative slot (kernel task) → EINVAL.
        assert_eq!(
            table.pm_isokendpt(Endpoint::CLOCK),
            Err(EndpointError::InvalidSlot)
        );
        // Slot >= NR_PROCS → EINVAL (the same as C's `*proc >= NR_PROCS`).
        assert_eq!(
            table.pm_isokendpt(Endpoint::from_generation_slot(0, NR_PROCS as i32)),
            Err(EndpointError::InvalidSlot)
        );
        // A special endpoint (ANY/NONE/SELF) has a slot far above NR_PROCS →
        // EINVAL.
        assert_eq!(
            table.pm_isokendpt(Endpoint::ANY),
            Err(EndpointError::InvalidSlot)
        );
        assert_eq!(
            table.pm_isokendpt(Endpoint::NONE),
            Err(EndpointError::InvalidSlot)
        );
    }

    #[test]
    fn test_pm_isokendpt_generation_mismatch() {
        let mut table = ProcTable::new();
        table.procs[7].state.lifecycle = Lifecycle::Running;
        table.procs[7].identity.endpoint = Endpoint::from_generation_slot(2, 7);
        // Generation mismatch → EDEADEPT (stale reference).
        let stale = Endpoint::from_generation_slot(1, 7);
        assert_eq!(
            table.pm_isokendpt(stale),
            Err(EndpointError::DeadEndpoint)
        );
    }

    #[test]
    fn test_pm_isokendpt_not_in_use() {
        let mut table = ProcTable::new();
        // An unused slot holds a matching endpoint (the Rust counterpart of the
        // stale-endpoint case after C's cleanup: here the endpoint has been
        // reset to NONE, so this simulates C's stale value).
        table.procs[7].identity.endpoint = Endpoint::from_generation_slot(2, 7);
        let ep = table.procs[7].endpoint();
        assert_eq!(
            table.pm_isokendpt(ep),
            Err(EndpointError::DeadEndpoint)
        );
    }

    #[test]
    fn test_pm_isokendpt_released_slot() {
        let mut table = ProcTable::new();
        let slot = table.alloc_slot().unwrap();
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        let old_ep = table.procs[slot].endpoint();
        table.release_slot(slot);

        // After the release: the stale endpoint → EDEADEPT (C: the !IN_USE
        // test; Rust: NONE does not compare equal).
        assert_eq!(
            table.pm_isokendpt(old_ep),
            Err(EndpointError::DeadEndpoint)
        );
    }

    #[test]
    fn test_find_proc() {
        let mut table = ProcTable::new();
        table.procs[3].state.lifecycle = Lifecycle::Running;
        table.procs[3].identity.id.pid = 42;
        table.procs[10].state.lifecycle = Lifecycle::Running;
        table.procs[10].identity.id.pid = 43;

        assert_eq!(table.find_proc(42), Some(UserSlot::new(3)));
        assert_eq!(table.find_proc(43), Some(UserSlot::new(10)));
        assert_eq!(table.find_proc(44), None);
    }

    #[test]
    fn test_find_proc_skips_released_slot() {
        let mut table = ProcTable::new();
        // A stale pid in an unused slot does not match (C: the
        // `mp_flags & IN_USE` test, utility.c:82).
        table.procs[5].identity.id.pid = 42;
        assert_eq!(table.find_proc(42), None);
    }

    #[test]
    fn test_endpoint_error_to_errno() {
        // C: sys/errno.h:64/211.
        assert_eq!(EndpointError::InvalidSlot.to_errno(), Errno::EINVAL);
        assert_eq!(EndpointError::DeadEndpoint.to_errno(), Errno::EDEADEPT);
    }
}
