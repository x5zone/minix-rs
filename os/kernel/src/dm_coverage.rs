//! Boot-time Direct Map coverage establishment (kernel boot path).
//!
//! Implements the coverage half of `07-paging_init_design` §6.1: after
//! paging is enabled on the bootstrap root, two DM windows are installed
//! into that same root —
//!
//! - **Kernel DM** first (supervisor RW, full PA span): the access backend
//!   for kernel-context `Paging` handles and general kernel PA access once
//!   the identity mapping is superseded.
//! - **VM DM** second (user RW, PA span clipped to `VM_DIRECT_MAP_SIZE`):
//!   the access backend for the VM server — self page-table walks, page
//!   zeroing, allocator metadata.
//!
//! Kernel DM is established first so that PA access to the superseded
//! identity range flows through the kernel channel from that point on
//! (§6.1 constraint ①).
//!
//! # Two-source candidate union
//!
//! Mapping candidates are the union of (§6.1 "映射候选的两源并集"):
//!
//! 1. **Resource-classified memmap ranges** — `KernelInfo.memmap`; the boot
//!    shim already restricts it to conventional RAM (the source of VM PMM
//!    RAM), so no additional type filtering happens here.
//! 2. **Explicit bootstrap PhysAccess ranges** — the VM self page-table
//!    tree: the root page and the boot bump region. Both are `LOADER_DATA`
//!    allocations that never appear in a conventional memmap; without
//!    explicit registration the self root could never become DM-covered
//!    (the §6.1 derivation gap).
//!
//! A source-2 candidate overlapping a memmap range (fallback/test paths
//! where the bump region lives inside conventional RAM) is dropped —
//! union semantics, no double mapping.
//!
//! All PTE writes go through the bootstrap root's identity write channel
//! (`VA = PA`, §6.1 "DM 建立的自举写通道闭环"); on x86-64 the VM DM window
//! overlaps `PDPTE[2]` and its split path rewrites only VA [2 GiB, 3 GiB)
//! translation, orthogonal to the `VA < 1 GiB` write channel.
//!
//! # Window capacity note (riscv64)
//!
//! The Sv39 kernel half spans 256 GiB and the kernel DM base sits 16 GiB
//! below its top, so the riscv64 kernel DM window bounds PA at 16 GiB.
//! Current platforms stay far below that bound (qemu virt RAM ≤ a few
//! GiB); the VM PMM eligibility clip (`VM_DIRECT_MAP_SIZE` = 16 GiB) caps
//! the managed side at the same limit.

use crate::boot_alloc;
use minix_arch::paging::PageFlags;
use minix_arch::{
    CurrentDirectMap, CurrentDmCoverage, DirectMapArch, DmRange, establish_dm_range,
};
use minix_boot::KernelInfo;
use minix_types::PhysBytes;

/// 4 KiB — the base leaf size of every current architecture.
const PAGE_SIZE: u64 = 4096;

/// Establish both DM windows on the bootstrap root (§6.1 D8-②).
///
/// Called once from `arch_boot_impl` after paging is enabled and before
/// any VM physical-memory access through the windows (e.g. VM ELF loading).
/// Establishment failures are boot-layout bugs (leaf conflicts cannot arise
/// from a single candidate pass into fresh window slots) — the kernel
/// refuses to start.
pub fn establish_boot_dm(kernel_info: &KernelInfo, root: PhysBytes) {
    let kernel_flags = PageFlags::kernel_read_write();
    let vm_flags = PageFlags::read_write();
    let kernel_va = CurrentDirectMap::KERNEL_DIRECT_MAP_BASE;
    let vm_va = CurrentDirectMap::VM_DIRECT_MAP_BASE;
    let vm_pa_limit = CurrentDirectMap::VM_DIRECT_MAP_SIZE;

    // Kernel DM first (§6.1 constraint ①), full PA span.
    for r in memmap_candidates(kernel_info) {
        establish_dm_range::<CurrentDmCoverage>(
            root, r, u64::MAX, kernel_va, kernel_flags,
        )
        .expect("boot DM: kernel window establishment failed");
    }
    for r in bootstrap_tree_candidates(kernel_info, root).into_iter().flatten() {
        establish_dm_range::<CurrentDmCoverage>(
            root, r, u64::MAX, kernel_va, kernel_flags,
        )
        .expect("boot DM: kernel window establishment failed");
    }

    // VM DM second, PA span clipped to the window; candidates beyond the
    // window stay outside DM representability (resource qualification
    // excludes them from VM PMM — §6.1 资格过滤).
    for r in memmap_candidates(kernel_info) {
        establish_dm_range::<CurrentDmCoverage>(root, r, vm_pa_limit, vm_va, vm_flags)
            .expect("boot DM: VM window establishment failed");
    }
    for r in bootstrap_tree_candidates(kernel_info, root).into_iter().flatten() {
        establish_dm_range::<CurrentDmCoverage>(root, r, vm_pa_limit, vm_va, vm_flags)
            .expect("boot DM: VM window establishment failed");
    }

    validate_bootstrap_tree(root);
}

