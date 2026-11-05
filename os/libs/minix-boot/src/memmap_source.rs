//! The memory map of the device-tree boot chains: one concrete collector plus
//! the two-table shape every boot chain hands the OS semantic layer.
//!
//! Semantic contract (the positive use of axiom 3): every boot chain exposes
//! the **same** shape to the OS semantic layer — two `'static` tables,
//! conventional (the PMM may allocate from it directly) and reserved (touched
//! through the Direct Map at run time but never reallocated). Each chain's own
//! firmware shape (UEFI `GetMemoryMap`, DTB `/memory`) stays inside its own
//! collector and does not leak into consumers (previously the opensbi leg handed
//! the kernel a "hard-coded single QEMU virt DRAM region" directly — exactly the
//! leak this contract closes).
//!
//! The contract is carried by the **result shape** that [`DtbMemoryMap::collect`]
//! returns, not by a trait. A trait here would advertise source substitutability
//! that no call site uses: no consumer ever took the collector as a trait object
//! or under a generic bound, the production UEFI leg cannot satisfy the
//! "two `'static` tables" signature while boot services are still alive (its
//! classification writes into the handoff arena instead), and a placeholder for
//! a boot form this repository does not have can only implement it by returning
//! two empty tables — an empty table that looks like a valid answer is worse
//! than no answer, because it fails deep in the kernel instead of at the edge.
//! A caller names the concrete collector it uses. An interface becomes worth
//! introducing when a second source gains a consumer that genuinely wants to be
//! source-agnostic.
//!
//! The boot-form layer is **not** merged here (axiom 3: the standalone `BootShim`
//! executable and the `bootface` self-boot face correspond to different machine
//! states; this layer only unifies the memmap source underneath).
//!
//! Implementation placement: this DTB collector lives in the library (DTB
//! parsing is isomorphic for every bare-metal boot face, and the `fdt`
//! dependency is already on this library's list). The UEFI leg lives in
//! boot-shim, because `GetMemoryMap` needs the `uefi` crate, which must not
//! point back into this library.

use crate::kernel_info::MemoryRegion;
use minix_types::PhysBytes;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Capacity of the DTB memory map. Symmetric with kernel-image bootface's old
/// `MEMMAP_MAX = 8` (the capacity moved with the migration; consumer semantics
/// are unchanged).
const DTB_MEMMAP_MAX: usize = 8;

/// Capacity of the memory reservation block scan. A missing `{0, 0}`
/// terminator makes the library keep yielding pairs, so this cap turns that
/// malformed shape into a refusal instead of a silent over-read.
const DTB_MRB_MAX: usize = 16;

/// Why a DTB memory map could not be produced.
///
/// Capacity is a policy input, not a display detail: dropping the excess
/// would hand the kernel a map that silently lacks a real RAM or reserved
/// range, so the collector fails instead of truncating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtbMemMapError {
    /// `/memory` produced more conventional spans than `DTB_MEMMAP_MAX`
    /// (the firmware-prefix split and the declared reservations each add
    /// spans, so the count is taken after both).
    TooManyFreeRegions,
    /// The reserved table (firmware prefix plus declared reservations) needs
    /// more entries than `DTB_MEMMAP_MAX`.
    TooManyReservedRegions,
    /// A declared range wraps past the 64-bit address space: `address + size`
    /// does not fit, so the range itself is malformed.
    RangeOverflow,
    /// A declared range is well formed but its exclusive end point is exactly
    /// 2^64 (or the outward page rounding lands there). `SourceRecord` keeps a
    /// `u64` end, so this project refuses the highest page rather than
    /// truncating it. The refusal is an input-policy choice and is named
    /// separately so it can never be a side effect of `u64` arithmetic.
    UnsupportedPhysicalAddressEnd,
    /// Two `/reserved-memory` declarations **partially** overlap. Exact
    /// duplicates (same range, same intent) are deduplicated in the projection
    /// while both source records survive; the same range declared with and
    /// without `no-map` is a semantic conflict
    /// (`ConflictingReservationAttributes`), not a duplicate.
    OverlappingReservedRanges,
    /// A `/reserved-memory` entry states its extent in a shape this collector
    /// cannot read: cell counts above two, or a `reg` that does not parse.
    /// Skipping it would free pages firmware reserved.
    UnsupportedReservedDeclaration,
    /// A `/reserved-memory` child declares only `size` (no `reg`): it asks for
    /// a *dynamically allocated* region. This boot has no allocator that could
    /// place it, so there is no range to reserve — silently ignoring the node
    /// would hand the pages to the PMM while a later consumer still expects
    /// them to be reserved. Reject instead of guessing a placement.
    UnsupportedDynamicReservation,
    /// A `/reserved-memory` node is visible in the tree but upstream path
    /// lookup cannot reach it. Treating it as absent would free its pages.
    ReservedMemoryUnreachable,
    /// The `/reserved-memory` parent does not declare `ranges` at all. DTSpec
    /// requires the property; a missing one is a malformed input, not an
    /// unsupported feature.
    MissingReservedMemoryRanges,
    /// The `/reserved-memory` parent declares a non-empty `ranges`: child
    /// addresses would need a translation this collector does not implement.
    /// The input is well-formed — this is a project supported-subset limit.
    UnsupportedReservedMemoryRanges,
    /// The `/reserved-memory` parent (or the root) does not declare
    /// `#address-cells` / `#size-cells`. DTSpec requires both.
    MissingReservedMemoryCells,
    /// The parent's cell counts differ from the root's. DTSpec says they
    /// *should* match; requiring equality is this project's supported subset,
    /// because child `reg` is read as physical as written.
    MismatchedReservedMemoryCells,
    /// A `/reserved-memory` child declares both `no-map` and `reusable`; the
    /// devicetree specification makes the two mutually exclusive.
    ConflictingReservationAttributes,
    /// A marker property (`no-map`, `reusable`) carries a value; both are
    /// defined as empty properties, and a value means the encoding is wrong.
    NonEmptyReservationAttribute,
    /// The memory reservation block has more entries than `DTB_MRB_MAX` (a
    /// missing terminator also lands here).
    TooManyMemoryReservations,
    /// A memory reservation entry has a zero size but a non-zero address. Only
    /// `{0, 0}` terminates the block; a zero-length entry is malformed and
    /// must not be silently accepted as a reservation.
    ZeroLengthMemoryReservation,
    /// Two memory reservation block entries overlap. DTSpec 5.3.2 says the
    /// regions "shall not overlap each other", so an identical pair is an
    /// overlap too — no deduplication here, unlike `/reserved-memory`
    /// duplicates, which are a project-level tolerance.
    OverlappingMemoryReservations,
    /// A `/memory` node carries a `status` other than "okay" (or an unknown
    /// value). Such memory is not described as usable RAM, so handing it to
    /// the allocator would ignore the node's own state.
    UnsupportedMemoryNodeStatus,
    /// Two `/memory` declarations partially overlap. A partial overlap would
    /// place the same page in the panorama twice; this project refuses it as a
    /// firmware input anomaly (exact duplicates are deduplicated instead).
    OverlappingMemoryRanges,
    /// A `/memory` node is marked `hotpluggable`. The kernel has no memory
    /// removal mechanism, so removable memory must not enter the PMM.
    HotpluggableMemoryUnsupported,
    /// The source-record buffer would overflow. Unreachable while the two
    /// projections fit their own capacities; kept as a refusal so a future
    /// capacity change cannot silently drop a record.
    TooManySourceRecords,
}

// `static mut`: the same form as bootface's MEMMAP_STORAGE — the single-hart
// boot face writes once and the storage is read-only afterwards; the read side
// never hands out `&mut` (addr_of! base plus from_raw_parts).
static mut DTB_FREE: [MemoryRegion; DTB_MEMMAP_MAX] =
    [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
static mut DTB_RESERVED: [MemoryRegion; DTB_MEMMAP_MAX] =
    [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
static DTB_FREE_N: AtomicUsize = AtomicUsize::new(0);
static DTB_RESERVED_N: AtomicUsize = AtomicUsize::new(0);

/// Capacity of the source-record buffer. The bound comes from the **input**
/// caps this collector enforces, not from the projection tables: `/memory`
/// spans and declared reservations each stop at `DTB_MEMMAP_MAX` (the same
/// `push_span`/`push_declared` caps — `no-map` declarations count too), the
/// firmware prefix cannot outnumber `/memory` spans, and the MRB stops at
/// `DTB_MRB_MAX`. Worst case 8 + 8 + 16 + 8 = 40. An overflow is a refusal
/// (`TooManySourceRecords`), never a silent drop.
const DTB_SOURCE_MAX: usize = DTB_MEMMAP_MAX + DTB_MEMMAP_MAX + DTB_MRB_MAX + DTB_MEMMAP_MAX;

/// Which declaration a range came from. Records are facts; every policy
/// projection (PMM, mapping denial, access denial) decides on its own how to
/// intersect, round and merge them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// A `/memory` `reg` span (the RAM panorama).
    Memory,
    /// The firmware-resident prefix carved out of a `/memory` span.
    FirmwarePrefix,
    /// One entry of the device-tree memory reservation block (MRB).
    MrbEntry,
    /// A `/reserved-memory` child range without `no-map` or `reusable`.
    ReservedOrdinary,
    /// A `/reserved-memory` child range marked `reusable`.
    ReservedReusable,
    /// A `/reserved-memory` child range marked `no-map`.
    ReservedNoMap,
}

/// Whether a record is declared by the input object or derived by this project.
/// The distinction matters for audit: a derived prefix must not read as a
/// firmware-declared reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceClass {
    /// Declared by the input object itself (`/memory`, `reg`, MRB entry).
    InputDeclaration,
    /// Derived from an input fact plus a boot parameter (`firmware_reserve_end`).
    DerivedReservation,
}

impl SourceKind {
    pub const fn class(self) -> SourceClass {
        match self {
            SourceKind::FirmwarePrefix => SourceClass::DerivedReservation,
            _ => SourceClass::InputDeclaration,
        }
    }
}

/// Where inside the input object a record came from. Two declarations of the
/// same range must stay distinguishable, so identity is not the byte range.
/// Ordinals are 0-based in document order: `node` counts matching nodes only
/// (`/memory` nodes for the first variant, `/reserved-memory` children for the
/// last), and `reg` counts tuples **inside that node's `reg` property**.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceOrigin {
    /// The `node`-th `/memory` node, `reg` tuple `reg`.
    MemoryNode { node: u16, reg: u16 },
    /// Derived by this project from the `/memory` record `memory_id`, cut at
    /// `cut` (the boot-path `firmware_reserve_end` the range was intersected
    /// with), so the derivation stays explainable after the DTB is released.
    FirmwarePrefix { memory_id: u16, cut: u64 },
    /// The `entry`-th memory reservation block entry (0-based, terminator
    /// excluded).
    MrbEntry(u16),
    /// The `node`-th `/reserved-memory` child, `reg` tuple `reg`.
    ReservedMemoryChild { node: u16, reg: u16 },
}

