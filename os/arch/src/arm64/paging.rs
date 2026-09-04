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
        const AP1   = 1 << 5;   // AP[1]: 0=EL1 only, 1=EL0+EL1
        const AP2   = 1 << 6;   // AP[2]: 0=writable, 1=read-only
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
/// SAFETY: the channel's Direct Map window must be active; `paddr` must be
/// a valid 8-byte aligned PTE address. `vaddr_for_flush` is the virtual
/// address the PTE covers (used for TLB invalidation; pass 0 for
/// intermediate tables where no leaf TLB entry exists yet).
#[inline]
unsafe fn write_pte_dm(paddr: u64, value: u64, vaddr_for_flush: u64, channel: PteChannel) {
    core::ptr::write_volatile(channel_to_ptr(paddr, channel), value);
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
        let new_pte = (paddr.0 & ADDR_MASK) | flags_to_pte(flags);
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
                let new_pte = (paddr.0 & ADDR_MASK) | flags_to_pte(flags);
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
                // Preserve the physical address, replace only the flag bits.
                let new_pte = (pte & ADDR_MASK) | flags_to_pte(flags);
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
        // SAFETY: identity channel active; fresh leaf slot.
        unsafe { write_entry(l3_ptr, i3, (paddr.0 & ADDR_MASK) | pte_flags) };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}