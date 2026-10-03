//! RISC-V 64-bit (Sv39) paging implementation
//!
//! Boot-stage methods (`new_from_page`, `enable`, `map_huge`) work with the
//! identity mapping (VA = PA) that is active before the MMU (satp) is
//! switched to our own page table.
//!
//! Runtime methods (`map`, `unmap`, `query`, `remap`, `update_flags`,
//! `new`, `destroy`) use the kernel Direct Map
//! (`DirectMapArch::kernel_phys_to_virt`) to read and write page table
//! entries through physical addresses after the kernel's own page table
//! is live.
//!
//! # Sv39 page table architecture (3-level)
//!
//! | Level | Shift | Entry size | Page size   |
//! |-------|-------|------------|-------------|
//! | L2    | 30    | 1 GB       | 1 GB block  |
//! | L1    | 21    | 2 MB       | 2 MB block  |
//! | L0    | 12    | 4 KB       | 4 KB page   |
//!
//! # PTE format
//!
//! - Bit  0: V (Valid)
//! - Bit  1: R (Readable)
//! - Bit  2: W (Writable) — requires R=1
//! - Bit  3: X (Executable)
//! - Bit  4: U (User mode accessible)
//! - Bit  5: G (Global)
//! - Bit  6: A (Accessed)
//! - Bit  7: D (Dirty)
//! - Bits 53:10: PPN (Physical Page Number = PA >> 12)
//!
//! A PTE is a **leaf** if any of R/W/X is set, otherwise it is a **table**
//! pointer (non-leaf).

use crate::paging::{Paging, PageFlags, PageTableError};
use crate::paging_ext::HugePages;
use crate::direct_map::{DirectMapArch, Riscv64DirectMap};
use crate::pte_walk_arch::PteWalkArch;
use minix_types::{PhysBytes, VirBytes};
use core::arch::asm;

bitflags::bitflags! {
    /// RISC-V Sv39 page table entry flags (hardware encoding).
    ///
    /// This is the hardware-level PTE bit layout, separate from the
    /// OS-semantic `PageFlags`. Key constraints:
    /// - W=1 requires R=1 (RISC-V Privileged Spec §4.3.1)
    /// - R=W=X=0 indicates a non-leaf (table pointer) entry
    /// - R=0,X=1 is execute-only (optional, requires menvcfg.CBIE)
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Sv39PteFlags: u64 {
        const V = 1 << 0;  // Valid
        const R = 1 << 1;  // Readable
        const W = 1 << 2;  // Writable (requires R=1)
        const X = 1 << 3;  // Executable
        const U = 1 << 4;  // User mode accessible
        const G = 1 << 5;  // Global
        const A = 1 << 6;  // Accessed
        const D = 1 << 7;  // Dirty
    }
}

const L2_SHIFT: u32 = 30;
const L1_SHIFT: u32 = 21;
const L0_SHIFT: u32 = 12;
/// Mask for the PPN field within a PTE (bits 53:10).
/// In RISC-V Sv39, PTE[53:10] = PPN[43:0], where PPN = PA >> 12.
/// So PTE_PPN = (PA >> 12) << 10 = PA >> 2 (since PA[11:0] = 0).
const PTE_PPN_MASK: u64 = 0x003F_FFFF_FFFF_FC00;

/// Convert a physical address to the PPN field in a PTE.
/// PTE[53:10] = PPN = PA >> 12, so the PTE PPN bits = (PA >> 12) << 10 = PA >> 2.
#[inline]
fn paddr_to_pte(paddr: u64) -> u64 {
    (paddr >> 2) & PTE_PPN_MASK
}

/// Extract the physical address from a PTE's PPN field.
/// PA = PPN << 12 = (PTE[53:10]) << 12 = (PTE & PTE_PPN_MASK) >> 10 << 12.
#[inline]
fn pte_to_paddr(pte: u64) -> u64 {
    ((pte & PTE_PPN_MASK) >> 10) << 12
}

/// Check whether a PTE is a leaf entry (has any of R/W/X set).
/// In Sv39, R=W=X=0 means non-leaf (table pointer); any of R/W/X set means leaf.
#[inline]
fn pte_is_leaf(pte: u64) -> bool {
    let flags = Sv39PteFlags::from_bits_truncate(pte);
    flags.intersects(Sv39PteFlags::R | Sv39PteFlags::W | Sv39PteFlags::X)
}

fn l2_index(vaddr: u64) -> usize {
    ((vaddr >> L2_SHIFT) & 0x1FF) as usize
}

fn l1_index(vaddr: u64) -> usize {
    ((vaddr >> L1_SHIFT) & 0x1FF) as usize
}

fn l0_index(vaddr: u64) -> usize {
    ((vaddr >> L0_SHIFT) & 0x1FF) as usize
}

unsafe fn read_entry(table: *mut u64, idx: usize) -> u64 {
    core::ptr::read_volatile(table.add(idx))
}

unsafe fn write_entry(table: *mut u64, idx: usize, val: u64) {
    core::ptr::write_volatile(table.add(idx), val);
}

unsafe fn phys_to_ptr(phys: u64) -> *mut u64 {
    phys as *mut u64
}

/// Translate OS-semantic `PageFlags` into Sv39 hardware PTE flags.
///
/// Boot stage: all pages are readable (R=1) because:
/// 1. RISC-V requires R=1 when W=1 (W-implies-R, Privileged Spec §4.3.1)
/// 2. Execute-only pages (R=0,X=1) are optional and require menvcfg.CBIE;
///    boot stage does not set this CSR, so R=1 is required for X=1 as well.
/// 3. Boot mappings (identity + kernel) are always RWX, so R=1 is correct.
///
/// When runtime `map()` is implemented, this function should be revisited
/// to support execute-only pages if the hardware supports them.
fn flags_to_pte(flags: PageFlags) -> u64 {
    let mut pte = Sv39PteFlags::V | Sv39PteFlags::A | Sv39PteFlags::D;
    // R=1 always set in boot stage (see function doc above).
    pte |= Sv39PteFlags::R;
    if flags.contains(PageFlags::WRITABLE) {
        pte |= Sv39PteFlags::W;
    }
    if flags.contains(PageFlags::EXECUTABLE) {
        pte |= Sv39PteFlags::X;
    }
    if flags.contains(PageFlags::USER_ACCESSIBLE) {
        pte |= Sv39PteFlags::U;
    }
    if flags.contains(PageFlags::GLOBAL) {
        pte |= Sv39PteFlags::G;
    }
    pte.bits()
}

