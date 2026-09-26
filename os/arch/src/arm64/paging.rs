//! ARM64 (aarch64) paging implementation
//!
//! Boot-stage methods (`new_from_page`, `enable`, `map_huge`) work with the
//! UEFI identity-mapped memory (VA = PA) that is active before the MMU is
//! switched to our own page table.
//!
//! Runtime methods (`map`, `unmap`, `query`, `remap`, `update_flags`,
//! `new`, `destroy`) read and write page table entries through physical
//! addresses after the kernel's own page table is live. The Direct Map
//! window used for that access is pinned per-handle as a [`PteChannel`]:
//! kernel-context handles (`new_from_page`, `from_active_root`) use the
//! kernel Direct Map (`DirectMapArch::kernel_phys_to_virt`), VM-context
//! handles (`new`, `adopt_active_root`) use the VM Direct Map
//! (`DirectMapArch::vm_phys_to_virt`).
//!
//! # ARM64 page table architecture (4KB granule, 4-level)
//!
//! | Level | Name | Shift  | Entry size  | Page size |
//! |-------|------|--------|-------------|-----------|
//! | L0    | PGD  | 39     | 512 GB      | —         |
//! | L1    | PUD  | 30     | 1 GB        | 1 GB blk  |
//! | L2    | PMD  | 21     | 2 MB        | 2 MB blk  |
//! | L3    | PTE  | 12     | 4 KB        | 4 KB page |
//!
//! # PTE format (Stage 1, EL1&0)
//!
//! - Bit  0: Valid (1)
//! - Bit  1: Type (0=block/page, 1=table)
//! - Bit  5: AP[1]  (0=EL1 only, 1=EL0+EL1)
//! - Bit  6: AP[2]  (0=writable, 1=read-only at EL1)
//! - Bit 10: AF (Access Flag)
//! - Bit 11: nG (0=global, 1=non-global)
//! - Bit 53: PXN (Privileged Execute Never)
//! - Bit 54: XN  (Execute Never for EL0)

use crate::paging::{Paging, PageFlags, PageTableError};
use crate::paging_ext::HugePages;
use crate::direct_map::{DirectMapArch, AArch64DirectMap};
use crate::pte_walk_arch::PteWalkArch;
use minix_types::{PhysBytes, VirBytes};
use core::arch::asm;

bitflags::bitflags! {
    /// ARM64 page table entry flags (hardware encoding, Stage 1 EL1&0).
    ///
    /// This is the hardware-level PTE bit layout, separate from the
    /// OS-semantic `PageFlags`. Key differences from x86-64/RISC-V:
    /// - Bit 1 is Type (0=block/page, 1=table), not a permission flag
    /// - AP bits are inverted: AP2=1 means read-only, XN=1 means no-execute
    /// - AF (Access Flag) must be set or hardware may fault
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct Arm64PteFlags: u64 {
        const VALID = 1 << 0;   // Valid
        const TABLE = 1 << 1;   // Type: 0=block/page, 1=table
        /// Same bit as TABLE, named for the leaf level: ARM ARM requires
        /// bits[1:0] = 0b11 for a valid **page** descriptor at L3, exactly
        /// as for a table descriptor at L0-L2. `flags_to_pte` leaves it
        /// clear (correct for L1/L2 BLOCK descriptors), so every 4 KiB
        /// leaf writer must OR it in — the 0b01 encoding is RESERVED at
        /// L3 and the MMU takes a translation fault at level 3 on it.
        const PAGE  = 1 << 1;
        // AP[2:1] live at bits [7:6] (ARM ARM D8.3.1, stage-1 4 KiB page
        // descriptor); bit 5 is NS, NOT an AP bit. The pre-K12b values
        // (1<<5 / 1<<6) were each shifted one bit low: USER_ACCESSIBLE
        // set NS (granting nothing) and read-only marked AP[1] (granting
        // EL0!). Live-found by the aarch64 birth-chain carrier: the EL0
        // stack write took a permission fault level 3 while the text page
        // — writable through the mis-shifted AP[1] — fetched fine.
        const AP1   = 1 << 6;   // AP[1]: 0=EL1 only, 1=EL0+EL1
        const AP2   = 1 << 7;   // AP[2]: 0=writable, 1=read-only
        const AF    = 1 << 10;  // Access Flag
        const NG    = 1 << 11;  // non-Global: 0=global, 1=process-local
        const PXN   = 1 << 53;  // Privileged Execute Never
        const XN    = 1 << 54;  // Execute Never (EL0)
    }
}

const L0_SHIFT: u32 = 39;
const L1_SHIFT: u32 = 30;
const L2_SHIFT: u32 = 21;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

fn l0_index(vaddr: u64) -> usize {
    ((vaddr >> L0_SHIFT) & 0x1FF) as usize
}

fn l1_index(vaddr: u64) -> usize {
    ((vaddr >> L1_SHIFT) & 0x1FF) as usize
}

