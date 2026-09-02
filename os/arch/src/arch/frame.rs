//! VM Bootstrap Memory Handoff.
//!
//! =====================================================================
//!
//! The kernel-side handoff of physical memory to the VM at boot. The
//! module owns three collaborating types and exposes two clearly
//! separated layers: **selection** (cut surviving slices from the
//! boot-shim memory map) and **allocation** (hand frames out to the
//! ELF loader).
//!
//! > **Reading order**: skim "What this module owns", then "How to use
//! > it". Invariants and C↔Rust alignment are reference material.
//!
//! ## What this module owns
//!
//! - [`VmBootRegion`] — a kernel-verified contiguous physical sub-range.
//!   Invariants: page-aligned `[start, end)`; disjoint from any reserved
//!   region the kernel has reported through [`VmBootRegion::select_multi`]'s
//!   `exclusions` argument.
//! - [`VmBootRegions<N>`] — a fixed-capacity, owned, **descending-sorted**
//!   (by `start`) list of regions. Storage is a fixed `[VmBootRegion; N]`
//!   — no `Vec`, no `Box`, no allocator calls. Default `N` =
//!   [`MAX_BOOT_REGIONS`].
//! - [`VmBootAllocator<N>`] — a one-shot bump allocator consuming a
//!   [`VmBootRegions<N>`]. Allocation-only, no `free()`. Per region
//!   it walks from `region.end` down to `region.start`; across regions
//!   it follows the list's descending order, so consecutively returned
//!   frames are monotonically descending in physical address.
//!
//! ## How to use it
//!
//! The kernel-side caller (boot, single-threaded, no PMM) follows three
//! steps:
//!
//! 1. **Select** surviving slices from the boot-shim memmap, excluding
//!    every region the kernel already owns:
//!
//!    ```ignore
//!    let regions = VmBootRegion::select_multi(
//!        kernel_info.memmap(),
//!        &[ /* kernel image */, /* every boot module */ ],
//!    )?;
//!    let regions = regions.expect("no free memory for VM bootstrap");
//!    ```
//!
//! 2. **Hand** the list to a [`VmBootAllocator`]; the loader then calls
//!    [`VmBootAllocator::alloc_page`] per ELF page (and per stack page):
//!
//!    ```ignore
//!    let mut vm_alloc = VmBootAllocator::new(regions);
//!    let frame = vm_alloc.alloc_page()?;
//!    ```
//!
//! 3. **Handoff** to VM init: `vm_alloc.regions()` returns the whole
//!    pool — VM's runtime PMM init reads it to learn its physical
//!    memory universe.
//!
//! ## C ↔ Rust alignment table
//!
//! | C | Rust |
//! |---|------|
//! | `pg_alloc_page(cbi)` ([pg_utils.c:138-160](file:///minix3/minix/kernel/arch/i386/pg_utils.c#L138-L160)) — walks memmap from `mmap_size-1` down, each entry consuming `mm_length -= I386_PAGE_SIZE` (the highest free page in the highest entry) | [`VmBootAllocator::alloc_page`] — per region returns the frame at `cursor - PAGE_SIZE`, where `cursor` starts at `region.end` and decreases; cross-region jumps follow the regions' descending order |
//! | `pg_map(PG_ALLOCATEME, ...)` ([pg_utils.c:267-310](file:///minix3/minix/kernel/arch/i386/pg_utils.c#L267-L310)) | [`load_vm_elf`](crate::arch::boot::load_vm_elf) passing `vm_alloc` |
//! | `cut_memmap(cbi, base, len)` ([pre_init.c:190-214](file:///minix3/minix/kernel/arch/i386/pre_init.c#L190-L214)) | [`VmBootRegion::select_multi`]'s `exclusions` parameter — the kernel-side caller builds the array of occupied ranges; `select_multi` performs the per-entry cuts |
//!
//! ## Why "from high address"
//!
//! Two reasons, both real (not just taste):
//!
//! 1. **Semantic alignment with C.** `PG_ALLOCATEME` means "allocator
//!    picks a frame". C's allocator picks the highest free address in
//!    the highest surviving memmap entry
//!    ([pg_utils.c:138-160](file:///minix3/minix/kernel/arch/i386/pg_utils.c#L138-L160):
//!    `for (m = mmap_size-1; ...)`; each returned frame is
//!    `mmap[m].mm_base_addr + mmap[m].mm_length`, i.e. the high end of
//!    the entry). Our allocator does the same per region: first frame
//!    = `region.end - PAGE_SIZE`. Behaviour is byte-comparable across
//!    the C↔Rust boundary.
//!
//!    ```text
//!    C memmap after cut_memmap:        Rust VmBootRegions (descending):
//!
//!    mm_base [0x300000, 0x302000)        r[0] = [0x400000, 0x500000)   (high)
//!              [0x240000, 0x300000)        r[1] = [0x240000, 0x300000)
//!    mm_base [0x100000, 0x200000)        r[2] = [0x100000, 0x200000)   (low)
//!
//!    alloc #1 → mmap[m].base+mmap[m].length - PAGE_SIZE
//!             = 0x302000 - 0x1000 = 0x301000      rust r[0].end - PAGE_SIZE = 0x4FF000
//!    alloc #2 → 0x2FF000 (still in same entry)      r[0].end - 2*PAGE_SIZE = 0x4FE000
//!    ...
//!    ```
//!
//! 2. **Low-address preservation for forward compatibility.** The
//!    kernel's low physical address range hosts the bootstrap identity
//!    map (`IDENTITY_MAP_END` in `lib.rs`), page-table pages
//!    (`boot_alloc::BootAlloc` range), the kernel image itself, and
//!    firmware-reserved or pinned DMA regions. Allocating VM image
//!    frames from the top down leaves the bottom free for these
//!    consumers.
//!
//! ## What this module does **not** own
//!
//! - General physical memory management. No bitmap, no buddy. No
//!   `FrameAllocator` trait.
//! - Runtime PMM. After VM starts, VM owns its own physical memory.
//! - `DirectMapArch` allocation. [`PhysAccess`] translates PA → VA; it
//!   does **not** allocate.
//!
//! ## Invariants enforced by construction
//!
//! 1. `VmBootRegion ∩ ReservedRegions = ∅`. Enforced inside
//!    [`VmBootRegion::select_multi`] by per-memmap-entry exclusion
//!    cutting. The allocator does **no** overlap detection.
//! 2. [`VmBootRegions<N>`] is descending-by-`start` and pairwise
//!    disjoint. Enforced by [`VmBootRegions::from_sorted`]. Construction
//!    rejects ascending / overlapping input with
//!    [`RegionError::NotDescending`] / [`RegionError::Overlapping`].
//! 3. [`VmBootAllocator<N>`] cursor stays inside the active region while
//!    [`VmBootAllocator::alloc_page`] has frames left; the
//!    `cursor > region.start` guard makes this invariant hard to
//!    violate.
//!
//! ## Zero-heap contract
//!
//! All data structures live in stack-allocated (or compile-time
//! inlined) `[T; N]` arrays. No `Vec`, no `Box`, no `alloc`. The const
//! generic `N` enforces the capacity ceiling at the type level. The
//! test module verifies the invariant by construction.

