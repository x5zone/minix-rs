//! Kernel system call dispatch — enum + match replaces C's call_vec[].
//!
//! # Minix3 C Source Mapping
//!
//! - `system.c:168-278` — system_init(): IRQ hook init + alarm timer init + call_vec registration
//! - `system.c:103-116` — kernel_call_dispatch(): call_vec[call_nr] dispatch
//! - `system.c:58-90` — kernel_call_finish(): VMSUSPEND handling + result copy
//! - `com.h:207-267` — SYS_* constant definitions
//! - `com.h:270` — NR_SYS_CALLS = 58
//!
//! # Design Decisions (08-system-init-boot-finish.md §3)
//!
//! - **D1**: `enum Syscall + match` replaces C's `call_vec[]` function pointer array.
//!   Benefits: type safety, compile-time exhaustiveness check, no function pointers.
//! - **D2**: `const assert` replaces C's `map()` macro runtime assert.
//! - **D6**: Architecture-specific syscalls dispatch via `ArchSyscall` trait
//!   with default `BadCall` implementations. Each arch overrides only the
//!   methods it supports (ZST impls). `CurrentArchSyscall` type alias selects
//!   the implementation via a single `#[cfg(target_arch)]` (one location),
//!   avoiding scattered `#[cfg]` behavior selection in the dispatch match.

use crate::proc::{KProcess, ProcNr};
use crate::kpriv::PrivTable;
use crate::ipc_filter::kcall_filter_check;
use crate::clock::ClockState;
use crate::proc_table::ProcessTable;
use minix_types::Message;

/// Total number of kernel system calls.
/// C: NR_SYS_CALLS = 58 — minix/com.h:270
pub const NR_SYS_CALLS: usize = 58;

// ── Minix3 error codes used in dispatch ──
// Centralized in `crate::errno` to prevent value drift (FIX-01: R-02/R-09/R-18).
use crate::errno::*;

/// Kernel system call number.
///
/// C: `SYS_*` constants in minix/com.h:206-266
///
/// Design decision D1: enum + match replaces C's call_vec[] function pointer array.
/// Design decision D6: architecture-specific syscalls are included in the enum
/// for all platforms; unsupported ones return BadCall via ArchSyscall trait
/// default methods (dispatched through `CurrentArchSyscall`).
///
/// # Reserved/Unused call numbers (WONTFIX)
///
/// Numbers 11, 12, 20, 29, 30, 37, 38, 41, 42, 47, 48, 49 are **not defined**
/// in Minix3 C source (`com.h` has no `SYS_*` macro for them) and have no
/// entry in C's `call_vec[]` (system.c:188-189 initializes all slots to NULL;
/// only 46 slots are mapped via `map()`). These are permanent gaps in the
/// call vector — not stubs to be implemented.
///
/// **Behavior**: `Syscall::try_from(value)` returns `Err(())` for these
/// numbers → `kernel_call_dispatch_inner` returns `KcallResult::BadCall` →
/// caller receives `EBADREQUEST` (212). This matches C's behavior:
/// `call_vec[call_nr] == NULL → result = EBADREQUEST` (system.c:119-123).
///
/// **WONTFIX rationale**: These numbers were never assigned to any syscall
/// in any version of Minix3. The checklist.md previously listed fabricated
/// names (SYS_KERNINFO, SYS_GETEP, SYS_INT86, etc.) for some of them —
/// those names do not exist in the C source. No implementation is planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Syscall {
    Fork = 0,
    Exec = 1,
    Clear = 2,
    Schedule = 3,
    Privctl = 4,
    Trace = 5,
    Kill = 6,
    Getksig = 7,
    Endksig = 8,
    Sigsend = 9,
    Sigreturn = 10,
    // 11-12: WONTFIX — reserved/unused (no SYS_* in com.h, no map() in system.c)
    Memset = 13,
    Umap = 14,
    Vircopy = 15,
    Physcopy = 16,
    UmapRemote = 17,
    Vumap = 18,
    Irqctl = 19,
    // 20: WONTFIX — reserved/unused
    Devio = 21,
    Sdevio = 22,
    Vdevio = 23,
    Setalarm = 24,
    Times = 25,
    Getinfo = 26,
    Abort = 27,
    Iopenable = 28,
    // 29-30: WONTFIX — reserved/unused
    SafecopyFrom = 31,
    SafecopyTo = 32,
    Vsafecopy = 33,
    Setgrant = 34,
    Readbios = 35,
    Sprof = 36,
    // 37-38: WONTFIX — reserved/unused
    Stime = 39,
    Settime = 40,
    // 41-42: WONTFIX — reserved/unused
    Vmctl = 43,
    Diagctl = 44,
    Vtimer = 45,
    Runctl = 46,
    // 47-49: WONTFIX — reserved/unused
    Getmcontext = 50,
    Setmcontext = 51,
    Update = 52,
    Exit = 53,
    Schedctl = 54,
    Statectl = 55,
    Safememset = 56,
    Padconf = 57,
}

impl TryFrom<u16> for Syscall {
    type Error = ();

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Syscall::Fork),
            1 => Ok(Syscall::Exec),
            2 => Ok(Syscall::Clear),
            3 => Ok(Syscall::Schedule),
            4 => Ok(Syscall::Privctl),
            5 => Ok(Syscall::Trace),
            6 => Ok(Syscall::Kill),
            7 => Ok(Syscall::Getksig),
            8 => Ok(Syscall::Endksig),
            9 => Ok(Syscall::Sigsend),
            10 => Ok(Syscall::Sigreturn),
            13 => Ok(Syscall::Memset),
            14 => Ok(Syscall::Umap),
            15 => Ok(Syscall::Vircopy),
            16 => Ok(Syscall::Physcopy),
            17 => Ok(Syscall::UmapRemote),
            18 => Ok(Syscall::Vumap),
            19 => Ok(Syscall::Irqctl),
            21 => Ok(Syscall::Devio),
            22 => Ok(Syscall::Sdevio),
            23 => Ok(Syscall::Vdevio),
            24 => Ok(Syscall::Setalarm),
            25 => Ok(Syscall::Times),
            26 => Ok(Syscall::Getinfo),
            27 => Ok(Syscall::Abort),
            28 => Ok(Syscall::Iopenable),
            31 => Ok(Syscall::SafecopyFrom),
            32 => Ok(Syscall::SafecopyTo),
            33 => Ok(Syscall::Vsafecopy),
            34 => Ok(Syscall::Setgrant),
            35 => Ok(Syscall::Readbios),
            36 => Ok(Syscall::Sprof),
            39 => Ok(Syscall::Stime),
            40 => Ok(Syscall::Settime),
            43 => Ok(Syscall::Vmctl),
            44 => Ok(Syscall::Diagctl),
            45 => Ok(Syscall::Vtimer),
            46 => Ok(Syscall::Runctl),
            50 => Ok(Syscall::Getmcontext),
            51 => Ok(Syscall::Setmcontext),
            52 => Ok(Syscall::Update),
            53 => Ok(Syscall::Exit),
            54 => Ok(Syscall::Schedctl),
            55 => Ok(Syscall::Statectl),
            56 => Ok(Syscall::Safememset),
            57 => Ok(Syscall::Padconf),
            _ => Err(()),
        }
    }
}

/// Compile-time verification that all syscall enum values are within [0, NR_SYS_CALLS).
///
/// C: map() macro's assert(call_index >= 0 && call_index < NR_SYS_CALLS)
/// Design decision D2: const assert replaces C's runtime assert in map() macro.
const _: () = {
    // R-16 (2026-08-12): SAFETY: `Syscall` is `#[repr(u16)]` (see enum decl
    // above), so every discriminant is stored as a u16 and `as u16` is a
    // lossless no-op that cannot truncate.
    assert!(Syscall::Fork as u16 == 0);
    assert!(Syscall::Padconf as u16 == 57);
    assert!((Syscall::Padconf as u16) < (NR_SYS_CALLS as u16));
};

/// Kernel call dispatch result.
///
/// C: return values from kernel_call_dispatch() + kernel_call_finish()
/// - Positive/zero values: OK result code
/// - VMSUSPEND (-996): call needs VM assistance
/// - EDONTREPLY: no reply should be sent
/// - EBADREQUEST (212): invalid syscall number
/// - ECALLDENIED (210): no permission for system call
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KcallResult {
    /// Call completed with an **error** return code (C: `return(EPERM)` etc.).
    /// SYSCALL 腿线上交付时取负（见 `syscall_leg_wire`），用户态 `reply < 0` 拦截。
    Ok(i32),
    /// Call completed with a **data** return code (C: `VMPTYPE_CHECK` 等正值语义码）。
    /// SYSCALL 腿线上交付时**不取负**——用户态用 `reply == expected_data` 判别，
    /// 与 C 的 `m_type = result` 直接传递正值等价。NK4-C F10b：与 `Ok(i32)` 区分，
    /// 使 ENOENT（空队列数据信号）、VMPTYPE_CHECK（1）、GetPdbr 物理地址等正值
    /// 不被 F10 取负，从而保持 `reply == ENOENT` 等 C 语义的用户态判断有效。
    Data(i32),
    /// Call requires VM assistance (C: VMSUSPEND = -996).
    VmSuspend,
    /// No reply should be sent (C: EDONTREPLY).
    NoReply,
    /// Invalid or unimplemented syscall number (C: EBADREQUEST = 212).
    BadCall,
    /// Caller lacks permission for this system call (C: ECALLDENIED = 210).
    /// C: `!GET_BIT(priv(caller)->s_k_call_mask, call_nr)` — system.c:111
    CallDenied,
}

impl KcallResult {
    /// Returns the errno to reply with, or `None` if no reply should be sent.
    ///
    /// Used by `kernel_call_finish` to unify the non-VmSuspend paths:
    /// C `kernel_call_finish` else-branch handles all non-VMSUSPEND cases
    /// uniformly (clear saved_msg + optional reply + release BKL).
    /// `VmSuspend` is excluded — it has its own dedicated path.
    pub(crate) fn reply_code(&self) -> Option<i32> {
        match self {
            KcallResult::Ok(ret) => Some(*ret),
            KcallResult::Data(v) => Some(*v),
            KcallResult::BadCall => Some(EBADREQUEST),
            KcallResult::CallDenied => Some(ECALLDENIED),
            KcallResult::NoReply | KcallResult::VmSuspend => None,
        }
    }

    /// SYSCALL 腿的**线上值**（F10b）：已按交付约定处理完毕，写入
    /// `frame.rax`/eager `m_type`/`set_ipc_return_code` 时直接使用，不再取负。
    ///
    /// - `Ok(code)`：错误码，线上取负（`syscall_leg_wire`）→ 用户态 `reply < 0` 拦截。
    /// - `Data(v)`：数据码（VMPTYPE_CHECK、ENOENT-as-empty、GetPdbr 地址等正值），
    ///   原样传递 → 用户态 `reply == expected` 判别，与 C 的 `m_type = result` 同义。
    /// - `BadCall`/`CallDenied`：簿记错误码，同样取负。
    /// - `NoReply`/`VmSuspend`：`None`（本腿不交付回执）。
    pub(crate) fn reply_wire(&self) -> Option<i32> {
        match self {
            KcallResult::Ok(ret) => Some(syscall_leg_wire(*ret)),
            KcallResult::Data(v) => Some(*v),
            KcallResult::BadCall => Some(syscall_leg_wire(EBADREQUEST)),
            KcallResult::CallDenied => Some(syscall_leg_wire(ECALLDENIED)),
            KcallResult::NoReply | KcallResult::VmSuspend => None,
        }
    }
}

/// NK4-C F10（P0-wire）：SYSCALL 腿（kernel_call）回执的**线上形式**。
///
/// 内核内部统一携带正 errno（C `do_privctl` 等 handler 的 `return(EPERM)`
/// 同形）；但交付给用户态的那一腿按本项目 ABI 契约走**负 errno**——
/// `servers/rs/src/trap_api.rs` 头注第 2 条「`sys_*` 内核调用 wrapper：
/// 裸 i32，负 = errno」，用户态 21+ 处 `reply < 0` 门控都建在这条契约上
///（s13b 实证：privctl SET_SYS 的 EFAULT=14 以正数上线后被 `< 0` 门控
/// 吞掉，RS 静默把失败当成功，boot 尾 10 服务器卡 NO_PRIV）。
///
/// 与 C 的关系如实登记：C 线上是正 errno + 调用方 `!= OK` 判定
///（system.c:79 `msg->m_type = result` + libsys 全仓 `r != OK`）；minix-rs
/// 用户态选择负 errno 契约后，腿内取负是两边自洽的最小改动面（改动点
/// 集中在这一条腿的交付侧，而不是散落的用户态判定）。int33 陷阱腿
/// 不取负——它的 rax 车道携带 IPC 状态位（正 bitfield，见
/// `kernel_call_finish_ipc_door` 的门纪律文档）。
pub(crate) const fn syscall_leg_wire(code: i32) -> i32 {
    code.wrapping_neg()
}

// ── Architecture-specific syscall dispatch trait (D6) ──
//
// D6: Architecture-specific syscalls return `BadCall` on unsupported platforms
// via default trait method implementations, avoiding `#[cfg(target_arch)]`
// behavior selection in the dispatch match. Each architecture implements this
// trait as a ZST, overriding only the methods it supports. The
// `CurrentArchSyscall` type alias selects the correct implementation via a
// single `#[cfg(target_arch)]` (one location, not scattered across 12 stubs).
//
// This replaces the previous pattern of 6 `#[cfg(not(target_arch))]` stubs +
// 6 `#[cfg(target_arch)]` real impls (12 cfg blocks total).

/// Architecture-specific kernel syscall dispatch.
///
/// Default implementations return `BadCall` — architectures override only
/// the methods they support.
pub trait ArchSyscall {
    /// SYS_DEVIO — port I/O (x86-only). C: do_devio.c
    ///
    /// K20: carries `proc_table` (uniform second slot).
    fn dispatch_devio(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &mut Message,
        priv_table: &PrivTable,
    ) -> KcallResult {
        let _ = (caller_nr, proc_table, msg, priv_table);
        KcallResult::BadCall
    }

    /// SYS_SDEVIO — sequential port I/O (x86-only). C: do_sdevio.c
    fn dispatch_sdevio(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &Message,
        priv_table: &PrivTable,
    ) -> KcallResult {
        let _ = (caller_nr, proc_table, msg, priv_table);
        KcallResult::BadCall
    }

    /// SYS_VDEVIO — vectored port I/O (x86-only). C: do_vdevio.c
    ///
    /// K20: carries `proc_table` (uniform second slot) — the converted
    /// callee re-borrows the caller slot from it for the VMSUSPEND tail.
    fn dispatch_vdevio(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &Message,
        priv_table: &PrivTable,
    ) -> KcallResult {
        let _ = (caller_nr, proc_table, msg, priv_table);
        KcallResult::BadCall
    }

    /// SYS_IOPENABLE — enable user I/O privilege (x86-only). C: do_iopenable.c
    fn dispatch_iopenable(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &Message,
    ) -> KcallResult {
        let _ = (caller_nr, proc_table, msg);
        KcallResult::BadCall
    }

    /// SYS_READBIOS — read BIOS memory (x86-only). C: do_readbios.c
    ///
    /// K20: carries `proc_table` (see `dispatch_vdevio`).
    fn dispatch_readbios(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &Message,
    ) -> KcallResult {
        let _ = (caller_nr, proc_table, msg);
        KcallResult::BadCall
    }

    /// SYS_PADCONF — pad configuration (32-bit arm only).
    ///
    /// C: `map(SYS_PADCONF, do_padconf)` is compiled `#if defined(__arm__)`
    /// only (system.c:251-253); do_padconf.c exists solely under
    /// `arch/earm/` (TI OMAP BSP). On every non-arm C build the call_vec
    /// entry is NULL → `EBADREQUEST` (system.c:120-123). The Rust kernel
    /// has no arm32 target, so `BadCall` (= EBADREQUEST via
    /// `reply_code()`) IS the C-parity answer for all supported
    /// architectures — there is nothing to implement.
    fn dispatch_padconf(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &Message,
    ) -> KcallResult {
        let _ = (caller_nr, proc_table, msg);
        KcallResult::BadCall
    }
}

/// x86_64 syscall dispatch — overrides x86-specific syscalls.
///
/// Delegates to `syscall_device::dispatch_*` with `CurrentPortIo`,
/// which uses x86 `in/out` instructions via inline assembly.
pub struct X86_64Syscall;

impl ArchSyscall for X86_64Syscall {
    fn dispatch_devio(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &mut Message,
        priv_table: &PrivTable,
    ) -> KcallResult {
        // C: do_devio.c — SYS_DEVIO (x86-only)
        let port_io = minix_plat::CurrentPortIo::new();
        crate::syscall_device::dispatch_devio(caller_nr, proc_table, msg, &port_io, priv_table)
    }

    fn dispatch_sdevio(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &Message,
        priv_table: &PrivTable,
    ) -> KcallResult {
        // C: do_sdevio.c — SYS_SDEVIO (x86-only)
        // Full implementation: parameter extraction, endpoint validation,
        // type/direction parsing, permission check (CHECK_IO_PORT), alignment
        // check, and batch I/O transfer (SAFE path: verify_grant +
        // data_copy_vmcheck; unsafe path: copy_from_user/copy_to_user).
        let port_io = minix_plat::CurrentPortIo::new();
        crate::syscall_device::dispatch_sdevio(caller_nr, proc_table, msg, &port_io, priv_table)
    }

    fn dispatch_vdevio(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &PrivTable) -> KcallResult {
        // C: do_vdevio.c — SYS_VDEVIO (x86-only)
        // Full batch I/O: copy (port,value) pairs from user, permission check,
        // execute via PortIo, copy results back for input.
        let port_io = minix_plat::CurrentPortIo::new();
        crate::syscall_device::dispatch_vdevio(caller_nr, proc_table, msg, &port_io, priv_table)
    }

    fn dispatch_iopenable(
        caller_nr: crate::proc::ProcNr,
        proc_table: &mut ProcessTable,
        msg: &Message,
    ) -> KcallResult {
        // C: do_iopenable.c — SYS_IOPENABLE (x86-only)
        // SELF endpoint resolution + IOPL enable via
        // CurrentCpuContextArch::enable_user_io (kernel-layer abstraction).
        crate::syscall_device::dispatch_iopenable(caller_nr, proc_table, msg)
    }

    fn dispatch_readbios(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult {
        // C: do_readbios.c — SYS_READBIOS (x86-only)
        // Full implementation: parameter extraction, BIOS memory range
        // validation, and page-by-page copy from BIOS memory via
        // `data_copy_vmcheck` (matches C's `virtual_copy_vmcheck`).
        crate::syscall_device::dispatch_readbios(caller_nr, proc_table, msg)
    }
}

/// ARM (32-bit) syscall dispatch — overrides ARM-specific syscalls.
///
/// Unused today (no `arm` target in this workspace). `SYS_PADCONF` keeps
/// the trait default: its C map is `#if defined(__arm__)` only
/// (system.c:251-253), so `BadCall` (= EBADREQUEST, system.c:120-123
/// NULL-entry behavior) is the correct answer on every architecture this
/// workspace builds.
pub struct ArmSyscall;

impl ArchSyscall for ArmSyscall {
    // dispatch_padconf uses the trait default (BadCall → EBADREQUEST) —
    // correct by ground truth, not a deferred implementation.
}

/// Default syscall dispatch for architectures without arch-specific syscalls
/// (e.g., aarch64, riscv64). All arch-specific syscalls return `BadCall`.
pub struct DefaultSyscall;

impl ArchSyscall for DefaultSyscall {}

/// Current architecture's syscall dispatch type.
///
/// Selected via a single `#[cfg(target_arch)]` (one location), replacing
/// the previous 12 scattered `#[cfg]` blocks for individual stub functions.
///
/// # aarch64 fallback (intentional)
///
/// `L370` matches `arm` (32-bit ARM). aarch64 falls through to the
/// default `DefaultSyscall` (no-op impl) — **aarch64 syscall dispatch is
/// not yet implemented**. When aarch64 support lands, insert **before**
/// the `arm` arm:
///
/// ```ignore
/// #[cfg(target_arch = "aarch64")]
/// pub type CurrentArchSyscall = Aarch64Syscall;
/// ```
///
/// Otherwise the default fallback silently applies and `ArchSyscall`
/// calls return `BadCall` for every syscall. Tracked in todo.md B-X.
#[cfg(target_arch = "x86_64")]
pub type CurrentArchSyscall = X86_64Syscall;
#[cfg(target_arch = "arm")]
pub type CurrentArchSyscall = ArmSyscall;
#[cfg(not(any(target_arch = "x86_64", target_arch = "arm")))]
pub type CurrentArchSyscall = DefaultSyscall;

/// Dispatch a kernel system call.
///
/// C: kernel_call_dispatch() in system.c:103-116
/// C: kernel_call_finish() in system.c:58-90
///
/// Design decision D1: match replaces call_vec[] dispatch.
/// Design decision D5: s_k_call_mask checked at dispatch entry (runtime bitmap).
/// Design decision D6: arch-specific syscalls return BadCall on unsupported
/// platforms via ArchSyscall trait default methods instead of being
/// conditionally compiled out.
///
/// # BKL (Big Kernel Lock) — SMP Safety
///
/// This function acquires the BKL on entry and releases it on exit.
/// In C, the BKL is acquired in the assembly trap entry (`mpx.S`) and
/// released in `switch_to_user()`. In Rust, we acquire it here because
/// the kernel does not yet have an assembly-level BKL wrapper.
///
/// On single-CPU builds, `bkl_lock()` is a compiler fence + atomic CAS
/// that succeeds immediately (the lock is never contended), so the
/// overhead is negligible.
///
/// **Invariant**: The BKL must be held for the entire duration of
/// `kernel_call_dispatch` + `kernel_call_finish`. The only exception
/// is the `VmSuspend` path in `kernel_call_finish`, which releases
/// the BKL before waiting for VM (see `kernel_call_finish` docs).
/// D-8: `kernel_call()` wrapper — C system.c:136-163.
///
/// The full kernel call sequence: save user msg address → copy message
/// from user space (TOCTOU defense: the kernel works on its own copy,
/// not the user's mutable memory) → dispatch → finish. On copy failure:
/// SIGSEGV (C system.c:152-155 — bad user pointer, not a kernel bug).
///
/// # TOCTOU defense
///
/// C: `copy_msg_from_user(m_user, &msg)` copies the user-space message
/// into a kernel-stack `msg` at entry; dispatch and finish operate on
/// this kernel copy. Without this, a malicious user could mutate the
/// message between the dispatch's parameter validation and the finish's
/// reply construction (check-to-use window).
///
/// # Arguments
///
/// * `caller` — the process making the kernel call
/// * `m_user` — user-space virtual address of the message
/// * `proc_table` / `priv_table` / `clock_state` — passed through to dispatch
/// * `user_copy` — user-space copy abstraction (arch-injected)
pub fn kernel_call(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    m_user: minix_types::VirBytes,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
    user_copy: &dyn crate::ipc::UserCopy,
) -> KcallResult {
    // C system.c:141 — save the user-space reply address. K20 caller-by-nr:
    // the caller slot is re-borrowed for each short access.
    // NK4-C S2h 现场打印（task1-close 裁决删除）：抹写目标锁定为陈旧
    // p_delivermsg_vir（fx 写的 0x9da8 ≠ 窗口内活缓冲 r2=0x9d28）——追
    // 踪每个存值的来源时刻，与 fx 写目标离线对账。
    // c40 扩展（task1-close 裁决删除）：加入 INIT（ep 0xb）——c39 实锤
    // INIT 停车时 pdmv 非零而崩溃投递 start=0x0，SYSCALL 腿是本端唯一
    // 对 INIT 盲区的 pdmv 写点，若某次 RDI=0 即清掉 pdmv。
    #[cfg(not(feature = "mock"))]
    #[cfg(target_arch = "x86_64")]
    let nk4a_pdmv_probe_ep = proc_table
        .get(caller_nr)
        .map(|p| p.p_endpoint.0)
        .filter(|&e| e == 2 || e == 0xb);
    #[cfg(not(feature = "mock"))]
    #[cfg(target_arch = "x86_64")]
    let nk4a_pdmv_old = proc_table
        .get(caller_nr)
        .map(|p| p.p_delivermsg_vir.0)
        .unwrap_or(0);
    #[cfg(not(feature = "mock"))]
    #[cfg(target_arch = "x86_64")]
    if nk4a_pdmv_probe_ep == Some(2) {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static PSET: AtomicUsize = AtomicUsize::new(0);
        // S2h 评审修复：4096→512 + 触顶现形标记。
        let ps = PSET.fetch_add(1, AtomicOrd::Relaxed);
        if ps == 512 {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: pdmv-cap\n");
        }
        if ps < 512 {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: pdmv-set krn m_user=");
            C0::write_hex(m_user.0);
            C0::write_str(" old=");
            C0::write_hex(nk4a_pdmv_old);
            C0::write_str("\n");
        }
    }
    proc_table
        .get_mut(caller_nr)
        .expect("kernel_call: caller slot must exist")
        .p_delivermsg_vir = m_user;

    // C system.c:147 — copy the message from user space (TOCTOU defense).
    let msg = match user_copy.copy_msg_from_user(m_user) {
        Ok(m) => m,
        Err(_) => {
            // 续-184 探针（用后即滚）：EFAULT 现场——caller 根、delivermsg
            // VA、该 VA 在 caller 根的 walk 结果（NP/映射值）。定谳「双
            // SETADDRSPACE 换根后 delivermsg 页 PTE 缺失」。
            #[cfg(target_arch = "riscv64")]
            {
                let root = proc_table
                    .get(caller_nr)
                    .map(|p| p.p_seg.phys_root.0)
                    .unwrap_or(0);
                use minix_arch::paging::Paging as _;
                let mut walk = minix_arch::CurrentPaging::from_active_root(
                    minix_types::PhysBytes(root),
                );
                let q = walk.query(minix_types::VirBytes(m_user.0));
                use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                Console::write_str("nk4a: kc-efault caller=");
                Console::write_hex(caller_nr.0 as u64);
                Console::write_str(" root=");
                Console::write_hex(root);
                Console::write_str(" dva=");
                Console::write_hex(m_user.0);
                Console::write_str(" q=");
                match q {
                    Some((pa, f)) => {
                        Console::write_hex(pa.0);
                        Console::write_str("/");
                        Console::write_hex(f.bits() as u64);
                    }
                    None => Console::write_str("NP"),
                }
                Console::write_str("\n");
            }
            // C system.c:152-155 — printf WARNING + cause_sig(SIGSEGV).
            // Rust: route to cause_signal(SIGSEGV) — same signal closed
            // loop as D-45/D-43 in process_misc_flags.
            crate::syscall_signal::cause_signal(
                caller_nr,
                crate::syscall_signal::SIGSEGV,
                proc_table,
                priv_table,
            );
            return KcallResult::Ok(EFAULT);
        }
    };

    // C system.c:148 — stamp the sender's endpoint.
    let mut msg = msg;
    msg.m_source = proc_table
        .get(caller_nr)
        .map(|c| c.p_endpoint)
        .expect("kernel_call: caller slot must exist");

    // c40 探针（task1-close 裁决删除）：INIT（ep 0xb）的 SYSCALL 腿存值
    // 现场——m_user(RDI) + 旧 pdmv + m_type（哪个 SYS_ 调用）。独立计数
    // 器，不与 RS 探针共享额度。
    #[cfg(not(feature = "mock"))]
    #[cfg(target_arch = "x86_64")]
    if nk4a_pdmv_probe_ep == Some(0xb) {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static PSET_I: AtomicUsize = AtomicUsize::new(0);
        let ps = PSET_I.fetch_add(1, AtomicOrd::Relaxed);
        if ps == 256 {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: pdmv-i-cap\n");
        }
        if ps < 256 {
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            C0::write_str("nk4a: pdmv-set krn-i m_user=");
            C0::write_hex(m_user.0);
            C0::write_str(" old=");
            C0::write_hex(nk4a_pdmv_old);
            C0::write_str(" mt=0x");
            C0::write_hex(msg.m_type as u64);
            C0::write_str("\n");
        }
    }

    // C system.c:149 — dispatch.
    let result = kernel_call_dispatch(caller_nr, proc_table, &mut msg, priv_table, clock_state);

    // C system.c:160 — kbill_kcall = caller (D-9, inside dispatch).

    // C system.c:162 — finish (VMSUSPEND / reply / BKL release).
    kernel_call_finish(caller_nr, proc_table, &msg, result, priv_table);
    result
}

pub fn kernel_call_dispatch(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // Acquire BKL — C: BKL_LOCK() in mpx.S kernel_call_entry_common
    // R-03: Keep the guard alive and derive a BklSection witness for
    // compile-time BKL proof on global accessor calls (irq_manager_with, etc.).
    // R-05/B1: BklGuard is RAII (Drop releases BKL); transfer() hands
    // ownership to the ambient held-BKL scope because BKL must stay held
    // until kernel_call_finish() releases it.
    let bkl_guard = crate::smp::bkl_lock();
    let result = {
        let bkl_section = bkl_guard.section();
        kernel_call_dispatch_inner(caller_nr, proc_table, msg, priv_table, clock_state, &bkl_section)
    };
    // D-9 (C system.c:160) — the kernel call is in flight for `caller`:
    // attribute kernel time to it at the next context_stop. C sets the
    // marker after dispatch, before finish, unconditionally (a failed
    // call's kernel work is still the caller's); kernel_call_resume does
    // not re-set it.
    crate::set_kbill_kcall_with(caller_nr, &bkl_guard.section());
    // BKL is NOT released here — transfer() suppressed the guard's Drop.
    // BKL is released in:
    //   1. kernel_call_finish() — for normal completion (before switch_to_user)
    //   2. switch_to_user() — before returning to user mode
    bkl_guard.transfer();
    result
}

/// Inner dispatch logic, called after BKL is acquired.
///
/// `pub(crate)`: the scheduler-loop KCALL_RESUME re-dispatch (stage 3a)
/// calls this directly — C's `kernel_call_resume` dispatches the saved
/// reqmsg without re-entering mpx.S's BKL_LOCK, and the Rust BKL is
/// bundled in [`kernel_call_dispatch`] which would self-deadlock under
/// the scheduler loop's held lock.
pub(crate) fn kernel_call_dispatch_inner(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
    bkl_section: &crate::smp::BklSection<'_>,
) -> KcallResult {
    // R-16 (2026-08-12): SAFETY: `msg.m_type` is `i32`; the cast keeps the low
    // 16 bits. All legitimate kernel-call numbers are `< NR_SYS_CALLS` which
    // fits in u16, so valid calls survive intact. Any out-of-range (or
    // truncated) value is rejected by `Syscall::try_from` below as `BadCall`,
    // and in-range survivors are still gated by the per-caller `kcall_mask`.
    //
    // Base normalization (C mpx.S `subl $KERNEL_CALL` parity): libc issues
    // ABSOLUTE call numbers (com.h `SYS_*` = KERNEL_CALL + offset — the
    // E2/E6 wrapper precedent), while the dispatch tables index by the
    // RELATIVE offset. Host tests feed relative numbers directly; both
    // shapes normalize here — anything below the base passes through.
    let call_nr = if msg.m_type >= minix_types::KERNEL_CALL {
        (msg.m_type - minix_types::KERNEL_CALL) as u16
    } else {
        msg.m_type as u16
    };

    let syscall = match Syscall::try_from(call_nr) {
        Ok(s) => s,
        Err(()) => return KcallResult::BadCall,
    };
    // NK4-A C-3 迭代6 取证（task1-close 裁决删除）：内核调用流水（限 64
    // 条）——定位 RS 用户态的循环点（哪个调用在反复挂起/重试）。
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static KCALL_LOG: AtomicUsize = AtomicUsize::new(0);
        // 只记 RS(caller 2) 的调用——定位其用户态循环点。
        if caller_nr.0 == 2 && KCALL_LOG.fetch_add(1, AtomicOrd::Relaxed) < 48 {
            use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
            Console::write_str("nk4a: kc");
            Console::write_hex(KCALL_LOG.load(AtomicOrd::Relaxed) as u64);
            Console::write_str(" caller=");
            Console::write_hex(caller_nr.0 as u64);
            Console::write_str(" call=");
            Console::write_hex(call_nr as u64);
            Console::write_str("\n");
        }
    }

