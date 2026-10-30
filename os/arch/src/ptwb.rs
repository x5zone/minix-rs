//! Page-table write "record - quiescent-point walk" sampler (riscv64).
//!
//! # Lineage (why it has this shape)
//!
//! The first shape tried on the evening of `2026-10-19` (an inline walk-back
//! hooking all four leaf writes: map/remap/unmap/update_flags re-read the
//! table immediately after writing) hung the riscv ATF boot leg as soon as it
//! shipped (fatal SIGSEGV in the sig manager): the observation apparatus
//! re-entered the page walk inside the write window under test (walk_read
//! interacting with the write window) and perturbed the live shape of the
//! system under test (case-file 4.2 lesson one manifesting on real hardware).
//! It was fully reverted. The reshape moves the walk out of the write window to
//! a **quiescent point** (the timer tick arm: a single hart plus the BKL means
//! page-table writers cannot be in flight), leaving only one bounded record at
//! the write point (a ring push); the drain side uses a reentrancy guard to
//! block the second reentry that occurs when a tick nests into kcall.
//!
//! # Coverage (v1 boundary, stated honestly)
//!
//! - **KernelDm leaf writes (walk level)**: the four kernel-side leaf-write
//!   methods (map/remap/unmap/update_flags) record {root, vaddr, want, op}
//!   after writing; a quiescent-point drain walks again from root and compares
//!   class by class. Kernel-side manual mapping and boot loading use this path.
//! - **Gateway PteWrite (slot level)**: kernel-proxied writes made by VM via
//!   SYS_VMCTL PteWrite (KDM direct write plus global sfence, the
//!   `syscall.rs` arm) record {slot physical address, value}; a
//!   quiescent-point drain re-reads the slot. The wire carries only the slot
//!   address and the value (m1p1/m1p2); vaddr/root are out of scope (widening
//!   the wire would change the `[ARCH: riscv-vmddm]` contract, not done in v1).
//! - **Not covered**: boot-time direct writes in the unregistered VmDm state
//!   (the contract argument is not-present->present, so there is no stale entry
//!   to report); the child-table leaf copy in `split_huge` (one-time at boot);
//!   the PteZero whole-page zero fill (a new table has no translation). Once
//!   SMP is unclamped this instrument must be revisited per hart slot (the same
//!   precondition tf_guard carries).
//!
//! # Perturbation budget (a direct answer to case-file 4.2 lesson one)
//!
//! The record side **arms itself unconditionally** (the same precedent as
//! tf_guard LIVE: harmless magnitude over N=7 rounds of evidence), each leaf
//! write costs 3-4 Relaxed writes plus the wrap counter, with no disarm
//! branch: boundedness is borne by the ring capacity and the drain budget, not
//! by a switch. The drain side walks <=8 entries within the per-tick budget
//! (3 slot reads each) plus <=32 slot re-reads; printing over the whole boot is
//! <=1 liveness line plus <=8 HIT lines (the cap follows the tf_guard house
//! rule; serial writes are the ATF bottleneck, hence zero periodic printing).
//!
//! The pure logic layer (ring/classification/budgeted batch take) depends on
//! neither the Direct Map nor CSRs and is host-testable (the same non-gated
//! precedent as `riscv64_walk`); the walk/print wiring lives in
//! `riscv64::paging::ptwb_drain_budgeted` (inside the gate).

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// KernelDm leaf-write ring capacity (entries).
pub const RING_LEAF: usize = 64;
/// Gateway write slot-level ring capacity (entries).
pub const RING_SLOT: usize = 128;
/// Per-tick walk-level budget (entries).
pub const DRAIN_WALK_BUDGET: usize = 8;
/// Per-tick slot-level budget (entries).
pub const DRAIN_SLOT_BUDGET: usize = 32;
/// HIT print cap (house rule of 8; anything beyond is only counted).
pub const PRINT_CAP: usize = 8;

/// Leaf-write operation tag (for recording; HIT lines print the static string).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeafOp {
    /// `Paging::map`
    Map,
    /// `Paging::remap`
    Remap,
    /// `Paging::unmap` (want = 0)
    Unmap,
    /// `Paging::update_flags`
    UpdateFlags,
}