/// Source 1: conventional RAM ranges (the boot shim already type-filtered).
fn memmap_candidates(kernel_info: &KernelInfo) -> impl Iterator<Item = DmRange> + '_ {
    kernel_info
        .memmap()
        .iter()
        .map(|r| DmRange::new(r.base.0, r.len as u64))
}

/// Source 2: the bootstrap page-table tree — root page + boot bump region.
///
/// Ranges overlapping a memmap candidate are dropped (union semantics:
/// the fallback/test paths allocate the bump region inside conventional
/// RAM, where source 1 already maps every page).
///
/// When the bump region contains the root page (the root is allocated from
/// the bump), the two source-2 candidates overlap; the bump candidate is
/// front-trimmed past the root so the root leaf installs exactly once.
fn bootstrap_tree_candidates(kernel_info: &KernelInfo, root: PhysBytes) -> [Option<DmRange>; 2] {
    let in_memmap =
        |r: &DmRange| memmap_candidates(kernel_info).any(|m| r.base < m.base + m.len && m.base < r.base + r.len);

    let root_range = DmRange::new(root.0, PAGE_SIZE);
    let root_end = root.0 + PAGE_SIZE;
    let bump_range = boot_alloc::boot_alloc_region()
        .map(|(base, end)| {
            let start = core::cmp::max(base, root_end);
            DmRange::new(start, end - start)
        })
        .filter(|r| r.len > 0);

    let mut out = [None, None];
    if !in_memmap(&root_range) {
        out[0] = Some(root_range);
    }
    if let Some(r) = bump_range.filter(|r| !in_memmap(r)) {
        out[1] = Some(r);
    }
    out
}

/// Boot-time validation (§6.1 资格过滤 ②): the bootstrap tree must be
/// representable in both windows — the identity write channel requires
/// `PA < IDENTITY_MAP_END`, and VM self page-table walks read/write the
/// root and PT pages through the VM DM window. Violation refuses boot.
///
/// The bound is the same per-arch minimum the boot shim must allocate
/// under (`min(IDENTITY_MAP_END, VM DM window PA end)`); asserting it here
/// keeps the kernel side independent of shim-side enforcement.
fn validate_bootstrap_tree(root: PhysBytes) {
    let bound = core::cmp::min(IDENTITY_MAP_END, CurrentDirectMap::VM_DIRECT_MAP_SIZE);
    assert!(
        root.0.checked_add(PAGE_SIZE).is_some_and(|end| end <= bound),
        "boot DM: bootstrap root {:#x} outside DM-admissible bound {:#x}",
        root.0,
        bound
    );
    if let Some((base, end)) = boot_alloc::boot_alloc_region() {
        assert!(
            end <= bound,
            "boot DM: boot bump region [{base:#x}, {end:#x}) outside DM-admissible bound {bound:#x}"
        );
    }
}

use crate::IDENTITY_MAP_END;

#[cfg(test)]
mod tests {
    use super::*;
    use minix_boot::{KernelInfo, MemoryRegion};
    use minix_types::VirBytes;