    // C: `else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr))` — system.c:111
    // Check if the caller has permission to invoke this system call.
    // Processes without an assigned privilege (priv_id == None) are denied
    // all kernel calls — this should not happen for running processes.
    //
    // Composed as `Option::and_then` + `is_none_or(...)`:  `is_none_or` has
    // the same semantics as `map_or(true, ...)` (None → deny, i.e. true):
    //   - `None` (no priv_id, or priv_id not in table) → deny (true)
    //   - `Some(priv)` → deny iff `kcall_filter_check` returns false
    let call_denied = proc_table
        .get(caller_nr)
        .and_then(|c| c.priv_id)
        .and_then(|id| priv_table.get(id))
        .is_none_or(|caller_priv| !kcall_filter_check(caller_priv, call_nr as u32));
    if call_denied {
        return KcallResult::CallDenied;
    }

    match syscall {
        Syscall::Fork => dispatch_fork(caller_nr, proc_table, msg, priv_table),
        Syscall::Exec => dispatch_exec(caller_nr, proc_table, msg),
        Syscall::Clear => dispatch_clear(caller_nr, proc_table, msg, priv_table, clock_state),
        Syscall::Exit => dispatch_exit(caller_nr, proc_table, msg, priv_table),
        Syscall::Schedule => dispatch_schedule(caller_nr, proc_table, msg, priv_table),
        Syscall::Privctl => dispatch_privctl(caller_nr, proc_table, msg, priv_table, clock_state),
        Syscall::Trace => dispatch_trace(caller_nr, proc_table, msg, priv_table),
        Syscall::Kill => dispatch_kill(caller_nr, proc_table, msg, priv_table),
        Syscall::Getksig => dispatch_getksig(caller_nr, proc_table, msg, priv_table),
        Syscall::Endksig => dispatch_endksig(caller_nr, proc_table, msg, priv_table),
        Syscall::Sigsend => dispatch_sigsend(caller_nr, proc_table, msg),
        Syscall::Sigreturn => dispatch_sigreturn(caller_nr, proc_table, msg),
        Syscall::Memset => dispatch_memset(caller_nr, proc_table, msg),
        Syscall::Umap => dispatch_umap(caller_nr, proc_table, msg, priv_table),
        Syscall::Vircopy => dispatch_vircopy(caller_nr, proc_table, msg),
        Syscall::Physcopy => dispatch_physcopy(caller_nr, proc_table, msg),
        Syscall::UmapRemote => dispatch_umap_remote(caller_nr, proc_table, msg, priv_table),
        Syscall::Vumap => dispatch_vumap(caller_nr, proc_table, msg, priv_table),
        Syscall::Irqctl => dispatch_irqctl(caller_nr, proc_table, msg, priv_table, bkl_section),
        // D6: x86-specific syscalls — return BadCall on other architectures.
        Syscall::Devio => CurrentArchSyscall::dispatch_devio(caller_nr, proc_table, msg, priv_table),
        Syscall::Sdevio => CurrentArchSyscall::dispatch_sdevio(caller_nr, proc_table, msg, priv_table),
        // D6: VDEVIO is also x86-specific (system.c:215-216: #if defined(__i386__))
        Syscall::Vdevio => CurrentArchSyscall::dispatch_vdevio(caller_nr, proc_table, msg, priv_table),
        Syscall::Setalarm => dispatch_setalarm(caller_nr, msg, priv_table, clock_state, proc_table),
        Syscall::Times => dispatch_times(caller_nr, msg, proc_table),
        Syscall::Getinfo => dispatch_getinfo(caller_nr, proc_table, msg, priv_table, clock_state),
        Syscall::Abort => dispatch_abort(caller_nr, proc_table, msg),
        Syscall::Iopenable => CurrentArchSyscall::dispatch_iopenable(caller_nr, proc_table, msg),
        Syscall::SafecopyFrom => dispatch_safecopy_from(caller_nr, proc_table, msg, priv_table),
        Syscall::SafecopyTo => dispatch_safecopy_to(caller_nr, proc_table, msg, priv_table),
        Syscall::Vsafecopy => dispatch_vsafecopy(caller_nr, proc_table, msg, priv_table),
        Syscall::Setgrant => dispatch_setgrant(caller_nr, proc_table, msg, priv_table),
        Syscall::Readbios => CurrentArchSyscall::dispatch_readbios(caller_nr, proc_table, msg),
        Syscall::Sprof => dispatch_sprofile(caller_nr, proc_table, msg),
        Syscall::Stime => dispatch_stime(msg, clock_state),
        Syscall::Settime => dispatch_settime(msg, clock_state),
        Syscall::Vmctl => dispatch_vmctl(caller_nr, proc_table, msg, priv_table),
        Syscall::Diagctl => dispatch_diagctl(caller_nr, proc_table, msg, priv_table),
        Syscall::Vtimer => dispatch_vtimer(caller_nr, msg, priv_table, proc_table),
        Syscall::Runctl => dispatch_runctl(caller_nr, proc_table, msg),
        Syscall::Getmcontext => dispatch_getmcontext(caller_nr, proc_table, msg),
        Syscall::Setmcontext => dispatch_setmcontext(caller_nr, proc_table, msg),
        Syscall::Update => dispatch_update(caller_nr, proc_table, msg, priv_table),

        Syscall::Schedctl => dispatch_schedctl(caller_nr, proc_table, msg),
        Syscall::Statectl => dispatch_statectl(caller_nr, proc_table, msg, priv_table, crate::ipc_filter_pool_with(bkl_section)),
        Syscall::Safememset => dispatch_safememset(caller_nr, proc_table, msg, priv_table),
        // D6: ARM-specific — return BadCall on other architectures.
        Syscall::Padconf => CurrentArchSyscall::dispatch_padconf(caller_nr, proc_table, msg),
    }
}

/// Public entry point for IPC traps.
///
/// This is the IPC counterpart to `kernel_call_dispatch`. The arch trap
/// entry handler calls this directly when the trap is an IPC call
/// (SEND/RECEIVE/SENDREC/NOTIFY/SENDNB/SENDA), bypassing
/// `kernel_call_dispatch_inner` to avoid call_nr range conflict with
/// SYS_* calls.
///
/// # Architecture dispatch (Phase 1A, 2026-08-12)
///
/// - **x86-64**: IDT vector 33 (IPC_VECTOR) → `ipc_entry_softint_orig`
///   assembly → `dispatch_ipc_entry`. Configured by
///   `TrapEntryArch::configure_ipc_entry` which sets IDT gate 33.
///   C: protect.c:147 `{ ipc_entry_softint_orig, IPC_VECTOR_ORIG, USER_PRIVILEGE }`.
/// - **ARM64**: SVC handler reads r3 register at runtime:
///   `r3 == IPCVEC_INTR` → `ipc_entry` assembly → `dispatch_ipc_entry`.
///   C: earm/mpx.S:183-184 `cmp r3, #IPCVEC_INTR; beq ipc_entry`.
/// - **RISC-V**: ecall handler reads a7 register at runtime:
///   `a7 < 17` → `dispatch_ipc_entry` (IPC call numbers 1-16).
///
/// # BKL ownership
///
/// Acquires BKL before dispatching (same pattern as `kernel_call_dispatch`).
/// BKL is released in `kernel_call_finish` or `switch_to_user` — see
/// `kernel_call_dispatch` docs for the BKL lifetime contract.
///
/// C: `do_ipc(r1, r2, r3)` — proc.c:599-697
pub fn dispatch_ipc_entry(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
    priv_table: &mut PrivTable,
) -> KcallResult {
    // Decode IPC call number from m_type.
    // C: proc.c:602 — `int call_nr = (int) r1;` where r1 is the syscall
    // number register. In Rust, m_type holds the call number (set by arch
    // trap entry before calling this function).
    let call_nr = msg.m_type;
    let ipc_call = match crate::ipc::IpcCall::from_raw(call_nr) {
        Some(c) => c,
        None => return KcallResult::Ok(crate::errno::EBADCALL),
    };

    // Extract caller_idx before borrowing the procs slice (FIX-21, Phase
    // 1C: avoids split-borrow aliasing).
    let caller_idx = crate::proc_table::nr_to_idx(caller_nr)
        .expect("dispatch_ipc_entry: caller_nr out of range") as usize;

    // Acquire BKL — C: BKL_LOCK() in mpx.S ipc_entry assembly.
    // R-05/B1: the guard is RAII; `transfer()` hands ownership to the
    // ambient held-BKL scope because BKL must stay held until
    // kernel_call_finish() releases it.
    let bkl_guard = crate::smp::bkl_lock();
    // `caller` is derived from `proc_table` by the trap entry (raw-pointer
    // global access); passing `proc_table` (not a slice) lets dispatch_ipc
    // run the scheduler-aware sig_delay_done protocol.
    let result = dispatch_ipc(proc_table, caller_idx, msg, priv_table, ipc_call);
    // BKL is NOT released here — transfer() suppressed the guard's Drop.
    // BKL is released in:
    //   1. kernel_call_finish() — for normal completion (before switch_to_user)
    //   2. switch_to_user() — before returning to user mode
    bkl_guard.transfer();
    result
}

/// Dispatch IPC primitives (SEND/RECEIVE/SENDREC/NOTIFY/SENDNB/SENDA).
///
/// C: `do_ipc(r1, r2, r3)` — proc.c:599-697. In Minix3, the arch syscall
/// handler calls `do_ipc` directly for IPC call numbers, bypassing
/// `call_vec[]`. In Rust, `kernel_call_dispatch_inner` routes to this
/// function when `call_nr` matches an `IpcCall` variant.
///
/// # Parameter extraction (L1, 2026-08-12)
///
/// The arch trap entry handler stores register arguments in `caller.p_defer`
/// before entering `kernel_call_dispatch`:
/// - `p_defer.r2` = `src_dst` endpoint (for SEND/RECEIVE/SENDREC/NOTIFY/SENDNB)
///   or table count (for SENDA)
/// - `p_defer.r3` = user-space table pointer (SENDA only)
/// - `msg.m_type` = IPC call_nr (SEND=1..SENDA=16)
///
/// This mirrors C's register-to-`p_defer` mapping in `arch_system.c:492`
/// (`do_ipc(proc->p_defer.r1, proc->p_defer.r2, proc->p_defer.r3)`).
///
/// # Outcome mapping
///
/// - `Delivered` → `Ok(OK=0)`: message delivered, caller stays runnable
/// - `Blocked` → `NoReply`: caller is now blocked (RTS_SENDING/RECEIVING),
///   no reply should be sent — scheduler will pick next process
/// - `Error(e)` → `Ok(errno)`: IPC failed, caller stays runnable with errno
///
/// # Delay-end signal protocol (C: proc.c:1082-1083)
///
/// When a receiver takes a message from a sender whose `MF_SIG_DELAY` is
/// set (PM's delayed stop), the sender has reached a quiescent point and
/// PM must be told via `sig_delay_done` → `cause_sig(SIGSNDELAY)`. That
/// notification sets `RTS_SIGNALED` through the scheduler-aware `rts_set`
/// (dequeue), which requires `ProcessTable` — hence this function takes
/// `proc_table` (not a bare slice) and runs the protocol after `do_ipc`.
pub(crate) fn dispatch_ipc(
    proc_table: &mut crate::proc_table::ProcessTable,
    caller_idx: usize,
    msg: &Message,
    priv_table: &mut PrivTable,
    ipc_call: crate::ipc::IpcCall,
) -> KcallResult {
    use crate::ipc::{IpcEngine, IpcOutcome, IpcError, KernelUserCopy, SendFlags};
    use crate::errno::*;
    use minix_types::VirBytes;
    use minix_arch::{CurrentCpuContextArch, CpuContextArch};

    // ── MINIX_KERNINFO (6) ──
    // C: proc.c:685-693 — the kernel info page is handed over through the
    // secondary IPC return channel, not a message: check the page has been
    // published to user space, store its address in the caller's saved
    // context, return OK. No message buffer is read or written, and the
    // endpoint/SENDA argument decode below does not apply.
    if matches!(ipc_call, crate::ipc::IpcCall::KernInfo) {
        let page = crate::globals::MINIX_KERNINFO_USER.load(core::sync::atomic::Ordering::Relaxed);
        if page == crate::globals::KERNINFO_USER_UNSET {
            // C: proc.c:687-689 — "It might not be initialized yet."
            return KcallResult::Ok(EBADCALL);
        }
        let procs = proc_table.procs_slice_mut();
        <CurrentCpuContextArch as CpuContextArch>::set_secondary_ipc_return(
            &mut procs[caller_idx].cpu_context,
            page,
        );
        return KcallResult::Ok(OK);
    }

    // Read caller fields by index (avoiding split-borrow issue — the caller
    // lives inside `proc_table`, so we derive it by index and copy the
    // fields out before constructing the `IpcEngine` over the process
    // slice). FIX-21, Phase 1C: refactored from
    // (caller: &mut KProcess, proc_table) to (procs, caller_idx); this
    // variant takes `ProcessTable` directly so the delay-end signal
    // protocol below can run scheduler-aware.
    let (caller_nr, defer) = {
        let procs = proc_table.procs_slice_mut();
        let caller = &procs[caller_idx];
        (caller.p_nr, caller.p_defer)
    };

    // Extract dst_endpoint (or SENDA table params) from p_defer.
    // C: r2 = src_dst (for sync IPC) or count (for SENDA)
    // (r1 = call_nr is already carried by `msg.m_type`.)
    let dst_endpoint;
    let senda_table;
    match ipc_call {
        crate::ipc::IpcCall::SendA => {
            // C: proc.c:673 — `size_t msg_size = (size_t) r2;`
            // C: proc.c:683 — `mini_senda(caller_ptr, (asynmsg_t *) r3, msg_size);`
            let count = defer.r2;
            let table_ptr = defer.r3;
            dst_endpoint = minix_types::Endpoint::ANY;
            senda_table = Some((VirBytes(table_ptr as u64), count));
        }
        _ => {
            // Sync IPC: r2 = src_dst endpoint
            // R-16 SAFETY: p_defer.r2 is `usize`; casting to i32 preserves the
            // low 32 bits. Endpoints are i32 in Minix3, so valid endpoints
            // survive intact. Invalid values are caught by IpcEngine's
            // endpoint validity check (Layer 1: idx_by_endpoint returns None).
            dst_endpoint = minix_types::Endpoint(defer.r2 as i32);
            senda_table = None;
        }
    }

    // Construct IpcEngine and dispatch.
    // C: do_ipc → do_sync_ipc → mini_send/receive/notify/sendrec
    //
    // The engine borrows the process slice out of `proc_table`; the scope
    // block ends that borrow so the `sig_delay_done` protocol below can
    // touch `proc_table` again (the scheduler-aware `cause_signal`).
    let (outcome, pending_sig_delay, woken_target) = {
        // D-16: wire the global IPC filter pool. A1 chain root (D-63②):
        // this is the kernel_call dispatch path — the BKL is held by the
        // kernel_call contract (kernel_call_dispatch acquires it); the
        // fn signature cannot carry the witness, so the root takes
        // `assume_held` (debug builds assert the lock). S-8 will thread a
        // real witness from the trap entry.
        let section = unsafe { crate::smp::BklSection::assume_held() };
        // C-25（C `proc.c:607`）：`do_ipc` 入口无条件把 kbill_ipc 记到调用者
        // ——在权限与跟踪检查**之前**（C 的位置在 do_ipc 第一行），这样连
        // 被拒的 IPC 的核内时间也算在发起者头上。
        crate::set_kbill_ipc_with(caller_nr, &section);
        let mut engine = IpcEngine::new(proc_table.procs_slice_mut(), priv_table, &KernelUserCopy)
            .with_filter_pool(crate::ipc_filter_pool_with(&section));
        let outcome = engine.do_ipc(
            caller_nr,
            ipc_call,
            dst_endpoint,
            msg,
            SendFlags::NONE,
            senda_table,
        );
        // A sender whose message was delivered while `MF_SIG_DELAY` was set
        // needs its PM stop-delay ended: take the record out of the engine
        // (it holds the proc_table/priv_table borrows).
        let pending = engine.take_sig_delay_sender();
        // The engine may also have WOKEN a parked process (clearing
        // RTS_RECEIVING/RTS_SENDING with the primitive setter) — take the
        // record so the ProcessTable-level code below can enqueue it.
        // 1.12：收集全部 wake 记录（多唤醒不互覆）。
        let mut woken = [None; 4];
        for slot in woken.iter_mut() {
            *slot = engine.take_wake_target();
        }
        (outcome, pending, woken)
    };

    // Map IpcOutcome → KcallResult.
    // C: do_ipc returns errno (OK=0 for success/delivered, ELOCKED etc. for
    // errors). Blocked is implicit in C (RTS flags set), but Rust makes it
    // explicit via IpcOutcome::Blocked.
    let result = match outcome {
        IpcOutcome::Delivered => KcallResult::Ok(OK),
        IpcOutcome::Blocked => KcallResult::NoReply,
        IpcOutcome::Error(e) => {
            // NK4-C 1.10j 取证探针（task1-close 裁决删除）：IPC 错误返回
            // 现场（caller 端点 + 错误类别）——SCHED/PM「receive 连续失败」
            // fail-fast 的错误类别定位。
            #[cfg(not(feature = "mock"))]
            {
                use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static IPCERR_N: AtomicUsize = AtomicUsize::new(0);
                if IPCERR_N.fetch_add(1, AtomicOrd::Relaxed) < 64
                    && matches!(caller_nr.0, 0 | 4)
                {
                    Console::write_str("nk4a: ipcerr caller=");
                    Console::write_hex(caller_nr.0 as u64);
                    Console::write_str(" err=");
                    match e {
                        IpcError::Deadlock => Console::write_str("ELOCKED"),
                        IpcError::DeadSrcDst => Console::write_str("EDEADSRCDST"),
                        IpcError::NotReady => Console::write_str("ENOTREADY"),
                        IpcError::BadCall => Console::write_str("EBADCALL"),
                        IpcError::Fault => Console::write_str("EFAULT"),
                        IpcError::CallDenied => Console::write_str("ECALLDENIED"),
                        IpcError::TrapDenied => Console::write_str("ETRAPDENIED"),
                        IpcError::Permission => Console::write_str("EPERM"),
                        IpcError::Invalid => Console::write_str("EINVAL"),
                    }
                    Console::write_str("\n");
                }
            }
            let errno = match e {
                IpcError::Deadlock => ELOCKED,
                IpcError::DeadSrcDst => EDEADSRCDST,
                IpcError::NotReady => ENOTREADY,
                IpcError::BadCall => EBADCALL,
                IpcError::Fault => EFAULT,
                IpcError::CallDenied => ECALLDENIED,
                IpcError::TrapDenied => ETRAPDENIED,
                IpcError::Permission => EPERM,
                IpcError::Invalid => EINVAL,
            };
            KcallResult::Ok(errno)
        }
    };

    // C: proc.c:1082-1083 — sig_delay_done(sender) for a delay-stopped
    // sender whose message was just delivered. Runs after do_ipc but still
    // under BKL (dispatch_ipc_entry), so no observable interleaving vs C.
    if let Some(sender_nr) = pending_sig_delay {
        proc_table.sig_delay_done(sender_nr, priv_table);
    }

    // Complete the wake ENQUEUE half for a process the engine unparked
    // (C: RTS_UNSET's enqueue — see ProcessTable::enqueue_if_woken). Runs
    // under the same BKL discipline as sig_delay_done above.
    // 1.12：engine 的 wake 记录为 4 槽（同一次 syscall 可能唤醒多个
    // 进程——sendrec 的 send 腿 + receive 腿 drain），必须全部入队；
    // 旧的单次 if let 丢多 wake。
    for woken_nr in woken_target.into_iter().flatten() {
        proc_table.enqueue_if_woken(woken_nr);
    }

    result
}

// ── Dispatch functions ──
// Each dispatch_* function delegates to the corresponding subsystem module.
// Functions that need ProcessTable or PrivTable receive them from
// kernel_call_dispatch (threaded through since 2026-06-15).

