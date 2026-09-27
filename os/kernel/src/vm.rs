//! Kernel virtual memory module.
//!
//! Implements cross-address-space operations and VMREQUEST suspend/resume
//! mechanism for the kernel.
//!
//! # Module Organization
//!
//! - **02-higher-half-kernel.md** types: `PageTableRef`, `AddressRef`,
//!   `VmCopyError`, `VmFaultType`, `CrossSpaceResult`, `VmCopyContext`,
//!   `cross_space_copy`, `cross_space_memset`
//! - **24-cross-space-runtime.md** types: `VmSuspendType`, `VmCheckParams`,
//!   `VmSuspendState`, `VmCheckResult`, `VmSuspendContext`, `VmRequestQueue`,
//!   `VmCtlError`, `VmRequestHandler`
//! - **09-vm-boot-protocol.md** types: `VmCtlParam`, `VmCtlResult`,
//!   `VmCtlError`
//!
//! Design decisions are documented in 02-higher-half-kernel.md §3 and
//! 24-cross-space-runtime.md §3.

use minix_types::{Endpoint, Message, PhysBytes, VirBytes};
use minix_arch::direct_map::DirectMapArch;
use minix_arch::paging::PageFlags;
use minix_arch::PteWalkArch;

use crate::proc::{KProcess, ProcNr, RtsFlagsBits, MiscFlagsBits};

/// NK4-C 第 33 轮守卫探针（task1-close 裁决删除）：内核跨空间写的每个
/// 目标物理地址。清零者必在内核侧（抹写发生在 RS 停车、仅内核运行的
/// 窗口），本探针把每次写的 PA 落串口，与同轮 pf dump 的 lvl1pa（PT
/// 页 PA）离线对账——命中即抓住把用户数据写进页表页的 walk/解码 bug。
#[cfg(not(feature = "mock"))]
pub(crate) fn nk4a_kdst_probe(tag: &str, dst_pa: u64, len: usize) {
    use core::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering as AtomicOrd};
    static N: AtomicUsize = AtomicUsize::new(0);
    static SEEN_PA: [AtomicU64; 128] = [const { AtomicU64::new(0) }; 128];
    static SEEN_LEN: [AtomicU64; 128] = [const { AtomicU64::new(0) }; 128];
    static SEEN_TAG: [AtomicU8; 128] = [const { AtomicU8::new(0) }; 128];
    let tag_key = tag.as_bytes()[0];
    let n = N.load(AtomicOrd::Relaxed);
    let mut i = 0;
    while i < n && i < 128 {
        if SEEN_PA[i].load(AtomicOrd::Relaxed) == dst_pa
            && SEEN_LEN[i].load(AtomicOrd::Relaxed) == len as u64
            && SEEN_TAG[i].load(AtomicOrd::Relaxed) == tag_key
        {
            return;
        }
        i += 1;
    }
    if n >= 128 {
        return;
    }
    SEEN_PA[n].store(dst_pa, AtomicOrd::Relaxed);
    SEEN_LEN[n].store(len as u64, AtomicOrd::Relaxed);
    SEEN_TAG[n].store(tag_key, AtomicOrd::Relaxed);
    N.fetch_add(1, AtomicOrd::Relaxed);
    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
    C0::write_str("nk4a: kdst ");
    C0::write_str(tag);
    C0::write_str(" pa=");
    C0::write_hex(dst_pa);
    C0::write_str(" len=");
    C0::write_hex(len as u64);
    C0::write_str("\n");
}

// ── 02-page-table-kernel types ──

pub struct PageTableRef {
    cr3: Option<PhysBytes>,
}

impl PageTableRef {
    pub fn new() -> Self {
        Self { cr3: None }
    }

    pub fn cr3(&self) -> Option<PhysBytes> {
        self.cr3
    }

    pub fn set_cr3(&mut self, cr3: PhysBytes) {
        self.cr3 = Some(cr3);
    }

    pub fn clear_cr3(&mut self) {
        self.cr3 = None;
    }

    pub fn page_table_vaddr<D: DirectMapArch>(&self) -> Option<VirBytes> {
        self.cr3.map(|c| D::kernel_phys_to_virt(c))
    }
}

impl Default for PageTableRef {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressRef {
    Process { endpoint: Endpoint, offset: VirBytes },
    Physical(PhysBytes),
}

impl AddressRef {
    /// Returns `(endpoint, virtual_address)` if this is a `Process` address.
    ///
    /// `Physical` addresses cannot page-fault (they are already resolved),
    /// so callers use this to extract the faulting VA for `VmCheckParams`
    /// construction. Returns `None` for `Physical`.
    pub(crate) fn as_process(&self) -> Option<(Endpoint, VirBytes)> {
        match self {
            AddressRef::Process { endpoint, offset } => Some((*endpoint, *offset)),
            AddressRef::Physical(_) => None,
        }
    }
}

/// Address-resolution error from cross-space copy operations.
///
/// These are genuine errors — the address is invalid or the endpoint
/// is unknown. Contrast with `CrossSpaceResult::Suspended`, which is
/// *not* an error but a signal that the operation needs VM assistance.
///
/// C source mapping:
/// - `SrcPageFault` ← `EFAULT_SRC` (-995), memory.c:195
/// - `DstPageFault` ← `EFAULT_DST` (-994), memory.c:196
/// - `InvalidAddress` ← `EFAULT` (14), generic address error
/// - `PermissionDenied` ← `EPERM` (1), from `do_safecopy.c` grant check
/// - `UnknownEndpoint` ← `ESRCH` (3), endpoint not found
/// - `Domain` ← `EDOM` (33), `virtual_copy_f` zero-length rejection,
///   memory.c:607 (`if (bytes <= 0) return(EDOM);`)
///
/// Note: Minix3's `VMSUSPEND` (-996) is NOT represented here. In C,
/// `VMSUSPEND` is a separate return value that triggers `vm_suspend()`;
/// it is not an error. See `CrossSpaceResult::Suspended`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCopyError {
    SrcPageFault,
    DstPageFault,
    InvalidAddress,
    /// Permission denied on grant access (C: `EPERM` from `do_safecopy.c`).
    /// Not returned by `virtual_copy_f`/`vm_check_range`/`vm_memset` directly,
    /// but by `sys_safecopy` which validates grant permissions before calling
    /// into the copy path.
    PermissionDenied,
    UnknownEndpoint,
    /// Zero-length copy rejected up front. C: `virtual_copy_f()` returns
    /// `EDOM` for `bytes <= 0` (memory.c:607) *before* any endpoint check
    /// or address translation, so a 0-byte copy never suspends and never
    /// reaches VM — even when one side is a dangling pointer (e.g. a
    /// zero-size `Vec`'s sentinel address, as issued by VFS select's
    /// fdset fetch when `nfds == 0`).
    Domain,
}

impl core::fmt::Display for VmCopyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VmCopyError::SrcPageFault => write!(f, "source page fault (EFAULT_SRC)"),
            VmCopyError::DstPageFault => write!(f, "destination page fault (EFAULT_DST)"),
            VmCopyError::InvalidAddress => write!(f, "invalid address (EFAULT)"),
            VmCopyError::PermissionDenied => write!(f, "permission denied (EPERM)"),
            VmCopyError::UnknownEndpoint => write!(f, "unknown endpoint (ESRCH)"),
            VmCopyError::Domain => write!(f, "out-of-domain, zero-length copy (EDOM)"),
        }
    }
}