use minix_boot::MemoryRegion;
use minix_types::{PhysBytes, PhysFrame, VirBytes};

/// Frame size — matches [`PhysFrame::SIZE`] (4 KiB base page on all
/// supported architectures).
const PAGE_SIZE: u64 = PhysFrame::SIZE;

/// Maximum number of bootstrap memory regions the VM owns.
///
/// Bounded by `exclusions` cutting memmap — typical value 4–6, N=8
/// leaves 2× headroom. Callers needing a different capacity use the
/// const generic on [`VmBootRegions<N>`] / [`VmBootAllocator<N>`] and
/// the const-generic variant of [`VmBootRegion::select_multi`].
pub(crate) const MAX_BOOT_REGIONS: usize = 8;

// =====================================================================
// Errors
// =====================================================================

/// Errors produced by [`VmBootRegion`] / [`VmBootRegions`]
/// constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionError {
    /// `start > end`. Empty / zero-length inputs return
    /// [`RegionError::TooSmall`] instead.
    InvalidRange,
    /// `start` or `end` is not 4 KiB aligned.
    Misaligned,
    /// Region covers less than one page (`end - start < PAGE_SIZE`,
    /// including the empty case `start == end`).
    TooSmall,
    /// [`VmBootRegions::from_sorted`] input was not sorted descending
    /// by `start`. Repair: sort caller-side or call
    /// [`VmBootRegion::select_multi`] which produces descending output.
    NotDescending,
    /// [`VmBootRegions::from_sorted`] input contained overlapping
    /// entries. Repair: callers that own multiple slices should
    /// compress disjoint ranges themselves.
    Overlapping,
    /// [`VmBootRegion::select_multi`] produced more surviving slices
    /// than the destination capacity `N`. Bump the const generic `N`
    /// on the [`VmBootRegions<N>`] type or trim `exclusions` upstream.
    TooManyRegions,
}

/// Errors returned by [`VmBootAllocator::alloc_page`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmBootAllocError {
    /// All regions exhausted — total handed-out frames equals
    /// [`VmBootRegions::total_frames`].
    OutOfMemory,
}

// =====================================================================
// VmBootRegion — kernel-verified contiguous physical sub-range
// =====================================================================

/// A kernel-verified contiguous physical memory domain the VM may
/// bootstrap from.
///
/// Invariants (enforced by [`VmBootRegion::new`]):
///
/// 1. `start` and `end` are 4 KiB aligned; `start < end`; `end - start
///    ≥ PhysFrame::SIZE`.
/// 2. The caller (kernel memory-map logic) must guarantee `VmBootRegion
///    ∩ ReservedRegions = ∅`. [`VmBootRegion::new`] does **no** overlap
///    detection; detection and exclusion are the kernel memory-map's
///    responsibility and happen inside
///    [`VmBootRegion::select_multi`].
///
/// Fields are `pub` because VM runtime PMM init reads them across the
/// handoff boundary. Direct construction is allowed (caller-verified),
/// but the canonical construction path is [`VmBootRegion::new`] (with
/// validation) or [`VmBootRegion::select_multi`] (with exclusions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VmBootRegion {
    pub start: PhysBytes,
    pub end: PhysBytes,
}

impl VmBootRegion {
    /// Constructs a region from a caller-verified `[start, end)` range.
    ///
    /// Validates ordering (`start < end`), alignment (both endpoints
    /// 4 KiB aligned), and minimum size (`end - start ≥ PAGE_SIZE`).
    /// Does **not** verify exclusion from reserved regions — the
    /// caller (kernel memory-map logic) is responsible; prefer
    /// [`VmBootRegion::select_multi`] for the kernel-side path.
    pub const fn new(start: PhysBytes, end: PhysBytes) -> Result<Self, RegionError> {
        let start_u = start.0;
        let end_u = end.0;
        if start_u > end_u {
            return Err(RegionError::InvalidRange);
        }
        if start_u == end_u {
            return Err(RegionError::TooSmall);
        }
        if !start_u.is_multiple_of(PAGE_SIZE)
            || !end_u.is_multiple_of(PAGE_SIZE)
        {
            return Err(RegionError::Misaligned);
        }
        if end_u - start_u < PAGE_SIZE {
            return Err(RegionError::TooSmall);
        }
        Ok(Self { start, end })
    }

    /// Number of frames in this region.
    pub const fn frame_count(self) -> u64 {
        (self.end.0 - self.start.0) / PAGE_SIZE
    }

    /// Whether `addr` falls inside `[start, end)`.
    pub const fn contains(self, addr: PhysBytes) -> bool {
        addr.0 >= self.start.0 && addr.0 < self.end.0
    }