fn l2_index(vaddr: u64) -> usize {
    ((vaddr >> L2_SHIFT) & 0x1FF) as usize
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

fn flags_to_pte(flags: PageFlags) -> u64 {
    let mut pte = Arm64PteFlags::VALID | Arm64PteFlags::AF;
    // ARM64 AP[2] (bit 6): 0 = writable at EL1, 1 = read-only at EL1.
    // So we set AP2 only when the page is NOT writable.
    if !flags.contains(PageFlags::WRITABLE) {
        pte |= Arm64PteFlags::AP2;
    }
    if flags.contains(PageFlags::USER_ACCESSIBLE) {
        pte |= Arm64PteFlags::AP1;
    }
    if !flags.contains(PageFlags::GLOBAL) {
        pte |= Arm64PteFlags::NG;
    }
    if !flags.contains(PageFlags::EXECUTABLE) {
        pte |= Arm64PteFlags::XN;
    }
    pte.bits()
}

/// `flags_to_pte` for a 4 KiB **page** leaf (L3): adds the page bit.
///
/// Live-found on the aarch64 birth-chain carrier (edge1 K12b): the loader
/// mapped the user ELF with plain `flags_to_pte`, the software walk found
/// every entry (`query()` returned the right PA and flags), and the MMU
/// still refused the EL0 fetch — translation fault level 3 — because the
/// leaf read 0b01, a reserved encoding at L3 (the arm64 sibling of the
/// x86 "present bit" and riscv64 "V bit": without it the descriptor is
/// invisible to hardware).
fn flags_to_pte_page(flags: PageFlags) -> u64 {
    flags_to_pte(flags) | Arm64PteFlags::PAGE.bits()
}

/// Translate ARM64 hardware PTE flags into OS-semantic `PageFlags`.
///
/// Inverse of `flags_to_pte`. ARM64 uses inverted semantics for several
/// flags: AP2=1 means read-only, nG=1 means non-global, XN=1 means
/// non-executable. This function inverts those back to the normal
/// (set=enabled) `PageFlags` semantics.
///
/// Note: this does NOT set `PageFlags::HUGE_PAGE` — that flag is added by
/// the walk function when a block descriptor is encountered at L1 (1GB)
/// or L2 (2MB), because ARM64 has no dedicated "huge page" PTE bit; the
/// page size is determined by the table level, not a PTE flag.
fn pte_to_flags(pte: u64) -> PageFlags {
    let hw = Arm64PteFlags::from_bits_truncate(pte);
    let mut flags = PageFlags::empty();
    if hw.contains(Arm64PteFlags::VALID) {
        flags |= PageFlags::PRESENT;
    }
    // AP2=0 means writable (inverted).
    if !hw.contains(Arm64PteFlags::AP2) {
        flags |= PageFlags::WRITABLE;
    }
    if hw.contains(Arm64PteFlags::AP1) {
        flags |= PageFlags::USER_ACCESSIBLE;
    }
    // nG=0 means global (inverted).
    if !hw.contains(Arm64PteFlags::NG) {
        flags |= PageFlags::GLOBAL;
    }
    // XN=0 means executable (inverted).
    if !hw.contains(Arm64PteFlags::XN) {
        flags |= PageFlags::EXECUTABLE;
    }
    flags
}

pub struct AArch64Paging {
    root_paddr: u64,
    /// PTE access channel pinned at construction (see [`PteChannel`]).
    channel: PteChannel,
}

// ── Page table walk helpers (runtime, via Direct Map) ──

/// PTE access channel, pinned at handle construction.
///
/// ARM64 exposes two Direct Map windows with different privilege levels:
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
    match channel {
        PteChannel::KernelDm => {
            AArch64DirectMap::kernel_phys_to_virt(PhysBytes(phys)).0 as *mut u64
        }
        PteChannel::VmDm => AArch64DirectMap::vm_phys_to_virt(PhysBytes(phys)).0 as *mut u64,
    }
}

/// Read a PTE at the given physical address via the Direct Map.
///
/// SAFETY: the channel's Direct Map window must be active; `paddr` must be
/// a valid 8-byte aligned PTE address.
#[inline]
unsafe fn read_pte_dm(paddr: u64, channel: PteChannel) -> u64 {
    core::ptr::read_volatile(channel_to_ptr(paddr, channel))
}

/// Write a PTE at the given physical address via the Direct Map, with
/// a TLB invalidation for the affected virtual address.
///
/// Flush behavior is pinned to the channel, mirroring how the channel
/// itself pins the execution context (see the x86-64 sibling of this
/// function, which this documents identically):
/// - `KernelDm` handles run at EL1, so `tlbi vae1is` is legal and kept
///   for present→X transitions defense-in-depth.
/// - `VmDm` handles are exercised by VM at EL0, where `tlbi` is a
///   privileged instruction — executing it raises a synchronous exception
///   (EC=0 "Unknown reason"; live-found on the aarch64 boot: VM
///   `relocate` → `heap_arena_grow` → `map` faulted at `tlbi vae1is` with
///   `esr=0x02000000 far=0x0 spsr=0x60000000` (EL0t)). No flush is needed
///   there: the only writes such a handle performs during boot are
///   not-present → present (`map` refuses `AlreadyMapped`, and `walk_alloc`
///   only creates absent intermediates), and ARMv8 never caches a
///   translation derived from an invalid descriptor, so no stale TLB entry
///   can exist for the newly-installed one.
///   present→X transitions on a `VmDm` handle (unmap/remap of VM's own
///   live address space) would need a kernel-assisted flush, which is not
///   wired — the same VmDm flush gap as x86-64.
///
/// SAFETY: the channel's Direct Map window must be active; `paddr` must be
/// a valid 8-byte aligned PTE address. `vaddr_for_flush` is the virtual
/// address the PTE covers (used for TLB invalidation; pass 0 for
/// intermediate tables where no leaf TLB entry exists yet).
#[inline]
unsafe fn write_pte_dm(paddr: u64, value: u64, vaddr_for_flush: u64, channel: PteChannel) {
    core::ptr::write_volatile(channel_to_ptr(paddr, channel), value);
    if channel == PteChannel::KernelDm {
        // Flush any stale TLB entry for this virtual address. For intermediate
        // table descriptors (L0/L1/L2 table entries), no leaf TLB entry exists
        // yet, so the flush is a conservative no-op. For leaf PTE entries, this
        // ensures stale mappings are evicted.
        unsafe {
            asm!("dsb ishst");
            asm!("tlbi vae1is, {}", in(reg) vaddr_for_flush);
            asm!("dsb ish");
            asm!("isb");
        }
    }
    // VmDm: see doc comment — `tlbi` at EL0 faults, and the writes this
    // channel performs during boot (not-present → present) need no flush.
}

/// Result of a read-only walk down the 4-level ARM64 page table.
#[derive(Debug)]
enum WalkResult {
    /// Reached the leaf L3 page entry. Holds (leaf_pte_paddr, raw_pte).
    Leaf(u64, u64),
    /// Hit a 1GB block descriptor at L1 level. Holds (paddr, flags).
    Huge1G(PhysBytes, PageFlags),
    /// Hit a 2MB block descriptor at L2 level. Holds (paddr, flags).
    Huge2M(PhysBytes, PageFlags),
    /// Entry not present at some intermediate level.
    NotPresent,
}

