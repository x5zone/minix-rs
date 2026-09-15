//! The service layer: one struct that assembles the decision modules into
//! a working IPC server (documents 05-08 wiring; 13-stage-ipc/todo.md
//! IPC-P1-1).
//!
//! The rule this module lives by (set in `server.rs`'s header, IPC-P2-1):
//! judgement lives in the decision modules (`sem/`, `shm/`, `perms.rs`,
//! `events.rs`, `mib_tree.rs`); this layer only sequences — decode the
//! message, run the C-ordered decisions, hand effects to the boundary.
//! Every method below transliterates one C function's call order; where C
//! inlines a cross-service call, the boundary trait carries the verb.
//!
//! [`IpcBoundary`] is that verb set: kernel clock, PM credentials, data
//! copies, process-event subscription, waiter wakeups, and the VM verbs
//! shared memory needs. The in-crate [`TestBoundary`] drives the tests
//! without a kernel; the production implementation (thin `minix-sys`
//! wrappers) lands with E-IPCWIRE — the descriptor wire layouts it needs
//! are registered there too (item 8), which is also why the MIB protocol
//! glue (C `rmib_process` calling our two info assemblers) rides the
//! boundary instead of reaching into `minix-sys` directly: the pure
//! halves of that glue exist (`mib_tree.rs` routing, `assemble_mib_info`
//! on both tables) and are tested; only the protocol transport is
//! outstanding.

use alloc::boxed::Box;
use alloc::vec::Vec;

use minix_types::{
    Endpoint, IpcCall, Message, E2BIG, EACCES, EINVAL, ENOMEM, IPC_CREAT, IPC_PRIVATE, OK,
    SEMMSL, SUSPEND,
};

use crate::dispatch::proc_event_reply_type;
use crate::events::{SyncAction, Subscription};
use crate::perms::{Identity, SetOptions, check_perm};
use crate::sem::ctl::{self, SemctlCommand};
use crate::sem::op::{SemOp, TryOutcome, commit, retry, try_ops, validate_ops};
use crate::sem::table::{SemaphoreTable, TableEffect};
use crate::sem::waiter::{Waiter, WaiterTable};
use crate::shm::attach::{
    self, ShmctlCommand, align_addr, apply_set, attach_mask, find_by_phys, mark_destroy,
    record_attach, record_detach,
};
use crate::shm::refcount::{RefQuery, sweep};
use crate::shm::segment::{Backing, CreateParams, ShmTable};
use crate::shm::ShmIdView;
use crate::server::CallHandler;

/// The request caller's PM-backed credentials (who, and which pid).
///
/// C reads the three fields at need (`getnuid`/`getngid` — utility.c:10-11,
/// `getnpid` — sem.c:720); one lookup serves both here so a production
/// implementation pays one PM round trip per request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Credentials {
    /// Caller user and group (the permission identity).
    pub identity: Identity,
    /// Caller process id (the actor stamped on semaphores and segments).
    pub pid: i32,
}

// ============================================================================
// Boundary: every cross-service verb the sequencing needs
// ============================================================================

/// Cross-service effects, as verbs.
///
/// One trait, not several: the verbs share one production implementation
/// (a thin `minix-sys` wrapper) and one test double, and splitting them
/// would only re-merge at the impl site. Kernel-call mapping notes live
/// on each method; the production implementation maps verb → trap.
pub trait IpcBoundary {
    /// Kernel clock (C: `clock_time(NULL)`).
    fn now(&self) -> u64;

    /// PM-backed credentials for a caller endpoint
    /// (C: `getnuid`/`getngid`/`getnpid`). `None`: PM does not know the
    /// endpoint — a caller without credentials is refused.
    fn credentials_of(&self, endpoint: Endpoint) -> Option<Credentials>;

    /// Copy the semop operation array in from the caller
    /// (C: `sys_datacopy`, sem.c:680-682; allocation failure → `ENOMEM`,
    /// copy failure → the kernel's errno, `EFAULT` family).
    fn copy_in_ops(&self, from: Endpoint, ptr: u32, count: usize) -> Result<Vec<SemOp>, i32>;

    /// Copy a `SETALL` value array in (C: sem.c:615-617 — exactly
    /// `sem_nsems` elements; short buffers surface as copy errors here).
    fn copy_in_setall(&self, from: Endpoint, ptr: u32, count: usize) -> Result<Vec<u16>, i32>;

    /// Copy a `GETALL` value array out (C: sem.c:584-588 — exactly
    /// `sem_nsems` elements of the buffer).
    fn copy_out_getall(
        &self,
        to: Endpoint,
        ptr: u32,
        values: &[u16],
        count: usize,
    ) -> Result<(), i32>;

    /// Copy a `semid_ds` status snapshot out (C: sem.c:553 `IPC_STAT` /
    /// :547 `SEM_STAT` copyout). The wire layout is the boundary's
    /// business (E-IPCWIRE item 8); the service supplies the fields.
    fn copy_out_sem_stat(&self, to: Endpoint, ptr: u32, view: &ctl::SemIdView)
        -> Result<(), i32>;

    /// Copy a `seminfo` summary/detail block out (C: sem.c:566-572).
    fn copy_out_seminfo(&self, to: Endpoint, ptr: u32, info: &ctl::SemInfo) -> Result<(), i32>;

    /// Copy an `IPC_SET` permission draft in (C: sem.c:551-553 — the perm
    /// half of the caller's descriptor).
    fn copy_in_set_options(&self, from: Endpoint, ptr: u32) -> Result<SetOptions, i32>;

    /// Copy a `shmid_ds` status snapshot out (C: shm.c:306-309).
    fn copy_out_shm_stat(&self, to: Endpoint, ptr: u32, view: &ShmIdView) -> Result<(), i32>;

