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
//! The mapping candidate set is a two-source union (`07-paging_init_design`
//! §6.1):
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

use super::boot_arch::BootArch;
use super::direct_map::{DirectMapArch, DirectMapLayout, DmLayoutError};
use super::paging::{PageFlags, PageTableError};
use super::paging_ext::{PageSize, PageSupportError};
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

/// Pure Direct Map planning: validated capability facts plus layout.
///
/// The policy answers "which leaf sizes may this boot use, and inside which
/// window", but performs no table access. Windows are derived from the
/// layout the policy carries, so the planner and the installer cannot
/// disagree about the address range.
pub struct DmPagePolicy<A: BootArch> {
    preferred_page_size: Option<PageSize>,
    fallback_page_size: PageSize,
    base_page_size: PageSize,
    phys_addr_limit: u64,
    layout: DirectMapLayout<A>,
}

impl<A: BootArch> Copy for DmPagePolicy<A> {}

impl<A: BootArch> Clone for DmPagePolicy<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: BootArch> core::fmt::Debug for DmPagePolicy<A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DmPagePolicy")
            .field("preferred_page_size", &self.preferred_page_size)
            .field("fallback_page_size", &self.fallback_page_size)
            .field("base_page_size", &self.base_page_size)
            .field("phys_addr_limit", &self.phys_addr_limit)
            .finish()
    }
}

impl<A: BootArch> DmPagePolicy<A> {
    pub const fn layout(&self) -> DirectMapLayout<A> {
        self.layout
    }

    pub const fn preferred_page_size(&self) -> Option<PageSize> {
        self.preferred_page_size
    }

    pub const fn fallback_page_size(&self) -> PageSize {
        self.fallback_page_size
    }

    pub const fn base_page_size(&self) -> PageSize {
        self.base_page_size
    }

    pub const fn phys_addr_limit(&self) -> u64 {
        self.phys_addr_limit
    }

    pub fn kernel_window(&self) -> DmWindow<A> {
        DmWindow::kernel(&self.layout)
    }

    pub fn vm_window(&self) -> DmWindow<A> {
        DmWindow::vm(&self.layout)
    }
}

/// One validated Direct Map window derived from a [`DmPagePolicy`].
pub struct DmWindow<A: BootArch> {
    va_base: u64,
    pa_capacity: u64,
    va_end: u64,
    flags: PageFlags,
    _arch: core::marker::PhantomData<fn() -> A>,
}

impl<A: BootArch> Copy for DmWindow<A> {}

impl<A: BootArch> Clone for DmWindow<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: BootArch> DmWindow<A> {
    fn kernel(layout: &DirectMapLayout<A>) -> Self {
        Self {
            va_base: layout.kernel_base(),
            pa_capacity: layout.kernel_pa_capacity(),
            va_end: layout.kernel_va_end(),
            flags: PageFlags::kernel_read_write(),
            _arch: core::marker::PhantomData,
        }
    }

    fn vm(layout: &DirectMapLayout<A>) -> Self {
        Self {
            va_base: layout.vm_base(),
            pa_capacity: layout.vm_pa_capacity(),
            va_end: layout.vm_va_end(),
            flags: PageFlags::read_write(),
            _arch: core::marker::PhantomData,
        }
    }

    pub const fn va_base(&self) -> u64 {
        self.va_base
    }

    pub const fn pa_capacity(&self) -> u64 {
        self.pa_capacity
    }

    pub const fn va_end(&self) -> u64 {
        self.va_end
    }

    pub const fn flags(&self) -> PageFlags {
        self.flags
    }
}

/// Direct Map planning errors; each variant answers only this layer's
/// question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmPolicyError {
    /// The capability snapshot's static page-size configuration is invalid.
    InvalidSupport(PageSupportError),
    /// The layout disagrees with the architecture constants or is
    /// geometrically inconsistent.
    InvalidDmLayout,
    /// The fallback size is not aligned to one of the window bases.
    InvalidDmAlignment,
    /// The fallback size is not supported by this platform.
    FallbackUnsupported(PageSize),
}

impl core::fmt::Display for DmPolicyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidSupport(e) => write!(f, "invalid page-size support: {e}"),
            Self::InvalidDmLayout => write!(f, "invalid direct-map layout"),
            Self::InvalidDmAlignment => write!(f, "direct-map base not aligned to fallback page"),
            Self::FallbackUnsupported(size) => {
                write!(f, "fallback page size {} is not supported", size.bytes())
            }
        }
    }
}