impl LeafOp {
    /// Static name for HIT lines (avoids runtime formatting).
    pub fn name(self) -> &'static str {
        match self {
            LeafOp::Map => "map",
            LeafOp::Remap => "remap",
            LeafOp::Unmap => "unmap",
            LeafOp::UpdateFlags => "update_flags",
        }
    }
}

/// Walk classification: the divergence shapes between the reported state and
/// the write-time intent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitClass {
    /// Write-time intent = invalid (unmap), yet the table has V=1: "ghost revival".
    Revived,
    /// Write-time intent = valid (map/remap/update_flags), yet the table has
    /// V=0 or the walk does not hit.
    Gone,
    /// Both sides valid but the PTE values differ (PPN or flag-bit drift).
    Changed,
    /// Slot-level re-read: the value written by the gateway is no longer the
    /// written value at the quiescent point.
    SlotDrift,
}

impl HitClass {
    /// Static name for HIT lines.
    pub fn name(self) -> &'static str {
        match self {
            HitClass::Revived => "REVIVED",
            HitClass::Gone => "GONE",
            HitClass::Changed => "CHANGED",
            HitClass::SlotDrift => "SLOT-DRIFT",
        }
    }
}

/// Sv39 valid bit (same value as `Sv39PteFlags::V`; the pure layer does not
/// depend on the in-gate bitflags).
pub const PTE_V: u64 = 1 << 0;

// ── Walk-level ring: {root, vaddr, want, op} × RING_LEAF ──
static LEAF_ROOT: [AtomicUsize; RING_LEAF] = [const { AtomicUsize::new(0) }; RING_LEAF];
static LEAF_VADDR: [AtomicUsize; RING_LEAF] = [const { AtomicUsize::new(0) }; RING_LEAF];
static LEAF_WANT: [AtomicUsize; RING_LEAF] = [const { AtomicUsize::new(0) }; RING_LEAF];
static LEAF_OP: [AtomicUsize; RING_LEAF] = [const { AtomicUsize::new(0) }; RING_LEAF];

// ── Slot-level ring: {slot physical address, written value} × RING_SLOT ──
static SLOT_PA: [AtomicUsize; RING_SLOT] = [const { AtomicUsize::new(0) }; RING_SLOT];
static SLOT_VAL: [AtomicUsize; RING_SLOT] = [const { AtomicUsize::new(0) }; RING_SLOT];

// head = slot for the next write; tail = slot of the oldest not-yet-drained
// sample; COUNT = samples in the ring (0..=capacity; when the ring is full a
// write overwrites the oldest, which is "spot-check" semantics: keep the newest
// samples when the drain budget cannot keep up); WRAPS = wrap count. A
// count-based ring avoids the empty/full ambiguity (head==tail in both shapes).
static LEAF_HEAD: AtomicUsize = AtomicUsize::new(0);
static LEAF_TAIL: AtomicUsize = AtomicUsize::new(0);
static LEAF_COUNT: AtomicUsize = AtomicUsize::new(0);
static LEAF_WRAPS: AtomicUsize = AtomicUsize::new(0);
static SLOT_HEAD: AtomicUsize = AtomicUsize::new(0);
static SLOT_TAIL: AtomicUsize = AtomicUsize::new(0);
static SLOT_COUNT: AtomicUsize = AtomicUsize::new(0);
static SLOT_WRAPS: AtomicUsize = AtomicUsize::new(0);

// ── Reading counters (for the settlement line / negative control) and the drain reentrancy guard ──
static VERIFIED_LEAF: AtomicUsize = AtomicUsize::new(0);
static VERIFIED_SLOT: AtomicUsize = AtomicUsize::new(0);
static DRAINS: AtomicUsize = AtomicUsize::new(0);
static HITS: AtomicUsize = AtomicUsize::new(0);
static DRAINING: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicBool = AtomicBool::new(false);
static PRINTED: AtomicUsize = AtomicUsize::new(0);

/// One-shot voucher for the liveness line: the first drain that verified
/// something prints one execution record (swap guarantees it fires exactly
/// once; the printing itself sits in the in-gate drain fn, serial writes do not
/// enter the pure layer).
pub fn first_live() -> bool {
    !LIVE.swap(true, Ordering::AcqRel)
}