/// Walk the 4-level table read-only, returning the leaf PTE address and
/// raw value, or the block mapping if encountered.
///
/// Does NOT allocate intermediate tables. Returns `NotPresent` if any
/// level's entry is absent.
///
/// ARM64 4-level walk (4KB granule, 48-bit VA):
/// - L0 (PGD, shift 39): table descriptor only (no block descriptors)
/// - L1 (PUD, shift 30): table or 1GB block descriptor
/// - L2 (PMD, shift 21): table or 2MB block descriptor
/// - L3 (PTE, shift 12): page descriptor (4KB page)
///
/// # Safety precondition (not enforced at compile time)
///
/// The Direct Map window selected by `channel` must be active — i.e.
/// paging is enabled with that window mapped (kernel: `PA=0` at
/// `KERNEL_DIRECT_MAP_BASE`; VM: the VM Direct Map window). Callers must
/// not invoke this before paging is enabled.
fn walk_read(root_paddr: u64, vaddr: u64, channel: PteChannel) -> WalkResult {
    let i0 = l0_index(vaddr);
    // SAFETY: channel's Direct Map active per function precondition; the L0
    // entry address is root_paddr + i0*8, within the root page.
    let l0e = unsafe { read_pte_dm(root_paddr + (i0 as u64) * 8, channel) };
    if l0e & Arm64PteFlags::VALID.bits() == 0 {
        return WalkResult::NotPresent;
    }
    // L0 entries are always table descriptors (bit1=1); descend to L1.
    let l1 = l0e & ADDR_MASK;

    let i1 = l1_index(vaddr);
    // SAFETY: see above; L1 entry address is within the L1 page.
    let l1e = unsafe { read_pte_dm(l1 + (i1 as u64) * 8, channel) };
    if l1e & Arm64PteFlags::VALID.bits() == 0 {
        return WalkResult::NotPresent;
    }
    // 1GB block descriptor: VALID + !TABLE (bit1=0).
    if l1e & Arm64PteFlags::TABLE.bits() == 0 {
        let paddr = (l1e & ADDR_MASK) | (vaddr & 0x3FFF_FFFF);
        let mut flags = pte_to_flags(l1e);
        flags |= PageFlags::HUGE_PAGE;
        return WalkResult::Huge1G(PhysBytes(paddr), flags);
    }
    let l2 = l1e & ADDR_MASK;

    let i2 = l2_index(vaddr);
    // SAFETY: see above; L2 entry address is within the L2 page.
    let l2e = unsafe { read_pte_dm(l2 + (i2 as u64) * 8, channel) };
    if l2e & Arm64PteFlags::VALID.bits() == 0 {
        return WalkResult::NotPresent;
    }
    // 2MB block descriptor: VALID + !TABLE (bit1=0).
    if l2e & Arm64PteFlags::TABLE.bits() == 0 {
        let paddr = (l2e & ADDR_MASK) | (vaddr & 0x1F_FFFF);
        let mut flags = pte_to_flags(l2e);
        flags |= PageFlags::HUGE_PAGE;
        return WalkResult::Huge2M(PhysBytes(paddr), flags);
    }
    let l3 = l2e & ADDR_MASK;

    let l3_idx = ((vaddr >> 12) & 0x1FF) as u64;
    let leaf_paddr = l3 + l3_idx * 8;
    // SAFETY: see above; L3 entry address is within the L3 page.
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
        WalkResult::Leaf(_leaf_paddr, pte) if pte & Arm64PteFlags::VALID.bits() != 0 => {
            // For 4KB leaf pages, the offset within the page comes from
            // the low 12 bits of vaddr.
            let offset = vaddr & 0xFFF;
            Some((PhysBytes((pte & ADDR_MASK) | offset), pte_to_flags(pte)))
        }
        WalkResult::Huge1G(paddr, flags) | WalkResult::Huge2M(paddr, flags) => {
            // For block descriptors, the offset is the low bits of vaddr
            // below the block boundary (already folded into paddr by
            // walk_read).
            Some((paddr, flags))
        }
        _ => None,
    }
}

/// ZST implementor of `PteWalkArch` for aarch64.
///
/// Delegates to `walk_translate`, which reuses the same `walk_read`
/// helper that powers `Paging::query`. This ensures the offline walk
/// (used by the kernel for cross-space copy) and the live walk (used
/// by `Paging::query`) produce identical results.
pub struct AArch64PteWalk;

impl PteWalkArch for AArch64PteWalk {
    fn walk(root_paddr: PhysBytes, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        walk_translate(root_paddr.0, vaddr.0)
    }
}

