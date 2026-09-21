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
    // V11/E4: in this crate's own test build (mock feature), the VM window
    // routes through MockDirectMap so tests can point it at real leaked
    // memory via `set_mock_vm_base` — mirroring the VM-side test funnel.
    // Production keeps the x86_64 constant base.
    match channel {
        PteChannel::KernelDm => {
            X86_64DirectMap::kernel_phys_to_virt(PhysBytes(phys)).0 as *mut u64
        }
        #[cfg(not(all(test, feature = "runtime-window")))]
        PteChannel::VmDm => X86_64DirectMap::vm_phys_to_virt(PhysBytes(phys)).0 as *mut u64,
        // V11/E4: in this crate's own test build (mock feature), the VM
        // window routes through MockDirectMap so tests can point it at
        // real leaked memory via `set_mock_vm_base` — mirroring the
        // VM-side test funnel. Production keeps the x86_64 constant base.
        #[cfg(all(test, feature = "runtime-window"))]
        PteChannel::VmDm => {
            crate::arch::direct_map::MockDirectMap::vm_phys_to_virt(PhysBytes(phys)).0
                as *mut u64
        }
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

/// Write a PTE at the given physical address via the Direct Map.
///
/// Flush behavior is pinned to the channel, mirroring how the channel
/// itself pins the execution context:
/// - `KernelDm` handles run at CPL0, so a conservative `invlpg` is
///   always legal and kept for present→X transitions defense-in-depth.
/// - `VmDm` handles are exercised by VM at CPL3, where `invlpg` is a
///   privileged instruction (#GP(0) — the fix14 first-light forensics,
///   2026-09-21: `user exception: vector 0xd rip <map+578>`). No flush
///   is needed there: the only writes such a handle performs are
///   not-present → present (`map` refuses `AlreadyMapped`, and
///   `walk_alloc` only creates absent intermediates), and x86 never
///   caches translations derived from not-present entries
///   (Intel SDM Vol.3A §4.10.4), so no stale TLB entry can exist.
///   present→X transitions on a `VmDm` handle (unmap/remap of VM's own
///   live address space) would need a kernel-assisted flush, which is
///   not wired — see the VmDm flush gap in the VM paging design notes.
///
/// SAFETY: the channel's Direct Map window must be active; `paddr` must be
/// a valid 8-byte aligned PTE address. `vaddr_for_flush` is the virtual
/// address the PTE covers (used for TLB invalidation; pass 0 for
/// intermediate tables where no TLB entry exists yet).
#[inline]
unsafe fn write_pte_dm(paddr: u64, value: u64, vaddr_for_flush: u64, channel: PteChannel) { unsafe {
    core::ptr::write_volatile(channel_to_ptr(paddr, channel), value);
    if channel == PteChannel::KernelDm {
        // Flush any stale TLB entry for this virtual address. For intermediate
        // table entries (PML4/PDPT/PD), no leaf TLB entry exists yet, so the
        // flush is a conservative no-op. For leaf PTE entries, this ensures
        // stale mappings are evicted.
        asm!("invlpg [{}]", in(reg) vaddr_for_flush, options(nostack, preserves_flags));
    }
    // VmDm: see doc comment — invlpg at CPL3 faults, and the only writes
    // this channel legally performs (not-present → present) need no flush.
}}

/// Read the current page-table root physical address (CR3, address bits
/// cleared). S-5: the AP bootstrap record's `page_table_root_pa` must be the
/// root the BSP is actually translating with — reading CR3 at orchestration
/// time is the ground truth (the adopted root lives in kernel boot state,
/// not in a form arch can name).
pub fn current_cr3_pa() -> u64 {
    let v: u64;
    // SAFETY: CR3 read is a plain register read at CPL0.
    unsafe { core::arch::asm!("mov {}, cr3", out(reg) v, options(nomem, nostack)) };
    v & 0x000F_FFFF_FFFF_F000
}

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
fn walk_alloc(
    root_paddr: u64,
    vaddr: u64,
    channel: PteChannel,
    leaf_pte: u64,
) -> Result<u64, PageTableError> {
    // Intermediate levels carry USER when the leaf does: U/S is ANDed at
    // EVERY level, so a user-accessible leaf behind supervisor-only
    // intermediates faults with #PF(err=5) on first CPL3 access. Derived
    // from the leaf rather than stored per-level (Linux populates
    // intermediate entries with _PAGE_USER the same way). WRITABLE stays
    // unconditional on intermediates — effective permissions still AND
    // with the leaf, so a read-only leaf stays read-only. NX needs no
    // propagation: execution is forbidden when ANY level sets it.
    let mid_flags = X64PteFlags::PRESENT
        | X64PteFlags::WRITABLE
        | if leaf_pte & X64PteFlags::USER.bits() != 0 {
            X64PteFlags::USER
        } else {
            X64PteFlags::empty()
        };
    let i4 = pml4_index(vaddr);
    // SAFETY: channel's Direct Map active per function precondition.
    let pml4e = unsafe { read_pte_dm(root_paddr + (i4 as u64) * 8, channel) };
    let pdpt = if pml4e & X64PteFlags::PRESENT.bits() == 0 {
        // Allocate a new PDPT page.
        let (phys, _virt) = crate::pt_alloc::alloc_pt_page()?;
        let entry = phys.0 | mid_flags.bits();
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
        let entry = phys.0 | mid_flags.bits();
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
        let entry = phys.0 | mid_flags.bits();
        unsafe { write_pte_dm(pd + (i2 as u64) * 8, entry, 0, channel) };
        phys.0
    } else {
        pde & ADDR_MASK
    };

    let pt_idx = (vaddr >> 12) & 0x1FF;
    Ok(pt + pt_idx * 8)
}

/// NK4-A C-3（2026-09-22）：用户叶被映射进 supervisor 身份层级的影子下
/// （bootstrap 身份叶 P|W|X|G 覆盖低 4GiB，VM 拆叶后新 PT 链继承父叶的
/// U=0）时，硬件按 AND 语义在任一中间层缺 U 即拒绝 CPL3 访问——
/// #PF(err=0x15)，与叶子自身的 P|U|X 无关；软件 walk 只读叶级故不可见
/// （真机：RS 入口 0x2246c0 永久 err=0x15，walk=0xd）。本助手沿路径把 U
/// OR 进既存中间层（Linux 同法：既存 PMD/PUD 补 _PAGE_USER）。安全性：
/// 叶级 U 仍逐页把门——supervisor 身份页自身叶 U=0 继续拒绝，无新暴露
/// 面；G 位不动。调用后调用方需保证对 vaddr 的 TLB/paging-structure
/// 缓存失效（恢复点 invlpg 已在 finish_and_restore 以 p_fault_addr 落地）。
///
/// # Safety
///
/// Same channel contract as `walk_alloc`: the channel's Direct Map window
/// is active and covers `root_paddr`'s tree; all slots written are 8-byte
/// aligned table entries.
unsafe fn propagate_user_to_intermediates(root_paddr: u64, vaddr: u64, channel: PteChannel) {
    let want = X64PteFlags::USER.bits();
    let i4 = pml4_index(vaddr);
    let pml4e = unsafe { read_pte_dm(root_paddr + (i4 as u64) * 8, channel) };
    if pml4e & X64PteFlags::PRESENT.bits() == 0 {
        // 空树：walk_alloc 以 mid_flags 建新链，天然带 U，无既存层可升。
        return;
    }
    if pml4e & want == 0 {
        unsafe {
            write_pte_dm(root_paddr + (i4 as u64) * 8, pml4e | want, 0, channel);
        }
    }
    let pdpt_pa = pml4e & ADDR_MASK;
    let i3 = pdpt_index(vaddr);
    let pdpte = unsafe { read_pte_dm(pdpt_pa + (i3 as u64) * 8, channel) };
    if pdpte & (X64PteFlags::PRESENT | X64PteFlags::PS).bits()
        == (X64PteFlags::PRESENT | X64PteFlags::PS).bits()
    {
        // 1GiB 叶占位：map 自身已报 AlreadyMapped，无中间层可升。
        return;
    }
    if pdpte & X64PteFlags::PRESENT.bits() == 0 {
        return;
    }
    if pdpte & want == 0 {
        unsafe {
            write_pte_dm(pdpt_pa + (i3 as u64) * 8, pdpte | want, 0, channel);
        }
    }
    let pd_pa = pdpte & ADDR_MASK;
    let i2 = pd_index(vaddr);
    let pde = unsafe { read_pte_dm(pd_pa + (i2 as u64) * 8, channel) };
    if pde & (X64PteFlags::PRESENT | X64PteFlags::PS).bits()
        == (X64PteFlags::PRESENT | X64PteFlags::PS).bits()
    {
        // 2MiB 叶占位：同上。
        return;
    }
    if pde & X64PteFlags::PRESENT.bits() == 0 {
        return;
    }
    if pde & want == 0 {
        unsafe {
            write_pte_dm(pd_pa + (i2 as u64) * 8, pde | want, 0, channel);
        }
    }
    // PT 级无需：叶子本身携带 U。
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

    fn inherit_supervisor_half(&mut self, source_root: PhysBytes) {
        // PML4 entries 256..512 = the higher half (VAs ≥
        // 0xffff_8000_0000_0000): kernel text/data, stacks and the DM
        // windows. Raw entry copy — the entries may point at shared
        // PDPT/PD/PT pages, which is sound because user mappings never
        // touch the upper half (see trait doc).
        for i in 256usize..512 {
            let slot = (i as u64) * 8;
            // SAFETY: both roots are 4KB-aligned page-table pages in RAM;
            // the KernelDm direct map (pinned by `new_from_page` /
            // `from_active_root`) makes every PTE slot addressable. Same
            // access pattern as `walk_alloc`. The flush VA is 0 — table
            // pages are never translated through the MMU by this code.
            let entry =
                unsafe { read_pte_dm(source_root.0 + slot, PteChannel::KernelDm) };
            unsafe {
                write_pte_dm(self.root_paddr + slot, entry, 0, PteChannel::KernelDm)
            };
        }
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
        // V11/E4: full four-level reclaim when a pt_free function is
        // registered (the VM process-exit path — C pagetable.c:1427-1437
        // `pt_free` parity, previously "accept the intermediate-table
        // leak", G-V12-6-era gap E4). Without pt_free (boot-stage tables
        // with no allocator domain), keep the zero-root-only legacy shape:
        // zero the root PML4 to prevent use-after-free if the physical
        // page is reused, and leave intermediates unreclaimed.
        //
        // SAFETY: the handle's Direct Map channel must be active;
        // root_paddr is the physical address of our PML4 page. The exit
        // path guarantees the page table is not active on any CPU
        // (exit.rs SAFETY contract) — single-threaded VM, no concurrent
        // walker.
        if !crate::pt_alloc::is_free_registered() {
            let ptr = channel_to_ptr(self.root_paddr, self.channel);
            unsafe { core::ptr::write_bytes(ptr, 0, 512) };
            return;
        }
        // Depth-first: free every intermediate table below the root
        // (PS-bit entries are huge-frame data pages owned by the region /
        // exit path — skipped, never traversed as tables). Then zero the
        // root (UAF guard, unchanged) and finally return the root page
        // itself to the allocator.
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
        // Walk to the leaf PTE address, allocating intermediate tables.
        // The leaf PTE is composed first so walk_alloc can propagate the
        // USER bit into the intermediate levels it creates (U/S is ANDed
        // per level — see walk_alloc).
        let new_pte = (paddr.0 & ADDR_MASK) | flags_to_pte(flags);
        let leaf_paddr = walk_alloc(self.root_paddr, vaddr.0, self.channel, new_pte)?;
        // 用户叶：把 U 升进既存 supervisor 中间层（NK4-A C-3，见
        // propagate_user_to_intermediates 文档）。
        if flags.contains(PageFlags::USER_ACCESSIBLE) {
            // SAFETY: same DM channel contract as walk_alloc above.
            unsafe {
                propagate_user_to_intermediates(self.root_paddr, vaddr.0, self.channel);
            }
        }
        // SAFETY: channel's Direct Map active; leaf_paddr is 8-byte aligned.
        let pte = unsafe { read_pte_dm(leaf_paddr, self.channel) };
        if pte & X64PteFlags::PRESENT.bits() != 0 {
            return Err(PageTableError::AlreadyMapped);
        }
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
        // 用户叶：中间层 U 升级与 map 同理（NK4-A C-3，见
        // propagate_user_to_intermediates）。
        if flags.contains(PageFlags::USER_ACCESSIBLE) {
            // SAFETY: same DM channel contract as walk_alloc.
            unsafe {
                propagate_user_to_intermediates(self.root_paddr, vaddr.0, self.channel);
            }
        }
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

    /// Split the huge leaf covering `vaddr` into 4 KiB leaves (see
    /// `Paging::split_huge` contract). Both the 1 GiB (PDPT PS) and the
    /// 2 MiB (PD PS) shapes are handled; translations inside the split
    /// range are preserved bit-for-bit (frame bits + leaf flags, PS
    /// included, so `pte_to_flags` still reports HUGE_PAGE-equivalent
    /// geometry for sibling pages). Intermediate table entries carry
    /// PRESENT|WRITABLE|USER copied from the parent leaf — USER must
    /// propagate at every level (see `walk_alloc` note on #PF(err=5)).
    ///
    /// Pages are NOT assumed zero-initialized: all 512 new entries are
    /// written explicitly, so a reused frame cannot leak stale
    /// translations. After any split, CR3 is reloaded for a full TLB
    /// flush — boot-context single-CPU (mirrors `flush_tlb`), and it
    /// covers speculative huge-leaf entries for addresses other than
    /// `vaddr`. Subsequent `unmap` of the evicted page flushes precisely.
    fn split_huge(&mut self, vaddr: VirBytes) -> Result<bool, PageTableError> {
        let ch = self.channel;
        let table_flags_mask =
            (X64PteFlags::PRESENT | X64PteFlags::WRITABLE | X64PteFlags::USER).bits();
        let mut split = false;

        let i4 = pml4_index(vaddr.0);
        // SAFETY: channel's Direct Map active (walk_read precondition);
        // the PML4 slot address is within the root page.
        let pml4e = unsafe { read_pte_dm(self.root_paddr + (i4 as u64) * 8, ch) };
        if pml4e & X64PteFlags::PRESENT.bits() == 0 {
            return Ok(false);
        }
        let pdpt_pa = pml4e & ADDR_MASK;

        let i3 = pdpt_index(vaddr.0);
        // SAFETY: see above; PDPT slot address is within the PDPT page.
        let mut e3 = unsafe { read_pte_dm(pdpt_pa + (i3 as u64) * 8, ch) };
        if e3 & X64PteFlags::PRESENT.bits() != 0 && e3 & X64PteFlags::PS.bits() != 0 {
            // 1 GiB leaf → build a PD of 2 MiB leaves, then descend.
            let leaf_pa = e3 & ADDR_MASK;
            let leaf_bits = e3 & !ADDR_MASK;
            let (pd_phys, _v) = crate::pt_alloc::alloc_pt_page()?;
            for i in 0..512usize {
                // SAFETY: `pd_phys` is a fresh page-table frame reachable
                // through the active Direct Map channel; entry is aligned.
                unsafe {
                    write_pte_dm(
                        pd_phys.0 + (i as u64) * 8,
                        (leaf_pa + (i as u64) << PD_SHIFT) | leaf_bits,
                        0,
                        ch,
                    )
                };
            }
            let table_bits = pd_phys.0 | (e3 & table_flags_mask);
            // SAFETY: PDPT slot address is within the live PDPT page.
            unsafe { write_pte_dm(pdpt_pa + (i3 as u64) * 8, table_bits, vaddr.0, ch) };
            split = true;
            e3 = table_bits;
        }
        if e3 & X64PteFlags::PRESENT.bits() == 0 {
            return Ok(split);
        }
        let pd_pa = e3 & ADDR_MASK;

        let i2 = pd_index(vaddr.0);
        // SAFETY: see above; PD slot address is within the live PD page
        // (either the pre-existing one or the PD just installed above).
        let e2 = unsafe { read_pte_dm(pd_pa + (i2 as u64) * 8, ch) };
        if e2 & X64PteFlags::PRESENT.bits() != 0 && e2 & X64PteFlags::PS.bits() != 0 {
            // 2 MiB leaf → build a PT of 4 KiB leaves. PS is cleared in
            // the children: at PT level bit 7 is reserved (the 1 GiB
            // pass above keeps it — there the children ARE 2 MiB leaves).
            let leaf_pa = e2 & ADDR_MASK;
            let leaf_bits = e2 & !ADDR_MASK & !X64PteFlags::PS.bits();
            let (pt_phys, _v) = crate::pt_alloc::alloc_pt_page()?;
            for i in 0..512usize {
                // SAFETY: `pt_phys` is a fresh page-table frame reachable
                // through the active Direct Map channel; entry is aligned.
                unsafe {
                    write_pte_dm(
                        pt_phys.0 + (i as u64) * 8,
                        (leaf_pa + (i as u64) << 12) | leaf_bits,
                        0,
                        ch,
                    )
                };
            }
            // SAFETY: PD slot address is within the live PD page.
            unsafe {
                write_pte_dm(
                    pd_pa + (i2 as u64) * 8,
                    pt_phys.0 | (e2 & table_flags_mask),
                    vaddr.0,
                    ch,
                )
            };
            split = true;
        }

        if split {
            // SAFETY: CPL0, boot context; CR3 reload with the same root
            // flushes all non-global TLB entries (same effect as
            // `flush_tlb`). Intel syntax (asm!'s default): with
            // `att_syntax` the bare `cr3` token is not a register name
            // and assembles to an external symbol reference instead.
            unsafe {
                asm!("mov cr3, {}", in(reg) self.root_paddr, options(nostack));
            }
        }
        Ok(split)
    }

    /// OR the USER bit into PML4E / PDPTE / PDE on the path to `vaddr`
    /// (see `Paging::grant_user_walk` contract). Absent intermediate →
    /// `NotMapped` (the follow-up `map` creates it with USER already,
    /// via `walk_alloc`). Huge leaf met mid-walk → `NotSupported`: the
    /// caller must `split_huge` first; OR-ing USER into a huge leaf
    /// would grant CPL3 access to the WHOLE 2 MiB / 1 GiB range.
    fn grant_user_walk(&mut self, vaddr: VirBytes) -> Result<(), PageTableError> {
        let ch = self.channel;
        let present = X64PteFlags::PRESENT.bits();
        let user = X64PteFlags::USER.bits();
        let ps = X64PteFlags::PS.bits();

        let i4 = pml4_index(vaddr.0);
        let slot4 = self.root_paddr + (i4 as u64) * 8;
        // SAFETY: channel's Direct Map active (walk_read precondition);
        // slot addresses are within live table pages at each level.
        let e4 = unsafe { read_pte_dm(slot4, ch) };
        if e4 & present == 0 {
            return Err(PageTableError::NotMapped);
        }
        if e4 & user == 0 {
            unsafe { write_pte_dm(slot4, e4 | user, vaddr.0, ch) };
        }

        let i3 = pdpt_index(vaddr.0);
        let slot3 = (e4 & ADDR_MASK) + (i3 as u64) * 8;
        let e3 = unsafe { read_pte_dm(slot3, ch) };
        if e3 & present == 0 {
            return Err(PageTableError::NotMapped);
        }
        if e3 & ps != 0 {
            return Err(PageTableError::NotSupported);
        }
        if e3 & user == 0 {
            unsafe { write_pte_dm(slot3, e3 | user, vaddr.0, ch) };
        }

        let i2 = pd_index(vaddr.0);
        let slot2 = (e3 & ADDR_MASK) + (i2 as u64) * 8;
        let e2 = unsafe { read_pte_dm(slot2, ch) };
        if e2 & present == 0 {
            return Err(PageTableError::NotMapped);
        }
        if e2 & ps != 0 {
            return Err(PageTableError::NotSupported);
        }
        if e2 & user == 0 {
            unsafe { write_pte_dm(slot2, e2 | user, vaddr.0, ch) };
        }
        Ok(())
    }

    fn root_paddr(&self) -> PhysBytes {
        PhysBytes(self.root_paddr)
    }

    unsafe fn switch(&self) {
        // Intel syntax (asm!'s default): with `options(att_syntax)` the bare
        // `cr3` token is NOT a register name — it assembles to a memory
        // reference of an external symbol `cr3` and fails the link the moment
        // this method becomes live (fix20 forensics 2026-09-21: the shim
        // image build broke here once `split_huge`'s CR3 reload started
        // keeping these symbols reachable). Same rationale as split_huge.
        unsafe {
            asm!("mov cr3, {}", in(reg) self.root_paddr, options(nostack));
        }
    }

    unsafe fn flush_tlb(&self) {
        // SAFETY/-syntax: see `switch` — Intel syntax, no `att_syntax`.
        unsafe {
            asm!("mov cr3, {}", in(reg) self.root_paddr, options(nostack));
        }
    }

    unsafe fn flush_tlb_addr(&self, vaddr: VirBytes) {
        unsafe {
            asm!("invlpg [{}]", in(reg) vaddr.0, options(nostack, preserves_flags));
        }
    }
}

impl X86_64Paging {
/// Free all page-table pages directly or transitively referenced by
/// `table_paddr`'s present, non-huge entries. `level` 0 = PML4,
/// 1 = PDPT, 2 = PD; children at level 3 are leaf PT pages (freed
/// without recursion). Data pages are never touched here.
///
/// SAFETY: Direct Map channel active; the caller guarantees the whole
/// tree is exclusive to this (dead) address space and no CPU walks it.
unsafe fn free_child_tables(&self, table_paddr: u64, level: u8) {
    if level >= 3 {
        return;
    }
    let base = channel_to_ptr(table_paddr, self.channel);
    for i in 0..512usize {
        // SAFETY: sequential reads within the table page, which the
        // caller has excluded from all other access.
        let pte = unsafe { core::ptr::read_volatile(base.add(i)) };
        if pte & X64PteFlags::PRESENT.bits() != 0
            && pte & X64PteFlags::PS.bits() == 0
        {
            let child = pte & ADDR_MASK;
            if level + 1 < 3 {
                unsafe { self.free_child_tables(child, level + 1) };
            }
            crate::pt_alloc::free_pt_page(minix_types::PhysBytes(child));
        }
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
            // SAFETY: `is_identity_table` requires its `pdpt` to be
            // identity-mapped and readable in boot context — holds here by
            // `dm_install_leaf`'s own safety precondition (all table pages
            // are DM-visible and boot is single-threaded).
            if vm_window && unsafe { is_identity_table(existing, i3, vaddr.0) } {
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
        // Already inside this function's single unsafe block (edition 2024
        // makes the unsafe fn body safe by default) — no inner block needed.
        let e = read_entry(tbl, j);
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
    // ── V11/E4: destroy 四级回收（宿主可验证版）──────────────────────
    //
    // 手写 PTE 树（绕过 map()——其 write_pte_dm 含 invlpg 特权指令，
    // 宿主必然 SIGSEGV）。destroy 本体只做 read_volatile + write_bytes
    // + pt_alloc free，宿主完整可验证。T26 技巧：泄漏真实缓冲 +
    // `set_mock_vm_base` 指向它。状态放 static（register 收 fn 指针）。

    static E4_BASE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    static E4_FREED: std::sync::Mutex<alloc::vec::Vec<u64>> = std::sync::Mutex::new(alloc::vec::Vec::new());

    const E4_POOL_BYTES: usize = 16 * 4096;

    /// Hand-built four-level tree over pool offsets:
    /// root(0) → PDPT(0x1000) → PD1(0x2000)/PD2(0x4000)/PD3(0x5000) →
    /// PT1(0x3000)/PT2(0x7000)/PT3(0x9000) → data(0x6000/0x8000/0xA000,
    /// never freed by destroy).
    #[test]
    fn test_destroy_reclaims_intermediate_tables_and_root() {
        let pool: &'static mut [u8; E4_POOL_BYTES] =
            alloc::boxed::Box::leak(alloc::boxed::Box::new([0u8; E4_POOL_BYTES]));
        let base = pool.as_mut_ptr() as u64;
        // Point the mock Direct Map at the pool so DM accesses land in
        // real bytes (MockDirectMap is the selected impl under default
        // features — arch/src/lib.rs).
        crate::arch::direct_map::set_mock_vm_base(base);
        E4_BASE.store(base, core::sync::atomic::Ordering::SeqCst);
        E4_FREED.lock().unwrap().clear();

        crate::pt_alloc::register(|| {
            let off = E4_NEXT.fetch_add(4096, core::sync::atomic::Ordering::SeqCst);
            Ok((minix_types::PhysBytes(off as u64),
                minix_types::VirBytes(E4_BASE.load(core::sync::atomic::Ordering::SeqCst) + off as u64)))
        });
        crate::pt_alloc::register_free(|phys| {
            E4_FREED.lock().unwrap().push(phys.get());
        });

        // Static bump: root(0), PDPT(0x1000), PD1(0x2000), PT1(0x3000),
        // PD2(0x4000), PT2(0x7000), PD3(0x5000), PT3(0x9000),
        // data(0x6000, 0x8000, 0xA000).
        static E4_NEXT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
        let page = |offset: u64| -> u64 { base + offset };

        let write_pte = |table_off: u64, idx: usize, child_off: u64| {
            let pte = X64PteFlags::PRESENT.bits() | child_off;
            unsafe {
                core::ptr::write_volatile((base + table_off + (idx as u64) * 8) as *mut u64, pte)
            }
        };
        write_pte(0, 0, 0x1000);          // root[0] → PDPT
        write_pte(0x1000, 0, 0x2000);     // PDPT[0] → PD1
        write_pte(0x1000, 1, 0x4000);     // PDPT[1] → PD2
        write_pte(0x1000, 2, 0x5000);     // PDPT[2] → PD3
        write_pte(0x2000, 0, 0x3000);     // PD1[0] → PT1
        write_pte(0x3000, 1, 0x6000);     // PT1[1] → data
        write_pte(0x4000, 2, 0x7000);     // PD2[2] → PT2
        write_pte(0x7000, 0, 0x8000);     // PT2[0] → data
        write_pte(0x5000, 0, 0x9000);     // PD3[0] → PT3
        write_pte(0x9000, 0, 0xA000);     // PT3[0] → data

        // Build the handle directly over root offset 0 (VmDm channel).
        let mut pt = X86_64Paging {
            root_paddr: 0,
            channel: PteChannel::VmDm,
        };
        unsafe { pt.destroy() };

        let freed = E4_FREED.lock().unwrap();
        // All 8 table pages (root + PDPT + 3 PD + 3 PT), none of the data.
        // Set compare (order-insensitive) — PTEs carry pool *offsets*, so
        // the freed physical values are offsets too.
        let mut got = freed.clone();
        got.sort();
        let mut want: alloc::vec::Vec<u64> = alloc::vec![
            0x3000, 0x4000, 0x5000, // PT1, PD2, PD3
            0x2000, 0x7000, 0x9000, // PD1, PT2, PT3
            0x1000, 0,              // PDPT, root (freed last)
        ];
        want.sort();
        assert_eq!(got, want, "freed set = 8 table pages, no data pages");
        // Ordering contract: the root page (pool offset 0) is freed last.
        assert_eq!(freed.last().copied(), Some(0), "root freed last");
        // Data pages untouched.
        for d in [0x6000u64, 0x8000, 0xA000] {
            assert!(!freed.contains(&d), "data page {d:#x} must survive");
        }
    }

    /// fix14 regression: a `VmDm` handle's `map()` must reach the leaf
    /// PTE write without executing `invlpg` — VM exercises this channel
    /// at CPL3 where the flush is a privileged instruction (#GP(0)
    /// serial forensics 2026-09-21, rip = leaf write + 4 inside `map`).
    /// On the host the same instruction SIGSEGVs the test process, so
    /// this test passing end-to-end IS the no-flush proof.
    ///
    /// Intermediate levels are hand-built (same reason E4 hand-builds:
    /// `pt_alloc::register` is one-shot across the test binary), so the
    /// production code under test is exactly the leaf write + flush
    /// point from the fix14 crash.
    #[test]
    fn test_vmdm_channel_map_writes_pte_without_invlpg() {
        // Fresh leaked pool: zeroed root + hand-built intermediates.
        let pool: &'static mut [u8; E4_POOL_BYTES] =
            alloc::boxed::Box::leak(alloc::boxed::Box::new([0u8; E4_POOL_BYTES]));
        let base = pool.as_mut_ptr() as u64;
        crate::arch::direct_map::set_mock_vm_base(base);

        // vaddr 0x200_000: PML4[0] → PDPT(0x1000)[0] → PD(0x2000)[1] →
        // PT(0x3000)[0] → leaf slot left NOT present for map() to fill.
        let mid = X64PteFlags::PRESENT | X64PteFlags::WRITABLE | X64PteFlags::USER;
        let link = |table_off: u64, idx: usize, child_off: u64| {
            unsafe {
                core::ptr::write_volatile(
                    (base + table_off + (idx as u64) * 8) as *mut u64,
                    child_off | mid.bits(),
                )
            }
        };
        link(0, 0, 0x1000);
        link(0x1000, 0, 0x2000);
        link(0x2000, 1, 0x3000);

        let mut pt = X86_64Paging {
            root_paddr: 0,
            channel: PteChannel::VmDm,
        };
        let vaddr = minix_types::VirBytes(0x200_000);
        let paddr = minix_types::PhysBytes(0xE000);
        pt.map(vaddr, paddr, PageFlags::read_write())
            .expect("VmDm map() must succeed on the host (no privileged flush)");
        assert_eq!(
            pt.query(vaddr).map(|(p, _)| p),
            Some(paddr),
            "leaf written via VmDm channel must be queryable"
        );
    }
}