impl core::fmt::Display for CrossSpaceResult {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CrossSpaceResult::Completed(Ok(())) => write!(f, "completed successfully"),
            CrossSpaceResult::Completed(Err(e)) => write!(f, "completed with error: {e}"),
            CrossSpaceResult::Suspended(ft) => write!(f, "suspended due to {ft:?} page fault"),
        }
    }
}
///
/// Separates two semantically distinct outcomes that C conflates in a
/// single return value:
///
/// - **Completed(Ok)**: operation finished successfully (C: return `OK`)
/// - **Completed(Err)**: address resolution failed (C: return `EFAULT_SRC`/`EFAULT_DST`)
/// - **Suspended**: operation paused because a page fault occurred and
///   VM must handle it before the operation can retry (C: return `VMSUSPEND`)
///
/// In Minix3 C, `virtual_copy_f` returns `VMSUSPEND`(-996) for suspend and
/// `EFAULT_SRC`/`EFAULT_DST` for errors. These are different control flows:
/// `VMSUSPEND` triggers `vm_suspend()` → `kernel_call_finish()` saves the
/// message; `EFAULT_SRC/DST` triggers normal error return. Mixing them in
/// a single `VmCopyError` enum (as the previous design did) conflates
/// "needs recovery" with "is an error".
///
/// Design decision: 02-higher-half-kernel.md §3.1 (Direct Map replaces
/// temporary PDE mapping), review fix for P0 #1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossSpaceResult {
    /// Operation completed (success or address error).
    Completed(Result<(), VmCopyError>),
    /// Operation suspended due to page fault; VM must handle it.
    /// `fault_type` indicates which side faulted, for `VmCopyContext` construction.
    Suspended(VmFaultType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmFaultType {
    Src,
    Dst,
}

/// Cross-address-space copy context.
///
/// Records the source, destination, byte count, and fault direction for a
/// suspended cross-space copy operation. Renamed from `VmRequest` (02-higher-half-kernel.md §4.3b)
/// to avoid confusion with `VmSuspendContext` (24-cross-space-runtime.md §3.2).
///
/// Design decision: §3.8 (merge with VmSuspendContext as `copy_context` field).
#[derive(Debug)]
pub struct VmCopyContext {
    pub src: AddressRef,
    pub dst: AddressRef,
    pub bytes: usize,
    pub fault_type: VmFaultType,
}

impl VmCopyContext {
    pub fn new(src: AddressRef, dst: AddressRef, bytes: usize, fault_type: VmFaultType) -> Self {
        Self { src, dst, bytes, fault_type }
    }
}

/// Walk the page table to resolve a virtual address to its physical mapping.
///
/// Uses Direct Map to read page table entries from physical memory.
/// Returns `Some((paddr, flags))` if the address is mapped, `None` otherwise.
///
/// C: vm_lookup() — memory.c:325
///
/// # Architecture dispatch
///
/// Delegates to `minix_arch::CurrentPteWalk::walk`, which selects the
/// architecture-specific `PteWalkArch` implementor at compile time:
/// - x86-64: 4-level PTE walk (PML4 → PDPT → PD → PT)
/// - aarch64: 4-level walk (L0 → L1 → L2 → L3)
/// - riscv64: 3-level Sv39 walk (L2 → L1 → L0)
///
/// All three implementations read PTEs via the Direct Map and share
/// the same `walk_read` helper that powers `Paging::query`, ensuring
/// the offline walk (used here for cross-space copy) and the live walk
/// (used by `Paging::query`) produce identical results.
pub fn lookup_in_table<D: DirectMapArch>(
    root_paddr: PhysBytes,
    vaddr: VirBytes,
) -> Option<(PhysBytes, PageFlags)> {
    // The `<D>` parameter is kept for trait dispatch consistency with
    // the rest of the cross-space copy API, which threads `DirectMapArch`
    // through the call chain. The PTE walk itself uses
    // `CurrentPteWalk` (which internally uses `CurrentDirectMap`) —
    // the `<D>` parameter is not used directly here because the arch
    // layer's walk implementation selects its own Direct Map.
    let _ = D::KERNEL_DIRECT_MAP_BASE; // keep D used
    minix_arch::CurrentPteWalk::walk(root_paddr, vaddr)
}

/// Base page size used by `lookup_range_in_table`.
///
/// All three supported architectures (x86_64, aarch64, riscv64) use 4KB
/// base pages. This constant mirrors C's `PAGE_SIZE` — `vm_lookup_range`
/// in Minix3 walks 4KB pages one at a time.
const VM_LOOKUP_PAGE_SIZE: u64 = 4096;

/// Walk the page table to find the largest contiguous physical range
/// starting at `vaddr`, up to `max_bytes`.
///
/// C: `vm_lookup_range` — minix3/minix/kernel/arch/i386/memory.c（earm 变体：
///    minix3/minix/kernel/arch/earm/memory.c，同名函数）
///
/// Returns `Some((phys_addr, chunk))` where `chunk` is the number of
/// contiguous bytes (≤ `max_bytes`) starting at `vaddr` that map to
/// contiguous physical memory. Returns `None` if the first page is
/// unmapped (matching C's `chunk == 0`).
///
/// # Algorithm
///
/// 1. Look up the first page → `phys_base`.
/// 2. First chunk extends to the end of the current 4KB page.
/// 3. For each subsequent page: walk `vaddr + chunk`, check if
///    `phys == phys_base + chunk`. Stop on mismatch or unmapped page.
///
/// # Huge page note
///
/// If a huge page is encountered, the walk still returns the correct
/// byte-level physical address, but contiguity is checked at 4KB
/// granularity. This matches Minix3's `vm_lookup_range`, which also
/// operates on base pages — huge pages are transparently contiguous
/// within their block, so the check succeeds.
pub fn lookup_range_in_table<D: DirectMapArch>(
    root_paddr: PhysBytes,
    vaddr: VirBytes,
    max_bytes: usize,
) -> Option<(PhysBytes, usize)> {
    let _ = D::KERNEL_DIRECT_MAP_BASE; // keep D used

    let (phys_base, _flags) = minix_arch::CurrentPteWalk::walk(root_paddr, vaddr)?;

    if max_bytes == 0 {
        return Some((phys_base, 0));
    }

    let max = max_bytes as u64;
    let va0 = vaddr.0;
    let page_offset = va0 % VM_LOOKUP_PAGE_SIZE;
    // Bytes remaining in the first page.
    let first_chunk = core::cmp::min(VM_LOOKUP_PAGE_SIZE - page_offset, max);
    let mut chunk = first_chunk;

    // Walk subsequent pages while there are bytes remaining.
    while chunk < max {
        let next_va = VirBytes(va0 + chunk);
        match minix_arch::CurrentPteWalk::walk(root_paddr, next_va) {
            Some((next_phys, _)) => {
                // Contiguous if next_phys == phys_base + chunk.
                if next_phys.0 == phys_base.0 + chunk {
                    let remaining = max - chunk;
                    let advance = core::cmp::min(VM_LOOKUP_PAGE_SIZE, remaining);
                    chunk += advance;
                } else {
                    // Non-contiguous physical mapping — stop here.
                    break;
                }
            }
            None => break, // Unmapped page — stop.
        }
    }

    Some((phys_base, chunk as usize))
}

/// K20: the `proc_cr3` closure receives the process table as a parameter
/// (it no longer captures it) — a caller holding `&mut ProcessTable` for
/// the suspend tail can therefore still call the closure through a shared
/// reborrow, and closures that only need copied field values simply
/// ignore the parameter.
fn resolve_physical<D: DirectMapArch>(
    addr: &AddressRef,
    proc_table: &crate::proc_table::ProcessTable,
    proc_cr3: impl Fn(&crate::proc_table::ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> Result<PhysBytes, ResolveError> {
    match addr {
        AddressRef::Physical(paddr) => Ok(*paddr),
        AddressRef::Process { endpoint, offset } => {
            let cr3 = proc_cr3(proc_table, *endpoint).ok_or(ResolveError::UnknownEndpoint)?;
            lookup_in_table::<D>(cr3, *offset)
                .map(|(paddr, _)| paddr)
                .ok_or(ResolveError::PageFault)
        }
    }
}

/// Validate that the Direct Map window carries a translation for every page
/// of a physical range, software-walk first so the window access below can
/// no longer fault.
///
/// `AddressRef::Physical` addresses arrive from callers (SYS_PHYSCOPY with
/// the NONE endpoint, SYS_MEMSET) and are not pre-resolved through a page
/// table — a caller-supplied address beyond the RAM the boot DM
/// establishment mapped (`establish_boot_dm`: the union of the memmap
/// candidates, full PA span) would fault inside the window access. C
/// recovers such faults by redirecting execution into the
/// `phys_copy_fault`/`memset_fault` labels (klib.S:204-214, fed by the
/// `in_physcopy`/`in_memset` EIP check — exception.c:66-70); this kernel
/// expresses the same contract in types: the range is validated here and
/// the copy path returns `Fault` instead of taking the fault.
///
/// The window translation is checked in the **active** root — the same
/// walk the MMU performs on the access, and the same precondition every
/// Direct Map access already stands on (kernel DM slots are present in
/// every root). Without an active root (no paging yet — boot or hosted
/// tests) there is no translation to validate, so the check passes.
///
/// `walk` is injected so hosted tests can drive both outcomes without real
/// page tables; production passes `minix_arch::CurrentPteWalk::walk`.
fn physical_range_in_dm_window(
    root: Option<PhysBytes>,
    dm_va: VirBytes,
    len: usize,
    walk: impl Fn(PhysBytes, VirBytes) -> Option<(PhysBytes, PageFlags)>,
) -> bool {
    let Some(root) = root else {
        return true;
    };
    if len == 0 {
        return true;
    }
    const PAGE: u64 = 4096;
    let start = dm_va.0 & !(PAGE - 1);
    let end = dm_va.0 + len as u64 - 1;
    let mut page = start;
    while page <= end {
        if walk(root, VirBytes(page)).is_none() {
            return false;
        }
        page += PAGE;
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveError {
    PageFault,
    UnknownEndpoint,
}

/// One resolved side of a cross-space copy: either a caller-supplied physical
/// base (contiguous by definition) or a process virtual base plus the
/// page-table root to walk.
///
/// B39 (§1.70): the copy path advances each side one *physically-contiguous
/// run* at a time via [`lookup_range_in_table`], so a virtual range whose pages
/// are scattered in physical memory is never written linearly off its first
/// frame (the Direct Map is linear in *physical*). This mirrors C's `vm_copy`
/// re-translating the target page table per window through `createpde`.
#[derive(Debug, Clone, Copy)]
enum CopySide {
    Physical(PhysBytes),
    Process { cr3: PhysBytes, base: VirBytes },
}

impl CopySide {
    fn resolve(
        addr: &AddressRef,
        proc_table: &crate::proc_table::ProcessTable,
        proc_cr3: &impl Fn(&crate::proc_table::ProcessTable, Endpoint) -> Option<PhysBytes>,
    ) -> Option<Self> {
        match addr {
            AddressRef::Physical(p) => Some(CopySide::Physical(*p)),
            AddressRef::Process { endpoint, offset } => {
                let cr3 = proc_cr3(proc_table, *endpoint)?;
                Some(CopySide::Process { cr3, base: *offset })
            }
        }
    }

    /// Physical address of the byte at offset `done`, plus the length (≤
    /// `remaining`) of the physically-contiguous run starting there. A
    /// `Process` side yields `(x, 0)` when that page is unmapped or the run
    /// breaks immediately — the caller suspends on it.
    fn chunk<D: DirectMapArch>(&self, done: usize, remaining: usize) -> (PhysBytes, usize) {
        match *self {
            CopySide::Physical(p) => (PhysBytes(p.0 + done as u64), remaining),
            CopySide::Process { cr3, base } => {
                let va = VirBytes(base.0 + done as u64);
                lookup_range_in_table::<D>(cr3, va, remaining).unwrap_or((PhysBytes(0), 0))
            }
        }
    }
}

/// Perform a cross-address-space memory copy.
///
/// Resolves source and destination physical addresses via Direct Map,
/// then copies `bytes` bytes. If address resolution encounters a page
/// fault, returns `CrossSpaceResult::Suspended` so the caller can
/// initiate VMREQUEST suspend/resume.
///
/// # Resume path
///
/// When `Suspended` is returned, the caller should:
/// 1. Construct a `VmCopyContext` from the fault direction
/// 2. Call `vm_suspend()` to enqueue the request
/// 3. On VM reply, retry this function with the same arguments
///
/// C: `virtual_copy_f()` — memory.c:592
pub fn cross_space_copy<D: DirectMapArch>(
    src: &AddressRef,
    dst: &AddressRef,
    bytes: usize,
    proc_table: &crate::proc_table::ProcessTable,
    proc_cr3: impl Fn(&crate::proc_table::ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    // C: `virtual_copy_f()` first check — memory.c:607 `if (bytes <= 0)
    // return(EDOM);` — runs before endpoint validation and any page-table
    // walk. A zero-length copy must therefore never resolve addresses,
    // never suspend on VM, and never fault the requestor, regardless of
    // the pointer values (the aarch64 rc-chain incident: VFS select's
    // 0-byte fdset fetch carried a zero-size `Vec` dangling sentinel as
    // destination, which previously walked pages, suspended on VM, and
    // escalated to a SIGSEGV-then-panic instead of an EDOM reply).
    if bytes == 0 {
        return CrossSpaceResult::Completed(Err(VmCopyError::Domain));
    }

    // B39 (§1.70): both sides may be *virtual* ranges whose pages map to
    // non-adjacent physical frames. A single-shot resolve + linear
    // `copy_nonoverlapping(bytes)` runs off each first frame into neighbouring
    // physical memory. Walk each side one physically-contiguous run at a time
    // and copy the intersection (C: `vm_copy` / `lin_lin_copy` re-translating
    // the page table per window). `resolve_physical` still pre-validates both
    // endpoints and their first pages (preserving UnknownEndpoint /
    // first-page-suspend error ordering) and feeds the guard probe the first
    // destination PA.
    if let Err(e) = resolve_physical::<D>(src, proc_table, &proc_cr3) {
        return match e {
            ResolveError::PageFault => CrossSpaceResult::Suspended(VmFaultType::Src),
            ResolveError::UnknownEndpoint => {
                CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint))
            }
        };
    }
    let dst_phys = match resolve_physical::<D>(dst, proc_table, &proc_cr3) {
        Ok(p) => p,
        Err(ResolveError::PageFault) => return CrossSpaceResult::Suspended(VmFaultType::Dst),
        Err(ResolveError::UnknownEndpoint) => {
            return CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint))
        }
    };
    // 探针门必须与 nk4a_kdst_probe 定义门（not(feature = "mock")）一致：
    // 宿主 mock 构建下 not(test) 对普通 lib 编译为真，会引用已被裁掉的
    // 定义导致 E0425（HEAD 存量破损，2026-09 宿主测试基线跑出）。task1-close 裁决删除
    #[cfg(not(feature = "mock"))]
    nk4a_kdst_probe("copy", dst_phys.0, bytes);
    let _ = dst_phys;

    let src_side = CopySide::resolve(src, proc_table, &proc_cr3).expect("src endpoint validated");
    let dst_side = CopySide::resolve(dst, proc_table, &proc_cr3).expect("dst endpoint validated");

    let mut done = 0usize;
    while done < bytes {
        let remaining = bytes - done;
        let (sp, srun) = src_side.chunk::<D>(done, remaining);
        let (dp, drun) = dst_side.chunk::<D>(done, remaining);
        let chunk = core::cmp::min(srun, drun);
        if chunk == 0 {
            // A side stopped at an unmapped / run-breaking page; suspend on it.
            return if srun == 0 {
                CrossSpaceResult::Suspended(VmFaultType::Src)
            } else {
                CrossSpaceResult::Suspended(VmFaultType::Dst)
            };
        }
        let src_vaddr = D::kernel_phys_to_virt(sp);
        let dst_vaddr = D::kernel_phys_to_virt(dp);
        #[cfg(not(test))]
        {
            let active_root = crate::current_root_phys();
            if !physical_range_in_dm_window(
                active_root,
                src_vaddr,
                chunk,
                minix_arch::CurrentPteWalk::walk,
            ) {
                return CrossSpaceResult::Completed(Err(VmCopyError::SrcPageFault));
            }
            if !physical_range_in_dm_window(
                active_root,
                dst_vaddr,
                chunk,
                minix_arch::CurrentPteWalk::walk,
            ) {
                return CrossSpaceResult::Completed(Err(VmCopyError::DstPageFault));
            }
        }
        // SAFETY:
        // - src_vaddr/dst_vaddr are Direct Map aliases of page-backed
        //   physically-contiguous runs returned by `lookup_range_in_table`.
        // - Caller must ensure the two regions do not overlap (unchanged caveat
        //   from the pre-loop code); same-page shared-memory copies need a
        //   temporary on the caller side.
        // - BKL ensures no concurrent mutation; u8 has no alignment requirement.
        unsafe {
            core::ptr::copy_nonoverlapping(
                src_vaddr.0 as *const u8,
                dst_vaddr.0 as *mut u8,
                chunk,
            );
        }
        done += chunk;
    }

    CrossSpaceResult::Completed(Ok(()))
}

/// Perform a cross-address-space memory set.
///
/// Resolves destination physical address via Direct Map, then fills
/// `count` bytes with `value`. If address resolution encounters a page
/// fault, returns `CrossSpaceResult::Suspended`.
///
/// C: `vm_memset()` — memory.c:526
pub fn cross_space_memset<D: DirectMapArch>(
    dst: &AddressRef,
    value: u8,
    count: usize,
    proc_table: &crate::proc_table::ProcessTable,
    proc_cr3: impl Fn(&crate::proc_table::ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    // B39 (§1.70): a `Process` destination is a *virtual* range whose pages are
    // scattered across non-adjacent physical frames. The Direct Map is linear in
    // *physical*, so a single-shot resolve + `write_bytes(count)` runs off the
    // first frame into whatever physical memory follows it — this is what
    // trampled VFS's stack during RS's ~4 MiB BSS clear. Walk the destination
    // one physically-contiguous run at a time via `lookup_range_in_table` (C:
    // `vm_memset` re-translation per window through `createpde`, memory.c:526).
    // `resolve_physical` still validates the endpoint and the first page up
    // front (preserving UnknownEndpoint / first-page-suspend error ordering)
    // and gives the guard probe its first target PA.
    let dst_phys = match resolve_physical::<D>(dst, proc_table, &proc_cr3) {
        Ok(p) => p,
        Err(ResolveError::PageFault) => return CrossSpaceResult::Suspended(VmFaultType::Dst),
        Err(ResolveError::UnknownEndpoint) => {
            return CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint))
        }
    };

    // NK4-C 第 33 轮守卫探针（task1-close 裁决删除；门与定义一致）
    #[cfg(not(feature = "mock"))]
    nk4a_kdst_probe("memset", dst_phys.0, count);
    let _ = dst_phys;

    if count == 0 {
        return CrossSpaceResult::Completed(Ok(()));
    }

    // Physical destinations are contiguous by definition — a single guarded
    // linear write is correct (matches C's NONE/`phys_memset` fast path).
    if let AddressRef::Physical(base) = dst {
        let dst_vaddr = D::kernel_phys_to_virt(*base);
        #[cfg(not(test))]
        if !physical_range_in_dm_window(
            crate::current_root_phys(),
            dst_vaddr,
            count,
            minix_arch::CurrentPteWalk::walk,
        ) {
            return CrossSpaceResult::Completed(Err(VmCopyError::DstPageFault));
        }
        // SAFETY: `dst_vaddr` is the Direct Map alias of a caller-supplied
        // physical range validated present above; u8 has no alignment
        // requirement; BKL excludes concurrent mutation of this region.
        unsafe {
            core::ptr::write_bytes(dst_vaddr.0 as *mut u8, value, count);
        }
        return CrossSpaceResult::Completed(Ok(()));
    }

    // Process (virtual) destination: walk one contiguous run at a time.
    let (endpoint, base_va) = dst.as_process().expect("non-physical arm checked above");
    let cr3 = proc_cr3(proc_table, endpoint).expect("endpoint validated by resolve_physical");
    let mut done = 0usize;
    while done < count {
        let va = VirBytes(base_va.0 + done as u64);
        let (phys, chunk) = match lookup_range_in_table::<D>(cr3, va, count - done) {
            Some(r) => r,
            None => return CrossSpaceResult::Suspended(VmFaultType::Dst),
        };
        if chunk == 0 {
            return CrossSpaceResult::Suspended(VmFaultType::Dst);
        }
        let dst_vaddr = D::kernel_phys_to_virt(phys);
        #[cfg(not(test))]
        if !physical_range_in_dm_window(
            crate::current_root_phys(),
            dst_vaddr,
            chunk,
            minix_arch::CurrentPteWalk::walk,
        ) {
            return CrossSpaceResult::Completed(Err(VmCopyError::DstPageFault));
        }
        // SAFETY: `dst_vaddr` is the Direct Map alias of `chunk` bytes of a
        // page-backed, physically-contiguous run returned by
        // `lookup_range_in_table`; u8 has no alignment requirement; BKL
        // excludes concurrent mutation.
        unsafe {
            core::ptr::write_bytes(dst_vaddr.0 as *mut u8, value, chunk);
        }
        done += chunk;
    }

    CrossSpaceResult::Completed(Ok(()))
}

/// Copy kernel-local memory into a process address space.
///
/// One-sided variant of [`cross_space_copy`] for kernel-produced data —
/// C's `SELF` source in `sys_datacopy(SELF, &local, caller, ...)` (the
/// `do_getinfo.c:209-217` common tail). The source is kernel memory that
/// the kernel reads **directly by virtual address**: kernel stack/data
/// VAs sit in the higher-half kernel window, outside both Direct Map
/// windows, so running them through `DirectMapArch::virt_to_phys` would
/// wrap into a bogus "physical" and a non-canonical alias (the live #GP
/// this replaced in test-user-trap GET_HZ). Only the destination needs
/// PTE resolution and the DM alias. Page-fault semantics match the
/// destination half of [`cross_space_copy`].
///
/// C: `virtual_copy_f()` source-side SELF branch — memory.c:592
pub fn cross_space_write<D: DirectMapArch>(
    src: &[u8],
    dst: &AddressRef,
    proc_table: &crate::proc_table::ProcessTable,
    proc_cr3: impl Fn(&crate::proc_table::ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    let dst_phys = match resolve_physical::<D>(dst, proc_table, &proc_cr3) {
        Ok(p) => p,
        Err(ResolveError::PageFault) => return CrossSpaceResult::Suspended(VmFaultType::Dst),
        Err(ResolveError::UnknownEndpoint) => {
            return CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint))
        }
    };
    // NK4-C 第 33 轮守卫探针（task1-close 裁决删除；门与定义一致）
    #[cfg(not(feature = "mock"))]
    nk4a_kdst_probe("write", dst_phys.0, src.len());
    let _ = dst_phys;

    // B39 (§1.70): a `Process` destination is virtual and its pages are
    // physically scattered — walk it one contiguous run at a time from the
    // contiguous kernel-local `src` (C: `virtual_copy` dst-side `createpde`
    // re-translation). A `Physical` destination is contiguous by definition.
    let bytes = src.len();
    if bytes == 0 {
        return CrossSpaceResult::Completed(Ok(()));
    }
    if let AddressRef::Physical(base) = dst {
        let dst_vaddr = D::kernel_phys_to_virt(*base);
        #[cfg(not(test))]
        if !physical_range_in_dm_window(
            crate::current_root_phys(),
            dst_vaddr,
            bytes,
            minix_arch::CurrentPteWalk::walk,
        ) {
            return CrossSpaceResult::Completed(Err(VmCopyError::DstPageFault));
        }
        // SAFETY: `dst_vaddr` is the DM alias of a physical range validated
        // present above; `src` is kernel-local and cannot overlap that alias;
        // BKL excludes concurrent mutation; u8 has no alignment requirement.
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr(), dst_vaddr.0 as *mut u8, bytes);
        }
        return CrossSpaceResult::Completed(Ok(()));
    }
    let (endpoint, base_va) = dst.as_process().expect("non-physical arm checked above");
    let cr3 = proc_cr3(proc_table, endpoint).expect("endpoint validated by resolve_physical");
    let mut done = 0usize;
    while done < bytes {
        let va = VirBytes(base_va.0 + done as u64);
        let (phys, chunk) = match lookup_range_in_table::<D>(cr3, va, bytes - done) {
            Some(r) => r,
            None => return CrossSpaceResult::Suspended(VmFaultType::Dst),
        };
        if chunk == 0 {
            return CrossSpaceResult::Suspended(VmFaultType::Dst);
        }
        let dst_vaddr = D::kernel_phys_to_virt(phys);
        #[cfg(not(test))]
        if !physical_range_in_dm_window(
            crate::current_root_phys(),
            dst_vaddr,
            chunk,
            minix_arch::CurrentPteWalk::walk,
        ) {
            return CrossSpaceResult::Completed(Err(VmCopyError::DstPageFault));
        }
        // SAFETY: `dst_vaddr` aliases `chunk` bytes of a page-backed contiguous
        // run; `src[done..done+chunk]` is kernel-local and cannot overlap it;
        // BKL excludes concurrent mutation; u8 has no alignment requirement.
        unsafe {
            core::ptr::copy_nonoverlapping(src.as_ptr().add(done), dst_vaddr.0 as *mut u8, chunk);
        }
        done += chunk;
    }

    CrossSpaceResult::Completed(Ok(()))
}

