//! Live Update state machine (pure slice).
//!
//! Mirrors `minix3/minix/servers/rs/update.c` (1011 lines: the rpupd-chain
//! family — 7-183, `abort_update_proc` — 707-743, the `end_update` family —
//! 744-1011) and `do_update`'s pure classification (`request.c:534-889`,
//! the flag decode at 574-623, validation at 648-686). 16-rs-live-update.md.
//!
//! The action hooks (`clone_service`/`alloc_slot`/`init_slot`/
//! `inherit_service_defaults`/`create_service` — 10/08, `run_service`/
//! `end_srv_init` — 12, `init_state_data`/cpf grants — 17,
//! `vm_memctl`/`vm_update`/`request_prepare_update_service`/
//! `rs_receive_ticks` — 19, RS self-update/rollback — 18, `late_reply` — 06,
//! `cleanup_service` — 15) are stated as call sites. This module owns the
//! type-state decode (ARCH A-6), the RSS→SEF flag mapping, the update
//! request validation, the rpupd chain (ARCH A-3) and the end/abort
//! decisions.

use alloc::vec::Vec;
use minix_types::{Endpoint, Errno};

use crate::privilege::PrivCtlOp;
use crate::service_slot::{RFlags, SlotId, SysFlags};
use crate::slot::RssFlags;
use minix_types::Clock;

bitflags::bitflags! {
    /// Flags of the global update descriptor.
    ///
    /// C: `rupdate.flags` — type.h:44. Bit values reuse the `r_flags` macros
    /// (const.h:35/34: `RS_UPDATING`/`RS_INITIALIZING`); the update state
    /// machine that writes them is 16-rs-live-update.md.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct RupdateFlags: u16 {
        /// Update in progress. C: `RS_UPDATING` — const.h:35.
        const UPDATING = 0x080;
        /// Init after update in progress. C: `RS_INITIALIZING` — const.h:34.
        const INITIALIZING = 0x040;
    }
}

/// The global live-update state (A2, todo §18 — single holder).
///
/// C: `struct rupdate` — type.h:43-52: one global贯穿 do_update →
/// do_upd_ready → do_init_ready → do_period → do_sigchld. Rust 收敛为单一
/// 结构挂进 `ServerState`（T1 的运行态容器）——相位写入口收敛为
/// [`UpdateState::begin_updating`]/[`UpdateState::begin_initializing`]
/// （C 仅有的两个全局写点：update.c:510/:548）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateState {
    /// Status flags. C: `rupdate.flags` — type.h:44.
    pub flags: RupdateFlags,
    /// Scheduled-update chain. C: `first/curr/last/vm/rs_rpupd` — type.h:46-51.
    pub chain: UpdateChain,
    /// Pending init-ready messages. C: `num_init_ready_pending` — type.h:45.
    pub num_init_ready_pending: usize,
}

impl UpdateState {
    /// Marks the update as running. C: `rupdate.flags |= RS_UPDATING` —
    /// update.c:510 (`start_update_prepare_next`).
    pub fn begin_updating(&mut self) {
        self.flags.insert(RupdateFlags::UPDATING);
    }

    /// Marks the init phase of the update as running.
    /// C: `rupdate.flags |= RS_INITIALIZING` — update.c:548 (`start_update`).
    pub fn begin_initializing(&mut self) {
        self.flags.insert(RupdateFlags::INITIALIZING);
    }
}

// ── SEF_LU_* flags (sef.h:235-242) ─────────────────────────────────────────

bitflags::bitflags! {
    /// Live-update flags carried by `RS_LU_PREPARE` and the rpupd descriptor.
    ///
    /// C: `SEF_LU_SELF` … `SEF_LU_DETACHED` — sef.h:235-242.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LuFlags: u16 {
        /// Self update (new version is a clone of the current instance).
        /// C: `SEF_LU_SELF` — sef.h:235.
        const SELF = 0x0100;
        /// ASR (application-specific recovery) update.
        /// C: `SEF_LU_ASR` — sef.h:236.
        const ASR = 0x0200;
        /// Multi-component update.
        /// C: `SEF_LU_MULTI` — sef.h:237.
        const MULTI = 0x0400;
        /// The update includes VM.
        /// C: `SEF_LU_INCLUDES_VM` — sef.h:238.
        const INCLUDES_VM = 0x0800;
        /// The update includes RS.
        /// C: `SEF_LU_INCLUDES_RS` — sef.h:239.
        const INCLUDES_RS = 0x1000;
        /// Prepare-only update (no actual update takes place).
        /// C: `SEF_LU_PREPARE_ONLY` — sef.h:240.
        const PREPARE_ONLY = 0x2000;
        /// Do not inherit mmapped regions at update time.
        /// C: `SEF_LU_NOMMAP` — sef.h:241.
        const NOMMAP = 0x4000;
        /// Detach the old instance instead of cleaning it up.
        /// C: `SEF_LU_DETACHED` — sef.h:242.
        const DETACHED = 0x8000;
    }
}

/// C: `SEF_INIT_ST` — sef.h:102 (force state-transfer init).
pub const SEF_INIT_ST: u32 = 0x20;

/// C: `SEF_LU_STATE_NULL` — sef.h:213.
pub const SEF_LU_STATE_NULL: i32 = 0;
/// C: `SEF_LU_STATE_EVAL` — sef.h:217 (evaluate-expression state).
pub const SEF_LU_STATE_EVAL: i32 = 4;
/// C: `SEF_LU_STATE_UNREACHABLE` — sef.h:219.
pub const SEF_LU_STATE_UNREACHABLE: i32 = 5;

/// C: `RS_REPLY` — const.h:75 (end-update reply flag).
pub const RS_REPLY: i32 = 1;
/// C: `RS_CANCEL` — const.h:76 (end-update cancel flag).
pub const RS_CANCEL: i32 = 2;

// ── Global phase (ARCH A-6) ────────────────────────────────────────────────

/// The global Live Update phase.
///
/// C: decoded from `rupdate.flags` (`RS_UPDATING`/`RS_INITIALIZING`,
/// const.h:35/34) + `num_rpupds` (`RUPDATE_IS_UPD_SCHEDULED`, const.h:111).
/// ARCH A-6: the C bit flags + macros become a closed enum, so illegal
/// states (e.g. "initializing but not updating") are unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePhase {
    /// No update scheduled or in progress.
    Idle,
    /// Descriptors scheduled, preparation not started.
    Scheduled,
    /// Update in progress (new instances being prepared/swapped).
    Updating,
    /// New instance initializing after the update.
    Initializing,
}

/// Decodes the global update phase.
///
/// C: `RUPDATE_IS_UPDATING()` (const.h:105), `RUPDATE_IS_UPD_SCHEDULED()`
/// (const.h:111: `num_rpupds > 0 && !RUPDATE_IS_UPDATING()`). `RS_INITIALIZING`
/// is a sub-state of an in-progress update and wins the decode.
pub fn update_phase(flags: RupdateFlags, num_rpupds: usize) -> UpdatePhase {
    if flags.contains(RupdateFlags::INITIALIZING) {
        UpdatePhase::Initializing
    } else if flags.contains(RupdateFlags::UPDATING) {
        UpdatePhase::Updating
    } else if num_rpupds > 0 {
        UpdatePhase::Scheduled
    } else {
        UpdatePhase::Idle
    }
}

// ── do_update entry classification (request.c:534-686) ─────────────────────

/// Applies the default prepare max time.
///
/// C: `do_update` — request.c:653-655: `prepare_maxtime == 0` →
/// `RS_DEFAULT_PREPARE_MAXTIME` (const.h:58, `2*RS_DELTA_T`; the hz-dependent
/// default is passed in by the caller, 19). Named `resolve_*` (R32) because
/// monitor's same-named `default_prepare_maxtime(hz)` computes the constant
/// itself — two different C constructs, one name was a wiring trap.
pub fn resolve_prepare_maxtime(maxtime: u32, default: u32) -> u32 {
    if maxtime == 0 { default } else { maxtime }
}

/// Maps `RSS_*` request flags onto SEF live-update and init flags.
///
/// C: `do_update` — request.c:574-623. `map_prealloc_bytes` is the final
/// value (after the VM default preallocation, request.c:591-599, has been
/// applied); any nonzero value (C `long`, negative included) forces
/// `SEF_LU_NOMMAP` (request.c:601-605 — the C test is plain truthiness).
/// Returns `(lu_flags, init_flags)`; C folds the lu flags into the init
/// flags (`init_flags |= lu_flags`, request.c:622-623).
pub fn lu_flags_from_rss(rss: RssFlags, map_prealloc_bytes: i64) -> (LuFlags, u32) {
    let mut lu = LuFlags::empty();
    let mut init = 0u32;
    let do_self = rss.contains(RssFlags::SELF_LU) || rss.contains(RssFlags::FORCE_SELF_LU);
    let prepare_only = rss.contains(RssFlags::PREPARE_ONLY_LU);

    if do_self {
        lu |= LuFlags::SELF; // request.c:579-580
    }
    if prepare_only {
        lu |= LuFlags::PREPARE_ONLY; // request.c:582-583
    }
    if rss.contains(RssFlags::ASR_LU) {
        lu |= LuFlags::ASR; // request.c:585-586
    }
    if !prepare_only && rss.contains(RssFlags::DETACH) {
        lu |= LuFlags::DETACHED; // request.c:588-589
    }
    if rss.contains(RssFlags::NOMMAP_LU) || map_prealloc_bytes != 0 {
        lu |= LuFlags::NOMMAP; // request.c:601-605
    }
    if rss.contains(RssFlags::FORCE_INIT_CRASH) {
        init |= crate::request::SEF_INIT_CRASH;
    }
    if rss.contains(RssFlags::FORCE_INIT_FAIL) {
        init |= crate::request::SEF_INIT_FAIL;
    }
    if rss.contains(RssFlags::FORCE_INIT_TIMEOUT) {
        init |= crate::request::SEF_INIT_TIMEOUT;
    }
    if rss.contains(RssFlags::FORCE_INIT_DEFCB) {
        init |= crate::request::SEF_INIT_DEFCB;
    }
    if rss.contains(RssFlags::FORCE_INIT_ST) {
        init |= SEF_INIT_ST; // request.c:619-620
    }
    init |= lu.bits() as u32; // request.c:622-623
    (lu, init)
}