/// Translate Sv39 hardware PTE flags into OS-semantic `PageFlags`.
///
/// Inverse of `flags_to_pte`. RISC-V uses normal (set=enabled) semantics
/// for all permission bits, so the inversion is straightforward.
///
/// Note: this does NOT set `PageFlags::HUGE_PAGE` — that flag is added by
/// the walk function when a leaf PTE is encountered at L2 (1GB) or L1
/// (2MB), because RISC-V has no dedicated "huge page" PTE bit; the page
/// size is determined by which table level the leaf entry is at.
fn pte_to_flags(pte: u64) -> PageFlags {
    let hw = Sv39PteFlags::from_bits_truncate(pte);
    let mut flags = PageFlags::empty();
    if hw.contains(Sv39PteFlags::V) {
        flags |= PageFlags::PRESENT;
    }
    // R is always set for leaf PTEs; PageFlags has no explicit READABLE,
    // so PRESENT implies readable. We map W → WRITABLE, X → EXECUTABLE.
    if hw.contains(Sv39PteFlags::W) {
        flags |= PageFlags::WRITABLE;
    }
    if hw.contains(Sv39PteFlags::X) {
        flags |= PageFlags::EXECUTABLE;
    }
    if hw.contains(Sv39PteFlags::U) {
        flags |= PageFlags::USER_ACCESSIBLE;
    }
    if hw.contains(Sv39PteFlags::G) {
        flags |= PageFlags::GLOBAL;
    }
    flags
}

pub struct Riscv64Paging {
    root_paddr: u64,
    /// PTE access channel pinned at construction (see [`PteChannel`]).
    channel: PteChannel,
}

// ── Page table walk helpers (runtime, via Direct Map) ──

/// PTE access channel, pinned at handle construction.
///
/// RISC-V exposes two Direct Map windows with different privilege levels:
/// the kernel Direct Map (supervisor-only, high half) and the VM Direct
/// Map (user-accessible). A page-table handle exercised in kernel context
/// reaches PTE pages through the kernel window; a handle exercised by VM —
/// a user-space process — can only reach them through the VM window. The
/// channel is chosen by the constructor: `new_from_page`/`from_active_root`
/// pin `KernelDm`, `new()` (whose production callers are all VM-side) and
/// `adopt_active_root` pin `VmDm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PteChannel {
    /// Kernel context: PTE pages accessed via the kernel Direct Map.
    KernelDm,
    /// VM context: PTE pages accessed via the VM Direct Map.
    VmDm,
}

#[inline]
fn channel_to_ptr(phys: u64, channel: PteChannel) -> *mut u64 {
    // V11/E4: in this crate's own test build (mock feature), the VM window
    // routes through MockDirectMap so tests can point it at real leaked
    // memory via `set_mock_vm_base` — mirroring the x86_64 funnel.
    // Production keeps the riscv64 constant base.
    #[cfg(all(test, feature = "runtime-window"))]
    if matches!(channel, PteChannel::VmDm) {
        return crate::arch::direct_map::MockDirectMap::vm_phys_to_virt(PhysBytes(phys)).0
            as *mut u64;
    }
    match channel {
        PteChannel::KernelDm => {
            Riscv64DirectMap::kernel_phys_to_virt(PhysBytes(phys)).0 as *mut u64
        }
        PteChannel::VmDm => Riscv64DirectMap::vm_phys_to_virt(PhysBytes(phys)).0 as *mut u64,
    }
}

/// Read a PTE at the given physical address via the Direct Map.
///
/// SAFETY: the channel's Direct Map window must be active; `paddr` must be
/// a valid 8-byte aligned PTE address.
#[inline]
unsafe fn read_pte_dm(paddr: u64, channel: PteChannel) -> u64 {
    // 续-311 [ARCH: riscv-vmddm]：VmDm 槽读走内核代读钩子（VM 注册后），
    // 绕开 QEMU 平移层对 DM 窗 VA 的时序性误导；KernelDm（S 态 KDM 直读，
    // krewalk 实证无此症状）与未注册态维持直读。
    #[cfg(all(target_arch = "riscv64", not(test)))]
    if matches!(channel, PteChannel::VmDm) {
        if let Some(v) = vmdm::read(paddr) {
            return v;
        }
    }
    core::ptr::read_volatile(channel_to_ptr(paddr, channel))
}

/// Write a PTE at the given physical address via the Direct Map, with
/// a TLB invalidation for the affected virtual address — **gated on the
/// channel** (续-77, the riscv twin of the aarch64 §1.111 fix and the
/// x86 `invlpg` gate, mirroring `arm64::paging::write_pte_dm`).
///
/// `sfence.vma` is a supervisor instruction: executed from U-mode it
/// raises an illegal-instruction exception. The VM server is a user
/// process whose `map`/`remap`/`unmap`/`update_flags` funnels through
/// this funnel on a `VmDm` handle — the ungated flush was the second
/// aarch64-§1.111 root cause replicated verbatim on riscv
/// (riscv-reviewlog §A.5, the registered single-point gap).
///
/// Why skipping the flush on `VmDm` is sound (same argument as arm64):
/// the only writes a `VmDm` handle performs during boot are
/// not-present → present (`map` refuses `AlreadyMapped`, and `walk_alloc`
/// only creates absent intermediates), and RISC-V never caches a
/// translation derived from an invalid PTE, so no stale TLB entry can
/// exist for the newly-installed mapping. present→X transitions on a
/// `VmDm` handle would need a kernel-assisted flush — the same VmDm
/// flush gap as x86-64/aarch64 (documented, not silently widened).
///
/// SAFETY: the channel's Direct Map window must be active; `paddr` must
/// be a valid 8-byte aligned PTE address. `vaddr_for_flush` is the
/// virtual address the PTE covers (used for TLB invalidation; pass 0 for
/// intermediate tables where no leaf TLB entry exists yet).
#[inline]
unsafe fn write_pte_dm(paddr: u64, value: u64, vaddr_for_flush: u64, channel: PteChannel) {
    core::ptr::write_volatile(channel_to_ptr(paddr, channel), value);
    if channel == PteChannel::KernelDm {
        // Flush any stale TLB entry for this virtual address. For
        // intermediate table entries (L2/L1 non-leaf), no leaf TLB entry
        // exists yet, so the flush is a conservative no-op. For leaf PTE
        // entries, this ensures stale mappings are evicted.
        // sfence.vma rs1=vaddr, rs2=x0 (all ASIDs).
        unsafe { asm!("sfence.vma {}, x0", in(reg) vaddr_for_flush) };
    }
    // VmDm: see doc comment — `sfence.vma` at U-mode faults, and the
    // writes this channel performs during boot (not-present → present)
    // need no flush.
}

/// Result of a read-only walk down the 3-level Sv39 page table.
#[derive(Debug)]
enum WalkResult {
    /// Reached the leaf L0 page entry. Holds (leaf_pte_paddr, raw_pte).
    Leaf(u64, u64),
    /// Hit a 1GB leaf at L2 level. Holds (paddr, flags).
    Huge1G(PhysBytes, PageFlags),
    /// Hit a 2MB leaf at L1 level. Holds (paddr, flags).
    Huge2M(PhysBytes, PageFlags),
    /// Entry not present at some intermediate level.
    NotPresent,
}

