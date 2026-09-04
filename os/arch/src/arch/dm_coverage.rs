//! Direct Map coverage establishment on the bootstrap root (kernel boot path).
//!
//! Implements the coverage half of the paging-initialization design
//! (`07-paging_init_design` §6.1): after paging is enabled on the bootstrap
//! root, the kernel installs two Direct Map windows into that same root —
//!
//! - **Kernel DM** (`KERNEL_DIRECT_MAP_BASE`, supervisor `RW`): the access
//!   backend for every kernel-context `Paging` handle (the `KernelDm` PTE
//!   access channel) and for general kernel PA access once the identity
//!   mapping is partially demolished (see below).
//! - **VM DM** (`VM_DIRECT_MAP_BASE`, user `RW`): the access backend for VM
//!   (a user-space server) — VM self page-table walks, page zeroing, and
//!   allocator metadata all read/write physical pages through this window.
//!
//! # Coverage follows resource ranges, not `[0, max_phys)`
//!
//! The mapping candidate set is a two-source union
//! (`07-paging_init_design` §6.1 "映射候选的两源并集"):
//!
//! 1. **Resource-classified memmap ranges** — the conventional-RAM regions
//!    reported by boot (source of VM PMM RAM);
//! 2. **Explicit bootstrap PhysAccess ranges** — the VM self page-table tree
//!    (root page + boot bump region). Both are `LOADER_DATA` allocations that
//!    never appear in a conventional memmap; registering them explicitly is
//!    what closes the "self root can never be DM-covered from memmap alone"
//!    derivation gap.
//!
//! Reserved holes (VGA/ROM/BIOS, MMIO, firmware regions) are not members of
//! either source and therefore never receive a mapping — at any granularity.
//! Huge-page granularity selection is resource-containment-aware: a 1 GiB /
//! 2 MiB leaf is emitted only when the *whole* leaf lies inside one candidate
//! range, so a hole can never be swallowed by an aligned huge page
//! (Gate 8 Case E, mechanism level).
//!
//! # The boot identity write channel
//!
//! DM establishment itself must not depend on the windows it is building.
//! All PTE writes below go through the bootstrap root's own identity mapping
//! (`VA = PA`, supervisor, `[0, 4 GiB)`): allocation of intermediate table
//! pages via the registered boot bump allocator, zero-fill via `VA = PA`,
//! leaf/intermediate writes via `VA = PA`. On x86-64 the VM DM window
//! overlaps the identity mapping (`PDPTE[2]` covers both VA [2 GiB, 3 GiB)
//! identity and the window base); the resulting leaf demotion only rewrites
//! VA [2 GiB, 3 GiB) translation, which is orthogonal to the `VA < 1 GiB`
//! write channel — no circular dependency. Kernel DM is established first so
//! that PA access to the demolished identity range flows through the kernel
//! channel from that point on.

use super::paging::{PageFlags, PageTableError};
use minix_types::{PhysBytes, VirBytes};

/// A page-aligned physical range eligible for DM coverage.
///
/// One candidate from either source: a conventional memmap region, the VM
/// self root page, or the boot bump region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmRange {
    /// Physical base (page-aligned).
    pub base: u64,
    /// Length in bytes (page-aligned, non-zero).
    pub len: u64,
}

impl DmRange {
    /// Create a range. Debug-asserts page alignment (candidates must be
    /// page-size contained for huge-page selection to be sound).
    pub const fn new(base: u64, len: u64) -> Self {
        // const fn cannot assert in older MSRVs; enforce at use sites via
        // the debug_assert in plan_range_units.
        Self { base, len }
    }