/// Turn capability facts into a Direct Map policy. No hardware query and no
/// page-table access: the fallback size is a hard prerequisite, the preferred
/// size degrades to `None` on either missing hardware support or base
/// misalignment.
pub fn plan_dm_policy<A: BootArch>(
    support: &super::paging_ext::BootPageSupport<A>,
    layout: &DirectMapLayout<A>,
) -> Result<DmPagePolicy<A>, DmPolicyError> {
    support
        .validate()
        .map_err(DmPolicyError::InvalidSupport)?;
    layout
        .validate()
        .map_err(|DmLayoutError::InvalidLayout| DmPolicyError::InvalidDmLayout)?;

    let preferred = support.preferred();
    let fallback = support.fallback();
    let base = support.base_page_size();

    // The project supports platforms with at least the fallback size. Every
    // installer can write base-page leaves, so this is a support-policy hard
    // prerequisite, not a mechanism limitation.
    if !fallback.supported() {
        return Err(DmPolicyError::FallbackUnsupported(fallback.size()));
    }
    if layout.kernel_base() % fallback.size().bytes() != 0
        || layout.vm_base() % fallback.size().bytes() != 0
    {
        return Err(DmPolicyError::InvalidDmAlignment);
    }

    let preferred_size = if !preferred.supported()
        || layout.kernel_base() % preferred.size().bytes() != 0
        || layout.vm_base() % preferred.size().bytes() != 0
    {
        None
    } else {
        Some(preferred.size())
    };

    Ok(DmPagePolicy {
        preferred_page_size: preferred_size,
        fallback_page_size: fallback.size(),
        base_page_size: base,
        phys_addr_limit: support.phys_addr_limit(),
        layout: *layout,
    })
}

/// Greedy page-size selection over one candidate range.
///
/// Walks the range from base to end, emitting `(pa, size)` units with the
/// largest size whose alignment and remaining length permit: the policy's
/// preferred huge size (when present), then its fallback huge size, then the
/// base page. Every unit lies entirely inside `range`, which is the
/// resource-containment guarantee: a sub-range (hole) that is not part of
/// any candidate never appears in a unit, at any granularity.
///
/// The emit closure is fallible so the installation driver can propagate a
/// leaf conflict (`AlreadyMapped` — a boot-layout bug per the
/// [`BootDmInstaller`] contract) instead of panicking mid-establishment.
pub fn plan_range_units<A: BootArch, F: FnMut(u64, PageSize) -> Result<(), PageTableError>>(
    range: DmRange,
    policy: &DmPagePolicy<A>,
    mut emit: F,
) -> Result<(), PageTableError> {
    let base = policy.base_page_size().bytes();
    if range.base % base != 0 || range.len % base != 0 {
        return Err(PageTableError::InvalidAddress);
    }
    let end = range
        .base
        .checked_add(range.len)
        .ok_or(PageTableError::InvalidAddress)?;
    if range.len == 0 {
        return Ok(());
    }

    let preferred = policy.preferred_page_size();
    let fallback = policy.fallback_page_size();
    let mut cur = range.base;
    while cur < end {
        let remaining = end - cur;
        let unit = preferred
            .filter(|size| cur.is_multiple_of(size.bytes()) && remaining >= size.bytes())
            .or_else(|| {
                (cur.is_multiple_of(fallback.bytes()) && remaining >= fallback.bytes())
                    .then_some(fallback)
            })
            .unwrap_or(policy.base_page_size());

        let unit_bytes = unit.bytes();
        debug_assert!(unit_bytes > 0 && cur.is_multiple_of(unit_bytes));
        emit(cur, unit)?;
        cur = cur
            .checked_add(unit_bytes)
            .ok_or(PageTableError::InvalidAddress)?;
    }
    Ok(())
}