    /// Copy a `shminfo` block out (C: shm.c:349-352 — all constants).
    fn copy_out_shminfo(
        &self,
        to: Endpoint,
        ptr: u32,
        summary: &attach::ShmSummary,
    ) -> Result<(), i32>;

    /// Copy a `shm_info` aggregate out (C: shm.c:354-364).
    fn copy_out_shmagg(
        &self,
        to: Endpoint,
        ptr: u32,
        agg: &attach::ShmInfoAgg,
    ) -> Result<(), i32>;

    /// Subscribe to or unsubscribe from PM process events
    /// (C: `proceventmask` — main.c:163-166).
    fn set_proceventmask(&self, subscribe: bool);

    /// Send a waiter's completion reply (C: `send_reply` → `ipc_sendnb`,
    /// sem.c:207-215). `NO_REPLY` codes are swallowed here, mirroring
    /// C's `complete_semop`.
    fn send_wakeup(&self, endpoint: Endpoint, code: i32);

    /// Process one MIB request in place (C: `rmib_process` — main.c:250).
    /// The production implementation wraps the `minix-sys` rmib client
    /// with our two info assemblers; the test double records the visit.
    fn mib_process(&mut self, msg: &mut Message);

    /// Back a fresh segment: anonymous mapping plus physical snapshot
    /// (C: `mmap(MAP_ANON)` + `vm_getphys` — shm.c:113-118). `Err(ENOMEM)`
    /// on mapping failure.
    fn back_segment(&self, bytes: u64) -> Result<Backing, i32>;

    /// Map a segment into the caller's space (C: `vm_remap` —
    /// shm.c:158-160). `None` = `MAP_FAILED` (`ENOMEM` at the caller).
    fn remap(&self, caller: Endpoint, addr: u64, local: u64, bytes: u64) -> Option<u64>;

    /// Physical address behind a caller address (C: `vm_getphys` —
    /// shm.c:215-216). `None` = zero (the caller's `EINVAL`).
    fn phys_of(&self, caller: Endpoint, addr: u64) -> Option<u64>;

    /// Unmap one caller mapping (C: `vm_unmap` — shm.c:232).
    fn unmap(&self, caller: Endpoint, addr: u64);

    /// Reference count of one of our mappings, ours included
    /// (C: `vm_getrefcount` — shm.c:181). `None` = region unknown (the
    /// `u8` `-1` sentinel), warned about and skipped in the sweep.
    fn refcount_of(&self, local: u64) -> Option<u8>;

    /// Release one of our mappings (C: `munmap` — shm.c:191-193).
    fn release_mapping(&self, addr: u64, len: u64);
}

// ============================================================================
// Service
// ============================================================================

/// The assembled IPC server state: both tables, the waiter slots, the
/// event subscription, and the boundary that executes effects.
///
/// The shared-memory table sits behind a `Box`: 1024 inline slots are
/// ~90 KB, and the service is a long-lived heap resident, not a stack
/// temporary (C's `shm_list` is a static global — the equivalent home is
/// the heap; IPC-P3-1).
pub struct IpcService<B: IpcBoundary> {
    boundary: B,
    sems: SemaphoreTable,
    waiters: WaiterTable,
    subscription: Subscription,
    shms: Box<ShmTable>,
}

impl<B: IpcBoundary> IpcService<B> {
    /// Assemble a service around a boundary. Tables start empty, the
    /// subscription off — the state a fresh `sef_cb_init_fresh` finds.
    pub fn new(boundary: B) -> Self {
        Self {
            boundary,
            sems: SemaphoreTable::new(),
            waiters: WaiterTable::new(),
            subscription: Subscription::new(),
            shms: Box::new(ShmTable::new()),
        }
    }

    /// Credentials or refuse: a caller PM cannot vouch for has no valid
    /// permission identity (C's `getnuid` cannot fail; this arm exists so
    /// the production boundary can say "unknown endpoint").
    fn creds(&self, endpoint: Endpoint) -> Result<Credentials, i32> {
        self.boundary.credentials_of(endpoint).ok_or(EINVAL)
    }

    /// Run the queue retry and deliver every owed wakeup (C:
    /// `check_set` + `complete_semop`'s send — sem.c:381-423/:223-244).
    /// `NO_REPLY` codes are swallowed by the boundary.
    fn retry_and_wake(&mut self, index: usize) {
        let now = self.boundary.now();
        let set = self.sems.get_mut(index).expect("retry target is live");
        for wake in retry(&mut self.waiters, set, index, now) {
            self.boundary.send_wakeup(wake.endpoint, wake.code);
        }
    }

    /// Sync the process-event subscription after a demand edge (C:
    /// `update_sem_sub`/`update_sub` — main.c:144-189). No edge, no call.
    fn sync_subscription(&mut self, want: bool) {
        if let Some(action) = self.subscription.set_sem_want(want) {
            self.boundary.set_proceventmask(action == SyncAction::Subscribe);
        }
    }

    /// Refresh every live segment's attach count from the boundary and
    /// destroy what is due (C: `update_refcount_and_destroy` —
    /// shm.c:173-206). Unmap requests execute here — the sweep produced
    /// them, this layer performs them.
    fn sweep_segments(&mut self) {
        let mut queries = Vec::new();
        for index in 0..self.shms.live_count() {
            if let Some(seg) = self.shms.get(index) {
                queries.push(RefQuery {
                    slot: index,
                    count: self.boundary.refcount_of(seg.backing.local),
                });
            }
        }
        let plan = sweep(&mut self.shms, &queries);
        for req in &plan.unmaps {
            self.boundary.release_mapping(req.addr, req.len);
        }
    }