/// Normalized physical range: 64-bit start, 128-bit exclusive end, so the last
/// page of the address space (`[0xffff_ffff_ffff_f000, 2^64)`) is representable
/// even though today's `MemoryRegion` projections still carry a `u64` extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRange {
    pub start: u64,
    pub end_exclusive: u128,
}

impl PhysicalRange {
    pub const fn new(start: u64, end_exclusive: u128) -> Self {
        Self { start, end_exclusive }
    }

    pub const fn len(self) -> u128 {
        self.end_exclusive - self.start as u128
    }

    /// `Some(end)` when the end fits in 64 bits; `None` when it is exactly
    /// `2^64` — the highest page, which the u64 projection arithmetic cannot
    /// carry yet.
    pub const fn projectable_end(self) -> Option<u64> {
        if self.end_exclusive < PHYS_END {
            Some(self.end_exclusive as u64)
        } else {
            None
        }
    }
}

/// One physical range carried by the source layer. For input declarations
/// (`class() == InputDeclaration`) the range is the declared extent, byte for
/// byte; for a derived reservation (`FirmwarePrefix`) it is the **derived**
/// extent `M_i ∩ [0, F)`. No page range is stored here — `/memory` projects
/// inward (`FullPages`) while reservations project outward, so a single
/// precomputed page extent would bake one consumer's rule into every consumer.
/// The field is typed `PhysicalRange`, so an end of `2^64` is representable at
/// the record layer; whether a projection can deliver it is a separate question
/// answered by `projectable_end()` and the refusal at each projection site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRecord {
    /// Stable within one input object; cross-input references need
    /// `(InputObjectId, LocalSourceId)`, never this id alone.
    pub id: u16,
    pub kind: SourceKind,
    pub origin: SourceOrigin,
    pub range: PhysicalRange,
}

static mut DTB_SOURCES: [SourceRecord; DTB_SOURCE_MAX] = [SourceRecord {
    id: 0,
    kind: SourceKind::Memory,
    origin: SourceOrigin::MemoryNode { node: 0, reg: 0 },
    range: PhysicalRange::new(0, 0),
}; DTB_SOURCE_MAX];
static DTB_SOURCES_N: AtomicUsize = AtomicUsize::new(0);

/// Clip the part of `[start, start+len)` that falls inside the resident
/// firmware area (`[0, fw_end)`, a lower-bound semantic measured in PAs from
/// zero) into reserved, and return the rest as conventional. Returns
/// `(free_start, free_len)`; `None` = the whole span is firmware area (no free
/// remainder).
///
/// A pure function — the compile-time guards at the bottom of this module pin
/// down its single-character mutations (the arithmetic was migrated from the
/// original bootface `clip_firmware`; with `fw_end` parameterized, the riscv leg
/// passes `KERN_PHYS_BASE` and the opensbi boot-shim leg passes 0 = no
/// clipping).
const fn clip_reserved(
    start: u64,
    len: u64,
    fw_end: u64,
) -> Option<(u64, u64)> {
    if start >= fw_end {
        return Some((start, len));
    }
    let drop = fw_end - start;
    if drop >= len {
        return None; // the whole span sits below the firmware area
                     // (including the exact len==drop case)
    }
    Some((fw_end, len - drop))
}

// Compile-time guards for the clipping arithmetic (migrated from the original
// bootface `clip_firmware` assertion group; `fw_end` is passed explicitly as
// 0x8020_0000 = the true value of the riscv leg's `KERN_PHYS_BASE`):
//   • a pure firmware span `[DRAM, FW_END)` → None
const _FW_PURE: () = assert!(clip_reserved(0x8000_0000, 0x20_0000, 0x8020_0000).is_none());
//   • a span entirely above the image → kept as is
const _FW_ABOVE: () = assert!(
    matches!(clip_reserved(0x8020_0000, 0x1000, 0x8020_0000), Some((0x8020_0000, 0x1000)))
);
//   • a straddling span → clip off the firmware area, keep the rest
const _FW_CROSS: () = assert!(
    matches!(
        clip_reserved(0x8000_0000, 0x20_1000, 0x8020_0000),
        Some((0x8020_0000, 0x1000))
    )
);

const PAGE_BYTES: u64 = 4096;

/// Page-rounded extent of a declared reservation, rounded outward: a page that
/// is only partly declared is not fully free.
/// The first address that does not fit a 64-bit physical address.
const PHYS_END: u128 = 1u128 << 64;

/// `address + size` in `u128`. Only a genuine overflow (`> 2^64`) is an error:
/// an end of exactly `2^64` is the well-formed top boundary and is representable
/// in a `PhysicalRange`. Whether a *projection* can deliver that end is checked
/// where the projection is built, not here.
fn physical_end(start: u64, len: u64) -> Result<u128, DtbMemMapError> {
    let end = (start as u128) + (len as u128);
    if end > PHYS_END {
        return Err(DtbMemMapError::RangeOverflow);
    }
    Ok(end)
}

fn declared_page_range(start: u64, len: u64) -> Result<(u64, u64), DtbMemMapError> {
    let range_end = physical_end(start, len)?;
    let start = start & !(PAGE_BYTES - 1);
    // range_end <= 2^64, so rounding outward can at most reach 2^64 exactly;
    // that is a projection-layer refusal, not an input error.
    let projected = (range_end + (PAGE_BYTES - 1) as u128) & !((PAGE_BYTES - 1) as u128);
    if projected >= PHYS_END {
        return Err(DtbMemMapError::UnsupportedPhysicalAddressEnd);
    }
    Ok((start, projected as u64))
}

fn push_span(
    table: &mut [MemoryRegion; DTB_MEMMAP_MAX],
    n: &mut usize,
    start: u64,
    len: u64,
    full: DtbMemMapError,
) -> Result<(), DtbMemMapError> {
    if len == 0 {
        return Ok(());
    }
    if *n >= DTB_MEMMAP_MAX {
        return Err(full);
    }
    table[*n] = MemoryRegion { base: PhysBytes(start), len: len as usize };
    *n += 1;
    Ok(())
}

fn push_source(
    sources: &mut [SourceRecord; DTB_SOURCE_MAX],
    n: &mut usize,
    kind: SourceKind,
    origin: SourceOrigin,
    range: PhysicalRange,
) -> Result<(), DtbMemMapError> {
    if *n >= DTB_SOURCE_MAX {
        return Err(DtbMemMapError::TooManySourceRecords);
    }
    sources[*n] = SourceRecord {
        id: *n as u16,
        kind,
        origin,
        range,
    };
    *n += 1;
    Ok(())
}

/// Remove `[start, end)` from the conventional table. A span that straddles the
/// hole yields up to two pieces; a table that cannot hold them fails instead of
/// merging the hole back into RAM.
fn carve_free(
    table: &mut [MemoryRegion; DTB_MEMMAP_MAX],
    n: &mut usize,
    start: u64,
    end: u64,
) -> Result<(), DtbMemMapError> {
    let mut out = [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
    let mut out_n = 0usize;
    for &span in table[..*n].iter() {
        let span_start = span.base.0;
        let span_len = span.len as u64;
        let span_end = span_start + span_len;
        if span_end <= start || end <= span_start {
            push_span(
                &mut out,
                &mut out_n,
                span_start,
                span_len,
                DtbMemMapError::TooManyFreeRegions,
            )?;
        } else {
            if span_start < start {
                push_span(
                    &mut out,
                    &mut out_n,
                    span_start,
                    start - span_start,
                    DtbMemMapError::TooManyFreeRegions,
                )?;
            }
            if end < span_end {
                push_span(
                    &mut out,
                    &mut out_n,
                    end,
                    span_end - end,
                    DtbMemMapError::TooManyFreeRegions,
                )?;
            }
        }
    }
    *table = out;
    *n = out_n;
    Ok(())
}

/// Record one declaration after checking it against the earlier ones. The
/// device-tree spec forbids overlapping children, and overlap is not benign
/// here even when only one side reaches the reserved table: a `no-map`
/// declaration overlapped by a plain one would otherwise authorise access to
/// pages the other declaration forbids.
/// How one `/reserved-memory` declaration was qualified. `reusable` is its own
/// intent: it carries an owner/reclaim semantics that ordinary reservations do
/// not, so the two must not collapse into "same declaration" just because both
/// leave the PMM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReservationIntent {
    Ordinary,
    Reusable,
    NoMap,
}

fn push_declared(
    declared: &mut [(u64, u64, ReservationIntent); DTB_MEMMAP_MAX],
    n: &mut usize,
    start: u64,
    end: u64,
    intent: ReservationIntent,
) -> Result<bool, DtbMemMapError> {
    for &(prev_start, prev_end, prev_intent) in declared[..*n].iter() {
        if start == prev_start && end == prev_end {
            if intent == prev_intent {
                // Same range, same intent: a duplicate declaration, not an
                // overlap. The projections already hold this range, so only
                // the record stays (identity is kept).
                return Ok(true);
            }
            // Same bytes under different intents cannot both be honoured:
            // ordinary vs `reusable` differs in lifecycle, ordinary vs
            // `no-map` differs in access, `reusable` vs `no-map` is the
            // specification's own mutual exclusion.
            return Err(DtbMemMapError::ConflictingReservationAttributes);
        }
        if start < prev_end && prev_start < end {
            return Err(DtbMemMapError::OverlappingReservedRanges);
        }
    }
    if *n >= DTB_MEMMAP_MAX {
        return Err(DtbMemMapError::TooManyReservedRegions);
    }
    declared[*n] = (start, end, intent);
    *n += 1;
    Ok(false)
}

/// DTB leg: parse `reg` of `/memory` and `/memory@<unit>` (the unit-address
/// variant), subtract the resident firmware area through [`clip_reserved`] and
/// every declared reservation (`/reserved-memory` children and the memory
/// reservation block), and produce the two tables. Lookup of `/memory` walks
/// the whole tree through `all_nodes` rather than `find_node` (upstream fdt
/// 0.1.5's `find_node` misses nodes in some firmware-rebuilt blobs —
/// demonstrated in practice, so both consuming bootfaces take the same detour);
/// `/reserved-memory` uses path lookup with a visibility cross-check, because
/// the same miss there would silently free reserved pages.
///
/// `firmware_reserve_end`: upper bound (PA) of the resident firmware area. The
/// riscv bootface leg passes `KERN_PHYS_BASE` (OpenSBI resides in
/// `[DRAM_BASE, KERN_PHYS_BASE)` and the VM bitmap metadata memsets that whole
/// span — confirmed on real hardware, so clipping is functionally required); the
/// opensbi boot-shim leg passes 0 = no clipping (that leg's kernel side cuts the
/// map itself through `cut_memmap`).
pub struct DtbMemoryMap<'a> {
    fdt: &'a fdt::Fdt<'a>,
    firmware_reserve_end: u64,
}