fn dispatch_fork(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &mut Message, priv_table: &PrivTable) -> KcallResult {
    crate::syscall_process::dispatch_fork(caller_nr, proc_table, msg, priv_table)
}
fn dispatch_exec(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult { crate::syscall_process::dispatch_exec(caller_nr, proc_table, msg) }
fn dispatch_clear(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &mut crate::kpriv::PrivTable, clock_state: &mut ClockState) -> KcallResult { crate::syscall_process::dispatch_clear(caller_nr, proc_table, msg, priv_table, clock_state) }
fn dispatch_exit(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &mut crate::kpriv::PrivTable) -> KcallResult { crate::syscall_process::dispatch_exit(caller_nr, proc_table, msg, priv_table) }

/// Dispatch SYS_SCHEDULE.
///
/// C: `do_schedule()` — system.c:284-323
///
/// Sets scheduling parameters (priority, quantum, CPU) for a process.
/// This is an internal kernel call used by the scheduler process.
///
/// # Implementation status (FIX-25, 2026-08-13)
///
/// Fully implemented — all 4 validation steps + `sched_proc()` body.
///   1. **SYS_PROC permission check**: `caller_has_sys_proc(caller)`.
///      SYS_SCHEDULE is only allowed from the system process.
///   2. **Endpoint validation**: `isokendpt(endpoint, &proc_nr)` rejects
///      unknown endpoints with EINVAL.
///   3. **Process slot lookup**: the target process must be in the
///      proc_table (validated by endpoint_to_nr).
///   4. **Permission check (p_scheduler)**: C's `caller != p->p_scheduler`
///      check. In Rust, the equivalent is `caller.p_nr != target.scheduler`
///      (or `target.scheduler.is_none()` to match C's `p_scheduler == NULL`
///      fallback).
///   5. **Apply scheduling parameters**: `sched::sched_proc(target, SchedParams)`
///      updates priority / quantum / cpu / niced on the target process.
///      Errors are translated to errno via `ToErrno` (D2).
///
/// # Historical note
///
/// Earlier revisions of this doc comment stated "Returns ENOSYS" — that was
/// stale: the body has called `sched_proc()` since 2026-06-16. The doc is
/// now aligned with the actual implementation (Pattern 11: 设计与实现一致).
fn dispatch_schedule(
    caller_nr: crate::proc::ProcNr,
    proc_table: &mut ProcessTable,
    msg: &Message,
    priv_table: &PrivTable,
) -> KcallResult {
    // FIX-25: Use `caller_has_sys_proc_with_table` (consults the caller-provided
    // `priv_table`) instead of the legacy `caller_has_sys_proc` (which builds a
    // fresh empty `PrivTable::new()` internally and always returns false,
    // breaking all privileged callers — same latent bug as `dispatch_privctl`).
    if !crate::syscall_clock::caller_has_sys_proc_with_table(
        proc_table.get(caller_nr).expect("dispatch_schedule: caller slot must exist"),
        priv_table,
    ) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_schedule.c:14-27 — extract parameters from mess_lsys_krn_schedule.
    // IMPORTANT: Do NOT use the M1 overlay here. The C struct layout is:
    //   endpoint@0, quantum@4, priority@8, cpu@12, niced@16
    // while MessageM1 has m1p1@16 (would read `niced` as `cpu`) — a P1
    // field-mapping bug. Always use the dedicated `MessLsysKrnSchedule`.
    msg.debug_check_m_type_any(&[Syscall::Schedule as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let sched = unsafe { msg.m_u.m_lsys_krn_schedule };
    let endpoint = sched.endpoint;
    let _quantum = sched.quantum;
    let _priority = sched.priority;
    let _cpu = sched.cpu;

    // C: do_schedule.c:14-15 — endpoint_to_nr lookup.
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpoint)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };
    // NK4-C 1.9 取证探针（task1-close 裁决删除）：SYS_SCHEDULE 全轨迹——
    // s14b/c 尾态 RS runnable=yes queued=no 的「幽灵 CPU 队列」假设判别
    // （p_sched.cpu 被迁到非 BSP 即成永不可挑）。
    #[cfg(not(feature = "mock"))]
    {
        use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static SCHED_N: AtomicUsize = AtomicUsize::new(0);
        if SCHED_N.fetch_add(1, AtomicOrd::Relaxed) < 16 {
            C0::write_str("nk4a: schedctl caller=");
            C0::write_hex(caller_nr.0 as u64);
            C0::write_str(" tgt=");
            C0::write_hex(target_nr.0 as u64);
            C0::write_str(" cpu=");
            C0::write_hex(sched.cpu as u64);
            C0::write_str(" prio=");
            C0::write_hex(sched.priority as u64);
            C0::write_str("\n");
        }
    }

    // C: do_schedule.c:18-19 — `caller != p->p_scheduler` check.
    // In Rust: target.scheduler is Option<ProcNr>; None matches C's
    // `p_scheduler == NULL` (kernel default), which allows any caller.
    // Some(scheduler_nr) must equal caller_nr to pass.
    let target = match proc_table.get(target_nr) {
        Some(p) => p,
        None => return KcallResult::Ok(EINVAL),
    };
    let allowed = match target.p_sched.scheduler {
        None => true, // C: p_scheduler == NULL → always allowed
        Some(sched_nr) => sched_nr == caller_nr,
    };
    if !allowed {
        return KcallResult::Ok(EPERM);
    }

    // Apply scheduling parameters via sched_proc (per-CPU scheduling queue).
    // C: do_schedule.c:21-25 — sched_proc(p, priority, quantum, cpu, niced).
    // C: do_schedule.c:27 — `niced = !!(m_ptr->m_lsys_krn_schedule.niced)`:
    // the SYS_SCHEDULE message itself is the data source (C has no SYS_NICE
    // kernel call — only a 2005 changelog mention in system.h:12). The
    // kernel's own SYS_SCHEDCTL path passes FALSE (do_schedctl.c:37).
    let niced = sched.niced != 0;

    // Design decision §3.8 (11-scheduling-primitives.md): convert C's i32 -1 sentinel
    // ("keep current") to Option. Negative values other than -1 are rejected
    // early to match C semantics (system.c:644-648).
    //
    // R-16-fix (2026-08-12): The original `v as u8` silently truncated
    // priorities ≥ 256. Combined with the downstream `sched_proc` check
    // (`v > MIN_USER_Q`), this created a privilege escalation: `priority = 256`
    // truncated to `0` (TASK_Q, highest priority) and passed validation.
    // Now we validate the full C range (`{-1} ∪ [0, NR_SCHED_QUEUES]`) before
    // the cast, matching C: system.c:644-645.
    let priority_opt = match sched.priority {
        -1 => None,
        v if v >= 0 && v <= crate::proc::priority::NR_SCHED_QUEUES as i32 => {
            Some(v as u8)  // Safe: v ∈ [0, 16], fits in u8
        }
        _ => return KcallResult::Ok(EINVAL), // priority < -1 || priority > NR_SCHED_QUEUES
    };
    let quantum_opt = match sched.quantum {
        -1 => None,
        v if v >= 1 => Some(v as u32),
        _ => return KcallResult::Ok(EINVAL), // quantum < 1 && != -1
    };
    let cpu_opt = if sched.cpu == -1 { None } else { Some(sched.cpu as u32) };

    // D-52: sched_proc takes the table (scheduler-aware rts_set/rts_unset
    // dequeue/re-enqueue the process around the parameter update — C
    // system.c:671-698 RTS_SET/RTS_UNSET semantics).
    match crate::sched::sched_proc(
        proc_table,
        target_nr,
        crate::sched::SchedParams { priority: priority_opt, quantum: quantum_opt, cpu: cpu_opt, niced },
    ) {
        Ok(()) => KcallResult::Ok(0),
        Err(e) => KcallResult::Ok(e.to_errno().to_i32()), // D2: ToErrno
    }
}

/// Copy a structure from the caller's user space to a kernel stack buffer.
///
/// Helper for SYS_PRIV_ADD_IO/ADD_MEM/ADD_IRQ/SET_SYS/UPDATE_SYS sub-commands
/// that need to `data_copy` argument structures from the caller's address space.
///
/// C: `data_copy(caller->p_endpoint, arg_ptr, KERNEL, &local_buf, sizeof(buf))`
///    — do_privctl.c:95-98, 201-202, 211-214, 227-228, 258-261
///
/// Returns `CrossSpaceResult` so callers can map outcomes to `KcallResult`:
/// - `Completed(Ok(()))` → proceed with the copied data
/// - `Completed(Err(_))` → `EFAULT`
/// - `Suspended(_)` → `VmSuspend` (caller will be retried after VM handles fault)
fn copy_struct_from_user(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    user_ptr: u64,
    kernel_buf: *mut u8,
    bytes: usize,
) -> crate::vm::CrossSpaceResult {
    use minix_types::VirBytes;
    use crate::vm::AddressRef;

    let caller_endpt = proc_table
        .get(caller_nr)
        .map(|p| p.p_endpoint)
        .expect("copy_struct_from_user: caller slot must exist");
    let caller_cr3 = proc_table
        .get(caller_nr)
        .map(|p| p.p_seg.phys_root)
        .expect("copy_struct_from_user: caller slot must exist");
    let src = AddressRef::Process {
        endpoint: caller_endpt,
        offset: VirBytes(user_ptr),
    };
    // NK4-C F10c：kernel_buf 是内核栈上的局部变量，VA 在 higher-half 段
    // [KERN_VIRT_BASE, KERNEL_DIRECT_MAP_BASE)——`virt_to_phys` 的 DM 算术
    // 在该段越界得到错误 PA（`0xffff7fff...` 形态）→ `Completed(Err(_))`
    // → EFAULT。改为 AddressRef::Process：caller 的 CR3 同时映射了内核
    // higher-half（SYSCALL 不切页表），`resolve_physical` 走真实页表得正确
    // PA，`kernel_phys_to_virt` 再经 DM 访问同一物理内存。语义等价 C
    // `vircopyf(VMIO_READ, user_ptr, size, &priv)`（&priv 是内核栈地址）。
    let dst = AddressRef::Process {
        endpoint: caller_endpt,
        offset: VirBytes(kernel_buf as u64),
    };
    let proc_cr3 = |pt: &crate::proc_table::ProcessTable, endpt: Endpoint| {
            if endpt == caller_endpt {
                Some(caller_cr3)
            } else {
                pt.endpoint_to_nr(endpt)
                    .and_then(|nr| pt.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };
    crate::cross_space::data_copy_vmcheck(caller_nr, proc_table, src, dst, bytes, proc_cr3)
}

/// Clear all IPC references for a target process.
///
/// C: `clear_ipc_refs()` — system.c:577-607
///
/// Called by `SYS_PRIV_CLEAR_IPC_REFS` (do_privctl.c:81-84) and
/// `SYS_STATE_CLEAR_IPC_REFS` (do_statectl.c:22-26).
///
/// # Semantics
///
/// 1. Clears `s_notify_pending` and `s_asyn_pending` bits for the target's
///    `priv_id` across **all** privilege slots (so no process has pending
///    notifications or async messages targeting the cleaned-up process).
/// 2. For each process blocked on the target's endpoint (`P_BLOCKEDON(rp) ==
///    target.p_endpoint`), clears `RTS_SENDING | RTS_RECEIVING` to make it
///    runnable.
///
/// # Design gap (return value register)
///
/// C sets `rp->p_reg.retreg = caller_ret` before clearing RTS flags, so the
/// woken process sees `EDEADSRCDST` as its IPC return value. Rust does not
/// model the register save area in `KProcess` (the trap frame lives on the
/// kernel stack during context switches and is not accessible here). The
/// woken process will be rescheduled but may not see the error code in its
/// return register. This is the same gap that exists in the normal IPC
/// wake-up path (see `IpcEngine::send` / `receive`). A `pending_ipc_error`
/// field on `KProcess` would close this gap — deferred to a future phase.
///
/// # Async send cancellation
///
/// C calls `has_pending_asend` + `cancel_async` in a loop to cancel pending
/// async sends to the target. Rust's `senda` does not persist the async
/// table (it tries immediate delivery and returns undelivered entries to
/// the caller), so there are no persistent async sends to cancel. The
/// `s_asyn_pending` bit clearing in step 1 achieves the equivalent effect.
pub(crate) fn clear_ipc_refs(
    proc_table: &mut ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    target_nr: ProcNr,
    _error_code: i32,
) {
    use crate::proc::RtsFlagsBits;
    use crate::kpriv::NR_SYS_PROCS;

    // Get target's endpoint and priv_id.
    let (target_ep, target_priv_id) = match proc_table.get(target_nr) {
        Some(p) => (p.p_endpoint, p.priv_id),
        None => return,
    };
    let target_priv_id = match target_priv_id {
        Some(id) => id,
        None => return,
    };

    // Step 1: Clear pending notification/async bits for target's priv_id
    // in all privilege slots.
    // C: system.c:596-599 — unset_sys_bit(priv(rp)->s_notify_pending, priv(rc)->s_id)
    //                      unset_sys_bit(priv(rp)->s_asyn_pending, priv(rc)->s_id)
    if (target_priv_id as u64) < 64 {
        let mask = !(1u64 << target_priv_id);
        for i in 0..NR_SYS_PROCS as u16 {
            if let Some(priv_) = priv_table.get_mut(i) {
                priv_.signals.s_notify_pending &= mask;
                priv_.signals.s_asyn_pending &= mask;
            }
        }
    }

    // Step 2: Wake up processes blocked on target's endpoint.
    // C: system.c:601-605 — if (P_BLOCKEDON(rp) == rc->p_endpoint) {
    //   rp->p_reg.retreg = caller_ret; clear_ipc(rp); }
    for proc in proc_table.iter_mut() {
        // C: isemptyp(rp) — skip free slots
        if proc.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE) {
            continue;
        }
        if proc.blocked_on() == Some(target_ep) {
            // NK4-C 449-livelock 取证探针（task1-close 裁决删除）：裸清现场
            // ——谁被清、目标端点、清前 flags。裸清不摘 caller_q、不入队
            // （C clear_ipc = 摘链 + RTS_UNSET 入队半，system.c:601-605），
            // 是幽灵队列条目的候选来源。
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                static CIR_N: AtomicUsize = AtomicUsize::new(0);
                if CIR_N.fetch_add(1, AtomicOrd::Relaxed) < 24 {
                    use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                    C0::write_str("nk4a: cir tgt_ep=");
                    C0::write_hex(target_ep.0 as u64);
                    C0::write_str(" nr=");
                    C0::write_hex(proc.p_nr.0 as u64);
                    C0::write_str(" fl=");
                    C0::write_hex(proc.p_rts_flags.load() as u64);
                    C0::write_str("\n");
                }
            }
            // C: clear_ipc(rp) — RTS_UNSET(rp, RTS_SENDING | RTS_RECEIVING)
            proc.p_rts_flags.clear(RtsFlagsBits::SENDING | RtsFlagsBits::RECEIVING);
            // C: rp->p_reg.retreg = caller_ret — see design gap note above.
            // The _error_code parameter is accepted for API completeness
            // but not yet applied to the register save area.
        }
    }
}

/// Remove a process from its send target's caller queue (kernel-internal).
///
/// C: `static void clear_ipc(struct proc *rc)` — system.c:509-535.
///
/// If the target process `rc` is currently in `RTS_SENDING` state (blocked
/// sending to `p_sendto_e`), it is enqueued on the destination's
/// `p_caller_q`. This function walks that queue and removes `rc`, then
/// clears `RTS_SENDING`. It also unconditionally clears `RTS_RECEIVING`.
///
/// The queue is the C-isomorphic intrusive FIFO: links resolve slot
/// indices (`ipc::caller_q_remove_by_nr` — same chain walk as C's
/// `p_caller_q`/`p_q_link` loop, system.c:520-531).
///
/// # Arguments
/// - `proc_table`: mutable borrow so we can touch both the target and its
///   send destination.
/// - `target_nr`: the process being cleaned up.
pub(crate) fn clear_ipc(proc_table: &mut ProcessTable, target_nr: ProcNr) {
    use crate::proc::RtsFlagsBits;

    // C: system.c:516 — if (RTS_ISSET(rc, RTS_SENDING))
    let is_sending = proc_table
        .get(target_nr)
        .map(|p| p.p_rts_flags.is_set(RtsFlagsBits::SENDING))
        .unwrap_or(false);

    if is_sending {
        // C: system.c:519 — okendpt(rc->p_sendto_e, &target_proc)
        let sendto_ep = proc_table
            .get(target_nr)
            .map(|p| p.p_sendto_e)
            .unwrap_or(Endpoint::NONE);

        // C: system.c:520-531 — walk proc_addr(target_proc)->p_caller_q
        // looking for `rc`, unlink if found.
        if let Some(dst_nr) = proc_table.endpoint_to_nr(sendto_ep) {
            // caller_q_remove_by_nr unlinks the first entry matching
            // `target_nr`. C's queue can only hold each sender once
            // (asserted in `send()`), so a single removal is sufficient.
            if let Some(dst_idx) = crate::proc_table::nr_to_idx(dst_nr) {
                crate::ipc::caller_q_remove_by_nr(
                    proc_table.procs_slice_mut(),
                    dst_idx,
                    target_nr,
                );
            }
        }

        // C: system.c:532 — RTS_UNSET(rc, RTS_SENDING)
        proc_table.rts_unset(target_nr, RtsFlagsBits::SENDING);
    }

    // C: system.c:534 — RTS_UNSET(rc, RTS_RECEIVING)
    proc_table.rts_unset(target_nr, RtsFlagsBits::RECEIVING);
}

/// Remove a process from the VM request queue (kernel-internal).
///
/// C: `static void clear_memreq(struct proc *rp)` — system.c:488-504.
///
/// If `rp` has `RTS_VMREQUEST` set, walk the global `vmrequest` linked list
/// and unlink `rp`, then clear `RTS_VMREQUEST`. If `RTS_VMREQUEST` is not
/// set, this is a no-op.
///
/// Rust replaces C's `p_vmrequest.nextrequestor` linked list with the
/// `VmRequestQueue` stored on `ProcessTable`.
///
/// # Arguments
/// - `proc_table`: mutable borrow so we can walk the queue and clear the
///   target's flag.
/// - `target_nr`: the process being cleaned up.
pub(crate) fn clear_memreq(proc_table: &mut ProcessTable, target_nr: ProcNr) {
    use crate::proc::RtsFlagsBits;

    // C: system.c:492 — if (!RTS_ISSET(rp, RTS_VMREQUEST)) return;
    let is_vm_req = proc_table
        .get(target_nr)
        .map(|p| p.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST))
        .unwrap_or(false);
    if !is_vm_req {
        return;
    }

    // C: system.c:495-501 — walk vmrequest linked list, unlink rp.
    // Rust: VmRequestQueue stores head: Option<ProcNr>; each process has
    // p_next_requestor: Option<ProcNr>. We traverse and unlink manually
    // to match C's pointer-to-pointer pattern.
    let mut current = proc_table.vm_request_queue().head();
    let mut prev_nr: Option<ProcNr> = None;

    while let Some(nr) = current {
        let next = proc_table
            .get(nr)
            .map(|p| p.p_next_requestor)
            .unwrap_or(None);

        if nr == target_nr {
            // Unlink target_nr from the queue.
            if let Some(pnr) = prev_nr {
                if let Some(prev) = proc_table.get_mut(pnr) {
                    prev.p_next_requestor = next;
                }
            } else {
                proc_table.vm_request_queue_mut().set_head(next);
            }
            if let Some(target) = proc_table.get_mut(target_nr) {
                target.p_next_requestor = None;
            }
            break;
        }

        prev_nr = Some(nr);
        current = next;
    }

    // C: system.c:503 — RTS_UNSET(rp, RTS_VMREQUEST)
    // Also clear p_vm_suspend to maintain the invariant
    // `RTS_VMREQUEST <==> p_vm_suspend.is_some()`.
    if let Some(target) = proc_table.get_mut(target_nr) {
        target.clear_vm_suspend();
    }
}

/// Release a process's address space (kernel-internal).
///
/// C: `void release_address_space(struct proc *pr)` — arch/i386/memory.c:986.
///
/// In C this just clears `pr->p_seg.p_cr3_v = NULL` (the kernel-virtual
/// alias of the page directory). The actual page-table reclamation is
/// performed by VM via a separate `SYS_CLEAR` → `clear_endpoint` →
/// `clear_memreq` flow; the kernel itself does not free page tables.
///
/// Rust: clear `p_seg.virt_root` (the alias of `p_cr3_v`) to `None`. The
/// physical root (`phys_root`) is preserved so VM can still identify the
/// page table being released.
pub(crate) fn release_address_space(target: &mut KProcess) {
    // C: pr->p_seg.p_cr3_v = NULL
    target.p_seg.virt_root = None;
}

/// Clean up the slot of a process (kernel-internal).
///
/// C: `void clear_endpoint(struct proc *rc)` — system.c:540-572.
///
/// Sequence:
/// 1. panic if slot is empty (defensive — should never happen)
/// 2. Set `RTS_NO_ENDPOINT` so the process is no longer scheduled
/// 3. If SYS_PROC, clear `s_asynsize`
/// 4. `clear_ipc(rc)` — remove from send queue, clear SENDING/RECEIVING
/// 5. `clear_ipc_refs(rc, EDEADSRCDST)` — wake processes blocked on rc
/// 6. `clear_memreq(rc)` — remove from VM request queue
///
/// # Arguments
/// - `proc_table`: mutable borrow for queue/flag manipulation.
/// - `priv_table`: mutable borrow for `s_asynsize` clear.
/// - `target_nr`: the process being cleaned up.
pub(crate) fn clear_endpoint(
    proc_table: &mut ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    target_nr: ProcNr,
) {
    use crate::proc::RtsFlagsBits;

    // C: system.c:543 — if(isemptyp(rc)) panic(...)
    // Rust: debug_assert (release builds skip the check, matching C's
    // panic-on-corruption intent without paying the cost in hot paths).
    debug_assert!(
        proc_table
            .get(target_nr)
            .map(|p| !p.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE))
            .unwrap_or(false),
        "clear_endpoint: empty process slot {:?}",
        target_nr
    );

    // C: system.c:550-551 — RTS_SET(rc, RTS_NO_ENDPOINT)
    proc_table.rts_set(target_nr, RtsFlagsBits::NO_ENDPOINT);

    // C: system.c:552-555 — if (priv(rc)->s_flags & SYS_PROC) priv(rc)->s_asynsize = 0
    if let Some(target) = proc_table.get(target_nr)
        && let Some(priv_id) = target.priv_id
            && let Some(kpriv) = priv_table.get_mut(priv_id)
                && kpriv.is_sys_proc() {
                    kpriv.signals.s_asynsize = 0;
                }

    // C: system.c:560 — clear_ipc(rc)
    clear_ipc(proc_table, target_nr);

    // C: system.c:566 — clear_ipc_refs(rc, EDEADSRCDST)
    clear_ipc_refs(proc_table, priv_table, target_nr, EDEADSRCDST);

    // C: system.c:571 — clear_memreq(rc)
    clear_memreq(proc_table, target_nr);
}

/// Dispatch SYS_PRIVCTL.
///
/// C: `do_privctl()` — do_privctl.c:26-275
///
/// Privilege control: set/clear privilege flags, add I/O/memory/IRQ
/// ranges, and manage process privilege structures.
///
/// # Message fields (C: mess_lsys_krn_sys_privctl, ipc.h:1227-1235)
///
/// Maps to M1 overlay:
/// - `m1i1` = request (SYS_PRIV_* sub-command)
/// - `m1i2` = endpt (target endpoint)
/// - `m1p1` = arg_ptr (pointer to argument data in caller's address space)
/// - `m1p2` = phys_start (physical address start, QUERY_MEM only)
/// - `m1p3` = phys_len (physical address length, QUERY_MEM only)
///
/// # Sub-commands implemented (FIX-25, Phase 5+6)
///
/// - `SYS_PRIV_ALLOW` (1): clear RTS_NO_PRIV on target
/// - `SYS_PRIV_DISALLOW` (2): set RTS_NO_PRIV on target
/// - `SYS_PRIV_SET_SYS` (3): allocate priv slot + set defaults + optional update
/// - `SYS_PRIV_SET_USER` (4): link target to USER_PRIV_ID
/// - `SYS_PRIV_ADD_IO` (5): data_copy io_range from user + add_io
/// - `SYS_PRIV_ADD_MEM` (6): data_copy mem_range from user + add_mem
/// - `SYS_PRIV_ADD_IRQ` (7): data_copy irq from user + add_irq
/// - `SYS_PRIV_QUERY_MEM` (8): check if target may map physical range
/// - `SYS_PRIV_UPDATE_SYS` (9): data_copy priv struct + update_priv
///   (field copies + fill_sendto_mask target-mask maintenance)
/// - `SYS_PRIV_YIELD` (10): clear RTS_NO_PRIV on target + set on caller
/// - `SYS_PRIV_CLEAR_IPC_REFS` (11): clear pending IPC for target
///
/// C: do_privctl.c:26-275
fn dispatch_privctl(
    caller_nr: crate::proc::ProcNr,
    proc_table: &mut ProcessTable,
    msg: &Message,
    priv_table: &mut crate::kpriv::PrivTable,
    clock_state: &mut ClockState,
) -> KcallResult {
    // C: do_privctl.c:47 — caller must be SYS_PROC
    //
    // FIX-25: Use `caller_has_sys_proc_with_table` (which consults the
    // caller-provided `priv_table`) instead of the legacy
    // `caller_has_sys_proc` (which builds a fresh empty `PrivTable::new()`
    // internally and always returns false, breaking all privileged callers).
    if !crate::syscall_clock::caller_has_sys_proc_with_table(
        proc_table.get(caller_nr).expect("dispatch_privctl: caller slot must exist"),
        priv_table,
    ) {
        return KcallResult::Ok(EPERM);
    }

    // Parse message fields via M1 overlay.
    // C: mess_lsys_krn_sys_privctl { request, endpt, arg_ptr, phys_start, phys_len }
    // Maps to M1: m1i1=request, m1i2=endpt, m1p1=arg_ptr, m1p2=phys_start, m1p3=phys_len
    msg.debug_check_m_type_any(&[Syscall::Privctl as i32]);
    // SAFETY: m_type verified above (debug) / guaranteed by dispatch (release).
    let m1 = unsafe { &msg.m_u.m_m1 };
    let request = m1.m1i1;
    let endpt_raw = m1.m1i2;
    let arg_ptr = m1.m1p1;  // User-space pointer to argument data
    let phys_start = m1.m1p2;
    let phys_len = m1.m1p3;

    // C: do_privctl.c:48-51 — resolve endpoint (SELF → caller)
    let target_ep = if endpt_raw == minix_types::Endpoint::SELF.0 {
        proc_table
            .get(caller_nr)
            .map(|c| c.p_endpoint)
            .expect("dispatch_privctl: caller slot must exist")
            .0
    } else {
        endpt_raw
    };
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(target_ep)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // Pre-compute target state needed by multiple sub-commands.
    // C: RTS_ISSET(rp, RTS_NO_PRIV) and priv(rp)->s_proc_nr / s_id.
    let (target_has_no_priv, target_priv_id) = {
        let target = match proc_table.get(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        (
            target.p_rts_flags.is_set(crate::proc::RtsFlagsBits::NO_PRIV),
            target.priv_id,
        )
    };

    // C: do_privctl.c:54 — switch on request
    let dispatch_result = match request {
        // SYS_PRIV_ALLOW = 1: Allow process to run.
        // C: do_privctl.c:56-64 — check RTS_NO_PRIV set + s_proc_nr != NONE,
        // then RTS_UNSET(rp, RTS_NO_PRIV)
        1 => privctl_allow(proc_table, priv_table, target_nr),

        // SYS_PRIV_DISALLOW = 2: Disallow process from running.
        // C: do_privctl.c:75-79 — if RTS_NO_PRIV already set, EPERM;
        // else RTS_SET(rp, RTS_NO_PRIV)
        2 => privctl_disallow(proc_table, target_nr),

        // SYS_PRIV_YIELD = 10: Allow process to run and suspend the caller.
        // C: do_privctl.c:66-73 — check target has RTS_NO_PRIV + s_proc_nr,
        // then RTS_SET(caller, RTS_NO_PRIV) + RTS_UNSET(rp, RTS_NO_PRIV)
        10 => privctl_yield(caller_nr, proc_table, priv_table, target_nr),

        // SYS_PRIV_QUERY_MEM = 8: Check if process may map physical range.
        // C: do_privctl.c:232-251 — check s_mem_tab for containing range
        8 => privctl_query_mem(proc_table, priv_table, target_nr, phys_start, phys_len),

        // SYS_PRIV_SET_USER = 4: Link target to USER_PRIV_ID.
        // C: do_privctl.c:176-185 — check RTS_NO_PRIV, then
        // priv(rp) = priv_addr(USER_PRIV_ID)
        4 => privctl_set_user(proc_table, priv_table, target_nr),

        // SYS_PRIV_SET_SYS = 3: Set privilege structure for a blocked system process.
        // C: do_privctl.c:86-174
        3 => privctl_set_sys(caller_nr, proc_table, priv_table, clock_state, target_nr, arg_ptr, target_has_no_priv),

        // SYS_PRIV_ADD_IO = 5: Add I/O port range to target's privilege.
        // C: do_privctl.c:187-204
        5 => privctl_add_io(caller_nr, proc_table, priv_table, target_priv_id, target_has_no_priv, arg_ptr),

        // SYS_PRIV_ADD_MEM = 6: Add memory range to target's privilege.
        // C: do_privctl.c:206-216
        6 => privctl_add_mem(caller_nr, proc_table, priv_table, target_priv_id, target_has_no_priv, arg_ptr),

        // SYS_PRIV_ADD_IRQ = 7: Add IRQ to target's privilege.
        // C: do_privctl.c:218-230
        7 => privctl_add_irq(caller_nr, proc_table, priv_table, target_priv_id, target_has_no_priv, arg_ptr),

        // SYS_PRIV_UPDATE_SYS = 9: Update existing privilege structure.
        // C: do_privctl.c:253-268
        9 => privctl_update_sys(caller_nr, proc_table, priv_table, target_priv_id, arg_ptr),

        // SYS_PRIV_CLEAR_IPC_REFS = 11: Clear pending IPC for target.
        // C: do_privctl.c:81-84 — clear_ipc_refs(rp, EDEADSRCDST)
        11 => {
            clear_ipc_refs(proc_table, priv_table, target_nr, EDEADSRCDST);
            KcallResult::Ok(0)
        }

        // Unknown request
        // C: do_privctl.c:270-273 — printf + return EINVAL
        _ => KcallResult::Ok(EINVAL),
    };
    // NK4-C 1.3 取证探针（task1-close 裁决删除）：boot 尾停点裁决——
    // s13a tail-dump 坐死 10 个服务器终态 flags=0x80（NO_PRIV 未清），
    // 而 RS 侧 post-privctl 照打（`?` 未拦截）。本探针打出每次 privctl
    // 分发的 request/target/结果/目标终态 flags，分清「Allow 返回 EPERM
    // 但错误没传到 RS」与「Allow 成功但事后被重设」两种形态。
    #[cfg(not(feature = "mock"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        static PCTL_LOG: AtomicUsize = AtomicUsize::new(0);
        if let KcallResult::Ok(code) = dispatch_result {
            if (1..=4).contains(&request)
                && PCTL_LOG.fetch_add(1, AtomicOrd::Relaxed) < 24
            {
                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                C0::write_str("nk4a: pctl req=");
                C0::write_hex(request as u64);
                C0::write_str(" tgt=");
                C0::write_hex(target_nr.0 as u64);
                C0::write_str(" r=");
                C0::write_hex(code as u64);
                let fl = proc_table
                    .get(target_nr)
                    .map_or(0xFFFF_FFFF, |p| p.p_rts_flags.load());
                C0::write_str(" tf=0x");
                C0::write_hex(fl as u64);
                C0::write_str("\n");
            }
        }
    }
    dispatch_result
}

/// SYS_PRIV_ALLOW — clear RTS_NO_PRIV after eligibility checks.
///
/// C: do_privctl.c:56-64.
fn privctl_allow(
    proc_table: &mut ProcessTable,
    priv_table: &crate::kpriv::PrivTable,
    target_nr: ProcNr,
) -> KcallResult {
    // Pre-checks with immutable borrows so the target slot is free for the
    // scheduler-aware rts_unset below (it needs &mut self to enqueue).
    let (has_no_priv, has_priv) = match proc_table.get(target_nr) {
        Some(p) => (
            p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::NO_PRIV),
            p.priv_id
                .and_then(|id| priv_table.get(id))
                .map(|kp| kp.identity.s_proc_nr.is_some())
                .unwrap_or(false),
        ),
        None => return KcallResult::Ok(EINVAL),
    };
    // C: if (!RTS_ISSET(rp, RTS_NO_PRIV) || priv(rp)->s_proc_nr == NONE)
    //    return(EPERM);
    if !has_no_priv || !has_priv {
        return KcallResult::Ok(EPERM);
    }
    // C: RTS_UNSET(rp, RTS_NO_PRIV) — clear + enqueue when the process
    // transitions non-runnable → runnable. A raw flag clear left the woken
    // server runnable but off the run queue forever (real machine NK4-C 1.3:
    // 8 servers runnable=yes queued=no after Allow returned OK).
    proc_table.rts_unset(target_nr, crate::proc::RtsFlagsBits::NO_PRIV);
    KcallResult::Ok(0)
}

/// SYS_PRIV_DISALLOW — set RTS_NO_PRIV (refuse unless already set → EPERM).
///
/// C: do_privctl.c:75-79.
fn privctl_disallow(proc_table: &mut ProcessTable, target_nr: ProcNr) -> KcallResult {
    // C: if (RTS_ISSET(rp, RTS_NO_PRIV)) return(EPERM);
    let has_no_priv = match proc_table.get(target_nr) {
        Some(p) => p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::NO_PRIV),
        None => return KcallResult::Ok(EINVAL),
    };
    if has_no_priv {
        return KcallResult::Ok(EPERM);
    }
    // C: RTS_SET(rp, RTS_NO_PRIV) — set + dequeue when the process was
    // runnable (scheduler-aware mirror in proc_table::rts_set).
    proc_table.rts_set(target_nr, crate::proc::RtsFlagsBits::NO_PRIV);
    KcallResult::Ok(0)
}

/// SYS_PRIV_YIELD — allow the target and suspend the caller in its place.
///
/// C: do_privctl.c:66-73 — check target has RTS_NO_PRIV + s_proc_nr,
/// then RTS_SET(caller, RTS_NO_PRIV) + RTS_UNSET(rp, RTS_NO_PRIV).
fn privctl_yield(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    priv_table: &crate::kpriv::PrivTable,
    target_nr: ProcNr,
) -> KcallResult {
    // Pre-checks with immutable borrows so the slots stay free for the
    // scheduler-aware rts_set/rts_unset below.
    let (has_no_priv, has_priv) = match proc_table.get(target_nr) {
        Some(p) => (
            p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::NO_PRIV),
            p.priv_id
                .and_then(|id| priv_table.get(id))
                .map(|kp| kp.identity.s_proc_nr.is_some())
                .unwrap_or(false),
        ),
        None => return KcallResult::Ok(EINVAL),
    };
    // C: if (!RTS_ISSET(rp, RTS_NO_PRIV) || priv(rp)->s_proc_nr == NONE)
    //    return(EPERM);
    if !has_no_priv || !has_priv {
        return KcallResult::Ok(EPERM);
    }
    // C: RTS_SET(caller, RTS_NO_PRIV) — suspend caller (dequeue if runnable).
    proc_table.rts_set(caller_nr, crate::proc::RtsFlagsBits::NO_PRIV);
    // C: RTS_UNSET(rp, RTS_NO_PRIV) — allow target (enqueue if now runnable).
    proc_table.rts_unset(target_nr, crate::proc::RtsFlagsBits::NO_PRIV);
    KcallResult::Ok(0)
}

/// SYS_PRIV_QUERY_MEM — may the target map the physical range
/// `[phys_start, phys_start+phys_len)`? Checks the priv `s_mem_tab`.
///
/// C: do_privctl.c:232-251.
fn privctl_query_mem(
    proc_table: &ProcessTable,
    priv_table: &crate::kpriv::PrivTable,
    target_nr: ProcNr,
    phys_start: u64,
    phys_len: u64,
) -> KcallResult {
    // C: addr = phys_start; limit = addr + phys_len - 1
    let addr = phys_start;
    let limit = if phys_len == 0 {
        0u64
    } else {
        match phys_start.checked_add(phys_len - 1) {
            Some(l) => l,
            None => return KcallResult::Ok(EPERM), // overflow
        }
    };
    // C: if (limit < addr) return EPERM
    if limit < addr {
        return KcallResult::Ok(EPERM);
    }
    // Get target's priv and check s_mem_tab
    let target = proc_table.get(target_nr);
    match target {
        Some(p) => {
            let allowed = p.priv_id
                .and_then(|id| priv_table.get(id))
                .map(|kp| {
                    // C: for i in 0..s_nr_mem_range:
                    //   if addr >= s_mem_tab[i].base && limit <= s_mem_tab[i].limit
                    //     return OK
                    for i in 0..kp.mem.s_nr_mem_range as usize {
                        let entry = &kp.mem.s_mem_tab[i];
                        if addr >= entry.base && limit <= entry.limit {
                            return true;
                        }
                    }
                    false
                })
                .unwrap_or(false);
            if allowed {
                KcallResult::Ok(0)
            } else {
                KcallResult::Ok(EPERM)
            }
        }
        None => KcallResult::Ok(EPERM),
    }
}

