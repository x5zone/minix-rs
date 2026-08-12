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
}

// ── Page table walk helpers (runtime, via Direct Map) ──

/// Convert a physical address to a kernel-virtual pointer via the Direct Map.
///
/// Runtime page table walk uses this to read/write PTEs through physical
/// addresses after the kernel page table is live. The boot-stage
/// `phys_to_ptr` (identity mapping) must NOT be used at runtime.
///
/// SAFETY: caller must ensure the Direct Map window is established
/// (paging is enabled with `KERNEL_DIRECT_MAP_BASE` mapped to PA=0).
#[inline]
fn phys_to_ptr_dm(phys: u64) -> *mut u64 {
    let vaddr = Riscv64DirectMap::kernel_phys_to_virt(PhysBytes(phys));
    vaddr.0 as *mut u64
}

/// Read a PTE at the given physical address via the Direct Map.
///
/// SAFETY: Direct Map must be active; `paddr` must be a valid 8-byte
/// aligned PTE address.
#[inline]
unsafe fn read_pte_dm(paddr: u64) -> u64 {
    core::ptr::read_volatile(phys_to_ptr_dm(paddr))
}

/// Write a PTE at the given physical address via the Direct Map, with
/// a TLB invalidation for the affected virtual address.
///
/// SAFETY: Direct Map must be active; `paddr` must be a valid 8-byte
/// aligned PTE address. `vaddr_for_flush` is the virtual address the
/// PTE covers (used for TLB invalidation; pass 0 for intermediate
/// tables where no leaf TLB entry exists yet).
#[inline]
unsafe fn write_pte_dm(paddr: u64, value: u64, vaddr_for_flush: u64) {
    core::ptr::write_volatile(phys_to_ptr_dm(paddr), value);
    // Flush any stale TLB entry for this virtual address. For intermediate
    // table entries (L2/L1 non-leaf), no leaf TLB entry exists yet, so the
    // flush is a conservative no-op. For leaf PTE entries, this ensures
    // stale mappings are evicted.
    // sfence.vma rs1=vaddr, rs2=x0 (all ASIDs).
    unsafe { asm!("sfence.vma {}, x0", in(reg) vaddr_for_flush) };
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
/// The kernel Direct Map must be active — i.e. paging is enabled with
/// `KERNEL_DIRECT_MAP_BASE` mapped to PA=0. This is true after
/// `Paging::enable()` returns.
fn walk_read(root_paddr: u64, vaddr: u64) -> WalkResult {
    let i2 = l2_index(vaddr);
    // SAFETY: Direct Map active per function precondition; the L2
    // entry address is root_paddr + i2*8, within the root page.
    let l2e = unsafe { read_pte_dm(root_paddr + (i2 as u64) * 8) };
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
    let l1e = unsafe { read_pte_dm(l1 + (i1 as u64) * 8) };
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
    let pte = unsafe { read_pte_dm(leaf_paddr) };
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
    match walk_read(root_paddr, vaddr) {
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
/// The kernel Direct Map must be active. See `walk_read`.
fn walk_alloc(root_paddr: u64, vaddr: u64) -> Result<u64, PageTableError> {
    let i2 = l2_index(vaddr);
    // SAFETY: Direct Map active per function precondition.
    let l2e = unsafe { read_pte_dm(root_paddr + (i2 as u64) * 8) };
    let l1 = if l2e & Sv39PteFlags::V.bits() == 0 {
        // Allocate a new L1 table page. Non-leaf PTE: V=1, R=W=X=0.
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = paddr_to_pte(phys.0) | Sv39PteFlags::V.bits();
        // SAFETY: Direct Map active; L2 entry slot is 8-byte aligned.
        unsafe { write_pte_dm(root_paddr + (i2 as u64) * 8, entry, 0) };
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
    let l1e = unsafe { read_pte_dm(l1 + (i1 as u64) * 8) };
    let l0 = if l1e & Sv39PteFlags::V.bits() == 0 {
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = paddr_to_pte(phys.0) | Sv39PteFlags::V.bits();
        unsafe { write_pte_dm(l1 + (i1 as u64) * 8, entry, 0) };
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
        Self { root_paddr: root_page.0 }
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
        Self { root_paddr: root_phys.0 }
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
        let (root_phys, _root_virt) = crate::pt_alloc::alloc_pt_page()?;
        let ptr = phys_to_ptr_dm(root_phys.0);
        // SAFETY: alloc_pt_page returns a fresh, 4KB-aligned page that is
        // not aliased by any other live reference. Direct Map must be active.
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Ok(Self { root_paddr: root_phys.0 })
    }

    unsafe fn destroy(&mut self) {
        // Full reclaim requires a free function registered with pt_alloc
        // (currently only alloc is registered). Without free, we zero the
        // root L2 to prevent use-after-free if the physical page is reused,
        // and accept the intermediate-table leak.
        //
        // SAFETY: Direct Map must be active; root_paddr is the physical
        // address of our L2 (root) page.
        let ptr = phys_to_ptr_dm(self.root_paddr);
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
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
        let leaf_paddr = walk_alloc(self.root_paddr, vaddr.0)?;
        // SAFETY: Direct Map active; leaf_paddr is 8-byte aligned PTE slot.
        let pte = unsafe { read_pte_dm(leaf_paddr) };
        if pte & Sv39PteFlags::V.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
        let new_pte = paddr_to_pte(paddr.0) | flags_to_pte(flags);
        // SAFETY: see above. Flush TLB for the target vaddr in case a
        // stale entry lingers from a prior unmap.
        unsafe { write_pte_dm(leaf_paddr, new_pte, vaddr.0) };
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
        match walk_read(self.root_paddr, vaddr.0) {
            WalkResult::Leaf(leaf_paddr, old_pte) => {
                let old = if old_pte & Sv39PteFlags::V.bits() != 0 {
                    Some((PhysBytes(pte_to_paddr(old_pte)), pte_to_flags(old_pte)))
                } else {
                    None
                };
                let new_pte = paddr_to_pte(paddr.0) | flags_to_pte(flags);
                // SAFETY: Direct Map active; leaf_paddr is 8-byte aligned.
                unsafe { write_pte_dm(leaf_paddr, new_pte, vaddr.0) };
                Ok(old)
            }
            // Not mapped and no leaf table — overwrite requires allocation,
            // which remap does not do (callers should use map() first).
            _ => Err(PageTableError::NotMapped),
        }
    }

    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
        match walk_read(self.root_paddr, vaddr.0) {
            WalkResult::Leaf(leaf_paddr, pte) if pte & Sv39PteFlags::V.bits() != 0 => {
                let old_paddr = PhysBytes(pte_to_paddr(pte));
                // SAFETY: Direct Map active; leaf_paddr is 8-byte aligned.
                // Clear the PTE (set to 0 = invalid) and flush TLB.
                unsafe { write_pte_dm(leaf_paddr, 0, vaddr.0) };
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
        match walk_read(self.root_paddr, vaddr.0) {
            WalkResult::Leaf(leaf_paddr, pte) if pte & Sv39PteFlags::V.bits() != 0 => {
                // Preserve the physical address (PPN), replace only flag bits.
                // PTE_PPN_MASK preserves bits [53:10]; flag bits are [9:0].
                let new_pte = (pte & PTE_PPN_MASK) | flags_to_pte(flags);
                // SAFETY: Direct Map active; leaf_paddr is 8-byte aligned.
                unsafe { write_pte_dm(leaf_paddr, new_pte, vaddr.0) };
                Ok(())
            }
            _ => Err(PageTableError::NotMapped),
        }
    }

    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        match walk_read(self.root_paddr, vaddr.0) {
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
