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
    for r in boot_module_candidates(kernel_info, root) {
        establish_dm_range::<CurrentDmCoverage>(root, r, vm_pa_limit, vm_va, vm_flags)
            .expect("boot DM: VM window module coverage failed");
        establish_dm_range::<CurrentDmCoverage>(
            root, r, u64::MAX, kernel_va, kernel_flags,
        )
        .expect("boot DM: kernel window module coverage failed");
    }

    // NK4-C 第 27 轮取证探针（task1-close 裁决删除）：打印 VM DM 窗口的
    // 实际覆盖清单（三源候选、窗口裁剪后的有效范围）。第 18 轮层级 dump
    // 实锤故障 PT 页落在低内存 PA（lvl2=0x7d027 / lvl1=0），本探针裁决
    // 该页是否在 VM 窗口覆盖内——不在则 VM 的 PTE 写静默丢失（refault
    // 循环的直接机制）。mock 门：宿主测试无端口 I/O 控制台（其余探针
    // 同一惯例），不带门 = 测试进程 SIGSEGV。
    #[cfg(not(feature = "mock"))]
    {
        use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
        C0::write_str("nk4a: dm-cov root=");
        C0::write_hex(root.0);
        C0::write_str(" vm_pa_limit=");
        C0::write_hex(vm_pa_limit);
        if let Some((base, end)) = boot_alloc::boot_alloc_region() {
            C0::write_str(" bump=[");
            C0::write_hex(base);
            C0::write_str(",");
            C0::write_hex(end);
            C0::write_str(")");
        }
        C0::write_str("\n");
        for r in memmap_candidates(kernel_info) {
            if let Some(c) = r.clipped_to(vm_pa_limit) {
                C0::write_str("nk4a: dm-mem [");
                C0::write_hex(c.base);
                C0::write_str(",");
                C0::write_hex(c.base + c.len);
                C0::write_str(")\n");
            }
        }
        for r in bootstrap_tree_candidates(kernel_info, root).into_iter().flatten() {
            if let Some(c) = r.clipped_to(vm_pa_limit) {
                C0::write_str("nk4a: dm-bump [");
                C0::write_hex(c.base);
                C0::write_str(",");
                C0::write_hex(c.base + c.len);
                C0::write_str(")\n");
            }
        }
        for r in boot_module_candidates(kernel_info, root) {
            if let Some(c) = r.clipped_to(vm_pa_limit) {
                C0::write_str("nk4a: dm-mod [");
                C0::write_hex(c.base);
                C0::write_str(",");
                C0::write_hex(c.base + c.len);
                C0::write_str(")\n");
            }
        }
    }

    validate_bootstrap_tree(root);
}
/// PA the per-process window mapping must reach; fix26, handoff v5).
///
/// Reuses exactly the candidate set [`establish_boot_dm`] installs into
/// the kernel window (unlimited PA bound): memmap ∪ bootstrap tree ∪
/// boot modules. `build_vm_handoff` derives `kern_dm_pages` from this so
/// VM's `map_kernel` covers what the kernel actually accesses through
/// its Direct Map — the previous hardcoded 4-page sentinel left every
/// address above 16 KiB not-present after the first CR3 switch.
///
/// Returned value is page-rounded up (a `map()`-ready page count times
/// [`PAGE_SIZE`]); 0 only if there are no candidates at all (no real
/// boot shape). The contiguous mapping `[0, end)` VM builds is a
/// superset of the (possibly non-contiguous) candidate union — holes
/// gain supervisor-only translations to unbacked addresses nobody
/// touches.
pub fn kernel_dm_pa_end(kernel_info: &KernelInfo, root: PhysBytes) -> u64 {
    let mut max_end = 0u64;
    let mut note = |r: DmRange| {
        max_end = max_end.max(r.base.saturating_add(r.len));
    };
    for r in memmap_candidates(kernel_info) {
        note(r);
    }
    for r in bootstrap_tree_candidates(kernel_info, root).into_iter().flatten() {
        note(r);
    }
    for r in boot_module_candidates(kernel_info, root) {
        note(r);
    }
    max_end.div_ceil(PAGE_SIZE) * PAGE_SIZE
}