/// Global HIT print permit (v3: the cap is <=8 lines for the **whole boot**,
/// not 8 per drain -- in v1/v2 a per-drain reset let the serial console flood
/// whenever HITs appeared; serial writes are the ATF bottleneck, and apparatus
/// self-perturbation is exactly the lesson shape behind the first failed
/// attempt). Print only after obtaining a permit.
pub fn take_print_permit() -> bool {
    PRINTED.fetch_add(1, Ordering::AcqRel) < PRINT_CAP
}

/// Drain reentrancy guard: when a tick nests into kcall the second drain exits
/// immediately (reentrancy was precisely the lesson of the failed attempt; swap
/// guarantees the nested caller gets nothing). Returns true = this call is the
/// outer one.
pub fn drain_guard_enter() -> bool {
    !DRAINING.swap(true, Ordering::AcqRel)
}

/// Release once the outer drain has finished.
pub fn drain_guard_exit() {
    DRAINING.store(false, Ordering::Release);
}

/// KernelDm leaf-write record (single writer inside the BKL critical section;
/// the atomics are only hygiene for a future SMP unclamp).
/// A same key (root,vaddr) already in the ring means the expectation is
/// **updated in place** (v2: later legal writes refresh the expectation instead
/// of stacking a new sample) -- drift at the quiescent point after dedup means
/// "an unrecorded writer changed the table", which is the contamination
/// signature itself; a full ring overwrites the oldest and advances tail so the
/// drain side only ever reads samples that were not overwritten.
pub fn record_leaf(root: u64, vaddr: u64, want: u64, op: LeafOp) {
    // Same-key dedup: scan only what is in the ring (tail..tail+count).
    let count = LEAF_COUNT.load(Ordering::Acquire);
    let tail = LEAF_TAIL.load(Ordering::Acquire);
    for k in 0..count {
        let j = (tail + k) % RING_LEAF;
        if LEAF_ROOT[j].load(Ordering::Relaxed) == root as usize
            && LEAF_VADDR[j].load(Ordering::Relaxed) == vaddr as usize
        {
            LEAF_WANT[j].store(want as usize, Ordering::Relaxed);
            LEAF_OP[j].store(op as usize, Ordering::Relaxed);
            return;
        }
    }
    let i = LEAF_HEAD.load(Ordering::Acquire);
    LEAF_ROOT[i].store(root as usize, Ordering::Relaxed);
    LEAF_VADDR[i].store(vaddr as usize, Ordering::Relaxed);
    LEAF_WANT[i].store(want as usize, Ordering::Relaxed);
    LEAF_OP[i].store(op as usize, Ordering::Relaxed);
    let head_after = (i + 1) % RING_LEAF;
    LEAF_HEAD.store(head_after, Ordering::Release);
    if head_after == 0 {
        LEAF_WRAPS.fetch_add(1, Ordering::Relaxed);
    }
    // Full ring: overwrite the oldest (tail advances in step), count capped at capacity.
    let count = LEAF_COUNT.load(Ordering::Acquire);
    if count < RING_LEAF {
        LEAF_COUNT.store(count + 1, Ordering::Release);
    } else {
        LEAF_TAIL.store((LEAF_TAIL.load(Ordering::Acquire) + 1) % RING_LEAF, Ordering::Release);
    }
}

/// Gateway PteWrite slot-level record (same key = slot physical address; the
/// dedup semantics are those of `record_leaf` v2).
pub fn record_slot(slot_pa: u64, val: u64) {
    let count = SLOT_COUNT.load(Ordering::Acquire);
    let tail = SLOT_TAIL.load(Ordering::Acquire);
    for k in 0..count {
        let j = (tail + k) % RING_SLOT;
        if SLOT_PA[j].load(Ordering::Relaxed) == slot_pa as usize {
            SLOT_VAL[j].store(val as usize, Ordering::Relaxed);
            return;
        }
    }
    let i = SLOT_HEAD.load(Ordering::Acquire);
    SLOT_PA[i].store(slot_pa as usize, Ordering::Relaxed);
    SLOT_VAL[i].store(val as usize, Ordering::Relaxed);
    let head_after = (i + 1) % RING_SLOT;
    SLOT_HEAD.store(head_after, Ordering::Release);
    if head_after == 0 {
        SLOT_WRAPS.fetch_add(1, Ordering::Relaxed);
    }
    let count = SLOT_COUNT.load(Ordering::Acquire);
    if count < RING_SLOT {
        SLOT_COUNT.store(count + 1, Ordering::Release);
    } else {
        SLOT_TAIL.store((SLOT_TAIL.load(Ordering::Acquire) + 1) % RING_SLOT, Ordering::Release);
    }
}