    /// Selects all surviving free regions for VM bootstrap.
    ///
    /// Walks `memmap` once. For each `MemoryRegion`, page-aligns the
    /// entry, then cuts the intersection with every exclusion; the
    /// surviving slices are pushed into a [`VmBootRegions<MAX_BOOT_REGIONS>`]
    /// sorted by `start` **descending** before return — matching C's
    /// `pg_alloc_page` allocation order (allocators consume high
    /// addresses first; see module doc).
    ///
    /// Returns:
    ///
    /// - `Ok(Some(regions))` — at least one page-aligned slice
    ///   survived. Caller may proceed to construct a
    ///   [`VmBootAllocator`].
    /// - `Ok(None)` — `memmap` contained no usable slice after
    ///   exclusions. The kernel has no free memory for VM bootstrap.
    /// - `Err(RegionError::TooManyRegions)` — more surviving slices
    ///   than the destination capacity `N`. Either bump the const
    ///   generic on the [`VmBootRegions<N>`] type passed via
    ///   [`VmBootRegion::select_multi_into`] or trim `exclusions`
    ///   upstream.
    ///
    /// Boot-shim caveat: the multiboot/UEFI memory map reports every
    /// conventional RAM as free *including* the kernel image and
    /// boot-module regions (because UEFI allocates them after the
    /// snapshot). Pass `exclusions = kernel_image + every module` to
    /// re-establish invariant (1).
    pub fn select_multi<const N: usize>(
        memmap: &[MemoryRegion],
        exclusions: &[MemoryRegion],
    ) -> Result<Option<VmBootRegions<N>>, RegionError> {
        let mut regions = VmBootRegions::<N>::empty();
        Self::select_multi_into(memmap, exclusions, &mut regions)?;
        if regions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(regions))
        }
    }

    /// Lower-level variant of [`VmBootRegion::select_multi`] that
    /// writes into a caller-supplied [`VmBootRegions<N>`]. Use this
    /// when the destination lives on a hot path that wants to avoid
    /// the `Option` round-trip.
    ///
    /// On `Ok(())`, the destination's `len()` is the number of
    /// surviving slices (0 means "memmap had nothing free"). On
    /// `Err(RegionError::TooManyRegions)`, the destination's first
    /// `N` slices are written and the rest are discarded; treat this
    /// as a hard capacity error.
    pub fn select_multi_into<const N: usize>(
        memmap: &[MemoryRegion],
        exclusions: &[MemoryRegion],
        out: &mut VmBootRegions<N>,
    ) -> Result<(), RegionError> {
        // Per-entry split-point buffer: each entry contributes its
        // own two endpoints plus up to 2 endpoints per exclusion (the
        // entry start/end splitters don't count toward the limit). The
        // boot kernel currently has 13 exclusions (1 kernel image + up
        // to 12 modules), so 28 split points is typical; 32 leaves a
        // small safety margin. Increasing `MAX_REGIONS_SPLITS` matters
        // only for synthetic tests with many exclusions.
        const MAX_REGIONS_SPLITS: usize = 32;
        let mut splits = [0u64; MAX_REGIONS_SPLITS];

        let mut tmp = [VmBootRegion::new(PhysBytes(0), PhysBytes(PAGE_SIZE))
            .expect("sentinel: empty page is valid"); N];
        let mut n_tmp: usize = 0;

        for entry in memmap {
            // 1. Page-align entry endpoints (ceil up start, floor down
            //    end) so we only emit fully-usable page-aligned ranges.
            let entry_start = entry.base.0.div_ceil(PAGE_SIZE) * PAGE_SIZE;
            let entry_end =
                (entry.base.0 + entry.len as u64) / PAGE_SIZE * PAGE_SIZE;
            if entry_end <= entry_start {
                continue;
            }

            // 2. Collect the two entry endpoints plus any exclusion
            //    endpoints that fall strictly inside the entry.
            let mut n_splits = 0usize;
            splits[n_splits] = entry_start;
            n_splits += 1;
            splits[n_splits] = entry_end;
            n_splits += 1;
            for excl in exclusions {
                let ex_start = excl.base.0.div_ceil(PAGE_SIZE) * PAGE_SIZE;
                let ex_end =
                    (excl.base.0 + excl.len as u64) / PAGE_SIZE * PAGE_SIZE;
                if ex_start > entry_start && ex_start < entry_end {
                    debug_assert!(
                        n_splits < MAX_REGIONS_SPLITS,
                        "VmBootRegion::select_multi_into: too many exclusion \
                         boundaries inside one entry (≥ {MAX_REGIONS_SPLITS})"
                    );
                    splits[n_splits] = ex_start;
                    n_splits += 1;
                }
                if ex_end > entry_start && ex_end < entry_end {
                    debug_assert!(
                        n_splits < MAX_REGIONS_SPLITS,
                        "VmBootRegion::select_multi_into: too many exclusion \
                         boundaries inside one entry (≥ {MAX_REGIONS_SPLITS})"
                    );
                    splits[n_splits] = ex_end;
                    n_splits += 1;
                }
            }

            // 3. Sort the populated prefix ascending (insertion sort,
            //    boot-critical, small N).
            for i in 1..n_splits {
                let key = splits[i];
                let mut j = i;
                while j > 0 && splits[j - 1] > key {
                    splits[j] = splits[j - 1];
                    j -= 1;
                }
                splits[j] = key;
            }

            // 4. Deduplicate in place (sorted list).
            let mut n_dedup = 0usize;
            for i in 0..n_splits {
                if i == 0 || splits[i] != splits[i - 1] {
                    splits[n_dedup] = splits[i];
                    n_dedup += 1;
                }
            }

            // 5. Walk adjacent pairs; emit each uncovered `[s, e)` as a
            //    surviving slice via `VmBootRegion::new` so invariants
            //    are re-validated.
            for pair in splits[..n_dedup].windows(2) {
                let s = pair[0];
                let e = pair[1];
                if e <= s {
                    continue;
                }
                let covered = exclusions.iter().any(|excl| {
                    let ex_start =
                        excl.base.0.div_ceil(PAGE_SIZE) * PAGE_SIZE;
                    let ex_end =
                        (excl.base.0 + excl.len as u64) / PAGE_SIZE * PAGE_SIZE;
                    s >= ex_start && e <= ex_end
                });
                if covered {
                    continue;
                }
                if n_tmp >= tmp.len() {
                    return Err(RegionError::TooManyRegions);
                }
                let region = match VmBootRegion::new(PhysBytes(s), PhysBytes(e)) {
                    Ok(r) => r,
                    Err(RegionError::TooSmall) => continue,
                    Err(other) => return Err(other),
                };
                tmp[n_tmp] = region;
                n_tmp += 1;
            }
        }

        if n_tmp == 0 {
            // `out` stays empty; caller observes `is_empty()`.
            return Ok(());
        }

        // 6. Sort by `start` descending (insertion sort, boot-critical,
        //    small N). Stable on ties would require an auxiliary key;
        //    ties on `start` cannot arise because we constructed each
        //    region from a non-empty page-aligned interval.
        for i in 1..n_tmp {
            let key = tmp[i];
            let mut j = i;
            while j > 0 && tmp[j - 1].start.0 < key.start.0 {
                tmp[j] = tmp[j - 1];
                j -= 1;
            }
            tmp[j] = key;
        }

        // 7. Write into the caller's buffer. `tmp[..n_tmp]` is already
        //    descending; we copy it in place.
        out.len = n_tmp;
        out.regions[..n_tmp].copy_from_slice(&tmp[..n_tmp]);
        Ok(())
    }
}