/// Per-architecture installation of one DM leaf via the boot identity write
/// channel.
///
/// The shared driver ([`plan_range_units`] + [`install_boot_dm_range`]) performs
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
pub trait BootDmInstaller {
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
    unsafe fn install_leaf(
        root_paddr: PhysBytes,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: PageSize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;
}

fn same_window<A: BootArch>(a: DmWindow<A>, b: DmWindow<A>) -> bool {
    a.va_base() == b.va_base()
        && a.pa_capacity() == b.pa_capacity()
        && a.va_end() == b.va_end()
        && a.flags() == b.flags()
}

/// Read-only preflight for one candidate range.
///
/// Checks the window, range endpoints, capacity, physical-address bound, and
/// every leaf the planner will emit — without touching a page table. All
/// geometric failures therefore surface before the first leaf write.
pub fn preflight_boot_dm_range<A: BootArch>(
    range: DmRange,
    window: DmWindow<A>,
    policy: &DmPagePolicy<A>,
) -> Result<(), PageTableError> {
    if !same_window(window, policy.kernel_window())
        && !same_window(window, policy.vm_window())
    {
        return Err(PageTableError::InvalidAddress);
    }
    if window
        .va_base()
        .checked_add(window.pa_capacity())
        != Some(window.va_end())
    {
        return Err(PageTableError::InvalidAddress);
    }
    let base = policy.base_page_size().bytes();
    if range.base % base != 0 || range.len % base != 0 {
        return Err(PageTableError::InvalidAddress);
    }
    let range_end = range
        .base
        .checked_add(range.len)
        .ok_or(PageTableError::InvalidAddress)?;
    if range_end > window.pa_capacity() || range_end > policy.phys_addr_limit() {
        return Err(PageTableError::InvalidAddress);
    }

    plan_range_units(range, policy, |pa, size| {
        let bytes = size.bytes();
        let va = window
            .va_base()
            .checked_add(pa)
            .ok_or(PageTableError::InvalidAddress)?;
        let pa_end = pa
            .checked_add(bytes)
            .ok_or(PageTableError::InvalidAddress)?;
        let va_end = va
            .checked_add(bytes)
            .ok_or(PageTableError::InvalidAddress)?;
        if pa % bytes != 0
            || va % bytes != 0
            || pa_end > range_end
            || pa_end > window.pa_capacity()
            || va_end > window.va_end()
        {
            return Err(PageTableError::InvalidAddress);
        }
        Ok(())
    })
}

/// Install a candidate range after [`preflight_boot_dm_range`] has passed.
///
/// This function performs no address validation of its own: the caller owns
/// the geometry proof and the boot-context proof.
///
/// # Safety
///
/// - `root` is a valid, page-aligned bootstrap root reachable through the
///   boot identity write channel, and paging is active on it.
/// - Single-threaded boot context; no concurrent table walkers.
/// - `window` came from the same `policy`; `range` is already clipped.
/// - Every page-table page (including intermediate pages allocated here) is
///   inside the identity-mapped writable range.
pub unsafe fn install_prevalidated_boot_dm_range<A: BootArch>(
    root: PhysBytes,
    range: DmRange,
    window: DmWindow<A>,
    policy: &DmPagePolicy<A>,
) -> Result<(), PageTableError> {
    plan_range_units(range, policy, |pa, size| {
        let va = window
            .va_base()
            .checked_add(pa)
            .ok_or(PageTableError::InvalidAddress)?;
        // SAFETY: caller satisfied the boot-context contract; geometry was
        // checked by `preflight_boot_dm_range`; sizes come from the policy.
        unsafe {
            A::DmInstaller::install_leaf(
                root,
                VirBytes(va),
                PhysBytes(pa),
                size,
                window.flags(),
            )
        }
    })
}

/// Preflight and install one Direct Map candidate range.
///
/// # Safety
///
/// Same contract as [`install_prevalidated_boot_dm_range`]. A partial write
/// on an underlying install error is fatal: the caller must terminate boot,
/// never continue with a partially established Direct Map.
pub unsafe fn install_boot_dm_range<A: BootArch>(
    root: PhysBytes,
    range: DmRange,
    window: DmWindow<A>,
    policy: &DmPagePolicy<A>,
) -> Result<(), PageTableError> {
    preflight_boot_dm_range(range, window, policy)?;
    // SAFETY: preflight passed; the caller owns the boot-context contract.
    unsafe { install_prevalidated_boot_dm_range(root, range, window, policy) }
}

/// Software model of DM coverage establishment for host tests.
///
/// `MockBootDmInstaller` records every leaf in a global registry instead of
/// touching real page tables, so `install_boot_dm_range` drivers (and later the
/// kernel boot path's two-source candidate assembly) can be tested for hole
/// exclusion, window clipping, VA offsetting and page-size selection without
/// hardware.
#[cfg(feature = "runtime-window")]
pub mod mock {
    use super::{BootDmInstaller, PageFlags, PageSize, PageTableError};
    use minix_types::{PhysBytes, VirBytes};
    use std::collections::BTreeMap;
    use std::sync::{Mutex, OnceLock};