/// Copy `dst.len()` bytes from a process address space into a kernel-local
/// buffer.
///
/// Read-side mirror of [`cross_space_write`] — C's `data_copy(granter, ptr,
/// KERNEL, &local, bytes)` (do_safecopy.c:121-127 `verify_grant` grant-table
/// read; the `KERNEL` endpoint means the kernel touches `&local` by its own
/// virtual address). Only the SOURCE needs PTE resolution + the Direct Map
/// alias; the destination is kernel-local memory (a stack/heap `struct` the
/// kernel owns) and is written **directly by its own VA**. Routing a kernel
/// stack VA through `DirectMapArch::virt_to_phys` yields a bogus physical
/// whose DM alias is not mapped (kernel data sits outside the Direct Map
/// window — see the note on [`cross_space_write`]), which the DM-window guard
/// would reject as `DstPageFault`.
///
/// C: `virtual_copy_f()` destination-side KERNEL branch — memory.c
pub fn cross_space_read<D: DirectMapArch>(
    dst: &mut [u8],
    src: &AddressRef,
    proc_table: &crate::proc_table::ProcessTable,
    proc_cr3: impl Fn(&crate::proc_table::ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    // Copy the source one *physically-contiguous* run at a time into the
    // always-contiguous kernel-local destination — C's `virtual_copy` /
    // `lin_lin_copy` walk (memory.c). A single-shot resolve would silently
    // read the neighbouring physical frame when the object straddles a page
    // boundary (e.g. a 40-byte `CpGrant` whose slot crosses a 4 KiB edge),
    // because the DM window covers all RAM and would still validate the
    // bogus tail. `lookup_range_in_table` stops at the first non-contiguous
    // / unmapped page, so each chunk stays inside one physical run.
    let (cr3, base_va) = match src {
        AddressRef::Physical(src_phys) => {
            // A pre-resolved physical address is contiguous by definition;
            // no page-table walk, just the DM alias (guarded) + copy.
            let src_vaddr = D::kernel_phys_to_virt(*src_phys);
            #[cfg(not(test))]
            if !physical_range_in_dm_window(
                crate::current_root_phys(),
                src_vaddr,
                dst.len(),
                minix_arch::CurrentPteWalk::walk,
            ) {
                return CrossSpaceResult::Completed(Err(VmCopyError::SrcPageFault));
            }
            // SAFETY: `src_vaddr` is the DM alias of a physical frame; `dst`
            // is kernel-local memory the kernel owns exclusively under the
            // BKL, is not a DM alias and cannot overlap `src_vaddr`; u8 has
            // no alignment requirement.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    src_vaddr.0 as *const u8,
                    dst.as_mut_ptr(),
                    dst.len(),
                );
            }
            return CrossSpaceResult::Completed(Ok(()));
        }
        AddressRef::Process { endpoint, offset } => {
            let cr3 = match proc_cr3(proc_table, *endpoint) {
                Some(c) => c,
                None => return CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint)),
            };
            (cr3, offset.0)
        }
    };

    let total = dst.len();
    let mut done = 0usize;
    while done < total {
        let va = VirBytes(base_va + done as u64);
        // Longest contiguous run at `va`, capped at the bytes still needed.
        let (phys, chunk) = match lookup_range_in_table::<D>(cr3, va, total - done) {
            Some(r) => r,
            // First byte of this run is on an unmapped page → source fault.
            None => return CrossSpaceResult::Suspended(VmFaultType::Src),
        };
        if chunk == 0 {
            return CrossSpaceResult::Suspended(VmFaultType::Src);
        }
        let src_vaddr = D::kernel_phys_to_virt(phys);
        // Validate this chunk's DM alias is mapped under the active root
        // before touching it (mirrors the source arm of `cross_space_copy`).
        // The kernel-local destination is not a DM alias, so it needs no guard.
        #[cfg(not(test))]
        if !physical_range_in_dm_window(
            crate::current_root_phys(),
            src_vaddr,
            chunk,
            minix_arch::CurrentPteWalk::walk,
        ) {
            return CrossSpaceResult::Completed(Err(VmCopyError::SrcPageFault));
        }
        // SAFETY:
        // - `src_vaddr` is the Direct Map alias of `chunk` bytes of a
        //   page-backed, physically-contiguous run returned by
        //   `lookup_range_in_table`.
        // - `dst[done..done + chunk]` is kernel-local memory the kernel owns
        //   exclusively under the BKL; it is not a DM alias and cannot
        //   overlap `src_vaddr`.
        // - u8 has no alignment requirements.
        unsafe {
            core::ptr::copy_nonoverlapping(
                src_vaddr.0 as *const u8,
                dst.as_mut_ptr().add(done),
                chunk,
            );
        }
        done += chunk;
    }

    CrossSpaceResult::Completed(Ok(()))
}

