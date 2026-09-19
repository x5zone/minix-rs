//! Cross-address-space runtime: `data_copy_vmcheck` and integration with the
//! VMREQUEST suspend/resume mechanism.
//!
//! # Minix3 C Source Mapping
//!
//! - `sys_datacopy` is a **user-space library macro** (`syslib.h:129`) that
//!   expands to `sys_vircopy(p1, v1, p2, v2, len, 0)`. There is **no**
//!   `SYS_DATACOPY` kernel call number and **no** `do_datacopy.c` file in
//!   Minix3.
//! - The kernel-side handler for `SYS_VIRCOPY`/`SYS_PHYSCOPY` is `do_copy()`
//!   in `kernel/system/do_copy.c:22-90`.
//! - `data_copy_vmcheck()` is defined in `arch/i386/memory.c:690-705` and
//!   `arch/earm/memory.c:590+` (architecture-specific entry points that
//!   delegate to the shared `virtual_copy_vmcheck()` in `kernel/memory.c`).
//! - `vm_suspend()` lives in `kernel/proc.c:234-257`; the VMREQUEST handshake
//!   uses `VMCTL_MEMREQ_GET`/`VMCTL_MEMREQ_REPLY` (see 09-vm-boot-protocol.md).
//!
//! # Design Decisions (24-cross-space-runtime.md §3)
//!
//! - **D1**: Direct Map replaces C's temporary PDE mapping (`createpde`)
//! - **D2**: `CrossSpaceResult` enum distinguishes Completed(Ok/Err) from
//!   Suspended(VmFaultType) — C conflates these in a single `int`
//! - **D3**: `AddressRef` enum (Process/Physical) replaces C's `vir_addr`
//!   struct + sentinel `proc_nr_e` values
//! - **D4**: the caller is named explicitly (`caller_nr` + `&mut ProcessTable`,
//!   K20 caller-by-nr, uniform parameter order) — VMSUSPEND requires setting
//!   RTS_VMREQUEST on the caller, so the borrow must be mutable
//! - **D7**: `dispatch_datacopy` is **deleted** — Minix3 has no
//!   `SYS_DATACOPY` call number; `sys_datacopy` is a user-space macro that
//!   expands to `sys_vircopy(..., 0)`, already dispatched by
//!   `syscall_copy.rs::dispatch_vircopy`
//! - **D8**: `CopyResult` is removed — `CrossSpaceResult` (from `crate::vm`)
//!   is the single canonical type for cross-space copy outcomes
//!
//! # ARCHITECTURE NOTE
//!
//! `data_copy_vmcheck` is the kernel-internal helper used by SIGSEND,
//! GETINFO, DIAGCTL, etc. (see `misc.rs`, `syscall_signal.rs`,
//! `syscall_process.rs`). It is **not** a system-call dispatcher; the
//! `SYS_VIRCOPY`/`SYS_PHYSCOPY` system calls are handled by
//! `syscall_copy.rs::dispatch_vircopy`.

use minix_types::{Endpoint, PhysBytes, VirBytes};

use crate::proc::KProcess;
use crate::proc::ProcNr;
use crate::proc_table::ProcessTable;
use crate::vm::{AddressRef, CrossSpaceResult, VmCopyContext, VmFaultType, VmSuspendType, cross_space_copy, cross_space_memset, cross_space_write};
use minix_arch::CurrentDirectMap;

// ── Kernel-internal cross-process copy ──