/// Drain side takes one batch of walk-level samples (oldest first, at most
/// `out.len()`). Returns the number actually taken; the samples are written
/// into `out` (each {root, vaddr, want, op}).
pub fn take_leaf_batch(out: &mut [[u64; 4]]) -> usize {
    let mut n = 0;
    while n < out.len() {
        let count = LEAF_COUNT.load(Ordering::Acquire);
        if count == 0 {
            break;
        }
        let tail = LEAF_TAIL.load(Ordering::Acquire);
        out[n] = [
            LEAF_ROOT[tail].load(Ordering::Relaxed) as u64,
            LEAF_VADDR[tail].load(Ordering::Relaxed) as u64,
            LEAF_WANT[tail].load(Ordering::Relaxed) as u64,
            LEAF_OP[tail].load(Ordering::Relaxed) as u64,
        ];
        LEAF_TAIL.store((tail + 1) % RING_LEAF, Ordering::Release);
        LEAF_COUNT.store(count - 1, Ordering::Release);
        n += 1;
    }
    n
}

/// Drain side takes one batch of slot-level samples (oldest first, at most
/// `out.len()`). Returns the number actually taken; the samples are written
/// into `out` (each {slot physical address, written value}).
pub fn take_slot_batch(out: &mut [[u64; 2]]) -> usize {
    let mut n = 0;
    while n < out.len() {
        let count = SLOT_COUNT.load(Ordering::Acquire);
        if count == 0 {
            break;
        }
        let tail = SLOT_TAIL.load(Ordering::Acquire);
        out[n] = [
            SLOT_PA[tail].load(Ordering::Relaxed) as u64,
            SLOT_VAL[tail].load(Ordering::Relaxed) as u64,
        ];
        SLOT_TAIL.store((tail + 1) % RING_SLOT, Ordering::Release);
        SLOT_COUNT.store(count - 1, Ordering::Release);
        n += 1;
    }
    n
}

/// Leaf-level classification (pure function): write-time intent vs the state
/// reported at the quiescent point.
pub fn classify_leaf(want: u64, got: u64) -> Option<HitClass> {
    if want & PTE_V == 0 {
        if got & PTE_V != 0 {
            Some(HitClass::Revived)
        } else {
            None
        }
    } else if got & PTE_V == 0 {
        Some(HitClass::Gone)
    } else if want != got {
        Some(HitClass::Changed)
    } else {
        None
    }
}

/// Slot-level classification (pure function): a quiescent-point re-read that
/// differs from the written value means drift.
pub fn classify_slot(want: u64, got: u64) -> Option<HitClass> {
    if want == got {
        None
    } else {
        Some(HitClass::SlotDrift)
    }
}

/// Record one drain (liveness / settlement readings).
pub fn note_drain(verified_leaf: usize, verified_slot: usize, hits: usize) {
    DRAINS.fetch_add(1, Ordering::Relaxed);
    VERIFIED_LEAF.fetch_add(verified_leaf, Ordering::Relaxed);
    VERIFIED_SLOT.fetch_add(verified_slot, Ordering::Relaxed);
    HITS.fetch_add(hits, Ordering::Relaxed);
}