    /// Mock-mode regression: establishment over a synthetic memmap runs
    /// through the registry-backed MockDmCoverage without touching real
    /// tables, and the union drops the bump candidate that sits inside a
    /// memmap range.
    #[test]
    #[cfg(feature = "mock")]
    fn test_union_drops_in_memmap_bump() {
        // Mutates BOOT_ALLOC (process-global) — serialize with the boot-flow
        // tests that read it (`crate::test_sync::lock_boot_globals`).
        let _boot = crate::test_sync::lock_boot_globals();
        use minix_boot::MemoryRegion;

        let info = KernelInfo {
            memmap: &[MemoryRegion { base: PhysBytes(0x10_0000), len: 0x100_0000 }],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: minix_types::VirBytes(0x7fff_ffff_f000),
            kern_stack_top: minix_types::VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: minix_types::VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        // Bump inside the memmap range → dropped by union semantics.
        boot_alloc::init_boot_pt_alloc(0x20_0000, 0x21_0000);
        let cands = bootstrap_tree_candidates(&info, PhysBytes(0x11_0000));
        assert!(cands[0].is_none(), "root inside memmap must be dropped");
        assert!(cands[1].is_none(), "bump inside memmap must be dropped");

        // Bump outside any memmap range → kept as an explicit candidate.
        boot_alloc::init_boot_pt_alloc(0x9000_0000, 0x9000_4000);
        let cands = bootstrap_tree_candidates(&info, PhysBytes(0x9000_0000));
        assert_eq!(cands[0], Some(DmRange::new(0x9000_0000, PAGE_SIZE)));
        assert_eq!(cands[1], Some(DmRange::new(0x9000_0000 + PAGE_SIZE, 0x3000)));
    }

    // ── Boot-flow integration: arch_boot_impl Step 4 establishes DM coverage ──

    /// Boot-shim contract simulation: the bump region must sit inside the
    /// DM-admissible bound `min(IDENTITY_MAP_END, VM window)`; the aarch64
    /// memmap base (0x4000_0000) is at/above the 1 GiB mock VM window, so
    /// the kernel fallback would be non-admissible and the shim must
    /// pre-register an in-bound region (Phase 2.5).
    fn boot_dm_test_preconditions() {
        boot_alloc::init_boot_pt_alloc(0x2000, 0x100_0000);
    }

    #[test]
    #[cfg(feature = "mock")]
    fn test_boot_dm_x86_64_style_params() {
        let _boot = crate::test_sync::lock_boot_globals();
        let info = KernelInfo {
            memmap: &[MemoryRegion { base: PhysBytes(0x100000), len: 0x1000000 }],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000,
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        let info_ref = crate::arch_boot_impl::<minix_arch::paging::mock::MockPaging>(&info, PhysBytes(0x1000));
        assert_eq!(info_ref.kern_virt_base.0, info.kern_virt_base.0,
            "returned KernelInfo should match input");
    }

    #[test]
    #[cfg(feature = "mock")]
    fn test_boot_dm_aarch64_style_params() {
        let _boot = crate::test_sync::lock_boot_globals();
        boot_dm_test_preconditions();
        let info = KernelInfo {
            memmap: &[MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x800_0000 }],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x4020_0000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: VirBytes(0x0000_7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        let info_ref = crate::arch_boot_impl::<minix_arch::paging::mock::MockPaging>(&info, PhysBytes(0x1000));
        assert_eq!(info_ref.kern_phys_base.0, 0x4020_0000);
    }

    #[test]
    #[cfg(feature = "mock")]
    fn test_boot_dm_riscv64_style_params() {
        let _boot = crate::test_sync::lock_boot_globals();
        boot_dm_test_preconditions();
        let info = KernelInfo {
            memmap: &[MemoryRegion { base: PhysBytes(0x8000_0000), len: 0x800_0000 }],
            kern_virt_base: VirBytes(0xFFFF_8000_0800_0000),
            kern_phys_base: PhysBytes(0x8000_0000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: VirBytes(0x0000_003f_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0800_0000 + 0x200_000),
            syscall_entry: VirBytes(0xFFFF_8000_0800_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        };
        let info_ref = crate::arch_boot_impl::<minix_arch::paging::mock::MockPaging>(&info, PhysBytes(0x1000));
        assert_eq!(info_ref.kern_phys_base.0, 0x8000_0000);
    }
}