/// Source 3: boot module blobs (`boot-shim: 12 boot modules loaded`).
///
/// The shim loads each module via `allocate_pages(LOADER_DATA)`, so the
/// final pre-EBS conventional-only memmap snapshot excludes those pages
/// (that is exactly how the VM PMM never hands out blob memory). But the
/// VM's `exec_bootproc` reads each image straight through the VM Direct
/// Map (`vm_server.rs` "no copy needed, unlike C") — an un-covered blob
/// page is a DM hole and the first ELF-magic read #PFs at CPL3 (fix16
/// first-light forensics 2026-09-21: cr2 0x9de0e000 = DM of PA
/// 0x1de0e000, a module blob page).
///
/// Coverage is PTE-only and does not alter memmap semantics: the pages
/// stay invisible to PMM resource qualification (exclusion unchanged).
/// Module ranges beyond the VM window's PA limit are clipped by
/// `establish_dm_range` and stay outside VM DM representability — same
/// treatment as out-of-window memmap ranges.
///
/// Union semantics: a module range overlapping an earlier candidate
/// (memmap, root/bump) is dropped — re-installing the same window slot
/// is an `AlreadyMapped` boot refusal, not a silent overwrite. Real UEFI
/// layouts are disjoint (every allocation is a distinct `AllocatePages`
/// result); the filter guards fallback/test shapes.
///
/// Heap-free by necessity: this runs inside `arch_boot` before the kernel
/// heap exists — a `collect` here is a 64-byte `handle_alloc_error` boot
/// failure (fix16 self-inflicted regression, 2026-09-21).
fn boot_module_candidates<'a>(
    kernel_info: &'a KernelInfo,
    root: PhysBytes,
) -> impl Iterator<Item = DmRange> + 'a {
    let source2 = bootstrap_tree_candidates(kernel_info, root);
    kernel_info
        .boot_modules
        .iter()
        .map(|m| DmRange::new(m.start.0, m.len as u64))
        .filter(move |r| {
            let overlaps = |o: DmRange| r.base < o.base + o.len && o.base < r.base + r.len;
            !memmap_candidates(kernel_info).any(overlaps)
                && !source2.into_iter().flatten().any(overlaps)
        })
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
/// The root page and the bump region overlap in two shapes:
///
/// - **Root allocated from the bump** (ascending: root is the first bump
///   page): the two source-2 candidates overlap; the bump candidate is
///   front-trimmed past the root so the root leaf installs exactly once.
/// - **Root allocated separately, above the bump** (the UEFI shim's
///   `MaxAddress` allocations descend: `alloc_root_page` runs first and
///   lands at the top of free low memory, the bump below it): the ranges
///   are disjoint, so the FULL bump range needs its own coverage. The
///   previous unconditional `max(base, root_end)` trim underflowed here —
///   `end − start` wrapped below base and the candidate became a silent
///   zero-iteration no-op, leaving every runtime-allocated page-table page
///   outside the kernel DM window (the first walker access then #PFs).
///
/// A root page strictly inside the bump (neither first nor disjoint) would
/// split the bump into two disjoint coverage ranges; no current allocator
/// produces that shape, and the `DmRange` slot pair cannot express it —
/// the middle piece is emitted as `[base, root_base)` and the remainder
/// above the root is skipped with the shape documented here.
fn bootstrap_tree_candidates(kernel_info: &KernelInfo, root: PhysBytes) -> [Option<DmRange>; 2] {
    let in_memmap =
        |r: &DmRange| memmap_candidates(kernel_info).any(|m| r.base < m.base + m.len && m.base < r.base + r.len);

    let root_range = DmRange::new(root.0, PAGE_SIZE);
    let root_end = root.0 + PAGE_SIZE;
    let bump_range = boot_alloc::boot_alloc_region().and_then(|(base, end)| {
        if root.0 >= base && root.0 < end {
            // Root inside the bump: front-trim past the root page. The trim
            // is guarded (start < end) so an empty remainder stays `None`
            // instead of wrapping into a bogus giant range.
            let start = core::cmp::max(base, root_end);
            (start < end).then(|| DmRange::new(start, end - start))
        } else {
            // Disjoint allocations: the whole bump range needs coverage.
            Some(DmRange::new(base, end - base))
        }
    });

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
            reserved_regions: &[],
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

    /// Disjoint shape (UEFI shim): `alloc_root_page` runs first with
    /// `MaxAddress` and lands ABOVE the later-allocated bump region, so the
    /// root page is not inside the bump at all. The full bump range must
    /// survive as a candidate — the old unconditional `max(base, root_end)`
    /// trim underflowed (`end − start` wrapped) and the candidate silently
    /// established nothing, leaving every runtime page-table page outside
    /// the kernel DM window (live #PF: walk of the user-top PDPT at
    /// PA 0x0dfb4000 through DM 0xffff8080_0dfb4ff8, test-user-trap E8).
    #[test]
    #[cfg(feature = "mock")]
    fn test_union_keeps_full_bump_when_root_above_bump() {
        let _boot = crate::test_sync::lock_boot_globals();
        use minix_boot::MemoryRegion;

        let info = KernelInfo {
            memmap: &[MemoryRegion { base: PhysBytes(0), len: 0x800_0000 }],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: minix_types::VirBytes(0x7fff_ffff_f000),
            kern_stack_top: minix_types::VirBytes(0xFFFF_8000_0020_0000),
            syscall_entry: minix_types::VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
            reserved_regions: &[],
        };
        // Real test-user-trap numbers: bump [0x0dfaf000, 0x0dfef000) (64
        // pages), root page 0x0e789000 — above the bump end, outside the
        // 128 MiB memmap.
        boot_alloc::init_boot_pt_alloc(0x0dfa_f000, 0x0dfe_f000);
        let cands = bootstrap_tree_candidates(&info, PhysBytes(0x0e78_9000));
        assert_eq!(cands[0], Some(DmRange::new(0x0e78_9000, PAGE_SIZE)));
        assert_eq!(
            cands[1],
            Some(DmRange::new(0x0dfa_f000, 0x0dfe_f000 - 0x0dfa_f000)),
            "disjoint bump must be covered in full, not trimmed/underflowed"
        );
    }

    // ── Boot-flow integration: arch_boot_impl Step 4 establishes DM coverage ──

    /// fix16: boot module blobs are LOADER_DATA (absent from the
    /// conventional-only memmap) yet the VM reads them through the VM DM
    /// window (`exec_bootproc` "no copy needed, unlike C"). They must
    /// surface as explicit candidates; a module overlapping an earlier
    /// candidate is dropped (union semantics, no double install).
    #[test]
    #[cfg(feature = "mock")]
    fn test_boot_module_candidates_union() {
        let _boot = crate::test_sync::lock_boot_globals();
        use minix_boot::{BootModule, MemoryRegion};

        let mk = |modules: &'static [BootModule]| KernelInfo {
            memmap: &[MemoryRegion { base: PhysBytes(0), len: 0x100_0000 }],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: modules,
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
            reserved_regions: &[],
        };
        // Real fix16 shape (blob at PA 0x1de0e000, above the 16 MiB
        // memmap) plus a hypothetical in-memmap module (dropped).
        static MODS: &[BootModule] = &[
            BootModule { name: "rs", start: PhysBytes(0x1de0_e000), len: 0x42_000 },
            BootModule { name: "overlap", start: PhysBytes(0x80_0000), len: 0x1000 },
        ];
        boot_alloc::init_boot_pt_alloc(0x2000, 0x100_0000); // bump inside memmap → dropped by source 2 too
        let cands: alloc::vec::Vec<DmRange> =
            boot_module_candidates(&mk(MODS), PhysBytes(0x1000)).collect();
        assert_eq!(cands, alloc::vec![DmRange::new(0x1de0_e000, 0x42_000)],
            "out-of-memmap module kept, overlapping module dropped");
    }

    /// fix26: `kernel_dm_pa_end` returns the page-rounded highest end of
    /// the same candidate union `establish_boot_dm` installs into the
    /// kernel window — here the out-of-memmap module blob (0x1de0e000 +
    /// 0x42000), which the old 4-page sentinel never covered.
    #[test]
    #[cfg(feature = "mock")]
    fn test_kernel_dm_pa_end_covers_all_sources() {
        let _boot = crate::test_sync::lock_boot_globals();
        use minix_boot::{BootModule, MemoryRegion};

        static MODS: &[BootModule] = &[BootModule {
            name: "rs",
            start: PhysBytes(0x1de0_e000),
            len: 0x42_000,
        }];
        let info = KernelInfo {
            memmap: &[MemoryRegion { base: PhysBytes(0), len: 0x100_0000 }],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: MODS,
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
            reserved_regions: &[],
        };
        boot_alloc::init_boot_pt_alloc(0x2000, 0x100_0000); // inside memmap → dropped
        assert_eq!(kernel_dm_pa_end(&info, PhysBytes(0x1000)), 0x1de5_0000,
            "module end 0x1de0e000+0x42000=0x1de50000 is the highest candidate");
    }

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
            reserved_regions: &[],
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
            reserved_regions: &[],
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
            reserved_regions: &[],
        };
        let info_ref = crate::arch_boot_impl::<minix_arch::paging::mock::MockPaging>(&info, PhysBytes(0x1000));
        assert_eq!(info_ref.kern_phys_base.0, 0x8000_0000);
    }
}