// =====================================================================
// VmBootRegions<N> — owned, descending-sorted region list
// =====================================================================

/// Kernel-verified physical memory regions the VM bootstrap owns.
///
/// Invariants (enforced by construction):
///
/// - `len ∈ [0, N]` and `len` is the populated prefix length.
/// - For `0 ≤ i < len - 1`: `regions[i].start > regions[i + 1].start`
///   (strict descending) and the entries are pairwise disjoint.
/// - Every populated entry satisfies [`VmBootRegion`] invariants
///   (page-aligned, non-empty, caller-verified free of reserved
///   regions).
///
/// Storage is a `[VmBootRegion; N]` array. The unused capacity slots
/// (indices `len..N`) hold a sentinel region `[0, PAGE_SIZE)` only so
/// the storage type-checks even when `len == 0`; **never read or pass
/// them to other code** — use [`VmBootRegions::len`] (and the
/// [`VmBootRegions::iter`] / [`VmBootRegions::get`] helpers) instead.
/// The tests rely on `len` to separate the populated prefix from the
/// sentinel tail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmBootRegions<const N: usize = MAX_BOOT_REGIONS> {
    len: usize,
    regions: [VmBootRegion; N],
}

impl<const N: usize> VmBootRegions<N> {
    /// Constructs a regions list from a caller-provided slice that is
    /// already sorted descending and pairwise disjoint.
    ///
    /// Validates:
    ///
    /// - Capacity: `regions.len() ≤ N`.
    /// - Sorted descending by `start` (strict).
    /// - Pairwise disjoint (no two regions overlap).
    ///
    /// Most callers should use [`VmBootRegion::select_multi`]
    /// directly, which constructs this type. This constructor exists
    /// for tests and for callers who already hold a
    /// descending-disjoint list.
    pub fn from_sorted(regions: &[VmBootRegion]) -> Result<Self, RegionError> {
        if regions.len() > N {
            return Err(RegionError::TooManyRegions);
        }
        for window in regions.windows(2) {
            let (a, b) = (window[0], window[1]);
            // a is the higher pair (strict descending). Reject if a
            // does not strictly dominate b.
            if a.start.0 <= b.start.0 {
                return Err(RegionError::NotDescending);
            }
            // Disjoint: since a is strictly above b, a and b overlap
            // iff b.end > a.start (the lower region extends into the
            // higher one's range).
            if b.end.0 > a.start.0 {
                return Err(RegionError::Overlapping);
            }
        }
        let mut out = Self::empty();
        out.len = regions.len();
        out.regions[..regions.len()].copy_from_slice(regions);
        Ok(out)
    }

    /// Constructs an empty regions list. Useful as a stack default
    /// before [`VmBootRegion::select_multi_into`] populates it.
    pub const fn empty() -> Self {
        // Sentinel satisfies `VmBootRegion` invariants (a single
        // page, page-aligned, `start < end`); the storage remains
        // valid even when `len == 0`. Unused slots are **never read**
        // by this module's own API.
        Self {
            len: 0,
            regions: [VmBootRegion {
                start: PhysBytes(0),
                end: PhysBytes(PAGE_SIZE),
            }; N],
        }
    }

    /// Populated length.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether no region is populated.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Total frames across all populated regions.
    pub const fn total_frames(&self) -> u64 {
        let mut total = 0u64;
        let mut i = 0;
        while i < self.len {
            total += self.regions[i].frame_count();
            i += 1;
        }
        total
    }

    /// Borrows the populated prefix as a slice (excludes the
    /// sentinel tail at indices `len..N`).
    pub fn as_slice(&self) -> &[VmBootRegion] {
        &self.regions[..self.len]
    }

    /// Returns the populated region's copy at `idx`, or `None` if
    /// `idx >= len`. The sentinel slots at indices `len..N` are
    /// **not** readable through this API.
    pub fn get(&self, idx: usize) -> Option<VmBootRegion> {
        if idx < self.len {
            Some(self.regions[idx])
        } else {
            None
        }
    }

    /// Iterates the populated regions.
    pub fn iter(&self) -> impl Iterator<Item = VmBootRegion> + '_ {
        self.regions[..self.len].iter().copied()
    }
}

impl<const N: usize> core::ops::Index<usize> for VmBootRegions<N> {
    type Output = VmBootRegion;
    fn index(&self, idx: usize) -> &VmBootRegion {
        &self.regions[idx]
    }
}

// =====================================================================
// VmBootAllocator<N> — one-shot bump, descending per region
// =====================================================================