// ── NK4-C 续-311 内核代读旁路（[ARCH: riscv-vmddm]；(A) 结案时裁决去留）──
// VM 的 VmDm 槽读在特定时序被 QEMU 平移层误导（§续-291..310 穷举收口：
// RAM 恒净/链健全/绑定正确，而 VM 读到 poison）。本钩子让 VM 注册代读
// fn（实现=gateway kernel-call，内核 S 态 KDM 直读，绕开平移层），
// read_pte_dm 的 VmDm 腿改经此。None=直读（未注册/测试态）。KernelDm
// （内核自读）不经此——S 态 KDM 直读无此症状（krewalk 实证）。
#[cfg(all(target_arch = "riscv64", not(test)))]
pub mod vmdm {
    use core::sync::atomic::{AtomicUsize, Ordering};

    type VmDmRead = fn(paddr: u64) -> u64;
    static VMDM_READ: AtomicUsize = AtomicUsize::new(0);

    /// VM init 注册（write-once，先于任何子进程 walk；契约同
    /// pt_alloc::register）。
    pub fn register_read(f: VmDmRead) {
        VMDM_READ.store(f as usize, Ordering::Relaxed);
    }

    /// 钩子未注册返回 None（调用方回落直读）。
    pub(crate) fn read(paddr: u64) -> Option<u64> {
        let f = VMDM_READ.load(Ordering::Relaxed);
        if f == 0 {
            return None;
        }
        // SAFETY: 同型 fn 由 register_read 存入；注册先于任何 walk，
        // VM 单线程无写竞争。
        Some(unsafe { (core::mem::transmute::<usize, VmDmRead>(f))(paddr) })
    }
}

/// Walk the 3-level Sv39 table read-only, returning the leaf PTE address
/// and raw value, or the huge-page mapping if encountered.
///
/// Does NOT allocate intermediate tables. Returns `NotPresent` if any
/// level's entry is absent.
///
/// Sv39 3-level walk (4KB granule, 39-bit VA):
/// - L2 (root, shift 30): 1GB leaf or table pointer
/// - L1 (shift 21): 2MB leaf or table pointer
/// - L0 (shift 12): 4KB page leaf
///
/// A PTE is a **leaf** if any of R/W/X is set; otherwise (V=1, R=W=X=0)
/// it is a **table pointer** to the next level.
///
/// # Safety precondition (not enforced at compile time)
///
/// The Direct Map window selected by `channel` must be active — i.e.
/// paging is enabled with that window mapped (kernel: `PA=0` at
/// `KERNEL_DIRECT_MAP_BASE`; VM: the VM Direct Map window). Callers must
/// not invoke this before paging is enabled.
fn walk_read(root_paddr: u64, vaddr: u64, channel: PteChannel) -> WalkResult {
    let i2 = l2_index(vaddr);
    // SAFETY: channel's Direct Map active per function precondition; the L2
    // entry address is root_paddr + i2*8, within the root page.
    let l2e = unsafe { read_pte_dm(root_paddr + (i2 as u64) * 8, channel) };
    if l2e & Sv39PteFlags::V.bits() == 0 {
        return WalkResult::NotPresent;
    }
    // 1GB leaf: V=1 and any of R/W/X set.
    if pte_is_leaf(l2e) {
        let paddr = pte_to_paddr(l2e) | (vaddr & 0x3FFF_FFFF);
        let mut flags = pte_to_flags(l2e);
        flags |= PageFlags::HUGE_PAGE;
        return WalkResult::Huge1G(PhysBytes(paddr), flags);
    }
    let l1 = pte_to_paddr(l2e);

    let i1 = l1_index(vaddr);
    // SAFETY: see above; L1 entry address is within the L1 page.
    let l1e = unsafe { read_pte_dm(l1 + (i1 as u64) * 8, channel) };
    if l1e & Sv39PteFlags::V.bits() == 0 {
        return WalkResult::NotPresent;
    }
    // 2MB leaf: V=1 and any of R/W/X set.
    if pte_is_leaf(l1e) {
        let paddr = pte_to_paddr(l1e) | (vaddr & 0x1F_FFFF);
        let mut flags = pte_to_flags(l1e);
        flags |= PageFlags::HUGE_PAGE;
        return WalkResult::Huge2M(PhysBytes(paddr), flags);
    }
    let l0 = pte_to_paddr(l1e);

    let l0_idx = l0_index(vaddr);
    let leaf_paddr = l0 + (l0_idx as u64) * 8;
    // SAFETY: see above; L0 entry address is within the L0 page.
    let pte = unsafe { read_pte_dm(leaf_paddr, channel) };
    WalkResult::Leaf(leaf_paddr, pte)
}

/// Read-only page table walk for offline VA→PA translation.
///
/// Wraps `walk_read` and converts the internal `WalkResult` into the
/// public `Option<(PhysBytes, PageFlags)>` form. This is the entry
/// point used by `PteWalkArch::walk` (and thus by the kernel's
/// cross-space copy code) when it needs to translate a foreign
/// process's virtual address without constructing a `Paging` instance.
///
/// # Safety precondition
///
/// The kernel Direct Map must be active. See `walk_read`.
pub(crate) fn walk_translate(root_paddr: u64, vaddr: u64) -> Option<(PhysBytes, PageFlags)> {
    // Cross-space translation runs in kernel context (the kernel reads a
    // foreign process's page table during IPC copies), so the kernel DM
    // channel is hardcoded — independent of any handle's pinned channel.
    match walk_read(root_paddr, vaddr, PteChannel::KernelDm) {
        WalkResult::Leaf(_leaf_paddr, pte) if pte & Sv39PteFlags::V.bits() != 0 => {
            // For 4KB leaf pages, the offset within the page comes from
            // the low 12 bits of vaddr.
            let offset = vaddr & 0xFFF;
            Some((PhysBytes(pte_to_paddr(pte) | offset), pte_to_flags(pte)))
        }
        WalkResult::Huge1G(paddr, flags) | WalkResult::Huge2M(paddr, flags) => {
            // For huge pages, the offset is the low bits of vaddr
            // below the huge-page boundary (already folded into paddr
            // by walk_read).
            Some((paddr, flags))
        }
        _ => None,
    }
}

/// ZST implementor of `PteWalkArch` for riscv64 (Sv39).
///
/// Delegates to `walk_translate`, which reuses the same `walk_read`
/// helper that powers `Paging::query`. This ensures the offline walk
/// (used by the kernel for cross-space copy) and the live walk (used
/// by `Paging::query`) produce identical results.
pub struct Riscv64PteWalk;

impl PteWalkArch for Riscv64PteWalk {
    fn walk(root_paddr: PhysBytes, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        walk_translate(root_paddr.0, vaddr.0)
    }
}