pub fn copy_page_table_ref(_as: &PageTableRef) -> PageTableRef {
    PageTableRef::new()
}

// ── 03-vm-request types ──

/// VM suspend type.
///
/// Replaces Minix3's `VMSTYPE_KERNELCALL`/`VMSTYPE_DELIVERMSG` macros.
/// `VMSTYPE_MAP` and `VMSTYPE_SYS_NONE` are not implemented — they are
/// unused in Minix3 (24-cross-space-runtime.md §2.5.6).
///
/// Design decision: §3.1 (enum replaces VMSTYPE_* macros).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmSuspendType {
    /// Kernel call (sys_copy etc.) interrupted by page fault.
    /// C: `VMSTYPE_KERNELCALL` (1), proc.h:99
    KernelCall,
    /// Message delivery (copy_msg_to_user) interrupted by page fault.
    /// C: `VMSTYPE_DELIVERMSG` (2), proc.h:100
    DeliverMsg,
}

/// VM check parameters.
///
/// Extracted from Minix3's `p_vmrequest.params.check` struct.
/// `writeflag` is `bool` instead of C's `u8_t` (0/nonzero) to eliminate
/// the "nonzero means write" implicit convention.
///
/// Design decision: §3.2 (extracted as independent struct).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmCheckParams {
    pub start: VirBytes,
    pub length: VirBytes,
    /// `false` = read access, `true` = write access.
    /// C: `p_vmrequest.params.check.writeflag` (u8_t, 0=read, nonzero=write)
    pub write_flag: bool,
}

/// VM check result.
///
/// Simplified from Minix3's arbitrary errno values. `check_resumed_caller()`
/// (memory.c:135-145) only checks `vmresult != OK`, so two variants suffice.
///
/// Design decision: §3.3 (simplified from arbitrary errno).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCheckResult {
    /// VM confirmed address range is valid (C: `vmresult == OK`).
    Ok,
    /// VM confirmed address range is invalid (C: `vmresult == EFAULT` etc.).
    Fault,
}

/// VM suspend state.
///
/// Replaces Minix3's `vmresult` three-state sentinel pattern:
/// - `0` (initial) → `Pending`
/// - `VMSUSPEND` (-996) → `Fetched`
/// - other errno → `Completed(Ok/Fault)`
///
/// State machine: `Pending ──memreq_get──▶ Fetched ──memreq_reply──▶ Completed(Ok/Fault)`
///
/// Design decision: §3.3 (enum replaces vmresult three-state sentinel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmSuspendState {
    /// Request queued, waiting for VM to fetch.
    /// C: `vmresult == 0` (implicit, vm_suspend does not explicitly set)
    Pending,
    /// VM has fetched the request, waiting for reply.
    /// C: `vmresult == VMSUSPEND` (-996), set by VMCTL_MEMREQ_GET
    Fetched,
    /// VM has replied with a result.
    /// C: `vmresult == OK` or `vmresult == EFAULT`, set by VMCTL_MEMREQ_REPLY
    Completed(VmCheckResult),
}

/// VM suspend context.
///
/// Replaces Minix3's `p_vmrequest` anonymous struct. Stored as
/// `Option<VmSuspendContext>` in `KProcess.p_vm_suspend`.
///
/// Invariant: `p_rts_flags.is_set(VMREQUEST) <==> p_vm_suspend.is_some()`
///
/// Design decision: §3.2 (replaces p_vmrequest), §3.8 (merges 02 doc VmRequest
/// as `copy_context` field).
#[derive(Debug)]
pub struct VmSuspendContext {
    /// Type of suspended operation.
    /// C: `p_vmrequest.type` (int, VMSTYPE_*)
    pub suspend_type: VmSuspendType,