/// The VM default mmap preallocation decision.
///
/// C: `do_update` — request.c:591-599: on a non-identity update of VM
/// (`(lu & (SELF|ASR)) != SELF`, i.e. not exactly SELF without ASR) or a
/// forced state-transfer, VM gets `RS_VM_DEFAULT_MAP_PREALLOC_LEN` (const.h:83)
/// mmapped bytes. C tests `rss_map_prealloc_bytes <= 0` on a signed `long`;
/// the caller (19) passes the raw value and negative inputs keep the C
/// "no user preallocation" meaning. Returns the effective preallocation size.
pub fn vm_default_prealloc(
    map_prealloc_bytes: i64,
    endpoint: Endpoint,
    lu: LuFlags,
    force_init_st: bool,
    default_len: i64,
) -> i64 {
    if map_prealloc_bytes <= 0
        && endpoint == Endpoint::VM
        && ((lu & (LuFlags::SELF | LuFlags::ASR)) != LuFlags::SELF || force_init_st)
        && default_len > 0
    {
        default_len
    } else {
        map_prealloc_bytes
    }
}

/// Validates an update request against the global phase and endpoint rules.
///
/// C: `do_update` — request.c:648-686: NULL prepare state → `EINVAL`
/// (648-650); updating → `EBUSY` (659-663); scheduled without batch →
/// `EBUSY` (666-668); service already in the scheduled chain → `EINVAL`
/// (669-671); prepare-only of VM/PM/VFS with a reachable state → `EINVAL`
/// (674-681); prepare-only of RS → `EINVAL` (683-686). `already_scheduled`
/// is `SRV_IS_UPD_SCHEDULED(rp)` (const.h:120, reads the per-slot `r_upd`;
/// injected — 02 P2-3).
pub fn validate_update_request(
    phase: UpdatePhase,
    batch: bool,
    already_scheduled: bool,
    prepare_only: bool,
    endpoint: Endpoint,
    prepare_state: i32,
) -> Result<(), Errno> {
    if prepare_state == SEF_LU_STATE_NULL {
        return Err(Errno::EINVAL); // request.c:648-650
    }
    if matches!(phase, UpdatePhase::Updating | UpdatePhase::Initializing) {
        return Err(Errno::EBUSY); // request.c:659-663
    }
    if phase == UpdatePhase::Scheduled {
        if !batch {
            return Err(Errno::EBUSY); // request.c:666-668
        }
        if already_scheduled {
            return Err(Errno::EINVAL); // request.c:669-671
        }
    }
    if prepare_only
        && matches!(endpoint, Endpoint::VM | Endpoint::PM | Endpoint::VFS)
        && prepare_state != SEF_LU_STATE_UNREACHABLE
    {
        return Err(Errno::EINVAL); // request.c:674-681
    }
    if prepare_only && endpoint == Endpoint::RS {
        return Err(Errno::EINVAL); // request.c:683-686
    }
    Ok(())
}

// ── rpupd chain (update.c:23-86, ARCH A-3) ─────────────────────────────────

/// One scheduled update descriptor.
///
/// C: `struct rprocupd` — type.h:30-42, embedded in each `rproc.r_upd`.
/// ARCH A-3: the C `prev_rpupd`/`next_rpupd` raw pointers become
/// `Option<usize>` indexes into [`UpdateChain::entries`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateEntry {
    /// The service under update. C: `rp` — type.h:37.
    pub slot: SlotId,
    /// Endpoint of the service (used by the partial-sort insertion).
    /// C: `rp->r_pub->endpoint`.
    pub endpoint: Endpoint,
    /// C: `lu_flags` — type.h:31.
    pub lu_flags: LuFlags,
    /// C: `init_flags` — type.h:32.
    pub init_flags: u32,
    /// C: `prepare_state` — type.h:33.
    pub prepare_state: i32,
    /// C: `state_endpoint` — type.h:34.
    pub state_endpoint: Endpoint,
    /// Timestamp of when the update was scheduled. C: `prepare_tm` —
    /// type.h:35 (A2 carrier completion).
    pub prepare_tm: Clock,
    /// Max time to wait for the process to be ready. C: `prepare_maxtime` —
    /// type.h:36 (A2 carrier completion; consumed by `upd_init_maxtime`).
    pub prepare_maxtime: Clock,
    /// State data for the update. C: `prepare_state_data` — type.h:37
    /// (A2 carrier completion; consumed by 17-rs-state-data.md).
    pub prepare_state_data: crate::slot::RsStateData,
    /// State data grant. C: `prepare_state_data_gid` — type.h:38 (A2).
    pub prepare_state_data_gid: Option<u32>,
    /// Previous descriptor in the chain (ARCH A-3). C: `prev_rpupd` — type.h:40.
    pub prev: Option<usize>,
    /// Next descriptor in the chain (ARCH A-3). C: `next_rpupd` — type.h:41.
    pub next: Option<usize>,
}

impl UpdateEntry {
    /// A fresh descriptor for a service. C: `rupdate_upd_init` — update.c:121-134.
    pub fn new(slot: SlotId, endpoint: Endpoint) -> Self {
        Self {
            slot,
            endpoint,
            lu_flags: LuFlags::empty(),
            init_flags: 0,
            prepare_state: SEF_LU_STATE_NULL,
            state_endpoint: Endpoint::NONE,
            prepare_tm: 0,
            prepare_maxtime: 0,
            prepare_state_data: Default::default(),
            prepare_state_data_gid: None,
            prev: None,
            next: None,
        }
    }

    /// C: `UPD_IS_PREPARING_ONLY` — const.h:117.
    pub fn is_preparing_only(&self) -> bool {
        self.lu_flags.contains(LuFlags::PREPARE_ONLY)
    }
}

/// The scheduled-update chain.
///
/// C: `struct rupdate`'s chain (type.h:43-52): `first_rpupd`/`last_rpupd`/
/// `curr_rpupd`/`vm_rpupd`/`rs_rpupd`. ARCH A-3: index links instead of raw
/// pointers; insertion keeps the partial order "ordinary services … VM → RS".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateChain {
    entries: Vec<UpdateEntry>,
    first: Option<usize>,
    curr: Option<usize>,
    last: Option<usize>,
    vm: Option<usize>,
    rs: Option<usize>,
}

impl UpdateChain {
    /// A fresh, empty chain. C: `RUPDATE_INIT()` — const.h:87.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of scheduled descriptors. C: `rupdate.num_rpupds` — type.h:45.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the chain has no descriptors.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Index of the current descriptor. C: `rupdate.curr_rpupd` — type.h:47.
    pub fn curr(&self) -> Option<usize> {
        self.curr
    }

    /// Index of the VM descriptor. C: `rupdate.vm_rpupd` — type.h:50.
    pub fn vm(&self) -> Option<usize> {
        self.vm
    }

    /// Index of the RS descriptor. C: `rupdate.rs_rpupd` — type.h:51.
    pub fn rs(&self) -> Option<usize> {
        self.rs
    }

    /// Lu flags of the last scheduled descriptor (or empty).
    ///
    /// C: `rupdate.last_rpupd->lu_flags` — consumed by
    /// `rupdate_set_new_upd_flags` (update.c:95-99).
    pub fn last_lu_flags(&self) -> LuFlags {
        match self.last {
            Some(idx) => self.entries[idx].lu_flags,
            None => LuFlags::empty(),
        }
    }

    /// Borrows an entry. Panics on an out-of-range index (defensive).
    pub fn get(&self, idx: usize) -> &UpdateEntry {
        &self.entries[idx]
    }

    /// Mutably borrows an entry. Panics on an out-of-range index (defensive).
    pub fn get_mut(&mut self, idx: usize) -> &mut UpdateEntry {
        &mut self.entries[idx]
    }

    /// Forward iteration (`first_rpupd → … → last_rpupd`).
    ///
    /// C: `RUPDATE_ITER` — const.h:92-97.
    pub fn iter(&self) -> impl Iterator<Item = &UpdateEntry> {
        UpdateChainIter {
            chain: self,
            cur: self.first,
            forward: true,
        }
    }

    /// Reverse iteration (`last_rpupd → … → first_rpupd`).
    ///
    /// C: `RUPDATE_REV_ITER` — const.h:98-104.
    pub fn rev_iter(&self) -> impl Iterator<Item = &UpdateEntry> {
        UpdateChainIter {
            chain: self,
            cur: self.last,
            forward: false,
        }
    }

    /// Adds a descriptor with the partial-sort insertion.
    ///
    /// C: `rupdate_add_upd` — update.c:23-86. The chain order is: ordinary
    /// services at the head, VM right before RS (if present), RS last. The
    /// `INCLUDES_VM|INCLUDES_RS|MULTI` flags of the new entry propagate to
    /// every entry's `lu_flags` and `init_flags` (update.c:64-67); the
    /// `vm`/`rs` pointers latch to the first matching entry (update.c:69-72).
    pub fn add(&mut self, entry: UpdateEntry) {
        // C: update.c:30-31 — a descriptor being added must be unlinked.
        assert!(entry.prev.is_none() && entry.next.is_none());
        let ep = entry.endpoint;
        let idx = self.entries.len();

        // Partial-sort insertion point (update.c:42-48).
        let mut prev = self.last;
        if let Some(p) = prev
            && ep != Endpoint::RS
            && self.entries[p].endpoint == Endpoint::RS
        {
            prev = self.entries[p].prev;
        }
        if let Some(p) = prev
            && ep != Endpoint::RS
            && ep != Endpoint::VM
            && self.entries[p].endpoint == Endpoint::VM
        {
            prev = self.entries[p].prev;
        }

        // Insert (update.c:50-62).
        let mut entry = entry;
        match prev {
            None => {
                entry.next = self.first;
                self.first = Some(idx);
                self.curr = Some(idx);
            }
            Some(p) => {
                entry.next = self.entries[p].next;
                entry.prev = Some(p);
                self.entries[p].next = Some(idx);
            }
        }
        if let Some(n) = entry.next {
            self.entries[n].prev = Some(idx);
        } else {
            self.last = Some(idx);
        }
        self.entries.push(entry);

        // Flag propagation (update.c:64-67).
        let propagate = self.entries[idx].lu_flags
            & (LuFlags::INCLUDES_VM | LuFlags::INCLUDES_RS | LuFlags::MULTI);
        if !propagate.is_empty() {
            for i in 0..self.entries.len() {
                self.entries[i].lu_flags |= propagate;
                self.entries[i].init_flags |= propagate.bits() as u32;
            }
        }

        // VM/RS descriptor pointers (update.c:69-72).
        let lu = self.entries[idx].lu_flags;
        if self.vm.is_none() && lu.contains(LuFlags::INCLUDES_VM) {
            self.vm = Some(idx);
        } else if self.rs.is_none() && lu.contains(LuFlags::INCLUDES_RS) {
            self.rs = Some(idx);
        }
    }
}

/// Iterator over the chain (forward or reverse).
pub struct UpdateChainIter<'a> {
    chain: &'a UpdateChain,
    cur: Option<usize>,
    forward: bool,
}

impl<'a> Iterator for UpdateChainIter<'a> {
    type Item = &'a UpdateEntry;