/// Walk the 3-level Sv39 table, allocating intermediate tables (L1/L0)
/// when not present. Returns the physical address of the leaf L0 PTE slot.
///
/// Returns `AllocationFailed` if `alloc_pt_page` fails, or
/// `AlreadyMapped` if a leaf at L2/L1 blocks the 4KB walk.
///
/// # Safety precondition (not enforced at compile time)
///
/// The Direct Map window selected by `channel` must be active. See
/// `walk_read`.
fn walk_alloc(root_paddr: u64, vaddr: u64, channel: PteChannel) -> Result<u64, PageTableError> {
    let i2 = l2_index(vaddr);
    // SAFETY: channel's Direct Map active per function precondition.
    let l2e = unsafe { read_pte_dm(root_paddr + (i2 as u64) * 8, channel) };
    let l1 = if l2e & Sv39PteFlags::V.bits() == 0 {
        // Allocate a new L1 table page. Non-leaf PTE: V=1, R=W=X=0.
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = paddr_to_pte(phys.0) | Sv39PteFlags::V.bits();
        // SAFETY: channel's Direct Map active; L2 entry slot is 8-byte aligned.
        unsafe { write_pte_dm(root_paddr + (i2 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        if pte_is_leaf(l2e) {
            // A 1GB leaf already occupies this slot — cannot install 4KB.
            return Err(PageTableError::AlreadyMapped);
        }
        pte_to_paddr(l2e)
    };

    let i1 = l1_index(vaddr);
    // SAFETY: see above.
    let l1e = unsafe { read_pte_dm(l1 + (i1 as u64) * 8, channel) };
    let l0 = if l1e & Sv39PteFlags::V.bits() == 0 {
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = paddr_to_pte(phys.0) | Sv39PteFlags::V.bits();
        unsafe { write_pte_dm(l1 + (i1 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        if pte_is_leaf(l1e) {
            // A 2MB leaf already occupies this slot.
            return Err(PageTableError::AlreadyMapped);
        }
        pte_to_paddr(l1e)
    };

    let l0_idx = l0_index(vaddr);
    Ok(l0 + (l0_idx as u64) * 8)
}

impl Riscv64Paging {
/// Free all page-table pages directly or transitively referenced by
/// `table_paddr`'s valid non-leaf entries. `level` 0 = L2 (root),
/// 1 = L1; children at level 2 are leaf L0 pages (freed without
/// recursion). Data pages (leaf PTEs) are never touched here.
///
/// SAFETY: Direct Map channel active; the caller guarantees the whole
/// tree is exclusive to this (dead) address space and no CPU walks it.
unsafe fn free_child_tables(&self, table_paddr: u64, level: u8) {
    if level >= 2 {
        return;
    }
    let base = channel_to_ptr(table_paddr, self.channel);
    for i in 0..512usize {
        // SAFETY: sequential reads within the table page, which the
        // caller has excluded from all other access.
        let pte = unsafe { core::ptr::read_volatile(base.add(i)) };
        // Valid non-leaf: V=1 and R|W|X = 0 (Sv39 spec 4.3c). Leaves
        // are data pages owned by the region/exit path — skipped.
        if pte & Sv39PteFlags::V.bits() != 0 && !pte_is_leaf(pte) {
            let child = pte_to_paddr(pte);
            unsafe { self.free_child_tables(child, level + 1) };
            crate::pt_alloc::free_pt_page(minix_types::PhysBytes(child));
        }
    }
}
}

impl Paging for Riscv64Paging {
    const PAGE_SIZE: usize = 4096;

    /// Uses a pre-allocated physical page for the root L2 table.
    ///
    /// During boot, the OpenSBI or UEFI shim passes the page for the root
    /// (Sv39 L2) page table. There is no kernel page allocator at this point,
    /// so `new()` (which would allocate internally) cannot be used.
    /// See `Paging::new()` for the runtime variant.
    fn new_from_page(root_page: PhysBytes) -> Self {
        let ptr = unsafe { phys_to_ptr(root_page.0) };
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Self { root_paddr: root_page.0, channel: PteChannel::KernelDm }
    }

    /// Wrap an already-active Sv39 root page table without zeroing it.
    ///
    /// Unlike `new_from_page`, this constructor assumes the root page table
    /// at `root_phys` is already initialized and currently loaded into
    /// `satp` (via a prior `enable()` call). It creates a handle that can
    /// perform `map`/`remap`/`query` operations on the live page table.
    ///
    /// # Use case
    ///
    /// `arch_boot_impl` creates the bootstrap page table, enables paging,
    /// and drops the `Paging` instance. Later phases (e.g.,
    /// `init_proc_and_boot` loading the VM ELF) need to add mappings to
    /// the *same* page table. `from_active_root` lets them obtain a handle
    /// without re-allocating or zeroing the root.
    ///
    /// # Safety contract (caller responsibility)
    ///
    /// - `root_phys` must point to a valid, 4KB-aligned Sv39 L2 table.
    /// - The L2 table must be currently loaded into `satp` (or accessible
    ///   via the kernel Direct Map, which `walk_read`/`walk_alloc` rely on).
    /// - The returned handle must not outlive the page table it wraps.
    fn from_active_root(root_phys: PhysBytes) -> Self {
        Self { root_paddr: root_phys.0, channel: PteChannel::KernelDm }
    }

    fn inherit_supervisor_half(&mut self, source_root: PhysBytes) {
        // Sv39 L2 entries 256..512 = the canonical high half (VAs with
        // bit 38 set): kernel text/data and the DM windows. Raw entry
        // copy — the entries may point at shared L1/L0 table pages,
        // which is sound because user mappings (VAs below 2^38) never
        // touch the upper half (see trait doc).
        for i in 256usize..512 {
            let slot = (i as u64) * 8;
            // SAFETY: both roots are 4KB-aligned page-table pages in RAM;
            // the KernelDm direct map (pinned by `new_from_page` /
            // `from_active_root`) makes every PTE slot addressable. Same
            // access pattern as `walk_alloc`; no fence needed — the copy
            // runs before the root is ever loaded into `satp`.
            let entry =
                unsafe { read_pte_dm(source_root.0 + slot, PteChannel::KernelDm) };
            unsafe {
                write_pte_dm(self.root_paddr + slot, entry, 0, PteChannel::KernelDm)
            };
        }
    }

    /// Wrap an already-active Sv39 root page table for VM-context access.
    ///
    /// Same wrapping semantics as `from_active_root`, but the handle's PTE
    /// access channel is the VM Direct Map: the handle is exercised while
    /// VM (a user-space process) runs, where the supervisor-only kernel
    /// window is unreachable. See [`PteChannel`].
    ///
    /// # Safety contract (caller responsibility)
    ///
    /// Same as `from_active_root`, plus: every page-table page reachable
    /// from the root must be covered by the VM Direct Map window.
    fn adopt_active_root(root_phys: PhysBytes) -> Self {
        Self { root_paddr: root_phys.0, channel: PteChannel::VmDm }
    }

    unsafe fn enable(&self) -> PhysBytes {
        unsafe {
            // satp = (MODE_Sv39 << 60) | (ASID << 44) | (root_paddr >> 12).
            // Unlike x86-64/aarch64 where UEFI firmware already runs with
            // paging enabled (and we switch to our own page table), OpenSBI
            // on riscv64 does NOT enable the MMU — we enable Sv39 paging
            // from scratch. After this write, all addresses go through the
            // 3-level Sv39 page table rooted at self.root_paddr.
            //
            // Both csrw satp and sfence.vma MUST be in the same asm! block
            // to prevent the compiler from inserting memory accesses between
            // them. After csrw satp, the MMU is on and any subsequent memory
            // access goes through the page table. sfence.vma flushes the TLB
            // so the new mappings are visible.
            let satp = (8u64 << 60) | (self.root_paddr >> 12);
            asm!(
                "csrw satp, {0}",
                "sfence.vma",
                in(reg) satp,
            );
        }
        PhysBytes(self.root_paddr)
    }

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized,
    {
        // Allocate the root L2 (Sv39 root) page via the registered
        // page-table allocator. The allocator (boot bump or VM-side) is
        // responsible for zero-filling; we additionally zero here for
        // defense-in-depth.
        //
        // Channel: VM-context (VmDm). In production this constructor is only
        // called from the VM server (VM-managed process page tables), and a
        // user-space process can only reach PTE pages through the VM Direct
        // Map. Kernel-context construction goes through `from_active_root`.
        let (root_phys, _root_virt) = crate::pt_alloc::alloc_pt_page()?;
        // SAFETY: the VM Direct Map window must be established (paging
        // enabled with the VM DM window mapped); the fresh root page is
        // RAM covered by that window.
        let ptr = channel_to_ptr(root_phys.0, PteChannel::VmDm);
        // SAFETY: alloc_pt_page returns a fresh, 4KB-aligned page that is
        // not aliased by any other live reference. Direct Map must be active.
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Ok(Self { root_paddr: root_phys.0, channel: PteChannel::VmDm })
    }

    unsafe fn destroy(&mut self) {
        // V11/E4: full three-level reclaim when a pt_free function is
        // registered (C pagetable.c:1427-1437 `pt_free` parity — previously
        // "accept the intermediate-table leak"). Without pt_free (boot-stage
        // tables with no allocator domain), keep the zero-root-only legacy
        // shape: zero the root L2 to prevent use-after-free if the physical
        // page is reused.
        //
        // SAFETY: the handle's Direct Map channel must be active;
        // root_paddr is the physical address of our L2 (root) page. The
        // exit path guarantees the page table is not active on any CPU —
        // single-threaded VM, no concurrent walker.
        if !crate::pt_alloc::is_free_registered() {
            let ptr = channel_to_ptr(self.root_paddr, self.channel);
            unsafe { core::ptr::write_bytes(ptr, 0, 512) };
            return;
        }
        // Depth-first: free every intermediate table below the root (L1
        // pages, then L0 pages; leaf/data PTEs — V=1 with R|W|X — are data
        // pages owned by the region/exit path — skipped). Then zero the
        // root and return the root page itself to the allocator.
        unsafe { self.free_child_tables(self.root_paddr, 0) };
        let ptr = channel_to_ptr(self.root_paddr, self.channel);
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        crate::pt_alloc::free_pt_page(minix_types::PhysBytes(self.root_paddr));
    }



    fn map(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        // 4KB alignment check.
        if vaddr.0 & 0xFFF != 0 || paddr.0 & 0xFFF != 0 {
            return Err(PageTableError::InvalidAddress);
        }
        // Walk to the leaf L0 PTE address, allocating intermediate tables.
        let leaf_paddr = walk_alloc(self.root_paddr, vaddr.0, self.channel)?;
        // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned PTE slot.
        let pte = unsafe { read_pte_dm(leaf_paddr, self.channel) };
        if pte & Sv39PteFlags::V.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
        let new_pte = paddr_to_pte(paddr.0) | flags_to_pte(flags);
        // SAFETY: see above. Flush TLB for the target vaddr in case a
        // stale entry lingers from a prior unmap.
        unsafe { write_pte_dm(leaf_paddr, new_pte, vaddr.0, self.channel) };
        Ok(())
    }

    fn remap(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        flags: PageFlags,
    ) -> Result<Option<(PhysBytes, PageFlags)>, PageTableError> {
        if vaddr.0 & 0xFFF != 0 || paddr.0 & 0xFFF != 0 {
            return Err(PageTableError::InvalidAddress);
        }
        // Walk read-only first to locate the leaf (do not allocate).
        match walk_read(self.root_paddr, vaddr.0, self.channel) {
            WalkResult::Leaf(leaf_paddr, old_pte) => {
                let old = if old_pte & Sv39PteFlags::V.bits() != 0 {
                    Some((PhysBytes(pte_to_paddr(old_pte)), pte_to_flags(old_pte)))
                } else {
                    None
                };
                let new_pte = paddr_to_pte(paddr.0) | flags_to_pte(flags);
                // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned.
                unsafe { write_pte_dm(leaf_paddr, new_pte, vaddr.0, self.channel) };
                Ok(old)
            }
            // Not mapped and no leaf table — overwrite requires allocation,
            // which remap does not do (callers should use map() first).
            _ => Err(PageTableError::NotMapped),
        }
    }

    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
        match walk_read(self.root_paddr, vaddr.0, self.channel) {
            WalkResult::Leaf(leaf_paddr, pte) if pte & Sv39PteFlags::V.bits() != 0 => {
                let old_paddr = PhysBytes(pte_to_paddr(pte));
                // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned.
                // Clear the PTE (set to 0 = invalid) and flush TLB.
                unsafe { write_pte_dm(leaf_paddr, 0, vaddr.0, self.channel) };
                Ok(old_paddr)
            }
            _ => Err(PageTableError::NotMapped),
        }
    }

    fn update_flags(
        &mut self,
        vaddr: VirBytes,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        match walk_read(self.root_paddr, vaddr.0, self.channel) {
            WalkResult::Leaf(leaf_paddr, pte) if pte & Sv39PteFlags::V.bits() != 0 => {
                // Preserve the physical address (PPN), replace only flag bits.
                // PTE_PPN_MASK preserves bits [53:10]; flag bits are [9:0].
                let new_pte = (pte & PTE_PPN_MASK) | flags_to_pte(flags);
                // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned.
                unsafe { write_pte_dm(leaf_paddr, new_pte, vaddr.0, self.channel) };
                Ok(())
            }
            _ => Err(PageTableError::NotMapped),
        }
    }

    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        // 续-155/续-173 q-badroot/l1raw 类调试探针已除（用后即滚）：本
        // query/walk_read 被 VM(U 态)与内核(S 态)共用，而 `CurrentEarlyConsole`
        // 的 riscv 后端是 SBI putchar `ecall a7=1`——在 U 态 ecall 会陷入
        // 内核 IPC 陷阱门被误当 `IpcCall::Send`（dst=a0=字符、消息指针
        // =a1=0）→ copy_msg_from_user(0) EFAULT → cause_signal(VM,SIGSEGV)
        // 致命（gh63 定谳的真身）。wrong-root 家族早已定谳（续-116/117
        // 零命中），无生产行为影响。
        match walk_read(self.root_paddr, vaddr.0, self.channel) {
            WalkResult::Leaf(_leaf_paddr, pte) if pte & Sv39PteFlags::V.bits() != 0 => {
                // For 4KB leaf pages, the offset within the page comes from
                // the low 12 bits of vaddr.
                let offset = vaddr.0 & 0xFFF;
                Some((PhysBytes(pte_to_paddr(pte) | offset), pte_to_flags(pte)))
            }
            WalkResult::Huge1G(paddr, flags) | WalkResult::Huge2M(paddr, flags) => {
                // For huge pages, the offset is the low bits of vaddr
                // below the huge-page boundary (already folded into paddr
                // by walk_read).
                Some((paddr, flags))
            }
            _ => None,
        }
    }

    fn root_paddr(&self) -> PhysBytes {
        PhysBytes(self.root_paddr)
    }

    unsafe fn switch(&self) {
        unsafe {
            let satp = (8u64 << 60) | (self.root_paddr >> 12);
            asm!("csrw satp, {}", in(reg) satp);
        }
    }

    unsafe fn flush_tlb(&self) {
        unsafe {
            asm!("sfence.vma");
        }
    }

    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes) {
        unsafe {
            asm!("sfence.vma {}", in(reg) vaddr.0);
        }
    }

    /// Split the huge leaf (1 GiB at L2 / 2 MiB at L1) covering `vaddr`
    /// into the next level's 512 entries, preserving every translation
    /// (same frames, same leaf flags) so individual 4 KiB pages become
    /// page-granular unmappable. NK4-C 续-90: the riscv mirror of
    /// aarch64/x86 `split_huge` — boot-identity leaves stand at 2 MiB
    /// granularity (Sv39 huge = 2 MiB when DRAM base is not 1-GiB aligned),
    /// and the boot ELF loader (`arch::boot::load_elf_into`) must evict
    /// them under user segment VAs or the fresh user `map` fails with
    /// `AlreadyMapped` (walk_alloc refuses to clobber a leaf). Returns
    /// `Ok(true)` when a leaf was split, `Ok(false)` when `vaddr` is
    /// already a 4 KiB leaf / hole (no change).
    ///
    /// Unlike x86/arm64, an Sv39 NON-LEAF (table) PTE carries NO permission
    /// bits — R/W/X must be 0 there (else reserved), and the `U` bit is
    /// meaningless on a table entry. So a split child table pointer is just
    /// `paddr_to_pte(child_pa) | V` (R=W=X=U=0); the leaf permission lives
    /// entirely in the replicated leaf PTEs one level down.
    fn split_huge(&mut self, vaddr: VirBytes) -> Result<bool, PageTableError> {
        let ch = self.channel;
        // `sfence.vma` (both the per-slot flush inside `write_pte_dm` and
        // the global flush below) is a supervisor instruction: a `VmDm`
        // (U-mode) handle would fault exactly like `write_pte_dm`'s flush
        // (§1.111). Every caller is kernel-context boot eviction; assert
        // the invariant so a future VmDm caller is caught in debug builds.
        debug_assert!(
            matches!(ch, PteChannel::KernelDm),
            "split_huge requires a KernelDm handle; VmDm would fault at sfence.vma"
        );
        let v = Sv39PteFlags::V.bits();
        let nonleaf_mask = Sv39PteFlags::R.bits() | Sv39PteFlags::W.bits() | Sv39PteFlags::X.bits();
        let mut split = false;

        // ── L2 (root, 1 GiB) ──
        let i2 = l2_index(vaddr.0);
        // SAFETY: channel's Direct Map active (read_pte_dm precondition);
        // the L2 slot lives inside the root page.
        let l2e = unsafe { read_pte_dm(self.root_paddr + (i2 as u64) * 8, ch) };
        if l2e & v == 0 {
            return Ok(false);
        }
        if l2e & nonleaf_mask != 0 {
            // A 1 GiB leaf at L2 → build an L1 table of 512 2 MiB leaves.
            let base = pte_to_paddr(l2e);
            let leaf_bits = l2e & !PTE_PPN_MASK;
            let (l1_phys, _va) = crate::pt_alloc::alloc_pt_page()?;
            for i in 0..512usize {
                // Child PA = base + i · 2 MiB. Parenthesised: `+` binds
                // tighter than `<<` in Rust (same hazard as the arm64/x86
                // split — a bare `base + i << 21` writes a bogus PA).
                let child_pa = base + ((i as u64) << L1_SHIFT);
                // SAFETY: fresh table frame reachable through the active
                // Direct Map; slot 8-byte aligned.
                unsafe {
                    write_pte_dm(
                        l1_phys.0 + (i as u64) * 8,
                        paddr_to_pte(child_pa) | leaf_bits,
                        0,
                        ch,
                    )
                };
            }
            let new_l2 = paddr_to_pte(l1_phys.0) | v; // non-leaf: R=W=X=0
            // SAFETY: L2 slot inside the live root page.
            unsafe { write_pte_dm(self.root_paddr + (i2 as u64) * 8, new_l2, vaddr.0, ch) };
            split = true;
        }

        // ── L1 (2 MiB) ──
        // Re-read L2 (may have just been rewritten as a table pointer).
        let l2e = unsafe { read_pte_dm(self.root_paddr + (i2 as u64) * 8, ch) };
        if l2e & v == 0 || l2e & nonleaf_mask != 0 {
            // Absent, or still a 1 GiB leaf (just split above — nothing
            // further to descend to at this VA in this call).
            return Ok(split);
        }
        let l1_pa = pte_to_paddr(l2e);
        let i1 = l1_index(vaddr.0);
        // SAFETY: L1 slot inside the live L1 page.
        let l1e = unsafe { read_pte_dm(l1_pa + (i1 as u64) * 8, ch) };
        if l1e & v != 0 && l1e & nonleaf_mask != 0 {
            // A 2 MiB leaf → build an L0 table of 512 4 KiB leaves, then
            // rewrite the L1 slot as a table pointer.
            let base = pte_to_paddr(l1e);
            let leaf_bits = l1e & !PTE_PPN_MASK;
            let (l0_phys, _va) = crate::pt_alloc::alloc_pt_page()?;
            for i in 0..512usize {
                let child_pa = base + ((i as u64) << L0_SHIFT);
                // SAFETY: fresh table frame reachable via active Direct
                // Map; slot 8-byte aligned; explicit parens for `+`/`<<`.
                unsafe {
                    write_pte_dm(
                        l0_phys.0 + (i as u64) * 8,
                        paddr_to_pte(child_pa) | leaf_bits,
                        0,
                        ch,
                    )
                };
            }
            let new_l1 = paddr_to_pte(l0_phys.0) | v; // non-leaf
            // SAFETY: L1 slot inside the live L1 page.
            unsafe { write_pte_dm(l1_pa + (i1 as u64) * 8, new_l1, vaddr.0, ch) };
            split = true;
        }

        if split {
            // A huge leaf may have populated TLB entries for any VA in its
            // 2 MiB / 1 GiB range; the per-slot `sfence.vma vaddr` inside
            // `write_pte_dm` only clears one address. Global flush (no
            // operands) is the Sv39 analogue of x86 CR3-reload / arm64
            // `tlbi vmalle1is`. SAFETY: boot context, KernelDm (supervisor).
            unsafe { asm!("sfence.vma") };
        }
        Ok(split)
    }

    /// Ensure the intermediate tables on the walk to `vaddr` do not veto
    /// user access. On Sv39 this is a walk-and-verify no-op: a NON-LEAF PTE
    /// cannot carry the `U` bit (RISC-V requires R=W=X=U=0 on table entries;
    /// the leaf alone decides user access), so there is no intermediate-level
    /// restriction to clear — unlike x86's per-level `U` or arm64's negative
    /// `APTable`. Present intermediates ⇒ `Ok(())`; the caller's fresh user
    /// leaf is then genuinely CPL3-reachable.
    ///
    /// Contract (mirrors the trait doc): `NotMapped` if any intermediate is
    /// absent (caller must go through `map`/`walk_alloc`, which creates
    /// fresh tables); `NotSupported` if a still-huge leaf is met mid-walk
    /// (caller must `split_huge` first — granting over a whole 2 MiB range
    /// would be a security regression).
    fn grant_user_walk(&mut self, vaddr: VirBytes) -> Result<(), PageTableError> {
        let ch = self.channel;
        let v = Sv39PteFlags::V.bits();
        let nonleaf_mask = Sv39PteFlags::R.bits() | Sv39PteFlags::W.bits() | Sv39PteFlags::X.bits();

        // SAFETY: channel DM active; L2 slot inside root page.
        let l2e = unsafe { read_pte_dm(self.root_paddr + (l2_index(vaddr.0) as u64) * 8, ch) };
        if l2e & v == 0 {
            return Err(PageTableError::NotMapped);
        }
        if l2e & nonleaf_mask != 0 {
            // 1 GiB leaf still present.
            return Err(PageTableError::NotSupported);
        }
        let l1_pa = pte_to_paddr(l2e);
        // SAFETY: L1 slot inside the live L1 page.
        let l1e = unsafe { read_pte_dm(l1_pa + (l1_index(vaddr.0) as u64) * 8, ch) };
        if l1e & v == 0 {
            return Err(PageTableError::NotMapped);
        }
        if l1e & nonleaf_mask != 0 {
            // 2 MiB leaf still present (split_huge not run).
            return Err(PageTableError::NotSupported);
        }
        // L2 and L1 are both valid table pointers; the L0 leaf is the
        // caller's to install with its own U bit. Nothing to amend.
        Ok(())
    }
}

impl HugePages for Riscv64Paging {
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21];
    const HUGE_PAGE_SIZE: u64 = 1 << 30;
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;
    /// RISC-V Sv39 does not use a dedicated PTE flag to indicate huge pages;
    /// the page size is determined by which page table level the entry is at.
    /// This constant is 0 to satisfy the `HugePages` trait interface.
    const PTE_HUGE_IDENTIFIER_BIT: u64 = 0;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        let pte_flags = flags_to_pte(flags);

        let l2 = unsafe { phys_to_ptr(self.root_paddr) };
        let i2 = l2_index(vaddr.0);
        let e2 = unsafe { read_entry(l2, i2) };

        if size >= 1 << 30 {
            // 1GB huge page: write directly into the L2 (root) table.
            unsafe { write_entry(l2, i2, paddr_to_pte(paddr.0) | pte_flags) };
            return Ok(());
        }

        // 2MB huge page: need an L1 page table under this L2 entry.
        let l1 = if e2 & Sv39PteFlags::V.bits() != 0 {
            // L2 entry is valid — check whether it's a leaf or a table pointer.
            if pte_is_leaf(e2) {
                // L2 entry is already a 1GB leaf (R/W/X set). Demoting it to
                // a table pointer would silently corrupt the existing 1GB mapping.
                // Return AlreadyMapped to signal the conflict.
                return Err(PageTableError::AlreadyMapped);
            }
            // Non-leaf entry: PPN points to an existing L1 page table.
            pte_to_paddr(e2) as *mut u64
        } else {
            // Allocate a new L1 page table.
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(l2, i2, paddr_to_pte(page) | Sv39PteFlags::V.bits()) };
            unsafe { phys_to_ptr(page) }
        };

        let i1 = l1_index(vaddr.0);
        unsafe { write_entry(l1, i1, paddr_to_pte(paddr.0) | pte_flags) };
        Ok(())
    }
}

