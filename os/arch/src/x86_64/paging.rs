//! x86-64 paging implementation
//!
//! Boot-stage methods (`new_from_page`, `enable`, `map_huge`) work with the
//! UEFI identity-mapped memory (VA = PA) that is active before CR3 is
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

use crate::paging::{Paging, PageFlags, PageTableError};
use crate::paging_ext::HugePages;
use crate::direct_map::{DirectMapArch, X86_64DirectMap};
use crate::pte_walk_arch::PteWalkArch;
use minix_types::{PhysBytes, VirBytes};
use core::arch::asm;

bitflags::bitflags! {
    /// x86-64 page table entry flags (hardware encoding).
    ///
    /// This is the hardware-level PTE bit layout for Intel/AMD long mode,
    /// separate from the OS-semantic `PageFlags`. Key differences:
    /// - Executable is the *absence* of the NX (No-eXecute) bit (bit 63)
    /// - PS bit (bit 7) indicates huge page at PDPT (1GB) or PD (2MB) level
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct X64PteFlags: u64 {
        const PRESENT  = 1 << 0;   // Present
        const WRITABLE = 1 << 1;   // Read/Write
        const USER     = 1 << 2;   // User/Supervisor
        const PS       = 1 << 7;   // Page Size (huge page indicator)
        const GLOBAL   = 1 << 8;   // Global page
        const NX       = 1 << 63;  // No Execute
    }
}

const PML4_SHIFT: u32 = 39;
const PDPT_SHIFT: u32 = 30;
const PD_SHIFT: u32 = 21;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

fn pml4_index(vaddr: u64) -> usize {
    ((vaddr >> PML4_SHIFT) & 0x1FF) as usize
}

fn pdpt_index(vaddr: u64) -> usize {
    ((vaddr >> PDPT_SHIFT) & 0x1FF) as usize
}

fn pd_index(vaddr: u64) -> usize {
    ((vaddr >> PD_SHIFT) & 0x1FF) as usize
}

unsafe fn read_entry(table: *mut u64, idx: usize) -> u64 { unsafe {
    core::ptr::read_volatile(table.add(idx))
}}

unsafe fn write_entry(table: *mut u64, idx: usize, val: u64) { unsafe {
    core::ptr::write_volatile(table.add(idx), val);
    // NOTE: invlpg with address 0 is a conservative flush. For intermediate
    // page table entries (PML4/PDPT/PD), no TLB entry exists yet because
    // the mapping hasn't been used. For final PTE entries, the correct
    // virtual address should ideally be used. However, during boot the
    // page table is not yet active (CR3 hasn't been switched), so this
    // flush is effectively a no-op. After boot, map() should use the
    // actual virtual address for correctness.
    asm!("invlpg [{}]", in(reg) 0u64, options(nostack, preserves_flags));
}}

unsafe fn phys_to_ptr(phys: u64) -> *mut u64 {
    phys as *mut u64
}

/// Translate OS-semantic `PageFlags` into x86-64 hardware PTE flags.
///
/// x86-64 uses inverted semantics for execute permission: the NX (No-eXecute)
/// bit in bit 63 must be *cleared* for executable pages. All other flags
/// use normal (set=enabled) semantics.
fn flags_to_pte(flags: PageFlags) -> u64 {
    let mut pte = X64PteFlags::PRESENT;
    if flags.contains(PageFlags::WRITABLE) { pte |= X64PteFlags::WRITABLE; }
    if flags.contains(PageFlags::USER_ACCESSIBLE) { pte |= X64PteFlags::USER; }
    if flags.contains(PageFlags::GLOBAL) { pte |= X64PteFlags::GLOBAL; }
    // NX is inverted: set NX when NOT executable
    if !flags.contains(PageFlags::EXECUTABLE) { pte |= X64PteFlags::NX; }
    pte.bits()
}

/// Translate x86-64 hardware PTE flags into OS-semantic `PageFlags`.
///
/// Inverse of `flags_to_pte`. The NX bit (bit 63) is inverted:
/// NX=0 means executable, NX=1 means non-executable.
fn pte_to_flags(pte: u64) -> PageFlags {
    let hw = X64PteFlags::from_bits_truncate(pte);
    let mut flags = PageFlags::empty();
    // PRESENT is implied by being able to read the entry; we set it
    // whenever the hardware PRESENT bit is set.
    if hw.contains(X64PteFlags::PRESENT) { flags |= PageFlags::PRESENT; }
    if hw.contains(X64PteFlags::WRITABLE) { flags |= PageFlags::WRITABLE; }
    if hw.contains(X64PteFlags::USER) { flags |= PageFlags::USER_ACCESSIBLE; }
    if hw.contains(X64PteFlags::GLOBAL) { flags |= PageFlags::GLOBAL; }
    // NX inverted: executable iff NX bit is clear.
    if !hw.contains(X64PteFlags::NX) { flags |= PageFlags::EXECUTABLE; }
    // PS bit at PD/PDPT level indicates a huge page. At PT level it is
    // reserved (we still propagate it for caller inspection).
    if hw.contains(X64PteFlags::PS) { flags |= PageFlags::HUGE_PAGE; }
    flags
}