/// Walk the 4-level table, allocating intermediate tables (L1/L2/L3)
/// when not present. Returns the physical address of the leaf L3 PTE slot.
///
/// Returns `AllocationFailed` if `alloc_pt_page` fails, or
/// `AlreadyMapped` if a block descriptor blocks the 4KB walk.
///
/// # Safety precondition (not enforced at compile time)
///
/// The Direct Map window selected by `channel` must be active. See
/// `walk_read`.
fn walk_alloc(root_paddr: u64, vaddr: u64, channel: PteChannel) -> Result<u64, PageTableError> {
    let i0 = l0_index(vaddr);
    // SAFETY: channel's Direct Map active per function precondition.
    let l0e = unsafe { read_pte_dm(root_paddr + (i0 as u64) * 8, channel) };
    let l1 = if l0e & Arm64PteFlags::VALID.bits() == 0 {
        // Allocate a new L1 table page.
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = phys.0 | (Arm64PteFlags::VALID | Arm64PteFlags::AF | Arm64PteFlags::TABLE).bits();
        // SAFETY: channel's Direct Map active; L0 entry slot is 8-byte aligned.
        unsafe { write_pte_dm(root_paddr + (i0 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        l0e & ADDR_MASK
    };

    let i1 = l1_index(vaddr);
    // SAFETY: see above.
    let l1e = unsafe { read_pte_dm(l1 + (i1 as u64) * 8, channel) };
    if l1e & Arm64PteFlags::VALID.bits() != 0 && l1e & Arm64PteFlags::TABLE.bits() == 0 {
        // A 1GB block descriptor already occupies this slot — cannot
        // install a 4KB page without demoting the block.
        return Err(PageTableError::AlreadyMapped);
    }
    let l2 = if l1e & Arm64PteFlags::VALID.bits() == 0 {
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = phys.0 | (Arm64PteFlags::VALID | Arm64PteFlags::AF | Arm64PteFlags::TABLE).bits();
        unsafe { write_pte_dm(l1 + (i1 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        l1e & ADDR_MASK
    };

    let i2 = l2_index(vaddr);
    // SAFETY: see above.
    let l2e = unsafe { read_pte_dm(l2 + (i2 as u64) * 8, channel) };
    if l2e & Arm64PteFlags::VALID.bits() != 0 && l2e & Arm64PteFlags::TABLE.bits() == 0 {
        // A 2MB block descriptor already occupies this slot.
        return Err(PageTableError::AlreadyMapped);
    }
    let l3 = if l2e & Arm64PteFlags::VALID.bits() == 0 {
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = phys.0 | (Arm64PteFlags::VALID | Arm64PteFlags::AF | Arm64PteFlags::TABLE).bits();
        unsafe { write_pte_dm(l2 + (i2 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        l2e & ADDR_MASK
    };

    let l3_idx = ((vaddr >> 12) & 0x1FF) as u64;
    Ok(l3 + l3_idx * 8)
}

impl Paging for AArch64Paging {
    const PAGE_SIZE: usize = 4096;

    /// Uses a pre-allocated physical page for the root L0 (PGD) table.
    ///
    /// During boot, the caller (UEFI shim) allocates a page via
    /// `AllocatePages` and passes it here. There is no kernel page allocator
    /// at this point, so `new()` (which would allocate internally) cannot be
    /// used. See `Paging::new()` for the runtime variant.
    fn new_from_page(root_page: PhysBytes) -> Self {
        let ptr = unsafe { phys_to_ptr(root_page.0) };
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Self { root_paddr: root_page.0, channel: PteChannel::KernelDm }
    }

    /// Wrap an already-active L0 translation table root without zeroing it.
    ///
    /// Unlike `new_from_page`, this constructor assumes the root page table
    /// at `root_phys` is already initialized and currently loaded into
    /// TTBR0_EL1 (via a prior `enable()` call). It creates a handle that
    /// can perform `map`/`remap`/`query` operations on the live page table.
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
    /// - `root_phys` must point to a valid, 4KB-aligned L0 translation table.
    /// - The L0 table must be currently loaded into TTBR0_EL1 (or accessible
    ///   via the kernel Direct Map, which `walk_read`/`walk_alloc` rely on).
    /// - The returned handle must not outlive the page table it wraps.
    fn from_active_root(root_phys: PhysBytes) -> Self {
        Self { root_paddr: root_phys.0, channel: PteChannel::KernelDm }
    }

    fn inherit_supervisor_half(&mut self, source_root: PhysBytes) {
        // L0 entries 256..512 = the TTBR1 region (VAs ≥
        // 0xffff_0000_0000_0000, T1SZ=16): kernel text/data and the DM
        // windows. This crate pins TTBR0_EL1 and TTBR1_EL1 to the SAME
        // root page (`enable`), so the "two tables" split lives inside
        // one 512-entry L0 and the upper half is exactly the kernel's.
        // Raw entry copy — the entries may point at shared L1/L2/L3
        // table pages, which is sound because TTBR0 walks (user VAs,
        // entries 0..256) never touch the upper half (see trait doc).
        for i in 256usize..512 {
            let slot = (i as u64) * 8;
            // SAFETY: both roots are 4KB-aligned page-table pages in RAM;
            // the KernelDm direct map (pinned by `new_from_page` /
            // `from_active_root`) makes every descriptor slot addressable.
            // Same access pattern as `walk_alloc`; cache maintenance is
            // not required before first use — the root is not loaded into
            // any TTBR until a later `enable()`/`switch()`.
            let entry =
                unsafe { read_pte_dm(source_root.0 + slot, PteChannel::KernelDm) };
            unsafe {
                write_pte_dm(self.root_paddr + slot, entry, 0, PteChannel::KernelDm)
            };
        }
    }

    /// Wrap an already-active L0 translation table root for VM-context access.
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
            // MAIR_EL1: Attr0 = Normal WBWA (0xFF).
            // UEFI firmware may have set its own MAIR; we override to ensure
            // the sole memory attribute (Normal Write-Back) is available.
            asm!("msr mair_el1, {}", in(reg) 0xFFu64);

            // TCR_EL1: Translation Control Register.
            // 4KB granule, 48-bit VA (T0SZ=16, T1SZ=16), 48-bit PA (IPS=5).
            // UEFI firmware configures TCR for its own page tables; we must
            // reconfigure it for ours (same granule/size is typical but not
            // guaranteed by the UEFI spec).
            let tcr: u64 = (16 << 0)
                | (0 << 6)
                | (0 << 7)
                | (1 << 8)
                | (1 << 10)
                | (3 << 12)
                | (0 << 14)
                | (16 << 16)
                | (0 << 22)
                | (0 << 23)
                | (1 << 24)
                | (1 << 26)
                | (3 << 28)
                | (2 << 30)
                | (5 << 32)
                | (0 << 36)
                | (0 << 37);
            asm!("msr tcr_el1, {}", in(reg) tcr);

            // Set both TTBR0 (user space) and TTBR1 (kernel space) to our
            // root page table. UEFI firmware sets these for its own use;
            // we replace them with ours which contains identity + kernel
            // mappings built by map_huge().
            asm!("msr ttbr0_el1, {}", in(reg) self.root_paddr);
            asm!("msr ttbr1_el1, {}", in(reg) self.root_paddr);

            asm!("dsb sy");
            asm!("isb");

            // Enable MMU (SCTLR_EL1.M = 1).
            // UEFI firmware already has the MMU on, but we must re-enable
            // it after changing MAIR/TCR/TTBR (per ARM ARM, changes only
            // take effect after SCTLR.M is toggled or on a subsequent
            // context synchronization event).
            let mut sctlr: u64;
            asm!("mrs {}, sctlr_el1", out(reg) sctlr);
            sctlr |= 1;
            asm!("msr sctlr_el1, {}", in(reg) sctlr);
            asm!("isb");
        }
        PhysBytes(self.root_paddr)
    }

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized,
    {
        // Allocate the root L0 (PGD) page via the registered page-table
        // allocator. The allocator (boot bump or VM-side) is responsible
        // for zero-filling; we additionally zero here for defense-in-depth.
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
        // Full reclaim requires a free function registered with pt_alloc
        // (currently only alloc is registered). Without free, we zero the
        // root L0 to prevent use-after-free if the physical page is reused,
        // and accept the intermediate-table leak.
        //
        // SAFETY: the handle's Direct Map channel must be active;
        // root_paddr is the physical address of our L0 (PGD) page.
        let ptr = channel_to_ptr(self.root_paddr, self.channel);
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
        // Walk to the leaf L3 PTE address, allocating intermediate tables.
        let leaf_paddr = walk_alloc(self.root_paddr, vaddr.0, self.channel)?;
        // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned PTE slot.
        let pte = unsafe { read_pte_dm(leaf_paddr, self.channel) };
        if pte & Arm64PteFlags::VALID.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
        let new_pte = (paddr.0 & ADDR_MASK) | flags_to_pte_page(flags);
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
                let old = if old_pte & Arm64PteFlags::VALID.bits() != 0 {
                    Some((PhysBytes(old_pte & ADDR_MASK), pte_to_flags(old_pte)))
                } else {
                    None
                };
                let new_pte = (paddr.0 & ADDR_MASK) | flags_to_pte_page(flags);
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
            WalkResult::Leaf(leaf_paddr, pte) if pte & Arm64PteFlags::VALID.bits() != 0 => {
                let old_paddr = PhysBytes(pte & ADDR_MASK);
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
            WalkResult::Leaf(leaf_paddr, pte) if pte & Arm64PteFlags::VALID.bits() != 0 => {
                // Preserve the physical address and the descriptor level
                // (bit 1: page at L3, block at L1/L2), replace only the
                // flag bits.
                let new_pte =
                    (pte & (ADDR_MASK | Arm64PteFlags::PAGE.bits())) | flags_to_pte(flags);
                // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned.
                unsafe { write_pte_dm(leaf_paddr, new_pte, vaddr.0, self.channel) };
                Ok(())
            }
            _ => Err(PageTableError::NotMapped),
        }
    }

    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        match walk_read(self.root_paddr, vaddr.0, self.channel) {
            WalkResult::Leaf(_leaf_paddr, pte) if pte & Arm64PteFlags::VALID.bits() != 0 => {
                // For 4KB leaf pages, the offset within the page comes from
                // the low 12 bits of vaddr.
                let offset = vaddr.0 & 0xFFF;
                Some((PhysBytes((pte & ADDR_MASK) | offset), pte_to_flags(pte)))
            }
            WalkResult::Huge1G(paddr, flags) | WalkResult::Huge2M(paddr, flags) => {
                // For block descriptors, the offset is the low bits of vaddr
                // below the block boundary (already folded into paddr by
                // walk_read).
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
            asm!("dsb sy");
            asm!("msr ttbr1_el1, {}", in(reg) self.root_paddr);
            asm!("isb");
        }
    }

    unsafe fn flush_tlb(&self) {
        unsafe {
            asm!("dsb sy");
            asm!("tlbi vmalle1is");
            asm!("dsb sy");
            asm!("isb");
        }
    }

    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes) {
        unsafe {
            asm!("dsb sy");
            asm!("tlbi vae1is, {}", in(reg) vaddr.0);
            asm!("dsb sy");
            asm!("isb");
        }
    }

    /// Split the huge leaf covering `vaddr` into 4 KiB leaves, preserving
    /// every translation inside it (same frames, same per-leaf flags).
    /// Mirrors `x86_64::paging::split_huge` (see trait contract, `arch/
    /// paging.rs::split_huge`, which explicitly calls out arm64 as the
    /// future consumer of the boot ELF eviction path).
    ///
    /// Arm64 has two huge levels — a 1 GiB Block at L1 and a 2 MiB Block
    /// at L2 — so the split runs the same two passes as x86-64:
    /// - L1 Block: allocate an L2 table page, fill all 512 slots with 2 MiB
    ///   Block descriptors carrying the parent's flag bits and PA
    ///   `leaf_pa + i*2 MiB`, then convert the L1 slot into a Table
    ///   descriptor.
    /// - L2 Block: allocate an L3 table page, fill all 512 slots with 4 KiB
    ///   Page descriptors carrying the parent's flag bits ORed with the
    ///   PAGE type bit and PA `leaf_pa + i*4 KiB`, then convert the L2 slot
    ///   into a Table descriptor.
    ///
    /// After any split, we take the equivalent of x86's CR3 reload: a
    /// broadcast `tlbi vmalle1is`. A huge leaf can populate TLB entries for
    /// many VAs inside its range, so per-VA `tlbi vae1is` on the caller's
    /// `vaddr` alone is not enough — stale entries for neighbours would
    /// keep the caller's later `unmap`/`map` pair non-coherent.
    fn split_huge(&mut self, vaddr: VirBytes) -> Result<bool, PageTableError> {
        let ch = self.channel;
        // The broadcast `tlbi vmalle1is` below is an EL1 privileged
        // instruction, so a `VmDm` (EL0) handle would fault exactly like
        // `write_pte_dm`'s flush does (§1.111). Every current caller is a
        // kernel-context handle (`kernel/src/lib.rs` boot eviction,
        // `arch::boot`); assert the invariant here rather than only in
        // prose so a future VmDm caller is caught in debug builds.
        debug_assert!(
            matches!(ch, PteChannel::KernelDm),
            "split_huge requires a KernelDm (EL1) handle; VmDm would fault at tlbi"
        );
        // Table-descriptor encoding for slots installed by this split.
        // Same bits as `walk_alloc`/`map_huge` intermediates (`VALID |
        // AF | TABLE`); no APTable/UXNTable/PXNTable restriction bits —
        // boot intermediates never impose those and this split must not
        // introduce them.
        const TABLE_ENTRY: u64 =
            Arm64PteFlags::VALID.bits() | Arm64PteFlags::AF.bits() | Arm64PteFlags::TABLE.bits();
        let valid = Arm64PteFlags::VALID.bits();
        let table = Arm64PteFlags::TABLE.bits();
        let mut split = false;

        let i0 = l0_index(vaddr.0);
        // SAFETY: the caller's channel Direct Map is active (walk_read
        // precondition); the L0 slot lives inside the root page.
        let l0e = unsafe { read_pte_dm(self.root_paddr + (i0 as u64) * 8, ch) };
        if l0e & valid == 0 {
            return Ok(false);
        }
        // L0 has no Block encoding on arm64 (bit 1 must be 1) — an
        // absent-table L0 slot would have to be a translation fault
        // already, so treat `!TABLE` here as “nothing to split”.
        if l0e & table == 0 {
            return Ok(false);
        }
        let l1_pa = l0e & ADDR_MASK;

        let i1 = l1_index(vaddr.0);
        // SAFETY: channel DM active; L1 slot inside the L1 page.
        let mut l1e = unsafe { read_pte_dm(l1_pa + (i1 as u64) * 8, ch) };
        if l1e & valid != 0 && l1e & table == 0 {
            // 1 GiB Block leaf → build an L2 of 2 MiB Blocks and descend.
            let leaf_pa = l1e & ADDR_MASK;
            let leaf_bits = l1e & !ADDR_MASK;
            let (l2_phys, _v) = crate::pt_alloc::alloc_pt_page()?;
            for i in 0..512usize {
                // SAFETY: fresh page-table frame reachable through the
                // active Direct Map; slot 8-byte aligned.
                //
                // Parenthesised on purpose: Rust's `+` binds tighter than
                // `<<`, so a bare `leaf_pa + i << 21` parses as
                // `(leaf_pa+i) << 21` and writes a bogus PA for any
                // non-zero leaf_pa (mirrors the x86 F7 lesson).
                unsafe {
                    write_pte_dm(
                        l2_phys.0 + (i as u64) * 8,
                        leaf_pa + ((i as u64) << L2_SHIFT) | leaf_bits,
                        0,
                        ch,
                    )
                };
            }
            let new_l1e = l2_phys.0 | TABLE_ENTRY;
            // SAFETY: L1 slot address is within the live L1 page.
            unsafe { write_pte_dm(l1_pa + (i1 as u64) * 8, new_l1e, vaddr.0, ch) };
            split = true;
            l1e = new_l1e;
        }
        if l1e & valid == 0 {
            return Ok(split);
        }
        if l1e & table == 0 {
            // Shouldn't reach here (handled by the branch above), but
            // guard against the caller racing us with a still-block L1.
            return Ok(split);
        }
        let l2_pa = l1e & ADDR_MASK;

        let i2 = l2_index(vaddr.0);
        // SAFETY: channel DM active; L2 slot inside the live L2 page (the
        // pre-existing one or the one just installed above).
        let l2e = unsafe { read_pte_dm(l2_pa + (i2 as u64) * 8, ch) };
        if l2e & valid != 0 && l2e & table == 0 {
            // 2 MiB Block leaf → build an L3 of 4 KiB Pages and rewrite
            // the L2 slot as a Table descriptor. Children differ from
            // their parent by the PAGE type bit (bit 1): the parent was
            // `VALID | flagbits` (bit 1 = 0), children must be
            // `VALID | PAGE | flagbits` (bit 1 = 1). Not clearing this
            // is the arm64 sibling of the x86 "PS bit is reserved at
            // PT level" hazard — 0b01 is a reserved encoding at L3.
            let leaf_pa = l2e & ADDR_MASK;
            let leaf_bits = (l2e & !ADDR_MASK) | Arm64PteFlags::PAGE.bits();
            let (l3_phys, _v) = crate::pt_alloc::alloc_pt_page()?;
            for i in 0..512usize {
                // SAFETY: fresh page-table frame reachable through the
                // active Direct Map; slot 8-byte aligned. Explicit
                // parenthesisation for the same reason as the L1 pass.
                unsafe {
                    write_pte_dm(
                        l3_phys.0 + (i as u64) * 8,
                        leaf_pa + ((i as u64) << 12) | leaf_bits,
                        0,
                        ch,
                    )
                };
            }
            // SAFETY: L2 slot address is within the live L2 page.
            unsafe {
                write_pte_dm(
                    l2_pa + (i2 as u64) * 8,
                    l3_phys.0 | TABLE_ENTRY,
                    vaddr.0,
                    ch,
                )
            };
            split = true;
        }

        if split {
            // Broadcast flush (inner-shareable, all priv levels for this
            // VMID/ASID): equivalent to x86's CR3 reload. The just-split
            // huge leaf may have populated TLB entries for any VA in its
            // 2 MiB / 1 GiB range; per-VA invalidation on `vaddr` alone
            // would leave neighbours pointing at the old Block.
            // SAFETY: boot context (single CPU up to AP bringup); the
            // instruction is architecturally an inner-shareable
            // broadcast and requires no EL privileges beyond EL1.
            unsafe {
                asm!("dsb sy");
                asm!("tlbi vmalle1is");
                asm!("dsb sy");
                asm!("isb");
            }
        }
        Ok(split)
    }

    /// Ensure the intermediates on the walk to `vaddr` do not veto EL0
    /// access, so a user leaf installed by the caller after this returns
    /// Ok is actually reachable from EL0.
    ///
    /// Arm64 differs from x86 here: Table descriptors do not carry a
    /// positive "user" bit; they carry a NEGATIVE restriction (`APTable`
    /// at bit 60) which, when set, forces the child's AP[1] to 0 for EL0
    /// access. The boot identity and this crate's `walk_alloc`/`map_huge`
    /// intermediates are built without `APTable` (they OR only
    /// `VALID | AF | TABLE`), so on the well-formed path this method is a
    /// walk-and-verify no-op. If any intermediate does carry `APTable`,
    /// we clear it — matching x86's contract of “the caller can rely on
    /// intermediates not vetoing the leaf's user bit”.
    ///
    /// Contract (mirrors `x86_64::paging::grant_user_walk`):
    /// - `NotMapped` if any intermediate is absent — the caller must go
    ///   through `map`/`walk_alloc`, which creates fresh intermediates.
    /// - `NotSupported` if a Block (huge leaf) is met mid-walk — the
    ///   caller must `split_huge` first; granting user on the whole
    ///   1 GiB / 2 MiB range would be a security regression.
    fn grant_user_walk(&mut self, vaddr: VirBytes) -> Result<(), PageTableError> {
        let ch = self.channel;
        const APTABLE: u64 = 1 << 60;
        let valid = Arm64PteFlags::VALID.bits();
        let table = Arm64PteFlags::TABLE.bits();

        let i0 = l0_index(vaddr.0);
        // SAFETY: channel DM active; L0 slot inside the root page.
        let l0e = unsafe { read_pte_dm(self.root_paddr + (i0 as u64) * 8, ch) };
        if l0e & valid == 0 {
            return Err(PageTableError::NotMapped);
        }
        if l0e & table == 0 {
            // L0 blocks don't exist (bit 1 forced to 1 by arch); treat
            // any unexpected encoding as NotSupported (caller must fix
            // the tree) rather than silently corrupting it.
            return Err(PageTableError::NotSupported);
        }
        if l0e & APTABLE != 0 {
            // SAFETY: L0 slot inside root page.
            unsafe { write_pte_dm(self.root_paddr + (i0 as u64) * 8, l0e & !APTABLE, 0, ch) };
        }
        let l1_pa = l0e & ADDR_MASK;

        let i1 = l1_index(vaddr.0);
        let slot1 = l1_pa + (i1 as u64) * 8;
        // SAFETY: L1 slot inside the L1 page.
        let l1e = unsafe { read_pte_dm(slot1, ch) };
        if l1e & valid == 0 {
            return Err(PageTableError::NotMapped);
        }
        if l1e & table == 0 {
            return Err(PageTableError::NotSupported);
        }
        if l1e & APTABLE != 0 {
            // SAFETY: L1 slot inside the L1 page.
            unsafe { write_pte_dm(slot1, l1e & !APTABLE, vaddr.0, ch) };
        }
        let l2_pa = l1e & ADDR_MASK;

        let i2 = l2_index(vaddr.0);
        let slot2 = l2_pa + (i2 as u64) * 8;
        // SAFETY: L2 slot inside the L2 page.
        let l2e = unsafe { read_pte_dm(slot2, ch) };
        if l2e & valid == 0 {
            return Err(PageTableError::NotMapped);
        }
        if l2e & table == 0 {
            return Err(PageTableError::NotSupported);
        }
        if l2e & APTABLE != 0 {
            // SAFETY: L2 slot inside the L2 page.
            unsafe { write_pte_dm(slot2, l2e & !APTABLE, vaddr.0, ch) };
        }
        Ok(())
    }
}

impl HugePages for AArch64Paging {
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21];
    const HUGE_PAGE_SIZE: u64 = 1 << 30;
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;
    const PTE_HUGE_IDENTIFIER_BIT: u64 = 0;

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        let pte_flags = flags_to_pte(flags);

        let l0 = unsafe { phys_to_ptr(self.root_paddr) };
        let i0 = l0_index(vaddr.0);
        let e0 = unsafe { read_entry(l0, i0) };
        let l1 = if e0 & Arm64PteFlags::VALID.bits() != 0 {
            (e0 & ADDR_MASK) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(l0, i0, page | (Arm64PteFlags::VALID | Arm64PteFlags::AF | Arm64PteFlags::TABLE).bits()) };
            unsafe { phys_to_ptr(page) }
        };

        if size >= 1 << 30 {
            unsafe { write_entry(l1, l1_index(vaddr.0), (paddr.0 & ADDR_MASK) | pte_flags) };
            return Ok(());
        }

        let i1 = l1_index(vaddr.0);
        let e1 = unsafe { read_entry(l1, i1) };
        // Check if L1 entry is already a 1GB block (Valid + !Table).
        // If so, demoting it would corrupt the existing mapping.
        if e1 & Arm64PteFlags::VALID.bits() != 0 && e1 & Arm64PteFlags::TABLE.bits() == 0 {
            return Err(PageTableError::AlreadyMapped);
        }
        let l2 = if e1 & Arm64PteFlags::VALID.bits() != 0 {
            (e1 & ADDR_MASK) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(l1, i1, page | (Arm64PteFlags::VALID | Arm64PteFlags::AF | Arm64PteFlags::TABLE).bits()) };
            unsafe { phys_to_ptr(page) }
        };

        let i2 = l2_index(vaddr.0);
        unsafe { write_entry(l2, i2, (paddr.0 & ADDR_MASK) | pte_flags) };
        Ok(())
    }
}

// ── DM coverage establishment (boot identity write channel) ────────────────

/// ZST implementor of [`DmCoverageArch`] for AArch64.
///
/// The VM DM window (0x0000_1000_0000_0000, L0 slot 32) and the kernel DM
/// window (0xFFFF_8000_0000_0000, L0 slot 256) both lie outside the identity
/// range [0, 4 GiB) (L0 slot 0), so coverage establishment is pure
/// incremental mapping — no demotion path exists: any present slot inside a
/// DM window on the bootstrap root is a boot-layout bug and fails fast.
/// All table access goes through the identity mapping (`VA = PA`), which
/// stays untouched by these writes (§6.1 "DM 建立的自举写通道闭环").
pub struct AArch64DmCoverage;

impl crate::arch::dm_coverage::DmCoverageArch for AArch64DmCoverage {
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
        // bitflags' `|` is not const; combine raw bit patterns instead.
        const TABLE_ENTRY: u64 =
            Arm64PteFlags::VALID.bits() | Arm64PteFlags::AF.bits() | Arm64PteFlags::TABLE.bits();

        // L0 → L1 (branch allocation only — fresh slots).
        let i0 = l0_index(vaddr.0);
        let l0 = unsafe { phys_to_ptr(root_paddr.0) };
        let e0 = unsafe { read_entry(l0, i0) };
        let l1 = if e0 & Arm64PteFlags::VALID.bits() == 0 {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active (§6.1 boot write channel); L0
            // slot is within the root page.
            unsafe { write_entry(l0, i0, phys.0 | TABLE_ENTRY) };
            phys.0
        } else {
            e0 & ADDR_MASK
        };

        // L1 level: 1 GiB block or branch to L2.
        let i1 = l1_index(vaddr.0);
        let l1_ptr = unsafe { phys_to_ptr(l1) };
        let e1 = unsafe { read_entry(l1_ptr, i1) };
        let present1 = e1 & Arm64PteFlags::VALID.bits() != 0;
        let block1 = present1 && e1 & Arm64PteFlags::TABLE.bits() == 0;

        if size == 1 << 30 {
            if present1 {
                // Block or table — both impossible from a single candidate
                // pass into a fresh window; fail fast.
                return Err(PageTableError::AlreadyMapped);
            }
            // SAFETY: identity channel active; fresh slot. pte_flags carries
            // no TABLE bit → 1 GiB block descriptor.
            unsafe { write_entry(l1_ptr, i1, (paddr.0 & ADDR_MASK) | pte_flags) };
            return Ok(());
        }
        if block1 {
            // A 1 GiB block blocks the finer install.
            return Err(PageTableError::AlreadyMapped);
        }
        let l2 = if present1 {
            e1 & ADDR_MASK
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active; fresh slot.
            unsafe { write_entry(l1_ptr, i1, phys.0 | TABLE_ENTRY) };
            phys.0
        };

        // L2 level: 2 MiB block or branch to L3.
        let i2 = l2_index(vaddr.0);
        let l2_ptr = unsafe { phys_to_ptr(l2) };
        let e2 = unsafe { read_entry(l2_ptr, i2) };
        let present2 = e2 & Arm64PteFlags::VALID.bits() != 0;
        let block2 = present2 && e2 & Arm64PteFlags::TABLE.bits() == 0;

        if size == 1 << 21 {
            if present2 {
                return Err(PageTableError::AlreadyMapped);
            }
            // SAFETY: identity channel active; fresh slot.
            unsafe { write_entry(l2_ptr, i2, (paddr.0 & ADDR_MASK) | pte_flags) };
            return Ok(());
        }
        if block2 {
            return Err(PageTableError::AlreadyMapped);
        }
        let l3 = if present2 {
            e2 & ADDR_MASK
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active; fresh slot.
            unsafe { write_entry(l2_ptr, i2, phys.0 | TABLE_ENTRY) };
            phys.0
        };

        // L3 level: 4 KiB leaf.
        let i3 = ((vaddr.0 >> 12) & 0x1FF) as usize;
        let l3_ptr = unsafe { phys_to_ptr(l3) };
        let e3 = unsafe { read_entry(l3_ptr, i3) };
        if e3 & Arm64PteFlags::VALID.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
        // SAFETY: identity channel active; fresh 4 KiB leaf slot — the
        // page bit (bit 1) joins pte_flags here; the 1 GiB/2 MiB branches
        // above write BLOCK descriptors and must leave it clear.
        unsafe {
            write_entry(
                l3_ptr,
                i3,
                (paddr.0 & ADDR_MASK) | pte_flags | Arm64PteFlags::PAGE.bits(),
            )
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Descriptor-type encoding witness (live-found bug, edge1 K12b):
    /// 4 KiB page leaves must carry bits[1:0] = 0b11; block descriptors
    /// (plain `flags_to_pte`) keep 0b01. Getting this wrong is invisible
    /// to the software walk and fatal to the MMU.
    #[test]
    fn test_page_leaf_descriptor_type_bits() {
        let flags = PageFlags::PRESENT | PageFlags::USER_ACCESSIBLE | PageFlags::EXECUTABLE;
        let page = flags_to_pte_page(flags);
        let block = flags_to_pte(flags);
        assert_eq!(page & 0b11, 0b11, "4 KiB page leaf must read as 0b11 at L3");
        assert_eq!(block & 0b11, 0b01, "block descriptor keeps 0b01");
        // Everything except the type bit is identical between the two.
        assert_eq!(page ^ block, Arm64PteFlags::PAGE.bits());
    }

    #[test]
    fn test_flags_to_pte_kernel_read_write() {
        // kernel_read_write = PRESENT | WRITABLE | GLOBAL
        let flags = PageFlags::kernel_read_write();
        let pte = flags_to_pte(flags);
        let pte_flags = Arm64PteFlags::from_bits_truncate(pte);
        assert!(pte_flags.contains(Arm64PteFlags::VALID), "PTE must be valid");
        assert!(pte_flags.contains(Arm64PteFlags::AF), "PTE must have AF set");
        assert!(!pte_flags.contains(Arm64PteFlags::AP2), "writable page: AP2 must be 0");
        assert!(!pte_flags.contains(Arm64PteFlags::AP1), "kernel page: AP1 must be 0 (EL1 only)");
        assert!(!pte_flags.contains(Arm64PteFlags::NG), "global page: nG must be 0");
        assert!(pte_flags.contains(Arm64PteFlags::XN), "non-executable: XN must be set");
    }

    #[test]
    fn test_flags_to_pte_kernel_read_write_exec() {
        // kernel_read_write | EXECUTABLE
        let flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let pte = flags_to_pte(flags);
        let pte_flags = Arm64PteFlags::from_bits_truncate(pte);
        assert!(pte_flags.contains(Arm64PteFlags::VALID));
        assert!(!pte_flags.contains(Arm64PteFlags::AP2), "writable: AP2=0");
        assert!(!pte_flags.contains(Arm64PteFlags::XN), "executable: XN must be 0");
    }

    #[test]
    fn test_flags_to_pte_user_accessible() {
        let flags = PageFlags::kernel_read_write() | PageFlags::USER_ACCESSIBLE;
        let pte = flags_to_pte(flags);
        let pte_flags = Arm64PteFlags::from_bits_truncate(pte);
        assert!(pte_flags.contains(Arm64PteFlags::AP1), "user accessible: AP1 must be set");
    }

    #[test]
    fn test_flags_to_pte_read_only() {
        // PRESENT | GLOBAL (no WRITABLE)
        let flags = PageFlags::PRESENT | PageFlags::GLOBAL;
        let pte = flags_to_pte(flags);
        let pte_flags = Arm64PteFlags::from_bits_truncate(pte);
        assert!(pte_flags.contains(Arm64PteFlags::AP2), "read-only: AP2 must be set");
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
    fn test_pte_to_flags_inverted_xn() {
        // Non-executable page must have XN set → pte_to_flags clears EXECUTABLE
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE;
        let pte = flags_to_pte(flags);
        let recovered = pte_to_flags(pte);
        assert!(!recovered.contains(PageFlags::EXECUTABLE), "non-exec page must not have EXECUTABLE");
    }

    #[test]
    fn test_pte_to_flags_inverted_ap2() {
        // Read-only page: AP2=1 → pte_to_flags clears WRITABLE
        let flags = PageFlags::PRESENT | PageFlags::GLOBAL;
        let pte = flags_to_pte(flags);
        let recovered = pte_to_flags(pte);
        assert!(!recovered.contains(PageFlags::WRITABLE), "read-only page must not have WRITABLE");
    }

    #[test]
    fn test_l1_index_high_half() {
        // 0xFFFF_8000_0000_0000 should map to a valid L1 index
        let idx = l1_index(0xFFFF_8000_0000_0000);
        assert!(idx < 512, "L1 index must be < 512, got {}", idx);
    }

    #[test]
    fn test_addr_mask_preserves_physical() {
        let phys = 0x4020_0000u64; // aarch64 QEMU RAM + 2MB offset
        let masked = phys & ADDR_MASK;
        assert_eq!(masked, phys, "physical address should be preserved by ADDR_MASK");
    }

    /// F7-precedence witness (mirrors x86_64/paging.rs's same test): split
    /// huge writes child PAs as `leaf_pa + (i << SHIFT)`. Rust's `+` binds
    /// tighter than `<<`, so a bare `leaf_pa + i << SHIFT` parses as
    /// `(leaf_pa + i) << SHIFT` and yields a bogus PA for any non-zero
    /// 1 GiB/2 MiB-aligned `leaf_pa`. Pin the two forms to be
    /// distinguishable so a future style sweep cannot silently reintroduce
    /// the bug.
    #[test]
    fn split_huge_leaf_entry_arithmetic_is_base_plus_index_times_page() {
        let leaf_pa: u64 = 0x4000_0000; // non-zero, 1 GiB-aligned
        let i: u64 = 1;
        let naive = (leaf_pa + i) << L2_SHIFT;
        let fixed = leaf_pa + ((i as u64) << L2_SHIFT);
        assert_ne!(naive, fixed, "two forms must be distinguishable");
        assert_eq!(fixed, leaf_pa + 0x20_0000, "fixed form = leaf_pa + i*2 MiB");
    }

    /// Arm64 Table descriptor's APTable (bit 60) has OPPOSITE polarity from
    /// x86's USER bit: set = restrict children, clear = permissive. This is
    /// what `grant_user_walk` must clear (not set) on the walk path — a
    /// regression that flips this polarity would silently break EL0 access
    /// while keeping the software walk happy.
    #[test]
    fn grant_user_walk_clears_not_sets_aptable_polarity_is_negative() {
        const APTABLE: u64 = 1 << 60;
        // Simulated intermediate descriptor carrying an unintended
        // restriction: clearing must leave PA + type bits intact.
        let restricted = 0x1000_u64 /* child_pa */ | APTABLE
            | Arm64PteFlags::VALID.bits()
            | Arm64PteFlags::AF.bits()
            | Arm64PteFlags::TABLE.bits();
        let cleared = restricted & !APTABLE;
        assert_eq!(cleared & APTABLE, 0, "APTable must be cleared, not set");
        assert_eq!(
            cleared & (Arm64PteFlags::VALID | Arm64PteFlags::AF | Arm64PteFlags::TABLE).bits(),
            (Arm64PteFlags::VALID | Arm64PteFlags::AF | Arm64PteFlags::TABLE).bits(),
            "type/valid/AF bits must survive the clear"
        );
        assert_eq!(cleared & 0xFFFF_FFFF_F000, 0x1000, "child PA preserved");
    }
}