// ── DM coverage establishment (boot identity write channel) ────────────────

/// ZST implementor of [`DmCoverageArch`] for RISC-V 64 (Sv39).
///
/// The VM DM window (0x0000_0010_0000_0000, L2 slot 64) and the kernel DM
/// window (0xFFFF_FC00_0000_0000, L2 slot 448) both lie outside the identity
/// range [0, 4 GiB) (L2 slots 0-3), so coverage establishment is pure
/// incremental mapping — no demotion path exists: any present slot inside a
/// DM window on the bootstrap root is a boot-layout bug and fails fast.
/// All table access goes through the identity mapping (`VA = PA`), which
/// stays untouched by these writes (§6.1 "DM 建立的自举写通道闭环").
pub struct Riscv64DmCoverage;

impl crate::arch::dm_coverage::DmCoverageArch for Riscv64DmCoverage {
    unsafe fn dm_install_leaf(
        root_paddr: PhysBytes,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        if size != 1 << 30 && size != 1 << 21 && size != 4096 {
            return Err(PageTableError::InvalidAddress);
        }
        if !vaddr.0.is_multiple_of(size as u64) || !paddr.0.is_multiple_of(size as u64) {
            return Err(PageTableError::InvalidAddress);
        }
        let pte_flags = flags_to_pte(flags);
        const TABLE_ENTRY_FLAGS: u64 = Sv39PteFlags::V.bits();

        // L2 (root) level: 1 GiB leaf or branch to L1.
        let i2 = l2_index(vaddr.0);
        let l2 = unsafe { phys_to_ptr(root_paddr.0) };
        let e2 = unsafe { read_entry(l2, i2) };
        let present2 = e2 & Sv39PteFlags::V.bits() != 0;

        if size == 1 << 30 {
            if present2 {
                // Leaf or table — both impossible from a single candidate
                // pass into a fresh window; fail fast.
                return Err(PageTableError::AlreadyMapped);
            }
            // SAFETY: identity channel active; fresh slot.
            unsafe { write_entry(l2, i2, paddr_to_pte(paddr.0) | pte_flags) };
            return Ok(());
        }
        if present2 && pte_is_leaf(e2) {
            // A 1 GiB leaf blocks the finer install.
            return Err(PageTableError::AlreadyMapped);
        }
        let l1 = if present2 {
            pte_to_paddr(e2)
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active; fresh slot. V=1 with R=W=X=0
            // is a non-leaf (table pointer) entry.
            unsafe { write_entry(l2, i2, paddr_to_pte(phys.0) | TABLE_ENTRY_FLAGS) };
            phys.0
        };

        // L1 level: 2 MiB leaf or branch to L0.
        let i1 = l1_index(vaddr.0);
        let l1_ptr = unsafe { phys_to_ptr(l1) };
        let e1 = unsafe { read_entry(l1_ptr, i1) };
        let present1 = e1 & Sv39PteFlags::V.bits() != 0;

        if size == 1 << 21 {
            if present1 {
                return Err(PageTableError::AlreadyMapped);
            }
            // SAFETY: identity channel active; fresh slot.
            unsafe { write_entry(l1_ptr, i1, paddr_to_pte(paddr.0) | pte_flags) };
            return Ok(());
        }
        if present1 && pte_is_leaf(e1) {
            return Err(PageTableError::AlreadyMapped);
        }
        let l0 = if present1 {
            pte_to_paddr(e1)
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active; fresh slot.
            unsafe { write_entry(l1_ptr, i1, paddr_to_pte(phys.0) | TABLE_ENTRY_FLAGS) };
            phys.0
        };

        // L0 level: 4 KiB leaf.
        let i0 = l0_index(vaddr.0);
        let l0_ptr = unsafe { phys_to_ptr(l0) };
        let e0 = unsafe { read_entry(l0_ptr, i0) };
        if e0 & Sv39PteFlags::V.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
        // SAFETY: identity channel active; fresh leaf slot.
        unsafe { write_entry(l0_ptr, i0, paddr_to_pte(paddr.0) | pte_flags) };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paddr_to_pte_roundtrip() {
        for &paddr in &[0x8000_0000u64, 0x8020_0000, 0x0, 0x4000_0000] {
            let pte = paddr_to_pte(paddr);
            let recovered = pte_to_paddr(pte);
            assert_eq!(recovered, paddr,
                "paddr_to_pte roundtrip failed: 0x{:x} → 0x{:x} → 0x{:x}",
                paddr, pte, recovered);
        }
    }

    #[test]
    fn test_flags_to_pte_kernel_read_write_exec() {
        let flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let pte = flags_to_pte(flags);
        let pte_flags = Sv39PteFlags::from_bits_truncate(pte);
        assert!(pte_flags.contains(Sv39PteFlags::V), "PTE must be valid");
        assert!(pte_flags.contains(Sv39PteFlags::R), "PTE must be readable (R=1 required for W=1)");
        assert!(pte_flags.contains(Sv39PteFlags::W), "PTE must be writable");
        assert!(pte_flags.contains(Sv39PteFlags::X), "PTE must be executable");
        assert!(pte_flags.contains(Sv39PteFlags::A), "PTE must have Accessed flag");
        assert!(pte_flags.contains(Sv39PteFlags::D), "PTE must have Dirty flag");
        assert!(pte_flags.contains(Sv39PteFlags::G), "PTE must be Global");
    }

    #[test]
    fn test_flags_to_pte_no_wx_combination() {
        // RISC-V forbids W=1,R=0. flags_to_pte always sets R=1 (boot stage),
        // so W=1 always implies R=1.
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE;
        let pte = flags_to_pte(flags);
        let pte_flags = Sv39PteFlags::from_bits_truncate(pte);
        assert!(pte_flags.contains(Sv39PteFlags::R), "W=1 requires R=1 in Sv39");
        assert!(pte_flags.contains(Sv39PteFlags::W));
    }

    #[test]
    fn test_pte_is_leaf() {
        // Non-leaf: V=1, R=W=X=0 (table pointer)
        let non_leaf = Sv39PteFlags::V.bits();
        assert!(!pte_is_leaf(non_leaf), "V-only PTE is non-leaf");

        // Leaf: V=1, R=1 (readable page)
        let leaf_r = (Sv39PteFlags::V | Sv39PteFlags::R).bits();
        assert!(pte_is_leaf(leaf_r), "V+R PTE is leaf");

        // Leaf: V=1, X=1, R=1 (execute page)
        let leaf_rx = (Sv39PteFlags::V | Sv39PteFlags::R | Sv39PteFlags::X).bits();
        assert!(pte_is_leaf(leaf_rx), "V+R+X PTE is leaf");

        // Leaf: V=1, R+W+X (full access page)
        let leaf_rwx = (Sv39PteFlags::V | Sv39PteFlags::R | Sv39PteFlags::W | Sv39PteFlags::X).bits();
        assert!(pte_is_leaf(leaf_rwx), "V+R+W+X PTE is leaf");

        // Invalid: V=0
        assert!(!pte_is_leaf(0), "V=0 PTE is neither leaf nor table");
    }

    #[test]
    fn test_pte_to_flags_roundtrip_user_rw() {
        // User read-write: PRESENT | WRITABLE | USER_ACCESSIBLE
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER_ACCESSIBLE;
        let pte = flags_to_pte(flags);
        assert_eq!(pte_to_flags(pte), flags);
    }

    #[test]
    fn test_pte_to_flags_roundtrip_kernel_exec() {
        // Kernel executable + global, not writable
        let flags = PageFlags::PRESENT | PageFlags::EXECUTABLE | PageFlags::GLOBAL;
        let pte = flags_to_pte(flags);
        assert_eq!(pte_to_flags(pte), flags);
    }

    #[test]
    fn test_pte_to_flags_roundtrip_read_only() {
        // Read-only page: PRESENT only (no WRITABLE, no EXECUTABLE)
        let flags = PageFlags::PRESENT | PageFlags::GLOBAL;
        let pte = flags_to_pte(flags);
        let recovered = pte_to_flags(pte);
        assert!(recovered.contains(PageFlags::PRESENT));
        assert!(!recovered.contains(PageFlags::WRITABLE), "read-only must not have WRITABLE");
        assert!(!recovered.contains(PageFlags::EXECUTABLE), "read-only must not have EXECUTABLE");
    }

    #[test]
    fn test_l0_index_4kb_page() {
        // 0x1000 = 4KB. L0 index should be 1.
        assert_eq!(l0_index(0x1000), 1);
    }

    #[test]
    fn test_l0_index_within_range() {
        // Any 39-bit VA should produce an L0 index < 512.
        let idx = l0_index(0x8020_1000);
        assert!(idx < 512, "L0 index must be < 512, got {}", idx);
    }

    #[test]
    fn test_l2_index_dram_base() {
        let idx = l2_index(0x8000_0000);
        assert_eq!(idx, 2, "DRAM_BASE should be at L2 index 2");
    }

    #[test]
    fn test_l1_index_2mb_offset() {
        let idx = l1_index(0x8020_0000);
        assert_eq!(idx, 0x401, "2MB offset from DRAM_BASE should be at L1 index 0x401");
    }

    #[test]
    fn test_paddr_to_pte_dram_base() {
        let pte = paddr_to_pte(0x8000_0000);
        assert_eq!(pte, 0x2000_0000, "paddr_to_pte(0x8000_0000) should be 0x2000_0000");
    }
}