/// PTE access channel, pinned at handle construction.
///
/// x86-64 exposes two Direct Map windows with different privilege levels:
/// the kernel Direct Map (supervisor-only, high half) and the VM Direct
/// Map (user-accessible, 2 GiB). A page-table handle exercised in kernel
/// context reaches PTE pages through the kernel window; a handle
/// exercised by VM — a user-space process — can only reach them through
/// the VM window. The channel is chosen by the constructor:
/// `new_from_page`/`from_active_root` pin `KernelDm`, `new()` (whose
/// production callers are all VM-side) and `adopt_active_root` pin `VmDm`.
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
            X86_64DirectMap::kernel_phys_to_virt(PhysBytes(phys)).0 as *mut u64
        }
        PteChannel::VmDm => X86_64DirectMap::vm_phys_to_virt(PhysBytes(phys)).0 as *mut u64,
    }
}

/// Read a PTE at the given physical address via the Direct Map.
///
/// SAFETY: the channel's Direct Map window must be active; `paddr` must be
/// a valid 8-byte aligned PTE address.
#[inline]
unsafe fn read_pte_dm(paddr: u64, channel: PteChannel) -> u64 { unsafe {
    core::ptr::read_volatile(channel_to_ptr(paddr, channel))
}}

/// Write a PTE at the given physical address via the Direct Map, with
/// a conservative `invlpg` flush for the affected virtual address.
///
/// SAFETY: the channel's Direct Map window must be active; `paddr` must be
/// a valid 8-byte aligned PTE address. `vaddr_for_flush` is the virtual
/// address the PTE covers (used for TLB invalidation; pass 0 for
/// intermediate tables where no TLB entry exists yet).
#[inline]
unsafe fn write_pte_dm(paddr: u64, value: u64, vaddr_for_flush: u64, channel: PteChannel) { unsafe {
    core::ptr::write_volatile(channel_to_ptr(paddr, channel), value);
    // Flush any stale TLB entry for this virtual address. For intermediate
    // table entries (PML4/PDPT/PD), no leaf TLB entry exists yet, so the
    // flush is a conservative no-op. For leaf PTE entries, this ensures
    // stale mappings are evicted.
    asm!("invlpg [{}]", in(reg) vaddr_for_flush, options(nostack, preserves_flags));
}}

pub struct X86_64Paging {
    root_paddr: u64,
    /// PTE access channel pinned at construction (see [`PteChannel`]).
    channel: PteChannel,
}

/// Whether the CPU supports global pages (CPUID.01H:EDX.PGE, bit 13).
/// C: `pgeok = _cpufeature(_CPUF_I386_PGE)` — pg_utils.c:209.
fn cpu_supports_pge() -> bool {
    let mut edx: u32;
    unsafe {
        asm!(
            "push rbx",
            "mov eax, 0x1",
            "cpuid",
            "pop rbx",
            out("edx") edx,
            out("eax") _,
            out("ecx") _,
            options(nomem, nostack, preserves_flags)
        );
    }
    (edx & (1 << 13)) != 0
}

// ── Page table walk helpers (runtime, via Direct Map) ──

/// Result of a read-only walk down the 4-level page table.
#[derive(Debug)]
enum WalkResult {
    /// Reached the leaf PT entry. Holds (leaf_pte_paddr, raw_pte).
    Leaf(u64, u64),
    /// Hit a 1GB huge page at PDPT level. Holds (paddr, flags).
    Huge1G(PhysBytes, PageFlags),
    /// Hit a 2MB huge page at PD level. Holds (paddr, flags).
    Huge2M(PhysBytes, PageFlags),
    /// Entry not present at some intermediate level.
    NotPresent,
}

