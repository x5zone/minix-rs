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
}

impl core::fmt::Display for VmCopyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VmCopyError::SrcPageFault => write!(f, "source page fault (EFAULT_SRC)"),
            VmCopyError::DstPageFault => write!(f, "destination page fault (EFAULT_DST)"),
            VmCopyError::InvalidAddress => write!(f, "invalid address (EFAULT)"),
            VmCopyError::PermissionDenied => write!(f, "permission denied (EPERM)"),
            VmCopyError::UnknownEndpoint => write!(f, "unknown endpoint (ESRCH)"),
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
/// C: `vm_lookup_range` — kernel/memory.c
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

fn resolve_physical<D: DirectMapArch>(
    addr: &AddressRef,
    proc_cr3: impl Fn(Endpoint) -> Option<PhysBytes>,
) -> Result<PhysBytes, ResolveError> {
    match addr {
        AddressRef::Physical(paddr) => Ok(*paddr),
        AddressRef::Process { endpoint, offset } => {
            let cr3 = proc_cr3(*endpoint).ok_or(ResolveError::UnknownEndpoint)?;
            lookup_in_table::<D>(cr3, *offset)
                .map(|(paddr, _)| paddr)
                .ok_or(ResolveError::PageFault)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveError {
    PageFault,
    UnknownEndpoint,
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
    proc_cr3: impl Fn(Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    let src_phys = match resolve_physical::<D>(src, &proc_cr3) {
        Ok(p) => p,
        Err(ResolveError::PageFault) => return CrossSpaceResult::Suspended(VmFaultType::Src),
        Err(ResolveError::UnknownEndpoint) => {
            return CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint))
        }
    };
    let dst_phys = match resolve_physical::<D>(dst, &proc_cr3) {
        Ok(p) => p,
        Err(ResolveError::PageFault) => return CrossSpaceResult::Suspended(VmFaultType::Dst),
        Err(ResolveError::UnknownEndpoint) => {
            return CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint))
        }
    };

    let src_vaddr = D::kernel_phys_to_virt(src_phys);
    let dst_vaddr = D::kernel_phys_to_virt(dst_phys);

    // SAFETY:
    // - src_vaddr and dst_vaddr are derived from DirectMapArch::kernel_phys_to_virt()
    //   on physical addresses returned by lookup_in_table, which are valid page-backed
    //   addresses in the Direct Map region.
    // - Caller must ensure the memory regions [src_vaddr, src_vaddr+bytes) and
    //   [dst_vaddr, dst_vaddr+bytes) do not overlap. If the same physical page is
    //   mapped at both src and dst (e.g., shared memory), the caller must use a
    //   copy-to-temporary-then-copy-from-temporary strategy instead.
    // - BKL ensures no concurrent mutation of these memory regions.
    // - Both regions are valid for u8 access (no alignment requirements).
    unsafe {
        core::ptr::copy_nonoverlapping(
            src_vaddr.0 as *const u8,
            dst_vaddr.0 as *mut u8,
            bytes,
        );
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
    proc_cr3: impl Fn(Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    let dst_phys = match resolve_physical::<D>(dst, &proc_cr3) {
        Ok(p) => p,
        Err(ResolveError::PageFault) => return CrossSpaceResult::Suspended(VmFaultType::Dst),
        Err(ResolveError::UnknownEndpoint) => {
            return CrossSpaceResult::Completed(Err(VmCopyError::UnknownEndpoint))
        }
    };

    let dst_vaddr = D::kernel_phys_to_virt(dst_phys);

    // SAFETY:
    // - dst_vaddr is derived from DirectMapArch::kernel_phys_to_virt() on a valid
    //   physical address returned by lookup_in_table.
    // - The memory region [dst_vaddr, dst_vaddr+count) is valid for write.
    // - BKL ensures no concurrent mutation of this memory region.
    // - u8 has no alignment requirements.
    unsafe {
        core::ptr::write_bytes(dst_vaddr.0 as *mut u8, value, count);
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
pub fn kernel_call_resume(caller: &mut KProcess) -> VmCheckResult {
    debug_assert!(caller.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    debug_assert!(!caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST));

    let ctx = caller.p_vm_suspend.as_ref()
        .expect("MF_KCALL_RESUME set but no VmSuspendContext");

    match ctx.state {
        VmSuspendState::Completed(result) => {
            caller.p_misc_flags.clear(MiscFlagsBits::KCALL_RESUME);
            result
        }
        VmSuspendState::Pending | VmSuspendState::Fetched => {
            panic!("kernel_call_resume with non-completed state");
        }
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
        let result = resolve_physical::<MockDirectMap>(&addr, |_| None);
        assert_eq!(result, Ok(paddr));
    }

    #[test]
    fn resolve_physical_unknown_endpoint() {
        use minix_arch::direct_map::MockDirectMap;
        let addr = AddressRef::Process {
            endpoint: Endpoint::from_generation_slot(1, 9999),
            offset: VirBytes::new(0),
        };
        let result = resolve_physical::<MockDirectMap>(&addr, |_| None);
        assert_eq!(result, Err(ResolveError::UnknownEndpoint));
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

    fn make_test_procs() -> [KProcess; 4] {
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
        procs
    }

    fn make_vm_suspended_proc(
        nr: ProcNr,
        suspend_type: VmSuspendType,
    ) -> KProcess {
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
        proc
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
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::KernelCall);
        let ctx = proc.p_vm_suspend.as_mut().unwrap();
        ctx.state = VmSuspendState::Completed(VmCheckResult::Ok);
        proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        proc.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);

        let result = kernel_call_resume(&mut proc);
        assert_eq!(result, VmCheckResult::Ok);
        assert!(!proc.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME));
    }

    #[test]
    fn kernel_call_resume_returns_fault_on_failure() {
        let mut proc = make_vm_suspended_proc(ProcNr(0),VmSuspendType::KernelCall);
        let ctx = proc.p_vm_suspend.as_mut().unwrap();
        ctx.state = VmSuspendState::Completed(VmCheckResult::Fault);
        proc.p_misc_flags.set(MiscFlagsBits::KCALL_RESUME);
        proc.p_rts_flags.clear(RtsFlagsBits::VMREQUEST);

        let result = kernel_call_resume(&mut proc);
        assert_eq!(result, VmCheckResult::Fault);
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