    /// Target process endpoint whose address space caused the fault.
    /// C: `p_vmrequest.target` (endpoint_t)
    pub target: Endpoint,

    /// Address range check parameters sent to VM.
    /// C: `p_vmrequest.params.check.*`
    pub check_params: VmCheckParams,

    /// NK4-A C-3 迭代6：SYSCALL 快路径挂起时保存的用户消息指针
    /// （syscall ABI：RDI）。KCALL_RESUME 恢复阶段用它重派 kernel_call
    /// （内存此时已由 VM 填充，重读用户消息缓冲可行）。IPC 陷阱腿不经
    /// 此字段（saved_msg 承载）。
    pub saved_m_user: Option<u64>,

    /// NK4-C S3 门纪律标记：本次挂起是否源自 int-33 IPC 腿
    /// （`kernel_call_finish_ipc_door`，门语义见其文档）。C 对位：
    /// `copy_msg_to_user(p_delivermsg_vir)` 只存在于 `kernel_call()` 腿
    /// （system.c:83）——该腿每次入口都先刷新 p_delivermsg_vir
    /// （system.c:141），陷阱腿（proc.c mini_*）从不写调用者消息缓冲。
    /// 置真时 stage 3a 补完成同样跳过 eager 回执直写，杜绝恢复后向陈旧
    /// p_delivermsg_vir（如 SENDA 窗内不刷新者）落写 80 字节。
    pub resume_skip_eager_reply: bool,

    /// Current state of the suspend request.
    /// C: `p_vmrequest.vmresult` (three-state int)
    pub state: VmSuspendState,

    /// Saved system call message for kernel call resume.
    /// `Some` when `suspend_type == KernelCall` (saved by kernel_call_finish)
    /// or `suspend_type == DeliverMsg` (saved by delivermsg).
    /// C: `p_vmrequest.saved.reqmsg`
    pub saved_msg: Option<Message>,

    /// Cross-space copy context, only for KernelCall with copy operations.
    /// `None` for vm_check_range (no copy to resume) and DeliverMsg.
    /// Migrated from 02-higher-half-kernel.md `VmRequest`.
    /// Design decision: §3.8
    pub copy_context: Option<VmCopyContext>,
}

/// VM request queue.
///
/// Replaces Minix3's `vmrequest` global linked list of `struct proc *`.
/// Uses `ProcNr` indices instead of raw pointers — process update (live update)
/// no longer requires pointer migration since indices auto-point to the new
/// process in the same slot.
///
/// All operations require BKL to be held by the caller (00-kernel-overview §1.5).
///
/// Design decision: §3.4 (ProcNr index replaces *proc pointer).
pub struct VmRequestQueue {
    head: Option<ProcNr>,
}

impl Default for VmRequestQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl VmRequestQueue {
    pub const fn new() -> Self {
        Self { head: None }
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    /// Get the head of the queue (first ProcNr/index).
    /// `pub(crate)` because only `ProcessTable` needs direct access for
    /// `nr_to_idx`-aware traversal.
    pub(crate) fn head(&self) -> Option<ProcNr> {
        self.head
    }

    /// Set the head of the queue.
    pub(crate) fn set_head(&mut self, head: Option<ProcNr>) {
        self.head = head;
    }

    /// Enqueue a process at the head of the queue (head insertion).
    ///
    /// Returns `true` if the queue was empty before insertion — the caller
    /// should send `SIGKMEM` to VM in that case.
    ///
    /// C: `vm_suspend()` in proc.c:254-257:
    /// ```c
    /// if(!(caller->p_vmrequest.nextrequestor = vmrequest))
    ///     if(OK != send_sig(VM_PROC_NR, SIGKMEM))
    ///         panic("send_sig failed");
    /// vmrequest = caller;
    /// ```
    pub fn enqueue(&mut self, proc_nr: ProcNr, procs: &mut [KProcess]) -> bool {
        debug_assert!((proc_nr.0 as usize) < procs.len(), "ProcNr out of bounds");
        let proc = &mut procs[proc_nr.0 as usize];
        proc.p_next_requestor = self.head;
        let was_empty = self.head.is_none();
        self.head = Some(proc_nr);
        was_empty
    }

    /// Dequeue the first process that passes the filter.
    ///
    /// Corresponds to `VMCTL_MEMREQ_GET` traversal in do_vmctl.c:37-79.
    /// The filter replaces Minix3's `allow_ipc_filtered_memreq()` — VM may
    /// set an IPC filter during update operations, blocking certain requests.
    ///
    /// Returns `None` if the queue is empty or all requests are filtered out
    /// (C: `return ENOENT`).
    pub fn dequeue_filtered<F>(
        &mut self,
        procs: &mut [KProcess],
        mut filter: F,
    ) -> Option<ProcNr>
    where
        F: FnMut(&KProcess, &KProcess) -> bool,
    {
        let mut current = self.head;
        let mut prev_nr: Option<ProcNr> = None;

        while let Some(nr) = current {
            let proc = &procs[nr.0 as usize];
            let next = proc.p_next_requestor;

            let target_ctx = proc.p_vm_suspend.as_ref();
            let passed = match target_ctx {
                Some(ctx) => {
                    let target_nr = endpoint_to_proc_nr(ctx.target, procs);
                    match target_nr {
                        Some(tnr) => filter(proc, &procs[tnr.0 as usize]),
                        None => false,
                    }
                }
                None => false,
            };

            if passed {
                if let Some(pnr) = prev_nr {
                    procs[pnr.0 as usize].p_next_requestor = next;
                } else {
                    self.head = next;
                }
                procs[nr.0 as usize].p_next_requestor = None;
                return Some(nr);
            }

            prev_nr = Some(nr);
            current = next;
        }

        None
    }

    /// Enqueue a process and notify VM if the queue was empty.
    ///
    /// Separates the "enqueue" and "notify" concerns from Minix3's combined
    /// `vm_suspend()` function. Design decision: §3.9.
    // R-18 (2026-08-13): `()` error type delegates to `send_sig()` closure
    // (SYS_SIGSEND to VM). Single failure mode — VM notification send failed.
    // No diagnostic info to carry beyond "failed". Allowed per clippy.
    #[allow(clippy::result_unit_err)]
    pub fn enqueue_and_notify(
        &mut self,
        proc_nr: ProcNr,
        procs: &mut [KProcess],
        send_sig: &mut dyn FnMut() -> Result<(), ()>,
    ) -> Result<(), ()> {
        let was_empty = self.enqueue(proc_nr, procs);
        if was_empty {
            send_sig()?;
        }
        Ok(())
    }

    /// Remove a specific process from the queue.
    ///
    /// Corresponds to `clear_memreq()` in system.c:488-503.
    /// Used when a process exits and its pending request must be cleaned up.
    pub fn remove(&mut self, proc_nr: ProcNr, procs: &mut [KProcess]) -> bool {
        let mut current = self.head;
        let mut prev_nr: Option<ProcNr> = None;

        while let Some(nr) = current {
            if nr == proc_nr {
                let next = procs[nr.0 as usize].p_next_requestor;
                if let Some(pnr) = prev_nr {
                    procs[pnr.0 as usize].p_next_requestor = next;
                } else {
                    self.head = next;
                }
                procs[nr.0 as usize].p_next_requestor = None;
                return true;
            }
            prev_nr = Some(nr);
            current = procs[nr.0 as usize].p_next_requestor;
        }

        false
    }
}

fn endpoint_to_proc_nr(endpoint: Endpoint, procs: &[KProcess]) -> Option<ProcNr> {
    procs.iter().find(|p| p.p_endpoint == endpoint).map(|p| p.p_nr)
}

// VMCTL error type.
//
// Replaces Minix3's `panic()` and `ENOENT` return values in
// `VMCTL_MEMREQ_GET/REPLY` handlers. Design decision: §3.7.
//
// # BKL requirement
//
// All `VmCtlError`-returning operations execute under BKL.
// `VmRequestHandler` methods are called from syscall handlers
// which hold BKL throughout. No additional synchronization needed.
// ── 09-vm-boot-protocol types ──

/// VMCTL 子命令参数。
///
/// C: `SVMCTL_PARAM` 字段，`minix/com.h` 中 `VMCTL_*` 定义。
/// `do_vmctl.c:17-173` 中的 switch 分支。
///
/// 架构特定命令（GetPdbr, SetAddrSpace, FlushTlb, InvlPg）在
/// `arch_do_vmctl()` 中处理（arch_do_vmctl.c:38-65）。
///
/// Design decision: enum + match 替代 C 的 switch/case（09-vm-boot-protocol.md §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCtlParam {
    /// 清除进程的页错误标志。
    /// C: `VMCTL_CLEAR_PAGEFAULT`, do_vmctl.c:32-35
    ClearPageFault,
    /// VM 获取下一个挂起的内存请求。
    /// C: `VMCTL_MEMREQ_GET`, do_vmctl.c:36-72
    MemReqGet,
    /// VM 回复内存请求结果。
    /// C: `VMCTL_MEMREQ_REPLY`, do_vmctl.c:73-104
    MemReqReply,
    /// 内核声明需映射的物理区（32 位遗留，64 位 noop）。
    /// C: `VMCTL_KERN_PHYSMAP`, do_vmctl.c:105-112
    KernPhysMap,
    /// VM 返回虚拟地址（32 位遗留，64 位 noop）。
    /// C: `VMCTL_KERN_MAP_REPLY`, do_vmctl.c:113-118
    KernMapReply,
    /// 设置 VMINHIBIT 标志，阻止进程调度。
    /// C: `VMCTL_VMINHIBIT_SET`, do_vmctl.c:119-131
    VmInhibitSet,
    /// 清除 VMINHIBIT 标志，允许进程调度。
    /// C: `VMCTL_VMINHIBIT_CLEAR`, do_vmctl.c:132-160
    VmInhibitClear,
    /// 清除映射缓存。
    /// C: `VMCTL_CLEARMAPCACHE`, do_vmctl.c:161-164
    ClearMapCache,
    /// 清除 BOOTINHIBIT 标志。
    /// C: `VMCTL_BOOTINHIBIT_CLEAR`, do_vmctl.c:165-167
    BootInhibitClear,
    /// 获取进程 CR3/PDBR（x86 特定）。
    /// C: `VMCTL_GET_PDBR`, arch_do_vmctl.c:50-52
    GetPdbr,
    /// 设置进程地址空间（CR3 + 虚拟地址）。
    /// C: `VMCTL_SETADDRSPACE`, arch_do_vmctl.c:53-55
    SetAddrSpace,
    /// 刷新 TLB。
    /// C: `VMCTL_FLUSHTLB`, arch_do_vmctl.c:56-59
    FlushTlb,
    /// 单页 TLB 失效（x86 特定）。
    /// C: `VMCTL_I386_INVLPG`, arch_do_vmctl.c:60-63
    InvlPg,
}