    /// Clip the range to `[0, pa_limit)`. Returns `None` if nothing remains.
    ///
    /// Used to constrain coverage to a window's representable PA span
    /// (`VM_DIRECT_MAP_SIZE` for the VM DM window): a conventional region
    /// extending past the window is partially covered — the remainder stays
    /// outside DM representability and is excluded from VM PMM eligibility
    /// by resource qualification.
    pub fn clipped_to(self, pa_limit: u64) -> Option<DmRange> {
        if self.base >= pa_limit {
            return None;
        }
        let end = core::cmp::min(self.base.checked_add(self.len)?, pa_limit);
        Some(DmRange { base: self.base, len: end - self.base })
    }
}

/// Greedy page-size selection over one candidate range.
///
/// Walks the range from base to end, emitting `(pa, size)` units with the
/// largest size whose alignment and remaining length permit:
/// 1 GiB (only when `one_gb` is supported by the implementation), then
/// 2 MiB, then 4 KiB. Every unit lies entirely inside `range`, which is the
/// resource-containment guarantee: a sub-range (hole) that is not part of
/// any candidate never appears in a unit, at any granularity.
///
/// `page_size` is the base leaf size (4 KiB on all current architectures).
///
/// The emit closure is fallible so the installation driver can propagate a
/// leaf conflict (`AlreadyMapped` — a boot-layout bug per the
/// [`DmCoverageArch`] contract) instead of panicking mid-establishment.
pub fn plan_range_units<F: FnMut(u64, usize) -> Result<(), PageTableError>>(
    range: DmRange,
    one_gb: bool,
    page_size: u64,
    mut emit: F,
) -> Result<(), PageTableError> {
    debug_assert!(
        range.base.is_multiple_of(page_size) && range.len.is_multiple_of(page_size),
        "DM candidate range must be page-aligned"
    );
    const TWO_MB: u64 = 1 << 21;
    const ONE_GB: u64 = 1 << 30;
    let mut cur = range.base;
    let end = range.base + range.len;
    while cur < end {
        let remaining = end - cur;
        if one_gb && cur.is_multiple_of(ONE_GB) && remaining >= ONE_GB {
            emit(cur, ONE_GB as usize)?;
            cur += ONE_GB;
        } else if cur.is_multiple_of(TWO_MB) && remaining >= TWO_MB {
            emit(cur, TWO_MB as usize)?;
            cur += TWO_MB;
        } else {
            emit(cur, page_size as usize)?;
            cur += page_size;
        }
    }
    Ok(())
}

/// Per-architecture installation of one DM leaf via the boot identity write
/// channel.
///
/// The shared driver ([`plan_range_units`] + [`establish_dm_range`]) performs
/// resource-containment-aware page-size selection; this trait is the only
/// architecture-specific part: walking the (architecture-shaped) table tree
/// from `root_paddr` to the leaf slot and writing it, allocating and
/// zero-filling intermediate table pages through the registered `pt_alloc`
/// (boot bump) along the way.
///
/// This is deliberately NOT a `HugePages` extension: coverage establishment
/// is a static boot-time operation on the bootstrap root, not a per-handle
/// page-table service, so implementors are dedicated ZSTs rather than the
/// runtime `Paging` types.
pub trait DmCoverageArch {
    /// Whether the MMU can emit 1 GiB leaves — input to page-size selection
    /// ([`plan_range_units`]). x86-64 overrides with a CPUID probe; ARM64 and
    /// RISC-V always support 1 GiB blocks.
    fn supports_1gb_page() -> bool {
        true
    }