impl<'a> DtbMemoryMap<'a> {
    pub fn new(fdt: &'a fdt::Fdt<'a>, firmware_reserve_end: u64) -> Self {
        Self {
            fdt,
            firmware_reserve_end,
        }
    }
}

impl DtbMemoryMap<'_> {
    /// Return the two `'static` tables `(conventional, reserved)`.
    ///
    /// Conventional holds the pages the PMM may allocate. Reserved holds pages
    /// that stay reachable but are never reallocated: the firmware-resident
    /// prefix and the declared reservations without `no-map`. A `no-map`
    /// reservation and every memory-reservation-block entry leave both tables,
    /// because that data may not be accessed at all.
    ///
    /// Borrowing `self` reads the DTB a second time is not required: the tables
    /// live in this library's static storage, so the returned slices outlive
    /// the borrow of the blob.
    pub fn collect(
        &self,
    ) -> Result<(&'static [MemoryRegion], &'static [MemoryRegion]), DtbMemMapError> {
        let (free, reserved, _) = self.collect_inner()?;
        Ok((free, reserved))
    }

    /// The same collection, plus the unclipped per-declaration records the
    /// projections were built from. A `no-map` or MRB range outside `/memory`
    /// appears in neither table, so this is the only output that still carries
    /// those facts.
    pub fn collect_with_sources(
        &self,
    ) -> Result<
        (
            &'static [MemoryRegion],
            &'static [MemoryRegion],
            &'static [SourceRecord],
        ),
        DtbMemMapError,
    > {
        self.collect_inner()
    }

    fn collect_inner(
        &self,
    ) -> Result<
        (
            &'static [MemoryRegion],
            &'static [MemoryRegion],
            &'static [SourceRecord],
        ),
        DtbMemMapError,
    > {
        // Single-hart boot face: written once, then read-only; a re-entry that
        // zeroes the counters (a second collect on the same face) merely
        // overwrites the same cells.
        DTB_FREE_N.store(0, Ordering::Relaxed);
        DTB_RESERVED_N.store(0, Ordering::Relaxed);
        DTB_SOURCES_N.store(0, Ordering::Relaxed);
        let mut sources = [SourceRecord {
            id: 0,
            kind: SourceKind::Memory,
            origin: SourceOrigin::MemoryNode { node: 0, reg: 0 },
            range: PhysicalRange::new(0, 0),
        }; DTB_SOURCE_MAX];
        let mut sources_n = 0usize;
        let mut free = [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
        let mut reserved = [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
        // The unclipped panorama: a declaration means something only where it
        // overlaps RAM, and its reserved entry must not extend past it (a
        // framebuffer-style reservation outside `/memory` claims no RAM).
        let mut memory = [MemoryRegion { base: PhysBytes(0), len: 0 }; DTB_MEMMAP_MAX];
        // Declarations in raw page coordinates with their `no-map` intent,
        // kept for the duplicate/overlap decision (the reserved table holds
        // just the reachable subset).
        let mut declared = [(0u64, 0u64, ReservationIntent::Ordinary); DTB_MEMMAP_MAX];
        let mut free_n = 0usize;
        let mut reserved_n = 0usize;
        let mut memory_n = 0usize;
        let mut declared_n = 0usize;

        // 1. The `/memory` panorama, with the firmware-resident prefix moved to
        //    the reserved table: those pages are reachable from the first
        //    instruction and are never reallocated.
        let mut memory_node_ordinal: u16 = 0;
        for node in self.fdt.all_nodes() {
            // `/memory` or `/memory@80000000` (the unit-address variant).
            if node.name.split('@').next() != Some("memory") {
                continue;
            }
            let node_ordinal = memory_node_ordinal;
            memory_node_ordinal = memory_node_ordinal.wrapping_add(1);
            // Missing `status` means "okay"; anything else describes memory
            // the node itself says is not usable, so it cannot be RAM.
            let status_ok = match node.property("status") {
                None => true,
                Some(p) => p.value.split(|&b| b == 0).next() == Some(&b"okay"[..]),
            };
            if !status_ok {
                return Err(DtbMemMapError::UnsupportedMemoryNodeStatus);
            }
            // The kernel cannot remove memory, so removable ranges must not
            // become ordinary allocations that later vanish.
            if node.property("hotpluggable").is_some() {
                return Err(DtbMemMapError::HotpluggableMemoryUnsupported);
            }
            let Some(regs) = node.reg() else { continue };
            for (reg_index, region) in regs.enumerate() {
                let start = region.starting_address as u64;
                let len = match region.size {
                    Some(s) if s != 0 => s as u64,
                    _ => continue,
                };
                let end = physical_end(start, len)?;
                let record_range = PhysicalRange::new(start, end);
                let Some(end) = record_range.projectable_end() else {
                    // Well-formed input, but the page-projection arithmetic is
                    // still u64: the top page is refused here, not at input.
                    return Err(DtbMemMapError::UnsupportedPhysicalAddressEnd);
                };
                let memory_id = sources_n as u16;
                // Overlapping RAM claims are a firmware input anomaly: a
                // partial overlap would put a page in the panorama twice and
                // let the PMM hand it out twice, so it is refused. An exact
                // duplicate is a repeated claim of the same bytes — it keeps
                // its record but projects only once.
                let mut duplicate = false;
                for &span in memory[..memory_n].iter() {
                    let prev_start = span.base.0;
                    let prev_end = prev_start + span.len as u64;
                    if start < prev_end && prev_start < end {
                        if start == prev_start && end == prev_end {
                            duplicate = true;
                            break;
                        }
                        return Err(DtbMemMapError::OverlappingMemoryRanges);
                    }
                }
                push_source(
                    &mut sources,
                    &mut sources_n,
                    SourceKind::Memory,
                    SourceOrigin::MemoryNode { node: node_ordinal, reg: reg_index as u16 },
                    record_range,
                )?;
                if duplicate {
                    continue;
                }
                push_span(
                    &mut memory,
                    &mut memory_n,
                    start,
                    len,
                    DtbMemMapError::TooManyFreeRegions,
                )?;
                if let Some((free_start, free_len)) =
                    clip_reserved(start, len, self.firmware_reserve_end)
                {
                    push_span(
                        &mut free,
                        &mut free_n,
                        free_start,
                        free_len,
                        DtbMemMapError::TooManyFreeRegions,
                    )?;
                }
                if start < self.firmware_reserve_end {
                    let prefix_len = (self.firmware_reserve_end - start).min(len);
                    push_span(
                        &mut reserved,
                        &mut reserved_n,
                        start,
                        prefix_len,
                        DtbMemMapError::TooManyReservedRegions,
                    )?;
                    push_source(
                        &mut sources,
                        &mut sources_n,
                        SourceKind::FirmwarePrefix,
                        SourceOrigin::FirmwarePrefix {
                            memory_id,
                            cut: self.firmware_reserve_end,
                        },
                        PhysicalRange::new(start, start as u128 + prefix_len as u128),
                    )?;
                }
            }
        }
        // 2. The device-tree memory reservation block: regions no client may
        //    access at all, so they leave the conventional table and enter
        //    neither (the reserved table means "reachable, never reallocated").
        let mut mrb = [(0u64, 0u64); DTB_MRB_MAX];
        let mut mrb_n = 0usize;
        for (entry_index, entry) in self.fdt.memory_reservations().enumerate() {
            if entry.size() == 0 {
                return Err(DtbMemMapError::ZeroLengthMemoryReservation);
            }
            let (start, end) = declared_page_range(entry.address() as u64, entry.size() as u64)?;
            push_source(
                &mut sources,
                &mut sources_n,
                SourceKind::MrbEntry,
                SourceOrigin::MrbEntry(entry_index as u16),
                PhysicalRange::new(
                    entry.address() as u64,
                    physical_end(entry.address() as u64, entry.size() as u64)?,
                ),
            )?;
            if mrb_n >= DTB_MRB_MAX {
                return Err(DtbMemMapError::TooManyMemoryReservations);
            }
            for &(prev_start, prev_end) in mrb[..mrb_n].iter() {
                if start < prev_end && prev_start < end {
                    return Err(DtbMemMapError::OverlappingMemoryReservations);
                }
            }
            mrb[mrb_n] = (start, end);
            mrb_n += 1;
            carve_free(&mut free, &mut free_n, start, end)?;
        }

        // 3. `/reserved-memory`: declared reservations are not allocatable.
        //    Without `no-map` they stay reachable through the Direct Map and
        //    join the reserved table; with `no-map` they leave both tables.
        if let Some(node) = self.lookup_reserved_memory()? {
            // `reg()` silently assumes default cell counts when the node does
            // not state them; a reservation whose shape we did not read is not
            // a reservation we can honour. The specification also recommends
            // the same cell counts as the root, and this collector relies on
            // that (child `reg` is read as physical as written).
            let Some(root) = self.fdt.all_nodes().next() else {
                return Err(DtbMemMapError::UnsupportedReservedDeclaration);
            };
            let root_cells = (
                root.property("#address-cells").and_then(|p| p.as_usize()),
                root.property("#size-cells").and_then(|p| p.as_usize()),
            );
            let node_cells = (
                node.property("#address-cells").and_then(|p| p.as_usize()),
                node.property("#size-cells").and_then(|p| p.as_usize()),
            );
            if node_cells.0.is_none()
                || node_cells.1.is_none()
                || root_cells.0.is_none()
                || root_cells.1.is_none()
            {
                return Err(DtbMemMapError::MissingReservedMemoryCells);
            }
            if node_cells != root_cells {
                // *Should* match in DTSpec; equality is this collector's
                // supported subset, not a format error.
                return Err(DtbMemMapError::MismatchedReservedMemoryCells);
            }
            // DTSpec requires `ranges`; it *should* be empty. A missing
            // property is malformed, a non-empty one is well-formed input this
            // collector cannot translate yet — two different failures.
            let Some(ranges) = node.property("ranges") else {
                return Err(DtbMemMapError::MissingReservedMemoryRanges);
            };
            if !ranges.value.is_empty() {
                return Err(DtbMemMapError::UnsupportedReservedMemoryRanges);
            }
            for (child_index, child) in node.children().enumerate() {
                if child.property("reg").is_none() {
                    // A child with `size` but no `reg` asks for a dynamic
                    // placement; a child with neither states no extent at all.
                    if child.property("size").is_some() {
                        return Err(DtbMemMapError::UnsupportedDynamicReservation);
                    }
                    return Err(DtbMemMapError::UnsupportedReservedDeclaration);
                }
                // `reg` wins when both are present: the region is statically
                // specified, so `size` (and `alignment` / `alloc-ranges`) are
                // dynamic-only hints this collector ignores on purpose.
                let Some(regs) = child.reg() else {
                    return Err(DtbMemMapError::UnsupportedReservedDeclaration);
                };
                // Both markers are defined as empty properties. Presence with a
                // value is a malformed encoding, and guessing at the intent
                // would silently change whether the range may be mapped.
                let marker = |name: &str| -> Result<bool, DtbMemMapError> {
                    match child.property(name) {
                        None => Ok(false),
                        Some(p) if p.value.is_empty() => Ok(true),
                        Some(_) => Err(DtbMemMapError::NonEmptyReservationAttribute),
                    }
                };
                let no_map = marker("no-map")?;
                let reusable = marker("reusable")?;
                if no_map && reusable {
                    return Err(DtbMemMapError::ConflictingReservationAttributes);
                }
                for (reg_index, region) in regs.enumerate() {
                    let len = match region.size {
                        Some(s) if s != 0 => s as u64,
                        _ => continue,
                    };
                    let (start, end) = declared_page_range(region.starting_address as u64, len)?;
                    push_source(
                        &mut sources,
                        &mut sources_n,
                        if no_map {
                            SourceKind::ReservedNoMap
                        } else if reusable {
                            SourceKind::ReservedReusable
                        } else {
                            SourceKind::ReservedOrdinary
                        },
                        SourceOrigin::ReservedMemoryChild {
                            node: child_index as u16,
                            reg: reg_index as u16,
                        },
                        PhysicalRange::new(
                            region.starting_address as u64,
                            physical_end(region.starting_address as u64, len)?,
                        ),
                    )?;
                    let intent = if no_map {
                        ReservationIntent::NoMap
                    } else if reusable {
                        ReservationIntent::Reusable
                    } else {
                        ReservationIntent::Ordinary
                    };
                    if push_declared(&mut declared, &mut declared_n, start, end, intent)? {
                        // Exact duplicate: the projection already covers it,
                        // the record above keeps the second source identity.
                        continue;
                    }
                    for &span in memory[..memory_n].iter() {
                        let span_start = span.base.0;
                        let span_end = span_start + span.len as u64;
                        let clip_start = start.max(span_start);
                        let clip_end = end.min(span_end);
                        if clip_start >= clip_end {
                            continue;
                        }
                        carve_free(&mut free, &mut free_n, clip_start, clip_end)?;
                        if no_map {
                            continue;
                        }
                        // The firmware prefix is already in the reserved table;
                        // only the part above it is new. Every caller passes a
                        // page-aligned firmware end, so the clamp keeps
                        // alignment.
                        let clip_start = clip_start.max(self.firmware_reserve_end);
                        if clip_start < clip_end {
                            push_span(
                                &mut reserved,
                                &mut reserved_n,
                                clip_start,
                                clip_end - clip_start,
                                DtbMemMapError::TooManyReservedRegions,
                            )?;
                        }
                    }
                }
            }
        }

        // SAFETY: single-hart boot face; the statics are written here once and
        // only read afterwards through the returned `'static` slices.
        unsafe {
            DTB_FREE = free;
            DTB_RESERVED = reserved;
            DTB_SOURCES = sources;
        }
        DTB_FREE_N.store(free_n, Ordering::Release);
        DTB_RESERVED_N.store(reserved_n, Ordering::Release);
        DTB_SOURCES_N.store(sources_n, Ordering::Release);
        // SAFETY: all three static arrays hold `n` initialised entries; on the
        // single-hart boot face the read only happens after the write.
        Ok(unsafe {
            (
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(DTB_FREE) as *const MemoryRegion,
                    free_n,
                ),
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(DTB_RESERVED) as *const MemoryRegion,
                    reserved_n,
                ),
                core::slice::from_raw_parts(
                    core::ptr::addr_of!(DTB_SOURCES) as *const SourceRecord,
                    sources_n,
                ),
            )
        })
    }

    /// `/reserved-memory`, distinguishing "absent" from "present but
    /// unreachable": upstream path lookup is known to miss nodes in some
    /// firmware-rebuilt blobs, and reading an unreachable reservation node as
    /// absent would hand its pages to the allocator.
    fn lookup_reserved_memory(
        &self,
    ) -> Result<Option<fdt::node::FdtNode<'_, '_>>, DtbMemMapError> {
        if let Some(node) = self.fdt.find_node("/reserved-memory") {
            return Ok(Some(node));
        }
        let visible = self
            .fdt
            .all_nodes()
            .any(|n| n.name.split('@').next() == Some("reserved-memory"));
        if visible {
            return Err(DtbMemMapError::ReservedMemoryUnreachable);
        }
        Ok(None)
    }
}