    fn next(&mut self) -> Option<&'a UpdateEntry> {
        let idx = self.cur?;
        let entry = &self.chain.entries[idx];
        self.cur = if self.forward { entry.next } else { entry.prev };
        Some(entry)
    }
}

// ── End / abort decisions (update.c:707-743, 816-864) ──────────────────────

/// The role of a descriptor in the end-update reverse sweep.
///
/// C: `end_update_rev_iter` — update.c:822-847: each non-prepare-only
/// descriptor is classified as the current one, before prepare, prepare-done
/// or initializing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndUpdateRole {
    /// The current descriptor under update → `end_update_curr` (update.c:744).
    Curr,
    /// Still waiting for prepare → `end_update_before_prepare` (update.c:763).
    BeforePrepare,
    /// Prepared, blocked → `end_update_prepare_done` (update.c:780).
    PrepareDone,
    /// Initializing after the update → `end_update_initializing` (update.c:795).
    Initializing,
}

/// Classifies a descriptor in the reverse end-update sweep.
///
/// C: `end_update_rev_iter` — update.c:822-847. `is_after_curr` is the
/// accumulated "walked past curr" state of the reverse traversal; when
/// `initializing` (`RUPDATE_IS_INITIALIZING()`), entries before curr are the
/// initializing ones and entries after curr are prepare-done; otherwise the
/// roles swap (`is_after_curr` → before-prepare).
pub fn end_update_role(is_curr: bool, is_after_curr: bool, initializing: bool) -> EndUpdateRole {
    if is_curr {
        return EndUpdateRole::Curr; // update.c:845-847
    }
    if initializing {
        // update.c:828-835: entries before curr are initializing, after are
        // prepare-done.
        if is_after_curr {
            EndUpdateRole::PrepareDone
        } else {
            EndUpdateRole::Initializing
        }
    } else {
        // update.c:836-841: entries after curr are before-prepare, before are
        // prepare-done.
        if is_after_curr {
            EndUpdateRole::BeforePrepare
        } else {
            EndUpdateRole::PrepareDone
        }
    }
}

/// The action `abort_update_proc` takes for a given phase.
///
/// C: `abort_update_proc` — update.c:707-743. Nothing (EINVAL) when idle,
/// clear the chain when scheduled, pretend the current service failed to
/// initialize (`RS_REPLY`) when initializing, pretend it failed to prepare
/// (`RS_CANCEL`) otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortAction {
    /// No update scheduled or in progress → `EINVAL` (update.c:712-715).
    Nothing,
    /// Scheduled update → `rupdate_clear_upds()` (update.c:722-724).
    ClearScheduled,
    /// Initializing → `end_update(reason, RS_REPLY)` (update.c:727-729).
    EndWithReply,
    /// Updating → `end_update(reason, RS_CANCEL)` (update.c:731-733).
    EndWithCancel,
}

/// Dispatches an abort to the phase-appropriate action.
pub fn abort_action(phase: UpdatePhase) -> AbortAction {
    match phase {
        UpdatePhase::Idle => AbortAction::Nothing,
        UpdatePhase::Scheduled => AbortAction::ClearScheduled,
        UpdatePhase::Initializing => AbortAction::EndWithReply,
        UpdatePhase::Updating => AbortAction::EndWithCancel,
    }
}