    /// Install one leaf mapping `vaddr → paddr` with `size ∈ {1 GiB, 2 MiB,
    /// 4 KiB}` (matching the sizes [`plan_range_units`] emits) and `flags`.
    ///
    /// Implementations must:
    /// - use the boot identity write channel (`VA = PA`) for every table
    ///   access — NOT the DM windows being established;
    /// - allocate intermediate table pages via `crate::pt_alloc` (zero-filled
    ///   by the allocator through the same identity channel);
    /// - on x86-64, when a finer leaf must be installed beneath the identity
    ///   mapping's `PDPTE[2]` 1 GiB leaf, replace that slot with an *empty*
    ///   lower table: the identity translation for VA [2 GiB, 3 GiB) is
    ///   wholesale superseded by DM semantics, and the replacement table
    ///   keeps only what subsequent allowed units re-map — a reserved hole
    ///   is therefore never swallowed (Gate 8 Case E, mechanism level);
    /// - return `PageTableError::AlreadyMapped` on a leaf conflict that the
    ///   boot layout cannot produce (windows outside the identity range are
    ///   fresh slots — a conflict is a boot bug, fail fast).
    ///
    /// # Safety
    ///
    /// - Paging must be enabled with the bootstrap root active, whose
    ///   identity mapping covers `root_paddr` and every intermediate page
    ///   (all bootstrap allocations satisfy the §6.1 allocation bound
    ///   `PA < min(IDENTITY_MAP_END, VM DM window PA end)`).
    /// - Single-threaded boot context; no concurrent table walkers.
    /// - `vaddr` must be aligned to `size`; `paddr` likewise.
    unsafe fn dm_install_leaf(
        root_paddr: PhysBytes,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;
}

/// Establish DM coverage for one candidate range within a window.
///
/// Clips `range` to `[0, window_pa_limit)` (pass `u64::MAX` for the kernel
/// DM, whose VA span covers all of physical memory), selects page sizes by
/// resource containment, and installs each unit at `va_base + pa`.
///
/// - Kernel DM: `va_base = KERNEL_DIRECT_MAP_BASE`, `flags` supervisor RW.
/// - VM DM: `va_base = VM_DIRECT_MAP_BASE`, `flags` user RW,
///   `window_pa_limit = VM_DIRECT_MAP_SIZE`.
pub fn establish_dm_range<P: DmCoverageArch>(
    root: PhysBytes,
    range: DmRange,
    window_pa_limit: u64,
    va_base: u64,
    flags: PageFlags,
) -> Result<(), PageTableError> {
    let Some(clipped) = range.clipped_to(window_pa_limit) else {
        return Ok(());
    };
    plan_range_units(clipped, P::supports_1gb_page(), 4096, |pa, size| {
        // SAFETY: boot context per `dm_install_leaf` contract — paging is
        // enabled on `root`, all table pages satisfy the allocation bound,
        // and the caller guarantees single-threaded boot.
        unsafe { P::dm_install_leaf(root, VirBytes(va_base + pa), PhysBytes(pa), size, flags) }
    })
}

/// Software model of DM coverage establishment for host tests.
///
/// `MockDmCoverage` records every leaf in a global registry instead of
/// touching real page tables, so `establish_dm_range` drivers (and later the
/// kernel boot path's two-source candidate assembly) can be tested for hole
/// exclusion, window clipping, VA offsetting and page-size selection without
/// hardware.
#[cfg(feature = "mock")]
pub mod mock {
    use super::{DmCoverageArch, PageFlags, PageTableError};
    use minix_types::{PhysBytes, VirBytes};
    use std::collections::BTreeMap;
    use std::sync::{Mutex, OnceLock};

    /// One recorded leaf: `(paddr, size, flags)`, keyed by leaf VA base.
    pub type MockDmLeaf = (u64, usize, PageFlags);

    fn registry() -> &'static Mutex<BTreeMap<u64, MockDmLeaf>> {
        static REGISTRY: OnceLock<Mutex<BTreeMap<u64, MockDmLeaf>>> = OnceLock::new();
        REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    /// Registry-backed implementor of [`DmCoverageArch`].
    pub struct MockDmCoverage;

    impl DmCoverageArch for MockDmCoverage {
        fn supports_1gb_page() -> bool {
            true
        }