/// Out-of-bounds gate for the aarch64 UEFI chain's DTB descriptor source (a
/// pure function, host-testable): every conventional span of the firmware
/// `GetMemoryMap` snapshot must be fully covered by the DTB `/memory` panorama
/// (produced by `DtbMemoryMap` with `firmware_reserve_end=0` = no clipping).
/// Out of bounds = the firmware snapshot contradicts platform truth (missing
/// accounting for self-occupied areas, or firmware lying); the caller then
/// fails fast (while the shim can still print). Returns `Err(base, len)` of the
/// offending span.
///
/// The v1 semantics are gate plus source printing, not source replacement — the
/// DTB `/memory` is the DRAM panorama (including the LOADER_DATA pages the shim
/// occupies itself), so using it directly as the conventional source would hand
/// those self-occupied pages to the kernel PMM (the self-allocation accounting
/// risk already on record); replacing the source awaits a separate accounting
/// decision.
pub fn dtb_covers_conventional(
    dtb_free: &[MemoryRegion],
    conventional: &[MemoryRegion],
) -> Result<(), (u64, usize)> {
    for r in conventional {
        let r_start = r.base.0;
        let r_end = r.base.0 + r.len as u64;
        let covered = dtb_free.iter().any(|d| {
            let d_start = d.base.0;
            let d_end = d.base.0 + d.len as u64;
            d_start <= r_start && r_end <= d_end
        });
        if !covered {
            return Err((r.base.0, r.len));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, vec, vec::Vec};

    /// One `/reserved-memory` child, as the fixture writes it.
    struct ReservedRow {
        base: u64,
        size: u64,
        no_map: bool,
        reusable: bool,
    }

    /// Hand-build a minimal FDT blob in canonical form: header(40) + memrsv
    /// + structure block + strings. The root node carries
    /// `#address-cells=2`/`#size-cells=2`, and each `/memory` child node (with
    /// or without the unit-address variant) hangs a 128-bit `reg`.
    fn build_fdt(memory_regs: &[(u64, u64)], unit_address: bool) -> Vec<u8> {
        build_fdt_ex(memory_regs, unit_address, &[], &[], 2)
    }

    /// The same blob, plus a `/reserved-memory` node (one child per row, all
    /// under `reserved_cells` address/size cells) and the given memory
    /// reservation block entries.
    fn build_fdt_ex(
        memory_regs: &[(u64, u64)],
        unit_address: bool,
        reserved: &[ReservedRow],
        memrsv: &[(u64, u64)],
        reserved_cells: u32,
    ) -> Vec<u8> {
        build_fdt_full(memory_regs, unit_address, reserved, memrsv, reserved_cells, &[], &[], 0, false)
    }

    /// The same blob with an explicit `ranges` value on `/reserved-memory`.
    fn build_fdt_full(
        memory_regs: &[(u64, u64)],
        unit_address: bool,
        reserved: &[ReservedRow],
        memrsv: &[(u64, u64)],
        reserved_cells: u32,
        ranges: &[u8],
        marker_value: &[u8],
        memory_status: u8,
        hotplug: bool,
    ) -> Vec<u8> {
        build_fdt_full_opts(
            memory_regs,
            unit_address,
            reserved,
            memrsv,
            Some(reserved_cells),
            Some(ranges),
            marker_value,
            memory_status,
            hotplug,
            true,
            &[],
        )
    }

    /// The same builder with optional `/reserved-memory` cells and `ranges`,
    /// so a test can omit either property entirely. `emit_reg` and `extra`
    /// shape the child itself: `ReservedRow` cannot express a `size`-only
    /// dynamic request, nor `reg` together with `size`.
    fn build_fdt_full_opts(
        memory_regs: &[(u64, u64)],
        unit_address: bool,
        reserved: &[ReservedRow],
        memrsv: &[(u64, u64)],
        reserved_cells: Option<u32>,
        ranges: Option<&[u8]>,
        marker_value: &[u8],
        memory_status: u8,
        hotplug: bool,
        emit_reg: bool,
        extra: &[(&str, &[u8])],
    ) -> Vec<u8> {
        let mut strings: Vec<u8> = Vec::new();
        strings.extend_from_slice(b"#address-cells\0");
        strings.extend_from_slice(b"#size-cells\0");
        let nameoff_cells = 0usize;
        let nameoff_size = 15usize; // "#address-cells\0" is exactly 15 bytes
        strings.extend_from_slice(b"reg\0");
        let nameoff_reg = 27usize; // 15 + 12 ("#size-cells\0" is exactly 12 bytes)
        strings.extend_from_slice(b"no-map\0");
        let nameoff_no_map = 31usize; // 27 + 4 ("reg\0" is exactly 4 bytes)
        strings.extend_from_slice(b"ranges\0");
        let nameoff_ranges = 38usize; // 31 + 7 ("no-map\0" is exactly 7 bytes)
        strings.extend_from_slice(b"reusable\0");
        let nameoff_reusable = 45usize; // 38 + 7 ("ranges\0" is exactly 7 bytes)
        strings.extend_from_slice(b"status\0");
        let nameoff_status = 54usize; // 45 + 9 ("reusable\0" is exactly 9 bytes)
        strings.extend_from_slice(b"hotpluggable\0");
        let nameoff_hotplug = 61usize; // 54 + 7 ("status\0" is exactly 7 bytes)
        // Test-only shapes (`size`, `alignment`) are not part of the fixed
        // table, so their names are appended and referenced by offset.
        let mut extra_offsets: Vec<u32> = Vec::with_capacity(extra.len());
        for (name, _) in extra {
            extra_offsets.push(strings.len() as u32);
            strings.extend_from_slice(name.as_bytes());
            strings.push(0);
        }
        let mut struct_blk: Vec<u8> = Vec::new();
        let push_u32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_be_bytes());
        let push_prop = |out: &mut Vec<u8>, nameoff: u32, val: &[u8]| {
            push_u32(out, 3); // FDT_PROP
            push_u32(out, val.len() as u32);
            push_u32(out, nameoff as u32);
            out.extend_from_slice(val);
            while out.len() % 4 != 0 { out.push(0); }
        };
        push_u32(&mut struct_blk, 1); // FDT_BEGIN_NODE (root, empty name)
        struct_blk.push(0);
        while struct_blk.len() % 4 != 0 { struct_blk.push(0); }
        push_prop(&mut struct_blk, nameoff_cells as u32, &2u32.to_be_bytes());
        push_prop(&mut struct_blk, nameoff_size as u32, &2u32.to_be_bytes());
        for (base, size) in memory_regs {
            push_u32(&mut struct_blk, 1); // FDT_BEGIN_NODE
            let name: &str = if unit_address {
                &format!("memory@{base:x}\u{0}")
            } else {
                "memory\u{0}"
            };
            struct_blk.extend_from_slice(name.as_bytes());
            while struct_blk.len() % 4 != 0 { struct_blk.push(0); }
            // The root declares cells 2/2, so reg is a full 128-bit pair
            // [hi(base), lo(base), hi(size), lo(size)]; for values below 2^32 the
            // high words are zero, exactly as the earlier hand-written encoding.
            // A lesson from real hardware: a short 1/1 encoding is truncated under
            // 2/2 semantics (size=None) → the region is silently skipped.
            let mut reg: Vec<u8> = Vec::new();
            reg.extend_from_slice(&((*base >> 32) as u32).to_be_bytes());
            reg.extend_from_slice(&(*base as u32).to_be_bytes());
            reg.extend_from_slice(&((*size >> 32) as u32).to_be_bytes());
            reg.extend_from_slice(&(*size as u32).to_be_bytes());
            push_prop(&mut struct_blk, nameoff_reg as u32, &reg);
            let status: &[u8] = match memory_status {
                0 => b"",
                1 => b"disabled\0",
                _ => b"reserved\0",
            };
            if !status.is_empty() {
                push_prop(&mut struct_blk, nameoff_status as u32, status);
            }
            if hotplug {
                push_prop(&mut struct_blk, nameoff_hotplug as u32, &[]);
            }
            push_u32(&mut struct_blk, 2); // FDT_END_NODE
        }
        if !reserved.is_empty() {
            push_u32(&mut struct_blk, 1); // FDT_BEGIN_NODE
            struct_blk.extend_from_slice(b"reserved-memory\0");
            while struct_blk.len() % 4 != 0 { struct_blk.push(0); }
            if let Some(cells) = reserved_cells {
                push_prop(&mut struct_blk, nameoff_cells as u32, &cells.to_be_bytes());
                push_prop(&mut struct_blk, nameoff_size as u32, &cells.to_be_bytes());
            }
            if let Some(ranges) = ranges {
                push_prop(&mut struct_blk, nameoff_ranges as u32, ranges);
            }
            let cells_for_reg = reserved_cells.unwrap_or(2);
            let encode_cells = |out: &mut Vec<u8>, value: u64, cells: u32| {
                for i in (0..cells).rev() {
                    let word = match i {
                        0 => value as u32,
                        1 => (value >> 32) as u32,
                        _ => 0,
                    };
                    out.extend_from_slice(&word.to_be_bytes());
                }
            };
            for row in reserved {
                push_u32(&mut struct_blk, 1); // FDT_BEGIN_NODE
                struct_blk.extend_from_slice(format!("region@{:x}\u{0}", row.base).as_bytes());
                while struct_blk.len() % 4 != 0 { struct_blk.push(0); }
                if emit_reg {
                    let mut reg: Vec<u8> = Vec::new();
                    encode_cells(&mut reg, row.base, cells_for_reg);
                    encode_cells(&mut reg, row.size, cells_for_reg);
                    push_prop(&mut struct_blk, nameoff_reg as u32, &reg);
                }
                if row.no_map {
                    push_prop(&mut struct_blk, nameoff_no_map as u32, marker_value);
                }
                if row.reusable {
                    push_prop(&mut struct_blk, nameoff_reusable as u32, marker_value);
                }
                for (index, (_, value)) in extra.iter().enumerate() {
                    push_prop(&mut struct_blk, extra_offsets[index], value);
                }
                push_u32(&mut struct_blk, 2); // FDT_END_NODE
            }
            push_u32(&mut struct_blk, 2); // FDT_END_NODE
        }
        push_u32(&mut struct_blk, 2); // FDT_END_NODE (root)
        push_u32(&mut struct_blk, 9); // FDT_END

        let struct_off = 40u32 + 16u32 * (memrsv.len() as u32 + 1);
        let strings_off = struct_off + struct_blk.len() as u32;
        let mut out: Vec<u8> = Vec::new();
        let push = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_be_bytes());
        push(&mut out, 0xd00d_feed);          // magic
        push(&mut out, 0);                    // totalsize (patched in later)
        push(&mut out, struct_off);
        push(&mut out, strings_off);
        push(&mut out, 40);                   // off_memrsv
        push(&mut out, 17);                   // version
        push(&mut out, 16);                   // last_comp_version
        push(&mut out, 0);                    // boot_cpuid
        push(&mut out, strings.len() as u32); // size_strings
        push(&mut out, struct_blk.len() as u32); // size_struct
        for (base, size) in memrsv {
            push(&mut out, (*base >> 32) as u32);
            push(&mut out, *base as u32);
            push(&mut out, (*size >> 32) as u32);
            push(&mut out, *size as u32);
        }
        out.extend_from_slice(&[0u8; 16]);    // memrsv terminator
        out.extend_from_slice(&struct_blk);
        out.extend_from_slice(&strings);
        while out.len() % 4 != 0 { out.push(0); }
        let total = out.len() as u32;
        out[4..8].copy_from_slice(&total.to_be_bytes());
        out
    }

    #[test]
    fn dtb_single_memory_region_fw_end_zero() {
        let blob = build_fdt(&[(0x8000_0000, 0x4000_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("规范形 stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect().expect("test map fits DTB_MEMMAP_MAX");
        assert_eq!(conv.len(), 1);
        assert_eq!(conv[0].base.0, 0x8000_0000);
        assert_eq!(conv[0].len, 0x4000_0000 as usize);
        assert!(res.is_empty(), "fw_end=0 不产生 reserved");
    }

    #[test]
    fn dtb_more_than_capacity_is_rejected_not_truncated() {
        // One span past the capacity: the collector must refuse the map rather
        // than hand back a table that silently lacks a real RAM range.
        let mut regions = [(0u64, 0u64); DTB_MEMMAP_MAX + 1];
        for (i, region) in regions.iter_mut().enumerate() {
            *region = (0x8000_0000 + i as u64 * 0x2000, 0x1000);
        }
        let blob = build_fdt(&regions, false);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::TooManyFreeRegions)
        ));
    }

    #[test]
    fn dtb_declared_reservation_is_never_conventional() {
        // A plain declaration (no `no-map`) is carved out of the free table
        // and stays reachable through the reserved table.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect().expect("map fits");
        assert_eq!(conv.len(), 2, "the hole splits the span in two");
        assert_eq!((conv[0].base.0, conv[0].len), (0x8000_0000, 0x4000));
        assert_eq!((conv[1].base.0, conv[1].len), (0x8000_5000, 0xb000));
        assert_eq!(res.len(), 1);
        assert_eq!((res[0].base.0, res[0].len), (0x8000_4000, 0x1000));
    }

    #[test]
    fn dtb_no_map_reservation_leaves_both_tables() {
        // `no-map` forbids access itself, so the pages are neither allocatable
        // nor reachable through the reserved (direct-mapped) table.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: true, reusable: false }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect().expect("map fits");
        assert_eq!(conv.len(), 2);
        assert!(res.is_empty(), "no-map ranges stay out of both tables");
    }

    #[test]
    fn dtb_no_map_reservation_keeps_a_source_record() {
        // The range leaves both projection tables, so the record is the only
        // output that still says where it was and that it was `no-map`.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: true, reusable: false }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (_, _, sources) =
            DtbMemoryMap::new(&fdt, 0).collect_with_sources().expect("map fits");
        let record = sources
            .iter()
            .find(|r| r.kind == SourceKind::ReservedNoMap)
            .expect("no-map declaration keeps its record");
        assert_eq!(
            (record.range.start, record.range.end_exclusive),
            (0x8000_4000, 0x8000_5000)
        );
        assert_eq!(record.origin, SourceOrigin::ReservedMemoryChild { node: 0, reg: 0 });
    }

    #[test]
    fn dtb_memory_reservation_record_keeps_the_raw_range() {
        // An unaligned MRB entry rounds outward for the projections but the
        // record has to keep the bytes the firmware actually reserved.
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &[], &[(0x8000_1000, 0x800)], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (_, _, sources) =
            DtbMemoryMap::new(&fdt, 0).collect_with_sources().expect("map fits");
        let record = sources
            .iter()
            .find(|r| r.kind == SourceKind::MrbEntry)
            .expect("MRB entry keeps its record");
        assert_eq!(
            (record.range.start, record.range.end_exclusive),
            (0x8000_1000, 0x8000_1800)
        );
        assert_eq!(record.origin, SourceOrigin::MrbEntry(0));
    }

    #[test]
    fn dtb_reservation_outside_memory_keeps_its_record() {
        // A framebuffer-style reservation outside `/memory` claims no RAM, so
        // both tables stay untouched — but the declaration itself must not
        // vanish from the input record.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x9000_0000, size: 0x1000, no_map: true, reusable: false }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (conv, res, sources) =
            DtbMemoryMap::new(&fdt, 0).collect_with_sources().expect("map fits");
        assert_eq!(conv.len(), 1);
        assert!(res.is_empty());
        let record = sources
            .iter()
            .find(|r| r.kind == SourceKind::ReservedNoMap)
            .expect("out-of-RAM declaration keeps its record");
        assert_eq!(
            (record.range.start, record.range.end_exclusive),
            (0x9000_0000, 0x9000_1000)
        );
    }

    #[test]
    fn dtb_source_records_cover_every_projected_span() {
        // Consistency between the record layer and the projections: every
        // reserved span comes from a record page range, and every RAM byte in
        // the conventional table is inside some `/memory` record.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_8000, size: 0x1000, no_map: false, reusable: false }],
            &[(0x8000_4000, 0x1000)],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (conv, res, sources) =
            DtbMemoryMap::new(&fdt, 0).collect_with_sources().expect("map fits");
        assert_eq!(sources.len(), 3, "one /memory span + one MRB entry + one reservation");
        let covered = |start: u64| {
            sources.iter().any(|r| {
                let first = (r.range.start & !0xfff) as u128;
                let end = (r.range.end_exclusive + 0xfff) & !0xfffu128;
                first <= start as u128 && (start as u128) < end
            })
        };
        for span in conv.iter().chain(res.iter()) {
            assert!(covered(span.base.0), "span at {:#x} has no source record", span.base.0);
        }
    }

    #[test]
    fn dtb_collect_and_collect_with_sources_agree() {
        // Both entry points run the same collection; the two-table view must
        // not drift from the record view.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_8000, size: 0x1000, no_map: true, reusable: false }],
            &[(0x8000_4000, 0x1000)],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (conv_a, res_a) = DtbMemoryMap::new(&fdt, 0).collect().expect("map fits");
        let conv_a: Vec<(u64, usize)> = conv_a.iter().map(|r| (r.base.0, r.len)).collect();
        let res_a: Vec<(u64, usize)> = res_a.iter().map(|r| (r.base.0, r.len)).collect();
        let (conv_b, res_b, _) =
            DtbMemoryMap::new(&fdt, 0).collect_with_sources().expect("map fits");
        let conv_b: Vec<(u64, usize)> = conv_b.iter().map(|r| (r.base.0, r.len)).collect();
        let res_b: Vec<(u64, usize)> = res_b.iter().map(|r| (r.base.0, r.len)).collect();
        assert_eq!(conv_a, conv_b);
        assert_eq!(res_a, res_b);
    }

    #[test]
    fn dtb_no_map_declarations_count_against_the_declaration_cap() {
        // These `no-map` ranges lie outside `/memory`: they enter neither table
        // and carve nothing, so the projection tables never grow. They are
        // still declarations, and the ninth must be refused instead of being
        // silently dropped — the reason the record capacity follows the input
        // caps, not the tables.
        let rows: Vec<ReservedRow> = (0..=DTB_MEMMAP_MAX)
            .map(|i| ReservedRow {
                base: 0x9000_0000 + i as u64 * 0x2000,
                size: 0x1000,
                no_map: true,
                reusable: false,
            })
            .collect();
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &rows, &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let result = DtbMemoryMap::new(&fdt, 0).collect().map(|_| ());
        assert!(
            matches!(result, Err(DtbMemMapError::TooManyReservedRegions)),
            "expected the declaration cap, got {result:?}"
        );
    }

    #[test]
    fn dtb_declaration_cap_is_shared_across_reservation_kinds() {
        // Ordinary, `reusable` and `no-map` declarations draw from one cap:
        // three of each (nine total) must be refused even though no single kind
        // reaches eight.
        let rows: Vec<ReservedRow> = (0..3)
            .flat_map(|i| {
                let base = 0x9000_0000 + i as u64 * 0x1_0000;
                [
                    ReservedRow { base, size: 0x1000, no_map: false, reusable: false },
                    ReservedRow { base: base + 0x2000, size: 0x1000, no_map: false, reusable: true },
                    ReservedRow { base: base + 0x4000, size: 0x1000, no_map: true, reusable: false },
                ]
            })
            .collect();
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &rows, &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let result = DtbMemoryMap::new(&fdt, 0).collect().map(|_| ());
        assert!(
            matches!(result, Err(DtbMemMapError::TooManyReservedRegions)),
            "expected one shared cap, got {result:?}"
        );
    }

    #[test]
    fn dtb_source_capacity_is_reachable_at_exactly_forty() {
        // 8 `/memory` spans + 8 prefixes (fw_end covers every span start) +
        // 16 MRB entries + 8 declarations = 40 records, exactly the buffer.
        let memory: Vec<(u64, u64)> =
            (0..DTB_MEMMAP_MAX).map(|i| (0x8000_0000 + i as u64 * 0x1000, 0x1000)).collect();
        let memrsv: Vec<(u64, u64)> =
            (0..DTB_MRB_MAX).map(|i| (0x9000_0000 + i as u64 * 0x2000, 0x1000)).collect();
        let rows: Vec<ReservedRow> = (0..DTB_MEMMAP_MAX)
            .map(|i| ReservedRow {
                base: 0xA000_0000 + i as u64 * 0x2000,
                size: 0x1000,
                no_map: false,
                reusable: false,
            })
            .collect();
        let blob = build_fdt_ex(&memory, true, &rows, &memrsv, 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (_, _, sources) = DtbMemoryMap::new(&fdt, 0x8000_8000)
            .collect_with_sources()
            .expect("capacity is exactly enough");
        assert_eq!(sources.len(), DTB_SOURCE_MAX);
        let count = |kind: SourceKind| sources.iter().filter(|r| r.kind == kind).count();
        assert_eq!(count(SourceKind::Memory), DTB_MEMMAP_MAX);
        assert_eq!(count(SourceKind::FirmwarePrefix), DTB_MEMMAP_MAX);
        assert_eq!(count(SourceKind::MrbEntry), DTB_MRB_MAX);
        assert_eq!(count(SourceKind::ReservedOrdinary), DTB_MEMMAP_MAX);
    }

    #[test]
    fn dtb_firmware_prefix_links_to_its_memory_record() {
        // The prefix is derived, not declared: its origin must name the
        // `/memory` record it was cut from.
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &[], &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (_, _, sources) =
            DtbMemoryMap::new(&fdt, 0x8000_4000).collect_with_sources().expect("map fits");
        let prefix = sources
            .iter()
            .find(|r| r.kind == SourceKind::FirmwarePrefix)
            .expect("prefix record exists");
        assert_eq!(prefix.kind.class(), SourceClass::DerivedReservation);
        let SourceOrigin::FirmwarePrefix { memory_id, cut } = prefix.origin else {
            panic!("prefix origin must name its source record");
        };
        assert_eq!(cut, 0x8000_4000, "the derivation parameter stays recorded");
        let memory = sources
            .iter()
            .find(|r| r.id == memory_id)
            .expect("prefix points at a live record");
        assert_eq!(memory.kind, SourceKind::Memory);
        assert_eq!(memory.range.start, prefix.range.start);
        assert_eq!(memory.kind.class(), SourceClass::InputDeclaration);
    }

    /// Blob whose root holds `/memory` nodes carrying **several `reg` tuples
    /// in one node** (the row-based builders emit exactly one per node). Root
    /// cells are 2/2, memory-reservation block is the empty terminator.
    fn build_fdt_multi_reg(memory_nodes: &[(&str, &[(u64, u64)])]) -> Vec<u8> {
        fn add(strings: &mut Vec<u8>, s: &str) -> u32 {
            let off = strings.len() as u32;
            strings.extend_from_slice(s.as_bytes());
            strings.push(0);
            off
        }
        let mut strings: Vec<u8> = Vec::new();
        let cells_off = add(&mut strings, "#address-cells");
        let size_off = add(&mut strings, "#size-cells");
        let reg_off = add(&mut strings, "reg");
        let mut blk: Vec<u8> = Vec::new();
        let push_u32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_be_bytes());
        let push_prop = |out: &mut Vec<u8>, nameoff: u32, val: &[u8]| {
            push_u32(out, 3); // FDT_PROP
            push_u32(out, val.len() as u32);
            push_u32(out, nameoff);
            out.extend_from_slice(val);
            while out.len() % 4 != 0 {
                out.push(0);
            }
        };
        push_u32(&mut blk, 1); // root
        blk.push(0);
        while blk.len() % 4 != 0 {
            blk.push(0);
        }
        push_prop(&mut blk, cells_off, &2u32.to_be_bytes());
        push_prop(&mut blk, size_off, &2u32.to_be_bytes());
        for (name, regs) in memory_nodes {
            push_u32(&mut blk, 1);
            blk.extend_from_slice(name.as_bytes());
            blk.push(0);
            while blk.len() % 4 != 0 {
                blk.push(0);
            }
            let mut reg: Vec<u8> = Vec::new();
            for (base, size) in *regs {
                reg.extend_from_slice(&0u32.to_be_bytes());
                reg.extend_from_slice(&(*base as u32).to_be_bytes());
                reg.extend_from_slice(&0u32.to_be_bytes());
                reg.extend_from_slice(&(*size as u32).to_be_bytes());
            }
            push_prop(&mut blk, reg_off, &reg);
            push_u32(&mut blk, 2); // FDT_END_NODE
        }
        push_u32(&mut blk, 2); // root end
        push_u32(&mut blk, 9); // FDT_END

        let struct_off = 56u32;
        let strings_off = struct_off + blk.len() as u32;
        let total = strings_off + strings.len() as u32;
        let mut out: Vec<u8> = vec![0u8; total as usize];
        out[0..4].copy_from_slice(&0xd00d_feedu32.to_be_bytes());
        out[4..8].copy_from_slice(&total.to_be_bytes());
        out[8..12].copy_from_slice(&struct_off.to_be_bytes());
        out[12..16].copy_from_slice(&strings_off.to_be_bytes());
        out[16..20].copy_from_slice(&40u32.to_be_bytes()); // off_mem_rsvmap
        out[20..24].copy_from_slice(&17u32.to_be_bytes()); // version
        out[24..28].copy_from_slice(&16u32.to_be_bytes()); // last_comp_version
        out[32..36].copy_from_slice(&(strings.len() as u32).to_be_bytes());
        out[36..40].copy_from_slice(&(blk.len() as u32).to_be_bytes());
        out[struct_off as usize..strings_off as usize].copy_from_slice(&blk);
        out[strings_off as usize..].copy_from_slice(&strings);
        out
    }

    #[test]
    fn dtb_identical_memory_tuples_are_deduplicated_not_rejected() {
        // One node, two identical `reg` tuples: the panorama holds the span
        // once (a second copy would let the PMM hand the same page out twice)
        // while both declarations keep their records and their origins.
        let blob = build_fdt_multi_reg(&[(
            "memory@80000000",
            &[(0x8000_0000, 0x4000), (0x8000_0000, 0x4000)],
        )]);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (free, _, sources) = DtbMemoryMap::new(&fdt, 0)
            .collect_with_sources()
            .expect("an exact duplicate is not an overlap");
        assert_eq!(free.len(), 1, "the projection holds the span once");
        let records: Vec<_> = sources.iter().filter(|r| r.kind == SourceKind::Memory).collect();
        assert_eq!(records.len(), 2, "both declarations keep their record");
        assert_eq!(records[0].origin, SourceOrigin::MemoryNode { node: 0, reg: 0 });
        assert_eq!(records[1].origin, SourceOrigin::MemoryNode { node: 0, reg: 1 });
        assert_eq!(records[0].range.start, records[1].range.start);
    }

    #[test]
    fn dtb_identical_reservations_are_deduplicated_not_rejected() {
        // Two children declaring the same range with the same intent: one
        // projection, two source identities.
        let rows = [
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false },
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false },
        ];
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &rows, &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (_, reserved, sources) =
            DtbMemoryMap::new(&fdt, 0).collect_with_sources().expect("duplicate is not an overlap");
        assert_eq!(reserved.len(), 1, "the projection holds the range once");
        let records: Vec<_> = sources
            .iter()
            .filter(|r| r.kind == SourceKind::ReservedOrdinary)
            .collect();
        assert_eq!(records.len(), 2, "both declarations keep their record");
        assert_ne!(records[0].id, records[1].id);
        assert_eq!(records[0].range.start, records[1].range.start);
    }

    #[test]
    fn dtb_reusable_duplicates_are_deduplicated_not_rejected() {
        // `reusable` repeats project once but keep both identities; the owner
        // relation of each declaration is future work and must not be merged
        // away here.
        let rows = [
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: true },
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: true },
        ];
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &rows, &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (_, reserved, sources) =
            DtbMemoryMap::new(&fdt, 0).collect_with_sources().expect("same intent is a duplicate");
        assert_eq!(reserved.len(), 1);
        assert_eq!(
            sources.iter().filter(|r| r.kind == SourceKind::ReservedReusable).count(),
            2
        );
    }

    #[test]
    fn dtb_ordinary_and_reusable_same_range_is_a_conflict() {
        // Both leave the PMM today, but `reusable` carries an owner/reclaim
        // semantics the ordinary declaration does not: same bytes, different
        // intent, so the pair is a conflict rather than a duplicate.
        let rows = [
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false },
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: true },
        ];
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &rows, &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let result = DtbMemoryMap::new(&fdt, 0).collect().map(|_| ());
        assert!(
            matches!(result, Err(DtbMemMapError::ConflictingReservationAttributes)),
            "expected an intent conflict, got {result:?}"
        );
    }

    #[test]
    fn dtb_reusable_and_no_map_same_range_is_a_conflict() {
        let rows = [
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: true },
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: true, reusable: false },
        ];
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &rows, &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let result = DtbMemoryMap::new(&fdt, 0).collect().map(|_| ());
        assert!(
            matches!(result, Err(DtbMemMapError::ConflictingReservationAttributes)),
            "expected an intent conflict, got {result:?}"
        );
    }

    #[test]
    fn dtb_partial_overlap_of_memory_ranges_is_rejected() {
        // A partial overlap would put the same page in the panorama twice and
        // let the PMM hand it out twice; exact duplicates are deduplicated,
        // partial overlaps are refused.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x2000), (0x8000_1000, 0x1000)],
            true,
            &[],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let result = DtbMemoryMap::new(&fdt, 0).collect().map(|_| ());
        assert!(
            matches!(result, Err(DtbMemMapError::OverlappingMemoryRanges)),
            "expected the RAM overlap refusal, got {result:?}"
        );
    }

    #[test]
    fn dtb_identical_mrb_entries_are_rejected() {
        // DTSpec 5.3.2: "These given regions shall not overlap each other."
        // An identical pair is an overlap, not a repeatable declaration, so it
        // is refused rather than deduplicated.
        let blob =
            build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &[], &[(0x8000_4000, 0x1000), (0x8000_4000, 0x1000)], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let result = DtbMemoryMap::new(&fdt, 0).collect().map(|_| ());
        assert!(
            matches!(result, Err(DtbMemMapError::OverlappingMemoryReservations)),
            "expected the overlap refusal, got {result:?}"
        );
    }

    #[test]
    fn dtb_same_range_with_different_semantics_is_a_conflict() {
        // Same bytes, one `no-map` and one not: the two claims cannot both be
        // honoured, so this is a conflict rather than a duplicate.
        let rows = [
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false },
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: true, reusable: false },
        ];
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &rows, &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let result = DtbMemoryMap::new(&fdt, 0).collect().map(|_| ());
        assert!(
            matches!(result, Err(DtbMemMapError::ConflictingReservationAttributes)),
            "expected a semantic conflict, got {result:?}"
        );
    }

    #[test]
    fn dtb_every_input_declaration_has_exactly_one_record() {
        // The reverse direction of the coverage test: every accepted input
        // tuple produces exactly one record, and no record is invented.
        let memory = [(0x8000_0000u64, 0x1_0000u64), (0x9000_0000, 0x1_0000)];
        let rows = [
            ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false },
            ReservedRow { base: 0x8000_8000, size: 0x1000, no_map: true, reusable: false },
        ];
        let memrsv = [(0x8000_6000u64, 0x1000u64)];
        let blob = build_fdt_ex(&memory, false, &rows, &memrsv, 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (_, _, sources) =
            DtbMemoryMap::new(&fdt, 0).collect_with_sources().expect("map fits");
        assert_eq!(sources.len(), 5, "2 memory + 1 MRB + 2 reservations");
        let mut origins: Vec<_> = sources
            .iter()
            .filter(|r| r.kind.class() == SourceClass::InputDeclaration)
            .map(|r| r.origin)
            .collect();
        origins.sort_by_key(|o| match o {
            SourceOrigin::MemoryNode { node, reg } => (0u8, *node as u32, *reg as u32),
            SourceOrigin::MrbEntry(e) => (1u8, *e as u32, 0),
            SourceOrigin::ReservedMemoryChild { node, reg } => (2u8, *node as u32, *reg as u32),
            SourceOrigin::FirmwarePrefix { .. } => (3u8, 0, 0),
        });
        assert_eq!(
            origins,
            vec![
                SourceOrigin::MemoryNode { node: 0, reg: 0 },
                SourceOrigin::MemoryNode { node: 1, reg: 0 },
                SourceOrigin::MrbEntry(0),
                SourceOrigin::ReservedMemoryChild { node: 0, reg: 0 },
                SourceOrigin::ReservedMemoryChild { node: 1, reg: 0 },
            ]
        );
    }

    #[test]
    fn dtb_reservation_inside_the_firmware_prefix_is_not_duplicated() {
        // OpenSBI describes its own resident area both as the prefix this
        // collector clips and as a `/reserved-memory` child; the declaration
        // must not append a second entry for pages already reserved.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_0000, size: 0x4000, no_map: false, reusable: false }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0x8000_4000).collect().expect("map fits");
        assert_eq!(conv.len(), 1);
        assert_eq!((conv[0].base.0, conv[0].len), (0x8000_4000, 0xc000));
        assert_eq!(res.len(), 1, "the prefix entry alone");
        assert_eq!((res[0].base.0, res[0].len), (0x8000_0000, 0x4000));
    }

    #[test]
    fn dtb_reservation_outside_memory_claims_no_ram() {
        // A framebuffer-style reservation outside `/memory` must not appear as
        // reachable memory, because the kernel does not map it as RAM.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x1_0000_0000, size: 0x1000, no_map: false, reusable: false }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect().expect("map fits");
        assert_eq!(conv.len(), 1);
        assert_eq!((conv[0].base.0, conv[0].len), (0x8000_0000, 0x1_0000));
        assert!(res.is_empty());
    }

    #[test]
    fn dtb_memory_reservation_block_is_carved_from_free() {
        // The reservation block forbids access, so its ranges must reach
        // neither the conventional nor the reserved table.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[],
            &[(0x8000_8000, 0x1000)],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect().expect("map fits");
        assert_eq!(conv.len(), 2);
        assert_eq!((conv[0].base.0, conv[0].len), (0x8000_0000, 0x8000));
        assert_eq!((conv[1].base.0, conv[1].len), (0x8000_9000, 0x7000));
        assert!(res.is_empty());
    }

    #[test]
    fn dtb_overlapping_reservations_are_rejected() {
        // The spec forbids overlapping children, and the overlap is not benign
        // when only one side reaches the reserved table: a plain declaration
        // over a `no-map` one would authorise access to forbidden pages.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[
                ReservedRow { base: 0x8000_4000, size: 0x2000, no_map: false, reusable: false },
                ReservedRow { base: 0x8000_5000, size: 0x1000, no_map: true, reusable: false },
            ],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::OverlappingReservedRanges)
        ));
    }

    #[test]
    fn dtb_reservation_with_wrapping_end_is_rejected() {
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: u64::MAX - 0x10, size: 0x100, no_map: true, reusable: false }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::RangeOverflow)
        ));
    }

    #[test]
    fn physical_range_expresses_the_top_page() {
        // The record layer can carry the highest page; only the projection is
        // limited to a u64 end. This is the type-level contract the boundary
        // tests below depend on.
        let top = PhysicalRange::new(0xffff_ffff_ffff_f000, 1u128 << 64);
        assert_eq!(top.end_exclusive, 1u128 << 64);
        assert_eq!(top.len(), 0x1000);
        assert_eq!(top.projectable_end(), None);
        let ordinary = PhysicalRange::new(0x8000_0000, 0x8000_1000);
        assert_eq!(ordinary.projectable_end(), Some(0x8000_1000));
    }

    #[test]
    fn dtb_memory_range_ending_at_2_64_is_refused_as_unsupported_end() {
        // address + size == 2^64 exactly: a well-formed range that the u64
        // exclusive end cannot hold. Refusing it is this project's input
        // policy, so it must not arrive as the arithmetic-wrap error.
        let blob = build_fdt(&[(0xffff_ffff_ffff_f000, 0x1000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::UnsupportedPhysicalAddressEnd)
        ));
    }

    #[test]
    fn dtb_reservation_ending_at_2_64_is_refused_as_unsupported_end() {
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0xffff_ffff_ffff_f000, size: 0x1000, no_map: true, reusable: false }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::UnsupportedPhysicalAddressEnd)
        ));
    }

    #[test]
    fn dtb_top_partial_page_reservation_is_refused_as_unsupported_end() {
        // The declared range ends 1 byte into the highest page (below 2^64),
        // but rounding outward reaches 2^64 — the same boundary, found only
        // after quantization.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow {
                base: 0xffff_ffff_ffff_e000,
                size: 0x1001,
                no_map: true,
                reusable: false,
            }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::UnsupportedPhysicalAddressEnd)
        ));
    }

    #[test]
    fn dtb_memory_range_past_2_64_is_range_overflow() {
        // address + size > 2^64 is a malformed range, and must keep the
        // arithmetic name.
        let blob = build_fdt(&[(u64::MAX - 0x10, 0x100)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::RangeOverflow)
        ));
    }

    #[test]
    fn dtb_reserved_memory_with_wide_cells_is_rejected() {
        // `reg()` refuses cell counts above two; silently skipping the node
        // would hand its pages to the allocator.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false }],
            &[],
            3,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::MismatchedReservedMemoryCells)
        ));
    }

    #[test]
    fn dtb_missing_reserved_memory_ranges_is_rejected() {
        // The property is required by the specification: its absence is a
        // malformed input, distinct from a non-empty `ranges` (well-formed but
        // needing a translation this collector does not implement).
        let blob = build_fdt_full_opts(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false }],
            &[],
            Some(2),
            None,
            &[],
            0,
            false,
            true,
            &[],
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::MissingReservedMemoryRanges)
        ));
    }

    #[test]
    fn dtb_missing_reserved_memory_cells_is_rejected() {
        // `#address-cells` / `#size-cells` are required on the parent; without
        // them the child `reg` cannot be read at all.
        let blob = build_fdt_full_opts(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false }],
            &[],
            None,
            Some(&[]),
            &[],
            0,
            false,
            true,
            &[],
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::MissingReservedMemoryCells)
        ));
    }

    /// One `/reserved-memory` child with a caller-chosen property shape:
    /// `emit_reg` decides whether `reg` is written at all, and `extra` appends
    /// raw properties (`size`, `alignment`) that `ReservedRow` cannot express.
    fn build_fdt_child(extra: &[(&str, &[u8])], emit_reg: bool) -> Vec<u8> {
        build_fdt_full_opts(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false }],
            &[],
            Some(2),
            Some(&[]),
            &[],
            0,
            false,
            emit_reg,
            extra,
        )
    }

    #[test]
    fn dtb_size_only_reservation_is_rejected_as_dynamic() {
        // A child with `size` and no `reg` asks for a dynamically allocated
        // region. This boot has no allocator to place it, and ignoring the node
        // would let the PMM hand out pages a later consumer still expects to be
        // reserved — so it is refused, with an error naming the request rather
        // than the generic unreadable-extent one.
        let size = [0u32.to_be_bytes(), 0x2000u32.to_be_bytes()].concat();
        let blob = build_fdt_child(&[("size", &size)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::UnsupportedDynamicReservation)
        ));
    }

    #[test]
    fn dtb_child_without_reg_and_without_size_is_an_unsupported_declaration() {
        // The generic error still covers a child that states no extent at all:
        // separating the two shapes is what makes the dynamic case visible.
        let blob = build_fdt_child(&[], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::UnsupportedReservedDeclaration)
        ));
    }

    #[test]
    fn reg_and_size_together_uses_reg_and_ignores_size() {
        // A node with both is statically allocated: `reg` is authoritative and
        // `size` is ignored. The record must therefore carry the `reg` extent,
        // not the 0x2000 request, and the tables must match the `reg`-only blob.
        let size = [0u32.to_be_bytes(), 0x2000u32.to_be_bytes()].concat();
        let with_size = build_fdt_child(&[("size", &size)], true);
        let plain = build_fdt_child(&[], true);
        let fdt_a = fdt::Fdt::new(&with_size).expect("stub must parse");
        let fdt_b = fdt::Fdt::new(&plain).expect("stub must parse");
        let a = DtbMemoryMap::new(&fdt_a, 0)
            .collect_with_sources()
            .expect("static reservation that also carries size");
        let b = DtbMemoryMap::new(&fdt_b, 0)
            .collect_with_sources()
            .expect("same reservation without size");
        assert_eq!(a.0.len(), b.0.len());
        assert_eq!(a.1.len(), b.1.len());
        assert_eq!(a.2.len(), b.2.len());
        assert_eq!(
            (a.2[1].range.start, a.2[1].range.end_exclusive),
            (0x8000_4000, 0x8000_5000)
        );
    }

    #[test]
    fn dtb_carving_that_exceeds_capacity_is_rejected() {
        // Ten 4 KiB holes split one 128 KiB span into ten free pieces, more
        // than the table holds. Truncating here would merge the holes back
        // into RAM.
        let reservations: Vec<ReservedRow> = (0..10)
            .map(|i| ReservedRow {
                base: 0x8000_1000 + i * 0x2000,
                size: 0x1000,
                no_map: true,
                reusable: false,
            })
            .collect();
        let blob = build_fdt_ex(&[(0x8000_0000, 0x2_0000)], false, &reservations, &[], 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::TooManyFreeRegions)
        ));
    }

    #[test]
    fn dtb_reserved_memory_without_empty_ranges_is_rejected() {
        // Child addresses are physical only when the parent declares an empty
        // `ranges`; a non-empty value would need a translation we do not do.
        let blob = build_fdt_full(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: false, reusable: false }],
            &[],
            2,
            &[0u8; 16],
            &[],
            0,
            false,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::UnsupportedReservedMemoryRanges)
        ));
    }

    #[test]
    fn dtb_reservation_with_no_map_and_reusable_is_rejected() {
        // The two properties are mutually exclusive in the specification.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: true, reusable: true }],
            &[],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::ConflictingReservationAttributes)
        ));
    }

    #[test]
    fn dtb_reservation_with_nonempty_marker_is_rejected() {
        // `no-map` / `reusable` are empty properties; a value is a malformed
        // encoding and guessing its meaning could flip the mapping policy.
        let blob = build_fdt_full(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x8000_4000, size: 0x1000, no_map: true, reusable: false }],
            &[],
            2,
            &[],
            &[0u8; 4],
            0,
            false,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::NonEmptyReservationAttribute)
        ));
    }

    #[test]
    fn dtb_reserved_memory_cells_must_match_root() {
        // Child `reg` is read as physical as written; that needs the same cell
        // counts as the root, not merely a parsable pair of its own.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[ReservedRow { base: 0x4000, size: 0x1000, no_map: false, reusable: false }],
            &[],
            1,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::MismatchedReservedMemoryCells)
        ));
    }

    #[test]
    fn dtb_overlapping_memory_reservations_are_rejected() {
        // The specification requires the block's entries to be disjoint.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[],
            &[(0x8000_8000, 0x2000), (0x8000_9000, 0x1000)],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::OverlappingMemoryReservations)
        ));
    }

    #[test]
    fn dtb_too_many_memory_reservations_are_rejected() {
        // A missing `{0, 0}` terminator makes the library keep yielding pairs;
        // the cap refuses the shape instead of reading past the block.
        let mut entries = [(0u64, 0u64); DTB_MRB_MAX + 1];
        for (i, entry) in entries.iter_mut().enumerate() {
            *entry = (0x1_0000_0000 + i as u64 * 0x2000, 0x1000);
        }
        let blob = build_fdt_ex(&[(0x8000_0000, 0x1_0000)], false, &[], &entries, 2);
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::TooManyMemoryReservations)
        ));
    }

    #[test]
    fn dtb_disabled_memory_node_is_rejected() {
        // `status = "disabled"` says the node is not running; its range is not
        // usable RAM and must not reach the allocator.
        let blob = build_fdt_full(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[],
            &[],
            2,
            &[],
            &[],
            1,
            false,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::UnsupportedMemoryNodeStatus)
        ));
    }

    #[test]
    fn dtb_hotpluggable_memory_is_rejected() {
        // The kernel cannot remove memory; removable ranges must not become
        // ordinary allocations.
        let blob = build_fdt_full(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[],
            &[],
            2,
            &[],
            &[],
            0,
            true,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::HotpluggableMemoryUnsupported)
        ));
    }

    #[test]
    fn dtb_zero_length_memory_reservation_is_rejected() {
        // Only `{0, 0}` terminates the block; a zero-size entry with a real
        // address is malformed and must not be accepted as a reservation.
        let blob = build_fdt_ex(
            &[(0x8000_0000, 0x1_0000)],
            false,
            &[],
            &[(0x8000_8000, 0)],
            2,
        );
        let fdt = fdt::Fdt::new(&blob).expect("stub must parse");
        assert!(matches!(
            DtbMemoryMap::new(&fdt, 0).collect(),
            Err(DtbMemMapError::ZeroLengthMemoryReservation)
        ));
    }

    #[test]
    fn dtb_multi_region_and_unit_address_variant() {
        // The two spans must be disjoint: 0x8000_0000 + 0x4000_0000 would end
        // at 0xC000_0000 and swallow the second span — the old fixture was an
        // overlapping-RAM input that nothing checked before.
        let blob = build_fdt(&[(0x8000_0000, 0x400_0000), (0x8800_0000, 0x1000_0000)], true);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, _res) = DtbMemoryMap::new(&fdt, 0).collect().expect("test map fits DTB_MEMMAP_MAX");
        assert_eq!(conv.len(), 2, "两段 /memory → 2 条（含 unit-address 变体）");
        assert_eq!(conv[0].base.0, 0x8000_0000);
        assert_eq!(conv[1].base.0, 0x8800_0000);
    }

    #[test]
    fn dtb_fw_clip_moves_tail_to_reserved() {
        // /memory = [0x8000_0000, +64MiB), fw_end = 0x8020_0000 (2MiB OpenSBI)
        // → free = [0x8020_0000, +62MiB), reserved = [0x8000_0000, 2MiB)
        let blob = build_fdt(&[(0x8000_0000, 0x400_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0x8020_0000).collect().expect("test map fits DTB_MEMMAP_MAX");
        assert_eq!(conv.len(), 1);
        assert_eq!(conv[0].base.0, 0x8020_0000, "削界后 free 起点抬到 fw_end");
        assert_eq!(conv[0].len, (0x400_0000 - 0x20_0000) as usize);
        assert_eq!(res.len(), 1, "削掉的固件段进 reserved");
        assert_eq!(res[0].base.0, 0x8000_0000);
        assert_eq!(res[0].len, 0x20_0000 as usize);
    }

    #[test]
    fn dtb_fully_reserved_region_drops_from_free() {
        // /memory = [0x8000_0000, +1MiB) lies entirely below fw_end=0x8020_0000
        // → free is empty and reserved holds the whole span (a bootface-side
        // consumer seeing an empty table halts honestly).
        let blob = build_fdt(&[(0x8000_0000, 0x10_0000)], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0x8020_0000).collect().expect("test map fits DTB_MEMMAP_MAX");
        assert!(conv.is_empty(), "整段固件区 → free 空（诚实停机由消费方判）");
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn dtb_no_memory_node_yields_empty() {
        let blob = build_fdt(&[], false);
        let fdt = fdt::Fdt::new(&blob).expect("stub 必须可解析");
        let (conv, res) = DtbMemoryMap::new(&fdt, 0).collect().expect("test map fits DTB_MEMMAP_MAX");
        assert!(conv.is_empty(), "无 /memory → 空（诚实停机由消费方判）");
        assert!(res.is_empty());
    }

    #[test]
    fn dtb_gate_covers_contained_and_multi_region() {
        // both conventional spans fall inside /memory [0x4000_0000, +512MiB) → Ok
        let dtb = [MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x2000_0000 }];
        let conv = [
            MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x1000_0000 },
            MemoryRegion { base: PhysBytes(0x5000_0000), len: 0x800_0000 },
        ];
        assert_eq!(dtb_covers_conventional(&dtb, &conv), Ok(()));
    }

    #[test]
    fn dtb_gate_reports_out_of_bounds_region() {
        // an out-of-bounds span (conventional above the /memory upper bound) →
        // Err(base, len)
        let dtb = [MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x1000_0000 }];
        let conv = [
            MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x800_0000 },
            MemoryRegion { base: PhysBytes(0x5000_0000), len: 0x1000 },
        ];
        assert_eq!(
            dtb_covers_conventional(&dtb, &conv),
            Err((0x5000_0000, 0x1000))
        );
    }

    #[test]
    fn dtb_gate_rejects_partial_overlap() {
        // a straddling span (lower half inside /memory, upper half outside) does
        // not count as covered
        let dtb = [MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x800_0000 }];
        let conv = [MemoryRegion { base: PhysBytes(0x4040_0000), len: 0x800_0000 }];
        assert_eq!(
            dtb_covers_conventional(&dtb, &conv),
            Err((0x4040_0000, 0x800_0000))
        );
    }

    #[test]
    fn dtb_gate_empty_conventional_is_ok() {
        let dtb = [MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x1000 }];
        assert_eq!(dtb_covers_conventional(&dtb, &[]), Ok(()));
    }
}