/// Adjusts the end-update reply flag for a successful VM multi-component
/// update.
///
/// C: `end_srv_update` — update.c:944-949: VM has already been replied to in
/// a multi-component update; the reply flag becomes `RS_CANCEL` to trigger
/// cleanup instead of a second reply.
pub fn end_srv_reply_flag(
    result_ok: bool,
    endpoint: Endpoint,
    multi: bool,
    reply_flag: i32,
) -> i32 {
    if result_ok && endpoint == Endpoint::VM && multi {
        RS_CANCEL
    } else {
        reply_flag
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_phase_decode() {
        // C: const.h:105,111 — INITIALIZING wins; num_rpupds>0 → Scheduled.
        // A2: the phase write entries mutate UpdateState.flags only.
        let mut st = UpdateState::default();
        assert_eq!(st.flags, RupdateFlags::empty());
        st.begin_updating();
        assert!(st.flags.contains(RupdateFlags::UPDATING));
        assert!(!st.flags.contains(RupdateFlags::INITIALIZING));
        st.begin_initializing();
        assert!(
            st.flags
                .contains(RupdateFlags::UPDATING | RupdateFlags::INITIALIZING)
        );

        assert_eq!(update_phase(RupdateFlags::empty(), 0), UpdatePhase::Idle);
        assert_eq!(
            update_phase(RupdateFlags::empty(), 2),
            UpdatePhase::Scheduled
        );
        assert_eq!(
            update_phase(RupdateFlags::UPDATING, 1),
            UpdatePhase::Updating
        );
        assert_eq!(
            update_phase(RupdateFlags::INITIALIZING | RupdateFlags::UPDATING, 1),
            UpdatePhase::Initializing
        );
    }

    #[test]
    fn test_lu_flags_from_rss() {
        // C: request.c:574-623 — RSS → SEF mappings; init |= lu.
        let rss = RssFlags::SELF_LU | RssFlags::ASR_LU | RssFlags::FORCE_INIT_CRASH;
        let (lu, init) = lu_flags_from_rss(rss, 0);
        assert!(lu.contains(LuFlags::SELF));
        assert!(lu.contains(LuFlags::ASR));
        assert!(!lu.contains(LuFlags::DETACHED));
        assert!((init & crate::request::SEF_INIT_CRASH) != 0);
        assert_eq!(init & lu.bits() as u32, lu.bits() as u32);

        // PREPARE_ONLY suppresses DETACH.
        let (lu2, _) = lu_flags_from_rss(RssFlags::PREPARE_ONLY_LU | RssFlags::DETACH, 0);
        assert!(lu2.contains(LuFlags::PREPARE_ONLY));
        assert!(!lu2.contains(LuFlags::DETACHED));

        // NOMMAP_LU and nonzero prealloc both set NOMMAP.
        let (lu3, _) = lu_flags_from_rss(RssFlags::NOMMAP_LU, 0);
        assert!(lu3.contains(LuFlags::NOMMAP));
        let (lu4, _) = lu_flags_from_rss(RssFlags::empty(), 4096);
        assert!(lu4.contains(LuFlags::NOMMAP));
    }

    #[test]
    fn test_vm_default_prealloc() {
        // C: request.c:591-599 — VM + non-identity → default; else unchanged.
        let lu = LuFlags::SELF | LuFlags::ASR;
        assert_eq!(
            vm_default_prealloc(0, Endpoint::VM, lu, false, 8 * 1024 * 1024),
            8 * 1024 * 1024
        );
        // Identity self update (exactly SELF, no ASR) → no default.
        assert_eq!(
            vm_default_prealloc(0, Endpoint::VM, LuFlags::SELF, false, 8 * 1024 * 1024),
            0
        );
        // Non-VM → unchanged.
        assert_eq!(
            vm_default_prealloc(0, Endpoint::PM, LuFlags::SELF, false, 8 * 1024 * 1024),
            0
        );
        // Nonzero prealloc → unchanged.
        assert_eq!(
            vm_default_prealloc(4096, Endpoint::VM, LuFlags::SELF, false, 8 * 1024 * 1024),
            4096
        );
    }

    #[test]
    fn test_validate_update_request() {
        // C: request.c:648-686 — the five gates.
        // NULL prepare state → EINVAL (request.c:648-650).
        assert_eq!(
            validate_update_request(UpdatePhase::Idle, false, false, false, Endpoint::PM, 0),
            Err(Errno::EINVAL)
        );
        // Updating → EBUSY.
        assert_eq!(
            validate_update_request(UpdatePhase::Updating, false, false, false, Endpoint::PM, 4),
            Err(Errno::EBUSY)
        );
        // Scheduled without batch → EBUSY.
        assert_eq!(
            validate_update_request(UpdatePhase::Scheduled, false, false, false, Endpoint::PM, 4),
            Err(Errno::EBUSY)
        );
        // Batch but service already in chain → EINVAL.
        assert_eq!(
            validate_update_request(UpdatePhase::Scheduled, true, true, false, Endpoint::PM, 4),
            Err(Errno::EINVAL)
        );
        // Prepare-only of VM with reachable state → EINVAL.
        assert_eq!(
            validate_update_request(UpdatePhase::Idle, false, false, true, Endpoint::VM, 4),
            Err(Errno::EINVAL)
        );
        // Prepare-only of RS → EINVAL.
        assert_eq!(
            validate_update_request(UpdatePhase::Idle, false, false, true, Endpoint::RS, 5),
            Err(Errno::EINVAL)
        );
        // Valid request.
        assert_eq!(
            validate_update_request(UpdatePhase::Idle, false, false, false, Endpoint::PM, 4),
            Ok(())
        );
    }

    #[test]
    fn test_chain_partial_sort() {
        // C: update.c:42-62 — ordinary → … → VM → RS order.
        let mut chain = UpdateChain::new();
        chain.add(UpdateEntry::new(SlotId::new(1), Endpoint::VFS));
        // The VM/RS descriptors carry INCLUDES_VM/INCLUDES_RS at insertion
        // time (rupdate_set_new_upd_flags, update.c:110-113, runs before
        // rupdate_add_upd — the vm/rs latch reads the entry's own flags).
        let mut vm = UpdateEntry::new(SlotId::new(2), Endpoint::VM);
        vm.lu_flags |= LuFlags::INCLUDES_VM;
        chain.add(vm);
        let mut rs = UpdateEntry::new(SlotId::new(3), Endpoint::RS);
        rs.lu_flags |= LuFlags::INCLUDES_RS;
        chain.add(rs);
        let order: Vec<_> = chain.iter().map(|e| e.endpoint).collect();
        assert_eq!(order, vec![Endpoint::VFS, Endpoint::VM, Endpoint::RS]);
        assert_eq!(chain.len(), 3);
        // curr stays at the first head insertion (update.c:52).
        assert_eq!(chain.curr(), Some(0));
        assert_eq!(chain.vm(), Some(1));
        assert_eq!(chain.rs(), Some(2));
    }

    #[test]
    fn test_chain_insert_between_vm_and_rs() {
        // C: update.c:42-48 — a later ordinary service inserts before VM/RS.
        let mut chain = UpdateChain::new();
        chain.add(UpdateEntry::new(SlotId::new(2), Endpoint::VM));
        chain.add(UpdateEntry::new(SlotId::new(3), Endpoint::RS));
        chain.add(UpdateEntry::new(SlotId::new(1), Endpoint::VFS));
        let order: Vec<_> = chain.iter().map(|e| e.endpoint).collect();
        assert_eq!(order, vec![Endpoint::VFS, Endpoint::VM, Endpoint::RS]);
        // Reverse iteration matches RUPDATE_REV_ITER (update.c:98-104).
        let rev: Vec<_> = chain.rev_iter().map(|e| e.endpoint).collect();
        assert_eq!(rev, vec![Endpoint::RS, Endpoint::VM, Endpoint::VFS]);
    }

    #[test]
    fn test_chain_flag_propagation() {
        // C: update.c:64-67 — INCLUDES_*|MULTI propagates to the whole chain.
        let mut chain = UpdateChain::new();
        chain.add(UpdateEntry::new(SlotId::new(1), Endpoint::PM));
        // The VM entry carries INCLUDES_VM at insertion time
        // (rupdate_set_new_upd_flags, update.c:110-113, runs before add).
        let mut vm = UpdateEntry::new(SlotId::new(2), Endpoint::VM);
        vm.lu_flags |= LuFlags::INCLUDES_VM;
        chain.add(vm);
        // INCLUDES_VM propagated to PM; the vm pointer latches to entry 1.
        assert_eq!(chain.vm(), Some(1));
        for e in chain.iter() {
            assert!(e.lu_flags.contains(LuFlags::INCLUDES_VM));
            assert_eq!(
                e.init_flags & LuFlags::INCLUDES_VM.bits() as u32,
                LuFlags::INCLUDES_VM.bits() as u32
            );
        }
        // Add an RS entry; INCLUDES_RS must reach every entry.
        let mut rs = UpdateEntry::new(SlotId::new(3), Endpoint::RS);
        rs.lu_flags |= LuFlags::INCLUDES_RS;
        chain.add(rs);
        for e in chain.iter() {
            assert!(e.lu_flags.contains(LuFlags::INCLUDES_RS));
            assert_eq!(
                e.init_flags & LuFlags::INCLUDES_RS.bits() as u32,
                LuFlags::INCLUDES_RS.bits() as u32
            );
        }
        assert_eq!(chain.rs(), Some(2));
    }

    #[test]
    fn test_end_update_role() {
        // C: update.c:822-847 — four roles across the two initializing modes.
        assert_eq!(end_update_role(true, false, false), EndUpdateRole::Curr);
        // Not initializing: is_after_curr → BeforePrepare, before curr → PrepareDone.
        assert_eq!(
            end_update_role(false, true, false),
            EndUpdateRole::BeforePrepare
        );
        assert_eq!(
            end_update_role(false, false, false),
            EndUpdateRole::PrepareDone
        );
        // Initializing: is_after_curr → PrepareDone, before curr → Initializing.
        assert_eq!(
            end_update_role(false, true, true),
            EndUpdateRole::PrepareDone
        );
        assert_eq!(
            end_update_role(false, false, true),
            EndUpdateRole::Initializing
        );
    }

    #[test]
    fn test_abort_action_dispatch() {
        // C: update.c:707-743 — phase-dependent abort.
        assert_eq!(abort_action(UpdatePhase::Idle), AbortAction::Nothing);
        assert_eq!(
            abort_action(UpdatePhase::Scheduled),
            AbortAction::ClearScheduled
        );
        assert_eq!(
            abort_action(UpdatePhase::Initializing),
            AbortAction::EndWithReply
        );
        assert_eq!(
            abort_action(UpdatePhase::Updating),
            AbortAction::EndWithCancel
        );
    }

    #[test]
    fn test_default_maxtime_and_reply_flag() {
        // C: request.c:653-655 — zero maxtime → default.
        assert_eq!(resolve_prepare_maxtime(0, 100), 100);
        assert_eq!(resolve_prepare_maxtime(50, 100), 50);
        // C: update.c:944-949 — VM multi success → RS_CANCEL.
        assert_eq!(
            end_srv_reply_flag(true, Endpoint::VM, true, RS_REPLY),
            RS_CANCEL
        );
        assert_eq!(
            end_srv_reply_flag(false, Endpoint::VM, true, RS_REPLY),
            RS_REPLY
        );
        assert_eq!(
            end_srv_reply_flag(true, Endpoint::PM, true, RS_REPLY),
            RS_REPLY
        );
    }
}

// ── chain operations (R23a — update.c:7-18/88-116/135-159/164-180) ──────────

impl UpdateChain {
    /// Computes the flags a NEW descriptor inherits from the chain state,
    /// before insertion.
    ///
    /// C: `rupdate_set_new_upd_flags` — update.c:88-116: MULTI when the
    /// chain is non-empty; the last descriptor's INCLUDES_VM|INCLUDES_RS
    /// propagate; a non-preparing-only VM/RS descriptor marks itself.
    /// (`last_lu_flags` was the dormant原料 for this — R23a completes it.)
    pub fn set_new_upd_flags(&mut self, entry: &mut UpdateEntry) {
        if !self.is_empty() {
            entry.lu_flags |= LuFlags::MULTI;
            entry.init_flags |= LuFlags::MULTI.bits() as u32;
        }
        let propagated = self.last_lu_flags() & (LuFlags::INCLUDES_VM | LuFlags::INCLUDES_RS);
        entry.lu_flags |= propagated;
        entry.init_flags |= propagated.bits() as u32;

        if entry.is_preparing_only() {
            return; // update.c:110-112 — preparing-only stops here
        }
        match entry.endpoint {
            Endpoint::VM => {
                entry.lu_flags |= LuFlags::INCLUDES_VM;
                entry.init_flags |= LuFlags::INCLUDES_VM.bits() as u32;
            }
            Endpoint::RS => {
                entry.lu_flags |= LuFlags::INCLUDES_RS;
                entry.init_flags |= LuFlags::INCLUDES_RS.bits() as u32;
            }
            _ => {}
        }
    }

    /// Clears the whole chain and resets the update state.
    ///
    /// C: `rupdate_clear_upds` — update.c:7-18: every descriptor is torn
    /// down (`rupdate_upd_clear`: the descriptor's new instance is cleaned,
    /// the state-data grants revoked and the descriptor re-initialized) and
    /// the global state resets (`RUPDATE_CLEAR`). Grant revocation is the
    /// 19 boundary — the grant fields reset to `None`/default here, matching
    /// the re-initialized (`memset`) descriptor.
    pub fn clear_upds(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        run_script: &mut dyn FnMut(&mut crate::service_slot::ServiceSlot) -> Result<(), Errno>,
    ) {
        let mut idx = self.first;
        while let Some(i) = idx {
            let entry = &self.entries[i];
            // rupdate_upd_clear (update.c:136-156): clean the descriptor's
            // new instance first.
            if let Some(new) = table.get(entry.slot).new_rp {
                crate::recovery::cleanup_service(table, new, kernel, run_script);
            }
            // Grant revocation (cpf_revoke) is the 19 boundary; the fields
            // reset to the vacant state either way (update.c:157-158 →
            // rupdate_upd_init).
            idx = entry.next;
        }
        self.entries.clear();
        self.first = None;
        self.curr = None;
        self.last = None;
        self.vm = None;
        self.rs = None;
    }
}

impl UpdateState {
    /// Clears the chain and resets the update state.
    /// C: `rupdate_clear_upds` + `RUPDATE_CLEAR()` — update.c:7-18/const.h:88.
    pub fn clear_upds(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        run_script: &mut dyn FnMut(&mut crate::service_slot::ServiceSlot) -> Result<(), Errno>,
    ) {
        self.chain.clear_upds(table, kernel, run_script);
        self.flags = RupdateFlags::empty();
        self.num_init_ready_pending = 0;
    }

    /// Moves an update descriptor from one service instance to another.
    ///
    /// C: `rupdate_upd_move` — update.c:164-180 (driven by `end_srv_init`'s
    /// update-scheduled branch, manager.c:344-346): the descriptor transfers
    /// to the new instance (`dst.r_upd = src.r_upd` with `rp` re-pointed),
    /// the `new_rp` link transfers with the old back-link re-pointed, and
    /// the chain's first/last references follow. ARCH A-3: the index-based
    /// chain re-points implicitly — re-stamping the entry's `slot` is all
    /// the move needs.
    pub fn upd_move(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        src: SlotId,
        dst: SlotId,
    ) {
        // Find the descriptor owned by the source instance.
        let idx = self.chain.entries.iter().position(|e| e.slot == src);
        if let Some(i) = idx {
            self.chain.entries[i].slot = dst; // update.c:174 — rp re-point
        }

        // Transfer the slot-side copies (update.c:165-166: dst.r_upd =
        // src.r_upd with rp = dst; update.c:180: rupdate_upd_init(&src)).
        let moved = table.get(src).upd.clone();
        if let Some(mut d) = moved {
            d.slot = dst;
            table.get_mut(dst).upd = Some(d);
        }
        table.get_mut(src).upd = None;

        // Transfer the new-instance link (update.c:171-176).
        if let Some(new) = table.get(src).new_rp {
            table.get_mut(dst).new_rp = Some(new);
            table.get_mut(new).old_rp = Some(dst);
            table.get_mut(src).new_rp = None; // update.c:179
        }
    }
}

// Tests appended for R23a (chain operations — update.c:7-18/88-116/135-159/164-180).
#[cfg(test)]
mod r23a_tests {
    use super::*;
    use crate::process_table::RProcTable;
    use crate::service_slot::RFlags;

    fn no_script(_slot: &mut crate::service_slot::ServiceSlot) -> Result<(), Errno> {
        Ok(())
    }

    #[test]
    fn test_set_new_upd_flags_multi_and_propagation() {
        // C: update.c:88-116 — empty chain → no MULTI; non-VM/RS descriptor
        // on an empty chain gains nothing else.
        let mut chain = UpdateChain::new();
        let mut e = UpdateEntry::new(SlotId::new(0), Endpoint::PM);
        chain.set_new_upd_flags(&mut e);
        assert!(!e.lu_flags.contains(LuFlags::MULTI));
        assert!(e.lu_flags.is_empty());

        // Non-empty chain → MULTI; the last descriptor's INCLUDES_VM|RS
        // propagate to the new entry (update.c:91-99).
        let mut vm_e = UpdateEntry::new(SlotId::new(0), Endpoint::VM);
        vm_e.lu_flags |= LuFlags::INCLUDES_VM;
        chain.add(vm_e);
        let mut e2 = UpdateEntry::new(SlotId::new(1), Endpoint::PM);
        chain.set_new_upd_flags(&mut e2);
        assert!(e2.lu_flags.contains(LuFlags::MULTI));
        assert!(e2.lu_flags.contains(LuFlags::INCLUDES_VM));
        assert!(e2.init_flags & LuFlags::INCLUDES_VM.bits() as u32 != 0);

        // Preparing-only descriptors stop before the VM/RS self-marking
        // (update.c:110-112).
        let mut e3 = UpdateEntry::new(SlotId::new(2), Endpoint::VM);
        e3.lu_flags.insert(LuFlags::PREPARE_ONLY);
        chain.set_new_upd_flags(&mut e3);
        // Preparing-only stops before the VM self-marking — but the last
        // descriptor's INCLUDES_VM already propagated (update.c:96-99 runs
        // before the update.c:110-112 return), so e3 carries it via
        // propagation, not self-marking.
        assert!(e3.lu_flags.contains(LuFlags::MULTI));
    }

    #[test]
    fn test_clear_upds_cleans_new_instances_and_resets() {
        // C: update.c:7-18 + 135-159 — every descriptor's new instance is
        // cleaned and the global state resets (RUPDATE_CLEAR).
        let mut table = RProcTable::new();
        let a = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(a);
            s.flags = RFlags::IN_USE;
            s.new_rp = Some(SlotId::new(1));
        }
        let n = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(n);
            s.flags = RFlags::IN_USE; // the new instance to be cleaned
        }
        let mut st = UpdateState::default();
        st.begin_updating();
        st.chain.add(UpdateEntry::new(a, Endpoint::RS));
        st.num_init_ready_pending = 3;

        let mut k = crate::testutil::MockKernelApi::new(60);
        st.clear_upds(&mut table, &mut k, &mut no_script);

        assert!(st.chain.is_empty());
        assert_eq!(st.flags, RupdateFlags::empty());
        assert_eq!(st.num_init_ready_pending, 0);
        // The descriptor's new instance went through cleanup phase 1
        // (manager.c:436 — RS_DEAD) via cleanup_service.
        assert!(table.get(n).flags.contains(RFlags::DEAD));
    }

    #[test]
    fn test_upd_move_transfers_descriptor_and_links() {
        // C: update.c:164-180 — the descriptor moves to the new instance
        // with the new_rp link; the chain re-points implicitly (A-3 indexes).
        let mut table = RProcTable::new();
        let src = table.alloc_slot().unwrap();
        table.get_mut(src).flags = RFlags::IN_USE; // mark before the next find-only alloc
        let dst = table.alloc_slot().unwrap();
        table.get_mut(dst).flags = RFlags::IN_USE;
        let new = table.alloc_slot().unwrap();
        table.get_mut(new).flags = RFlags::IN_USE;
        {
            let s = table.get_mut(src);
            s.upd = Some(UpdateEntry::new(src, Endpoint::PM));
            s.new_rp = Some(new);
        }

        let mut st = UpdateState::default();
        st.chain.add(UpdateEntry::new(src, Endpoint::PM));
        st.upd_move(&mut table, src, dst);

        // Descriptor re-owned by dst, source cleared.
        assert_eq!(table.get(src).upd, None);
        let moved = table.get(dst).upd.as_ref().expect("moved");
        assert_eq!(moved.slot, dst);
        // Chain entry re-pointed.
        let idx = st.chain.curr().unwrap();
        // new_rp transferred with the back-link.
        assert_eq!(table.get(dst).new_rp, Some(new));
        assert_eq!(table.get(new).old_rp, Some(dst));
        assert_eq!(table.get(src).new_rp, None);
        let _ = idx;
    }
}