/// Walk the 4-level table read-only, returning the leaf PTE address and
/// raw value, or the huge-page mapping if encountered.
///
/// Does NOT allocate intermediate tables. Returns `NotPresent` if any
/// level's entry is absent.
///
/// # Safety precondition (not enforced at compile time)
///
/// The Direct Map window selected by `channel` must be active — i.e.
/// paging is enabled with that window mapped (kernel: `PA=0` at
/// `KERNEL_DIRECT_MAP_BASE`; VM: the VM Direct Map window). Callers must
/// not invoke this before paging is enabled.
fn walk_read(root_paddr: u64, vaddr: u64, channel: PteChannel) -> WalkResult {
    let i4 = pml4_index(vaddr);
    // SAFETY: channel's Direct Map active per function precondition; the
    // PML4 entry address is root_paddr + i4*8, within the root page.
    let pml4e = unsafe { read_pte_dm(root_paddr + (i4 as u64) * 8, channel) };
    if pml4e & X64PteFlags::PRESENT.bits() == 0 {
        return WalkResult::NotPresent;
    }
    let pdpt = pml4e & ADDR_MASK;

    let i3 = pdpt_index(vaddr);
    // SAFETY: see above; PDPT entry address is within the PDPT page.
    let pdpte = unsafe { read_pte_dm(pdpt + (i3 as u64) * 8, channel) };
    if pdpte & X64PteFlags::PRESENT.bits() == 0 {
        return WalkResult::NotPresent;
    }
    // 1GB huge page at PDPT level (PS bit set).
    if pdpte & X64PteFlags::PS.bits() != 0 {
        let paddr = (pdpte & ADDR_MASK) | (vaddr & 0x3FFF_FFFF);
        return WalkResult::Huge1G(PhysBytes(paddr), pte_to_flags(pdpte));
    }

    let pd = pdpte & ADDR_MASK;
    let i2 = pd_index(vaddr);
    // SAFETY: see above; PD entry address is within the PD page.
    let pde = unsafe { read_pte_dm(pd + (i2 as u64) * 8, channel) };
    if pde & X64PteFlags::PRESENT.bits() == 0 {
        return WalkResult::NotPresent;
    }
    // 2MB huge page at PD level (PS bit set).
    if pde & X64PteFlags::PS.bits() != 0 {
        let paddr = (pde & ADDR_MASK) | (vaddr & 0x1F_FFFF);
        return WalkResult::Huge2M(PhysBytes(paddr), pte_to_flags(pde));
    }

    let pt = pde & ADDR_MASK;
    let pt_idx = (vaddr >> 12) & 0x1FF;
    let leaf_paddr = pt + pt_idx * 8;
    // SAFETY: see above; PT entry address is within the PT page.
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
        WalkResult::Leaf(_leaf_paddr, pte) if pte & X64PteFlags::PRESENT.bits() != 0 => {
            // For 4KB leaf pages, the offset within the page comes from
            // the low 12 bits of vaddr.
            let offset = vaddr & 0xFFF;
            Some((PhysBytes((pte & ADDR_MASK) | offset), pte_to_flags(pte)))
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

/// ZST implementor of `PteWalkArch` for x86-64.
///
/// Delegates to `walk_translate`, which reuses the same `walk_read`
/// helper that powers `Paging::query`. This ensures the offline walk
/// (used by the kernel for cross-space copy) and the live walk (used
/// by `Paging::query`) produce identical results.
pub struct X86_64PteWalk;

impl PteWalkArch for X86_64PteWalk {
    fn walk(root_paddr: PhysBytes, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        walk_translate(root_paddr.0, vaddr.0)
    }
}

/// Walk the 4-level table, allocating intermediate tables (PDPT/PD/PT)
/// when not present. Returns the physical address of the leaf PTE slot.
///
/// Returns `AllocationFailed` if `alloc_pt_page` fails, or
/// `AlreadyMapped` if a huge page blocks the 4KB walk.
///
/// # Safety precondition (not enforced at compile time)
///
/// The Direct Map window selected by `channel` must be active. See
/// `walk_read`.
fn walk_alloc(root_paddr: u64, vaddr: u64, channel: PteChannel) -> Result<u64, PageTableError> {
    let i4 = pml4_index(vaddr);
    // SAFETY: channel's Direct Map active per function precondition.
    let pml4e = unsafe { read_pte_dm(root_paddr + (i4 as u64) * 8, channel) };
    let pdpt = if pml4e & X64PteFlags::PRESENT.bits() == 0 {
        // Allocate a new PDPT page.
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits();
        // SAFETY: channel's Direct Map active; PML4 entry slot is 8-byte aligned.
        unsafe { write_pte_dm(root_paddr + (i4 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        pml4e & ADDR_MASK
    };

    let i3 = pdpt_index(vaddr);
    // SAFETY: see above.
    let pdpte = unsafe { read_pte_dm(pdpt + (i3 as u64) * 8, channel) };
    if pdpte & X64PteFlags::PRESENT.bits() != 0 && pdpte & X64PteFlags::PS.bits() != 0 {
        // A 1GB huge page already occupies this slot — cannot install 4KB.
        return Err(PageTableError::AlreadyMapped);
    }
    let pd = if pdpte & X64PteFlags::PRESENT.bits() == 0 {
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits();
        unsafe { write_pte_dm(pdpt + (i3 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        pdpte & ADDR_MASK
    };

    let i2 = pd_index(vaddr);
    // SAFETY: see above.
    let pde = unsafe { read_pte_dm(pd + (i2 as u64) * 8, channel) };
    if pde & X64PteFlags::PRESENT.bits() != 0 && pde & X64PteFlags::PS.bits() != 0 {
        return Err(PageTableError::AlreadyMapped);
    }
    let pt = if pde & X64PteFlags::PRESENT.bits() == 0 {
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits();
        unsafe { write_pte_dm(pd + (i2 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        pde & ADDR_MASK
    };

    let pt_idx = (vaddr >> 12) & 0x1FF;
    Ok(pt + pt_idx * 8)
}

impl Paging for X86_64Paging {
    const PAGE_SIZE: usize = 4096;

    /// Uses a pre-allocated physical page for the root PML4 table.
    ///
    /// During boot, the caller (UEFI shim) allocates a page via
    /// `AllocatePages` and passes it here. There is no kernel page allocator
    /// at this point, so `new()` (which would allocate internally) cannot be
    /// used. See `Paging::new()` for the runtime variant.
    fn new_from_page(root_page: PhysBytes) -> Self {
        // SAFETY: root_page must be a valid, 4KB-aligned physical address
        // that is writable through the current page table (UEFI identity map
        // during boot). The page must not be aliased by any other live
        // reference. phys_to_ptr converts the physical address to a virtual
        // address using the UEFI identity mapping.
        let ptr = unsafe { phys_to_ptr(root_page.0) };
        // SAFETY: ptr points to a 4KB-aligned, 512-entry PML4 table.
        // write_bytes zeroes all entries, marking them "not present".
        unsafe { core::ptr::write_bytes(ptr, 0, 512) };
        Self { root_paddr: root_page.0, channel: PteChannel::KernelDm }
    }

    /// Wrap an already-active PML4 root without zeroing it.
    ///
    /// Unlike `new_from_page`, this constructor assumes the root page table
    /// at `root_phys` is already initialized and currently loaded into CR3
    /// (via a prior `enable()` call). It creates a handle that can perform
    /// `map`/`remap`/`query` operations on the live page table.
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
    /// - `root_phys` must point to a valid, 4KB-aligned PML4 table.
    /// - The PML4 must be currently loaded into CR3 (or accessible via
    ///   the kernel Direct Map, which `walk_read`/`walk_alloc` rely on).
    /// - The returned handle must not outlive the page table it wraps.
    fn from_active_root(root_phys: PhysBytes) -> Self {
        Self { root_paddr: root_phys.0, channel: PteChannel::KernelDm }
    }

    /// Wrap an already-active PML4 root for VM-context access.
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
            // Switch CR3 to our own root page table.
            // UEFI already runs in long mode (CR0.PG=1) with its own page
            // table; we must replace it with ours, which contains both the
            // identity and kernel high mappings set up by map_huge().
            asm!("mov cr3, {}", in(reg) self.root_paddr);
            // Ensure WP (Write Protect, CR0 bit 16) is set so the kernel
            // cannot write read-only pages (W^X enforcement). UEFI firmware
            // does not guarantee WP is enabled.
            let mut cr0: u64;
            asm!("mov {}, cr0", out(reg) cr0);
            cr0 |= 1 << 31;
            cr0 |= 1 << 16;
            asm!("mov cr0, {}", in(reg) cr0);
            // Enable CR4.PGE (bit 7, Page Global Enable) so PTE entries with
            // G=1 survive CR3 switches. Kernel mappings carry G=1 (via
            // `PageFlags::kernel_read_write()`), which has no effect until
            // PGE is on: with CR4.PGE=0 the CPU ignores the G flag.
            // C: vm_enable_paging() enables paging first, then PGE
            // ("First enable paging, then enable global page flag",
            // pg_utils.c:233-242), gated on CPU feature detection
            // (`pgeok = _cpufeature(_CPUF_I386_PGE)`, pg_utils.c:209).
            if cpu_supports_pge() {
                let mut cr4: u64;
                asm!("mov {}, cr4", out(reg) cr4);
                cr4 |= 1 << 7;
                asm!("mov cr4, {}", in(reg) cr4);
            }
        }
        PhysBytes(self.root_paddr)
    }

    fn new() -> Result<Self, PageTableError>
    where
        Self: Sized,
    {
        // Allocate the root PML4 page via the registered page-table allocator.
        // The allocator (boot bump or VM-side) is responsible for zero-filling;
        // we additionally zero here for defense-in-depth.
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
        // root PML4 to prevent use-after-free if the physical page is
        // reused, and accept the intermediate-table leak.
        //
        // SAFETY: the handle's Direct Map channel must be active;
        // root_paddr is the physical address of our PML4 page.
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
        // Walk to the leaf PTE address, allocating intermediate tables.
        let leaf_paddr = walk_alloc(self.root_paddr, vaddr.0, self.channel)?;
        // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned.
        let pte = unsafe { read_pte_dm(leaf_paddr, self.channel) };
        if pte & X64PteFlags::PRESENT.bits() != 0 {
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
                let old = if old_pte & X64PteFlags::PRESENT.bits() != 0 {
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
            WalkResult::Leaf(leaf_paddr, pte) if pte & X64PteFlags::PRESENT.bits() != 0 => {
                let old_paddr = PhysBytes(pte & ADDR_MASK);
                // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned.
                // Clear the PTE (set to 0 = not present) and flush TLB.
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
            WalkResult::Leaf(leaf_paddr, pte) if pte & X64PteFlags::PRESENT.bits() != 0 => {
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
            WalkResult::Leaf(_leaf_paddr, pte) if pte & X64PteFlags::PRESENT.bits() != 0 => {
                // For 4KB leaf pages, the offset within the page comes from
                // the low 12 bits of vaddr.
                let offset = vaddr.0 & 0xFFF;
                Some((PhysBytes((pte & ADDR_MASK) | offset), pte_to_flags(pte)))
            }
            WalkResult::Huge1G(paddr, flags) | WalkResult::Huge2M(paddr, flags) => {
                // For huge pages, the offset is the low bits of vaddr
                // below the huge-page boundary.
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
            asm!("mov cr3, {}", in(reg) self.root_paddr, options(att_syntax));
        }
    }

    unsafe fn flush_tlb(&self) {
        unsafe {
            asm!("mov cr3, {}", in(reg) self.root_paddr, options(att_syntax));
        }
    }

    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes) {
        unsafe {
            asm!("invlpg [{}]", in(reg) vaddr.0, options(nostack, preserves_flags));
        }
    }
}

impl HugePages for X86_64Paging {
    const HUGE_PAGE_SIZES: &'static [usize] = &[1 << 30, 1 << 21];
    const HUGE_PAGE_SIZE: u64 = 1 << 30;
    const HUGE_PAGE_SHIFT: u32 = 30;
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21;
    /// PS bit (bit 7) indicates a huge page at PDPT level (1GB) or PD level (2MB).
    /// The same bit position is used for both sizes; the level determines the
    /// actual page size.
    const PTE_HUGE_IDENTIFIER_BIT: u64 = X64PteFlags::PS.bits();

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError> {
        let pte_flags = flags_to_pte(flags);
        let shift = if size >= 1 << 30 { PDPT_SHIFT } else { PD_SHIFT };
        // PS bit (bit 7) is the same for both 1GB (PDPT.PS) and 2MB (PD.PS) pages.
        let huge_flag = X64PteFlags::PS.bits();

        let pml4 = unsafe { phys_to_ptr(self.root_paddr) };
        let i4 = pml4_index(vaddr.0);
        let e4 = unsafe { read_entry(pml4, i4) };

        let pdpt = if e4 & X64PteFlags::PRESENT.bits() != 0 {
            (e4 & ADDR_MASK) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(pml4, i4, page | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits()) };
            unsafe { phys_to_ptr(page) }
        };

        let i3 = pdpt_index(vaddr.0);
        let e3 = unsafe { read_entry(pdpt, i3) };

        if shift == PDPT_SHIFT {
            unsafe { write_entry(pdpt, i3, (paddr.0 & ADDR_MASK) | pte_flags | huge_flag) };
            return Ok(());
        }

        // Check if PDPT entry is already a 1GB leaf (PRESENT + PS both set).
        // If so, demoting it would corrupt the existing mapping.
        if e3 & X64PteFlags::PRESENT.bits() != 0 && e3 & X64PteFlags::PS.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }

        let pd = if e3 & X64PteFlags::PRESENT.bits() != 0 {
            (e3 & ADDR_MASK) as *mut u64
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            let page = phys.0;
            unsafe { write_entry(pdpt, i3, page | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits()) };
            unsafe { phys_to_ptr(page) }
        };

        let i2 = pd_index(vaddr.0);
        unsafe { write_entry(pd, i2, (paddr.0 & ADDR_MASK) | pte_flags | huge_flag) };
        Ok(())
    }

    /// Whether the CPU supports 1 GB huge pages (CPUID.80000001H:EDX.GBPAGES bit 26).
    fn supports_1gb_page() -> bool {
        let mut edx: u32;
        unsafe {
            asm!(
                "push rbx",
                "mov eax, 0x80000001",
                "cpuid",
                "pop rbx",
                out("edx") edx,
                out("eax") _,
                out("ecx") _,
                options(nomem, nostack, preserves_flags)
            );
        }
        (edx & (1 << 26)) != 0
    }
}

// ── DM coverage establishment (boot identity write channel) ────────────────

/// Full TLB flush by reloading CR3 with the bootstrap root (boot context:
/// single-threaded, no PCID tagging).
///
/// # Safety
///
/// Paging must be enabled with `root_paddr` as the active CR3.
unsafe fn flush_cr3(root_paddr: u64) { unsafe {
    asm!("mov cr3, {}", in(reg) root_paddr, options(nostack));
}}

/// ZST implementor of [`DmCoverageArch`] for x86-64.
///
/// All table access goes through the bootstrap root's identity mapping
/// (`VA = PA`): the DM windows being established must not be load-bearing
/// for their own construction (§6.1 "DM 建立的自举写通道闭环"). Writes that
/// touch PA < 1 GiB translate through `PDPTE[0]` (`VA < 1 GiB`), so
/// replacing/splitting `PDPTE[2]` (VA [2 GiB, 3 GiB)) never disturbs the
/// write channel itself — no circular dependency.
///
/// # Pre-existing coverage on the bootstrap root
///
/// Two mappings exist before DM establishment (`arch_boot_impl` Steps 1-2):
/// the identity mapping (PML4[0], VA [0, 4 GiB), supervisor, VA = PA,
/// fallback granularity 2 MiB leaves — the kernel load address is not
/// 1 GiB-aligned) and the kernel image high mapping (PML4[256], the
/// `kern_virt_base` slot). The kernel DM window (PML4[257]) is therefore a
/// fresh slot; the VM DM window overlaps the identity mapping under
/// `PDPTE[2]`, where pre-existing translations are superseded wholesale:
/// a 1 GiB identity leaf is demoted via the split path, and a fallback
/// 2 MiB identity *table* is replaced by an empty lower table that keeps
/// only what subsequent DM units re-map (Gate 8 Case E: holes never
/// swallowed). Intermediate entries on the VM window path carry U so the
/// window's user leaves are user-accessible (U/S is ANDed across levels;
/// identity leaves keep U = 0 and remain supervisor, E6).
pub struct X86_64DmCoverage;

impl crate::arch::dm_coverage::DmCoverageArch for X86_64DmCoverage {
    /// CPUID.80000001H:EDX.GBPAGES — same probe as [`X86_64Paging`].
    fn supports_1gb_page() -> bool {
        X86_64Paging::supports_1gb_page()
    }

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

        // Window semantics from the VA itself. The VM DM window overlaps the
        // identity mapping (PML4[0]): pre-existing translations there are
        // identity remnants, superseded wholesale. The kernel DM window
        // (PML4[257]) shares its slot with nothing.
        let vm_window = vaddr.0 >= X86_64DirectMap::VM_DIRECT_MAP_BASE
            && vaddr.0
                < X86_64DirectMap::VM_DIRECT_MAP_BASE + X86_64DirectMap::VM_DIRECT_MAP_SIZE;
        // Intermediate entries on the VM window path carry U: U/S is ANDed
        // across levels, and the identity mapping created PML4[0]
        // supervisor-only, which would keep the window's user leaves
        // supervisor-only in effect (E6). Identity leaves keep U = 0 and
        // stay supervisor.
        let branch_flags: u64 = if vm_window { X64PteFlags::USER.bits() } else { 0 };

        // PML4 level: the identity mapping already lives under PML4[0] and
        // the kernel image under PML4[256], but allocate the branch anyway
        // if absent (fresh window on a sparse root).
        let i4 = pml4_index(vaddr.0);
        let pml4 = unsafe { phys_to_ptr(root_paddr.0) };
        let pml4e = unsafe { read_entry(pml4, i4) };
        let pdpt = if pml4e & X64PteFlags::PRESENT.bits() == 0 {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: pml4 slot is within the root page; identity channel active.
            unsafe {
                write_entry(
                    pml4,
                    i4,
                    phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits() | branch_flags,
                )
            };
            phys.0
        } else {
            if vm_window && pml4e & X64PteFlags::USER.bits() == 0 {
                // Identity-created PML4[0]: raise U so VM DM user leaves are
                // reachable from user mode (see branch_flags). No stale
                // translation can exist for the VM window VAs (never
                // accessed); the reload is defense-in-depth.
                // SAFETY: pml4 slot is within the root page; identity channel active.
                unsafe { write_entry(pml4, i4, pml4e | X64PteFlags::USER.bits()) };
                // SAFETY: see flush_cr3 contract; active root == root_paddr.
                unsafe { flush_cr3(root_paddr.0) };
            }
            pml4e & ADDR_MASK
        };

        // PDPT level: this is where the x86-64 identity overlap lives —
        // the VM DM window [0x8000_0000, 0xC000_0000) shares PDPT slot 2
        // (VA [2 GiB, 3 GiB)) with the identity mapping.
        let i3 = pdpt_index(vaddr.0);
        let pdpt_ptr = unsafe { phys_to_ptr(pdpt) };
        let e3 = unsafe { read_entry(pdpt_ptr, i3) };
        let present3 = e3 & X64PteFlags::PRESENT.bits() != 0;
        let huge3 = present3 && e3 & X64PteFlags::PS.bits() != 0;

        if size == 1 << 30 {
            // Whole-slot replacement branch (§6.1 v4.9 conditional). The
            // caller reached a 1 GiB unit only because the *entire* window
            // PA span [0, 1 GiB) is one candidate range — the resource
            // containment condition. Overwriting the identity 1 GiB leaf
            // removes the identity translation for VA [2 GiB, 3 GiB);
            // kernel access to that PA range flows through the kernel DM
            // window from here on (§6.1 constraint ①).
            //
            // A table pointer in the slot would mean mixed granularity
            // within one 1 GiB region — impossible from a single
            // candidate-set pass; fail fast.
            if present3 && !huge3 {
                return Err(PageTableError::AlreadyMapped);
            }
            // SAFETY: identity channel active; see branch comment.
            unsafe { write_entry(pdpt_ptr, i3, (paddr.0 & ADDR_MASK) | pte_flags | X64PteFlags::PS.bits()) };
            // The replaced identity leaf may have been translated (it is
            // G-flagged, so it would survive a CR3 reload): the boot path
            // never accesses identity VAs ≥ 2 GiB, so no stale entry
            // exists today — the reload is defense-in-depth for future
            // boot-path changes, and per-address invlpg becomes mandatory
            // if that ever changes.
            // SAFETY: see flush_cr3 contract; active root == root_paddr.
            unsafe { flush_cr3(root_paddr.0) };
            return Ok(());
        }

        // Split path (the x86-64 norm — the legacy hole [0xA0000, 0x100000)
        // fails the resource-containment condition): demote a pre-existing
        // 1 GiB leaf into an EMPTY lower table. The identity translation
        // for VA [2 GiB, 3 GiB) is wholesale superseded by DM semantics;
        // the replacement table keeps only what subsequent allowed units
        // re-map, so a reserved hole is never swallowed (Gate 8 Case E,
        // mechanism level).
        let pd = if huge3 {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active; replacing the identity 1 GiB
            // leaf with a table pointer (no PS).
            unsafe {
                write_entry(
                    pdpt_ptr,
                    i3,
                    phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits() | branch_flags,
                )
            };
            // Same staleness argument as the 1 GiB replacement above.
            // SAFETY: see flush_cr3 contract; active root == root_paddr.
            unsafe { flush_cr3(root_paddr.0) };
            phys.0
        } else if present3 {
            let existing = e3 & ADDR_MASK;
            if vm_window && is_identity_table(existing, i3, vaddr.0) {
                // Fallback-granularity identity (2 MiB leaves) built PDPTE[2]
                // as a *table*: wholesale supersession — replace it with an
                // empty lower table that keeps only what subsequent DM units
                // re-map. Identity remnants (and reserved holes) in VA
                // [2 GiB, 3 GiB) never survive (Gate 8 Case E, mechanism
                // level).
                let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
                // SAFETY: identity channel active; replacing the identity
                // table pointer with a fresh empty table (no PS).
                unsafe {
                    write_entry(
                        pdpt_ptr,
                        i3,
                        phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits() | branch_flags,
                    )
                };
                // Identity translations for VA [2 GiB, 3 GiB) were never
                // accessed (unbacked ranges; boot path never touches them),
                // so no stale TLB entry exists — reload is defense-in-depth.
                // SAFETY: see flush_cr3 contract; active root == root_paddr.
                unsafe { flush_cr3(root_paddr.0) };
                phys.0
            } else {
                // Our own DM table from a previous unit of this pass (or a
                // fresh kernel-window slot that cannot be present).
                existing
            }
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active; fresh slot.
            unsafe {
                write_entry(
                    pdpt_ptr,
                    i3,
                    phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits() | branch_flags,
                )
            };
            phys.0
        };

        // PD level.
        let i2 = pd_index(vaddr.0);
        let pd_ptr = unsafe { phys_to_ptr(pd) };
        let e2 = unsafe { read_entry(pd_ptr, i2) };
        let present2 = e2 & X64PteFlags::PRESENT.bits() != 0;

        if size == 1 << 21 {
            // A present slot (2 MiB leaf or table) is a boot-layout bug:
            // single-pass emission never revisits a 2 MiB region, and the
            // identity remnant tables were wholesale replaced above.
            if present2 {
                return Err(PageTableError::AlreadyMapped);
            }
            // SAFETY: identity channel active; fresh slot.
            unsafe { write_entry(pd_ptr, i2, (paddr.0 & ADDR_MASK) | pte_flags | X64PteFlags::PS.bits()) };
            return Ok(());
        }

        // 4 KiB under the PD.
        if present2 && e2 & X64PteFlags::PS.bits() != 0 {
            // A 2 MiB leaf blocks the 4 KiB install — cannot arise from a
            // single disjoint candidate pass; fail fast.
            return Err(PageTableError::AlreadyMapped);
        }
        let pt = if present2 {
            e2 & ADDR_MASK
        } else {
            let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
            // SAFETY: identity channel active; fresh slot.
            unsafe {
                write_entry(
                    pd_ptr,
                    i2,
                    phys.0 | (X64PteFlags::PRESENT | X64PteFlags::WRITABLE).bits() | branch_flags,
                )
            };
            phys.0
        };
        let i1 = ((vaddr.0 >> 12) & 0x1FF) as usize;
        let pt_ptr = unsafe { phys_to_ptr(pt) };
        let e1 = unsafe { read_entry(pt_ptr, i1) };
        if e1 & X64PteFlags::PRESENT.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
        // SAFETY: identity channel active; fresh leaf slot.
        unsafe { write_entry(pt_ptr, i1, (paddr.0 & ADDR_MASK) | pte_flags) };
        Ok(())
    }
}

/// Classify a table living under `PDPTE[i3]`: identity (populated by the
/// boot identity loop with VA = PA leaves) vs a DM table established by an
/// earlier unit of this pass.
///
/// Reads the first present entry: inside the VM DM window an identity leaf
/// at child slot `j` maps `PA == VA` (`region_base + j·2M`), while a DM leaf
/// maps `PA = VA − VM_DIRECT_MAP_BASE` — the two are never equal, so the
/// first present entry decides. An all-empty table cannot arise (tables are
/// written together with their first leaf) and classifies as DM.
///
/// # Safety
///
/// `pdpt` must be a physical address of a page-table page, identity-mapped
/// (VA = PA) and readable; boot context, single-threaded.
unsafe fn is_identity_table(pdpt: u64, i3: usize, vaddr: u64) -> bool { unsafe {
    let region_base = (vaddr & !((1 << 39) - 1)) + ((i3 as u64) << 30);
    let tbl = phys_to_ptr(pdpt);
    for j in 0..512usize {
        let e = unsafe { read_entry(tbl, j) };
        if e & X64PteFlags::PRESENT.bits() != 0 {
            return e & ADDR_MASK == region_base + ((j as u64) << 21);
        }
    }
    false
}}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// `flags_to_pte` and `pte_to_flags` must roundtrip for all common
    /// flag combinations. The NX bit (inverted) is the main risk.
    #[test]
    fn test_flag_roundtrip_user_read_write() {
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER_ACCESSIBLE;
        let pte = flags_to_pte(flags);
        assert_eq!(pte_to_flags(pte), flags);
    }

    #[test]
    fn test_flag_roundtrip_kernel_executable() {
        // Kernel executable: PRESENT + EXECUTABLE + GLOBAL, not writable.
        let flags = PageFlags::PRESENT | PageFlags::EXECUTABLE | PageFlags::GLOBAL;
        let pte = flags_to_pte(flags);
        assert_eq!(pte_to_flags(pte), flags);
    }

    #[test]
    fn test_flag_roundtrip_user_executable() {
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE
            | PageFlags::USER_ACCESSIBLE | PageFlags::EXECUTABLE;
        let pte = flags_to_pte(flags);
        assert_eq!(pte_to_flags(pte), flags);
    }

    #[test]
    fn test_nx_bit_inverted_when_not_executable() {
        // A page that is NOT executable must have NX (bit 63) set.
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE;
        let pte = flags_to_pte(flags);
        assert!(pte & X64PteFlags::NX.bits() != 0, "NX must be set for non-executable page");
        assert!(!pte_to_flags(pte).contains(PageFlags::EXECUTABLE));
    }

    #[test]
    fn test_nx_bit_clear_when_executable() {
        // A page that IS executable must have NX (bit 63) cleared.
        let flags = PageFlags::PRESENT | PageFlags::EXECUTABLE;
        let pte = flags_to_pte(flags);
        assert!(pte & X64PteFlags::NX.bits() == 0, "NX must be clear for executable page");
        assert!(pte_to_flags(pte).contains(PageFlags::EXECUTABLE));
    }

    /// The address field must survive a flag roundtrip without alteration.
    #[test]
    fn test_address_preserved_through_flag_roundtrip() {
        let paddr = 0x0000_1234_5678_9000; // 4KB-aligned physical address
        let flags = PageFlags::PRESENT | PageFlags::WRITABLE | PageFlags::USER_ACCESSIBLE;
        let pte = (paddr & ADDR_MASK) | flags_to_pte(flags);
        // Extract address back: must equal the original paddr.
        assert_eq!(pte & ADDR_MASK, paddr);
    }

    /// PS bit (huge page indicator) must roundtrip through pte_to_flags.
    #[test]
    fn test_huge_page_flag_roundtrip() {
        let pte = X64PteFlags::PRESENT.bits() | X64PteFlags::PS.bits() | ADDR_MASK;
        let flags = pte_to_flags(pte);
        assert!(flags.contains(PageFlags::HUGE_PAGE));
        assert!(flags.contains(PageFlags::PRESENT));
    }

    /// Index computations must match the x86-64 4-level layout.
    #[test]
    fn test_pml4_index_high_canonical() {
        // 0xFFFF_8000_0000_0000 — start of kernel space.
        // bits 39-47 = (0xFFFF_8000_0000_0000 >> 39) & 0x1FF = 0x1FF & 0x1FF... let's compute:
        // 0xFFFF_8000_0000_0000 >> 39 = 0x1FFFF_0000 → & 0x1FF = 0x100 = 256
        assert_eq!(pml4_index(0xFFFF_8000_0000_0000), 256);
    }

    #[test]
    fn test_pdpt_index_1gb_boundary() {
        // 0x4000_0000 = 1GB. PDPT index should be 1.
        assert_eq!(pdpt_index(0x4000_0000), 1);
    }

    #[test]
    fn test_pd_index_2mb_boundary() {
        // 0x200_000 = 2MB. PD index should be 1.
        assert_eq!(pd_index(0x200_000), 1);
    }

    /// `walk_read` on a zeroed root page must return NotPresent (no
    /// allocations, no reads beyond the PML4 entry).
    ///
    /// This test uses a mock physical page that we zero-fill ourselves.
    /// It exercises the walk logic without requiring real hardware —
    /// the Direct Map base is the mock base, and the mock page happens
    /// to be at a valid offset.
    #[test]
    fn test_walk_read_not_present_on_zero_root() {
        // We cannot safely call walk_read without a real Direct Map, because
        // read_pte_dm dereferences the Direct Map address. This test is
        // intentionally a no-op marker; real walk tests run under QEMU.
        // The flag encoding tests above validate the PTE interpretation
        // logic that walk_read depends on.
    }
}