/// Liveness/settlement readings (for the four-part settlement line): (drain
/// count, walk-level verifications, slot-level verifications, HIT count, sum of
/// wrap counts).
pub fn readings() -> (usize, usize, usize, usize, usize) {
    (
        DRAINS.load(Ordering::Relaxed),
        VERIFIED_LEAF.load(Ordering::Relaxed),
        VERIFIED_SLOT.load(Ordering::Relaxed),
        HITS.load(Ordering::Relaxed),
        LEAF_WRAPS.load(Ordering::Relaxed) + SLOT_WRAPS.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
mod ptwb_tests {
    use super::*;

    // The ring is a process-wide static and tests contaminate each other, so
    // every case clears state first (safe under single-threaded test semantics).
    fn reset_ring() {
        LEAF_HEAD.store(0, Ordering::Relaxed);
        LEAF_TAIL.store(0, Ordering::Relaxed);
        LEAF_COUNT.store(0, Ordering::Relaxed);
        LEAF_WRAPS.store(0, Ordering::Relaxed);
        SLOT_HEAD.store(0, Ordering::Relaxed);
        SLOT_TAIL.store(0, Ordering::Relaxed);
        SLOT_COUNT.store(0, Ordering::Relaxed);
        SLOT_WRAPS.store(0, Ordering::Relaxed);
        DRAINING.store(false, Ordering::Relaxed);
        for i in 0..RING_LEAF {
            LEAF_ROOT[i].store(0, Ordering::Relaxed);
            LEAF_VADDR[i].store(0, Ordering::Relaxed);
            LEAF_WANT[i].store(0, Ordering::Relaxed);
            LEAF_OP[i].store(0, Ordering::Relaxed);
        }
        for i in 0..RING_SLOT {
            SLOT_PA[i].store(0, Ordering::Relaxed);
            SLOT_VAL[i].store(0, Ordering::Relaxed);
        }
    }

    #[test]
    fn classify_leaf_matrix() {
        // unmap intent (want V=0)
        assert_eq!(classify_leaf(0, 0), None);
        assert_eq!(classify_leaf(0, PTE_V | 0x123), Some(HitClass::Revived));
        // map intent (want V=1)
        let want = PTE_V | (1 << 10) | (1 << 6) | (1 << 7); // one frame of V|R|A|D
        assert_eq!(classify_leaf(want, want), None);
        assert_eq!(classify_leaf(want, 0), Some(HitClass::Gone));
        assert_eq!(classify_leaf(want, want ^ (1 << 10)), Some(HitClass::Changed));
        // a single-bit flag drift also classifies as Changed
        assert_eq!(classify_leaf(want, want | (1 << 4)), Some(HitClass::Changed));
    }

    #[test]
    fn classify_slot_matrix() {
        assert_eq!(classify_slot(0xabc, 0xabc), None);
        assert_eq!(classify_slot(0xabc, 0), Some(HitClass::SlotDrift));
    }

    #[test]
    fn leaf_ring_fifo_within_capacity() {
        reset_ring();
        for k in 0..5u64 {
            record_leaf(0x8000_0000 + k, k * 0x1000, PTE_V | k, LeafOp::Map);
        }
        let mut out = [[0u64; 4]; DRAIN_WALK_BUDGET];
        let n = take_leaf_batch(&mut out);
        assert_eq!(n, 5);
        for (k, e) in out.iter().enumerate().take(5) {
            assert_eq!(e[0], 0x8000_0000 + k as u64);
            assert_eq!(e[1], (k as u64) * 0x1000);
            assert_eq!(e[3], LeafOp::Map as u64);
        }
        // a take after the ring is drained = 0
        assert_eq!(take_leaf_batch(&mut out), 0);
    }

    #[test]
    fn leaf_ring_overwrites_oldest_when_full() {
        reset_ring();
        for k in 0..(RING_LEAF as u64 + 3) {
            record_leaf(k, k, PTE_V, LeafOp::Unmap);
        }
        assert!(LEAF_WRAPS.load(Ordering::Relaxed) >= 1);
        let mut out = [[0u64; 4]; RING_LEAF];
        let n = take_leaf_batch(&mut out);
        // after the full-ring overwrite only the newest RING_LEAF remain: seq 3 … RING_LEAF+2 (0..2 pushed out)
        assert_eq!(n, RING_LEAF);
        assert_eq!(out[0][0], 3);
        assert_eq!(out[RING_LEAF - 1][0], (RING_LEAF as u64 + 2));
    }

    #[test]
    fn slot_ring_batch_and_budget() {
        reset_ring();
        for k in 0..40u64 {
            record_slot(0x1000 + k * 8, 0xdead + k);
        }
        let mut out = [[0u64; 2]; DRAIN_SLOT_BUDGET];
        assert_eq!(take_slot_batch(&mut out), DRAIN_SLOT_BUDGET);
        assert_eq!(out[0][0], 0x1000);
        assert_eq!(out[0][1], 0xdead);
        // the remaining 8 drained in one take
        let mut out2 = [[0u64; 2]; DRAIN_SLOT_BUDGET];
        assert_eq!(take_slot_batch(&mut out2), 8);
        assert_eq!(out2[7][0], 0x1000 + 39 * 8);
        assert_eq!(take_slot_batch(&mut out2), 0);
    }

    #[test]
    fn readings_accumulate() {
        reset_ring();
        record_leaf(1, 2, PTE_V, LeafOp::Map);
        record_slot(3, 4);
        let mut leaf_out = [[0u64; 4]; DRAIN_WALK_BUDGET];
        let mut slot_out = [[0u64; 2]; DRAIN_SLOT_BUDGET];
        let nl = take_leaf_batch(&mut leaf_out);
        let ns = take_slot_batch(&mut slot_out);
        note_drain(nl, ns, 0);
        let (d, vl, vs, h, _w) = readings();
        assert_eq!((d, vl, vs, h), (1, 1, 1, 0));
    }

    #[test]
    fn leaf_ring_dedup_updates_expectation_in_place() {
        reset_ring();
        record_leaf(0x8000_0000, 0x1000, PTE_V | 1, LeafOp::Map);
        record_leaf(0x8000_0000, 0x2000, PTE_V | 2, LeafOp::Map);
        // same-key rewrite: the expectation is refreshed in place, no sample appended
        record_leaf(0x8000_0000, 0x1000, PTE_V | 9, LeafOp::UpdateFlags);
        let mut out = [[0u64; 4]; RING_LEAF];
        assert_eq!(take_leaf_batch(&mut out), 2, "去重后仍只 2 条在环");
        assert_eq!(out[0][1], 0x1000);
        assert_eq!(out[0][2], PTE_V | 9, "期望值被刷新");
        assert_eq!(out[0][3], LeafOp::UpdateFlags as u64);
        assert_eq!(out[1][1], 0x2000);
    }

    #[test]
    fn slot_ring_dedup_updates_val_in_place() {
        reset_ring();
        record_slot(0x8300, 0xaaa);
        record_slot(0x8400, 0xbbb);
        record_slot(0x8300, 0xccc);
        let mut out = [[0u64; 2]; RING_SLOT];
        assert_eq!(take_slot_batch(&mut out), 2);
        assert_eq!(out[0][0], 0x8300);
        assert_eq!(out[0][1], 0xccc, "同槽重写刷新期望值");
        assert_eq!(out[1][0], 0x8400);
    }

    #[test]
    fn dedupe_ignores_drained_entries() {
        reset_ring();
        record_slot(0x9000, 0x1);
        let mut out = [[0u64; 2]; RING_SLOT];
        take_slot_batch(&mut out); // drain fully
        // record the same key again: the old entry was drained, so it should enter as a new sample
        record_slot(0x9000, 0x2);
        assert_eq!(take_slot_batch(&mut out), 1);
    }

    #[test]
    fn drain_guard_blocks_recursion() {
        reset_ring();
        assert!(drain_guard_enter());
        // the nested caller does not get the guard
        assert!(!drain_guard_enter());
        drain_guard_exit();
        // reentry is allowed after the release
        assert!(drain_guard_enter());
        drain_guard_exit();
    }

    #[test]
    fn print_permit_is_global_boot_cap() {
        // Note: PRINTED is a process-wide static, so permits already handed out
        // (including on real hardware) are shared within this test process --
        // take the remainder up to PRINT_CAP, and the 9th is refused.
        let start = PRINTED.load(Ordering::Relaxed);
        let budget = if start < PRINT_CAP { PRINT_CAP - start } else { 0 };
        for _ in 0..budget {
            assert!(take_print_permit());
        }
        assert!(!take_print_permit(), "第 {} 张后必须拒绝", PRINT_CAP);
        // Old readings are not restored (inter-test hygiene; the excess is never
        // reclaimed -- later cases in this test process do not depend on the
        // print permit any more).
    }

    #[test]
    fn op_names_are_static() {
        assert_eq!(LeafOp::Map.name(), "map");
        assert_eq!(LeafOp::Unmap.name(), "unmap");
        assert_eq!(HitClass::Revived.name(), "REVIVED");
        assert_eq!(HitClass::SlotDrift.name(), "SLOT-DRIFT");
    }
}