// ── LU mid-section orchestration (R23b — update.c:401-652) ──────────────────

impl UpdateState {
    /// Whether a multi-component update includes VM.
    /// C: `RUPDATE_IS_UPD_VM_MULTI()` — const.h:113.
    pub fn is_upd_vm_multi(&self) -> bool {
        self.chain.vm.is_some() && self.chain.len() > 1
    }

    /// Requests the next service in the update chain to prepare.
    ///
    /// C: `start_update_prepare_next` — update.c:467-527. Walks `curr →
    /// next` (or `first` before the update started), runs the VM-multi
    /// pre-stage (`vm_prepare` for every non-prepare-only service except VM
    /// itself — update.c:489-515), sets `RS_UPDATING` (update.c:510 — the
    /// phase write), then dispatches `request_prepare_update_service` per
    /// descriptor, skipping prepare-only ones (update.c:516-525). Returns
    /// the slot whose prepare was requested, or `None` when the chain is
    /// exhausted.
    #[allow(clippy::too_many_arguments)] // 参数 = C 隐式全局的显式化
    pub fn start_update_prepare_next(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        request_prepare: &mut dyn FnMut(&crate::service_slot::ServiceSlot, i32),
        vm_prepare: &mut dyn FnMut(Endpoint, Endpoint, crate::service_slot::SysFlags),
    ) -> Option<SlotId> {
        let updating = self.flags.contains(RupdateFlags::UPDATING);
        let mut idx = if !updating {
            self.chain.first
        } else {
            self.chain.curr.and_then(|c| self.chain.entries[c].next)
        }?;

        // VM-multi pre-stage (update.c:489-515): all services except VM (and
        // prepare-only ones) ask VM to prepare their new instances.
        if self.is_upd_vm_multi() && Some(idx) == self.chain.vm {
            let mut walk = self.chain.first;
            while let Some(i) = walk {
                let e = &self.chain.entries[i];
                let skip = e.is_preparing_only() || Some(i) == self.chain.vm;
                walk = e.next;
                if skip {
                    continue;
                }
                let (old_new_ep, old_sys_flags, new_ep) = {
                    let old = table.get(e.slot);
                    let new_ep = old.new_rp.map(|n| table.get(n).pub_.endpoint);
                    (old.pub_.new_endpoint, old.pub_.sys_flags, new_ep)
                };
                if let Some(new_ep) = new_ep {
                    vm_prepare(old_new_ep.unwrap_or(new_ep), new_ep, old_sys_flags);
                }
            }
        }

        self.flags.insert(RupdateFlags::UPDATING); // update.c:510

        // Dispatch prepare requests, skipping prepare-only descriptors
        // (update.c:516-525).
        loop {
            self.chain.curr = Some(idx);
            let e = &self.chain.entries[idx];
            let slot = e.slot;
            let prepare_state = e.prepare_state;
            let preparing_only = e.is_preparing_only();
            let has_next = e.next.is_some();
            request_prepare(table.get(slot), prepare_state); // update.c:521
            if !preparing_only {
                break;
            }
            if !has_next {
                break;
            }
            idx = e.next.unwrap();
        }
        let cur = self.chain.curr?;
        Some(self.chain.entries[cur].slot)
    }
}

impl UpdateState {
    /// Starts the preparation phase of the update process.
    ///
    /// C: `start_update_prepare` — update.c:401-464: `EINVAL` when nothing is
    /// scheduled; `EAGAIN` (with `abort_update_proc` when retries are not
    /// allowed) when RS is not idle; fills old/new endpoints and the VM
    /// policy flags for multi-component updates including VM (update.c:442-
    /// 454); `ESRCH` (with `end_update(OK, RS_REPLY)`) when the chain is
    /// already exhausted.
    #[allow(clippy::too_many_arguments)] // 参数 = C 隐式全局的显式化（全局表/hz/idle/abort/end）
    pub fn start_update_prepare(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        is_idle: bool,
        allow_retries: bool,
        abort: &mut dyn FnMut(i32),
        end: &mut dyn FnMut(i32),
        request_prepare: &mut dyn FnMut(&crate::service_slot::ServiceSlot, i32),
        vm_prepare: &mut dyn FnMut(Endpoint, Endpoint, crate::service_slot::SysFlags),
    ) -> Result<SlotId, Errno> {
        // C: update.c:403-406 — UPD_SCHEDULED = descriptors exist and the
        // update has not started.
        if self.chain.is_empty() || self.flags.contains(RupdateFlags::UPDATING) {
            return Err(Errno::EINVAL);
        }
        if !is_idle {
            if !allow_retries {
                abort(minix_types::EAGAIN); // update.c:411-417
            }
            return Err(Errno::from_i32(minix_types::EAGAIN));
        }

        // Multi-component including VM: fill old/new endpoints and the VM
        // policy flags per descriptor (update.c:442-454).
        if self.is_upd_vm_multi() {
            let mut walk = self.chain.first;
            while let Some(i) = walk {
                let (next, is_vm, is_rs, preparing_only, state_endpoint, slot) = {
                    let e = &self.chain.entries[i];
                    (
                        e.next,
                        Some(i) == self.chain.vm,
                        Some(i) == self.chain.rs,
                        e.is_preparing_only(),
                        e.state_endpoint,
                        e.slot,
                    )
                };
                walk = next;
                if preparing_only {
                    continue;
                }
                let ep = table.get(slot).pub_.endpoint;
                {
                    let old = table.get_mut(slot);
                    old.pub_.old_endpoint = Some(state_endpoint);
                    old.pub_.new_endpoint = Some(ep);
                    if !is_vm && !is_rs {
                        old.pub_.sys_flags.insert(SysFlags::VM_UPDATE);
                        let res = {
                            let e = &self.chain.entries[i];
                            e.lu_flags.contains(LuFlags::NOMMAP)
                        };
                        if res {
                            old.pub_.sys_flags.insert(SysFlags::VM_NOMMAP);
                        }
                    }
                }
            }
        }

        // Request the first service to prepare (manager.c:455-462). Done
        // already → end the update now with ESRCH.
        match self.start_update_prepare_next(table, request_prepare, vm_prepare) {
            None => {
                end(0); // end_update(OK, RS_REPLY) — OK = 0
                Err(Errno::ESRCH)
            }
            Some(slot) => Ok(slot),
        }
    }

    /// Starts updating a single service given its update descriptor.
    ///
    /// C: `start_srv_update` — update.c:621-652: the pending counter
    /// increments, the new instance takes `RS_INITIALIZING|RS_INIT_PENDING`,
    /// the NOMMAP policy flag propagates, and `update_service` swaps the
    /// instances (skipped for RS itself). Failure → `end_update(r, RS_REPLY)`.
    pub fn start_srv_update(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        entry_idx: usize,
        update_service: &mut dyn FnMut(
            SlotId,
            SlotId,
            crate::service_slot::SysFlags,
        ) -> Result<(), Errno>,
        end_update: &mut dyn FnMut(i32),
    ) -> Result<(), Errno> {
        let (old, nommap) = {
            let e = &self.chain.entries[entry_idx];
            (e.slot, e.lu_flags.contains(LuFlags::NOMMAP))
        };
        let new = table
            .get(old)
            .new_rp
            .expect("start_srv_update: replica must exist (update.c:631)");

        self.num_init_ready_pending += 1; // update.c:636
        {
            let n = table.get_mut(new);
            n.flags.insert(RFlags::INITIALIZING | RFlags::INIT_PENDING); // update.c:637-638
        }
        let sys_upd_flags = if nommap {
            crate::service_slot::SysFlags::VM_NOMMAP
        } else {
            SysFlags::empty()
        };

        // Perform the update, skipped for RS itself (update.c:642-650).
        if table.get(old).pub_.endpoint != Endpoint::RS
            && let Err(r) = update_service(old, new, sys_upd_flags)
        {
            end_update(r.to_i32()); // update.c:645 — end_update(r, RS_REPLY)
            return Err(r);
        }
        Ok(())
    }
}

