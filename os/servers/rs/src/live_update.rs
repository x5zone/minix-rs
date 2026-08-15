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
use minix_types::{EBUSY, EINVAL, Endpoint};

use crate::process_table::RupdateFlags;
use crate::service_slot::SlotId;
use crate::slot::RssFlags;

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
/// default is passed in by the caller, 19).
pub fn default_prepare_maxtime(maxtime: u32, default: u32) -> u32 {
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
) -> Result<(), i32> {
    if prepare_state == SEF_LU_STATE_NULL {
        return Err(EINVAL); // request.c:648-650
    }
    if matches!(phase, UpdatePhase::Updating | UpdatePhase::Initializing) {
        return Err(EBUSY); // request.c:659-663
    }
    if phase == UpdatePhase::Scheduled {
        if !batch {
            return Err(EBUSY); // request.c:666-668
        }
        if already_scheduled {
            return Err(EINVAL); // request.c:669-671
        }
    }
    if prepare_only
        && matches!(endpoint, Endpoint::VM | Endpoint::PM | Endpoint::VFS)
        && prepare_state != SEF_LU_STATE_UNREACHABLE
    {
        return Err(EINVAL); // request.c:674-681
    }
    if prepare_only && endpoint == Endpoint::RS {
        return Err(EINVAL); // request.c:683-686
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
            Err(EINVAL)
        );
        // Updating → EBUSY.
        assert_eq!(
            validate_update_request(UpdatePhase::Updating, false, false, false, Endpoint::PM, 4),
            Err(EBUSY)
        );
        // Scheduled without batch → EBUSY.
        assert_eq!(
            validate_update_request(UpdatePhase::Scheduled, false, false, false, Endpoint::PM, 4),
            Err(EBUSY)
        );
        // Batch but service already in chain → EINVAL.
        assert_eq!(
            validate_update_request(UpdatePhase::Scheduled, true, true, false, Endpoint::PM, 4),
            Err(EINVAL)
        );
        // Prepare-only of VM with reachable state → EINVAL.
        assert_eq!(
            validate_update_request(UpdatePhase::Idle, false, false, true, Endpoint::VM, 4),
            Err(EINVAL)
        );
        // Prepare-only of RS → EINVAL.
        assert_eq!(
            validate_update_request(UpdatePhase::Idle, false, false, true, Endpoint::RS, 5),
            Err(EINVAL)
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
        assert_eq!(default_prepare_maxtime(0, 100), 100);
        assert_eq!(default_prepare_maxtime(50, 100), 50);
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