    /// One recorded leaf: `(paddr, size, flags)`, keyed by leaf VA base.
    pub type MockDmLeaf = (u64, usize, PageFlags);

    fn registry() -> &'static Mutex<BTreeMap<u64, MockDmLeaf>> {
        static REGISTRY: OnceLock<Mutex<BTreeMap<u64, MockDmLeaf>>> = OnceLock::new();
        REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    /// Registry-backed implementor of [`BootDmInstaller`].
    pub struct MockBootDmInstaller;

    impl BootDmInstaller for MockBootDmInstaller {
        unsafe fn install_leaf(
            _root_paddr: PhysBytes,
            vaddr: VirBytes,
            paddr: PhysBytes,
            size: PageSize,
            flags: PageFlags,
        ) -> Result<(), PageTableError> {
            let size = size
                .try_as_usize()
                .ok_or(PageTableError::InvalidAddress)?;
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

#[cfg(all(test, feature = "runtime-window"))]
mod tests {
    use super::*;
    use crate::arch::boot_arch::MockBootArch;
    use crate::arch::direct_map::DirectMapLayout;
    use crate::arch::paging_ext::{BootPageSupport, PageSupport};
    use std::sync::{Mutex, MutexGuard};

    const MOCK_VM_BASE: u64 = 0x0000_0000_8000_0000;
    const MOCK_ROOT: u64 = 0x1000;

    fn mock_policy_with(preferred_supported: bool, phys_limit: u64) -> DmPagePolicy<MockBootArch> {
        let support = BootPageSupport::<MockBootArch>::from_parts(
            PageSize::KIB_4,
            phys_limit,
            PageSupport::new(PageSize::GIB_1, preferred_supported),
            PageSupport::new(PageSize::MIB_2, true),
        );
        let layout = DirectMapLayout::<MockBootArch>::from_arch();
        plan_dm_policy::<MockBootArch>(&support, &layout).unwrap()
    }

    fn mock_policy() -> DmPagePolicy<MockBootArch> {
        mock_policy_with(true, 1 << 52)
    }

    fn collect(range: DmRange, prefer_1g: bool) -> Vec<(u64, PageSize)> {
        let policy = mock_policy_with(prefer_1g, 1 << 52);
        let mut units = Vec::new();
        plan_range_units(range, &policy, |pa, size| {
            units.push((pa, size));
            Ok(())
        })
        .unwrap();
        units
    }

    fn covered(units: &[(u64, PageSize)]) -> Vec<(u64, u64)> {
        units
            .iter()
            .map(|&(pa, s)| (pa, pa + s.bytes()))
            .collect()
    }

    /// A 2 MiB-aligned conventional region maps as 2 MiB huge pages.
    #[test]
    fn test_two_mb_region_uses_two_mb_units() {
        let units = collect(DmRange::new(0x20_0000, 0x40_0000), false);
        assert_eq!(
            units,
            vec![
                (0x20_0000, PageSize::MIB_2),
                (0x40_0000, PageSize::MIB_2)
            ]
        );
    }

    /// Unaligned head and tail degrade to 4 KiB units; the aligned middle
    /// stays huge — a hole inside the unaligned fragments is structurally
    /// unmapped because only candidate ranges are walked.
    #[test]
    fn test_unaligned_fragments_degrade_to_4k() {
        // [1 MiB, 5 MiB): 4K for [1M, 2M), 2M for [2M, 4M), 4K for [4M, 5M).
        let units = collect(DmRange::new(0x10_0000, 0x40_0000), false);
        assert_eq!(units.first().copied(), Some((0x10_0000, PageSize::KIB_4)));
        assert_eq!(*units.last().unwrap(), (0x4F_F000, PageSize::KIB_4));
        assert_eq!(units.len(), 256 + 1 + 256);
        assert_eq!(units[256], (0x20_0000, PageSize::MIB_2));
        assert_eq!(units[257], (0x40_0000, PageSize::KIB_4));
    }

    /// Gate 8 Case E (mechanism level): a reserved hole between two
    /// conventional ranges is covered by NO unit at any granularity.
    #[test]
    fn test_legacy_hole_never_swallowed() {
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
        let units = collect(DmRange::new(0x4000_0000, 0x1_4000_0000), true);
        assert_eq!(
            units,
            vec![
                (0x4000_0000, PageSize::GIB_1),
                (0x8000_0000, PageSize::GIB_1),
                (0xC000_0000, PageSize::GIB_1),
                (0x1_0000_0000, PageSize::GIB_1),
                (0x1_4000_0000, PageSize::GIB_1),
            ]
        );
        let units = collect(DmRange::new(0x4000_0000, 0x400_0000), false);
        assert_eq!(units.len(), 32);
        assert_eq!(units[0], (0x4000_0000, PageSize::MIB_2));
        assert_eq!(units[1], (0x4020_0000, PageSize::MIB_2));

        let units = collect(DmRange::new(0x8000_0000 - 0x1000, 0x2000 + (1 << 30)), true);
        assert!(units.contains(&(0x8000_0000, PageSize::GIB_1)));
        assert_eq!(
            units.first().copied(),
            Some((0x8000_0000 - 0x1000, PageSize::KIB_4))
        );
    }

    /// The planner itself rejects unaligned and overflowing ranges instead of
    /// silently emitting base-page leaves for them.
    #[test]
    fn test_planner_rejects_invalid_geometry() {
        let policy = mock_policy();
        let mut emitted = 0;
        let err = plan_range_units(DmRange::new(0x1000, 0x1001), &policy, |_, _| {
            emitted += 1;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err, PageTableError::InvalidAddress);
        assert_eq!(emitted, 0);

        let err = plan_range_units(DmRange::new(u64::MAX - 0xFFF, 0x2000), &policy, |_, _| {
            emitted += 1;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err, PageTableError::InvalidAddress);
        assert_eq!(emitted, 0);

        plan_range_units(DmRange::new(0x1000, 0), &policy, |_, _| {
            emitted += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(emitted, 0);
    }

    /// The preflight and the install pass must walk the same deterministic
    /// candidate sequence.
    #[test]
    fn test_planner_sequence_is_deterministic() {
        let range = DmRange::new(0x10_0000, 0x31_0000);
        assert_eq!(collect(range, true), collect(range, true));
    }

    /// Window clipping: coverage stops at the window's PA limit; ranges
    /// beyond it are dropped entirely.
    #[test]
    fn test_window_clipping() {
        let range = DmRange::new(0x3000_0000, 0x8000_0000)
            .clipped_to(0x4000_0000)
            .unwrap();
        assert_eq!(range, DmRange::new(0x3000_0000, 0x1000_0000));
        assert_eq!(
            DmRange::new(0x5000_0000, 0x1000).clipped_to(0x4000_0000),
            None
        );
        assert_eq!(DmRange::new(0, 0x1000).clipped_to(0), None);
    }

    /// The bootstrap tree source (root page + bump) is expressible even
    /// though it never appears in a conventional memmap.
    #[test]
    fn test_bootstrap_tree_fragment_coverage() {
        let root = collect(DmRange::new(0x9000_0000, 0x1000), true);
        let bump = collect(DmRange::new(0x9000_1000, 0x4000), true);
        assert_eq!(root, vec![(0x9000_0000, PageSize::KIB_4)]);
        assert_eq!(
            bump,
            vec![
                (0x9000_1000, PageSize::KIB_4),
                (0x9000_2000, PageSize::KIB_4),
                (0x9000_3000, PageSize::KIB_4),
                (0x9000_4000, PageSize::KIB_4)
            ]
        );
    }

    // ── install driver tests (MockBootDmInstaller registry) ──
    // The mock registry serializes single operations, but each test's
    // clear → install → assert sequence is only meaningful as a whole.
    static SEQ_LOCK: Mutex<()> = Mutex::new(());

    fn driver_seq_lock() -> MutexGuard<'static, ()> {
        SEQ_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Install one range into the VM window, clipping first exactly as the
    /// boot candidate assembly does.
    fn install_vm(policy: &DmPagePolicy<MockBootArch>, range: DmRange) -> Result<(), PageTableError> {
        let window = policy.vm_window();
        let Some(clipped) = range.clipped_to(window.pa_capacity()) else {
            return Ok(());
        };
        preflight_boot_dm_range(clipped, window, policy)?;
        // SAFETY: host test mock records leaves; no page tables are touched.
        unsafe {
            install_prevalidated_boot_dm_range(PhysBytes(MOCK_ROOT), clipped, window, policy)
        }
    }

    /// End-to-end over the mock: two candidate ranges straddling the legacy
    /// hole produce leaves that (a) offset VA by the window base, (b) never
    /// cover the hole at any granularity, (c) carry the window flags.
    #[test]
    fn test_establish_skips_hole_and_offsets_va() {
        use super::mock::{mock_dm_clear, mock_dm_leaves};
        let _seq = driver_seq_lock();
        mock_dm_clear();
        let policy = mock_policy();
        install_vm(&policy, DmRange::new(0, 0xA_0000)).unwrap();
        install_vm(&policy, DmRange::new(0x10_0000, 0x20_0000)).unwrap();

        let leaves = mock_dm_leaves();
        assert!(!leaves.is_empty());
        for (&va, &(pa, size, f)) in leaves.iter() {
            assert_eq!(va, MOCK_VM_BASE + pa, "leaf VA must be window base + PA");
            assert_eq!(f, policy.vm_window().flags());
            let end = pa + size as u64;
            assert!(
                end <= 0xA_0000 || pa >= 0x10_0000,
                "leaf [{pa:#x}, {end:#x}) overlaps the legacy hole"
            );
        }
        assert_eq!(leaves[&MOCK_VM_BASE].1, 4096);
        assert_eq!(leaves[&(MOCK_VM_BASE + 0x9_0000)].1, 4096);
        assert!(!leaves
            .keys()
            .any(|&va| (MOCK_VM_BASE + 0xA_0000..MOCK_VM_BASE + 0x10_0000).contains(&va)));
    }

    /// Window clipping through the driver: coverage stops at the window
    /// PA limit and VAs stay inside the window.
    #[test]
    fn test_establish_window_pa_limit() {
        use super::mock::{mock_dm_clear, mock_dm_leaves};
        let _seq = driver_seq_lock();
        mock_dm_clear();
        let policy = mock_policy();
        install_vm(&policy, DmRange::new(0x3000_0000, 0x8000_0000)).unwrap();
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
    /// emits a single 1 GiB unit.
    #[test]
    fn test_establish_one_gb_single_leaf() {
        use super::mock::{mock_dm_clear, mock_dm_leaves};
        let _seq = driver_seq_lock();
        mock_dm_clear();
        let policy = mock_policy();
        install_vm(&policy, DmRange::new(0, 1 << 30)).unwrap();
        let leaves = mock_dm_leaves();
        assert_eq!(leaves.len(), 1);
        assert_eq!(
            leaves[&MOCK_VM_BASE],
            (0, 1 << 30, policy.vm_window().flags())
        );
    }

    /// Two overlapping establishment passes conflict on the shared leaf —
    /// `AlreadyMapped` propagates out of the driver instead of panicking.
    #[test]
    fn test_establish_conflict_propagates() {
        use super::mock::mock_dm_clear;
        let _seq = driver_seq_lock();
        mock_dm_clear();
        let policy = mock_policy();
        install_vm(&policy, DmRange::new(0x10_0000, 0x20_0000)).unwrap();
        let err = install_vm(&policy, DmRange::new(0x10_0000, 0x20_0000)).unwrap_err();
        assert_eq!(err, PageTableError::AlreadyMapped);
    }

    /// Preflight rejects geometry errors before any leaf is written: a range
    /// past the window capacity, a range past the physical-address bound, and
    /// an unaligned range all leave the registry empty.
    #[test]
    fn test_preflight_writes_nothing_on_geometry_errors() {
        use super::mock::{mock_dm_clear, mock_dm_leaves};
        let _seq = driver_seq_lock();
        mock_dm_clear();
        let policy = mock_policy();
        let window = policy.vm_window();

        let over_capacity = DmRange::new(window.pa_capacity(), 0x1000);
        assert_eq!(
            preflight_boot_dm_range(over_capacity, window, &policy),
            Err(PageTableError::InvalidAddress)
        );
        assert!(mock_dm_leaves().is_empty());

        let unaligned = DmRange::new(0x1000, 0x2001);
        assert_eq!(
            preflight_boot_dm_range(unaligned, window, &policy),
            Err(PageTableError::InvalidAddress)
        );
        assert!(mock_dm_leaves().is_empty());

        let tight = mock_policy_with(true, 0x1000);
        let over_phys = DmRange::new(0, 0x2000);
        assert_eq!(
            preflight_boot_dm_range(over_phys, tight.vm_window(), &tight),
            Err(PageTableError::InvalidAddress)
        );
        assert!(mock_dm_leaves().is_empty());
    }
}