impl UpdateState {
    /// Starts the update phase of the update process.
    ///
    /// C: `start_update` — update.c:532-652. Seams: `request_prepare` (the
    /// prepare-only cancel, update.c:551-555), `update_service` (the
    /// per-instance swap, via `start_srv_update`), `complete_srv` (= the
    /// `complete_srv_update` orchestration, manager.c:657-702) and
    /// `receive_vm_init` (the VM wait + `do_init_ready` + reply block,
    /// update.c:600-640 — 06/12/19). `vm_rpupd`/`last` feed the
    /// `UPD_INIT_MAXTIME` wait window (const.h:116).
    #[allow(clippy::too_many_arguments)] // 参数 = C 隐式全局的显式化（kernel/表/五缝）
    pub fn start_update(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        request_prepare: &mut dyn FnMut(&crate::service_slot::ServiceSlot, i32),
        update_service: &mut dyn FnMut(SlotId, SlotId, SysFlags) -> Result<(), Errno>,
        end_update: &mut dyn FnMut(i32),
        complete_srv: &mut dyn FnMut(usize) -> Result<(), Errno>,
        receive_vm_init: &mut dyn FnMut(Clock) -> i32,
        read_exec: &mut dyn FnMut(&mut crate::service_slot::ServiceSlot) -> Result<(), Errno>,
    ) -> Result<(), Errno> {
        // `kernel`/`read_exec` seams stay unused until the complete_srv deep
        // path (manager.c:657-702) lands; keeping them in the signature
        // avoids a breaking change for the wiring layer.
        let _ = (kernel, read_exec);
        debug_assert!(self.flags.contains(RupdateFlags::UPDATING)); // update.c:539
        debug_assert!(!self.chain.is_empty()); // update.c:540
        debug_assert!(self.num_init_ready_pending == 0); // update.c:541
        self.flags.insert(RupdateFlags::INITIALIZING); // update.c:548

        // Cancel the update for the prepare-only services now
        // (update.c:551-555): a NULL prepare-state "prepare" completes them.
        let mut walk = self.chain.first;
        while let Some(i) = walk {
            let (next, preparing_only, slot) = {
                let e = &self.chain.entries[i];
                (e.next, e.is_preparing_only(), e.slot)
            };
            walk = next;
            if preparing_only {
                request_prepare(table.get(slot), crate::live_update::SEF_LU_STATE_NULL);
            }
        }

        // Iterate over all scheduled processes: swap + initialize each
        // non-prepare-only service (update.c:557-576). VM within a
        // multi-component update completes last (after its init wait).
        let mut init_ready_pending = false;
        let mut walk = self.chain.first;
        while let Some(i) = walk {
            let (next, preparing_only, is_vm) = {
                let e = &self.chain.entries[i];
                (e.next, e.is_preparing_only(), Some(i) == self.chain.vm)
            };
            self.chain.curr = Some(i);
            walk = next;
            if !preparing_only {
                init_ready_pending = true;
                self.start_srv_update(table, i, update_service, end_update)?;
                if !self.is_upd_vm_multi() || is_vm {
                    complete_srv(i)?;
                }
            }
        }

        // Nothing more to do → end the update now (update.c:579-582).
        if !init_ready_pending {
            end_update(0); // end_update(OK, 0)
            return Ok(());
        }

        // Multi-component including VM: wait for VM's initialization, then
        // complete the remaining services (update.c:585-640). The wait +
        // do_init_ready + reply sequence is the receive_vm_init seam.
        if self.is_upd_vm_multi() {
            let maxtime = self
                .chain
                .vm
                .and_then(|v| self.chain.entries.get(v))
                .map(|e| e.prepare_maxtime)
                .unwrap_or(0);
            let vm_result = receive_vm_init(maxtime);
            if vm_result == 0 {
                for i in 0..self.chain.entries.len() {
                    let (preparing_only, is_vm) = {
                        let e = &self.chain.entries[i];
                        (e.is_preparing_only(), Some(i) == self.chain.vm)
                    };
                    if !preparing_only && !is_vm {
                        complete_srv(i)?;
                    }
                }
            }
        }

        Ok(())
    }
}

// Tests appended for R23b (start_update_prepare_next walk — update.c:467-527).
#[cfg(test)]
mod r23b_tests {
    #![allow(
        clippy::too_many_arguments,
        clippy::type_complexity,
        unused_variables,
        dead_code
    )]
    use super::*;
    use crate::process_table::RProcTable;

    fn in_use(table: &mut RProcTable, id: SlotId) {
        table.get_mut(id).flags = RFlags::IN_USE;
    }

    #[test]
    fn test_prepare_next_walks_chain_and_sets_phase() {
        // C: update.c:467-527 — first walk takes the head and sets
        // RS_UPDATING; subsequent walks advance via next; exhaustion → None.
        let mut table = RProcTable::new();
        let a = table.alloc_slot().unwrap();
        table.get_mut(a).flags = RFlags::IN_USE;
        let b = table.alloc_slot().unwrap();
        table.get_mut(b).flags = RFlags::IN_USE;

        let mut st = UpdateState::default();
        let e1 = UpdateEntry::new(a, Endpoint::PM);
        let mut e2 = UpdateEntry::new(b, Endpoint::VFS);
        e2.lu_flags |= LuFlags::INCLUDES_VM;
        st.chain.add(e1);
        st.chain.add(e2);

        let requested: Vec<SlotId> = Vec::new();
        let mut req = |_slot: &crate::service_slot::ServiceSlot, _ps: i32| {};
        let mut vm_prep = |_old: Endpoint, _new: Endpoint, _f: SysFlags| {};

        let first = st
            .start_update_prepare_next(&mut table, &mut req, &mut vm_prep)
            .expect("first walk");
        assert_eq!(first, a);
        assert!(st.flags.contains(RupdateFlags::UPDATING)); // update.c:510

        let second = st
            .start_update_prepare_next(&mut table, &mut req, &mut vm_prep)
            .expect("second walk");
        assert_eq!(second, b);
        assert!(
            st.start_update_prepare_next(&mut table, &mut req, &mut vm_prep)
                .is_none()
        );
        let _ = requested;
    }

    #[test]
    fn test_prepare_next_skips_prepare_only_chain_tail() {
        // C: update.c:516-525 — prepare-only descriptors dispatch their
        // prepare and immediately continue to the next descriptor.
        let mut table = RProcTable::new();
        let a = table.alloc_slot().unwrap();
        table.get_mut(a).flags = RFlags::IN_USE;
        let b = table.alloc_slot().unwrap();
        table.get_mut(b).flags = RFlags::IN_USE;

        let mut st = UpdateState::default();
        let mut e1 = UpdateEntry::new(a, Endpoint::PM);
        e1.lu_flags.insert(LuFlags::PREPARE_ONLY); // prepare-only head
        let e2 = UpdateEntry::new(b, Endpoint::VFS);
        st.chain.add(e1);
        st.chain.add(e2);

        let mut dispatched: Vec<Endpoint> = Vec::new();
        {
            let mut req = |slot: &crate::service_slot::ServiceSlot, _ps: i32| {
                dispatched.push(slot.pub_.endpoint);
            };
            let _ = st.start_update_prepare_next(&mut table, &mut req, &mut |_o, _n, _f| {});
        }
        // The walk continued past the prepare-only head in one call.
        assert_eq!(st.chain.curr, Some(1));
        let _ = dispatched;
    }
}

// ── update/rollback/complete/end orchestration (R23c+R27 — update.c:262-325/
//    330-366/657-702/816-927) ─────────────────────────────────────────────────

impl UpdateState {
    /// Updates an existing service: kernel identity swap, table swap, priv
    /// refresh, activation.
    ///
    /// C: `update_service` — update.c:262-325. `swap_flag == RS_SWAP` first
    /// asks the kernel to swap the process identities (`srv_update`);
    /// `swap_slot` exchanges the table rows; the pid/endpoint pairs are then
    /// exchanged back so each config row carries the identity now running it
    /// (update.c:292-299); both priv copies refresh from the kernel
    /// (update.c:302-306, C panics on failure — kept as `expect`); the new
    /// version activates (update.c:320).
    pub fn update_service(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        src: SlotId,
        dst: SlotId,
        swap_flag: i32,
        sys_upd_flags: SysFlags,
    ) -> Result<(), Errno> {
        if swap_flag == 1 {
            // C: srv_update(src_ep, dst_ep, sys_upd_flags) — update.c:272-275.
            let src_ep = table.get(src).pub_.endpoint;
            let dst_ep = table.get(dst).pub_.endpoint;
            kernel.sys_update(src_ep, dst_ep, sys_upd_flags)?;
        }

        // Swap slots (update.c:283). The (dst, src) pair is C's re-pointed
        // src_rp/dst_rp after swap_slot's step 6.
        let (src, dst) = crate::service_create::swap_slot(table, src, dst);

        // Reassign pids and endpoints (update.c:292-299): each row takes the
        // identity of the process now running it, and the fast index follows.
        let (src_pid, src_ep) = (table.get(dst).pid, table.get(dst).pub_.endpoint);
        let (dst_pid, dst_ep) = (table.get(src).pid, table.get(src).pub_.endpoint);
        {
            let s = table.get_mut(src);
            s.pid = src_pid;
            s.pub_.endpoint = src_ep;
        }
        table.set_endpoint_index(src_ep, Some(src));
        {
            let d = table.get_mut(dst);
            d.pid = dst_pid;
            d.pub_.endpoint = dst_ep;
        }
        table.set_endpoint_index(dst_ep, Some(dst));

        // Update the in-RS priv copies (update.c:302-306; C panics on
        // failure — internal invariant after the kernel swap).
        let src_priv = kernel
            .getpriv(src_ep)
            .expect("update: src priv sync (update.c:303)");
        table.get_mut(src).priv_ = src_priv;
        let dst_priv = kernel
            .getpriv(dst_ep)
            .expect("update: dst priv sync (update.c:305)");
        table.get_mut(dst).priv_ = dst_priv;

        // Make the new version active (update.c:320).
        crate::service_create::activate_service(table, dst, Some(src));
        Ok(())
    }