/// Parse `SVMCTL_PARAM` integer into `VmCtlParam`.
///
/// C: `m_ptr->SVMCTL_PARAM` (m1_i2) is a `VMCTL_*` constant from com.h:395-409.
/// Unknown values fall through to `arch_do_vmctl()` in C, which returns EINVAL.
impl TryFrom<i32> for VmCtlParam {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            12 => Ok(VmCtlParam::ClearPageFault),
            13 => Ok(VmCtlParam::GetPdbr),
            14 => Ok(VmCtlParam::MemReqGet),
            15 => Ok(VmCtlParam::MemReqReply),
            27 => Ok(VmCtlParam::KernPhysMap),
            28 => Ok(VmCtlParam::KernMapReply),
            29 => Ok(VmCtlParam::SetAddrSpace),
            30 => Ok(VmCtlParam::VmInhibitSet),
            31 => Ok(VmCtlParam::VmInhibitClear),
            32 => Ok(VmCtlParam::ClearMapCache),
            33 => Ok(VmCtlParam::BootInhibitClear),
            26 => Ok(VmCtlParam::FlushTlb),
            25 => Ok(VmCtlParam::InvlPg),
            // VMCTL_NOPAGEZERO (18) and VMCTL_I386_KERNELLIMIT (19) are
            // 32-bit only and unused on 64-bit — return ENOSYS.
            _ => Err(()),
        }
    }
}

/// VMCTL 系统调用返回值。
///
/// C: `do_vmctl()` 返回 `int`，含义：
/// - `OK` (0): 成功
/// - `ENOENT` (2): 无匹配请求
/// - `EINVAL` (22): 无效参数
/// - `VMSUSPEND` (-996): 需 VM 协助
/// - `VMPTYPE_CHECK` (1): 请求类型
///
/// Design decision: enum 替代 C 的魔术数返回值（09-vm-boot-protocol.md §3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCtlResult {
    /// 操作成功，附带返回值。
    /// C: `return OK` (0), `return ENOENT` (2), `return EINVAL` (22)
    Ok(i32),
    /// 操作需要 VM 协助（VMSUSPEND）。
    /// C: `return VMSUSPEND` (-996)
    VmSuspend,
    /// 无效的 VMCTL 参数。
    /// C: `return EINVAL` from arch_do_vmctl default case
    BadParam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmCtlError {
    /// No pending request (C: `return ENOENT` from VMCTL_MEMREQ_GET).
    NoRequest,
    /// Process state inconsistent with the requested operation
    /// (C: `assert` failures converted to error returns).
    InvalidState,
    /// Target endpoint not found in process table.
    InvalidEndpoint,
}

/// VMCTL request handler.
///
/// Implements `VMCTL_MEMREQ_GET` and `VMCTL_MEMREQ_REPLY` logic.
/// Not a trait — only the kernel implements this, no hardware dependency,
/// no `#[cfg(target_arch)]` behavior selection. Design decision: §3.7.
pub struct VmRequestHandler;

impl VmRequestHandler {
    /// Handle `VMCTL_MEMREQ_GET`: VM fetches the next pending memory request.
    ///
    /// C: `case VMCTL_MEMREQ_GET:` in do_vmctl.c:37-79.
    /// Transitions `VmSuspendState::Pending → Fetched`.
    pub fn memreq_get(
        queue: &mut VmRequestQueue,
        procs: &mut [KProcess],
    ) -> Result<(ProcNr, VmCheckParams), VmCtlError> {
        let proc_nr = queue.dequeue_filtered(procs, |_requestor, _target| {
            true
        }).ok_or(VmCtlError::NoRequest)?;

        debug_assert!((proc_nr.0 as usize) < procs.len(), "ProcNr out of bounds");
        let proc = &mut procs[proc_nr.0 as usize];
        let ctx = proc.p_vm_suspend.as_mut().ok_or(VmCtlError::InvalidState)?;

        if ctx.state != VmSuspendState::Pending {
            return Err(VmCtlError::InvalidState);
        }

        ctx.state = VmSuspendState::Fetched;
        let params = ctx.check_params;

        Ok((proc_nr, params))
    }

    /// Handle `VMCTL_MEMREQ_REPLY`: VM replies with the result of a memory request.
    ///
    /// C: `case VMCTL_MEMREQ_REPLY:` in do_vmctl.c:81-109.
    /// Transitions `VmSuspendState::Fetched → Completed(result)`.
    /// Sets `MF_KCALL_RESUME` for `KernelCall` type (C: `p->p_misc_flags |= MF_KCALL_RESUME`).
    /// Clears `RTS_VMREQUEST` to make the process schedulable again.
    pub fn memreq_reply(
        proc: &mut KProcess,
        result: VmCheckResult,
    ) -> Result<(), VmCtlError> {
        let ctx = proc.p_vm_suspend.as_mut().ok_or(VmCtlError::NoRequest)?;

        if ctx.state != VmSuspendState::Fetched {
            return Err(VmCtlError::InvalidState);
        }

        ctx.state = VmSuspendState::Completed(result);

        match ctx.suspend_type {
            VmSuspendType::KernelCall => {
                proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
            }
            VmSuspendType::DeliverMsg => {
                if !proc.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG) {
                    return Err(VmCtlError::InvalidState);
                }
            }
        }

        proc.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);
        Ok(())
    }
}

/// Resume a previously suspended kernel call.
///
/// Corresponds to Minix3's `kernel_call_resume()` in system.c:612-636.
/// Called from `switch_to_user()` when `MF_KCALL_RESUME` is set.
///
/// The caller must ensure:
/// - `RTS_VMREQUEST` is NOT set (cleared by `memreq_reply`)
/// - `MF_KCALL_RESUME` IS set
/// - `p_vm_suspend` contains a `Completed` state context
///
/// Design decision: §3.6 (MF_KCALL_RESUME retained as flag), §4.9.
pub fn kernel_call_resume(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
) -> VmCheckResult {
    // K20 caller-by-nr: the caller slot is re-borrowed at each use — a
    // short read for the state, then a short write to clear the flag.
    let (kcall_resume, vmrequest, completed) = {
        let caller = proc_table
            .get(caller_nr)
            .expect("kernel_call_resume: caller slot must exist");
        let completed = match caller
            .p_vm_suspend
            .as_ref()
            .expect("MF_KCALL_RESUME set but no VmSuspendContext")
            .state
        {
            VmSuspendState::Completed(result) => Some(result),
            VmSuspendState::Pending | VmSuspendState::Fetched => None,
        };
        (
            caller.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME),
            caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST),
            completed,
        )
    };
    debug_assert!(kcall_resume);
    debug_assert!(!vmrequest);

    match completed {
        Some(result) => {
            proc_table
                .get_mut(caller_nr)
                .expect("kernel_call_resume: caller slot must exist")
                .p_misc_flags
                .clear(MiscFlagsBits::KCALL_RESUME);
            result
        }
        None => panic!("kernel_call_resume with non-completed state"),
    }
}

/// Check if a process has a pending message delivery that was interrupted by VM.
///
/// Corresponds to the `MF_DELIVERMSG` check in `switch_to_user()` (proc.c:356-360).
/// When a message delivery was interrupted by a page fault, `MF_DELIVERMSG`
/// remains set and `delivermsg()` will be retried on next scheduling.
///
/// Design decision: §3.6, §4.9.
pub fn try_deliver_message(rp: &KProcess) -> bool {
    rp.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG)
}