/// Kernel-internal cross-process copy with VM check.
///
/// C: `data_copy_vmcheck(caller, from_proc, from_addr, to_proc, to_addr, bytes)`
///    — `arch/i386/memory.c:690-705`, `arch/earm/memory.c:590+`
///
/// This is the core function used by SIGSEND, GETINFO, DIAGCTL, and other
/// kernel-internal paths that need to copy data across address spaces. It
/// differs from `sys_vircopy` (the system call) in two ways:
///
/// 1. **No message parsing** — callers pass already-resolved `AddressRef`s
///    directly (C's `do_copy` parses `m_lsys_krn_sys_copy` first).
/// 2. **VMSUSPEND side effect** — on a page fault, this function **sets
///    `RTS_VMREQUEST` on `caller`** and stores a `VmCopyContext` in
///    `caller.p_vm_suspend` so that `kernel_call_resume()` can retry after
///    VM handles the fault. The C version returns `VMSUSPEND` (-996) and
///    the caller is responsible for invoking `vm_suspend()`; the Rust
///    version inlines this step to make the borrow explicit.
///
/// # Parameters
///
/// - `src` / `dst`: `AddressRef::Process` for virtual addresses (resolved
///   via PTE walk), `AddressRef::Physical` for physical addresses (used
///   directly, no walk needed — corresponds to C's `proc_nr_e == NONE`).
/// - `proc_cr3`: closure resolving `Endpoint → Option<PhysBytes>` (page-table
///   root). The dispatcher reads `caller.p_seg.phys_root` and
///   `proc_table.get(nr).p_seg.phys_root` before calling this function,
///   avoiding the borrow conflict between `&mut caller` and `&proc_table`.
///
/// # Direct Map implementation
///
/// The kernel's Direct Map window
/// (`PA=0..total_phys → KERNEL_DIRECT_MAP_BASE+PA`) lets us read/write any
/// physical page by adding a fixed offset. The copy proceeds in three steps:
///
/// 1. Walk the source process's page table to translate `src_addr` → `src_phys`.
///    Uses `CurrentPteWalk::walk` (trait-dispatched, no `#[cfg(target_arch)]`).
/// 2. Walk the destination process's page table to translate
///    `dst_addr` → `dst_phys`.
/// 3. Convert both physical addresses to kernel-virtual via
///    `DirectMapArch::kernel_phys_to_virt`, then `copy_nonoverlapping`
///    the bytes.
///
/// If either walk hits a non-present entry, the page may be lazy
/// (not yet faulted in). This function then:
/// - Constructs a `VmCopyContext` from the fault direction (`VmFaultType::Src`
///   or `VmFaultType::Dst`) and the original src/dst/bytes.
/// - Calls `caller.suspend_for_vm_with_copy(...)` to set `RTS_VMREQUEST`
///   and store the context (analogous to C's `vm_suspend()`).
/// - Returns `CrossSpaceResult::Suspended(fault_type)` so the caller knows
///   **not** to modify process registers (the operation will be retried
///   after VM replies).
///
/// # Anti-translate note
///
/// C takes `struct proc * caller` (raw pointer) and resolves `from_proc` /
/// `to_proc` to `struct proc *` internally via `isokendpt()` + `proc_addr()`.
/// Rust takes `AddressRef` values (endpoint + offset, or physical address)
/// plus a `proc_cr3` closure. Since K20 (caller-by-nr) the caller travels
/// as `caller_nr: ProcNr` (with `proc_table` in the uniform second slot)
/// and the suspend side effect re-borrows its slot at the point of use —
/// the closure captures only copied field values, never a table borrow.
///
/// # Critical invariant
///
/// If this returns `CrossSpaceResult::Suspended(_)`, the caller MUST NOT
/// modify process registers (e.g., in SIGSEND's sigframe setup), because
/// the process will be resumed and the copy retried with the original
/// register state.
pub fn data_copy_vmcheck(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    src: AddressRef,
    dst: AddressRef,
    bytes: usize,
    proc_cr3: impl Fn(&ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    let result = cross_space_copy::<CurrentDirectMap>(&src, &dst, bytes, proc_table, &proc_cr3);

    // On suspend, set RTS_VMREQUEST on the caller and store the copy
    // context so kernel_call_resume() can retry. This inlines C's
    // vm_suspend() call (proc.c:234-257) — the borrow checker enforces
    // that the mutable borrow is unique.
    if let CrossSpaceResult::Suspended(fault_type) = result {
        let copy_ctx = VmCopyContext::new(src, dst, bytes, fault_type);
        // VmCheckParams records the faulting range for VM's range check.
        // C: `p_vmrequest.params.check.{start, length, writeflag}`.
        // - `start` = the faulting side's virtual address (Src/Dst)
        // - `length` = bytes remaining (we use the full `bytes` here).
        //   A more precise value would subtract the already-copied
        //   portion, but `cross_space_copy` is **idempotent on retry**:
        //   re-copying already-copied bytes is safe (write-after-write
        //   to the same physical page produces the same content).
        //   WONTFIX partial-progress tracking — the extra VM fault
        //   handling cost is acceptable for typical small kernel-internal
        //   copies (sigframe, getinfo struct, diagctl buffer).
        // - `write_flag` = true for Dst (write fault), false for Src (read)
        //
        // Physical addresses cannot fault (they are pre-resolved), so
        // `as_process()` is guaranteed to return `Some` here.
        let (target, start, write_flag) = match fault_type {
            VmFaultType::Src => {
                let (endpt, offset) = src.as_process()
                    .expect("Physical address cannot produce Src page fault");
                (endpt, offset, false)
            }
            VmFaultType::Dst => {
                let (endpt, offset) = dst.as_process()
                    .expect("Physical address cannot produce Dst page fault");
                (endpt, offset, true)
            }
        };
        let check_params = crate::vm::VmCheckParams {
            start,
            length: VirBytes(bytes as u64),
            write_flag,
        };
        // saved_msg is None: data_copy_vmcheck is a kernel-internal helper,
        // not a kernel-call dispatcher. The dispatcher layer
        // (dispatch_vircopy) is responsible for saving the request message
        // before calling this function if resumption requires it.
        proc_table
            .get_mut(caller_nr)
            .expect("data_copy_vmcheck: caller slot must exist")
            .suspend_for_vm_with_copy(
                VmSuspendType::KernelCall,
                target,
                check_params,
                None,
                copy_ctx,
            );
    }

    result
}

/// Kernel-local → process copy with VM check.
///
/// One-sided companion of [`data_copy_vmcheck`] for kernel-produced data
/// (C: `sys_datacopy(SELF, &local, caller, ...)` — the `do_getinfo.c:209-217`
/// common tail). The source is kernel memory read directly by virtual
/// address; only the destination is PTE-resolved, so only a destination
/// fault can suspend. On `Suspended(Dst)` this sets `RTS_VMREQUEST` on the
/// caller and stores the copy context, mirroring the destination arm of
/// [`data_copy_vmcheck`].
pub fn write_to_process_vmcheck(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    src: &[u8],
    dst: AddressRef,
    proc_cr3: impl Fn(&ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    let result = cross_space_write::<CurrentDirectMap>(src, &dst, proc_table, &proc_cr3);

    if let CrossSpaceResult::Suspended(fault_type) = result {
        // Kernel-local sources cannot fault — only the destination can.
        debug_assert!(matches!(fault_type, VmFaultType::Dst));
        let (target, start) = dst
            .as_process()
            .expect("kernel-local source cannot fault; dst must be a process address");
        let check_params = crate::vm::VmCheckParams {
            start,
            length: VirBytes(src.len() as u64),
            write_flag: true,
        };
        // The source is kernel-local (not an AddressRef) — the context's
        // src half is unused on retry; the resume path re-dispatches the
        // kernel call from the caller's saved message.
        let copy_ctx = crate::vm::VmCopyContext::new(
            AddressRef::Physical(PhysBytes(0)),
            dst,
            src.len(),
            fault_type,
        );
        proc_table
            .get_mut(caller_nr)
            .expect("data_copy_vmcheck: caller slot must exist")
            .suspend_for_vm_with_copy(
                VmSuspendType::KernelCall,
                target,
                check_params,
                None,
                copy_ctx,
            );
    }

    result
}

/// Kernel-internal cross-process memset with VM check.
///
/// C: `vm_memset(caller, proc_nr, addr, pattern, count)`
///    — `arch/i386/memory.c:526-577`
///
/// Fills `count` bytes in the destination address space with `value`.
/// On page fault, sets `RTS_VMREQUEST` on `caller` (without a copy context,
/// since memset has no source) and returns `CrossSpaceResult::Suspended`.
///
/// Mirrors `data_copy_vmcheck` but for the memset path — the VMSUSPEND
/// side effect uses `suspend_for_vm` (no `VmCopyContext`) because memset
/// is a one-sided operation (no source to resume).
pub fn memset_vmcheck(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    dst: AddressRef,
    value: u8,
    count: usize,
    proc_cr3: impl Fn(&ProcessTable, Endpoint) -> Option<PhysBytes>,
) -> CrossSpaceResult {
    let result = cross_space_memset::<CurrentDirectMap>(&dst, value, count, proc_table, &proc_cr3);

    if let CrossSpaceResult::Suspended(VmFaultType::Dst) = result {
        // Physical addresses cannot fault — only Process addresses can.
        let (target, start) = dst.as_process()
            .expect("Physical address cannot produce Dst page fault");
        let check_params = crate::vm::VmCheckParams {
            start,
            length: VirBytes(count as u64),
            write_flag: true,
        };
        // No VmCopyContext for memset — it's a one-sided operation.
        // kernel_call_resume() will re-dispatch SYS_MEMSET, which calls
        // this function again with the original message parameters.
        proc_table
            .get_mut(caller_nr)
            .expect("memset_vmcheck: caller slot must exist")
            .suspend_for_vm(
                VmSuspendType::KernelCall,
                target,
                check_params,
                None,
            );
    }

    result
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::ProcNr;
    use crate::syscall::KcallResult;

    /// Sanity: CrossSpaceResult variants are distinct.
    /// This replaces the pre-rewrite `test_copy_result_variants` which
    /// tested the now-deleted `CopyResult` enum.
    #[test]
    fn test_cross_space_result_variants() {
        let ok = CrossSpaceResult::Completed(Ok(()));
        let fault = CrossSpaceResult::Completed(Err(crate::vm::VmCopyError::InvalidAddress));
        let suspend_src = CrossSpaceResult::Suspended(VmFaultType::Src);
        let suspend_dst = CrossSpaceResult::Suspended(VmFaultType::Dst);
        assert_ne!(ok, fault);
        assert_ne!(suspend_src, suspend_dst);
        assert_ne!(ok, suspend_src);
    }

    /// L1 parity: CrossSpaceResult::Completed(Ok) corresponds to C's
    /// `return OK` from `virtual_copy_vmcheck()` (memory.c:507-535).
    #[test]
    fn test_completed_ok_matches_c_ok() {
        let r = CrossSpaceResult::Completed(Ok(()));
        assert!(matches!(r, CrossSpaceResult::Completed(Ok(()))));
    }

    /// L1 parity: CrossSpaceResult::Suspended(VmFaultType::Src)
    /// corresponds to C's `return VMSUSPEND` with `fault_type = EFAULT_SRC`
    /// (memory.c, virtual_copy_f path).
    #[test]
    fn test_suspended_src_matches_c_vmsuspend_src() {
        let r = CrossSpaceResult::Suspended(VmFaultType::Src);
        assert!(matches!(r, CrossSpaceResult::Suspended(VmFaultType::Src)));
    }

    /// L1 parity: CrossSpaceResult::Suspended(VmFaultType::Dst)
    /// corresponds to C's `return VMSUSPEND` with `fault_type = EFAULT_DST`.
    #[test]
    fn test_suspended_dst_matches_c_vmsuspend_dst() {
        let r = CrossSpaceResult::Suspended(VmFaultType::Dst);
        assert!(matches!(r, CrossSpaceResult::Suspended(VmFaultType::Dst)));
    }

    /// L2 contract: CrossSpaceResult is exhaustive — `match` must cover
    /// all three variants. This test will fail to compile if a new variant
    /// is added without updating callers.
    #[test]
    fn test_cross_space_result_match_exhaustive() {
        fn classify(r: &CrossSpaceResult) -> &'static str {
            match r {
                CrossSpaceResult::Completed(Ok(())) => "ok",
                CrossSpaceResult::Completed(Err(_)) => "fault",
                CrossSpaceResult::Suspended(VmFaultType::Src) => "suspend-src",
                CrossSpaceResult::Suspended(VmFaultType::Dst) => "suspend-dst",
            }
        }
        assert_eq!(classify(&CrossSpaceResult::Completed(Ok(()))), "ok");
        assert_eq!(
            classify(&CrossSpaceResult::Completed(Err(crate::vm::VmCopyError::InvalidAddress))),
            "fault"
        );
        assert_eq!(
            classify(&CrossSpaceResult::Suspended(VmFaultType::Src)),
            "suspend-src"
        );
    }

    /// VmCopyContext construction preserves src/dst/bytes/fault_type.
    /// L1 parity with C's `p_vmrequest.params.check.{start,length,writeflag}`.
    #[test]
    fn test_vm_copy_context_construction() {
        let src = AddressRef::Physical(minix_types::PhysBytes(0x1000));
        let dst = AddressRef::Physical(minix_types::PhysBytes(0x2000));
        let ctx = VmCopyContext::new(src, dst, 4096, VmFaultType::Src);
        assert_eq!(ctx.bytes, 4096);
        assert_eq!(ctx.fault_type, VmFaultType::Src);
    }

    /// Suppress unused-import warning for KcallResult in test context.
    /// (Pre-rewrite tests referenced KcallResult via CopyResult::to_kcall_result;
    /// post-rewrite, the conversion happens at the dispatcher layer.)
    #[test]
    fn _ensure_kcall_result_import_used() {
        let _: Option<KcallResult> = None;
    }

    // ── T-7: data_copy_vmcheck 行为矩阵（doc 24 §5.2 计划）──

    /// L1 奇偶：C virtual_copy_vmcheck（memory.c:507-535）三态——OK /
    /// EFAULT / VMSUSPEND。Rust 对应 Completed(Ok) / Completed(Err) /
    /// Suspended。宿主可安全触发：零字节物理→物理 = Ok；closure 返 None =
    /// UnknownEndpoint(Err)；未映射用户地址 = Suspended。
    /// caller-by-nr 夹具（K20）：调用者占表槽 `ProcNr(0)`。
    fn caller_table() -> crate::test_helpers::TestProcTable {
        crate::test_helpers::test_proc_table()
    }

    #[test]
    fn test_data_copy_vmcheck_parity_with_c() {
        let mut table = caller_table();

        // OK 态：零字节物理→物理（无解析、无内存访问）。
        let ok = data_copy_vmcheck(
            ProcNr(0),
            &mut table,
            AddressRef::Physical(PhysBytes(0x2000)),
            AddressRef::Physical(PhysBytes(0x3000)),
            0,
            |_pt: &crate::proc_table::ProcessTable, _| Some(PhysBytes(0)),
        );
        assert_eq!(ok, CrossSpaceResult::Completed(Ok(())));

        // EFAULT 态：endpoint 无 cr3 → Completed(Err(UnknownEndpoint))。
        let fault = data_copy_vmcheck(
            ProcNr(0),
            &mut table,
            AddressRef::Process { endpoint: Endpoint(999), offset: VirBytes(0) },
            AddressRef::Physical(PhysBytes(0)),
            4,
            |_pt: &crate::proc_table::ProcessTable, _| None,
        );
        assert!(matches!(fault, CrossSpaceResult::Completed(Err(_))));

        // VMSUSPEND 态：未映射用户目标 → Suspended(Dst)。
        let suspend = data_copy_vmcheck(
            ProcNr(0),
            &mut table,
            AddressRef::Physical(PhysBytes(0)),
            AddressRef::Process { endpoint: Endpoint(100), offset: VirBytes(0x1000) },
            4,
            |_pt: &crate::proc_table::ProcessTable, _| Some(PhysBytes(0)),
        );
        assert!(matches!(suspend, CrossSpaceResult::Suspended(_)));
    }

    /// VMSUSPEND 副作用：caller.p_rts_flags 置 RTS_VMREQUEST 且
    /// p_vm_suspend 就绪（C vm_suspend() proc.c:234-257 内联）。
    #[test]
    fn test_data_copy_vmcheck_sets_rts_vmrequest() {
        use crate::proc::RtsFlagsBits;
        let mut table = caller_table();
        let r = data_copy_vmcheck(
            ProcNr(0),
            &mut table,
            AddressRef::Physical(PhysBytes(0)),
            AddressRef::Process { endpoint: Endpoint(100), offset: VirBytes(0x1000) },
            4,
            |_pt: &crate::proc_table::ProcessTable, _| Some(PhysBytes(0)),
        );
        assert!(matches!(r, CrossSpaceResult::Suspended(_)));
        let caller = table.get(ProcNr(0)).unwrap();
        assert!(caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST),
            "挂起必须置 RTS_VMREQUEST");
        assert!(caller.p_vm_suspend.is_some(), "挂起必须保存 VmSuspendContext");
    }

    /// VMSUSPEND 后 copy_context 保全：VmCopyContext(src, dst, bytes,
    /// fault_type) + VmCheckParams{start, length, write_flag} 与故障面一致
    /// （kernel_call_resume 重试依据）。
    #[test]
    fn test_data_copy_vmcheck_preserves_copy_context() {
        use crate::proc::RtsFlagsBits;
        let mut table = caller_table();
        let r = data_copy_vmcheck(
            ProcNr(0),
            &mut table,
            AddressRef::Physical(PhysBytes(0)),
            AddressRef::Process { endpoint: Endpoint(100), offset: VirBytes(0x1000) },
            4,
            |_pt: &crate::proc_table::ProcessTable, _| Some(PhysBytes(0)),
        );
        assert!(matches!(r, CrossSpaceResult::Suspended(VmFaultType::Dst)));
        let caller = table.get(ProcNr(0)).unwrap();
        let ctx = caller.p_vm_suspend.as_ref()
            .and_then(|s| s.copy_context.as_ref())
            .expect("挂起必须携带 copy_context");
        assert_eq!(ctx.bytes, 4);
        assert!(matches!(ctx.fault_type, VmFaultType::Dst));
        let params = &caller.p_vm_suspend.as_ref().unwrap().check_params;
        assert_eq!(params.start.0, 0x1000, "check_params.start = 故障侧虚拟地址");
        assert_eq!(params.length.0, 4);
        assert!(params.write_flag, "Dst 故障 → write_flag = true");
        let _ = RtsFlagsBits::VMREQUEST; // 引用避免未用告警（本测试聚焦上下文）
    }

    /// 边界：零字节拷贝返回 Completed(Ok)。注意与 C 的差异——解析先于
    /// 字节数检查，故零字节仅在物理地址（免解析）上有确定语义。
    #[test]
    fn test_zero_byte_copy_returns_ok() {
        let mut table = caller_table();
        let r = data_copy_vmcheck(
            ProcNr(0),
            &mut table,
            AddressRef::Physical(PhysBytes(0x2000)),
            AddressRef::Physical(PhysBytes(0x3000)),
            0,
            |_pt: &crate::proc_table::ProcessTable, _| None,
        );
        assert_eq!(r, CrossSpaceResult::Completed(Ok(())));
    }
}