/// SYS_PRIV_SET_USER — link the target to the shared USER_PRIV_ID slot.
///
/// C: do_privctl.c:176-185 — check RTS_NO_PRIV, then
/// priv(rp) = priv_addr(USER_PRIV_ID).
fn privctl_set_user(
    proc_table: &mut ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    target_nr: ProcNr,
) -> KcallResult {
    let target = proc_table.get_mut(target_nr);
    match target {
        Some(p) => {
            if !p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::NO_PRIV) {
                return KcallResult::Ok(EPERM);
            }
            // C: priv(rp) = priv_addr(USER_PRIV_ID)
            // Link target's priv_id to USER_PRIV_ID
            p.priv_id = Some(crate::kpriv::USER_PRIV_ID);
            // Update USER_PRIV_ID's s_proc_nr to point to target
            if let Some(user_priv) = priv_table.get_mut(crate::kpriv::USER_PRIV_ID) {
                user_priv.identity.s_proc_nr = Some(target_nr);
            }
            KcallResult::Ok(0)
        }
        None => KcallResult::Ok(EINVAL),
    }
}

/// SYS_PRIV_SET_SYS — allocate and initialise a privilege structure.
///
/// C: do_privctl.c:86-174 — determine priv_id (static vs dynamic),
/// get_priv, restore s_id/s_proc_nr, reset the alarm timer, clear pending
/// IPC, apply DSRV_* defaults, fill the send-to mask, then override with
/// user-provided settings when `arg_ptr` carries a PrivUpdateRequest.
#[allow(clippy::too_many_arguments)]
fn privctl_set_sys(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    clock_state: &mut ClockState,
    target_nr: ProcNr,
    arg_ptr: u64,
    target_has_no_priv: bool,
) -> KcallResult {
    // C: do_privctl.c:88 — target must have RTS_NO_PRIV
    if !target_has_no_priv {
        return KcallResult::Ok(EPERM);
    }

    // C: do_privctl.c:91-104 — determine priv_id
    // If arg_ptr provided and DYN_PRIV_ID not set, use static id from request.
    // Else use NULL_PRIV_ID for dynamic allocation.
    let priv_id = if arg_ptr != 0 {
        let mut req = crate::kpriv::PrivUpdateRequest::new();
        let copy_result = copy_struct_from_user(
            caller_nr, proc_table, arg_ptr,
            &mut req as *mut _ as *mut u8,
            core::mem::size_of::<crate::kpriv::PrivUpdateRequest>(),
        );
        match copy_result {
            crate::vm::CrossSpaceResult::Completed(Ok(())) => req,
            crate::vm::CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            crate::vm::CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
        }
    } else {
        crate::kpriv::PrivUpdateRequest::new()
    };

    // C: do_privctl.c:101-103 — static id if not DYN_PRIV_ID
    let alloc_id = if !crate::capability::ProcessCapability::from_wire(priv_id.s_flags)
        .contains(crate::capability::ProcessCapability::DYN_PRIV_ID)
        && arg_ptr != 0
    {
        priv_id.s_id
    } else {
        crate::kpriv::NULL_PRIV_ID
    };

    // C: do_privctl.c:110-115 — get_priv(rp, priv_id)
    let allocated = priv_table.get_priv(target_nr, alloc_id);
    match allocated {
        Ok(actual_id) => {
            // C: system.c:298 — get_priv links both directions
            // (`rc->p_priv = sp`). `PrivTable::get_priv` can only
            // reach the priv slot (s_proc_nr), so the process-side
            // link happens here. Without it the target stays
            // priv-less from its own side: a later UPDATE_SYS or
            // GET_PRIV on this process would fail as if SET_SYS
            // never ran.
            if let Some(p) = proc_table.get_mut(target_nr) {
                p.priv_id = Some(actual_id);
            }
            // C: do_privctl.c:116-119 — restore s_id + s_proc_nr
            // (get_priv already sets s_proc_nr; s_id is the slot index)
            let target_ep = proc_table.get(target_nr)
                .map(|p| p.p_endpoint)
                .unwrap_or(minix_types::Endpoint::NONE);

            // C: do_privctl.c:127 — reset_kernel_timer(&priv(rp)->s_alarm_timer)
            // Chain-aware alarm reset: the priv slot may be recycled
            // with a stale node still linked into the clock chain, so
            // the local node clear in `reset_pending_ipc` (C:
            // `tmr_inittimer` semantics) must be preceded by the
            // chain unlink.
            crate::clock::reset_alarm_timer(priv_table, clock_state, actual_id);

            if let Some(priv_) = priv_table.get_mut(actual_id) {
                // C: do_privctl.c:121-131 — clear pending IPC state
                priv_.reset_pending_ipc();
                // C: do_privctl.c:133-164 — set defaults
                priv_.flags.s_flags = crate::capability::ProcessCapability::DSRV_F;
                priv_.init.s_init_flags = 0; // DSRV_I = 0
                priv_.ipc.s_trap_mask = crate::capability::TrapMask::ALL; // DSRV_T = ~0
                priv_.ipc.s_k_call_mask = crate::capability::KCallMask::ALL; // DSRV_KC = ALL_C
                priv_.signals.s_sig_mgr = minix_types::Endpoint::RS; // DSRV_SM = ROOT_SYS_PROC_NR
                priv_.signals.s_bak_sig_mgr = minix_types::Endpoint::NONE;
                priv_.reset_resources(target_ep);
            }

            // C: do_privctl.c:138-143 — default target mask: map =
            // DSRV_M (= ALL_M) expanded to every priv id, then
            // fill_sendto_mask — the association/self guards apply
            // and every send-capable target receives the reciprocal
            // bit (system.c:349-358). A raw `s_ipc_to = ALL` here
            // would also pre-authorize slots RS has not bound yet
            // and set the self bit, which C never does.
            priv_table.fill_sendto_mask(actual_id, crate::capability::IpcMask::ALL);

            // C: do_privctl.c:167-172 — override with user-provided settings
            if arg_ptr != 0
                && priv_table.update_priv(actual_id, &priv_id).is_err() {
                    return KcallResult::Ok(EINVAL);
                }
            // NK4-C 1.10y 取证探针（task1-close 裁决删除）：SET_SYS 落点
            // ——目标端点 + 覆盖后的生效 flags（sched setalarm EPERM 判定：
            // RS 下发的 SRV_F(0x12) 是否真的落到 priv 槽）。
            #[cfg(not(feature = "mock"))]
            {
                use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
                static SETSYS_N: AtomicUsize = AtomicUsize::new(0);
                if SETSYS_N.fetch_add(1, AtomicOrd::Relaxed) < 16 {
                    Console::write_str("nk4a: setsys tgt=");
                    Console::write_hex(target_nr.0 as u64);
                    Console::write_str(" req_fl=0x");
                    Console::write_hex(priv_id.s_flags as u64);
                    if let Some(kp) = priv_table.get(actual_id) {
                        Console::write_str(" eff_fl=0x");
                        Console::write_hex(kp.flags.s_flags.bits() as u64);
                    }
                    Console::write_str("\n");
                }
            }
            KcallResult::Ok(0)
        }
        Err(ENOSPC) => KcallResult::Ok(ENOSPC),
        Err(EINVAL) => KcallResult::Ok(EINVAL),
        Err(EBUSY) => KcallResult::Ok(EBUSY),
        Err(_) => KcallResult::Ok(EINVAL),
    }
}

/// SYS_PRIV_ADD_IO — add an I/O port range to the target's privilege.
///
/// C: do_privctl.c:187-204.
fn privctl_add_io(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    target_priv_id: Option<crate::kpriv::PrivId>,
    target_has_no_priv: bool,
    arg_ptr: u64,
) -> KcallResult {
    if target_has_no_priv {
        return KcallResult::Ok(EPERM);
    }
    let target_priv_id = match target_priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM),
    };
    let mut io_range = crate::kpriv::IoRange::new();
    let copy_result = copy_struct_from_user(
        caller_nr, proc_table, arg_ptr,
        &mut io_range as *mut _ as *mut u8,
        core::mem::size_of::<crate::kpriv::IoRange>(),
    );
    match copy_result {
        crate::vm::CrossSpaceResult::Completed(Ok(())) => {
            match priv_table.get_mut(target_priv_id) {
                Some(priv_) => {
                    match priv_.add_io(&io_range) {
                        Ok(()) => KcallResult::Ok(0),
                        Err(()) => KcallResult::Ok(ENOSPC),
                    }
                }
                None => KcallResult::Ok(EINVAL),
            }
        }
        crate::vm::CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        crate::vm::CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

/// SYS_PRIV_ADD_MEM — add a memory range to the target's privilege.
///
/// C: do_privctl.c:206-216.
fn privctl_add_mem(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    target_priv_id: Option<crate::kpriv::PrivId>,
    target_has_no_priv: bool,
    arg_ptr: u64,
) -> KcallResult {
    if target_has_no_priv {
        return KcallResult::Ok(EPERM);
    }
    let target_priv_id = match target_priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM),
    };
    let mut mem_range = crate::kpriv::MemRange::new();
    let copy_result = copy_struct_from_user(
        caller_nr, proc_table, arg_ptr,
        &mut mem_range as *mut _ as *mut u8,
        core::mem::size_of::<crate::kpriv::MemRange>(),
    );
    match copy_result {
        crate::vm::CrossSpaceResult::Completed(Ok(())) => {
            match priv_table.get_mut(target_priv_id) {
                Some(priv_) => {
                    match priv_.add_mem(&mem_range) {
                        Ok(()) => KcallResult::Ok(0),
                        Err(()) => KcallResult::Ok(ENOSPC),
                    }
                }
                None => KcallResult::Ok(EINVAL),
            }
        }
        crate::vm::CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        crate::vm::CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

/// SYS_PRIV_ADD_IRQ — add an IRQ line to the target's privilege.
///
/// C: do_privctl.c:218-230.
fn privctl_add_irq(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    target_priv_id: Option<crate::kpriv::PrivId>,
    target_has_no_priv: bool,
    arg_ptr: u64,
) -> KcallResult {
    if target_has_no_priv {
        return KcallResult::Ok(EPERM);
    }
    let target_priv_id = match target_priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM),
    };
    let mut irq: i32 = 0;
    let copy_result = copy_struct_from_user(
        caller_nr, proc_table, arg_ptr,
        &mut irq as *mut _ as *mut u8,
        core::mem::size_of::<i32>(),
    );
    match copy_result {
        crate::vm::CrossSpaceResult::Completed(Ok(())) => {
            match priv_table.get_mut(target_priv_id) {
                Some(priv_) => {
                    match priv_.add_irq(irq) {
                        Ok(()) => KcallResult::Ok(0),
                        Err(()) => KcallResult::Ok(ENOSPC),
                    }
                }
                None => KcallResult::Ok(EINVAL),
            }
        }
        crate::vm::CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        crate::vm::CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}