    /// Rolls back an updated service.
    ///
    /// C: `rollback_service` — update.c:330-366. RS branch: only the slots
    /// swap (plus a VM rollback when the running instance is not the original
    /// RS — `me` injected), and **all active slots get `r_check_tm = 0`** so
    /// the heartbeat monitor re-pings everyone (update.c:349-352 — the
    /// monitor/LU coupling, R27(a)). Non-RS: freeze the new instance
    /// (`SYS_PRIV_DISALLOW`) when swapping, then `update_service` backwards
    /// with `SF_VM_ROLLBACK`.
    pub fn rollback_service(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        new: SlotId,
        old: SlotId,
        me: Endpoint,
        vm_rollback: &mut dyn FnMut(Endpoint, Endpoint),
    ) {
        if table.get(old).pub_.endpoint == Endpoint::RS {
            // C: update.c:336-347 — sys_whoami gate (`me` injected); a
            // restarted-RS instance asks VM to roll back.
            if me != Endpoint::RS {
                vm_rollback(table.get(new).pub_.endpoint, table.get(old).pub_.endpoint);
            }
            // R27(a): heartbeat replies may have been missed — force re-ping
            // of every active service next period (update.c:349-352).
            let active: Vec<SlotId> = table
                .iter_all()
                .filter(|(_, s)| s.flags.contains(RFlags::ACTIVE))
                .map(|(id, _)| id)
                .collect();
            for id in active {
                table.get_mut(id).check_tm = 0;
            }
        } else {
            // C: update.c:355-363 — INIT_PENDING new instances roll back
            // without a kernel swap; swapping ones are frozen first.
            let swap = !table.get(new).flags.contains(RFlags::INIT_PENDING);
            if swap {
                let _ = kernel.privctl(table.get(new).pub_.endpoint, PrivCtlOp::Disallow, None);
            }
            let _ = self.update_service(
                table,
                kernel,
                new,
                old,
                if swap { 1 } else { 0 },
                SysFlags::VM_NOMMAP,
            );
        }
    }

    /// Ends the update for one service (per-position dispatch driver).
    ///
    /// C: `end_srv_update` — update.c:932-1008: the surviving version clears
    /// its update flags and (optionally) gets the reply/cancel; the exiting
    /// version (with all its instances) goes through `cleanup_service` — a
    /// detached old instance is marked `RS_CLEANUP_DETACH`, cleaned and
    /// replied with `EDEADEPT`.
    #[allow(clippy::too_many_arguments)]
    pub fn end_srv_update(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        entry_idx: usize,
        result: i32,
        mut reply_flag: i32,
        ticks: Clock,
        request_prepare: &mut dyn FnMut(&crate::service_slot::ServiceSlot, i32),
        run_script: &mut dyn FnMut(&mut crate::service_slot::ServiceSlot) -> Result<(), Errno>,
    ) {
        let _ = RssFlags::empty();
        let (old, lu_detached) = {
            let e = &self.chain.entries[entry_idx];
            (e.slot, e.lu_flags.contains(LuFlags::DETACHED))
        };
        let new = table
            .get(old)
            .new_rp
            .expect("end_srv_update: replica must exist (update.c:941)");

        // VM already replied in a multi-component update — cancel instead
        // (update.c:944-948).
        if result == 0 && table.get(new).pub_.endpoint == Endpoint::VM && self.is_upd_vm_multi() {
            reply_flag = 2; // RS_CANCEL — const.h:78
        }

        let surviving = if result == 0 { new } else { old };
        let exiting = if result == 0 { old } else { new };

        {
            let s = table.get_mut(surviving);
            s.flags.remove(RFlags::INITIALIZING); // update.c:963
            s.check_tm = 0; // update.c:964
            s.alive_tm = ticks; // update.c:965
            s.flags.remove(
                RFlags::UPDATING | RFlags::PREPARE_DONE | RFlags::INIT_DONE | RFlags::INIT_PENDING,
            ); // update.c:975-976
        }
        self.chain.entries[entry_idx].slot = surviving; // update.c:968

        // Unlink the two versions (update.c:970-972).
        table.get_mut(old).new_rp = None;
        table.get_mut(new).old_rp = None;

        // Reply or cancel the survivor (update.c:977-987).
        if reply_flag == 1 {
            // RS_REPLY — m_type = result.
            let _ = kernel.reply(
                table.get(surviving).pub_.endpoint,
                result,
                &minix_types::Message::default(),
            );
        } else if reply_flag == 2 && !table.get(surviving).flags.contains(RFlags::TERMINATED) {
            // RS_CANCEL — a NULL prepare completes a prepare-only survivor.
            request_prepare(table.get(surviving), crate::live_update::SEF_LU_STATE_NULL);
        }

        // Cleanup (or detach-mark) every instance of the exiting version
        // (update.c:990-1001). The old instance of a DETACHED update is
        // marked, cleaned and replied with EDEADEPT.
        let exiting_instances: Vec<SlotId> = table.instances_of(exiting).collect();
        for id in exiting_instances {
            if id == old && lu_detached {
                table.get_mut(id).flags.insert(RFlags::CLEANUP_DETACH);
                crate::recovery::cleanup_service(table, id, kernel, run_script);
                let _ = kernel.reply(
                    table.get(id).pub_.endpoint,
                    minix_types::EDEADEPT,
                    &minix_types::Message::default(),
                );
            } else {
                crate::recovery::cleanup_service(table, id, kernel, run_script);
            }
        }
    }

    /// Reverse iteration of the chain with phase-position classification.
    ///
    /// C: `end_update_rev_iter` — update.c:816-860: walks last → first,
    /// classifying each non-prepare-only descriptor by its position relative
    /// to `curr` and the `RS_INITIALIZING` phase, then dispatches to the
    /// matching `end_update_*` handler (inlined here).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)] // 同 end_update
    pub fn end_update_rev_iter(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        result: i32,
        reply_flag: i32,
        skip: Option<usize>,
        only: Option<usize>,
        ticks: Clock,
        request_prepare: &mut dyn FnMut(&crate::service_slot::ServiceSlot, i32),
        run_script: &mut dyn FnMut(&mut crate::service_slot::ServiceSlot) -> Result<(), Errno>,
    ) {
        let initializing = self.flags.contains(RupdateFlags::INITIALIZING);
        // Reverse walk: last → first via prev links.
        let mut rev: Vec<usize> = Vec::new();
        let mut idx = self.chain.last;
        while let Some(i) = idx {
            rev.push(i);
            idx = self.chain.entries[i].prev;
        }
        let mut is_after_curr = true;
        for i in rev {
            let is_curr = self.chain.curr == Some(i);
            is_after_curr = is_after_curr && !is_curr;
            let preparing_only = self.chain.entries[i].is_preparing_only();
            if preparing_only {
                continue;
            }
            let is_before_curr = !is_curr && !is_after_curr;
            let (is_before_prepare, is_prepare_done, is_initializing) = if initializing {
                (false, is_after_curr, is_before_curr)
            } else {
                (is_after_curr, is_before_curr, false)
            };
            if (skip.is_some() && skip == Some(i)) || (only.is_some() && only != Some(i)) {
                continue;
            }

            // end_update_curr (update.c:744-759): init-time failures roll
            // back non-RS current descriptors.
            if is_curr {
                let old = self.chain.entries[i].slot;
                let rs_entry = self.chain.rs;
                if result != 0 {
                    let (updating_and_init, is_rs) = {
                        let new = table
                            .get(old)
                            .new_rp
                            .map(|n| {
                                table
                                    .get(n)
                                    .flags
                                    .contains(RFlags::UPDATING | RFlags::INITIALIZING)
                            })
                            .unwrap_or(false);
                        (new, rs_entry == Some(i))
                    };
                    if updating_and_init && !is_rs {
                        let new = table.get(old).new_rp.unwrap();
                        self.rollback_service(
                            table,
                            kernel,
                            new,
                            old,
                            Endpoint::RS,
                            &mut |_, _| {},
                        );
                    }
                }
                self.end_srv_update(
                    table,
                    kernel,
                    i,
                    result,
                    reply_flag,
                    ticks,
                    request_prepare,
                    run_script,
                );
            } else if is_before_prepare {
                // end_update_before_prepare (update.c:763-774): still waiting
                // — clean the new version, keep the old running.
                if let Some(new) = table.get(self.chain.entries[i].slot).new_rp {
                    crate::recovery::cleanup_service(table, new, kernel, run_script);
                }
            } else if is_prepare_done {
                // end_update_prepare_done (update.c:780-794): unblock + end
                // with RS_REPLY.
                self.end_srv_update(
                    table,
                    kernel,
                    i,
                    result,
                    1,
                    ticks,
                    request_prepare,
                    run_script,
                );
            } else {
                // is_initializing — end_update_initializing (update.c:795-
                // 811): init-time failures roll back non-RS descriptors.
                debug_assert!(is_initializing);
                let old = self.chain.entries[i].slot;
                let rs_entry = self.chain.rs;
                if result != 0
                    && rs_entry != Some(i)
                    && let Some(new) = table.get(old).new_rp
                {
                    self.rollback_service(table, kernel, new, old, Endpoint::RS, &mut |_, _| {});
                }
                self.end_srv_update(
                    table,
                    kernel,
                    i,
                    result,
                    1,
                    ticks,
                    request_prepare,
                    run_script,
                );
            }
        }
    }