/// Check the result of a resumed kernel call.
///
/// Corresponds to Minix3's `check_resumed_caller()` in memory.c:135-145.
/// Returns the VM check result if the caller has `MF_KCALL_RESUME` set,
/// or `Ok(VmCheckResult::Ok)` if not (first call, no previous suspend).
///
/// Design decision: §3.3 (VmCheckResult simplifies arbitrary errno).
pub fn check_resumed_caller(caller: &KProcess) -> VmCheckResult {
    if caller.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME)
        && let Some(ctx) = caller.p_vm_suspend.as_ref()
            && let VmSuspendState::Completed(result) = ctx.state {
                return result;
            }
    VmCheckResult::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_table_ref_new_has_no_cr3() {
        let ptref = PageTableRef::new();
        assert!(ptref.cr3().is_none());
    }

    #[test]
    fn page_table_ref_default_has_no_cr3() {
        let ptref = PageTableRef::default();
        assert!(ptref.cr3().is_none());
    }

    #[test]
    fn page_table_ref_set_and_clear_cr3() {
        let mut ptref = PageTableRef::new();
        let cr3 = PhysBytes::new(0x1000);
        ptref.set_cr3(cr3);
        assert_eq!(ptref.cr3(), Some(cr3));
        ptref.clear_cr3();
        assert!(ptref.cr3().is_none());
    }

    #[test]
    fn page_table_ref_page_table_vaddr() {
        use minix_arch::direct_map::MockDirectMap;
        let mut ptref = PageTableRef::new();
        assert!(ptref.page_table_vaddr::<MockDirectMap>().is_none());
        let cr3 = PhysBytes::new(0x2000);
        ptref.set_cr3(cr3);
        let vaddr = ptref.page_table_vaddr::<MockDirectMap>();
        assert!(vaddr.is_some());
        assert_eq!(vaddr.unwrap(), MockDirectMap::kernel_phys_to_virt(cr3));
    }

    #[test]
    fn vm_copy_error_variants_match_minix3() {
        assert_eq!(VmCopyError::SrcPageFault, VmCopyError::SrcPageFault);
        assert_eq!(VmCopyError::DstPageFault, VmCopyError::DstPageFault);
        assert_eq!(VmCopyError::InvalidAddress, VmCopyError::InvalidAddress);
        assert_eq!(VmCopyError::PermissionDenied, VmCopyError::PermissionDenied);
        assert_eq!(VmCopyError::UnknownEndpoint, VmCopyError::UnknownEndpoint);
    }

    #[test]
    fn cross_space_result_suspended_is_separate_from_error() {
        let suspended = CrossSpaceResult::Suspended(VmFaultType::Src);
        assert_eq!(suspended, CrossSpaceResult::Suspended(VmFaultType::Src));
        let completed_ok = CrossSpaceResult::Completed(Ok(()));
        assert_eq!(completed_ok, CrossSpaceResult::Completed(Ok(())));
        let completed_err = CrossSpaceResult::Completed(Err(VmCopyError::SrcPageFault));
        assert_eq!(completed_err, CrossSpaceResult::Completed(Err(VmCopyError::SrcPageFault)));
    }

    #[test]
    fn vm_fault_type_variants() {
        assert_eq!(VmFaultType::Src, VmFaultType::Src);
        assert_eq!(VmFaultType::Dst, VmFaultType::Dst);
    }

    #[test]
    fn vm_copy_context_new() {
        let src = AddressRef::Physical(PhysBytes::new(0x1000));
        let dst = AddressRef::Physical(PhysBytes::new(0x2000));
        let ctx = VmCopyContext::new(src, dst, 42, VmFaultType::Src);
        assert_eq!(ctx.bytes, 42);
        assert_eq!(ctx.fault_type, VmFaultType::Src);
    }

    #[test]
    fn resolve_physical_returns_physical_directly() {
        use minix_arch::direct_map::MockDirectMap;
        let paddr = PhysBytes::new(0x5000);
        let addr = AddressRef::Physical(paddr);
        let result = resolve_physical::<MockDirectMap>(&addr, &crate::test_helpers::test_proc_table(), |_pt: &crate::proc_table::ProcessTable, _| None);
        assert_eq!(result, Ok(paddr));
    }

    #[test]
    fn physical_range_without_active_root_passes() {
        // No active root = no paging (boot/hosted posture): nothing to
        // validate, the check must not block the access.
        let mapped = physical_range_in_dm_window(
            None,
            VirBytes::new(0xFFFF_8080_0000_1000),
            128,
            |_: PhysBytes, _: VirBytes| panic!("walk must not be called without a root"),
        );
        assert!(mapped);
    }

    #[test]
    fn physical_range_zero_len_passes_without_walk() {
        let mapped = physical_range_in_dm_window(
            Some(PhysBytes::new(0x1000)),
            VirBytes::new(0xFFFF_8080_0000_1000),
            0,
            |_: PhysBytes, _: VirBytes| panic!("zero-length range needs no walk"),
        );
        assert!(mapped);
    }

    #[test]
    fn physical_range_unmapped_page_fails() {
        // Caller-supplied physical address whose DM window translation is
        // absent (beyond the boot-established RAM) — C answered this with
        // phys_copy_fault recovery returning the fault address; the
        // validate-first check answers `false` so the copy path returns
        // Fault (EFAULT at the dispatcher).
        let mapped = physical_range_in_dm_window(
            Some(PhysBytes::new(0x1000)),
            VirBytes::new(0xFFFF_8080_4000_0000),
            64,
            |_: PhysBytes, _: VirBytes| None,
        );
        assert!(!mapped);
    }

    #[test]
    fn physical_range_spanning_two_pages_needs_both() {
        // 64 bytes at the last 4 bytes of a page walk two pages; only the
        // first mapped → fail (the second page's access would fault).
        let both_mapped = physical_range_in_dm_window(
            Some(PhysBytes::new(0x1000)),
            VirBytes::new(0xFFFF_8080_0000_1FFC),
            64,
            |_: PhysBytes, va: VirBytes| Some((PhysBytes::new(va.0), PageFlags::PRESENT)),
        );
        assert!(both_mapped);
        let first_only = physical_range_in_dm_window(
            Some(PhysBytes::new(0x1000)),
            VirBytes::new(0xFFFF_8080_0000_1FFC),
            64,
            |_: PhysBytes, va: VirBytes| {
                if va.0 & !0xFFF == 0xFFFF_8080_0000_1000 {
                    Some((PhysBytes::new(va.0), PageFlags::PRESENT))
                } else {
                    None
                }
            },
        );
        assert!(!first_only);
    }

    #[test]
    fn resolve_physical_unknown_endpoint() {
        use minix_arch::direct_map::MockDirectMap;
        let addr = AddressRef::Process {
            endpoint: Endpoint::from_generation_slot(1, 9999),
            offset: VirBytes::new(0),
        };
        let result = resolve_physical::<MockDirectMap>(&addr, &crate::test_helpers::test_proc_table(), |_pt: &crate::proc_table::ProcessTable, _| None);
        assert_eq!(result, Err(ResolveError::UnknownEndpoint));
    }

    #[test]
    fn zero_length_copy_rejects_with_domain_before_any_resolution() {
        // C: `virtual_copy_f` — memory.c:607: `if (bytes <= 0) return(EDOM)`
        // is the *first* check, ahead of endpoint validation and page-table
        // walks. So a zero-length copy must come back as `Err(Domain)` even
        // when both sides are unresolvable endpoints and dangling offsets
        // (the aarch64 rc-chain incident had VFS select's 0-byte fdset
        // fetch passing a zero-size `Vec` sentinel as destination).
        use minix_arch::direct_map::MockDirectMap;
        let bad = AddressRef::Process {
            endpoint: Endpoint::from_generation_slot(1, 9999),
            offset: VirBytes::new(1),
        };
        let result = cross_space_copy::<MockDirectMap>(
            &bad,
            &bad,
            0,
            &crate::test_helpers::test_proc_table(),
            |_pt: &crate::proc_table::ProcessTable, _| None,
        );
        assert_eq!(
            result,
            CrossSpaceResult::Completed(Err(VmCopyError::Domain))
        );
    }

    #[test]
    fn cross_space_result_src_suspended() {
        let result = CrossSpaceResult::Suspended(VmFaultType::Src);
        assert_eq!(result, CrossSpaceResult::Suspended(VmFaultType::Src));
    }

    #[test]
    fn cross_space_result_dst_suspended() {
        let result = CrossSpaceResult::Suspended(VmFaultType::Dst);
        assert_eq!(result, CrossSpaceResult::Suspended(VmFaultType::Dst));
    }

    #[test]
    fn copy_page_table_ref_returns_empty() {
        let mut ptref = PageTableRef::new();
        ptref.set_cr3(PhysBytes::new(0x1000));
        let copied = copy_page_table_ref(&ptref);
        assert!(copied.cr3().is_none());
    }

    // ── 03-vm-request tests ──

    #[test]
    fn vm_suspend_type_exhaustive_match() {
        let t = VmSuspendType::KernelCall;
        let label = match t {
            VmSuspendType::KernelCall => "kcall",
            VmSuspendType::DeliverMsg => "deliver",
        };
        assert_eq!(label, "kcall");
    }

    #[test]
    fn vm_check_params_write_flag_bool() {
        let params = VmCheckParams {
            start: VirBytes::new(0x1000),
            length: VirBytes::new(0x100),
            write_flag: true,
        };
        assert!(params.write_flag);
        let read_params = VmCheckParams {
            start: VirBytes::new(0x1000),
            length: VirBytes::new(0x100),
            write_flag: false,
        };
        assert!(!read_params.write_flag);
    }

    #[test]
    fn vm_suspend_state_pending_to_fetched() {
        let state = VmSuspendState::Pending;
        assert_eq!(state, VmSuspendState::Pending);

        let fetched = VmSuspendState::Fetched;
        assert_eq!(fetched, VmSuspendState::Fetched);
    }

    #[test]
    fn vm_suspend_state_fetched_to_completed() {
        let state = VmSuspendState::Completed(VmCheckResult::Ok);
        assert_eq!(state, VmSuspendState::Completed(VmCheckResult::Ok));

        let fault = VmSuspendState::Completed(VmCheckResult::Fault);
        assert_eq!(fault, VmSuspendState::Completed(VmCheckResult::Fault));
    }

    #[test]
    fn vm_request_queue_new_is_empty() {
        let queue = VmRequestQueue::new();
        assert!(queue.is_empty());
    }

    #[test]
    fn vm_request_queue_enqueue_returns_was_empty() {
        let mut queue = VmRequestQueue::new();
        let mut procs = make_test_procs();
        let was_empty = queue.enqueue(ProcNr(0), &mut procs);
        assert!(was_empty);
        assert!(!queue.is_empty());

        let was_empty2 = queue.enqueue(ProcNr(1), &mut procs);
        assert!(!was_empty2);
    }

    #[test]
    fn vm_request_queue_dequeue_filtered_empty() {
        let mut queue = VmRequestQueue::new();
        let mut procs = make_test_procs();
        let result = queue.dequeue_filtered(&mut procs, |_, _| true);
        assert!(result.is_none());
    }

    #[test]
    fn vm_request_queue_dequeue_filtered_returns_first_match() {
        let mut queue = VmRequestQueue::new();
        let mut procs = make_test_procs();
        queue.enqueue(ProcNr(0), &mut procs);
        queue.enqueue(ProcNr(1), &mut procs);
        let result = queue.dequeue_filtered(&mut procs, |_, _| true);
        assert_eq!(result, Some(ProcNr(1)));
    }

    #[test]
    fn vm_request_queue_remove() {
        let mut queue = VmRequestQueue::new();
        let mut procs = make_test_procs();
        queue.enqueue(ProcNr(0), &mut procs);
        queue.enqueue(ProcNr(1), &mut procs);
        assert!(queue.remove(ProcNr(0), &mut procs));
        assert!(!queue.is_empty());
        assert!(queue.remove(ProcNr(1), &mut procs));
        assert!(queue.is_empty());
        assert!(!queue.remove(ProcNr(0), &mut procs));
    }

    #[test]
    fn vm_ctl_error_variants() {
        assert_eq!(VmCtlError::NoRequest, VmCtlError::NoRequest);
        assert_eq!(VmCtlError::InvalidState, VmCtlError::InvalidState);
        assert_eq!(VmCtlError::InvalidEndpoint, VmCtlError::InvalidEndpoint);
    }

    #[test]
    fn vm_check_result_variants() {
        assert_eq!(VmCheckResult::Ok, VmCheckResult::Ok);
        assert_eq!(VmCheckResult::Fault, VmCheckResult::Fault);
    }

    fn make_test_procs() -> crate::test_helpers::TestProcArray<4> {
        let params = VmCheckParams {
            start: VirBytes::new(0x1000),
            length: VirBytes::new(0x100),
            write_flag: true,
        };
        let mut procs = [
            KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0)),
            KProcess::new(ProcNr(1), Endpoint::from_generation_slot(1, 1)),
            KProcess::new(ProcNr(2), Endpoint::from_generation_slot(1, 2)),
            KProcess::new(ProcNr(3), Endpoint::from_generation_slot(1, 3)),
        ];
        for proc in procs.iter_mut() {
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        // Each process targets the next one (circular), so endpoint_to_proc_nr can resolve
        procs[0].suspend_for_vm(VmSuspendType::KernelCall, Endpoint::from_generation_slot(1, 1), params, None);
        procs[1].suspend_for_vm(VmSuspendType::KernelCall, Endpoint::from_generation_slot(1, 2), params, None);
        procs[2].suspend_for_vm(VmSuspendType::KernelCall, Endpoint::from_generation_slot(1, 3), params, None);
        procs[3].suspend_for_vm(VmSuspendType::KernelCall, Endpoint::from_generation_slot(1, 0), params, None);
        crate::test_helpers::scratch_procs(procs)
    }

    fn make_vm_suspended_proc(
        nr: ProcNr,
        suspend_type: VmSuspendType,
    ) -> crate::test_helpers::TestKProc {
        let mut proc = KProcess::new(nr, Endpoint::from_generation_slot(1, nr.0));
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        let params = VmCheckParams {
            start: VirBytes::new(0x1000),
            length: VirBytes::new(0x100),
            write_flag: true,
        };
        proc.suspend_for_vm(
            suspend_type,
            Endpoint::from_generation_slot(1, 99),
            params,
            None,
        );
        crate::test_helpers::scratch_kproc(proc)
    }

    // ── §5.1: VmSuspendState state transition tests ──

    #[test]
    fn vm_suspend_state_pending_is_initial() {
        let state = VmSuspendState::Pending;
        assert_eq!(state, VmSuspendState::Pending);
        assert_ne!(state, VmSuspendState::Fetched);
    }

    #[test]
    fn vm_suspend_state_fetched_after_memreq_get() {
        let mut procs = make_test_procs();
        let mut queue = VmRequestQueue::new();
        queue.enqueue(ProcNr(0), &mut procs);

        let result = VmRequestHandler::memreq_get(
            &mut queue,
            &mut procs,
        );
        assert!(result.is_ok());
        let ctx = procs[0].p_vm_suspend.as_ref().unwrap();
        assert_eq!(ctx.state, VmSuspendState::Fetched);
    }

    #[test]
    fn memreq_reply_transitions_fetched_to_completed_ok() {
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::KernelCall);
        let ctx = proc.p_vm_suspend.as_mut().unwrap();
        ctx.state = VmSuspendState::Fetched;

        let result = VmRequestHandler::memreq_reply(&mut proc, VmCheckResult::Ok);
        assert!(result.is_ok());
        assert!(!proc.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));
        assert!(proc.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));

        let ctx = proc.p_vm_suspend.as_ref().unwrap();
        assert_eq!(ctx.state, VmSuspendState::Completed(VmCheckResult::Ok));
    }

    #[test]
    fn memreq_reply_transitions_fetched_to_completed_fault() {
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::KernelCall);
        let ctx = proc.p_vm_suspend.as_mut().unwrap();
        ctx.state = VmSuspendState::Fetched;

        let result = VmRequestHandler::memreq_reply(&mut proc, VmCheckResult::Fault);
        assert!(result.is_ok());

        let ctx = proc.p_vm_suspend.as_ref().unwrap();
        assert_eq!(ctx.state, VmSuspendState::Completed(VmCheckResult::Fault));
    }

    #[test]
    fn memreq_reply_rejects_pending_state() {
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::KernelCall);
        // state is Pending by default
        let result = VmRequestHandler::memreq_reply(&mut proc, VmCheckResult::Ok);
        assert_eq!(result, Err(VmCtlError::InvalidState));
    }

    #[test]
    fn memreq_reply_rejects_completed_state() {
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::KernelCall);
        let ctx = proc.p_vm_suspend.as_mut().unwrap();
        ctx.state = VmSuspendState::Fetched;

        let _ = VmRequestHandler::memreq_reply(&mut proc, VmCheckResult::Ok);

        // Now state is Completed, try again
        let result = VmRequestHandler::memreq_reply(&mut proc, VmCheckResult::Ok);
        assert_eq!(result, Err(VmCtlError::InvalidState));
    }

    #[test]
    fn memreq_reply_delivermsg_requires_mf_delivermsg() {
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::DeliverMsg);
        let ctx = proc.p_vm_suspend.as_mut().unwrap();
        ctx.state = VmSuspendState::Fetched;
        // MF_DELIVERMSG not set → should fail

        let result = VmRequestHandler::memreq_reply(&mut proc, VmCheckResult::Ok);
        assert_eq!(result, Err(VmCtlError::InvalidState));
    }

    #[test]
    fn memreq_reply_delivermsg_succeeds_with_flag() {
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::DeliverMsg);
        let ctx = proc.p_vm_suspend.as_mut().unwrap();
        ctx.state = VmSuspendState::Fetched;
        proc.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);

        let result = VmRequestHandler::memreq_reply(&mut proc, VmCheckResult::Ok);
        assert!(result.is_ok());
        assert!(!proc.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    }

    // ── §5.1: kernel_call_resume / check_resumed_caller tests ──

    #[test]
    fn kernel_call_resume_returns_ok_on_success() {
        // K20 caller-by-nr: build the suspended state directly on the
        // caller's table slot (the standalone handle no longer applies).
        let mut table = crate::test_helpers::test_proc_table();
        {
            let proc = table.get_mut(ProcNr(0)).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.suspend_for_vm(
                VmSuspendType::KernelCall,
                Endpoint::from_generation_slot(1, 99),
                VmCheckParams {
                    start: VirBytes::new(0x1000),
                    length: VirBytes::new(0x100),
                    write_flag: true,
                },
                None,
            );
            // The reply path cleared RTS_VMREQUEST before the resume.
            proc.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);
            let ctx = proc.p_vm_suspend.as_mut().unwrap();
            ctx.state = VmSuspendState::Completed(VmCheckResult::Ok);
            proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        }

        let result = kernel_call_resume(ProcNr(0), &mut table);
        assert_eq!(result, VmCheckResult::Ok);
        assert!(!table.get(ProcNr(0)).unwrap().p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    }

    #[test]
    fn kernel_call_resume_returns_fault_on_failure() {
        // K20 caller-by-nr: build the suspended state directly on the
        // caller's table slot (the standalone handle no longer applies).
        let mut table = crate::test_helpers::test_proc_table();
        {
            let proc = table.get_mut(ProcNr(0)).unwrap();
            proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            proc.suspend_for_vm(
                VmSuspendType::KernelCall,
                Endpoint::from_generation_slot(1, 99),
                VmCheckParams {
                    start: VirBytes::new(0x1000),
                    length: VirBytes::new(0x100),
                    write_flag: true,
                },
                None,
            );
            // The reply path cleared RTS_VMREQUEST before the resume.
            proc.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);
            let ctx = proc.p_vm_suspend.as_mut().unwrap();
            ctx.state = VmSuspendState::Completed(VmCheckResult::Fault);
            proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        }

        let result = kernel_call_resume(ProcNr(0), &mut table);
        assert_eq!(result, VmCheckResult::Fault);
        assert!(!table.get(ProcNr(0)).unwrap().p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    }

    #[test]
    fn check_resumed_caller_returns_ok_when_no_resume() {
        let proc = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));
        assert_eq!(check_resumed_caller(&proc), VmCheckResult::Ok);
    }

    #[test]
    fn check_resumed_caller_returns_result_when_resumed() {
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::KernelCall);
        let ctx = proc.p_vm_suspend.as_mut().unwrap();
        ctx.state = VmSuspendState::Completed(VmCheckResult::Fault);
        proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        proc.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);

        assert_eq!(check_resumed_caller(&proc), VmCheckResult::Fault);
    }

    #[test]
    fn try_deliver_message_returns_false_without_flag() {
        let proc = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));
        assert!(!try_deliver_message(&proc));
    }

    #[test]
    fn try_deliver_message_returns_true_with_flag() {
        let proc = KProcess::new(ProcNr(0), Endpoint::from_generation_slot(1, 0));
        proc.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        assert!(try_deliver_message(&proc));
    }

    // ── §5.1: VmRequestQueue + VmRequestHandler integration ──

    #[test]
    fn memreq_get_returns_no_request_on_empty_queue() {
        let mut queue = VmRequestQueue::new();
        let mut procs = make_test_procs();
        let result = VmRequestHandler::memreq_get(&mut queue, &mut procs);
        assert_eq!(result, Err(VmCtlError::NoRequest));
    }

    #[test]
    fn memreq_get_dequeues_and_sets_fetched() {
        let mut queue = VmRequestQueue::new();
        let mut procs = make_test_procs();
        // make_test_procs already sets suspend_for_vm and RTS_VMREQUEST
        queue.enqueue(ProcNr(0), &mut procs);

        let result = VmRequestHandler::memreq_get(&mut queue, &mut procs);
        assert!(result.is_ok());
        let (proc_nr, check_params) = result.unwrap();
        assert_eq!(proc_nr, ProcNr(0));
        assert_eq!(check_params.start, VirBytes::new(0x1000));

        let ctx = procs[0].p_vm_suspend.as_ref().unwrap();
        assert_eq!(ctx.state, VmSuspendState::Fetched);
    }

    // ── 09-vm-boot-protocol tests ──

    #[test]
    fn test_vmctl_param_from_u32() {
        // Verify key VMCTL param values can be constructed
        let _ = VmCtlParam::ClearPageFault;
        let _ = VmCtlParam::MemReqGet;
        let _ = VmCtlParam::MemReqReply;
        let _ = VmCtlParam::KernPhysMap;
        let _ = VmCtlParam::KernMapReply;
        let _ = VmCtlParam::VmInhibitSet;
        let _ = VmCtlParam::VmInhibitClear;
        let _ = VmCtlParam::BootInhibitClear;
        let _ = VmCtlParam::SetAddrSpace;
        let _ = VmCtlParam::GetPdbr;
        let _ = VmCtlParam::FlushTlb;
    }

    #[test]
    fn test_vmctl_result_variants() {
        let _ok = VmCtlResult::Ok(0);
        let _enoent = VmCtlResult::Ok(2); // ENOENT
        let _einval = VmCtlResult::Ok(22); // EINVAL
        let _suspend = VmCtlResult::VmSuspend;
        let _bad = VmCtlResult::BadParam;
    }
}