/// One-shot bump allocator consuming [`VmBootRegions<N>`].
///
/// - **Per region** cursor starts at `region.end` and decreases; the
///   first handed-out frame is `region.end - PAGE_SIZE`, the next is
///   `region.end - 2 * PAGE_SIZE`, etc.
/// - **Across regions** the allocator walks in the regions'
///   descending order, so consecutively returned frames are strictly
///   descending in physical address — even across region boundaries.
/// - **Allocation-only** — no `free()`. The whole pool becomes VM's at
///   handoff; a future VM restart resets ownership, not allocation
///   state.
/// - **No discovery, no verification** — consumes an already-verified
///   [`VmBootRegions<N>`].
///
/// Default capacity `N` = [`MAX_BOOT_REGIONS`]. The const generic
/// allows tests to use smaller capacities (e.g. `VmBootRegions::<2>`)
/// to exercise overflow paths.
#[derive(Debug)]
pub struct VmBootAllocator<const N: usize = MAX_BOOT_REGIONS> {
    regions: VmBootRegions<N>,
    /// Index of the region currently being consumed.
    /// Invariant: `idx < regions.len()` while not fully exhausted.
    idx: usize,
    /// Next frame's PA. Decreases monotonically from
    /// `regions[idx].end` down to `regions[idx].start`.
    cursor: PhysBytes,
    /// Total frames already handed out.
    used_frames: u64,
}

impl<const N: usize> VmBootAllocator<N> {
    /// Constructs an allocator over the given (already verified)
    /// regions. Requires a non-empty regions list.
    pub fn new(regions: VmBootRegions<N>) -> Self {
        assert!(!regions.is_empty(), "VmBootAllocator::new: empty regions");
        let first = regions[0];
        let cursor = first.end;
        debug_assert!(
            cursor.0 >= PAGE_SIZE,
            "VmBootAllocator::new: region.end below PAGE_SIZE"
        );
        debug_assert!(
            cursor.0.is_multiple_of(PAGE_SIZE),
            "VmBootAllocator::new: region.end not page-aligned"
        );
        Self {
            regions,
            idx: 0,
            cursor,
            used_frames: 0,
        }
    }

    /// Allocates the next frame.
    ///
    /// Returns the frame at `cursor - PAGE_SIZE` and decreases `cursor`.
    /// When the current region's `cursor` reaches `region.start`, jumps
    /// to the next (lower) region, resetting `cursor = region.end`.
    /// When all regions are exhausted, returns
    /// [`VmBootAllocError::OutOfMemory`].
    pub fn alloc_page(&mut self) -> Result<PhysFrame, VmBootAllocError> {
        loop {
            let r = self.regions[self.idx];
            if self.cursor.0 > r.start.0 {
                let new_cursor = PhysBytes(self.cursor.0 - PAGE_SIZE);
                self.cursor = new_cursor;
                self.used_frames += 1;
                return Ok(PhysFrame::new(new_cursor));
            }
            if self.idx + 1 >= self.regions.len() {
                return Err(VmBootAllocError::OutOfMemory);
            }
            self.idx += 1;
            let next = self.regions[self.idx];
            self.cursor = next.end;
        }
    }

    /// Total frames already handed out across all regions.
    pub const fn used_frames(&self) -> u64 {
        self.used_frames
    }

    /// Total frames available across all regions.
    pub const fn total_frames(&self) -> u64 {
        self.regions.total_frames()
    }

    /// Remaining frames (= total − used).
    pub const fn remaining_frames(&self) -> u64 {
        self.regions.total_frames() - self.used_frames
    }

    /// Cursor (next frame's PA). Test/observability helper.
    pub const fn cursor(&self) -> PhysBytes {
        self.cursor
    }

    /// Index of the region currently being consumed.
    pub const fn current_region_idx(&self) -> usize {
        self.idx
    }

    /// Handoff boundary: returns the allocator's whole region pool (not
    /// just the consumed slice). VM runtime PMM init reads this to
    /// learn its physical memory universe.
    pub fn regions(&self) -> &VmBootRegions<N> {
        &self.regions
    }
}

// =====================================================================
// PhysAccess — PA → kernel VA boundary trait
// =====================================================================

/// Boundary trait: "given a physical frame, how does kernel code
/// access it?" — i.e. PA → kernel VA conversion.
///
/// This keeps ELF loaders and test mocks independent of the concrete
/// `DirectMapArch` implementation.
/// [`DirectMapArch`](crate::arch::direct_map::DirectMapArch) remains
/// purely the PA↔VA conversion mechanism (it does **not**
/// allocate).
///
/// Contract: the returned VA must make the whole frame
/// `[pa, pa + PhysFrame::SIZE)` accessible, so loaders can zero + copy
/// the frame after `phys_to_virt`.
pub trait PhysAccess {
    /// Convenience default: `phys_to_virt(frame.start())`.
    fn frame_virt(&self, frame: PhysFrame) -> VirBytes {
        self.phys_to_virt(frame.start())
    }

    /// Converts a physical address to a kernel-accessible virtual
    /// address.
    fn phys_to_virt(&self, phys: PhysBytes) -> VirBytes;
}