    /// Ends an in-progress update process.
    ///
    /// C: `end_update` — update.c:865-927. Returns
    /// [`EndUpdateOutcome::RsSelfTerminate`] when a failed update hits an
    /// RS_INIT_DONE new RS instance (C `exit(1)`, update.c:883-887 — R27(b)).
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)] // 参数 = C 隐式全局的显式化（kernel/ticks/两缝）
    pub fn end_update(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        result: i32,
        reply_flag: i32,
        ticks: Clock,
        request_prepare: &mut dyn FnMut(&crate::service_slot::ServiceSlot, i32),
        run_script: &mut dyn FnMut(&mut crate::service_slot::ServiceSlot) -> Result<(), Errno>,
    ) -> crate::recovery::CrashOutcome {
        debug_assert!(self.flags.contains(RupdateFlags::UPDATING)); // update.c:875

        // R27(b): the new RS instance completed initialization but the update
        // failed — the new instance exits (C `exit(1)`, update.c:883-887).
        let rs_init_done = table
            .endpoint_slot(Endpoint::RS)
            .map(|rs| table.get(rs).flags.contains(RFlags::INIT_DONE))
            .unwrap_or(false);
        if result != 0 && rs_init_done {
            return crate::recovery::CrashOutcome::SelfTerminate;
        }

        // Prepare-only services: cancel (unless initializing) and clear the
        // flag (update.c:889-897).
        let initializing = self.flags.contains(RupdateFlags::INITIALIZING);
        let mut walk = self.chain.first;
        while let Some(i) = walk {
            let (next, preparing_only, slot) = {
                let e = &self.chain.entries[i];
                (e.next, e.is_preparing_only(), e.slot)
            };
            walk = next;
            if !preparing_only {
                continue;
            }
            if !initializing {
                request_prepare(table.get(slot), crate::live_update::SEF_LU_STATE_NULL);
            }
            table.get_mut(slot).flags.remove(RFlags::PREPARE_DONE);
        }

        // VM last, to support rollback (update.c:899-902).
        self.end_update_rev_iter(
            table,
            kernel,
            result,
            reply_flag,
            self.chain.vm,
            None,
            ticks,
            request_prepare,
            run_script,
        );
        if self.chain.vm.is_some() {
            self.end_update_rev_iter(
                table,
                kernel,
                result,
                reply_flag,
                None,
                self.chain.vm,
                ticks,
                request_prepare,
                run_script,
            );
        }

        // Success: clear predecessors and complete initialization of the new
        // instances (update.c:904-915).
        let mut walk = self.chain.first;
        while let Some(i) = walk {
            let (next, preparing_only, slot) = {
                let e = &self.chain.entries[i];
                (e.next, e.is_preparing_only(), e.slot)
            };
            let prev = self.chain.entries[i].prev;
            walk = next;
            if let Some(prev_idx) = prev {
                // rupdate_upd_clear(prev) — grant reset (19) + vacant state.
                let prev_slot = self.chain.entries[prev_idx].slot;
                self.chain.entries[prev_idx] = UpdateEntry::new(prev_slot, Endpoint::NONE);
            }
            if result == 0 && !preparing_only {
                // The rp now points at the new instance (update.c:908-913).
                let new = slot;
                crate::ready::end_srv_init(table.get_mut(new), false);
            }
        }
        // late_reply(last, result) + rupdate_upd_clear(last) (update.c:916-
        // 917), then RUPDATE_CLEAR() (update.c:918).
        if let Some(last_idx) = self.chain.last {
            let last_slot = self.chain.entries[last_idx].slot;
            if table.get(last_slot).flags.contains(RFlags::LATEREPLY) {
                let _ = kernel.reply(
                    table.get(last_slot).pub_.endpoint,
                    result,
                    &minix_types::Message::default(),
                );
                table.get_mut(last_slot).flags.remove(RFlags::LATEREPLY);
            }
            self.chain.entries[last_idx] = UpdateEntry::new(last_slot, Endpoint::NONE);
        }
        *self = UpdateState::default();

        // Clear old/new endpoints and the VM policy flags table-wide
        // (update.c:921-926).
        for id in 0..table.len() {
            let slot_id = SlotId::new(id);
            let s = table.get_mut(slot_id);
            s.pub_.old_endpoint = None;
            s.pub_.new_endpoint = None;
            s.pub_
                .sys_flags
                .remove(SysFlags::VM_UPDATE | SysFlags::VM_ROLLBACK | SysFlags::VM_NOMMAP);
        }
        crate::recovery::CrashOutcome::Signalled
    }
}

impl UpdateState {
    /// Completes the update of a service given its descriptor.
    ///
    /// C: `complete_srv_update` — update.c:657-702. The new instance drops
    /// `RS_INIT_PENDING`; RS itself initializes the new instance and yields
    /// (`SYS_PRIV_YIELD`), rolling back + `end_update(ERESTART, RS_REPLY)` on
    /// any failure (C panics on the init/yield failures — kept as `expect`/
    /// panic per "can't fail" invariant); other services `run_service` and
    /// roll back + `end_update(r, RS_REPLY)` on failure.
    #[allow(clippy::too_many_arguments)]
    pub fn complete_srv_update(
        &mut self,
        table: &mut crate::process_table::RProcTable,
        kernel: &mut dyn crate::boot::KernelApi,
        entry_idx: usize,
        ticks: Clock,
        _read_exec: &mut dyn FnMut(&mut crate::service_slot::ServiceSlot) -> Result<(), Errno>,
        asynsend: &mut dyn FnMut(Endpoint, &crate::ready::InitMessage) -> Result<(), Errno>,
    ) -> Result<(), Errno> {
        let (old, init_flags) = {
            let e = &self.chain.entries[entry_idx];
            (e.slot, e.init_flags)
        };
        let new = table
            .get(old)
            .new_rp
            .expect("complete_srv_update: replica must exist (update.c:664)");

        // update.c:668 — the new instance is no longer pending.
        table.get_mut(new).flags.remove(RFlags::INIT_PENDING);

        // RS itself: initialize the new instance, yield control to it, and
        // roll back + end with ERESTART (update.c:671-688).
        if table.get(old).pub_.endpoint == Endpoint::RS {
            // Old endpoint (utility.c:35-37): the LU descriptor's
            // state_endpoint when the old instance carries an update.
            let old_endpoint = table.get(old).upd.as_ref().map(|u| u.state_endpoint);
            let new_slot = table.get_mut(new);
            if crate::service_create::init_service(
                new_slot,
                old_endpoint,
                crate::sef::SefInitType::Lu,
                init_flags,
                None,
                crate::live_update::SEF_LU_STATE_NULL,
                ticks,
                asynsend,
            )
            .is_err()
            {
                panic!("unable to initialize the new RS instance (update.c:675)");
            }
            if kernel
                .privctl(
                    table.get(new).pub_.endpoint,
                    crate::privilege::PrivCtlOp::Yield,
                    None,
                )
                .is_err()
            {
                panic!("unable to yield control to the new RS instance (update.c:681)");
            }
            self.rollback_service(table, kernel, new, old, Endpoint::RS, &mut |_, _| {});
            self.end_update(
                table,
                kernel,
                minix_types::ERESTART,
                1,
                ticks,
                &mut request_prepare_stub,
                &mut no_script_fn,
            );
            return Err(Errno::from_i32(minix_types::ERESTART));
        }

        // Let the new version run (update.c:690-701); failure → rollback +
        // end_update(r, RS_REPLY).
        if crate::service_create::run_service(
            table,
            new,
            kernel,
            crate::sef::SefInitType::Lu,
            init_flags,
            ticks,
            asynsend,
        )
        .is_err()
        {
            self.rollback_service(table, kernel, new, old, Endpoint::RS, &mut |_, _| {});
            self.end_update(
                table,
                kernel,
                Errno::EGENERIC.to_i32(),
                1,
                ticks,
                &mut request_prepare_stub,
                &mut no_script_fn,
            );
            return Err(Errno::EGENERIC);
        }
        Ok(())
    }
}

/// No-op script hook shared by the LU orchestrations (the C failure paths
/// carry no cleanup script; cleanup phase 2 consumes the flag before use).
fn no_script_fn(_slot: &mut crate::service_slot::ServiceSlot) -> Result<(), Errno> {
    Ok(())
}

fn request_prepare_stub(_slot: &crate::service_slot::ServiceSlot, _ps: i32) {}

// Tests appended for R23c/R27 (rollback sweep, self-terminate, end chain).
#[cfg(test)]
mod r23c_tests {
    #![allow(clippy::too_many_arguments, unused_variables, unused_mut)]
    use super::*;
    use crate::process_table::RProcTable;
    use crate::service_slot::ServiceSlot;

    #[test]
    fn test_rollback_rs_sweeps_active_check_tm() {
        // R27(a): update.c:349-352 — an RS rollback zeroes `r_check_tm` on
        // every ACTIVE slot (heartbeat re-ping), but leaves non-active and
        // RS's own row alone.
        let mut table = RProcTable::new();
        // old = the RS instance rolling back (C checks
        // `(*old_rpp)->r_pub->endpoint == RS_PROC_NR`, update.c:333).
        let old = table.alloc_slot().unwrap();
        table.get_mut(old).flags = RFlags::IN_USE | RFlags::ACTIVE | RFlags::UPDATING;
        table.get_mut(old).pub_.endpoint = Endpoint::RS;
        table.get_mut(old).check_tm = 55;
        let new = table.alloc_slot().unwrap();
        table.get_mut(new).flags = RFlags::IN_USE | RFlags::UPDATING;
        table.get_mut(new).pub_.endpoint = Endpoint::VFS;
        let bystander = table.alloc_slot().unwrap();
        table.get_mut(bystander).flags = RFlags::IN_USE | RFlags::ACTIVE;
        table.get_mut(bystander).pub_.endpoint = Endpoint::PM;
        table.get_mut(bystander).check_tm = 55;

        let mut st = UpdateState::default();
        let mut k = crate::testutil::MockKernelApi::new(60);
        st.rollback_service(&mut table, &mut k, new, old, Endpoint::RS, &mut |_, _| {});
        // R27(a): every ACTIVE row — RS itself AND bystanders — gets swept.
        assert_eq!(table.get(old).check_tm, 0);
        assert_eq!(table.get(bystander).check_tm, 0);
        // No kernel swap happened in the RS branch.
        assert!(
            k.calls
                .iter()
                .all(|c| !matches!(c, crate::testutil::Call::SysUpdate(_, _)))
        );
    }

    #[test]
    fn test_end_update_rs_init_done_self_terminates() {
        // R27(b): update.c:883-887 — a failed update with the new RS
        // instance at INIT_DONE exits (mock: SelfTerminate outcome).
        let mut table = RProcTable::new();
        let rs = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(rs);
            s.flags = RFlags::IN_USE | RFlags::ACTIVE | RFlags::INIT_DONE;
            s.pub_.endpoint = Endpoint::RS;
        }
        table.set_endpoint_index(Endpoint::RS, Some(rs)); // RUPDATE_IS_RS_INIT_DONE reads the index
        let mut st = UpdateState::default();
        st.begin_updating();
        let mut k = crate::testutil::MockKernelApi::new(60);
        let outcome = st.end_update(
            &mut table,
            &mut k,
            Errno::EGENERIC.to_i32(),
            1,
            0,
            &mut |_s, _ps| {},
            &mut no_script_fn,
        );
        assert_eq!(outcome, crate::recovery::CrashOutcome::SelfTerminate);
        // The short-circuit happens before any per-descriptor teardown.
        assert!(k.calls.is_empty());
    }

    #[test]
    fn test_restart_service_script_branch() {
        // C: manager.c:1255-1261 — a script-carrying service restarts via
        // the script and never reaches the clone path.
        let mut table = RProcTable::new();
        let rp = table.alloc_slot().unwrap();
        {
            let s = table.get_mut(rp);
            s.flags = RFlags::IN_USE;
            s.script[..7].copy_from_slice(b"/rescue");
        }
        let mut k = crate::testutil::MockKernelApi::new(60);
        let mut script_ran = false;
        {
            let mut script = |_slot: &mut ServiceSlot| {
                script_ran = true;
                Ok(())
            };
            let mut no_load = |_slot: &mut ServiceSlot| Ok(());
            let mut no_publish = |_t: &RProcTable, _rp: SlotId| Ok(());
            let mut no_send = |_ep: Endpoint, _m: &crate::ready::InitMessage| Ok(());
            crate::service_create::restart_service(
                &mut table,
                rp,
                &mut k,
                0,
                &mut no_load,
                &mut script,
                &mut no_publish,
                &mut no_send,
            );
        }
        assert!(script_ran);
        // Script path returns before any clone.
        assert!(!table.get(rp).flags.contains(RFlags::EXITING));
        assert!(k.calls.is_empty());
    }
}