    // ------------------------------------------------------------------
    // semget (C: do_semget — sem.c:93-156)
    // ------------------------------------------------------------------

    fn semget(&mut self, msg: &mut Message) -> i32 {
        let req = minix_types::IpcSemgetIn::decode_message(msg);
        let creds = match self.creds(req.caller) {
            Ok(c) => c,
            Err(e) => return e,
        };
        match self
            .sems
            .create(req.key, req.count, req.flag, creds.identity, self.boundary.now())
        {
            Ok((id, effect)) => {
                if effect == TableEffect::SubscribeEvents {
                    self.sync_subscription(true);
                }
                msg.m_u.m_lc_ipc_semget.retid = id;
                OK
            }
            Err(e) => e.to_errno(),
        }
    }

    // ------------------------------------------------------------------
    // semctl (C: do_semctl — sem.c:469-648)
    // ------------------------------------------------------------------

    fn semctl(&mut self, msg: &mut Message) -> i32 {
        let req = minix_types::IpcSemctlIn::decode_message(msg);
        let Some(cmd) = SemctlCommand::from_raw(req.command) else {
            return EINVAL;
        };
        let creds = match self.creds(req.caller) {
            Ok(c) => c,
            Err(e) => return e,
        };
        // Lookup rules (sem.c:491-507): the two info commands carry no
        // set; SEM_STAT takes a slot index; everything else an id.
        let slot = match cmd {
            SemctlCommand::Info | SemctlCommand::SemInfo => None,
            SemctlCommand::StatBySlot => {
                if req.id < 0 || req.id as usize >= self.sems.live_count() {
                    return EINVAL;
                }
                Some(req.id as usize)
            }
            _ => match self.sems.find_id(req.id) {
                Some(index) => Some(index),
                None => return EINVAL,
            },
        };
        // Permission ladder (sem.c:516-538; 04's mask table).
        if let Some(index) = slot {
            let perm = &self.sems.get(index).expect("live slot").perm;
            if let Err(e) = ctl::authorize(perm, creds.identity, cmd) {
                return e.to_errno();
            }
        }
        match cmd {
            SemctlCommand::Stat | SemctlCommand::StatBySlot => {
                let index = slot.expect("stat has a set");
                let set = self.sems.get(index).expect("live slot");
                let view = ctl::SemIdView {
                    perm: crate::perms::IpcPermSysctl::from_perm(&set.perm),
                    count: set.count as u16,
                    op_time: set.op_time,
                    change_time: set.change_time,
                };
                match self
                    .boundary
                    .copy_out_sem_stat(req.caller, req.option, &view)
                {
                    Ok(()) => {
                        if cmd == SemctlCommand::StatBySlot {
                            msg.m_u.m_lc_ipc_semctl.ret =
                                ctl::stat_reply_id(&self.sems, index).expect("live above");
                        }
                        OK
                    }
                    Err(e) => e,
                }
            }
            SemctlCommand::Set => {
                let index = slot.expect("set has a target");
                match self
                    .boundary
                    .copy_in_set_options(req.caller, req.option)
                {
                    Ok(options) => {
                        let now = self.boundary.now();
                        match ctl::apply_set(&mut self.sems, index, options, now) {
                            Ok(()) => OK,
                            Err(e) => e.to_errno(),
                        }
                    }
                    Err(e) => e,
                }
            }
            SemctlCommand::Remove => {
                let index = slot.expect("remove has a target");
                for wake in self.waiters.drain_set(index) {
                    self.boundary.send_wakeup(wake.endpoint, wake.code);
                }
                // C calls update_sem_sub(FALSE) only when the table
                // emptied (sem.c:279-280) — mid-life removals make no
                // subscription edge.
                let effect = self.sems.remove(index);
                if effect == TableEffect::UnsubscribeEvents {
                    self.sync_subscription(false);
                }
                OK
            }
            SemctlCommand::Info | SemctlCommand::SemInfo => {
                let info = ctl::fill_info(&self.sems, cmd == SemctlCommand::SemInfo);
                match self.boundary.copy_out_seminfo(req.caller, req.option, &info) {
                    Ok(()) => {
                        msg.m_u.m_lc_ipc_semctl.ret = ctl::highest_slot_reply(&self.sems);
                        OK
                    }
                    Err(e) => e,
                }
            }
            SemctlCommand::GetAll => {
                let index = slot.expect("getall has a target");
                let count = self.sems.get(index).expect("live above").count;
                match ctl::read_all(&self.sems, index) {
                    Ok(values) => {
                        match self
                            .boundary
                            .copy_out_getall(req.caller, req.option, &values, count)
                        {
                            Ok(()) => OK,
                            Err(e) => e,
                        }
                    }
                    Err(e) => e.to_errno(),
                }
            }
            SemctlCommand::SetAll => {
                let index = slot.expect("setall has a target");
                let count = self.sems.get(index).expect("live above").count;
                match self.boundary.copy_in_setall(req.caller, req.option, count) {
                    Ok(values) => {
                        let now = self.boundary.now();
                        match ctl::write_all(&mut self.sems, index, &values, now) {
                            Ok(()) => {
                                self.retry_and_wake(index);
                                OK
                            }
                            Err(e) => e.to_errno(),
                        }
                    }
                    Err(e) => e,
                }
            }
            SemctlCommand::SetValue => {
                let index = slot.expect("setval has a target");
                let now = self.boundary.now();
                match ctl::write_value(&mut self.sems, index, req.number, req.option as i32, now)
                {
                    Ok(()) => {
                        self.retry_and_wake(index);
                        OK
                    }
                    Err(e) => e.to_errno(),
                }
            }
            SemctlCommand::GetValue
            | SemctlCommand::GetLastPid
            | SemctlCommand::GetRaiseWaiters
            | SemctlCommand::GetZeroWaiters => {
                let index = slot.expect("scalar getters have a target");
                match ctl::query_scalar(&self.sems, index, cmd, req.number) {
                    Ok(value) => {
                        msg.m_u.m_lc_ipc_semctl.ret = value;
                        OK
                    }
                    Err(e) => e.to_errno(),
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // semop (C: do_semop — sem.c:654-779)
    // ------------------------------------------------------------------

    fn semop(&mut self, msg: &mut Message) -> i32 {
        let req = minix_types::IpcSemopIn::decode_message(msg);
        let Some(index) = self.sems.find_id(req.id) else {
            return EINVAL;
        };
        if req.operation_count == 0 {
            return OK;
        }
        if req.operation_count > minix_types::SEMOPM as u32 {
            return E2BIG;
        }
        let ops = match self.boundary.copy_in_ops(
            req.caller,
            req.operations_address,
            req.operation_count as usize,
        ) {
            Ok(ops) => ops,
            Err(e) => return e,
        };
        let creds = match self.creds(req.caller) {
            Ok(c) => c,
            Err(e) => return e,
        };
        // Trial on a scratch copy (C: try_semop — sem.c:294-373), after
        // the C-ordered entry checks — permission before number validity
        // (sem.c:690-693; IPC-P1-2).
        let set = self.sems.get(index).expect("live above");
        let mut scratch = [0u16; SEMMSL];
        for (i, sem) in set.sems.iter().enumerate().take(set.count) {
            scratch[i] = sem.value;
        }
        let outcome = {
            let perm = &set.perm;
            match validate_ops(&ops, set.count, perm, creds.identity) {
                Ok(_) => try_ops(&mut scratch, &ops),
                Err(e) => TryOutcome::Failed(e),
            }
        };
        match outcome {
            TryOutcome::Done => {
                let now = self.boundary.now();
                let set = self.sems.get_mut(index).expect("live above");
                commit(set, &scratch, &ops, creds.pid, now);
                self.retry_and_wake(index);
                OK
            }
            TryOutcome::Failed(e) => e.to_errno(),
            TryOutcome::Suspend { blocked_on } => {
                let set = self.sems.get_mut(index).expect("live above");
                self.waiters.park(
                    set,
                    index,
                    Waiter {
                        endpoint: req.caller,
                        pid: creds.pid,
                        ops,
                        blocked_on,
                        set_index: index,
                    },
                );
                SUSPEND
            }
        }
    }

    // ------------------------------------------------------------------
    // shmget (C: do_shmget — shm.c:51-127)
    // ------------------------------------------------------------------

    fn shmget(&mut self, msg: &mut Message) -> i32 {
        let req = minix_types::IpcShmgetIn::decode_message(msg);
        let creds = match self.creds(req.caller) {
            Ok(c) => c,
            Err(e) => return e,
        };
        // Fresh-create backs the mapping first (document 07 D2: a failed
        // mapping never reaches the table); the backing happens only when
        // the create would actually allocate, so an existing-key open or
        // a missing-create-flag miss never maps.
        let exists = req.key != IPC_PRIVATE && self.shms.find_key(req.key).is_some();
        let will_create = !exists && req.flag & IPC_CREAT != 0;
        let backing = if will_create {
            if req.size == 0 {
                return EINVAL;
            }
            match self
                .boundary
                .back_segment(crate::shm::segment::round_up(req.size))
            {
                Ok(backing) => backing,
                Err(e) => return e,
            }
        } else {
            Backing {
                local: 0,
                phys: 0,
            }
        };
        let params = CreateParams {
            key: req.key,
            size: req.size,
            flag: req.flag,
            caller: creds.identity,
            backing,
            now: self.boundary.now(),
            cpid: creds.pid,
        };
        match self.shms.create(params) {
            Ok(id) => {
                msg.m_u.m_lc_ipc_shmget.retid = id;
                OK
            }
            Err(crate::shm::ShmError::NoSpace) => {
                if will_create {
                    self.boundary.release_mapping(
                        backing.local,
                        crate::shm::segment::round_up(req.size),
                    );
                }
                crate::shm::ShmError::NoSpace.to_errno()
            }
            Err(e) => e.to_errno(),
        }
    }

    // ------------------------------------------------------------------
    // shmat (C: do_shmat — shm.c:130-169)
    // ------------------------------------------------------------------

    fn shmat(&mut self, msg: &mut Message) -> i32 {
        let req = minix_types::IpcShmatIn::decode_message(msg);
        let Some(index) = self.shms.find_id(req.id) else {
            return EINVAL;
        };
        let addr = match align_addr(req.address.get(), req.flag as u32) {
            Ok(addr) => addr,
            Err(e) => return e.to_errno(),
        };
        let creds = match self.creds(req.caller) {
            Ok(c) => c,
            Err(e) => return e,
        };
        {
            let seg = self.shms.get(index).expect("live above");
            if !check_perm(&seg.perm, creds.identity, attach_mask(req.flag as u32)) {
                return EACCES;
            }
        }
        let (local, bytes) = {
            let seg = self.shms.get(index).expect("live above");
            (seg.backing.local, seg.size_bytes)
        };
        let Some(mapped) = self.boundary.remap(req.caller, addr, local, bytes) else {
            return ENOMEM;
        };
        if let Err(e) =
            record_attach(&mut self.shms, index, creds.pid, self.boundary.now())
        {
            return e.to_errno();
        }
        msg.m_u.m_lc_ipc_shmat.retaddr = mapped as u32;
        OK
    }

    // ------------------------------------------------------------------
    // shmdt (C: do_shmdt — shm.c:208-242)
    // ------------------------------------------------------------------

    fn shmdt(&mut self, msg: &mut Message) -> i32 {
        let req = minix_types::IpcShmdtIn::decode_message(msg);
        let Some(vm_id) = self.boundary.phys_of(req.caller, req.address.get()) else {
            return EINVAL;
        };
        if let Some(index) = find_by_phys(&self.shms, vm_id) {
            let creds = match self.creds(req.caller) {
                Ok(c) => c,
                Err(e) => return e,
            };
            // C refreshes shm_ATIME here (shm.c:228-229) — the quirk is a
            // contract (IPC-P1-5).
            if let Err(e) =
                record_detach(&mut self.shms, index, creds.pid, self.boundary.now())
            {
                return e.to_errno();
            }
            self.boundary.unmap(req.caller, req.address.get());
        }
        // A miss logs (in C) and still succeeds (shm.c:236-241); either
        // way the detach tail runs the sweep like C's :241 call.
        self.sweep_segments();
        OK
    }

    // ------------------------------------------------------------------
    // shmctl (C: do_shmctl — shm.c:261-371)
    // ------------------------------------------------------------------

    fn shmctl(&mut self, msg: &mut Message) -> i32 {
        let req = minix_types::IpcShmctlIn::decode_message(msg);
        let Some(cmd) = ShmctlCommand::from_raw(req.command) else {
            return EINVAL;
        };
        // Status reads refresh the counts first — the answer may free a
        // slot, so this runs before the lookup (shm.c:280-282).
        if matches!(cmd, ShmctlCommand::Stat | ShmctlCommand::StatBySlot) {
            self.sweep_segments();
        }
        let slot = match cmd {
            ShmctlCommand::Info | ShmctlCommand::Aggregate => None,
            ShmctlCommand::StatBySlot => {
                if req.id < 0 || req.id as usize >= self.shms.live_count() {
                    return EINVAL;
                }
                Some(req.id as usize)
            }
            _ => match self.shms.find_id(req.id) {
                Some(index) => Some(index),
                None => return EINVAL,
            },
        };
        let creds = match self.creds(req.caller) {
            Ok(c) => c,
            Err(e) => return e,
        };
        if let Some(index) = slot {
            let perm = &self.shms.get(index).expect("live slot").perm;
            if let Err(e) = attach::authorize(perm, creds.identity, cmd) {
                return e.to_errno();
            }
        }
        match cmd {
            ShmctlCommand::Stat | ShmctlCommand::StatBySlot => {
                let index = slot.expect("stat has a segment");
                let seg = self.shms.get(index).expect("live above");
                let view = ShmIdView {
                    perm: crate::perms::IpcPermSysctl::from_perm(&seg.perm),
                    size: seg.size_bytes,
                    last_pid: seg.last_pid,
                    creator_pid: seg.creator_pid,
                    attach_time: seg.attach_time,
                    detach_time: seg.detach_time,
                    change_time: seg.change_time,
                    attached: seg.attached,
                };
                match self.boundary.copy_out_shm_stat(req.caller, req.buffer, &view) {
                    Ok(()) => {
                        if cmd == ShmctlCommand::StatBySlot {
                            msg.m_u.m_lc_ipc_shmctl.ret = crate::shm::segment::encode_id(
                                index,
                                self.shms.get(index).expect("live above").perm.seq,
                            );
                        }
                        OK
                    }
                    Err(e) => e,
                }
            }
            ShmctlCommand::Set => {
                let index = slot.expect("set has a target");
                match self.boundary.copy_in_set_options(req.caller, req.buffer) {
                    Ok(options) => {
                        let now = self.boundary.now();
                        match apply_set(&mut self.shms, index, options, now) {
                            Ok(()) => OK,
                            Err(e) => e.to_errno(),
                        }
                    }
                    Err(e) => e,
                }
            }
            ShmctlCommand::Remove => {
                let index = slot.expect("remove has a target");
                if let Err(e) = mark_destroy(&mut self.shms, index) {
                    return e.to_errno();
                }
                // C destroys as far as possible right now (shm.c:335-336).
                self.sweep_segments();
                OK
            }
            ShmctlCommand::Info => {
                let summary = attach::fill_summary();
                match self.boundary.copy_out_shminfo(req.caller, req.buffer, &summary) {
                    Ok(()) => {
                        msg.m_u.m_lc_ipc_shmctl.ret = attach::highest_slot_reply(&self.shms);
                        OK
                    }
                    Err(e) => e,
                }
            }
            ShmctlCommand::Aggregate => {
                let agg = attach::aggregate(&self.shms);
                match self.boundary.copy_out_shmagg(req.caller, req.buffer, &agg) {
                    Ok(()) => {
                        msg.m_u.m_lc_ipc_shmctl.ret = attach::highest_slot_reply(&self.shms);
                        OK
                    }
                    Err(e) => e,
                }
            }
        }
    }
}

impl<B: IpcBoundary> CallHandler for IpcService<B> {
    fn handle_call(&mut self, call: IpcCall, msg: &mut Message) -> i32 {
        match call {
            IpcCall::Semget => self.semget(msg),
            IpcCall::Semctl => self.semctl(msg),
            IpcCall::Semop => self.semop(msg),
            IpcCall::Shmget => self.shmget(msg),
            IpcCall::Shmat => self.shmat(msg),
            IpcCall::Shmdt => self.shmdt(msg),
            IpcCall::Shmctl => self.shmctl(msg),
        }
    }

    fn handle_proc_event(&mut self, event: minix_types::ProcEventIn) -> i32 {
        // C gates the cancellation on the subscription demand
        // (main.c:203-204); with no demand there are no waiters, so the
        // gate is an optimisation kept for fidelity.
        if self.subscription.wants() {
            // C: sem_process_event(endpt, has_exited) — sem.c:866-888.
            // The cancel walks the endpoint's single slot; scanning the
            // live sets matches C's slot lookup, because a waiter lives
            // on exactly one set (single suspension).
            for index in 0..self.sems.live_count() {
                let Some(set) = self.sems.get_mut(index) else {
                    continue;
                };
                if let Some(wake) = self
                    .waiters
                    .cancel(set, event.endpoint, event.exited)
                {
                    self.boundary.send_wakeup(wake.endpoint, wake.code);
                }
            }
        }
        proc_event_reply_type()
    }

    fn handle_mib(&mut self, msg: &mut Message) {
        self.boundary.mib_process(msg);
    }

    fn on_cycle_end(&mut self) {
        self.sweep_segments();
    }
}

// ============================================================================
// Test boundary
// ============================================================================

#[cfg(test)]
pub(crate) mod test_boundary {
    use super::*;
    use alloc::collections::BTreeMap;
    use core::cell::{Cell, RefCell};
    use minix_types::{EINTR, EFAULT};
    use crate::sem::NO_REPLY;

    /// In-memory boundary: canned clock ticks, credentials, copy payloads
    /// and VM answers; recorded effects for assertions. Effect fields sit
    /// behind `RefCell` because the trait verbs take `&self` (C's
    /// `sys_datacopy` and friends do not mutate the server either).
    #[derive(Default)]
    pub struct TestBoundary {
        clock: Vec<u64>,
        creds: BTreeMap<i32, Credentials>,
        ops_buffer: RefCell<Vec<SemOp>>,
        setall_buffer: RefCell<Vec<u16>>,
        set_options: RefCell<Option<SetOptions>>,
        masks: RefCell<Vec<bool>>,
        wakeups: RefCell<Vec<(Endpoint, i32)>>,
        mib_visits: Cell<u32>,
        backings: RefCell<Vec<Backing>>,
        remap_fail: Cell<bool>,
        unmaps: RefCell<Vec<(i32, u64)>>,
        refcounts: RefCell<BTreeMap<u64, Option<u8>>>,
        released: RefCell<Vec<(u64, u64)>>,
    }

    impl TestBoundary {
        /// Ten canned ticks; the clock stops at 77 when the script ends.
        pub fn new() -> Self {
            Self {
                clock: alloc::vec![100, 200, 300, 400, 500, 600, 700, 800, 900, 1000],
                ..Self::default()
            }
        }

        pub fn with_creds(mut self, endpoint: i32, uid: u32) -> Self {
            self.creds.insert(
                endpoint,
                Credentials {
                    identity: Identity {
                        uid,
                        gid: uid * 2,
                    },
                    pid: endpoint * 10,
                },
            );
            self
        }

        pub fn set_ops(&self, ops: &[SemOp]) {
            *self.ops_buffer.borrow_mut() = ops.to_vec();
        }

        pub fn set_setall(&self, values: &[u16]) {
            *self.setall_buffer.borrow_mut() = values.to_vec();
        }

        pub fn set_options(&self, options: SetOptions) {
            *self.set_options.borrow_mut() = Some(options);
        }

        pub fn masks(&self) -> alloc::vec::Vec<bool> {
            self.masks.borrow().clone()
        }

        pub fn wakeups(&self) -> alloc::vec::Vec<(Endpoint, i32)> {
            self.wakeups.borrow().clone()
        }

        pub fn released(&self) -> alloc::vec::Vec<(u64, u64)> {
            self.released.borrow().clone()
        }

        pub fn backings(&self) -> alloc::vec::Vec<Backing> {
            self.backings.borrow().clone()
        }

        pub fn set_refcount(&self, local: u64, count: Option<u8>) {
            self.refcounts.borrow_mut().insert(local, count);
        }

        pub fn mib_visits(&self) -> u32 {
            self.mib_visits.get()
        }
    }

    impl IpcBoundary for TestBoundary {
        fn now(&self) -> u64 {
            self.clock.first().copied().unwrap_or(77)
        }

        fn credentials_of(&self, endpoint: Endpoint) -> Option<Credentials> {
            self.creds.get(&endpoint.0).copied()
        }

        fn copy_in_ops(&self, _from: Endpoint, _ptr: u32, count: usize) -> Result<Vec<SemOp>, i32> {
            let buffer = self.ops_buffer.borrow();
            if buffer.len() < count {
                return Err(EFAULT);
            }
            Ok(buffer[..count].to_vec())
        }

        fn copy_in_setall(
            &self,
            _from: Endpoint,
            _ptr: u32,
            count: usize,
        ) -> Result<Vec<u16>, i32> {
            let buffer = self.setall_buffer.borrow();
            if buffer.len() < count {
                return Err(EFAULT);
            }
            Ok(buffer[..count].to_vec())
        }

        fn copy_out_getall(
            &self,
            _to: Endpoint,
            _ptr: u32,
            _values: &[u16],
            _count: usize,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn copy_out_sem_stat(
            &self,
            _to: Endpoint,
            _ptr: u32,
            _view: &ctl::SemIdView,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn copy_out_seminfo(
            &self,
            _to: Endpoint,
            _ptr: u32,
            _info: &ctl::SemInfo,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn copy_in_set_options(&self, _from: Endpoint, _ptr: u32) -> Result<SetOptions, i32> {
            self.set_options.borrow().ok_or(EFAULT)
        }

        fn copy_out_shm_stat(
            &self,
            _to: Endpoint,
            _ptr: u32,
            _view: &ShmIdView,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn copy_out_shminfo(
            &self,
            _to: Endpoint,
            _ptr: u32,
            _summary: &attach::ShmSummary,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn copy_out_shmagg(
            &self,
            _to: Endpoint,
            _ptr: u32,
            _agg: &attach::ShmInfoAgg,
        ) -> Result<(), i32> {
            Ok(())
        }

        fn set_proceventmask(&self, subscribe: bool) {
            self.masks.borrow_mut().push(subscribe);
        }

        fn send_wakeup(&self, endpoint: Endpoint, code: i32) {
            if code != NO_REPLY {
                self.wakeups.borrow_mut().push((endpoint, code));
            }
        }

        fn mib_process(&mut self, _msg: &mut Message) {
            self.mib_visits.set(self.mib_visits.get() + 1);
        }

        fn back_segment(&self, _bytes: u64) -> Result<Backing, i32> {
            let mut backings = self.backings.borrow_mut();
            let local = 0x5000_0000 + backings.len() as u64 * 0x10_000;
            let backing = Backing {
                local,
                phys: local,
            };
            backings.push(backing);
            Ok(backing)
        }

        fn remap(&self, caller: Endpoint, _addr: u64, _local: u64, _bytes: u64) -> Option<u64> {
            Some(0x7000_0000 + caller.0 as u64 * 0x1000)
        }

        fn phys_of(&self, _caller: Endpoint, _addr: u64) -> Option<u64> {
            self.backings.borrow().first().map(|b| b.phys)
        }

        fn unmap(&self, caller: Endpoint, addr: u64) {
            self.unmaps.borrow_mut().push((caller.0, addr));
        }

        fn refcount_of(&self, local: u64) -> Option<u8> {
            self.refcounts.borrow().get(&local).copied().unwrap_or(Some(1))
        }

        fn release_mapping(&self, addr: u64, len: u64) {
            self.released.borrow_mut().push((addr, len));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_boundary::TestBoundary;
    use super::*;
    use minix_types::{EINTR, MessLcIpcSemctl, MessLcIpcSemop};

    fn semget_msg(source: i32, key: i32, count: i32, flag: i32) -> Message {
        let mut msg = Message {
            m_source: Endpoint(source),
            m_type: minix_types::IPC_SEMGET,
            ..Message::default()
        };
        msg.m_u.m_lc_ipc_semget = minix_types::MessLcIpcSemget {
            key,
            nr: count,
            flag,
            ..Default::default()
        };
        msg
    }

    fn semop_msg(source: i32, id: i32) -> Message {
        let mut msg = Message {
            m_source: Endpoint(source),
            m_type: minix_types::IPC_SEMOP,
            ..Message::default()
        };
        msg.m_u.m_lc_ipc_semop = MessLcIpcSemop {
            id,
            size: 1,
            ops: 0x2000,
            ..Default::default()
        };
        msg
    }

    fn semctl_msg(source: i32, id: i32, num: i32, cmd: i32, option: u32) -> Message {
        let mut msg = Message {
            m_source: Endpoint(source),
            m_type: minix_types::IPC_SEMCTL,
            ..Message::default()
        };
        msg.m_u.m_lc_ipc_semctl = MessLcIpcSemctl {
            id,
            num,
            cmd,
            opt: option,
            ..Default::default()
        };
        msg
    }

    fn shmget_msg(source: i32, key: i32, size: u64) -> Message {
        let mut msg = Message {
            m_source: Endpoint(source),
            m_type: minix_types::IPC_SHMGET,
            ..Message::default()
        };
        msg.m_u.m_lc_ipc_shmget = minix_types::MessLcIpcShmget {
            key,
            size: size as u32,
            flag: 0o1000 | 0o600,
            ..Default::default()
        };
        msg
    }

    #[test]
    fn semget_writes_back_and_subscribes_on_first() {
        let mut service = IpcService::new(TestBoundary::new().with_creds(42, 100));
        let mut msg = semget_msg(42, 1, 2, 0o1000 | 0o600);
        assert_eq!(service.handle_call(IpcCall::Semget, &mut msg), OK);
        // C: m->m_lc_ipc_semget.retid = IXSEQ_TO_IPCID(...) — sem.c:154.
        assert_eq!(unsafe { msg.m_u.m_lc_ipc_semget.retid } & 0xffff, 0);
        assert_eq!(
            service.boundary.masks(),
            alloc::vec![true],
            "first set subscribes"
        );
        // A second set is silent; removing both unsubscribes.
        let mut msg2 = semget_msg(42, 2, 1, 0o1000 | 0o600);
        assert_eq!(service.handle_call(IpcCall::Semget, &mut msg2), OK);
        let id1 = unsafe { msg.m_u.m_lc_ipc_semget.retid };
        let mut rmid = semctl_msg(
            42,
            id1,
            0,
            minix_types::IPC_RMID,
            0,
        );
        assert_eq!(service.handle_call(IpcCall::Semctl, &mut rmid), OK);
        assert_eq!(service.boundary.masks().len(), 1, "no edge mid-life");
        let id2 = unsafe { msg2.m_u.m_lc_ipc_semget.retid };
        let mut rmid2 = semctl_msg(42, id2, 0, minix_types::IPC_RMID, 0);
        assert_eq!(service.handle_call(IpcCall::Semctl, &mut rmid2), OK);
        assert_eq!(
            service.boundary.masks(),
            alloc::vec![true, false],
            "last removal unsubscribes"
        );
    }

    #[test]
    fn semop_suspends_setall_wakes_then_eintr() {
        let boundary = TestBoundary::new().with_creds(42, 100).with_creds(43, 200);
        let mut service = IpcService::new(boundary);
        // 0666: both callers (uid 100 and uid 200) pass the permission
        // gate, so the scenario exercises suspension, not access denial.
        let mut msg = semget_msg(42, 1, 1, 0o1000 | 0o666);
        assert_eq!(service.handle_call(IpcCall::Semget, &mut msg), OK);
        let id = unsafe { msg.m_u.m_lc_ipc_semget.retid };

        // 43 suspends on -1 against value 0.
        service.boundary.set_ops(&[SemOp {
            num: 0,
            op: -1,
            flag: 0,
        }]);
        let mut opmsg = semop_msg(43, id);
        assert_eq!(service.handle_call(IpcCall::Semop, &mut opmsg), SUSPEND);

        // 42 sets the value to 1: the suspended op completes with success.
        service.boundary.set_setall(&[1]);
        let mut ctlmsg = semctl_msg(42, id, 0, minix_types::SETALL, 0x3000);
        assert_eq!(service.handle_call(IpcCall::Semctl, &mut ctlmsg), OK);
        assert_eq!(
            service.boundary.wakeups(),
            alloc::vec![(Endpoint(43), 0)],
            "the suspended waiter completes with success"
        );

        // A fresh suspend on -5, cancelled by a signal, wakes with EINTR.
        service.boundary.set_ops(&[SemOp {
            num: 0,
            op: -5,
            flag: 0,
        }]);
        let mut opmsg2 = semop_msg(43, id);
        assert_eq!(service.handle_call(IpcCall::Semop, &mut opmsg2), SUSPEND);
        service.handle_proc_event(minix_types::ProcEventIn {
            endpoint: Endpoint(43),
            exited: false,
        });
        assert_eq!(
            service.boundary.wakeups()[1..],
            alloc::vec![(Endpoint(43), EINTR)][..],
            "signal cancellation wakes with EINTR"
        );
        // An exit event for a stranger touches nothing.
        service.handle_proc_event(minix_types::ProcEventIn {
            endpoint: Endpoint(99),
            exited: true,
        });
        assert_eq!(service.boundary.wakeups().len(), 2);
    }

    #[test]
    fn semctl_ipc_set_applies_owner_draft() {
        let boundary = TestBoundary::new().with_creds(42, 100);
        let mut service = IpcService::new(boundary);
        let mut msg = semget_msg(42, 1, 1, 0o1000 | 0o600);
        assert_eq!(service.handle_call(IpcCall::Semget, &mut msg), OK);
        let id = unsafe { msg.m_u.m_lc_ipc_semget.retid };

        service.boundary.set_options(SetOptions {
            uid: 300,
            gid: 600,
            mode: 0o440,
        });
        let mut set = semctl_msg(42, id, 0, minix_types::IPC_SET, 0x4000);
        assert_eq!(service.handle_call(IpcCall::Semctl, &mut set), OK);
        let set_data = service.sems.get(0).unwrap();
        assert_eq!((set_data.perm.uid, set_data.perm.gid), (300, 600));
        assert_eq!(set_data.perm.creator_uid, 100, "cuid untouched");
        assert_eq!(set_data.perm.mode, minix_types::SEM_ALLOC | 0o440);

        // A stranger cannot read the set afterwards (mode is now 0440).
        let mut getval = semctl_msg(42, id, 0, minix_types::GETVAL, 0);
        let boundary2 = TestBoundary::new().with_creds(42, 100);
        let _ = boundary2; // (the caller identity is already in the service)
        assert_eq!(service.handle_call(IpcCall::Semctl, &mut getval), OK);
        assert_eq!(unsafe { getval.m_u.m_lc_ipc_semctl.ret }, 0);
    }

    #[test]
    fn semop_entry_order_is_c_faithful() {
        // No credentials for the caller at all: the permission gate is the
        // first thing to fail after lookup/count (IPC-P1-2 at the seam).
        let mut service = IpcService::new(TestBoundary::new().with_creds(42, 100));
        let mut msg = semget_msg(42, 1, 1, 0o1000 | 0o600);
        assert_eq!(service.handle_call(IpcCall::Semget, &mut msg), OK);
        let id = unsafe { msg.m_u.m_lc_ipc_semget.retid };
        service.boundary.set_ops(&[SemOp {
            num: 9,
            op: 1,
            flag: 0,
        }]);
        let mut opmsg = semop_msg(43, id); // endpoint 43 has no credentials
        assert_eq!(service.handle_call(IpcCall::Semop, &mut opmsg), EINVAL);
    }

    #[test]
    fn shm_rmid_cycle_destroys_via_cycle_end() {
        let boundary = TestBoundary::new().with_creds(42, 100);
        let mut service = IpcService::new(boundary);
        let mut msg = shmget_msg(42, 7, 5000);
        assert_eq!(service.handle_call(IpcCall::Shmget, &mut msg), OK);
        let id = unsafe { msg.m_u.m_lc_ipc_shmget.retid };
        let local = service.boundary.backings()[0].local;

        // RMID marks; the immediate sweep finds nattch 0 and frees — one
        // release lands at the boundary (C shm.c:334-336).
        let mut shmrmid = {
            let mut m = Message {
                m_source: Endpoint(42),
                m_type: minix_types::IPC_SHMCTL,
                ..Message::default()
            };
            m.m_u.m_lc_ipc_shmctl = minix_types::MessLcIpcShmctl {
                id,
                cmd: minix_types::IPC_RMID,
                ..Default::default()
            };
            m
        };
        assert_eq!(service.handle_call(IpcCall::Shmctl, &mut shmrmid), OK);
        assert_eq!(
            service.boundary.released(),
            alloc::vec![(local, 8192)],
            "unattached DEST segment dies in the immediate sweep"
        );
        assert!(service.shms.is_empty());
    }

    #[test]
    fn mib_visits_reach_the_boundary() {
        let mut service = IpcService::new(TestBoundary::new());
        let mut msg = Message::default();
        service.handle_mib(&mut msg);
        assert_eq!(service.boundary.mib_visits(), 1);
    }
}