/// Every `DirectMapArch` implementation is a `PhysAccess`: the kernel
/// direct-map window (`kernel_phys_to_virt`) is exactly the "how does
/// kernel code access this PA" answer. This keeps `load_vm_elf` typed
/// against `PhysAccess` while production passes the architecture's
/// direct-map unit struct.
impl<D: crate::arch::direct_map::DirectMapArch> PhysAccess for D {
    fn phys_to_virt(&self, phys: PhysBytes) -> VirBytes {
        D::kernel_phys_to_virt(phys)
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn page_aligned_region(start: u64, end: u64) -> VmBootRegion {
        VmBootRegion::new(PhysBytes(start), PhysBytes(end))
            .expect("test region must satisfy alignment / size invariants")
    }

    // -------- VmBootRegion --------

    #[test]
    fn region_new_validates_alignment_and_range() {
        assert_eq!(
            VmBootRegion::new(PhysBytes(0x1000), PhysBytes(0x2000)),
            Ok(VmBootRegion {
                start: PhysBytes(0x1000),
                end: PhysBytes(0x2000),
            })
        );
        assert_eq!(
            VmBootRegion::new(PhysBytes(0x1001), PhysBytes(0x2000)),
            Err(RegionError::Misaligned)
        );
        assert_eq!(
            VmBootRegion::new(PhysBytes(0x2000), PhysBytes(0x1000)),
            Err(RegionError::InvalidRange)
        );
        assert_eq!(
            VmBootRegion::new(PhysBytes(0x1000), PhysBytes(0x1000)),
            Err(RegionError::TooSmall)
        );
    }

    #[test]
    fn region_frame_count_and_contains() {
        let r = page_aligned_region(0x1000, 0x5000);
        assert_eq!(r.frame_count(), 4);
        assert!(r.contains(PhysBytes(0x1000)));
        assert!(r.contains(PhysBytes(0x4fff)));
        assert!(!r.contains(PhysBytes(0x5000)));
    }

    // -------- VmBootRegions --------

    #[test]
    fn regions_empty_has_zero_len() {
        let r: VmBootRegions<4> = VmBootRegions::empty();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
        assert_eq!(r.total_frames(), 0);
        assert_eq!(r.as_slice().len(), 0);
    }

    #[test]
    fn regions_from_sorted_accepts_descending_disjoint() {
        let buf = [
            page_aligned_region(0x100000, 0x200000),
            page_aligned_region(0x010000, 0x020000),
        ];
        let r = VmBootRegions::<2>::from_sorted(&buf).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].start, PhysBytes(0x100000));
        assert_eq!(r[1].start, PhysBytes(0x010000));
        assert_eq!(r.total_frames(), 0x100000 / PAGE_SIZE + 0x10000 / PAGE_SIZE);
        assert_eq!(r.as_slice().len(), 2);
    }

    #[test]
    fn regions_from_sorted_rejects_ascending() {
        let buf = [
            page_aligned_region(0x010000, 0x020000),
            page_aligned_region(0x100000, 0x200000),
        ];
        assert_eq!(
            VmBootRegions::<2>::from_sorted(&buf),
            Err(RegionError::NotDescending)
        );
    }

    #[test]
    fn regions_from_sorted_rejects_overlapping() {
        // Descending by start: a = [0x180000, 0x200000),
        // b = [0x100000, 0x190000) — disjoint by start but b.end
        // (0x190000) reaches into a's range (a.start..a.end =
        // 0x180000..0x200000); overlap is `b.end > a.start`.
        let buf = [
            page_aligned_region(0x180000, 0x200000),
            page_aligned_region(0x100000, 0x190000),
        ];
        assert_eq!(
            VmBootRegions::<2>::from_sorted(&buf),
            Err(RegionError::Overlapping)
        );
    }

    #[test]
    fn regions_from_sorted_rejects_over_capacity() {
        let buf = [
            page_aligned_region(0x300000, 0x400000),
            page_aligned_region(0x200000, 0x300000),
            page_aligned_region(0x100000, 0x200000),
        ];
        assert_eq!(
            VmBootRegions::<2>::from_sorted(&buf),
            Err(RegionError::TooManyRegions)
        );
    }

    #[test]
    fn regions_iter_yields_populated_prefix() {
        let buf = [
            page_aligned_region(0x100000, 0x200000),
            page_aligned_region(0x010000, 0x020000),
        ];
        let r = VmBootRegions::<3>::from_sorted(&buf).unwrap();
        let collected: Vec<_> = r.iter().collect();
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].start, PhysBytes(0x100000));
        assert_eq!(collected[1].start, PhysBytes(0x010000));
    }

    #[test]
    fn regions_sentinel_tail_is_outside_logic() {
        // Sentinel design: every populated `len < N` slot is real;
        // slots `len..N` are `[0, PAGE_SIZE)` placeholders. This test
        // pins the boundary so a future refactor cannot silently
        // extend the populated prefix.
        let r = VmBootRegions::<3>::from_sorted(&[page_aligned_region(
            0x1000,
            0x2000,
        )])
        .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r.as_slice().len(), 1);
        // Slot at `len` (= 1) holds the sentinel: not exposed via
        // `iter()` / `get(N)` / `as_slice()`.
        assert!(r.get(1).is_none());
        assert_eq!(r.iter().count(), 1);
    }

    // -------- select_multi / select_multi_into --------

    #[test]
    fn select_multi_no_exclusions_returns_all_descending() {
        let memmap = [
            MemoryRegion { base: PhysBytes(0x010000), len: 0x010000 },
            MemoryRegion { base: PhysBytes(0x100000), len: 0x100000 },
        ];
        let r: VmBootRegions<2> =
            VmBootRegion::select_multi(&memmap, &[]).unwrap().unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].start, PhysBytes(0x100000));
        assert_eq!(r[1].start, PhysBytes(0x010000));
    }

    #[test]
    fn select_multi_cuts_exclusions_and_sorts_descending() {
        // Single memmap entry (0x100000..0x400000). Exclude
        // kernel-image (0x200000..0x240000) and module
        // (0x300000..0x302000). Survivors:
        //   [0x100000, 0x200000), [0x240000, 0x300000),
        //   [0x302000, 0x400000)  → r[0..2] in that order.
        let memmap = [MemoryRegion {
            base: PhysBytes(0x100000),
            len: 0x300000,
        }];
        let exclusions = [
            MemoryRegion { base: PhysBytes(0x200000), len: 0x40000 },
            MemoryRegion { base: PhysBytes(0x300000), len: 0x002000 },
        ];
        let r: VmBootRegions<4> =
            VmBootRegion::select_multi(&memmap, &exclusions).unwrap().unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].start, PhysBytes(0x302000));
        assert_eq!(r[0].end, PhysBytes(0x400000));
        assert_eq!(r[1].start, PhysBytes(0x240000));
        assert_eq!(r[1].end, PhysBytes(0x300000));
        assert_eq!(r[2].start, PhysBytes(0x100000));
        assert_eq!(r[2].end, PhysBytes(0x200000));
    }

    #[test]
    fn select_multi_single_exclusion_in_the_middle() {
        // One memmap entry [0x100000, 0x500000); one exclusion
        // [0x200000, 0x300000) in the middle. The split-point pass
        // emits three adjacent pairs, but the exclusion itself is
        // covered by `exclusions.iter().any(...)` and skipped. The
        // result is two surviving slices, in descending order:
        //   r[0] = [0x300000, 0x500000)
        //   r[1] = [0x100000, 0x200000)
        let memmap = [MemoryRegion {
            base: PhysBytes(0x100000),
            len: 0x400000,
        }];
        let exclusions = [MemoryRegion {
            base: PhysBytes(0x200000),
            len: 0x100000,
        }];
        let r: VmBootRegions<4> =
            VmBootRegion::select_multi(&memmap, &exclusions).unwrap().unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].start, PhysBytes(0x300000));
        assert_eq!(r[0].end, PhysBytes(0x500000));
        assert_eq!(r[1].start, PhysBytes(0x100000));
        assert_eq!(r[1].end, PhysBytes(0x200000));
    }

    #[test]
    fn select_multi_aligns_partial_endpoints() {
        // Entry not page-aligned, ends mid-page: usable
        // [0x2000, 0x5000).
        let memmap = [MemoryRegion {
            base: PhysBytes(0x1001),
            len: 0x4000,
        }];
        let r: VmBootRegions<2> = VmBootRegion::select_multi(&memmap, &[])
            .unwrap()
            .unwrap();
        assert_eq!(r[0].start, PhysBytes(0x2000));
        assert_eq!(r[0].end, PhysBytes(0x5000));
    }

    #[test]
    fn select_multi_returns_none_when_nothing_survives() {
        let memmap = [MemoryRegion {
            base: PhysBytes(0x100000),
            len: 0x100000,
        }];
        let exclusions = [MemoryRegion {
            base: PhysBytes(0x100000),
            len: 0x100000,
        }];
        let r: Option<VmBootRegions<2>> =
            VmBootRegion::select_multi(&memmap, &exclusions).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn select_multi_empty_memmap_returns_none() {
        let r: Option<VmBootRegions<2>> =
            VmBootRegion::select_multi(&[], &[]).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn select_multi_returns_too_many_regions_when_over_capacity() {
        // One huge memmap entry with 9 single-page exclusions
        // arranged so 10 surviving slices fall out — exceeds the
        // requested `N = 1`.
        let mut exclusions = [MemoryRegion {
            base: PhysBytes(0),
            len: 0,
        }; 9];
        for (i, e) in exclusions.iter_mut().enumerate() {
            *e = MemoryRegion {
                base: PhysBytes(0x10000 + i as u64 * 0x10000),
                len: 0x1000,
            };
        }
        let memmap = [MemoryRegion {
            base: PhysBytes(0x0),
            len: 0xa0000,
        }];
        let r: Result<Option<VmBootRegions<1>>, RegionError> =
            VmBootRegion::select_multi(&memmap, &exclusions);
        assert_eq!(r, Err(RegionError::TooManyRegions));
    }

    #[test]
    fn select_multi_into_writes_into_caller_buffer() {
        // Same fixtures as `select_multi_cuts_exclusions_and_sorts_descending`,
        // but verify the lower-level `_into` variant.
        let memmap = [MemoryRegion {
            base: PhysBytes(0x100000),
            len: 0x300000,
        }];
        let exclusions = [
            MemoryRegion { base: PhysBytes(0x200000), len: 0x40000 },
            MemoryRegion { base: PhysBytes(0x300000), len: 0x002000 },
        ];
        let mut out: VmBootRegions<4> = VmBootRegions::empty();
        VmBootRegion::select_multi_into(&memmap, &exclusions, &mut out).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].start, PhysBytes(0x302000));
        assert_eq!(out[2].end, PhysBytes(0x200000));
    }

    #[test]
    fn select_multi_boundary_touching_exclusions_are_clipped() {
        let memmap = [MemoryRegion {
            base: PhysBytes(0x100000),
            len: 0x5000,
        }];
        // Excl starts at region start, covers 1 page.
        let exclusions = [MemoryRegion {
            base: PhysBytes(0x100000),
            len: 0x1000,
        }];
        let r: VmBootRegions<2> =
            VmBootRegion::select_multi(&memmap, &exclusions).unwrap().unwrap();
        assert_eq!(r[0].start, PhysBytes(0x101000));
        assert_eq!(r[0].end, PhysBytes(0x105000));

        // Excl ends at region end, covers last page only.
        let memmap2 = [MemoryRegion {
            base: PhysBytes(0x100000),
            len: 0x5000,
        }];
        let exclusions2 = [MemoryRegion {
            base: PhysBytes(0x104000),
            len: 0x1000,
        }];
        let r2: VmBootRegions<2> =
            VmBootRegion::select_multi(&memmap2, &exclusions2)
                .unwrap()
                .unwrap();
        assert_eq!(r2[0].start, PhysBytes(0x100000));
        assert_eq!(r2[0].end, PhysBytes(0x104000));
    }

    // -------- VmBootAllocator --------

    #[test]
    fn allocator_first_frame_is_region_end_minus_page_size() {
        let regions = VmBootRegions::<1>::from_sorted(&[page_aligned_region(
            0x1000, 0x4000,
        )])
        .unwrap();
        let mut a = VmBootAllocator::new(regions);
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x3000));
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x2000));
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x1000));
        assert_eq!(a.alloc_page(), Err(VmBootAllocError::OutOfMemory));
        assert_eq!(a.used_frames(), 3);
        assert_eq!(a.remaining_frames(), 0);
    }

    #[test]
    fn allocator_descends_within_single_region() {
        let regions = VmBootRegions::<1>::from_sorted(&[page_aligned_region(
            0x10000, 0x14000,
        )])
        .unwrap();
        let mut a = VmBootAllocator::new(regions);
        let f0 = a.alloc_page().unwrap().start();
        let f1 = a.alloc_page().unwrap().start();
        let f2 = a.alloc_page().unwrap().start();
        let f3 = a.alloc_page().unwrap().start();
        assert_eq!(f0, PhysBytes(0x13000));
        assert_eq!(f1, PhysBytes(0x12000));
        assert_eq!(f2, PhysBytes(0x11000));
        assert_eq!(f3, PhysBytes(0x10000));
        assert_eq!(a.alloc_page(), Err(VmBootAllocError::OutOfMemory));
    }

    #[test]
    fn allocator_crosses_to_next_region_when_exhausted() {
        // Two 1-frame regions.
        let buf = [
            page_aligned_region(0x2000, 0x3000),
            page_aligned_region(0x1000, 0x2000),
        ];
        let regions = VmBootRegions::<2>::from_sorted(&buf).unwrap();
        let mut a = VmBootAllocator::new(regions);

        // First frame from the higher region (0x2000).
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x2000));
        assert_eq!(a.current_region_idx(), 0);

        // Second alloc crosses into the lower region (0x1000).
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x1000));
        assert_eq!(a.current_region_idx(), 1);

        // Third alloc: both regions exhausted.
        assert_eq!(a.alloc_page(), Err(VmBootAllocError::OutOfMemory));
    }

    #[test]
    fn allocator_crosses_multiple_region_boundaries() {
        // Three disjoint regions (high above low, separated by gaps):
        //   r[0] = [0x6000, 0x8000) — frames 0x7000, 0x6000
        //   r[1] = [0x3000, 0x5000) — frames 0x4000, 0x3000
        //   r[2] = [0x0000, 0x2000) — frames 0x1000, 0x0000
        // The bump walks regions in descending order: r[0]'s two
        // frames first (descending), then crosses into r[1], then r[2],
        // then OOM.
        let buf = [
            page_aligned_region(0x6000, 0x8000),
            page_aligned_region(0x3000, 0x5000),
            page_aligned_region(0x0000, 0x2000),
        ];
        let regions = VmBootRegions::<3>::from_sorted(&buf).unwrap();
        let mut a = VmBootAllocator::new(regions);
        // Region r[0].
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x7000));
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x6000));
        assert_eq!(a.current_region_idx(), 0);
        // Crosses into r[1].
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x4000));
        assert_eq!(a.current_region_idx(), 1);
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x3000));
        // Crosses into r[2].
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x1000));
        assert_eq!(a.current_region_idx(), 2);
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x0000));
        // Pool exhausted.
        assert_eq!(a.alloc_page(), Err(VmBootAllocError::OutOfMemory));
        assert_eq!(a.used_frames(), 6);
    }

    #[test]
    fn allocator_out_of_memory_when_all_regions_exhausted() {
        let buf = [
            page_aligned_region(0x2000, 0x3000),
            page_aligned_region(0x1000, 0x2000),
        ];
        let regions = VmBootRegions::<2>::from_sorted(&buf).unwrap();
        let mut a = VmBootAllocator::new(regions);
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x2000));
        assert_eq!(a.alloc_page().unwrap().start(), PhysBytes(0x1000));
        assert_eq!(a.alloc_page(), Err(VmBootAllocError::OutOfMemory));
    }

    #[test]
    fn allocator_used_and_remaining_track_correctly() {
        let regions = VmBootRegions::<1>::from_sorted(&[page_aligned_region(
            0x1000, 0x5000,
        )])
        .unwrap();
        let mut a = VmBootAllocator::new(regions);
        assert_eq!(a.used_frames(), 0);
        assert_eq!(a.remaining_frames(), 4);
        assert_eq!(a.total_frames(), 4);
        a.alloc_page().unwrap();
        a.alloc_page().unwrap();
        assert_eq!(a.used_frames(), 2);
        assert_eq!(a.remaining_frames(), 2);
    }

    #[test]
    fn allocator_handoff_returns_full_regions_not_just_used() {
        let buf = [
            page_aligned_region(0x100000, 0x200000),
            page_aligned_region(0x010000, 0x020000),
        ];
        let regions = VmBootRegions::<2>::from_sorted(&buf).unwrap();
        let a = VmBootAllocator::new(regions);
        let handoff = a.regions();
        assert_eq!(handoff.len(), 2);
        assert_eq!(handoff[0].start, PhysBytes(0x100000));
        assert_eq!(handoff[1].start, PhysBytes(0x010000));
    }

    #[test]
    fn allocator_cursor_starts_at_region_end() {
        let regions = VmBootRegions::<1>::from_sorted(&[page_aligned_region(
            0x1000, 0x4000,
        )])
        .unwrap();
        let a = VmBootAllocator::new(regions);
        assert_eq!(a.cursor(), PhysBytes(0x4000));
    }

    #[test]
    fn allocator_new_panics_on_empty_regions() {
        let regions: VmBootRegions<2> = VmBootRegions::empty();
        let result =
            std::panic::catch_unwind(|| VmBootAllocator::new(regions));
        assert!(result.is_err());
    }

    // -------- PhysAccess --------

    struct MockAccess;
    impl PhysAccess for MockAccess {
        fn phys_to_virt(&self, phys: PhysBytes) -> VirBytes {
            VirBytes(phys.get() + 0xFFFF_0000_0000_0000)
        }
    }

    #[test]
    fn frame_virt_default_impl_matches_phys_to_virt() {
        let frame = PhysFrame::new(PhysBytes(0x3000));
        let va = MockAccess.frame_virt(frame);
        assert_eq!(va, VirBytes(0xFFFF_0000_0000_3000));
    }

    #[test]
    fn phys_frame_size_constant() {
        assert_eq!(PhysFrame::SIZE, 0x1000);
    }

    #[test]
    fn phys_frame_contains_half_open() {
        let f = PhysFrame::new(PhysBytes(0x2000));
        assert!(f.contains(PhysBytes(0x2000)));
        assert!(f.contains(PhysBytes(0x2fff)));
        assert!(!f.contains(PhysBytes(0x3000)));
        assert!(!f.contains(PhysBytes(0x1fff)));
    }
}