/// SYS_PRIV_UPDATE_SYS — update an existing privilege structure from user.
///
/// C: do_privctl.c:253-268.
fn privctl_update_sys(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    priv_table: &mut crate::kpriv::PrivTable,
    target_priv_id: Option<crate::kpriv::PrivId>,
    arg_ptr: u64,
) -> KcallResult {
    // C: do_privctl.c:255 — arg_ptr must be non-null
    if arg_ptr == 0 {
        return KcallResult::Ok(EINVAL);
    }
    let target_priv_id = match target_priv_id {
        Some(id) => id,
        None => return KcallResult::Ok(EINVAL),
    };
    let mut req = crate::kpriv::PrivUpdateRequest::new();
    let copy_result = copy_struct_from_user(
        caller_nr, proc_table, arg_ptr,
        &mut req as *mut _ as *mut u8,
        core::mem::size_of::<crate::kpriv::PrivUpdateRequest>(),
    );
    match copy_result {
        crate::vm::CrossSpaceResult::Completed(Ok(())) => {
            // C: do_privctl.c:265-267 — update_priv(rp, &priv).
            // Table-level: field copies onto the target slot plus
            // the whole-table target-mask fill (fill_sendto_mask,
            // system.c:349-358) that grants/revokes the reciprocal
            // bits and applies the association/self guards.
            match priv_table.update_priv(target_priv_id, &req) {
                Ok(()) => KcallResult::Ok(0),
                Err(_) => KcallResult::Ok(EINVAL),
            }
        }
        crate::vm::CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        crate::vm::CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}
fn dispatch_trace(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &mut Message, priv_table: &PrivTable) -> KcallResult { crate::misc::dispatch_trace(caller_nr, proc_table, msg, priv_table) }
fn dispatch_kill(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &mut PrivTable) -> KcallResult {
    crate::syscall_signal::dispatch_kill(caller_nr, proc_table, msg, priv_table)
}
fn dispatch_getksig(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &mut Message, priv_table: &PrivTable) -> KcallResult {
    crate::syscall_signal::dispatch_getksig(caller_nr, proc_table, msg, priv_table)
}
fn dispatch_endksig(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &PrivTable) -> KcallResult {
    crate::syscall_signal::dispatch_endksig(caller_nr, proc_table, msg, priv_table)
}
fn dispatch_sigsend(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult {
    crate::syscall_signal::dispatch_sigsend(caller_nr, proc_table, msg)
}
fn dispatch_sigreturn(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult {
    crate::syscall_signal::dispatch_sigreturn(caller_nr, proc_table, msg)
}
fn dispatch_memset(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult { crate::syscall_copy::dispatch_memset(caller_nr, proc_table, msg) }
fn dispatch_umap(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &mut Message, priv_table: &PrivTable) -> KcallResult { crate::syscall_copy::dispatch_umap(caller_nr, proc_table, msg, priv_table) }
fn dispatch_vircopy(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult { crate::syscall_copy::dispatch_vircopy(caller_nr, proc_table, msg) }
fn dispatch_physcopy(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult { crate::syscall_copy::dispatch_physcopy(caller_nr, proc_table, msg) }
fn dispatch_umap_remote(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &mut Message, priv_table: &PrivTable) -> KcallResult { crate::syscall_copy::dispatch_umap_remote(caller_nr, proc_table, msg, priv_table) }
fn dispatch_vumap(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &mut Message, priv_table: &PrivTable) -> KcallResult { crate::syscall_copy::dispatch_vumap(caller_nr, proc_table, msg, priv_table) }
fn dispatch_irqctl(
    caller_nr: crate::proc::ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
    priv_table: &mut PrivTable,
    bkl_section: &crate::smp::BklSection<'_>,
) -> KcallResult {
    // SYS_IRQCTL dispatcher — delegates to `syscall_device::dispatch_irqctl`,
    // which performs request validation, CHECK_IRQ permission checks, and the
    // IrqManager hook operations (SETPOLICY / RMPOLICY / ENABLE / DISABLE).
    //
    // The global `IrqManager<CurrentInterruptController>` is acquired via
    // `crate::irq_manager_with(bkl_section)`. The BklSection witness proves
    // BKL is held (R-03 compile-time capability token). The same pattern is
    // used by `dispatch_hardware_irq` (irq_manager.rs:215).
    //
    // C: do_irqctl.c — full IRQ control handler.
    let irq_mgr = crate::irq_manager_with(bkl_section);
    crate::syscall_device::dispatch_irqctl(caller_nr, proc_table, msg, irq_mgr, priv_table)
}
fn dispatch_setalarm(caller_nr: crate::proc::ProcNr, msg: &mut Message, priv_table: &mut PrivTable, clock_state: &mut ClockState, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_clock::dispatch_setalarm(caller_nr, msg, priv_table, clock_state, proc_table) }
fn dispatch_times(caller_nr: crate::proc::ProcNr, msg: &mut Message, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_clock::dispatch_times(caller_nr, msg, proc_table) }
fn dispatch_getinfo(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &mut Message, priv_table: &mut PrivTable, clock_state: &ClockState) -> KcallResult {
    crate::misc::dispatch_getinfo(caller_nr, proc_table, msg, priv_table, clock_state)
}
/// Dispatch SYS_ABORT.
///
/// C: `do_abort()` — do_abort.c:16-26 + `prepare_shutdown()` — main.c:351-363.
///
/// Emergency system shutdown. C extracts `how` from `m_lsys_krn_sys_abort.how`
/// and calls `prepare_shutdown(how)`, which sets a 1-second watchdog timer
/// that calls `minix_shutdown(how)`. `minix_shutdown` disables all interrupts,
/// stops the local timer, prints a shutdown message based on `how`
/// (`RB_POWERDOWN` → power off, `RB_HALT` → halt, else → reset), and calls
/// `arch_shutdown(how)`.
///
/// In the no_std Rust kernel, we cannot set a watchdog timer (no scheduler
/// context to run it) and there is no monitor to return to. The correct
/// kernel-level response is `panic!` with a diagnostic message that includes
/// the `how` flag's decoded meaning, matching C's "halt the system and print
/// a message" intent.
///
/// # `how` flag decoding (C: sys/reboot.h:45-54)
///
/// - `RB_HALT` (0x08): halt without rebooting
/// - `RB_POWERDOWN` (0x808 = RB_HALT | 0x800): power off (implies halt)
/// - `RB_NOSYNC` (0x04): don't sync filesystems
/// - `RB_DUMP` (0x100): dump kernel memory before reboot
/// - default: reboot/reset
fn dispatch_abort(
    caller_nr: crate::proc::ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &Message,
) -> KcallResult {
    // C: do_abort.c:21 — int how = m_ptr->m_lsys_krn_sys_abort.how
    // The abort message layout (ipc.h:1115-1119) is a single int `how` at
    // offset 0, which maps to M1's m1i1 field on all architectures.
    let m1 = unsafe { &msg.m_u.m_m1 };
    let how: i32 = m1.m1i1;

    // Decode the `how` flags (C: sys/reboot.h).
    const RB_NOSYNC: i32 = 0x0004;
    const RB_HALT: i32 = 0x0008;
    const RB_DUMP: i32 = 0x0100;
    const RB_POWERDOWN: i32 = RB_HALT | 0x0800;

    let action = if (how & RB_POWERDOWN) == RB_POWERDOWN {
        "power off"
    } else if (how & RB_HALT) != 0 {
        "halt"
    } else {
        "reset"
    };
    let nosync = if (how & RB_NOSYNC) != 0 { " [NOSYNC]" } else { "" };
    let dump = if (how & RB_DUMP) != 0 { " [DUMP]" } else { "" };

    // C: main.c:360 — printf("MINIX will now be shut down ...\n")
    // C: main.c:390-396 — direct_print("MINIX has halted ..." / "MINIX will now reset.\n")
    // Rust: panic with equivalent diagnostic.
    panic!(
        "MINIX will now be shut down ... (SYS_ABORT from endpoint {:?}, action={}{}{}, how=0x{:x})",
        proc_table
            .get(caller_nr)
            .map(|c| c.p_endpoint)
            .expect("dispatch_abort: caller slot must exist"),
        action,
        nosync,
        dump,
        how,
    );
}
fn dispatch_safecopy_from(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &crate::kpriv::PrivTable) -> KcallResult { crate::syscall_copy::dispatch_safecopy_from(caller_nr, proc_table, msg, priv_table) }
fn dispatch_safecopy_to(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &crate::kpriv::PrivTable) -> KcallResult { crate::syscall_copy::dispatch_safecopy_to(caller_nr, proc_table, msg, priv_table) }
fn dispatch_vsafecopy(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &crate::kpriv::PrivTable) -> KcallResult { crate::syscall_copy::dispatch_vsafecopy(caller_nr, proc_table, msg, priv_table) }
/// Dispatch SYS_SETGRANT.
///
/// C: `do_setgrant()` — do_setgrant.c:15-29
///
/// Copies the grant table address and size into the caller's privilege structure.
/// This is used by system processes (PM, VFS, RS) to register their grant tables
/// with the kernel for safe copy operations.
///
/// # Permission
///
/// Caller must have a privilege structure and must not have `RTS_NO_PRIV` set.
fn dispatch_setgrant(
    caller_nr: crate::proc::ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &Message,
    priv_table: &mut PrivTable,
) -> KcallResult {
    // C: do_setgrant.c:22 — check RTS_NO_PRIV (K20: slot read).
    if proc_table
        .get(caller_nr)
        .is_some_and(|c| c.p_rts_flags.is_set(crate::proc::RtsFlagsBits::NO_PRIV))
    {
        return KcallResult::Ok(EPERM);
    }

    // C: do_setgrant.c:22 — check priv(caller) exists
    let priv_id = match proc_table.get(caller_nr).and_then(|c| c.priv_id) {
        Some(id) => id,
        None => return KcallResult::Ok(EPERM),
    };

    // Parse message fields.
    // C: m_ptr->m_lsys_krn_sys_setgrant.addr / .size
    msg.debug_check_m_type_any(&[Syscall::Setgrant as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let grant_msg = unsafe { &msg.m_u.m_lsys_krn_sys_setgrant };

    // C: _K_SET_GRANT_TABLE(rp, ptr, entries) — safecopies.h:104-107
    // Sets priv(rp)->s_grant_table, s_grant_entries, s_grant_endpoint.
    if let Some(priv_entry) = priv_table.get_mut(priv_id) {
        priv_entry.runtime.s_grant_table = grant_msg.addr as usize;
        priv_entry.runtime.s_grant_entries = grant_msg.size;
        priv_entry.runtime.s_grant_endpoint = proc_table
            .get(caller_nr)
            .map(|c| c.p_endpoint)
            .expect("dispatch_setgrant: caller slot must exist");
        KcallResult::Ok(0)
    } else {
        KcallResult::Ok(EPERM)
    }
}
fn dispatch_sprofile(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult { crate::misc::dispatch_profile(caller_nr, proc_table, msg) }
fn dispatch_stime(msg: &Message, clock_state: &mut ClockState) -> KcallResult { crate::syscall_clock::dispatch_stime(msg, clock_state) }
fn dispatch_settime(msg: &Message, clock_state: &mut ClockState) -> KcallResult { crate::syscall_clock::dispatch_settime(msg, clock_state) }
/// Dispatch SYS_VMCTL.
///
/// C: `do_vmctl()` — do_vmctl.c:17-173
///
/// VM control interface. VM uses this syscall to:
/// - Clear page fault flags on processes after handling a fault
/// - Fetch pending memory requests (VMCTL_MEMREQ_GET)
/// - Reply to memory requests (VMCTL_MEMREQ_REPLY)
/// - Set/clear VMINHIBIT to pause/resume process scheduling
/// - Clear BOOTINHIBIT to allow a boot process to run
/// - Manage kernel physical mappings and address spaces
///
/// # Message fields (C: com.h:370-382)
///
/// - `SVMCTL_WHO` (m1_i1): target process endpoint
/// - `SVMCTL_PARAM` (m1_i2): VMCTL_* sub-command
/// - `SVMCTL_VALUE` (m1_i3): sub-command value
///
/// # Permission
///
/// Only system processes (SYS_PROC) may call SYS_VMCTL.
/// C: implicit — only VM calls this, and VM always has SYS_PROC.
fn dispatch_vmctl(
    caller_nr: crate::proc::ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
    priv_table: &PrivTable,
) -> KcallResult {
    use crate::vm::VmCtlParam;
    use minix_arch::TlbArch;
    use minix_types::VirBytes;

    // Permission check: only system processes may call VMCTL.
    // C: implicit — only VM calls this, and VM always has SYS_PROC.
    //
    // FIX-25 (2026-08-14): use `caller_has_sys_proc_with_table` — the legacy
    // `caller_has_sys_proc` builds a fresh empty `PrivTable::new()` internally
    // and always returns false, rejecting every privileged caller (same
    // latent bug as `dispatch_privctl`/`dispatch_schedule`, now fixed).
    if !crate::syscall_clock::caller_has_sys_proc_with_table(
        proc_table.get(caller_nr).expect("dispatch_vmctl: caller slot must exist"),
        priv_table,
    ) {
        return KcallResult::Ok(EPERM);
    }

    // Parse message fields.
    // C: SVMCTL_WHO = m1_i1, SVMCTL_PARAM = m1_i2, SVMCTL_VALUE = m1_i3
    // Read all fields upfront so the &msg borrow is dropped before we
    // potentially take &mut msg in MemReqGet/MemReqReply branches.
    let (who_ep, param_raw, value_raw) = {
        let m1 = unsafe { &msg.m_u.m_m1 };
        (m1.m1i1, m1.m1i2, m1.m1i3)
    };

    // Resolve target endpoint. C: do_vmctl.c:22-28
    // SELF means the caller's own endpoint.
    let target_ep = if who_ep == minix_types::Endpoint::SELF.0 {
        proc_table
            .get(caller_nr)
            .map(|c| c.p_endpoint)
            .expect("dispatch_vmctl: caller slot must exist")
            .0
    } else {
        who_ep
    };

    let target_ep = Endpoint(target_ep);
    let target_nr = match proc_table.endpoint_to_nr(target_ep) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // Parse sub-command. C: switch(m_ptr->SVMCTL_PARAM)
    let param = match VmCtlParam::try_from(param_raw) {
        Ok(p) => p,
        Err(()) => {
            // Unknown VMCTL param — in C this falls through to arch_do_vmctl()
            // which returns EINVAL. Return ENOSYS for unrecognized commands
            // to distinguish from valid-but-unimplemented (EINVAL).
            return KcallResult::Ok(ENOSYS);
        }
    };

    // Note on the result mapping: the pre-split dispatcher funnelled every
    // arm through `VmCtlResult` and converted at the end
    // (Ok(v)→Ok(v), VmSuspend→VmSuspend, BadParam→Ok(EINVAL)). No arm in
    // this switch ever produced VmSuspend or BadParam, so the sub-handlers
    // and inline arms below return `KcallResult` directly with the same
    // values — the funnel was an Ok-to-Ok identity for every path here.
    match param {
        // ── ClearPageFault: clear RTS_PAGEFAULT on target ──
        // C: do_vmctl.c:32-35 — assert(RTS_ISSET(p,RTS_PAGEFAULT)); RTS_UNSET(p, RTS_PAGEFAULT);
        VmCtlParam::ClearPageFault => vmctl_clear_page_fault(proc_table, target_nr),

        // ── MemReqGet: VM fetches the next pending memory request ──
        // C: do_vmctl.c:36-72 — traverse vmrequest linked list with IPC filter.
        // On success, fills reply message fields (SVMCTL_MRG_*) and returns
        // the request type (VMPTYPE_CHECK=1). On no-match, returns ENOENT=2.
        VmCtlParam::MemReqGet => vmctl_memreq_get(proc_table, msg),

        // ── MemReqReply: VM replies with the result of a memory request ──
        // C: do_vmctl.c:73-109 — set vmresult, set MF_KCALL_RESUME for
        // KernelCall type, clear RTS_VMREQUEST. Returns OK=0.
        VmCtlParam::MemReqReply => vmctl_memreq_reply(proc_table, target_nr, value_raw),

        // ── VmInhibitSet: set RTS_VMINHIBIT on target ──
        // C: do_vmctl.c:119-131
        VmCtlParam::VmInhibitSet => vmctl_vminhibit_set(proc_table, target_nr),

        // ── VmInhibitClear: clear RTS_VMINHIBIT on target ──
        // C: do_vmctl.c:132-160
        VmCtlParam::VmInhibitClear => {
            let r = vmctl_vminhibit_clear(proc_table, target_nr);
            nk4a_flags_mark("vminh-clear", proc_table, target_nr);
            r
        }

        // ── BootInhibitClear: clear RTS_BOOTINHIBIT on target ──
        // C: do_vmctl.c:165-167 — RTS_UNSET(p, RTS_BOOTINHIBIT)
        VmCtlParam::BootInhibitClear => {
            let r = vmctl_boot_inhibit_clear(proc_table, target_nr);
            nk4a_flags_mark("bootinh-clear", proc_table, target_nr);
            r
        }

        // ── ClearMapCache: clear cached mappings ──
        // C: do_vmctl.c:161-164 — mem_clear_mapcache()
        // WONTFIX (FIX-24): mem_clear_mapcache() is a 32-bit-only optimization
        // that flushes the kernel's cached physical-to-virtual mapping table
        // used by the 32-bit Direct Map implementation. On 64-bit minix-rs,
        // the Direct Map covers all physical memory with a static offset
        // (no cache table), so there is nothing to clear. Return OK (0)
        // to indicate success — matching the C behavior on architectures
        // where mem_clear_mapcache() is a no-op.
        VmCtlParam::ClearMapCache => KcallResult::Ok(0),

        // ── SetAddrSpace: switch target's page table root ──
        // C: arch_do_vmctl.c:48-50 → setcr3(p, SVMCTL_PTROOT, SVMCTL_PTROOT_V)
        // (see vmctl_set_addr_space for the full 5-step C mapping and
        // the vm_running C-bug correction note)
        // 续-311 内核代读旁路（[ARCH: riscv-vmddm]）：KDM 直读 m1p1 处
        // u64 回填 m1p1。仅 riscv64 生产形态使用（VM 的 VmDm 读钩子）。
        // 续-311 内核代写/零填旁路（[ARCH: riscv-vmddm] 同族）：KDM 直写
        // m1p1 处 u64（值=m1p2）/整页零填。仅 riscv64 生产形态使用。
        VmCtlParam::PteWrite => {
            #[cfg(all(target_arch = "riscv64", not(test)))]
            {
                const KDM: u64 = 0xFFFF_FFC0_4000_0000;
                let pa = unsafe { msg.m_u.m_m1.m1p1 } as u64;
                let val = unsafe { msg.m_u.m_m1.m1p2 } as u64;
                unsafe { ((KDM + pa) as *mut u64).write_volatile(val) };
                unsafe { core::arch::asm!("sfence.vma zero, zero") };
                KcallResult::Ok(0)
            }
            #[cfg(not(all(target_arch = "riscv64", not(test))))]
            {
                let _ = msg;
                KcallResult::Ok(38) // ENOSYS
            }
        }
        VmCtlParam::PteZero => {
            #[cfg(all(target_arch = "riscv64", not(test)))]
            {
                const KDM: u64 = 0xFFFF_FFC0_4000_0000;
                let pa = unsafe { msg.m_u.m_m1.m1p1 } as u64;
                for i in 0..512u64 {
                    unsafe { ((KDM + pa + i * 8) as *mut u64).write_volatile(0) };
                }
                unsafe { core::arch::asm!("sfence.vma zero, zero") };
                KcallResult::Ok(0)
            }
            #[cfg(not(all(target_arch = "riscv64", not(test))))]
            {
                let _ = msg;
                KcallResult::Ok(38) // ENOSYS
            }
        }
        VmCtlParam::PteRead => {
            #[cfg(all(target_arch = "riscv64", not(test)))]
            {
                const KDM: u64 = 0xFFFF_FFC0_4000_0000;
                let pa = unsafe { msg.m_u.m_m1.m1p1 } as u64;
                let v = unsafe { ((KDM + pa) as *const u64).read_volatile() };
                unsafe { msg.m_u.m_m1.m1p1 = v; };
                KcallResult::Ok(0)
            }
            #[cfg(not(all(target_arch = "riscv64", not(test))))]
            {
                let _ = msg;
                KcallResult::Ok(ENOSYS)
            }
        }
        VmCtlParam::SetAddrSpace => {
            let r = vmctl_set_addr_space(proc_table, target_nr, value_raw, msg);
            nk4a_flags_mark("setaddr", proc_table, target_nr);
            // 续-306 krewalk 配套（用后即滚）：setaddr 落的 root 值——与
            // fill-root 探针的 ptroot 对账，定案「根错配（in-code）」vs
            // 「视图分歧（平移层）」。
            #[cfg(all(target_arch = "riscv64", not(test)))]
            {
                use minix_plat::{CurrentEarlyConsole as SaConsole, EarlyConsole as _};
                SaConsole::write_str("nk4a: setaddr-root=");
                SaConsole::write_hex(value_raw as u64);
                SaConsole::write_str("\n");
            }
            r
        }

        // ── Arch-specific commands: GetPdbr, FlushTlb, InvlPg ──
        // C: handled by arch_do_vmctl() in arch_do_vmctl.c:38-65
        //
        // FIX-24 (Phase 5): Implemented via `TlbArch` trait (flush_all /
        // flush_addr) + direct field read (GetPdbr). Three architectures
        // covered: x86_64 (CR3/INVLPG), aarch64 (TLBI ALLE1IS/VAAE1IS),
        // riscv64 (SFENCE.VMA).
        //
        // V13-P2-1 (02-stage-vm): the VM server has ZERO callers of
        // FlushTlb/InvlPg — that is by design, not a missing wire. C's VM
        // flushed itself at 4 sites (pagetable.c:119/255/319/430) because it
        // aliased process memory into its own address space; minix-rs's
        // Direct Map keeps translations constant, and `Paging`'s
        // `write_pte_dm` binds an invlpg to every PTE write (08-pagetable-ops
        // §1.8). These commands stay as the kernel-side escape hatch for a
        // world where VM ever needs explicit TLB maintenance again — do not
        // read the absence of consumers as an unwired edge.
        VmCtlParam::GetPdbr => {
            // C: arch_do_vmctl.c:38-40 — rv = p->p_seg.p_cr3
            // Return the target process's page table root physical address.
            // This is a simple field read — no arch operation needed.
            match proc_table.get(target_nr) {
                Some(p) => KcallResult::Data(p.p_seg.phys_root.0 as i32), // data: physical address, not an error code
                None => KcallResult::Ok(EINVAL),
            }
        }

        VmCtlParam::FlushTlb => {
            // C: arch_do_vmctl.c:42-44 — write_cr3(p->p_seg.p_cr3)
            // Flush all non-global TLB entries on the current CPU.
            // The C code reloads the target's CR3, but the actual effect
            // is a full TLB flush (x86 reloads CR3 → flush). On ARM64/RISC-V
            // the equivalent is tlbi alle1is / sfence.vma zero, zero.
            //
            // SAFETY: Called from syscall dispatch context with paging
            // enabled. The target process must be valid (checked above).
            unsafe { minix_arch::CurrentTlbArch::flush_all(); }
            KcallResult::Ok(0)
        }

        VmCtlParam::InvlPg => {
            // C: arch_do_vmctl.c:52-54 — invlpg(m_ptr->SVMCTL_WHERE)
            // Invalidate the TLB entry for a single virtual address.
            // The virtual address comes from the SVMCTL_WHERE message field
            // (m1_i3, same field as SVMCTL_VALUE for SetAddrSpace).
            let vaddr = VirBytes(value_raw as u64);
            // SAFETY: Called from syscall dispatch context with paging
            // enabled. The virtual address is provided by the caller
            // (VM server) and is expected to be a valid user-space address.
            unsafe { minix_arch::CurrentTlbArch::flush_addr(vaddr); }
            KcallResult::Ok(0)
        }

        // ── 32-bit legacy: KernPhysMap, KernMapReply ──
        // C: do_vmctl.c:105-118 — arch_phys_map/arch_phys_map_reply
        // These are 32-bit-only (x86 PAE) and unused on 64-bit.
        VmCtlParam::KernPhysMap | VmCtlParam::KernMapReply => {
            KcallResult::Ok(ENOSYS)
        }
    }
}

/// ClearPageFault — clear RTS_PAGEFAULT on the target.
///
/// C: do_vmctl.c:32-35 — assert(RTS_ISSET(p,RTS_PAGEFAULT));
/// RTS_UNSET(p, RTS_PAGEFAULT). The C assert converts to EINVAL.
///
/// The clear goes through the scheduler-aware `rts_unset`, which is the
/// RTS_UNSET macro's enqueue half: the process became non-runnable at
/// fault time (RTS_PAGEFAULT, dequeued) and must re-enter its run queue
/// now that VM resolved the fault. The primitive flag clear alone left
/// the process dequeued forever — the fault loop never closed.
fn vmctl_clear_page_fault(
    proc_table: &mut crate::proc_table::ProcessTable,
    target_nr: ProcNr,
) -> KcallResult {
    let was_set = proc_table
        .get(target_nr)
        .is_some_and(|p| p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::PAGEFAULT));
    if !was_set {
        // C: assert(RTS_ISSET(p, RTS_PAGEFAULT)) — convert to error return
        return KcallResult::Ok(EINVAL);
    }
    // TLB 失效半的时点说明（2026-09-22）：INVLPG 只击落当前 CR3 标签的
    // 非 G 项——clear 时点执行在 VM 的 CR3 上下文里，杀不到故障进程自己
    // 的陈旧项。真正的 invlpg 在 finish_and_restore（被恢复进程的 CR3 已
    // 激活、iretq 之前）以其 p_fault_addr 执行；本臂只负责保留
    // p_fault_addr（不再清 None）供恢复点使用。
    proc_table.rts_unset(target_nr, crate::proc::RtsFlagsBits::PAGEFAULT);
    KcallResult::Ok(0)
}

/// MemReqGet — VM fetches the next pending memory request.
///
/// C: do_vmctl.c:36-72 — traverse vmrequest linked list with IPC filter.
/// On success, fills reply message fields (SVMCTL_MRG_*) and returns
/// the request type (VMPTYPE_CHECK=1). On no-match, returns ENOENT=2.
fn vmctl_memreq_get(
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &mut Message,
) -> KcallResult {
    // Bind to a local so the &mut proc_table borrow ends before
    // we access proc_table again for reading endpoint info.
    let get_result = proc_table.vm_memreq_get();
    match get_result {
        Ok((proc_nr, params)) => {
            // C: do_vmctl.c:61-72 — populate reply message fields.
            // SVMCTL_MRG_TARGET, SVMCTL_MRG_ADDR, SVMCTL_MRG_LENGTH,
            // SVMCTL_MRG_FLAG, SVMCTL_MRG_REQUESTOR.
            let m1 = unsafe { &mut msg.m_u.m_m1 };
            // Read target endpoint and requestor endpoint from the
            // process that was just dequeued. The mutable borrow from
            // vm_memreq_get() has ended, so we can borrow again.
            let proc = proc_table.get(proc_nr);
            let target_ep = proc.and_then(|p| p.p_vm_suspend.as_ref())
                .map(|ctx| ctx.target.0)
                .unwrap_or(0);
            let requestor_ep = proc.map(|p| p.p_endpoint.0).unwrap_or(0);

            m1.m1i1 = target_ep;                       // SVMCTL_MRG_TARGET
            m1.m1p1 = params.start.0;                  // SVMCTL_MRG_ADDR
            m1.m1p2 = params.length.0;                 // SVMCTL_MRG_LENGTH
            m1.m1i3 = if params.write_flag { 1 } else { 0 }; // SVMCTL_MRG_FLAG
            m1.m1p3 = requestor_ep as u64;             // SVMCTL_MRG_REQUESTOR

            // C: return rp->p_vmrequest.req_type (= VMPTYPE_CHECK = 1)
            KcallResult::Data(1) // VMPTYPE_CHECK: request found (data code, not negated on wire)
        }
        Err(crate::vm::VmCtlError::NoRequest) => KcallResult::Data(ENOENT), // empty queue (data signal, not an error)
        Err(crate::vm::VmCtlError::InvalidState) => KcallResult::Ok(EINVAL),
        Err(crate::vm::VmCtlError::InvalidEndpoint) => KcallResult::Ok(EINVAL),
    }
}

/// MemReqReply — VM replies with the result of a memory request.
///
/// C: do_vmctl.c:73-109 — set vmresult, set MF_KCALL_RESUME for
/// KernelCall type, clear RTS_VMREQUEST. Returns OK=0.
/// C: m_ptr->SVMCTL_VALUE carries the VM check result.
fn vmctl_memreq_reply(
    proc_table: &mut crate::proc_table::ProcessTable,
    target_nr: ProcNr,
    value_raw: i32,
) -> KcallResult {
    let vm_result = match value_raw {
        0 => crate::vm::VmCheckResult::Ok,   // VM confirmed valid
        _ => crate::vm::VmCheckResult::Fault, // VM reported fault
    };

    // 续-191 取证探针 P2（riscv64·判决边界·Fault 诞生点）：VM 对哪条
    // memreq 回了非零（=Fault），打在哪个 requestor 槽位上。与 P1
    // （handle_kernel_memreq 失败出口）+ P3（SIGSEGV 消费腿）对账，
    // 定谳 resume-Fault 产地。C-61 pattern-gate 前不滚，task-close 删。
    #[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
        use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
        static MRR: AtomicUsize = AtomicUsize::new(0);
        if MRR.fetch_add(1, AtomicOrd::Relaxed) < 64 {
            C0::write_str("nk4a: mrr nr=0x");
            C0::write_hex(target_nr.0 as u64);
            C0::write_str(" vraw=0x");
            C0::write_hex(value_raw as u32 as u64);
            C0::write_str(" verdict=");
            C0::write_str(if matches!(vm_result, crate::vm::VmCheckResult::Fault) {
                "Fault\n"
            } else {
                "Ok\n"
            });
        }
    }

    match proc_table.vm_memreq_reply(target_nr, vm_result) {
        Ok(()) => KcallResult::Ok(0), // C: return OK
        Err(crate::vm::VmCtlError::InvalidState) => KcallResult::Ok(EINVAL),
        Err(crate::vm::VmCtlError::NoRequest) => KcallResult::Ok(EINVAL),
        Err(crate::vm::VmCtlError::InvalidEndpoint) => KcallResult::Ok(EINVAL),
    }
}

/// VmInhibitSet — set RTS_VMINHIBIT on the target.
///
/// C: do_vmctl.c:119-131.
/// D-35 (C do_vmctl.c:118-130): if SMP and target on a different
/// CPU, send IPI via schedule_vminhibit; else set locally.
/// Single-CPU build: always local (target_cpu == current_cpu).
///
/// The FLUSH_TLB flag is set UNCONDITIONALLY after either arm — C
/// do_vmctl.c:133-135 keeps `p->p_misc_flags |= MF_FLUSH_TLB` outside
/// the if/else, and because the process table is shared, the flag lands
/// on the target no matter which CPU executed the request. The remote
/// IPI arm itself parks via VMINHIBIT only (C smp.c:180-182 sets
/// RTS_VMINHIBIT in the IPI handler, no FLUSH_TLB), so this
/// unconditional set is what completes the remote path — without it a
/// process parked over IPI would resume with stale translations
/// (edge1 K3 / E-VMTLB 余件 (a)).
fn vmctl_vminhibit_set(
    proc_table: &mut crate::proc_table::ProcessTable,
    target_nr: ProcNr,
) -> KcallResult {
    let target_cpu = proc_table
        .get(target_nr)
        .map(|p| crate::proc::CpuId::new_unchecked(
            p.p_sched.cpu.load(core::sync::atomic::Ordering::Acquire),
        ))
        .unwrap_or(crate::proc::CpuId::BSP);
    // SAFETY: dispatch_vmctl runs under BKL (kernel_call contract).
    let smp = unsafe { crate::smp_state() };
    let current_cpu = smp.bsp_cpu_id();
    if target_cpu != current_cpu {
        // SMP: route through IPI (schedule_sync → send_sched_ipi).
        // The IPI handler parks the process via VMINHIBIT only.
        smp.schedule_vminhibit::<minix_arch::CurrentSmpArch>(
            proc_table, target_nr, current_cpu,
        );
    } else {
        // Local: direct RTS_SET (C do_vmctl.c:132).
        vminhibit_park_local(proc_table, target_nr);
    }
    // C do_vmctl.c:133-135 — unconditional on both arms.
    mark_flush_tlb(proc_table, target_nr);
    KcallResult::Ok(0)
}

/// Local arm of VMINHIBIT set: park the target directly.
/// C: do_vmctl.c:132 — `RTS_SET(p, RTS_VMINHIBIT)`.
fn vminhibit_park_local(proc_table: &mut crate::proc_table::ProcessTable, target_nr: ProcNr) {
    if let Some(p) = proc_table.get_mut(target_nr) {
        p.p_rts_flags.set(crate::proc::RtsFlagsBits::VMINHIBIT);
    }
}

/// Request a TLB refresh at the target's next restore.
/// C: do_vmctl.c:133-135 — `p->p_misc_flags |= MF_FLUSH_TLB`, outside
/// the local/IPI if/else. Consumed by the pick-point/restore pair
/// (`needs_tlb_refresh` / `consume_flush_tlb_flag`, E-VMTLB 机制半).
fn mark_flush_tlb(proc_table: &mut crate::proc_table::ProcessTable, target_nr: ProcNr) {
    if let Some(p) = proc_table.get_mut(target_nr) {
        p.p_misc_flags.set(crate::proc::MiscFlagsBits::FLUSH_TLB);
    }
}

/// NK4-A 首亮取证路标（task1-close 裁决删除）：每个调度抑制腿走完后
/// 打印目标最终 RTS 位图与 runnable 判定（EarlyConsole=COM1），一次
/// 真机分清"旗没清干净"与"清了没入队/没被 pick"两类挂点。
/// 只用 `write_str`/`write_hex`——运行时内核 bump 堆已耗尽，`format!`
/// 在这里就是一次 76 字节分配失败（fix25b forensics 2026-09-21）。
fn nk4a_flags_mark(
    tag: &str,
    proc_table: &crate::proc_table::ProcessTable,
    nr: ProcNr,
) {
    #[cfg(not(feature = "mock"))]
    {
        use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
        let (flags, runnable) = proc_table
            .get(nr)
            .map_or((0xFFFF_FFFF, false), |p| {
                (p.p_rts_flags.load(), p.is_runnable())
            });
        let queued = proc_table.is_in_scheduler(nr);
        Console::write_str("nk4a: ");
        Console::write_str(tag);
        Console::write_str(" nr=");
        Console::write_hex(nr.0 as u64);
        Console::write_str(" flags=0x");
        Console::write_hex(flags as u64);
        Console::write_str(" runnable=");
        Console::write_str(if runnable { "yes" } else { "no" });
        Console::write_str(" queued=");
        Console::write_str(if queued { "yes" } else { "no" });
        Console::write_str("\n");
    }
    let _ = (tag, proc_table, nr);
}

/// VmInhibitClear — clear RTS_VMINHIBIT on the target.
///
/// C: do_vmctl.c:132-160. C's assert on RTS_VMINHIBIT converts to EINVAL;
/// SMP-only MF_SENDA_VM_MISS handling + stale TLB fill not yet implemented.
///
/// The clear goes through `rts_unset` (C: RTS_UNSET carries the
/// ready-queue check, proc.h:216-224): without it a process whose last
/// blocking bit was VMINHIBIT never reaches the run queue (NK4-A fix24
/// found the same shape on BootInhibitClear below).
fn vmctl_vminhibit_clear(
    proc_table: &mut crate::proc_table::ProcessTable,
    target_nr: ProcNr,
) -> KcallResult {
    // C: assert(RTS_ISSET(p, RTS_VMINHIBIT)) — convert to error (missing
    // slot takes the same arm, as before).
    if !proc_table
        .get(target_nr)
        .is_some_and(|p| p.p_rts_flags.is_set(crate::proc::RtsFlagsBits::VMINHIBIT))
    {
        return KcallResult::Ok(EINVAL);
    }
    proc_table.rts_unset(target_nr, crate::proc::RtsFlagsBits::VMINHIBIT);
    KcallResult::Ok(0)
}

/// BootInhibitClear — clear RTS_BOOTINHIBIT on the target.
///
/// C: do_vmctl.c:165-167 — RTS_UNSET(p, RTS_BOOTINHIBIT). The `rts_unset`
/// wrapper IS the C macro's ready-queue check (proc.h:216-224): the boot
/// processes were already released from PROC_STOP by bsp_finish_booting,
/// so clearing BOOTINHIBIT is the event that makes them runnable — a
/// bare flag clear left every exec'd boot process parked forever
/// (NK4-A fix24 forensics 2026-09-21: VM's `exec X ok` landmarks all
/// passed, no boot process ever reached `_start`).
fn vmctl_boot_inhibit_clear(
    proc_table: &mut crate::proc_table::ProcessTable,
    target_nr: ProcNr,
) -> KcallResult {
    if proc_table.get(target_nr).is_none() {
        return KcallResult::Ok(EINVAL);
    }
    proc_table.rts_unset(target_nr, crate::proc::RtsFlagsBits::BOOTINHIBIT);
    KcallResult::Ok(0)
}

/// SetAddrSpace — switch the target's page table root.
///
/// C: arch_do_vmctl.c:48-50 → setcr3(p, SVMCTL_PTROOT, SVMCTL_PTROOT_V)
///
/// C setcr3 (arch_do_vmctl.c:19-33) does:
///   1. p->p_seg.p_cr3 = cr3
///   2. p->p_seg.p_cr3_v = v
///   3. if (p == ptproc) write_cr3(p->p_seg.p_cr3)
///   4. if (p->p_nr == VM_PROC_NR) arch_enable_paging(p)
///   5. RTS_UNSET(p, RTS_VMINHIBIT)
///
/// Rust implements all 5 steps:
///   - Steps 1-2: data layer (p_seg.phys_root / virt_root).
///   - Step 3: `TlbArch::set_active_root` when target is current ptproc
///     (tracked by the per-CPU `CpuLocal.ptproc` slot (D-40), initialized in
///     `init_post_and_memory`). The arch impls write CR3/TTBR0/satp.
///   - Step 4: no-op on 64-bit (paging enabled at boot via
///     `Paging::enable`).
///   - Step 5: clear RTS_VMINHIBIT.
///
/// # C bug correction
///
/// Minix3 C never sets `vm_running = 1` (only `main.c:47` sets it to 0).
/// Rust corrects this: when the target is `VM_PROC_NR`, set
/// `vm_running = true` so readers (`do_umap_remote`, `acpi`, `oxpcie`)
/// see VM as active. See `09-vm-boot-protocol.md §3 decision4` and
/// `lib.rs::set_vm_running` doc comment.
fn vmctl_set_addr_space(
    proc_table: &mut crate::proc_table::ProcessTable,
    target_nr: ProcNr,
    value_raw: i32,
    msg: &Message,
) -> KcallResult {
    // SVMCTL_PTROOT = m1_i3 (same field as SVMCTL_VALUE)
    // SVMCTL_PTROOT_V = m1_p1 (virtual address of page table root)
    //
    // `as u32 as u64` preserves the C bit pattern: C assigns the `int`
    // field to a `u32_t` cr3 parameter (modular conversion, NK4-A
    // fix25 note — the previous plain `as u64` sign-extended physical
    // roots ≥ 0x8000_0000 into 0xFFFFFFFF_8xxxxxxx). Wire width keeps
    // the C i386 constraint: physical roots above 4 GB cannot travel
    // this field (boot-allocated page tables are low; same as C).
    let ptroot_phys = value_raw as u32 as u64; // m1_i3 (i32) → bit-preserving u64
    let ptroot_virt = unsafe { msg.m_u.m_m1.m1p1 }; // m1_p1

    // C: setcr3() is `static void` with no failure path; the slot-missing
    // case (impossible in C — the caller resolved the slot first) maps to
    // EINVAL here, same as before.
    if proc_table.get(target_nr).is_none() {
        return KcallResult::Ok(EINVAL);
    }
    {
        let p = proc_table
            .get_mut(target_nr)
            .expect("slot existence checked above");

        // Steps 1-2: Set page table roots.
        // C: p->p_seg.p_cr3 = cr3; p->p_seg.p_cr3_v = v;
        p.p_seg.phys_root = minix_types::PhysBytes(ptroot_phys);
        p.p_seg.virt_root = if ptroot_virt != 0 {
            Some(minix_types::VirBytes(ptroot_virt))
        } else {
            None
        };

        // ── NK4-C 1.54（B26 fix B）：把 kerninfo 页的用户只读映射复制进
        // 每个新根。C 把 `.usermapped` 段经 VMCTL_KERN_PHYSMAP 协议映射
        // 进每个进程地址空间（i386 memory.c arch_phys_map + VM per-space
        // map）；本重写由内核在本唯一提交点代做（kerninfo.rs 模块 doc
        // 登记的 mapping-responsibility 转移尚未发生，不新增外部契约）。
        // 先查后装（重绑同一根幂等；子根 virtual_copy 继承过则跳过）。
        // 中间表页经 boot_pt_alloc（其区域已从 VM free list 扣减，
        // §1.53 boot_alloc_used_bytes 记账）。
        if let Some(ki_pa) = crate::kerninfo::kerninfo_page_phys() {
            use minix_arch::paging::Paging as _;
            let mut target_paging = minix_arch::CurrentPaging::from_active_root(
                minix_types::PhysBytes(ptroot_phys),
            );
            let needs_map = match target_paging.query(minix_types::VirBytes(
                crate::kerninfo::KERNINFO_USER_VA,
            )) {
                Some((cur, _flags)) => cur.0 != ki_pa,
                None => true,
            };
            if needs_map {
                target_paging
                    .map(
                        minix_types::VirBytes(crate::kerninfo::KERNINFO_USER_VA),
                        minix_types::PhysBytes(ki_pa),
                        minix_arch::paging::PageFlags::read_only(),
                    )
                    .expect("vmctl_set_addr_space: failed to map the kerninfo page into the new root");
            }
        }

        // Step 3: If target is the current ptproc, reload the
        // hardware root register (CR3/TTBR0/satp) so the new
        // page table takes effect immediately.
        // C: if (p == get_cpulocal_var(ptproc)) write_cr3(p->p_seg.p_cr3);
        //
        // The ptproc comparison uses proc-nr (i32) rather than
        // pointer identity. This is equivalent because proc-nrs
        // uniquely identify process slots in the ProcessTable
        // (one-to-one mapping, no aliasing).
        //
        // `set_active_root_tracked` = C's write_cr3 + the Rust
        // CR3-mirror update: the scheduler's
        // `switch_address_space` compares against the mirror
        // (C reads the live CR3), so the mirror must reflect
        // every root change or the first dispatch after this
        // would needlessly reload the same root.
        if crate::current_ptproc_nr() == Some(p.p_nr) {
            crate::set_active_root_tracked(
                minix_types::PhysBytes(ptroot_phys),
            );
        }

        // Step 4: arch_enable_paging — no-op on 64-bit
        // (paging enabled in `arch_boot_impl` via `Paging::enable`).

        // C bug correction: set vm_running = true when target is VM.
        // C source omits this (never writes vm_running=1). Rust
        // corrects the omission so VM is marked as running after
        // it has switched to its own page table.
        if p.p_nr == crate::proc::proc_nr::VM_PROC_NR {
            crate::set_vm_running(true);
        }
    }

    // Step 5: Clear VMINHIBIT — this is THE boot-path clear leg.
    // C: RTS_UNSET(p, RTS_VMINHIBIT) (arch_do_vmctl.c:32), fired for
    // every pt_bind → VMCTL_SETADDRSPACE (exec_bootproc main.c:355,
    // do_clear exit.c:137, …). The `rts_unset` wrapper carries the
    // ready-queue check (NK4-A fix24/25: a bare clear left boot
    // processes VMINHIBIT-parked even after BOOTINHIBIT was lifted —
    // this arm was the third instance of that shape). For boot
    // processes the enqueue does not fire yet here (BOOTINHIBIT still
    // set); it fires on the later BOOTINHIBIT_CLEAR, in C order.
    proc_table.rts_unset(target_nr, crate::proc::RtsFlagsBits::VMINHIBIT);

    KcallResult::Ok(0)
}
/// Dispatch SYS_DIAGCTL.
///
/// C: `do_diagctl()` — do_diagctl.c:18-68
///
/// Diagnostic control interface. Used by system processes to:
/// - DIAG: output diagnostic messages through the kernel console
/// - STACKTRACE: request a stack trace of a process
/// - REGISTER: register to receive SIGKMESS notifications
/// - UNREGISTER: stop receiving SIGKMESS notifications
///
/// # Message fields (C: ipc.h)
///
/// - `m_lsys_krn_sys_diagctl.code`: request code
/// - `m_lsys_krn_sys_diagctl.buf`: buffer address (DIAG only)
/// - `m_lsys_krn_sys_diagctl.len`: buffer length (DIAG only)
/// - `m_lsys_krn_sys_diagctl.endpt`: target endpoint (STACKTRACE only)

/// 在 diagbuf 里找 `field=0x…` 并解析 hex（至非 hex 字符止）。
fn find_hex_field(buf: &[u8], field: &[u8]) -> Option<u64> {
    let pos = buf.windows(field.len()).position(|w| w == field)?;
    let mut i = pos + field.len();
    if i + 1 < buf.len() && buf[i] == b'0' && (buf[i + 1] | 0x20) == b'x' {
        i += 2;
    }
    let mut v: u64 = 0;
    while i < buf.len() {
        let d = match buf[i] {
            b'0'..=b'9' => (buf[i] - b'0') as u64,
            b'a'..=b'f' => (buf[i] - b'a' + 10) as u64,
            b'A'..=b'F' => (buf[i] - b'A' + 10) as u64,
            _ => break,
        };
        v = (v << 4) | d;
        i += 1;
    }
    Some(v)
}

fn dispatch_diagctl(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &Message,
    priv_table: &mut PrivTable,
) -> KcallResult {
    msg.debug_check_m_type_any(&[Syscall::Diagctl as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let diag_msg = unsafe { &msg.m_u.m_lsys_krn_sys_diagctl };
    match diag_msg.code {
        // DIAGCTL_CODE_DIAG = 1: output diagnostic message
        // C: do_diagctl.c:28-44 — data_copy from caller, then kputc each byte
        1 => {
            use crate::cross_space::data_copy_vmcheck;
            use crate::vm::{AddressRef, CrossSpaceResult};
            use minix_arch::{CurrentDirectMap, DirectMapArch};
            use minix_arch::{CurrentEarlyConsole as Console, EarlyConsole};
            use minix_types::{Endpoint, VirBytes};

            /// Buffer too large. C: E2BIG = 7
            const E2BIG: i32 = 7;
            /// Bad address. C: EFAULT = 14
            const EFAULT: i32 = 14;
            /// C: DIAGBUFSIZE — kernel/const.h
            const DIAGBUFSIZE: usize = 128;

            let len = diag_msg.len as usize;
            if len > DIAGBUFSIZE {
                return KcallResult::Ok(E2BIG);
            }
            if len == 0 {
                return KcallResult::Ok(0);
            }

            // Capture caller's endpoint and CR3 before mutable borrow.
            let caller_endpt = proc_table
                .get(caller_nr)
                .map(|p| p.p_endpoint)
                .expect("dispatch_diagctl: caller slot must exist");
            let caller_cr3 = proc_table
                .get(caller_nr)
                .map(|p| p.p_seg.phys_root)
                .expect("dispatch_diagctl: caller slot must exist");

            let mut diagbuf = [0u8; DIAGBUFSIZE];
            // `diagbuf` is a kernel-STACK local, not a kernel-image VA, so the
            // boot-span identity `kern_phys_base + (va - kern_virt_base)` (valid
            // only for the linearly-mapped image) under-flows here and yields a
            // stray-bit47 pseudo-phys; writing through it would clobber an
            // arbitrary physical page. SYSCALL does not switch CR3, so the
            // caller's page table maps the kernel higher-half including this
            // stack frame — resolve the destination through `AddressRef::Process`
            // on `caller_endpt` (a real page-table walk → the true phys), the same
            // pattern as the `copy_struct_from_user` and mcontext sites.
            let stack_va = diagbuf.as_mut_ptr() as u64;

            let proc_cr3 = |pt: &crate::proc_table::ProcessTable, endpt: Endpoint| {
            if endpt == caller_endpt {
                Some(caller_cr3)
            } else {
                pt.endpoint_to_nr(endpt)
                    .and_then(|nr| pt.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };

            let src = AddressRef::Process {
                endpoint: caller_endpt,
                offset: VirBytes(diag_msg.buf),
            };
            let dst = AddressRef::Process {
                endpoint: caller_endpt,
                offset: VirBytes(stack_va),
            };

            match data_copy_vmcheck(caller_nr, proc_table, src, dst, len, proc_cr3) {
                CrossSpaceResult::Completed(Ok(())) => {
                    // NK4C 续-279a：minix-rt 的 OOM 诊断行只走裸 kernel call 腿（拿不到
                    // 自身 pid），在内核侧补打 caller proc 号完成归因——**必须在
                    // data_copy 成功后**检查内容（拷贝前 diagbuf 全零，前置检查永不
                    // 命中），仅命中前缀时多打一行，普通 bootmark 零影响；结案随
                    // 诊断 mark 族一并滚除。
                    if diagbuf[..len].starts_with(b"nk4c: OOM-RT") {
                        use minix_plat::{CurrentEarlyConsole as DiagConsole, EarlyConsole as _};
                        DiagConsole::write_str("nk4a: oomrt caller=");
                        DiagConsole::write_hex(caller_nr.0 as u64);
                        DiagConsole::write_str("\n");
                    }
                    // §续-316 krewalk 扩展（用后即滚）：VM 报「not a valid
                    // ELF」时，内核侧重读 handoff 的 boot_procs 镜像首 16B
                    // ——证「RAM=有效 ELF vs VM 裸 DM 直读视图=垃圾」分歧。
                    if diagbuf[..len].windows(15).any(|w| w == b"not a valid ELF") {
                        use minix_plat::{CurrentEarlyConsole as ElfConsole, EarlyConsole as _};
                        const KDM: u64 = 0xFFFF_FFC0_4000_0000;
                        // 直接解析 boot 文件表（TABLE_PA=0x85000000，MNXBOOT1
                        // 格式=MAGIC u64+count u32+pad u32+16×(path[64]+pa
                        // u64+len u64)）——内核 bootface 同源数据。
                        let tbl = 0x8500_0000u64;
                        let magic = unsafe {
                            ((KDM + tbl) as *const u64).read_volatile()
                        };
                        ElfConsole::write_str("nk4c: elfchk magic=");
                        ElfConsole::write_hex(magic);
                        Console::write_str("\n");
                        if magic == 0x3154_4f4f_4258_4e4d {
                            let count = unsafe {
                                ((KDM + tbl + 8) as *const u32).read_volatile()
                            };
                            for n in 0..count.min(16) as u64 {
                                let base = tbl + 16 + n * (64 + 16);
                                let pa = unsafe {
                                    ((KDM + base + 64) as *const u64).read_volatile()
                                };
                                let ln = unsafe {
                                    ((KDM + base + 72) as *const u64).read_volatile()
                                };
                                if pa == 0 {
                                    continue;
                                }
                                let mut b8 = [0u8; 8];
                                for (i, b) in b8.iter_mut().enumerate() {
                                    *b = unsafe {
                                        ((KDM + pa + i as u64) as *const u8).read_volatile()
                                    };
                                }
                                ElfConsole::write_str("nk4c: elfchk pa=");
                                ElfConsole::write_hex(pa);
                                ElfConsole::write_str(" len=");
                                ElfConsole::write_hex(ln);
                                ElfConsole::write_str(" b8=");
                                for b in b8 {
                                    ElfConsole::write_hex(b as u64);
                                }
                                ElfConsole::write_str("\n");
                            }
                        }
                    }
                    // 续-298 krewalk 配套捕获（用后即滚）：fill-root 探针行的
                    // §续-368 实验丁（关中断臂）魔术串：探针在测试计算窗
                    // 口前后各发一条，内核置/清 arch 旗标（riscv64 用户返回
                    // 处跳过 SPIE 置位）。旗标由 set_active_root_tracked
                    // 在任何地址空间切换时自动清零——粘滞窗不越过调度点，
                    // 测试进程 abort/exit 也安全。
                    #[cfg(all(target_arch = "riscv64", not(feature = "mock")))]
                    if diagbuf[..len].starts_with(b"NK4C-CLI-ON") {
                        use core::sync::atomic::Ordering as AtomicOrd;
                        minix_arch::riscv64::trap_return::NK4C_CLI
                            .store(1, AtomicOrd::Relaxed);
                        return KcallResult::Ok(0);
                    }
                    #[cfg(all(target_arch = "riscv64", not(feature = "mock")))]
                    if diagbuf[..len].starts_with(b"NK4C-CLI-OFF") {
                        use core::sync::atomic::Ordering as AtomicOrd;
                        minix_arch::riscv64::trap_return::NK4C_CLI
                            .store(0, AtomicOrd::Relaxed);
                        return KcallResult::Ok(0);
                    }

                    // ptroot/pte_pa 存全局，供内核 pfvm 冷路径读「fill-root
                    // 刚写的叶槽」在故障时刻的现值（判 RAM 脏 vs 视图错位）。
                    if diagbuf[..len].starts_with(b"nk4a: fill-root") {
                        if let Some(r) = find_hex_field(&diagbuf[..len], b"ptroot=") {
                            crate::trap_dispatch::LAST_FILL_PTROOT.store(r, core::sync::atomic::Ordering::Relaxed);
                        }
                        if let Some(p) = find_hex_field(&diagbuf[..len], b"pte_pa=") {
                            crate::trap_dispatch::LAST_FILL_LEAF_PA.store(p, core::sync::atomic::Ordering::Relaxed);
                        }
                        // §续-336 KDM 走链对账（用后即滚）：fill 写后立即从
                        // 内核视角走 ptroot 链读叶 PTE——判「VmDm 写落错帧」
                        // vs「QEMU walk 视角不一致」。va 从消息里提。
                        #[cfg(all(target_arch = "riscv64", not(feature = "mock")))]
                        {
                            use minix_plat::{CurrentEarlyConsole as FwConsole, EarlyConsole as _};
                            const KDM: u64 = 0xFFFF_FFC0_4000_0000;
                            if let Some(va) = find_hex_field(&diagbuf[..len], b"va=") {
                                let root = find_hex_field(&diagbuf[..len], b"ptroot=").unwrap_or(0);
                                if root >= 0x8000_0000 {
                                    let kdm = |pa: u64| unsafe {
                                        ((KDM + pa) as *const u64).read_volatile()
                                    };
                                    let mut pa = root;
                                    let mut lvl = 2u8;
                                    let mut ok = true;
                                    let mut leaf_val: u64 = 0;
                                    for sh in [30u64, 21, 12] {
                                        let idx = (va >> sh) & 511;
                                        let slot = pa + idx * 8;
                                        if slot < 0x8000_0000 || slot >= 0xA000_0000 {
                                            ok = false;
                                            break;
                                        }
                                        let v = kdm(slot);
                                        FwConsole::write_str("nk4c: fw lvl=");
                                        FwConsole::write_hex(lvl as u64);
                                        FwConsole::write_str(" slot=");
                                        FwConsole::write_hex(slot);
                                        FwConsole::write_str(" pte=");
                                        FwConsole::write_hex(v);
                                        FwConsole::write_str("\n");
                                        if v & 1 == 0 { ok = false; break; }
                                        if sh == 12 { leaf_val = v; }
                                        pa = ((v >> 10) & 0xF_FFFF_F) << 12;
                                        lvl -= 1;
                                    }
                                    FwConsole::write_str("nk4c: fw done ok=");
                                    FwConsole::write_hex(ok as u64);
                                    FwConsole::write_str(" leaf=");
                                    FwConsole::write_hex(leaf_val);
                                    FwConsole::write_str("\n");
                                }
                            }
                        }
                    }
                    // C: do_diagctl.c:38-42 — kputc each byte. E-ISKMESS:
                    // the kmess ring is the C kputc accumulation half —
                    // record here so the IS `kmessages_dmp` replay
                    // (GET_KMESSAGES) has the same content the console
                    // shows.
                    crate::kmess::record_bytes(&diagbuf[..len]);
                    for &byte in &diagbuf[..len] {
                        Console::write_byte(byte);
                    }
                    KcallResult::Ok(0)
                }
                CrossSpaceResult::Completed(Err(_)) => {
                    // NK4-C 1.10y2 取证探针（task1-close 裁决删除）：diagctl
                    // 拷贝失败（源页未映射）——sched 侧诊断打点静默失败的
                    // 机制定位。
                    #[cfg(not(feature = "mock"))]
                    {
                        use minix_plat::{CurrentEarlyConsole as Console2, EarlyConsole as _};
                        Console2::write_str("nk4a: diag-efault caller=");
                        Console2::write_hex(caller_nr.0 as u64);
                        Console2::write_str("\n");
                    }
                    KcallResult::Ok(EFAULT)
                }
                CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
            }
        }

        // DIAGCTL_CODE_STACKTRACE = 2: print process stack trace
        // C: do_diagctl.c:45-48 — isokendpt + proc_stacktrace
        2 => {
            // C: isokendpt(m_ptr->m_lsys_krn_sys_diagctl.endpt, &proc_nr)
            let target_endpt = Endpoint(diag_msg.endpt);
            let target_nr = match proc_table.endpoint_to_nr(target_endpt) {
                Some(nr) => nr,
                None => return KcallResult::Ok(EINVAL),
            };

            // Delegate to the shared `proc_stacktrace` helper. This is the
            // same function the `cause_signal` fatal SELF panic path uses
            // (system.c:429), so DIAGCTL and the panic output share the
            // exact same walk + output formatting — no drift.
            //
            // # Implementation notes
            //
            // C's `proc_stacktrace` has a special case for KTS_SYSENTER /
            // KTS_SYSCALL trap styles where the full register context is not
            // in `p_reg` — it reads the frame pointer from the user stack at
            // `sp+16`. Rust does not yet track per-process trap_style, so we
            // use the stored frame pointer from `CpuContext` (the default
            // path). This is correct for aarch64/riscv64 (where the trap
            // always saves the full context) and for x86_64 when the trap
            // entry path properly saves RBP.
            //
            // # Page fault handling
            //
            // C uses `data_copy` (not `data_copy_vmcheck`) and treats any
            // failure as "stop walking". The shared helper maps both
            // `Completed(Err(_))` and `Suspended(_)` to `None` so the walk
            // stops with the C-equivalent placeholder.
            let target = match proc_table.get(target_nr) {
                Some(p) => p,
                None => return KcallResult::Ok(EINVAL),
            };
            crate::stacktrace::proc_stacktrace(target);

            KcallResult::Ok(OK)
        }

        // DIAGCTL_CODE_REGISTER = 3: register for SIGKMESS
        // C: do_diagctl.c:49-56 — check SYS_PROC, set s_diag_sig=TRUE,
        //   if kmess.km_size > 0 && !kinfo.do_serial_debug: send_sig
        3 => {
            let Some(priv_id) = proc_table.get(caller_nr).and_then(|p| p.priv_id) else {
                return KcallResult::Ok(EPERM);
            };
            match priv_table.get_mut(priv_id) {
                Some(p) => {
                    if !p.is_sys_proc() {
                        return KcallResult::Ok(EPERM);
                    }
                    p.mem.s_diag_sig = true;
                    // D-14 (2026-09-06 设计 no-op，W-7 连带结论)：
                    // C do_diagctl.c:54-56 — `if (kmess.km_size > 0 &&
                    // !kinfo.do_serial_debug) send_sig(caller->p_endpoint,
                    // SIGKMESS)`（目标是注册者自身，非 todo 原文所写 PM）。
                    // 两个条件输入在 W-7 演进下均不存在：kmess 缓冲已被
                    // EarlyConsole 直出替代（km_size 恒无意义），唯一消费
                    // 者 log 驱动（log.c:115 读 kmess）角色同被替代；
                    // SIGKMESS=72（sys/sys/signal.h:272）>64 亦超 SigSet
                    // 位宽。故本通知按 C 自身条件恒不触发——订阅状态
                    // （s_diag_sig 置位/复位/SET_SYS 清除，kpriv.rs
                    // reset_pending_ipc）完整保留。update 状态转移的
                    // s_diag_sig 保全已闭合（misc.rs:1845 捕获、:1887
                    // 回写，对齐 C do_update.c:293 adjust_priv_slot）；
                    // 同点对账发现 s_alarm_timer 未保全为真实缺口
                    // （C do_update.c:292，Rust 侧零处理），见 todo §22 U-1。
                    KcallResult::Ok(0)
                }
                None => KcallResult::Ok(EPERM),
            }
        }

        // DIAGCTL_CODE_UNREGISTER = 4: unregister from SIGKMESS
        // C: do_diagctl.c:57-60 — check SYS_PROC, set s_diag_sig=FALSE
        4 => {
            let Some(priv_id) = proc_table.get(caller_nr).and_then(|p| p.priv_id) else {
                return KcallResult::Ok(EPERM);
            };
            match priv_table.get_mut(priv_id) {
                Some(p) => {
                    if !p.is_sys_proc() {
                        return KcallResult::Ok(EPERM);
                    }
                    p.mem.s_diag_sig = false;
                    KcallResult::Ok(0)
                }
                None => KcallResult::Ok(EPERM),
            }
        }

        // Unknown request code
        _ => KcallResult::Ok(EINVAL),
    }
}
fn dispatch_vtimer(caller_nr: crate::proc::ProcNr, msg: &mut Message, priv_table: &PrivTable, proc_table: &crate::proc_table::ProcessTable) -> KcallResult { crate::syscall_clock::dispatch_vtimer(caller_nr, msg, priv_table, proc_table) }
fn dispatch_runctl(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult { crate::syscall_process::dispatch_runctl(caller_nr, proc_table, msg) }
/// Dispatch SYS_GETMCONTEXT.
///
/// C: `do_getmcontext()` — do_mcontext.c:23-68
///
/// # Implementation (2026-08-01)
///
/// Fully implemented for 64-bit:
/// 1. Endpoint validation (EINVAL)
/// 2. Kernel process rejection (EPERM)
/// 3. Copy mcontext from user → kernel via `data_copy_vmcheck`
/// 4. Zero `mc_flags` via `SignalContext::mcontext_clear_flags`
/// 5. Copy mcontext back to user via `data_copy_vmcheck`
///
/// The x86-32 FPU fast path (`proc_used_fpu` + `save_fpu` + `memcpy fpu_state`)
/// is `#if defined(__i386__)` in C — not applicable on 64-bit (lazy FPU).
fn dispatch_getmcontext(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &Message,
) -> KcallResult {
    use minix_arch::{CurrentSignalContext as SC, SignalContext};
    use crate::cross_space::data_copy_vmcheck;
    use crate::vm::{AddressRef, CrossSpaceResult};
    use minix_types::{Endpoint, VirBytes};

    // C: do_mcontext.c:13-14 — extract from mess_lsys_krn_sys_getmcontext.
    msg.debug_check_m_type_any(&[Syscall::Getmcontext as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let mc_msg = unsafe { msg.m_u.m_lsys_krn_sys_mcontext };
    let endpt = mc_msg.endpt;
    let ctx_ptr = mc_msg.ctx_ptr;

    // C: do_mcontext.c:26-27 — isokendpt(endpt, &proc_nr).
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_mcontext.c:28 — iskerneln(proc_nr) → EPERM.
    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_mcontext.c:31-39 (x86 only): proc_used_fpu fast path.
    // On 64-bit, there is no FPU fast path — modern 64-bit architectures
    // use lazy FPU initialization (no MF_FPU_INITIALIZED flag).

    // C: do_mcontext.c:42-45 — data_copy mcontext from user → kernel.
    let caller_endpt = proc_table
        .get(caller_nr)
        .map(|p| p.p_endpoint)
        .expect("mcontext: caller slot must exist");
    let caller_cr3 = proc_table
        .get(caller_nr)
        .map(|p| p.p_seg.phys_root)
        .expect("mcontext: caller slot must exist");

    let mut mc = <SC as SignalContext>::Mcontext::default();
    let mc_size = core::mem::size_of_val(&mc);
    // NK4-C F10c：mc 是内核栈局部变量，VA 在 higher-half 段 [KERN_VIRT_BASE,
    // KERNEL_DM_BASE)，`virt_to_phys` 对该段做 VM_DM 减法得到错误 PA。
    // 改为 AddressRef::Process：caller 的 CR3 同时映射内核 higher-half，
    // resolve_physical 走真实页表得正确 PA（同 copy_struct_from_user 修法）。
    // addr_of_mut! 不构造 shared 引用（评审 P1）：copy 向 mc 写入不应与 &mc 共存。
    let mc_vaddr = core::ptr::addr_of_mut!(mc) as u64;

    // First copy: user → kernel.
    {
        // K20: value-capturing closure — `src` 和 `dst` 均使用 caller_endpt，
        // 闭包直接返回 caller_cr3，不咨询 proc_table。
        let proc_cr3 = |pt: &crate::proc_table::ProcessTable, ept: Endpoint| {
            if ept == caller_endpt {
                Some(caller_cr3)
            } else {
                pt.endpoint_to_nr(ept)
                    .and_then(|nr| pt.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };
        let src = AddressRef::Process {
            endpoint: Endpoint(endpt),
            offset: VirBytes(ctx_ptr),
        };
        let dst = AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(mc_vaddr),
        };
        match data_copy_vmcheck(caller_nr, &mut *proc_table, src, dst, mc_size, proc_cr3) {
            CrossSpaceResult::Completed(Ok(())) => {}
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
        }
    }

    // C: do_mcontext.c:47 — mc.mc_flags = 0.
    // On 64-bit, no FPU copy (#if defined(__i386__) only).
    SC::mcontext_clear_flags(&mut mc);

    // C: do_mcontext.c:60-65 — data_copy mcontext from kernel → user.
    {
        // K20: value-capturing closure (see the getmcontext note).
        let proc_cr3 = |pt: &crate::proc_table::ProcessTable, ept: Endpoint| {
            if ept == caller_endpt {
                Some(caller_cr3)
            } else {
                pt.endpoint_to_nr(ept)
                    .and_then(|nr| pt.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };
        let src = AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(mc_vaddr),
        };
        let dst = AddressRef::Process {
            endpoint: Endpoint(endpt),
            offset: VirBytes(ctx_ptr),
        };
        match data_copy_vmcheck(caller_nr, &mut *proc_table, src, dst, mc_size, proc_cr3) {
            CrossSpaceResult::Completed(Ok(())) => {}
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
        }
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_SETMCONTEXT.
///
/// C: `do_setmcontext()` — do_mcontext.c:74-104
///
/// # Implementation (2026-08-01)
///
/// Fully implemented for 64-bit:
/// 1. Endpoint validation (EINVAL)
/// 2. Copy mcontext from user → kernel via `data_copy_vmcheck`
///
/// The x86-32 FPU state copy (`mc_flags & _MC_FPU_SAVED → memcpy fpu_state`)
/// is `#if defined(__i386__)` in C — not applicable on 64-bit (lazy FPU).
fn dispatch_setmcontext(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &Message,
) -> KcallResult {
    use minix_arch::{CurrentSignalContext as SC, SignalContext};
    use crate::cross_space::data_copy_vmcheck;
    use crate::vm::{AddressRef, CrossSpaceResult};
    use minix_types::{Endpoint, VirBytes};

    // C: do_mcontext.c:64-65 — extract from mess_lsys_krn_sys_setmcontext.
    msg.debug_check_m_type_any(&[Syscall::Setmcontext as i32]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    let mc_msg = unsafe { msg.m_u.m_lsys_krn_sys_mcontext };
    let endpt = mc_msg.endpt;
    let ctx_ptr = mc_msg.ctx_ptr;

    // C: do_mcontext.c:64 — isokendpt(endpt, &proc_nr).
    let _target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_mcontext.c:67-69 — data_copy mcontext from user → kernel.
    // On 64-bit, no FPU copy (#if defined(__i386__) only).
    let caller_endpt = proc_table
        .get(caller_nr)
        .map(|p| p.p_endpoint)
        .expect("mcontext: caller slot must exist");
    let caller_cr3 = proc_table
        .get(caller_nr)
        .map(|p| p.p_seg.phys_root)
        .expect("mcontext: caller slot must exist");

    let mut mc = <SC as SignalContext>::Mcontext::default();
    let mc_size = core::mem::size_of_val(&mc);
    // NK4-C F10c：mc 是内核栈局部变量，VA 在 higher-half 段 [KERN_VIRT_BASE,
    // KERNEL_DM_BASE)，`virt_to_phys` 对该段做 VM_DM 减法得到错误 PA。
    // 改为 AddressRef::Process：caller 的 CR3 同时映射内核 higher-half，
    // resolve_physical 走真实页表得正确 PA（同 copy_struct_from_user 修法）。
    // addr_of_mut! 不构造 shared 引用（评审 P1）：copy 向 mc 写入不应与 &mc 共存。
    let mc_vaddr = core::ptr::addr_of_mut!(mc) as u64;

    // K20: value-capturing closure — src 和 dst 均使用 caller_endpt，闭包返回 caller_cr3，不咨询 proc_table。
        let proc_cr3 = |pt: &crate::proc_table::ProcessTable, ept: Endpoint| {
            if ept == caller_endpt {
                Some(caller_cr3)
            } else {
                pt.endpoint_to_nr(ept)
                    .and_then(|nr| pt.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };
    let src = AddressRef::Process {
        endpoint: Endpoint(endpt),
        offset: VirBytes(ctx_ptr),
    };
    let dst = AddressRef::Process {
        endpoint: caller_endpt,
        offset: VirBytes(mc_vaddr),
    };
    match data_copy_vmcheck(caller_nr, &mut *proc_table, src, dst, mc_size, proc_cr3) {
        CrossSpaceResult::Completed(Ok(())) => {}
        CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
    }

    // C: do_mcontext.c:71-101 (x86 only): FPU state copy + release_fpu.
    // On 64-bit, no FPU copy — return OK directly.
    KcallResult::Ok(OK)
}
fn dispatch_update(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &mut PrivTable) -> KcallResult { crate::misc::dispatch_update(caller_nr, proc_table, msg, priv_table) }
fn dispatch_schedctl(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) -> KcallResult { crate::syscall_process::dispatch_schedctl(caller_nr, proc_table, msg) }
fn dispatch_statectl(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &mut PrivTable, pool: &mut crate::ipc_filter::IpcFilterPool) -> KcallResult {
    crate::syscall_process::dispatch_statectl(caller_nr, proc_table, msg, priv_table, pool)
}
fn dispatch_safememset(caller_nr: crate::proc::ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message, priv_table: &crate::kpriv::PrivTable) -> KcallResult { crate::syscall_copy::dispatch_safememset(caller_nr, proc_table, msg, priv_table) }

// ── kernel_call_finish / kernel_call_resume ──
// C: system.c:58-90 (kernel_call_finish), system.c:612-638 (kernel_call_resume)

use crate::proc::{MiscFlagsBits, RtsFlagsBits};
use minix_types::Endpoint;

// EBADREQUEST and ECALLDENIED now come from `crate::errno` (FIX-01: R-09).
// SYSTEM endpoint source for kernel replies. C: SYSTEM = -2 (proc.h)
// Use Endpoint::SYSTEM constant from minix-types instead of raw i32.

/// Copy a message to user space via the process's delivermsg buffer.
///
/// C: `copy_msg_to_user(msg, (message *)caller->p_delivermsg_vir)` — system.c:82
///
/// In Minix3, this uses `phys_copy` to copy the message from kernel space
/// to the user-space address stored in `p_delivermsg_vir`. In the Rust
/// rewrite, we store the reply in `p_delivermsg` (kernel-side buffer)
/// and set the `MF_DELIVERMSG` flag so the IPC engine delivers it on
/// the next `switch_to_user` cycle.
///
/// This approach avoids direct user-space memory writes from the syscall
/// dispatch path, which is safer and aligns with the IPC engine's
/// message delivery mechanism (`MF_DELIVERMSG` flag + `p_delivermsg`,
/// consumed by `ipc::delivermsg`).
fn copy_msg_to_user(caller_nr: ProcNr, proc_table: &mut crate::proc_table::ProcessTable, msg: &Message) {
    let caller = proc_table
        .get_mut(caller_nr)
        .expect("copy_msg_to_user: caller slot must exist");
    caller.p_delivermsg = *msg;
    caller.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
}

/// Finish a kernel call: handle VMSUSPEND or copy result to user.
///
/// C: `kernel_call_finish()` — system.c:58-90
///
/// # BKL (Big Kernel Lock)
///
/// This function releases the BKL before returning. The BKL was acquired
/// in `kernel_call_dispatch()` and must be held throughout the dispatch +
/// finish sequence. We release it here because:
///
/// 1. **Normal completion**: The syscall is done, shared state is consistent.
///    The caller will enter `switch_to_user()` which does not need BKL
///    (it only reads per-CPU state and performs the mode switch).
///
/// 2. **VmSuspend**: The process is waiting for VM. The BKL must be released
///    so other CPUs can enter the kernel while this process is suspended.
///    When VM replies, `kernel_call_resume()` will re-acquire the BKL.
///
/// This matches C's pattern where BKL is released before `switch_to_user()`
/// (or before blocking in IPC sendrecv).
pub fn kernel_call_finish(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &Message,
    result: KcallResult,
    priv_table: &mut PrivTable,
) {
    kernel_call_finish_holding_bkl(caller_nr, proc_table, msg, result, priv_table, true, true);
}

/// `kernel_call_finish` for the int-33 IPC trap door (mini_send /
/// mini_receive / mini_senda leg in `trap_dispatch`).
///
/// C parity (NK4-C S3 根因修复): the eager reply copy
/// `copy_msg_to_user(msg, p_delivermsg_vir)` exists ONLY on the
/// `kernel_call()` leg (system.c:83) — that leg always refreshes
/// `p_delivermsg_vir` at entry (system.c:141), so the target is by
/// construction a live buffer. The C trap leg (proc.c `mini_*`) never
/// writes the caller's message buffer from a finish path: status rides
/// `h_errno`/registers back through the stub, and a real reply rides the
/// `MF_DELIVERMSG` delivery machinery. Rust routed the int-33 door
/// through the same finish machine without the door discipline, so a
/// SENDA window (which deliberately does NOT refresh `p_delivermsg_vir`,
/// C parity proc.c:983) could complete an errno reply into a *stale*
/// delivermsg address — an already-popped user stack frame (real-machine
/// Task C: 80-byte reply clobbered a live slot, `self=0` SIGSEGV).
/// This variant keeps every other bookkeeping step (VmSuspend parking,
/// NoReply dequeue, saved_msg cleanup, BKL release) but skips the eager
/// reply write, and stamps the door onto a freshly parked suspend
/// context so a later stage-3a re-dispatch of the same call skips it too.
pub fn kernel_call_finish_ipc_door(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &Message,
    result: KcallResult,
    priv_table: &mut PrivTable,
) {
    kernel_call_finish_holding_bkl(caller_nr, proc_table, msg, result, priv_table, true, false);
}

/// `kernel_call_finish` without the BKL release, for re-dispatch contexts
/// that already run under the scheduler loop's held BKL (scheduler_loop
/// stage 3a KCALL_RESUME consume). C calls `kernel_call_finish` from
/// `kernel_call_resume` (system.c:636) while the scheduler holds the
/// kernel lock — the Rust unlock sites exist for the trap-entry dispatch
/// chain only, so they are conditional here.
pub(crate) fn kernel_call_finish_holding_bkl(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    msg: &Message,
    result: KcallResult,
    priv_table: &mut PrivTable,
    release_bkl: bool,
    eager_reply_copy: bool,
) {
    // B1: the dispatch entry transferred a held BKL into this chain
    // (kernel_call_dispatch / dispatch_ipc_entry via `transfer()`); the
    // VmSuspend branch below releases it. A lost lock must fail loudly.
    debug_assert!(crate::smp::bkl_is_locked(), "kernel_call_finish entered without the BKL held");
    // VmSuspend path: save msg + set MF_KCALL_RESUME + release BKL.
    // C: system.c:60-63 — `if (result == VMSUSPEND) { saved.reqmsg = *msg;
    // p_misc_flags |= MF_KCALL_RESUME; }`
    if matches!(result, KcallResult::VmSuspend) {
        if let Some(ctx) = proc_table
            .get_mut(caller_nr)
            .expect("kernel_call_finish: caller slot must exist")
            .p_vm_suspend
            .as_mut()
        {
            ctx.saved_msg = Some(*msg);
            // 门纪律（NK4-C S3）：IPC 陷阱腿的挂起要把门标记带上——
            // 补完成时同样不得 eager 直写回执（否则恢复后向陈旧
            // p_delivermsg_vir 落写）。重派再次挂起时新建的 ctx 不继承
            // 此标记，门归属由 stage 3a 从旧 ctx 读出后以 eager=false
            // 重新落到新 ctx 上（见 lib.rs KCALL_RESUME 消费块）。
            if !eager_reply_copy {
                ctx.resume_skip_eager_reply = true;
            }
        }
        proc_table
            .get_mut(caller_nr)
            .expect("kernel_call_finish: caller slot must exist")
            .p_misc_flags
            .set(MiscFlagsBits::KCALL_RESUME);
        // D-20 (C vm_suspend proc.c:253-257): enqueue into the global VM
        // request chain + wake up VM via mini_notify(SYSTEM→VM). A1: the
        // tables arrive as parameters (split borrows established by
        // kernel_call's own caller) — no global accessor needed.
        // c41 探针（task1-close 裁决删除）：SYSCALL 腿每次挂起的完整现场
        // ——caller 端点/nr + ctx.target + 故障范围 + m_type + pdmv，
        // 定位 c40 崩溃 memreq（target=11 start=0x0 len=0x90）的请求者。
        #[cfg(not(feature = "mock"))]
        #[cfg(target_arch = "x86_64")]
        {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
            {
                static SUS: AtomicUsize = AtomicUsize::new(0);
                if SUS.fetch_add(1, AtomicOrd::Relaxed) < 48 {
                    let p = proc_table.get(caller_nr);
                    let (ep, pdmv) =
                        p.map_or((0i32, 0u64), |q| (q.p_endpoint.0, q.p_delivermsg_vir.0));
                    let ctxf = p
                        .and_then(|q| q.p_vm_suspend.as_ref())
                        .map(|c| (c.target.0, c.check_params.start.0, c.check_params.length.0));
                    C0::write_str("nk4a: susp-krn nr=0x");
                    C0::write_hex(caller_nr.0 as u64);
                    C0::write_str(" ep=0x");
                    C0::write_hex(ep as u64);
                    C0::write_str(" mt=0x");
                    C0::write_hex(msg.m_type as u64);
                    C0::write_str(" tgt=0x");
                    C0::write_hex(ctxf.map_or(0, |t| t.0) as u64);
                    C0::write_str(" st=0x");
                    C0::write_hex(ctxf.map_or(0, |t| t.1));
                    C0::write_str(" ln=0x");
                    C0::write_hex(ctxf.map_or(0, |t| t.2));
                    C0::write_str(" pdmv=0x");
                    C0::write_hex(pdmv);
                    C0::write_str("\n");
                }
            }
        }
        proc_table.vm_enqueue_and_notify_vm(caller_nr, priv_table);
        // Release BKL — process is suspended waiting for VM.
        // Other CPUs can enter the kernel while we wait.
        // kernel_call_resume() will re-acquire BKL when VM replies.
        // (Skipped on the holding-bkl re-dispatch form: the scheduler
        // loop's lock outlives this call.)
        if release_bkl {
            crate::smp::bkl_unlock();
        }
        return;
    }

    // Non-VmSuspend path (Ok / NoReply / BadCall / CallDenied):
    // C: system.c:64-89 — single else-branch handles all non-VMSUSPEND cases
    // uniformly: clear saved_msg + optional reply + (BKL released below).
    //
    // Previous implementation scattered this across 4 match arms with 4×
    // `bkl_unlock()` and 3× duplicated reply construction; the unified path
    // also fixes a latent bug where BadCall/CallDenied skipped
    // `saved_msg = None` cleanup (harmless in practice because BadCall/
    // CallDenied cannot follow a VmSuspend, but diverges from C semantics).
    if let Some(ctx) = proc_table
        .get_mut(caller_nr)
        .expect("kernel_call_finish: caller slot must exist")
        .p_vm_suspend
        .as_mut()
    {
        ctx.saved_msg = None;
    }

    // NoReply = the caller is now blocked. The IPC engine set the blocking
    // RTS flag with the primitive `RtsFlags::set` (it holds a procs slice,
    // not the run queues) — complete the block by dequeuing the caller,
    // the dequeue half of C's `RTS_SET` macro. Without this the blocked
    // caller stays queued and the scheduler spin-picks it forever
    // (observed on real machine: 86k picks of a receiver parked in
    // `RTS_RECEIVING`, test-sysboot C-27 carrier). Both IPC doors
    // (int-33 and the syscall leg) route their NoReply through here.
    if matches!(result, KcallResult::NoReply) {
        proc_table.dequeue_if_blocked(caller_nr);
    }

    if let Some(wire) = result.reply_wire().filter(|_| {
        // 门纪律（NK4-C S3，见 `kernel_call_finish_ipc_door` 文档）：
        // eager 回执直写只属于 kernel_call()/SYSCALL 腿；IPC 陷阱腿
        //（eager_reply_copy=false）与被它挂起的调用恢复重派
        //（resume_skip_eager_reply）都不写——错误已经随 RAX 交付，
        // 真回执由 MF_DELIVERMSG 投递机制负责。
        let door_skip = proc_table.get(caller_nr).is_some_and(|p| {
            p.p_vm_suspend
                .as_ref()
                .is_some_and(|c| c.resume_skip_eager_reply)
        });
        eager_reply_copy && !door_skip
    }) {
        let mut reply = *msg;
        reply.m_source = Endpoint::SYSTEM;
        // NK4-C F10b（P0-wire）：SYSCALL 腿线上值由 `reply_wire()` 给出：
        // 错误码（Ok）取负，数据码（Data，如 VMPTYPE_CHECK/ENOENT-as-data）原样传递。
        reply.m_type = wire;
        // C system.c:71-77 对位：结果消息 phys_copy 直写调用者用户缓冲
        // （p_delivermsg_vir），不经 DELIVERMSG。旧实现经 p_delivermsg+
        // DELIVERMSG 投递——调用者（VM）的 sef_receive 随即消费自己的
        // 回执（src=SYSTEM/type=errno）形成接收自旋，饿死其他进程
        // （NK4-A C-3 真机：VM exec 后 receive 空转、RS 永久饿死，
        // 2026-09-22）。
        let root = proc_table.get(caller_nr).map(|p| p.p_seg.phys_root);
        let buf_va = proc_table.get(caller_nr).map(|p| p.p_delivermsg_vir.0);
        use minix_arch::DirectMapArch as _;
        // NK4-C S2c 哨兵（task1-close 裁决删除）：若调用者是 RS，直写
        // 前重读监视 PTE——回执 DM 直写自身就是候选抹写者，写前观测
        // 能自证清白/有罪。LAST 状态全局，必须按 ep==2 门控。
        #[cfg(not(feature = "mock"))]
        #[cfg(target_arch = "x86_64")]
        if let Some(r) = root.filter(|_| {
            proc_table
                .get(caller_nr)
                .is_some_and(|p| p.p_endpoint.0 == 2)
        }) {
            crate::trap_dispatch::nk4a_pte_watch("finw", r.0);
        }
        if let (Some(root), Some(buf_va)) = (root, buf_va) {
            let bytes = core::mem::size_of::<minix_types::Message>();
            use minix_arch::DirectMapArch as _;
            // NK4-C S2e 现场打印（task1-close 裁决删除）：RS 回执直写的
            // 每 chunk 目标与写入首字——去重探针会掩盖重复写，抹写
            // 收尾阶段需要无条件逐次证据（S2g 提到 4096：48 条在启动期
            // 耗尽，崩溃窗口无现场）。
            #[cfg(not(feature = "mock"))]
            #[cfg(target_arch = "x86_64")]
            let rs_trace = proc_table
                .get(caller_nr)
                .is_some_and(|p| p.p_endpoint.0 == 2);
            let mut off = 0usize;
            while off < bytes {
                let va = buf_va + off as u64;
                // 调用者页表翻译（其 CR3 未激活时经 DM 直写物理页）。
                match crate::pte_walk::walk_x86_64(root, minix_types::VirBytes(va)) {
                    Some((pa, _fl)) => {
                        // NK4-C S1 取证探针（task1-close 裁决删除）：errno 回执
                        // DM 直写的目标 (va, root, pa) 去重记录，与同轮 PT 页对账。
                        #[cfg(not(feature = "mock"))]
                        crate::trap_dispatch::nk4a_user_write_probe("finw", root.0, va, pa.0);
                        let chunk = core::cmp::min(bytes - off, (0x1000 - (va & 0xfff)) as usize);
                        #[cfg(not(feature = "mock"))]
                        #[cfg(target_arch = "x86_64")]
                        if rs_trace {
                            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
                            static TXN: AtomicUsize = AtomicUsize::new(0);
                            // S2h 评审修复：cap 4096→512（最坏串口耗时量级
                            // 降到秒级，不拖穿 timeout）+ 触顶现形标记 +
                            // 守卫字节读（S2f 旧形态按 [u8;8] 读在 buf_va 非
                            // 8 对齐时会造未对齐引用并越出 reply 尾端）。
                            let tn = TXN.fetch_add(1, AtomicOrd::Relaxed);
                            if tn == 512 {
                                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                                C0::write_str("nk4a: fx-cap\n");
                            }
                            if tn < 512 {
                                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                                let rb = |o: usize| -> u64 {
                                    let base =
                                        &reply as *const minix_types::Message as *const u8;
                                    let mut b = [0u8; 8];
                                    unsafe { // SAFETY: o+i<bytes 保证在 Message(80B) 对象内，逐字节读避免未对齐，越界部分补 0
                                        for i in 0..8 {
                                            if o + i < bytes {
                                                b[i] = *base.add(o + i);
                                            }
                                        }
                                    }
                                    u64::from_le_bytes(b)
                                };
                                C0::write_str("nk4a: fx va=");
                                C0::write_hex(va);
                                C0::write_str(" pa=");
                                C0::write_hex(pa.0);
                                C0::write_str(" len=");
                                C0::write_hex(chunk as u64);
                                C0::write_str(" w0=");
                                C0::write_hex(rb(off));
                                // NK4-C S2g：抹写点落在 buf+56（self 槽），
                                // 把回执行对 buf 基址 +56/+64 的字也打出来
                                // （仅首 chunk），离线直接对照被写入的值。
                                if off == 0 {
                                    C0::write_str(" t56=");
                                    C0::write_hex(rb(56));
                                    C0::write_str(" t64=");
                                    C0::write_hex(rb(64));
                                }
                                C0::write_str("\n");
                            }
                        }
                        let dm = <minix_arch::CurrentDirectMap as minix_arch::DirectMapArch>::kernel_phys_to_virt(minix_types::PhysBytes(pa.0)).0;
                        // SAFETY: DM 窗口覆盖全部物理内存；页为调用者
                        // 驻留的消息缓冲；跨页按页界分块。
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                (&reply as *const minix_types::Message as *const u8).add(off),
                                dm as *mut u8,
                                chunk,
                            );
                        }
                        off += chunk;
                    }
                    None => break, // 尾页未驻留：C phys_copy 同样静默失败
                }
            }
        }
        // NK4-C S2c 哨兵点（task1-close 裁决删除）：直写循环结束后再读
        // 一次——与写前观测夹住本通路，它自身若是抹写者必现形。
        #[cfg(not(feature = "mock"))]
        #[cfg(target_arch = "x86_64")]
        if let Some(r) = root.filter(|_| {
            proc_table
                .get(caller_nr)
                .is_some_and(|p| p.p_endpoint.0 == 2)
        }) {
            crate::trap_dispatch::nk4a_pte_watch("fina", r.0);
        }
    }

    // Release BKL — syscall complete (Ok/NoReply/BadCall/CallDenied).
    // (Skipped on the holding-bkl re-dispatch form — see fn doc.)
    if release_bkl {
        crate::smp::bkl_unlock();
    }
}

/// Resume a previously suspended kernel call (after VM handled the page fault).
///
/// C: `kernel_call_resume()` — system.c:612-638
///
/// # Invariants (C system.c:616-619)
///
/// On entry, the caller must satisfy:
/// 1. `!RTS_SLOT_FREE` — process slot is not being recycled
/// 2. `!RTS_VMREQUEST` — VM has finished processing the fault (flag cleared)
/// 3. `saved_msg.m_source == caller.p_endpoint` — saved message is still
///    sourced from this caller (not corrupted)
///
/// Additionally, `MF_KCALL_RESUME` must be set (set by `kernel_call_finish`
/// VmSuspend path) — its presence proves a prior dispatch returned VmSuspend.
pub fn kernel_call_resume(
    caller_nr: ProcNr,
    proc_table: &mut crate::proc_table::ProcessTable,
    priv_table: &mut PrivTable,
    clock_state: &mut ClockState,
) {
    // C: system.c:616-619 — three invariants + our MF_KCALL_RESUME marker.
    // K20 caller-by-nr: the slot is re-borrowed for each read.
    let caller = proc_table
        .get(caller_nr)
        .expect("kernel_call_resume: caller slot must exist");
    debug_assert!(!caller.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE),
        "kernel_call_resume: caller slot is being freed");
    debug_assert!(!caller.p_rts_flags.is_set(RtsFlagsBits::VMREQUEST),
        "kernel_call_resume: VM has not finished processing the fault");
    debug_assert!(caller.p_misc_flags.is_set(MiscFlagsBits::KCALL_RESUME),
        "kernel_call_resume: MF_KCALL_RESUME not set (no prior VmSuspend)");

    // C: system.c:619 — `saved.reqmsg.m_source == caller->p_endpoint`.
    // The saved message must be sourced from this caller.
    // Using `expect` instead of `unwrap_or_default` so that an invariant
    // violation (missing p_vm_suspend or saved_msg) panics loudly rather
    // than silently dispatching an empty message — the original
    // `unwrap_or_default()` masked corruption bugs.
    let saved_msg = caller.p_vm_suspend.as_ref()
        .and_then(|ctx| ctx.saved_msg)
        .expect("kernel_call_resume: p_vm_suspend.saved_msg must exist \
                 (VmSuspend path in kernel_call_finish always sets it)");
    debug_assert_eq!(saved_msg.m_source, caller.p_endpoint,
        "kernel_call_resume: saved_msg.m_source mismatch");

    let mut msg_copy = saved_msg;

    // C: system.c:627-630 — re-execute the kernel call with MF_KCALL_RESUME
    // still set so the call handler knows this is a retry. The flag is cleared
    // *after* dispatch returns (system.c:635) so it can be set again on a
    // subsequent VMSUSPEND within the same call.
    let result = kernel_call_dispatch(caller_nr, proc_table, &mut msg_copy, priv_table, clock_state);
    proc_table
        .get_mut(caller_nr)
        .expect("kernel_call_resume: caller slot must exist")
        .p_misc_flags
        .clear(MiscFlagsBits::KCALL_RESUME);
    kernel_call_finish(caller_nr, proc_table, &msg_copy, result, priv_table);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::ProcNr;
    use core::sync::atomic::Ordering;

    #[test]
    fn test_syscall_try_from_valid() {
        assert_eq!(Syscall::try_from(0), Ok(Syscall::Fork));
        assert_eq!(Syscall::try_from(1), Ok(Syscall::Exec));
        assert_eq!(Syscall::try_from(53), Ok(Syscall::Exit));
        assert_eq!(Syscall::try_from(57), Ok(Syscall::Padconf));
    }

    #[test]
    fn test_syscall_try_from_invalid() {
        assert_eq!(Syscall::try_from(11), Err(())); // unused gap
        assert_eq!(Syscall::try_from(58), Err(())); // >= NR_SYS_CALLS
        assert_eq!(Syscall::try_from(255), Err(()));
    }

    // ── dispatch_schedule tests (F-45) ────────────────────────────────

    #[test]
    fn test_dispatch_schedule_rejects_non_sys_proc_caller() {
        // C: do_schedule.c:9 (implicit) — only the system process may
        // call SYS_SCHEDULE. caller_has_sys_proc_with_table returns false
        // for any non-SYS_PROC caller → EPERM.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let priv_table = crate::test_helpers::test_priv_table();
        let msg = Message::default();
        let result = dispatch_schedule(ProcNr(0), &mut proc_table, &msg, &priv_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_schedule_rejects_invalid_endpoint() {
        // C: do_schedule.c:14-15 — endpoint_to_nr fails → EINVAL.
        // The SYS_PROC gate sits in front of the endpoint lookup, so the
        // caller must hold SYS_PROC for this test to reach the arm it
        // names; without it the run would pin EPERM and never exercise
        // EINVAL (the two tests above already cover the EPERM arm).
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        priv_table
            .get_mut(USER_PRIV_ID)
            .unwrap()
            .flags
            .s_flags |= ProcessCapability::SYS_PROC;
        let mut msg = Message::default();
        msg.m_type = Syscall::Schedule as i32;
        // endpoint = NONE; no slot carries it → endpoint_to_nr fails.
        msg.m_u.m_lsys_krn_schedule.endpoint = minix_types::Endpoint::NONE.0;
        let result = dispatch_schedule(ProcNr(0), &mut proc_table, &msg, &priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_schedule_sys_proc_caller_passes_priv_check() {
        // FIX-25 regression: confirm that a SYS_PROC caller is no longer
        // rejected by the legacy caller_has_sys_proc() that built a fresh
        // empty PrivTable internally. With caller_has_sys_proc_with_table,
        // a caller whose priv_id is USER_PRIV_ID and whose priv has
        // SYS_PROC flag set passes the permission check and reaches the
        // endpoint validation (EINVAL on NONE endpoint).
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Schedule as i32;
        msg.m_u.m_lsys_krn_schedule.endpoint = minix_types::Endpoint::NONE.0;
        let result = dispatch_schedule(ProcNr(0), &mut proc_table, &msg, &priv_table);
        // Should pass SYS_PROC check → reach endpoint validation → EINVAL.
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_vmctl_vminhibit_local_arm_sets_both_flags() {
        // C do_vmctl.c:128-135 — the local arm parks via VMINHIBIT and
        // the unconditional FLUSH_TLB set follows both arms, so a target
        // parked either way resumes with a TLB refresh pending.
        use crate::proc::{MiscFlagsBits, RtsFlagsBits};
        let mut table = crate::test_helpers::test_proc_table();
        table
            .get_mut(ProcNr(0))
            .unwrap()
            .p_rts_flags
            .clear(RtsFlagsBits::SLOT_FREE);
        vminhibit_park_local(&mut table, ProcNr(0));
        mark_flush_tlb(&mut table, ProcNr(0));
        let p = table.get(ProcNr(0)).unwrap();
        assert!(p.p_rts_flags.is_set(RtsFlagsBits::VMINHIBIT));
        assert!(p.p_misc_flags.is_set(MiscFlagsBits::FLUSH_TLB));
    }

    #[test]
    fn test_vmctl_vminhibit_park_helper_does_not_touch_flush_tlb() {
        // C smp.c:180-182 — the park (local or via IPI handler) sets
        // VMINHIBIT only; the FLUSH_TLB flag is delivered by the
        // unconditional marker (do_vmctl.c:133-135). Pinning the split
        // keeps the remote path honest: the IPI arm cannot forget it,
        // because it never owned it.
        use crate::proc::{MiscFlagsBits, RtsFlagsBits};
        let mut table = crate::test_helpers::test_proc_table();
        table
            .get_mut(ProcNr(0))
            .unwrap()
            .p_rts_flags
            .clear(RtsFlagsBits::SLOT_FREE);
        vminhibit_park_local(&mut table, ProcNr(0));
        assert!(table.get(ProcNr(0)).unwrap().p_rts_flags.is_set(RtsFlagsBits::VMINHIBIT));
        assert!(!table.get(ProcNr(0)).unwrap().p_misc_flags.is_set(MiscFlagsBits::FLUSH_TLB));
        mark_flush_tlb(&mut table, ProcNr(0));
        assert!(table.get(ProcNr(0)).unwrap().p_misc_flags.is_set(MiscFlagsBits::FLUSH_TLB));
    }

    #[test]
    fn test_dispatch_schedule_niced_wire_bit_sets_mf_niced() {
        // E-SCHEDNICED: C do_schedule.c:27 — `niced = !!(...)` coerces the
        // SYS_SCHEDULE wire field; sched_proc Step 8 maps it onto MF_NICED
        // (system.c:695-698). The old dispatch hard-coded `niced = false`,
        // so the wire value was dropped and NICED could never be set from
        // SYS_SCHEDULE.
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::MiscFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        // Target: slot 1, occupied with a resolvable endpoint; no user-space
        // scheduler registered (None → any caller allowed, do_schedule.c:18-19).
        proc_table.get_mut(ProcNr(1)).unwrap().p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        proc_table.get_mut(ProcNr(1)).unwrap().p_endpoint = minix_types::Endpoint(201);

        let mut msg = Message::default();
        msg.m_type = Syscall::Schedule as i32;
        msg.m_u.m_lsys_krn_schedule.endpoint = 201;
        msg.m_u.m_lsys_krn_schedule.priority = -1; // keep current
        msg.m_u.m_lsys_krn_schedule.quantum = -1;  // keep current
        msg.m_u.m_lsys_krn_schedule.cpu = -1;      // keep current
        msg.m_u.m_lsys_krn_schedule.niced = 1;     // the field under test
        let result = dispatch_schedule(ProcNr(0), &mut proc_table, &msg, &priv_table);
        assert_eq!(result, KcallResult::Ok(0));

        assert!(
            proc_table.get(ProcNr(1)).unwrap().p_misc_flags.is_set(MiscFlagsBits::NICED),
            "wire niced=1 必须经 dispatch 到 sched_proc 落成 MF_NICED"
        );
    }

    #[test]
    fn test_syscall_enum_tracks_minix_types_kernel_call_family() {
        // E-MINTYPES-SYS: the dispatch enum keeps C's relative-to-KERNEL_CALL
        // discriminants (internal idiom); the wire authority is minix-types'
        // kernel_call family (com.h:208-269). Every member must land on its
        // C number — the pin fires on the first drift on either side.
        use minix_types::{
            KERNEL_CALL, SYS_ABORT, SYS_CLEAR, SYS_DIAGCTL, SYS_EXEC, SYS_EXIT, SYS_FORK,
            SYS_GETINFO, SYS_GETKSIG, SYS_GETMCONTEXT, SYS_IOPENABLE, SYS_IRQCTL, SYS_KILL,
            SYS_MEMSET, SYS_PRIVCTL, SYS_PADCONF, SYS_READBIOS, SYS_RUNCTL, SYS_SAFECOPYFROM,
            SYS_SAFECOPYTO, SYS_SAFEMEMSET, SYS_SCHEDULE, SYS_SDEVIO, SYS_SETALARM,
            SYS_SETGRANT, SYS_SETMCONTEXT, SYS_SETTIME, SYS_SIGRETURN, SYS_SIGSEND, SYS_SPROF,
            SYS_STATECTL, SYS_STIME, SYS_SCHEDCTL, SYS_TRACE, SYS_UMAP, SYS_UMAP_REMOTE,
            SYS_UPDATE, SYS_VDEVIO, SYS_VIRCOPY, SYS_VMCTL, SYS_VTIMER, SYS_VUMAP, SYS_VSAFECOPY,
            SYS_DEVIO, SYS_ENDKSIG, SYS_PHYSCOPY, SYS_TIMES,
        };
        let pairs: [(Syscall, i32); 46] = [
            (Syscall::Fork, SYS_FORK),
            (Syscall::Exec, SYS_EXEC),
            (Syscall::Clear, SYS_CLEAR),
            (Syscall::Schedule, SYS_SCHEDULE),
            (Syscall::Privctl, SYS_PRIVCTL),
            (Syscall::Trace, SYS_TRACE),
            (Syscall::Kill, SYS_KILL),
            (Syscall::Getksig, SYS_GETKSIG),
            (Syscall::Endksig, SYS_ENDKSIG),
            (Syscall::Sigsend, SYS_SIGSEND),
            (Syscall::Sigreturn, SYS_SIGRETURN),
            (Syscall::Memset, SYS_MEMSET),
            (Syscall::Umap, SYS_UMAP),
            (Syscall::Vircopy, SYS_VIRCOPY),
            (Syscall::Physcopy, SYS_PHYSCOPY),
            (Syscall::UmapRemote, SYS_UMAP_REMOTE),
            (Syscall::Vumap, SYS_VUMAP),
            (Syscall::Irqctl, SYS_IRQCTL),
            (Syscall::Devio, SYS_DEVIO),
            (Syscall::Sdevio, SYS_SDEVIO),
            (Syscall::Vdevio, SYS_VDEVIO),
            (Syscall::Setalarm, SYS_SETALARM),
            (Syscall::Times, SYS_TIMES),
            (Syscall::Getinfo, SYS_GETINFO),
            (Syscall::Abort, SYS_ABORT),
            (Syscall::Iopenable, SYS_IOPENABLE),
            (Syscall::SafecopyFrom, SYS_SAFECOPYFROM),
            (Syscall::SafecopyTo, SYS_SAFECOPYTO),
            (Syscall::Vsafecopy, SYS_VSAFECOPY),
            (Syscall::Setgrant, SYS_SETGRANT),
            (Syscall::Readbios, SYS_READBIOS),
            (Syscall::Sprof, SYS_SPROF),
            (Syscall::Stime, SYS_STIME),
            (Syscall::Settime, SYS_SETTIME),
            (Syscall::Vmctl, SYS_VMCTL),
            (Syscall::Diagctl, SYS_DIAGCTL),
            (Syscall::Vtimer, SYS_VTIMER),
            (Syscall::Runctl, SYS_RUNCTL),
            (Syscall::Getmcontext, SYS_GETMCONTEXT),
            (Syscall::Setmcontext, SYS_SETMCONTEXT),
            (Syscall::Update, SYS_UPDATE),
            (Syscall::Exit, SYS_EXIT),
            (Syscall::Schedctl, SYS_SCHEDCTL),
            (Syscall::Statectl, SYS_STATECTL),
            (Syscall::Safememset, SYS_SAFEMEMSET),
            (Syscall::Padconf, SYS_PADCONF),
        ];
        for (member, wire) in pairs {
            assert_eq!(
                KERNEL_CALL + member as i32,
                wire,
                "枚举成员 {member:?} 与共享权威 {wire:#x} 不符"
            );
        }
    }

    // ── dispatch_privctl tests (FIX-25, Phase 5) ──────────────────────

    #[test]
    fn test_dispatch_privctl_rejects_non_sys_proc_caller() {
        // C: do_privctl.c:47 — caller must be SYS_PROC.
        // A fresh KProcess has no priv_id → caller_has_sys_proc returns false.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let msg = Message::default();
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_privctl_unknown_request_returns_einval() {
        // C: do_privctl.c:270-273 — unknown request → EINVAL.
        // We need a SYS_PROC caller to pass the first check.
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        // Make the caller a SYS_PROC by assigning a priv with SYS_PROC flag.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        // Insert target into proc_table — must clear SLOT_FREE so endpoint_to_nr finds it
        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 99; // unknown request
        msg.m_u.m_m1.m1i2 = 101; // target endpoint
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_privctl_disallow_sets_no_priv() {
        // C: do_privctl.c:75-79 — SYS_PRIV_DISALLOW sets RTS_NO_PRIV.
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.clear(RtsFlagsBits::NO_PRIV); // ensure not set
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 2; // SYS_PRIV_DISALLOW
        msg.m_u.m_m1.m1i2 = 101; // target endpoint
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(0));
        // Verify RTS_NO_PRIV was set
        let target = proc_table.get(target_nr).unwrap();
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::NO_PRIV));
    }

    #[test]
    fn test_dispatch_privctl_disallow_already_set_returns_eperm() {
        // C: do_privctl.c:77 — if RTS_NO_PRIV already set → EPERM.
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.set(RtsFlagsBits::NO_PRIV); // already set
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 2; // SYS_PRIV_DISALLOW
        msg.m_u.m_m1.m1i2 = 101;
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_privctl_query_mem_returns_eperm_no_ranges() {
        // C: do_privctl.c:232-251 — no s_mem_tab entries → EPERM.
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.priv_id = Some(USER_PRIV_ID);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 8; // SYS_PRIV_QUERY_MEM
        msg.m_u.m_m1.m1i2 = 101;
        msg.m_u.m_m1.m1p2 = 0x1000; // phys_start
        msg.m_u.m_m1.m1p3 = 0x100;  // phys_len
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        // No mem ranges in USER_PRIV_ID → EPERM
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_privctl_set_sys_without_no_priv_returns_eperm() {
        // C: do_privctl.c:88 — SET_SYS requires RTS_NO_PRIV on target.
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            // RTS_NO_PRIV not set → EPERM
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 3; // SYS_PRIV_SET_SYS
        msg.m_u.m_m1.m1i2 = 101;
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_privctl_set_sys_fills_guarded_default_mask() {
        // C: do_privctl.c:138-143 — SET_SYS defaults: map = DSRV_M (ALL_M)
        // expanded to every priv id, then fill_sendto_mask
        // (system.c:349-358). The fill applies the association/self
        // guards, so only *bound* slots get bits (never the target's own
        // slot, never an unbound one), and every send-capable target gets
        // the reciprocal bit.
        use crate::capability::{ProcessCapability, TrapMask};
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        // A send-capable system process the new service should reach, and
        // whose mask should gain the reciprocal bit.
        let driver = priv_table.assign_static(ProcNr(2)).unwrap();
        proc_table.get_mut(ProcNr(2)).unwrap().p_endpoint = minix_types::Endpoint(102);
        priv_table.get_mut(driver).unwrap().ipc.s_trap_mask = TrapMask::ALL;

        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.set(RtsFlagsBits::NO_PRIV); // SET_SYS precondition
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 3; // SYS_PRIV_SET_SYS
        msg.m_u.m_m1.m1i2 = 101;
        // arg_ptr = 0 → defaults only, no cross-space copy needed.
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(0));

        // The target got a dynamically allocated priv slot.
        let target_priv = proc_table.get(target_nr).unwrap()
            .priv_id.expect("SET_SYS must allocate a priv slot");
        assert_ne!(target_priv, USER_PRIV_ID);
        let mask = priv_table.get(target_priv).unwrap().ipc.s_ipc_to;
        // Bound and not self → granted (system.c:313-317 inverse).
        assert!(mask.may_send_to(USER_PRIV_ID as u8), "bound caller slot must be granted");
        assert!(mask.may_send_to(driver as u8), "bound driver slot must be granted");
        // Self and unassociated slots → not granted (C guards).
        assert!(!mask.may_send_to(target_priv as u8), "self bit must stay clear");
        let unbound = (crate::proc_table::NR_TASKS + 3) as u8; // ProcNr(3) never bound
        assert!(!mask.may_send_to(unbound),
            "unbound slot must not be pre-authorized by the ALL_M default");
        // Reciprocal: the driver's trap mask is ALL → it can reply.
        assert!(priv_table.get(driver).unwrap().ipc.s_ipc_to.may_send_to(target_priv as u8),
            "send-capable target must receive the reciprocal bit");
        // Reciprocal: the caller slot has no traps → no reply right.
        assert!(!priv_table.get(USER_PRIV_ID).unwrap().ipc.s_ipc_to.may_send_to(target_priv as u8),
            "RECEIVE-only caller slot must not receive the reciprocal bit");
        // Non-mask SET_SYS defaults are unchanged by the fill rework.
        assert_eq!(priv_table.get(target_priv).unwrap().ipc.s_trap_mask, TrapMask::ALL);
    }

    #[test]
    fn test_dispatch_privctl_add_io_without_priv_id_returns_eperm() {
        // C: do_privctl.c:188 — ADD_IO requires target has no RTS_NO_PRIV.
        // Target without priv_id → EPERM (no privilege structure).
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            // priv_id not set → EPERM
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 5; // SYS_PRIV_ADD_IO
        msg.m_u.m_m1.m1i2 = 101;
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_privctl_update_sys_without_arg_ptr_returns_einval() {
        // C: do_privctl.c:255 — UPDATE_SYS requires non-null arg_ptr.
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.priv_id = Some(USER_PRIV_ID);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 9; // SYS_PRIV_UPDATE_SYS
        msg.m_u.m_m1.m1i2 = 101;
        // m1p1 (arg_ptr) = 0 → EINVAL
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_privctl_clear_ipc_refs_returns_ok() {
        // C: do_privctl.c:81-84 — CLEAR_IPC_REFS clears pending IPC.
        use crate::capability::ProcessCapability;
        use crate::kpriv::USER_PRIV_ID;
        use crate::proc::RtsFlagsBits;

        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // K20 caller-by-nr: the caller identity lives on its table slot;
        // the dispatch re-borrows `proc_table.get(caller_nr)` at use point.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.priv_id = Some(USER_PRIV_ID);
        if let Some(p) = priv_table.get_mut(USER_PRIV_ID) {
            p.flags.s_flags |= ProcessCapability::SYS_PROC;
            p.identity.s_proc_nr = Some(ProcNr(0));
        }
        let target_nr = ProcNr(1);
        if let Some(p) = proc_table.get_mut(target_nr) {
            p.p_endpoint = minix_types::Endpoint(101);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.priv_id = Some(USER_PRIV_ID);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Privctl as i32;
        msg.m_u.m_m1.m1i1 = 11; // SYS_PRIV_CLEAR_IPC_REFS
        msg.m_u.m_m1.m1i2 = 101;
        let result = dispatch_privctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table, &mut crate::clock::ClockState::new());
        assert_eq!(result, KcallResult::Ok(0));
    }

    // ── dispatch_getmcontext / dispatch_setmcontext tests (F-43/F-44) ──

    #[test]
    fn test_dispatch_getmcontext_rejects_invalid_endpoint() {
        // C: do_mcontext.c:26-27 — isokendpt fails → EINVAL.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut msg = Message::default();
        msg.m_type = Syscall::Getmcontext as i32;
        msg.m_u.m_lsys_krn_sys_mcontext.endpt = 9999; // not in proc table
        msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000; // ctx_ptr (ignored in validation)
        let result = dispatch_getmcontext(ProcNr(0), &mut proc_table, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_getmcontext_rejects_kernel_target() {
        // C: do_mcontext.c:28 — iskerneln(proc_nr) → EPERM.
        use crate::proc::proc_nr::KERNEL;
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        if let Some(p) = proc_table.get_mut(KERNEL) {
            p.p_endpoint = minix_types::Endpoint(50);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Getmcontext as i32;
        msg.m_u.m_lsys_krn_sys_mcontext.endpt = 50;
        msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        let result = dispatch_getmcontext(ProcNr(0), &mut proc_table, &msg);
        assert_eq!(result, KcallResult::Ok(EPERM));
    }

    #[test]
    fn test_dispatch_getmcontext_user_target_suspends() {
        // 64-bit: no FPU fast path; copies mcontext from user → page fault
        // on unmapped ctx_ptr → VmSuspend.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = minix_types::Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Getmcontext as i32;
        msg.m_u.m_lsys_krn_sys_mcontext.endpt = 100;
        msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        let result = dispatch_getmcontext(ProcNr(0), &mut proc_table, &msg);
        assert_eq!(result, KcallResult::VmSuspend);
    }

    #[test]
    fn test_dispatch_setmcontext_rejects_invalid_endpoint() {
        // C: do_mcontext.c:64 — isokendpt fails → EINVAL.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut msg = Message::default();
        msg.m_type = Syscall::Setmcontext as i32;
        msg.m_u.m_lsys_krn_sys_mcontext.endpt = 9999;
        msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        let result = dispatch_setmcontext(ProcNr(0), &mut proc_table, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_dispatch_setmcontext_allows_kernel_target() {
        // C: do_mcontext.c:62-63 — setmcontext does NOT check iskerneln.
        // Kernel processes can have their FPU state restored (used during
        // context switch). We use a kernel slot and expect the call to
        // pass endpoint validation (no EPERM). The copy then page-faults
        // on the kernel target's unmapped ctx_ptr → VmSuspend.
        use crate::proc::proc_nr::KERNEL;
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        if let Some(p) = proc_table.get_mut(KERNEL) {
            p.p_endpoint = minix_types::Endpoint(50);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        // K20 caller-by-nr: the caller occupies its own slot (slot 0 is
        // the KERNEL target here).
        if let Some(c) = proc_table.get_mut(ProcNr(1)) {
            c.p_endpoint = minix_types::Endpoint(100);
            c.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Setmcontext as i32;
        msg.m_u.m_lsys_krn_sys_mcontext.endpt = 50;
        msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        let result = dispatch_setmcontext(ProcNr(1), &mut proc_table, &msg);
        // No EPERM (kernel target allowed); copy suspends on unmapped page.
        assert_eq!(result, KcallResult::VmSuspend);
    }

    #[test]
    fn test_dispatch_setmcontext_user_target_suspends() {
        // 64-bit: no FPU fast path; copies mcontext from user → page fault
        // on unmapped ctx_ptr → VmSuspend.
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        if let Some(p) = proc_table.get_mut(ProcNr(0)) {
            p.p_endpoint = minix_types::Endpoint(100);
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Setmcontext as i32;
        msg.m_u.m_lsys_krn_sys_mcontext.endpt = 100;
        msg.m_u.m_lsys_krn_sys_mcontext.ctx_ptr = 0x1000;
        let result = dispatch_setmcontext(ProcNr(0), &mut proc_table, &msg);
        assert_eq!(result, KcallResult::VmSuspend);
    }

    #[test]
    fn test_syscall_values_match_c() {
        // Verify key values match C's com.h definitions
        assert_eq!(Syscall::Fork as u16, 0);       // KERNEL_CALL + 0
        assert_eq!(Syscall::Schedule as u16, 3);    // KERNEL_CALL + 3
        assert_eq!(Syscall::Memset as u16, 13);     // KERNEL_CALL + 13
        assert_eq!(Syscall::Vmctl as u16, 43);      // KERNEL_CALL + 43
        assert_eq!(Syscall::Exit as u16, 53);       // KERNEL_CALL + 53
        assert_eq!(Syscall::Padconf as u16, 57);    // KERNEL_CALL + 57
    }

    #[test]
    fn test_kernel_call_dispatch_bad_call() {
        // Create a minimal message with invalid syscall number
        let mut msg = Message::default();
        msg.m_type = 99; // Invalid syscall number
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut clock_state = crate::clock::ClockState::new();
        let result = kernel_call_dispatch(ProcNr(0), &mut proc_table, &mut msg, &mut priv_table, &mut clock_state);
        assert_eq!(result, KcallResult::BadCall);
        // kernel_call_dispatch acquires BKL but does NOT release it —
        // the caller is expected to call kernel_call_finish() which
        // releases BKL. In this unit test we only test dispatch, so
        // we must release BKL manually to avoid poisoning other tests.
        crate::smp::bkl_unlock();
    }

    #[test]
    fn test_kernel_call_dispatch_call_denied_no_priv() {
        // Process without priv_id should be denied
        let mut msg = Message::default();
        // proc.priv_id is None by default
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut clock_state = crate::clock::ClockState::new();
        let result = kernel_call_dispatch(ProcNr(0), &mut proc_table, &mut msg, &mut priv_table, &mut clock_state);
        assert_eq!(result, KcallResult::CallDenied);
        // Same as above: release BKL acquired by kernel_call_dispatch.
        crate::smp::bkl_unlock();
    }

    #[test]
    fn test_kernel_call_dispatch_sets_kbill_marker() {
        // D-9 (C system.c:160): the kbill_kcall marker is set after
        // dispatch, before finish — unconditionally, even for a denied
        // call (the kernel work of handling the denial is still the
        // caller's).
        let mut msg = Message::default();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut clock_state = crate::clock::ClockState::new();
        let result = kernel_call_dispatch(ProcNr(0), &mut proc_table, &mut msg, &mut priv_table, &mut clock_state);
        assert_eq!(result, KcallResult::CallDenied);
        // Marker in flight for the caller despite the denial.
        // SAFETY: single-threaded test; BKL still held (released below).
        assert_eq!(unsafe { crate::kbill_kcall_raw() }, Some(ProcNr(0)));
        // Release the BKL acquired by dispatch (finish would normally).
        crate::smp::bkl_unlock();
        // Cleanup: consume with delta 0 — clears the marker without
        // attribution so later tests start neutral.
        let section = crate::smp::bkl_lock_section();
        let _ = crate::consume_kbill_kcall(&mut proc_table, 0, &section);
        crate::smp::bkl_unlock();
    }

    /// D-8: kernel_call wrapper — TOCTOU defense + SIGSEGV on bad copy.
    /// The wrapper copies the user message via UserCopy::copy_msg_from_user
    /// (TOCTOU defense: kernel works on its own copy), then dispatches.
    #[test]
    fn test_kernel_call_wrapper() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut clock_state = crate::clock::ClockState::new();
        let mut proc = KProcess::new(ProcNr(0), minix_types::Endpoint::KERNEL);
        proc.priv_id = Some(0);
        // Use an invalid syscall → BadCall deterministically. The wrapper
        // mechanics (p_delivermsg_vir save + dispatch + finish) are what
        // we're testing, not the dispatch semantics.
        // K20 caller-by-nr: the caller is slot ProcNr(0).
        proc_table.get_mut(ProcNr(0)).unwrap().p_endpoint = minix_types::Endpoint(100);
        proc_table.get_mut(ProcNr(0)).unwrap().priv_id = Some(0);
        let result = kernel_call(
            ProcNr(0),
            &mut proc_table,
            minix_types::VirBytes(0x1000),
            &mut priv_table,
            &mut clock_state,
            &crate::ipc::KernelUserCopy,
        );
        assert_eq!(result, KcallResult::CallDenied);
        // p_delivermsg_vir saved (C system.c:141)
        assert_eq!(
            proc_table.get(ProcNr(0)).unwrap().p_delivermsg_vir,
            minix_types::VirBytes(0x1000)
        );
        // kernel_call_finish already released BKL (CallDenied → non-VmSuspend → unlock).

        // Cleanup: D-9 marker
        let section = crate::smp::bkl_lock_section();
        let _ = crate::consume_kbill_kcall(&mut proc_table, 0, &section);
        crate::smp::bkl_unlock();
    }

    #[test]
    fn test_kcall_result_variants() {
        // Verify all KcallResult variants can be constructed
        let _ok = KcallResult::Ok(0);
        let _suspend = KcallResult::VmSuspend;
        let _no_reply = KcallResult::NoReply;
        let _bad = KcallResult::BadCall;
        let _denied = KcallResult::CallDenied;
    }

    // ── dispatch_irqctl wrapper tests ─────────────────────────────────
    //
    // The `dispatch_irqctl` wrapper in this module is now a thin delegation
    // to `syscall_device::dispatch_irqctl` (acquires the global IrqManager
    // under BKL and forwards). Its correctness is obvious from inspection;
    // the validation and hook-operation logic is tested directly in
    // `syscall_device::tests` with a local `IrqManager<MockController>`.

    // ── dispatch_ipc_entry tests (Phase 1A) ───────────────────────────
    //
    // dispatch_ipc_entry is the IPC trap entry point, called directly by
    // arch trap handlers (x86-64 IDT vector 33, ARM64/riscv64 software
    // dispatch) — bypassing kernel_call_dispatch_inner. See 13-syscall-
    // dispatch §4.8 and 12-ipc-core §4.8.

    #[test]
    fn test_dispatch_ipc_entry_bad_call_nr_returns_ebadcall() {
        // Invalid IPC call numbers (0, 17, 255) must return EBADCALL(209)
        // without entering dispatch_ipc (and thus without acquiring BKL).
        // C: proc.c:602-606 — do_ipc default branch returns EBADCALL.
        // (6 = MINIX_KERNINFO is a valid call now — see the kerninfo tests
        // below; 7..15 remain unassigned gaps in ipcconst.h.)
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();

        for &bad_nr in &[0i32, 7, 8, 9, 10, 11, 12, 13, 14, 15, 17, 100, 255] {
            let mut msg = Message::default();
            msg.m_type = bad_nr;
            let result = dispatch_ipc_entry(
                ProcNr(0),
                &mut proc_table,
                &mut msg,
                &mut priv_table,
            );
            assert_eq!(
                result,
                KcallResult::Ok(crate::errno::EBADCALL),
                "call_nr={} should return EBADCALL",
                bad_nr
            );
        }
    }

    #[test]
    fn test_dispatch_ipc_entry_kerninfo_unpublished_returns_ebadcall() {
        // MINIX_KERNINFO (6) before the kernel info page is published to
        // user space must return EBADCALL. C: proc.c:687-689 — "It might
        // not be initialized yet": `minix_kerninfo_user == 0` → EBADCALL.
        // The Rust sentinel (KERNINFO_USER_UNSET = 0) pairs with the same
        // observable behavior.
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut msg = Message::default();
        msg.m_type = 6; // MINIX_KERNINFO — ipcconst.h:12

        // Guard against cross-test leakage of the published address.
        crate::globals::MINIX_KERNINFO_USER
            .store(crate::globals::KERNINFO_USER_UNSET, Ordering::Relaxed);

        let result = dispatch_ipc_entry(
            ProcNr(0),
            &mut proc_table,
            &mut msg,
            &mut priv_table,
        );
        assert_eq!(result, KcallResult::Ok(crate::errno::EBADCALL));

        // dispatch_ipc_entry transfers BKL ownership out (see the SEND
        // test above) — release it to keep later tests unpoisoned.
        crate::smp::bkl_unlock();
    }

    #[test]
    fn test_dispatch_ipc_entry_kerninfo_published_returns_ok() {
        // With the page published, MINIX_KERNINFO returns OK and the
        // page address goes out through the secondary IPC return channel.
        // C: proc.c:690-692 — `arch_set_secondary_ipc_return(caller_ptr,
        // minix_kerninfo_user); return OK;`. The register write itself
        // (x86-64: saved R10 状态车道按调用兼任, whole-value assignment;
        // NK4-C 1.54/B26) is pinned at the arch layer —
        // `set_secondary_ipc_return_assigns_status_lane` in
        // arch/src/x86_64/boot.rs; this test pins the dispatch decision
        // and the OK outcome (no message is read or written either way).
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut msg = Message::default();
        msg.m_type = 6; // MINIX_KERNINFO — ipcconst.h:12

        const PUBLISHED_PAGE: u64 = 0x0000_7000_2000;
        crate::globals::MINIX_KERNINFO_USER.store(PUBLISHED_PAGE, Ordering::Relaxed);

        let result = dispatch_ipc_entry(
            ProcNr(0),
            &mut proc_table,
            &mut msg,
            &mut priv_table,
        );
        assert_eq!(result, KcallResult::Ok(crate::errno::OK));

        // Restore the unpublished sentinel so later tests see boot state.
        crate::globals::MINIX_KERNINFO_USER
            .store(crate::globals::KERNINFO_USER_UNSET, Ordering::Relaxed);
        crate::smp::bkl_unlock();
    }

    #[test]
    fn test_dispatch_ipc_entry_routes_send_to_ipc_engine() {
        // SEND (call_nr=1) must enter dispatch_ipc, which calls
        // check_ipc_permission. A caller without priv_id fails the IPC
        // target whitelist check (s_ipc_to) → ECALLDENIED(210).
        // C: proc.c:536-544 — may_send_to / s_ipc_to check; no priv → denied.
        let mut msg = Message::default();
        msg.m_type = 1; // IpcCall::Send
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        // K20 caller-by-nr: the caller identity lives on its table slot.
        // p_defer.r2 = dst endpoint (required by the SEND path); use a
        // valid-looking endpoint — the permission check fails before
        // endpoint validity is examined.
        let caller_slot = proc_table.get_mut(ProcNr(0)).unwrap();
        caller_slot.p_endpoint = minix_types::Endpoint(100);
        caller_slot.p_defer.r2 = 200; // dst endpoint

        let result = dispatch_ipc_entry(
            ProcNr(0),
            &mut proc_table,
            &mut msg,
            &mut priv_table,
        );
        // Expect ECALLDENIED(210) because caller has no priv_id, so the
        // s_ipc_to whitelist check fails first (before trap-mask check).
        assert_eq!(result, KcallResult::Ok(crate::errno::ECALLDENIED));

        // dispatch_ipc_entry acquires BKL via bkl_guard.transfer() —
        // BKL is NOT released by Drop. We must release manually to avoid
        // poisoning subsequent tests.
        crate::smp::bkl_unlock();
    }

    #[test]
    fn test_dispatch_ipc_entry_acquires_bkl() {
        // dispatch_ipc_entry must acquire BKL before calling dispatch_ipc.
        // We verify this indirectly: after dispatch_ipc_entry returns
        // (via the SEND path), BKL is held (not released by Drop due to
        // transfer()). We release it manually with bkl_unlock().
        //
        // If BKL were not acquired, bkl_unlock() here would underflow
        // (unlock without lock) and panic.
        let mut caller = KProcess::new(ProcNr(0), minix_types::Endpoint(100));
        caller.p_defer.r2 = 200;
        let mut msg = Message::default();
        msg.m_type = 4; // IpcCall::Notify (no dst blocking, simpler path)
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();

        let _ = dispatch_ipc_entry(
            ProcNr(0),
            &mut proc_table,
            &mut msg,
            &mut priv_table,
        );
        // BKL should be held now (acquired by dispatch_ipc_entry,
        // not released due to transfer()). Release it to restore state.
        crate::smp::bkl_unlock();
    }

    /// End-to-end (D-13): `dispatch_ipc` → `IpcEngine::receive` delivers
    /// a message from a `MF_SIG_DELAY` sender → `dispatch_ipc` runs
    /// `sig_delay_done` for it. C: proc.c:1082-1083 → system.c:454-464.
    ///
    /// This pins the glue between the slice-based IPC engine (which cannot
    /// run the scheduler-aware `cause_signal`) and the `ProcessTable`-level
    /// dispatcher that completes the PM stop-delay protocol.
    #[test]
    fn test_dispatch_ipc_receive_ends_sender_sig_delay() {
        use crate::proc::{MiscFlagsBits, RtsFlagsBits};
        use crate::capability::{IpcMask, TrapMask};
        use crate::proc_table::nr_to_idx;
        use crate::ipc::caller_q_push;

        // D-63②: dispatch_ipc's A1 chain root asserts the BKL — hold it
        // via RAII (drops unlocked at test end).
        let _bkl = {
            crate::smp::bkl_lock_reset_for_test();
            crate::smp::bkl_lock()
        };
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let receiver_nr = ProcNr(1);
        let sender_nr = ProcNr(0);
        let manager_nr = ProcNr(2);

        // Receiver: occupied + priv allowing RECEIVE (trap mask + ipc_to).
        {
            let p = proc_table.get_mut(receiver_nr).unwrap();
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.priv_id = Some(1);
        }
        priv_table.get_mut(1).unwrap().identity.s_proc_nr = Some(receiver_nr);
        priv_table.get_mut(1).unwrap().ipc.s_trap_mask = TrapMask::ALL;
        priv_table.get_mut(1).unwrap().ipc.s_ipc_to = IpcMask::from_bits(1u64 << 0);

        let receiver_ep = proc_table.get(receiver_nr).unwrap().p_endpoint;
        let sender_ep = proc_table.get(sender_nr).unwrap().p_endpoint;
        // Sender: occupied + SENDING (queued on receiver) + SIG_DELAY.
        {
            let p = proc_table.get_mut(sender_nr).unwrap();
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_rts_flags.set(RtsFlagsBits::SENDING);
            p.p_sendto_e = receiver_ep;
            p.p_sendmsg = Message::default();
            p.p_misc_flags.set(MiscFlagsBits::SIG_DELAY);
            p.priv_id = Some(0);
        }
        let manager_ep = proc_table.get(manager_nr).unwrap().p_endpoint;
        priv_table.get_mut(0).unwrap().identity.s_proc_nr = Some(sender_nr);
        priv_table.get_mut(0).unwrap().signals.s_sig_mgr = manager_ep;
        // Manager: occupied + listening (notification observable).
        {
            let p = proc_table.get_mut(manager_nr).unwrap();
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.priv_id = Some(2);
            p.p_rts_flags.set(RtsFlagsBits::RECEIVING);
            p.p_getfrom_e = minix_types::Endpoint::ANY;
        }
        priv_table.get_mut(2).unwrap().identity.s_proc_nr = Some(manager_nr);

        // Enqueue the sender on the receiver's caller queue.
        {
            let procs = proc_table.procs_slice_mut();
            caller_q_push(procs, nr_to_idx(receiver_nr).unwrap(), nr_to_idx(sender_nr).unwrap());
        }

        // Receiver performs RECEIVE from the sender.
        let mut msg = Message::default();
        msg.m_type = crate::ipc::IpcCall::Receive as i32;
        proc_table.get_mut(receiver_nr).unwrap().p_defer.r2 = sender_ep.0 as usize;

        let result = dispatch_ipc(
            &mut proc_table,
            nr_to_idx(receiver_nr).unwrap(),
            &msg,
            &mut priv_table,
            crate::ipc::IpcCall::Receive,
        );
        assert_eq!(result, KcallResult::Ok(crate::errno::OK));

        // Sender's delay ended: MF_SIG_DELAY cleared, RTS_SIGNALED set,
        // and the manager was woken (DELIVERMSG = direct notification).
        let sender = proc_table.get(sender_nr).unwrap();
        assert!(!sender.p_misc_flags.is_set(MiscFlagsBits::SIG_DELAY));
        assert!(sender.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert!(proc_table.get(manager_nr).unwrap().p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    // ── dispatch_diagctl STACKTRACE tests (P8-4) ──────────────────────

    #[test]
    fn test_dispatch_diagctl_stacktrace_invalid_endpoint() {
        // C: do_diagctl.c:44 — isokendpt fails → EINVAL.
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut msg = Message::default();
        msg.m_type = Syscall::Diagctl as i32;
        msg.m_u.m_lsys_krn_sys_diagctl.code = 2; // DIAGCTL_CODE_STACKTRACE
        msg.m_u.m_lsys_krn_sys_diagctl.endpt = minix_types::Endpoint::NONE.0;
        let result = dispatch_diagctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    /// D-14: DIAGCTL REGISTER/UNREGISTER lifecycle — a SYS_PROC caller
    /// sets `s_diag_sig` (C do_diagctl.c:51), a non-SYS_PROC caller gets
    /// EPERM (:50), and UNREGISTER clears it (:61). The SIGKMESS
    /// notification itself is a W-7 no-op (see the comment in the
    /// REGISTER arm): its condition inputs (kmess buffer) were removed
    /// by the EarlyConsole ARCH evolution.
    #[test]
    fn test_dispatch_diagctl_register_unregister_lifecycle() {
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let a_priv = priv_table.assign_static(ProcNr(0)).unwrap();
        priv_table.get_mut(a_priv).unwrap().flags.s_flags.insert(
            crate::capability::ProcessCapability::SYS_PROC,
        );
        // K20 caller-by-nr: the caller IS slot ProcNr(0) — its priv_id
        // must live on the slot (the standalone handle is gone).
        proc_table.get_mut(ProcNr(0)).unwrap().priv_id = Some(a_priv);

        // REGISTER: SYS_PROC → s_diag_sig set, OK.
        let mut msg = Message::default();
        msg.m_type = Syscall::Diagctl as i32;
        msg.m_u.m_lsys_krn_sys_diagctl.code = 3; // DIAGCTL_CODE_REGISTER
        let result = dispatch_diagctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(0));
        assert!(priv_table.get(a_priv).unwrap().mem.s_diag_sig);

        // UNREGISTER: clears the subscription.
        msg.m_u.m_lsys_krn_sys_diagctl.code = 4; // DIAGCTL_CODE_UNREGISTER
        let result = dispatch_diagctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(0));
        assert!(!priv_table.get(a_priv).unwrap().mem.s_diag_sig);
    }

    /// D-14: a caller without SYS_PROC privilege gets EPERM at REGISTER
    /// (C do_diagctl.c:50) and the flag stays clear.
    #[test]
    fn test_dispatch_diagctl_register_denied_without_sys_proc() {
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut proc_table = crate::test_helpers::test_proc_table();
        let a_priv = priv_table.assign_static(ProcNr(0)).unwrap();
        let mut caller = KProcess::new(ProcNr(0), minix_types::Endpoint(100));
        caller.priv_id = Some(a_priv);
        // No SYS_PROC capability inserted.

        let mut msg = Message::default();
        msg.m_type = Syscall::Diagctl as i32;
        msg.m_u.m_lsys_krn_sys_diagctl.code = 3;
        let result = dispatch_diagctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table);
        assert_eq!(result, KcallResult::Ok(EPERM));
        assert!(!priv_table.get(a_priv).unwrap().mem.s_diag_sig);
    }

    #[test]
    #[ignore = "requires real page table infrastructure; mock PteWalk returns None causing cross_space_copy to SIGSEGV on dst address arithmetic"]
    fn test_dispatch_diagctl_stacktrace_valid_endpoint_returns_ok() {
        // C: do_diagctl.c:46-47 — proc_stacktrace prints to console,
        // then returns OK. We verify the OK return; the console output
        // is a side effect (tested via integration on real hardware).
        let mut proc_table = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // Set a known endpoint on slot ProcNr(1) so it resolves.
        let target_endpt = minix_types::Endpoint(200);
        if let Some(target) = proc_table.get_mut(ProcNr(1)) {
            target.p_endpoint = target_endpt;
            // Clear SLOT_FREE so endpoint_to_nr can find it.
            target.p_rts_flags.unset(crate::proc::RtsFlagsBits::SLOT_FREE);
        }
        let mut msg = Message::default();
        msg.m_type = Syscall::Diagctl as i32;
        msg.m_u.m_lsys_krn_sys_diagctl.code = 2; // DIAGCTL_CODE_STACKTRACE
        msg.m_u.m_lsys_krn_sys_diagctl.endpt = target_endpt.0;
        let result = dispatch_diagctl(ProcNr(0), &mut proc_table, &msg, &mut priv_table);
        // The stack walk will fail immediately (frame pointer = 0, no
        // user-space mapping), but the function should still return OK
        // (matching C's unconditional `return OK` after proc_stacktrace).
        assert_eq!(result, KcallResult::Ok(OK));
    }

    #[test]
    fn test_padconf_unused_kernel_call_replies_ebadrequest() {
        // C ground truth: the SYS_PADCONF map entry is `#if defined(__arm__)`
        // only (system.c:251-253) and do_padconf.c exists only under
        // arch/earm/. On every non-arm C build the call_vec entry is NULL and
        // kernel_call answers `EBADREQUEST` (system.c:120-123). The Rust
        // kernel has no arm32 target, so the trait default's `BadCall` —
        // which `reply_code()` maps to EBADREQUEST (212) — IS the C-parity
        // reply. This test pins both links of that chain.
        let msg = Message::default();
        let mut proc_table = crate::test_helpers::test_proc_table();

        let result = <CurrentArchSyscall as ArchSyscall>::dispatch_padconf(
            ProcNr(0),
            &mut proc_table,
            &msg,
        );
        assert_eq!(result, KcallResult::BadCall,
            "padconf on a non-arm32 kernel is the C NULL-entry case");
        assert_eq!(result.reply_code(), Some(EBADREQUEST),
            "user-visible reply must be EBADREQUEST (212), matching system.c:123");
    }

    #[test]
    fn test_vmctl_clear_page_fault_requeues_target() {
        // C: do_vmctl.c:32-35 — RTS_UNSET(p, RTS_PAGEFAULT) is the
        // macro form: clear + enqueue when the process becomes runnable.
        // The process parked at fault time (rts_set → dequeued); VM's
        // ClearPageFault must return it to the run queue, or the E5(d)
        // loop never closes (the process is never scheduled again).
        use crate::proc::RtsFlagsBits;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let nr = ProcNr(0);
        {
            let p = proc_table.get_mut(nr).unwrap();
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.p_endpoint = minix_types::Endpoint(100);
        }
        // Fault-time park: PAGEFAULT set through the scheduler-aware
        // rts_set (the trap arm's transition).
        proc_table.rts_set(nr, RtsFlagsBits::PAGEFAULT);
        assert!(!proc_table.get(nr).unwrap().is_runnable(),
            "faulted process must be non-runnable before VM resolves");

        // Clear → runnable again, fault-address record dropped.
        assert_eq!(vmctl_clear_page_fault(&mut proc_table, nr), KcallResult::Ok(0));
        assert!(proc_table.get(nr).unwrap().is_runnable(),
            "RTS_UNSET must re-enqueue the resolved process (do_vmctl.c:35)");
        assert_eq!(proc_table.get(nr).unwrap().p_fault_addr, None);

        // Second clear without a pending fault → EINVAL (C assert parity).
        assert_eq!(vmctl_clear_page_fault(&mut proc_table, nr), KcallResult::Ok(EINVAL));

        // Free slot target → EINVAL as well (old None arm preserved).
        assert_eq!(
            vmctl_clear_page_fault(&mut proc_table, ProcNr(1)),
            KcallResult::Ok(EINVAL)
        );
    }

    /// NK4-C F10/F10b（P0-wire）判别测试：SYSCALL 腿线上交付约定。
    ///
    /// s13b 真机实证的吞错形态：privctl SET_SYS 失败返 EFAULT=14，正数
    /// 上线后被用户态 `reply < 0` 门控吞掉（RS 把失败当成功，boot 尾
    /// 10 服务器静默卡 NO_PRIV）。
    ///
    /// s13c 真机实证的破坏形态：F10 对所有码一视同仁取负，
    /// VMCTL_MEMREQ_GET 的数据码 ENOENT=2 取负后变为 -2，
    /// 触发 `reply < 0` 误入错误分支 → do_memory 不服务 → RS 永久 VMREQUEST。
    ///
    /// F10b 修复：`reply_wire()` 区分 Ok(错误码)→取负 和 Data(数据码)→原样传递。
    #[test]
    fn test_syscall_leg_wire_negates_nonzero_errno() {
        // 内部携正 errno（C handler `return(EPERM)` 同形）……
        assert_eq!(KcallResult::Ok(EFAULT).reply_code(), Some(EFAULT));
        // ……线上必须负：14 → -14，用户态 `reply < 0` 才能拦截。
        assert_eq!(syscall_leg_wire(EFAULT), -EFAULT);
        assert_eq!(syscall_leg_wire(EPERM), -EPERM);
        // 成功/数据零码不变号。
        assert_eq!(syscall_leg_wire(0), 0);
        // 簿记类回执（EBADREQUEST/ECALLDENIED）同样取负。
        assert_eq!(syscall_leg_wire(EBADREQUEST), -(EBADREQUEST as i32));
        assert!(syscall_leg_wire(ECALLDENIED) < 0);
        // F10b：`reply_wire()` 错误码取负……
        assert_eq!(KcallResult::Ok(EFAULT).reply_wire(), Some(-EFAULT));
        assert_eq!(KcallResult::Ok(EPERM).reply_wire(), Some(-EPERM));
        // ……数据码原样传递（C 语义：`reply == ENOENT` / `reply == VMPTYPE_CHECK`）。
        assert_eq!(KcallResult::Data(ENOENT).reply_wire(), Some(ENOENT));
        assert_eq!(KcallResult::Data(1).reply_wire(), Some(1)); // VMPTYPE_CHECK
        // 数据码 OK(0) 等价于成功，reply_wire() = Some(0)
        assert_eq!(KcallResult::Data(0).reply_wire(), Some(0));
    }
}