        unsafe fn dm_install_leaf(
            _root_paddr: PhysBytes,
            vaddr: VirBytes,
            paddr: PhysBytes,
            size: usize,
            flags: PageFlags,
        ) -> Result<(), PageTableError> {
            if !vaddr.0.is_multiple_of(size as u64) || !paddr.0.is_multiple_of(size as u64) {
                return Err(PageTableError::InvalidAddress);
            }
            let mut leaves = registry().lock().unwrap();
            if leaves.contains_key(&vaddr.0) {
                return Err(PageTableError::AlreadyMapped);
            }
            leaves.insert(vaddr.0, (paddr.0, size, flags));
            Ok(())
        }
    }

    /// Snapshot of recorded leaves, ordered by leaf VA base.
    pub fn mock_dm_leaves() -> BTreeMap<u64, MockDmLeaf> {
        registry().lock().unwrap().clone()
    }

    /// Reset the registry (test isolation).
    pub fn mock_dm_clear() {
        registry().lock().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(range: DmRange, one_gb: bool) -> Vec<(u64, usize)> {
        let mut units = Vec::new();
        plan_range_units(range, one_gb, 4096, |pa, size| {
            units.push((pa, size));
            Ok(())
        })
        .unwrap();
        units
    }

    fn covered(units: &[(u64, usize)]) -> Vec<(u64, u64)> {
        units.iter().map(|&(pa, s)| (pa, pa + s as u64)).collect()
    }

    /// A 2 MiB-aligned conventional region maps as 2 MiB huge pages.
    #[test]
    fn test_two_mb_region_uses_two_mb_units() {
        let units = collect(DmRange::new(0x20_0000, 0x40_0000), false);
        assert_eq!(units, vec![(0x20_0000, 0x20_0000), (0x40_0000, 0x20_0000)]);
    }

    /// Unaligned head and tail degrade to 4 KiB units; the aligned middle
    /// stays huge — a hole inside the unaligned fragments is structurally
    /// unmapped because only candidate ranges are walked.
    #[test]
    fn test_unaligned_fragments_degrade_to_4k() {
        // [1 MiB, 5 MiB): 4K for [1M, 2M), 2M for [2M, 4M), 4K for [4M, 5M).
        let units = collect(DmRange::new(0x10_0000, 0x40_0000), false);
        assert_eq!(units.first().copied(), Some((0x10_0000, 4096)));
        // Tail fragment: the last 4K unit ends the range.
        assert_eq!(*units.last().unwrap(), (0x4F_F000, 4096));
        // 256 × 4K + 1 × 2M + 256 × 4K
        assert_eq!(units.len(), 256 + 1 + 256);
        assert_eq!(units[256], (0x20_0000, 0x20_0000));
        assert_eq!(units[257], (0x40_0000, 4096));
    }

    /// Gate 8 Case E (mechanism level): a reserved hole between two
    /// conventional ranges is covered by NO unit at any granularity.
    #[test]
    fn test_legacy_hole_never_swallowed() {
        // Real x86 PC layout: conventional [0, 0xA0000), hole
        // [0xA0000, 0x100000), conventional [0x100000, ...).
        let low = collect(DmRange::new(0, 0xA_0000), true);
        let high = collect(DmRange::new(0x10_0000, 0x20_0000), true);
        for (start, end) in covered(&low).into_iter().chain(covered(&high)) {
            assert!(
                end <= 0xA_0000 || start >= 0x10_0000,
                "unit [{:#x}, {:#x}) overlaps the legacy hole",
                start,
                end
            );
        }
    }

    /// 1 GiB units are emitted only when supported, aligned, and fully
    /// contained in the candidate range.
    #[test]
    fn test_one_gb_selection() {
        // [1 GiB, 6 GiB) with 1 GiB support: five 1 GiB units.
        let units = collect(DmRange::new(0x4000_0000, 0x1_4000_0000), true);
        assert_eq!(
            units,
            vec![
                (0x4000_0000, 1 << 30),
                (0x8000_0000, 1 << 30),
                (0xC000_0000, 1 << 30),
                (0x1_0000_0000, 1 << 30),
                (0x1_4000_0000, 1 << 30),
            ]
        );
        // Same base without 1 GiB support degrades to 2 MiB units (32 of
        // them for 64 MiB).
        let units = collect(DmRange::new(0x4000_0000, 0x400_0000), false);
        assert_eq!(units.len(), 32);
        assert_eq!(units[0], (0x4000_0000, 0x20_0000));
        assert_eq!(units[1], (0x4020_0000, 0x20_0000));
        // 1 GiB support but a range straddling the 1 GiB boundary: only the
        // aligned interior gets 1 GiB units.
        let units = collect(DmRange::new(0x8000_0000 - 0x1000, 0x2000 + (1 << 30)), true);
        assert!(units.contains(&(0x8000_0000, 1 << 30)));
        assert_eq!(units.first().copied(), Some((0x8000_0000 - 0x1000, 4096)));
    }

    /// Window clipping: coverage stops at the window's PA limit; ranges
    /// beyond it are dropped entirely.
    #[test]
    fn test_window_clipping() {
        // 1 GiB window: [0, 0x4000_0000); a conventional region extending
        // past the limit is partially covered.
        let range = DmRange::new(0x3000_0000, 0x8000_0000).clipped_to(0x4000_0000).unwrap();
        assert_eq!(range, DmRange::new(0x3000_0000, 0x1000_0000));
        // Entirely outside → dropped.
        assert_eq!(DmRange::new(0x5000_0000, 0x1000).clipped_to(0x4000_0000), None);
        // Zero-length limit → everything dropped.
        assert_eq!(DmRange::new(0, 0x1000).clipped_to(0), None);
    }

    /// The bootstrap tree source (root page + bump) is expressible even
    /// though it never appears in a conventional memmap: explicit
    /// registration covers whatever page-aligned range it occupies,
    /// including sub-2 MiB fragments that degrade to 4 KiB units.
    #[test]
    fn test_bootstrap_tree_fragment_coverage() {
        // Root page at an arbitrary low page, bump right behind it.
        let root = collect(DmRange::new(0x9000_0000, 0x1000), true);
        let bump = collect(DmRange::new(0x9000_1000, 0x4000), true);
        assert_eq!(root, vec![(0x9000_0000, 4096)]);
        assert_eq!(
            bump,
            vec![
                (0x9000_1000, 4096),
                (0x9000_2000, 4096),
                (0x9000_3000, 4096),
                (0x9000_4000, 4096)
            ]
        );
    }

    // ── establish_dm_range driver tests (MockDmCoverage registry) ──
    // Gated on `mock` (not just `test`): they exercise the driver through
    // the registry-backed MockDmCoverage implementor.
    #[cfg(feature = "mock")]
    mod driver {
        use super::*;

        const MOCK_VM_BASE: u64 = 0x0000_0000_8000_0000;
        const MOCK_ROOT: u64 = 0x1000;

        /// End-to-end over the mock: two candidate ranges straddling the
        /// legacy hole produce leaves that (a) offset VA by the window base,
        /// (b) never cover the hole at any granularity, (c) carry the
        /// requested flags.
        #[test]
        fn test_establish_skips_hole_and_offsets_va() {
            use super::super::mock::{mock_dm_clear, mock_dm_leaves, MockDmCoverage};
            mock_dm_clear();
            let flags = PageFlags::read_write();
            establish_dm_range::<MockDmCoverage>(
                PhysBytes(MOCK_ROOT),
                DmRange::new(0, 0xA_0000),
                u64::MAX,
                MOCK_VM_BASE,
                flags,
            )
            .unwrap();
            establish_dm_range::<MockDmCoverage>(
                PhysBytes(MOCK_ROOT),
                DmRange::new(0x10_0000, 0x20_0000),
                u64::MAX,
                MOCK_VM_BASE,
                flags,
            )
            .unwrap();

            let leaves = mock_dm_leaves();
            assert!(!leaves.is_empty());
            for (&va, &(pa, size, f)) in leaves.iter() {
                assert_eq!(va, MOCK_VM_BASE + pa, "leaf VA must be window base + PA");
                assert_eq!(f, flags);
                let end = pa + size as u64;
                assert!(
                    end <= 0xA_0000 || pa >= 0x10_0000,
                    "leaf [{pa:#x}, {end:#x}) overlaps the legacy hole"
                );
            }
            // Low fragment [0, 0xA0000): unaligned → all 4 KiB leaves.
            assert_eq!(leaves[&MOCK_VM_BASE].1, 4096);
            assert_eq!(leaves[&(MOCK_VM_BASE + 0x9_0000)].1, 4096);
            // No leaf starts inside the hole.
            assert!(!leaves
                .keys()
                .any(|&va| (MOCK_VM_BASE + 0xA_0000..MOCK_VM_BASE + 0x10_0000).contains(&va)));
        }

        /// Window clipping through the driver: coverage stops at the window
        /// PA limit and VAs stay inside the window.
        #[test]
        fn test_establish_window_pa_limit() {
            use super::super::mock::{mock_dm_clear, mock_dm_leaves, MockDmCoverage};
            mock_dm_clear();
            let flags = PageFlags::read_write();
            // 1 GiB window: region [0x3000_0000, 0xB000_0000) clips to
            // [0x3000_0000, 0x4000_0000).
            establish_dm_range::<MockDmCoverage>(
                PhysBytes(MOCK_ROOT),
                DmRange::new(0x3000_0000, 0x8000_0000),
                0x4000_0000,
                MOCK_VM_BASE,
                flags,
            )
            .unwrap();
            let leaves = mock_dm_leaves();
            assert!(!leaves.is_empty());
            let min_pa = leaves.values().map(|&(pa, _, _)| pa).min().unwrap();
            let max_end = leaves
                .values()
                .map(|&(pa, size, _)| pa + size as u64)
                .max()
                .unwrap();
            assert_eq!(min_pa, 0x3000_0000);
            assert_eq!(max_end, 0x4000_0000);
            assert!(*leaves.keys().last().unwrap() < MOCK_VM_BASE + 0x4000_0000);
        }

        /// When the full window PA span is one candidate, page-size selection
        /// emits a single 1 GiB unit — the surface of the x86-64 whole-slot
        /// replacement branch (§6.1 v4.9 conditional).
        #[test]
        fn test_establish_one_gb_single_leaf() {
            use super::super::mock::{mock_dm_clear, mock_dm_leaves, MockDmCoverage};
            mock_dm_clear();
            let flags = PageFlags::read_write();
            establish_dm_range::<MockDmCoverage>(
                PhysBytes(MOCK_ROOT),
                DmRange::new(0, 1 << 30),
                1 << 30,
                MOCK_VM_BASE,
                flags,
            )
            .unwrap();
            let leaves = mock_dm_leaves();
            assert_eq!(leaves.len(), 1);
            assert_eq!(leaves[&MOCK_VM_BASE], (0, 1 << 30, flags));
        }

        /// Two overlapping establishment passes conflict on the shared leaf —
        /// `AlreadyMapped` propagates out of the driver instead of panicking.
        #[test]
        fn test_establish_conflict_propagates() {
            use super::super::mock::{mock_dm_clear, MockDmCoverage};
            mock_dm_clear();
            let flags = PageFlags::read_write();
            establish_dm_range::<MockDmCoverage>(
                PhysBytes(MOCK_ROOT),
                DmRange::new(0x10_0000, 0x20_0000),
                u64::MAX,
                MOCK_VM_BASE,
                flags,
            )
            .unwrap();
            let err = establish_dm_range::<MockDmCoverage>(
                PhysBytes(MOCK_ROOT),
                DmRange::new(0x10_0000, 0x20_0000),
                u64::MAX,
                MOCK_VM_BASE,
                flags,
            )
            .unwrap_err();
            assert_eq!(err, PageTableError::AlreadyMapped);
        }
    }
}
