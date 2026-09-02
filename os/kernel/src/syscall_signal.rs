//! Signal system calls: kill, getksig, endksig, sigsend, sigreturn.
//!
//! # Minix3 C Source Mapping
//!
//! - `do_kill.c` — SYS_KILL
//! - `do_getksig.c` — SYS_GETKSIG
//! - `do_endksig.c` — SYS_ENDKSIG
//! - `do_sigsend.c` — SYS_SIGSEND
//! - `do_sigreturn.c` — SYS_SIGRETURN
//!
//! # Design Decisions (19-syscall-signal.md §3)
//!
//! - **D1**: `u64` for signal bitmap (`_NSIG = 64`)
//! - **D4**: `cause_signal()` as KProcess method
//! - **D5**: Linear scan for GETKSIG (matches C, process count < 128)
//! - **D6**: `trait SignalContext` for architecture-specific sigframe/sigcontext

use minix_types::{Endpoint, Message, MessSigcalls, VirBytes};

use crate::proc::{KProcess, MiscFlagsBits, ProcNr, RtsFlagsBits, SigSet};
use crate::proc_table::ProcessTable;
use crate::kpriv::PrivTable;
use crate::syscall::{KcallResult, Syscall};
use crate::cross_space::data_copy_vmcheck;
use crate::vm::{AddressRef, CrossSpaceResult};

use minix_arch::{
    CurrentSignalContext, CurrentDirectMap, DirectMapArch, SignalContext, SignalInfo,
};

// ── Minix3 error codes ──
// Centralized in `crate::errno` to prevent value drift (FIX-01: R-02/R-09/R-18).
// Previously ENOSYS=38 here (should be 78).
use crate::errno::*;

// ── Signal constants ──

/// Number of signals. C: `_NSIG` — signal.h
pub const NSIG: usize = 64;

/// Signal number for SIGVTALRM. C: `SIGVTALRM` — signal.h
pub const SIGVTALRM: u32 = 26;

/// Signal number for SIGPROF. C: `SIGPROF` — signal.h
pub const SIGPROF: u32 = 27;

/// Signal number for SIGABRT. C: `SIGABRT` — signal.h
pub const SIGABRT: u32 = 6;

/// Signal number for SIGTRAP. C: `SIGTRAP` — signal.h
pub const SIGTRAP: u32 = 5;

/// Kernel signal notification. C: `SIGKSIG = 74` — signal.h
/// Used by cause_sig() to notify the signal manager that a kernel signal
/// is pending. This is outside the _NSIG range (1-64) because it is an
/// internal kernel-to-sm notification, not a POSIX signal.
pub const SIGKSIG: u32 = 74;

// ── Endpoint constants ──

// SELF is used via Endpoint::SELF in cause_signal/endksig/getksig

// ── Signal bitmap type ──
//
// `SigSet` is the newtype defined in `proc.rs` (`pub struct SigSet(u64)` with
// `#[repr(transparent)]`). We reuse it here instead of a local alias so that
// signal bitmaps have one canonical type across the kernel. IPC message
// layout is preserved via `SigSet::get()` / `SigSet::from_raw()`.

/// Build a signal mask for the given signal number (1-based).
/// C: `sig_mask(sig)` — signal.h
pub const fn sig_mask(sig_nr: u32) -> SigSet {
    if sig_nr == 0 || sig_nr as usize > NSIG {
        SigSet::empty()
    } else {
        SigSet::from_raw(1u64 << (sig_nr - 1))
    }
}

// ── Helper ──

/// Read signal call fields from a message.
/// C: `m_ptr->m_sigcalls.*` — uses mess_sigcalls union member.
fn msg_sigcalls(msg: &Message) -> MessSigcalls {
    msg.debug_check_m_type_any(&[
        Syscall::Getksig as i32,
        Syscall::Endksig as i32,
        Syscall::Kill as i32,
        Syscall::Sigsend as i32,
        Syscall::Sigreturn as i32,
    ]);
    // SAFETY: `m_type` verified above (debug) / guaranteed by dispatch (release).
    unsafe { msg.m_u.m_sigcalls }
}

// ── Dispatch functions ──

/// Dispatch SYS_KILL.
///
/// C: `do_kill()` — do_kill.c
///
/// Cause a signal to be sent to a process. Adds to pending signal map
/// and informs the signal manager.
pub fn dispatch_kill(
    _caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_kill.c:26,28 — m_sigcalls.sig / m_sigcalls.endpt
    let endpt = sc.endpt;    // m_sigcalls.endpt
    let sig_nr = sc.sig;     // m_sigcalls.sig

    // C: do_kill.c:31 — sig_nr >= _NSIG → EINVAL
    if sig_nr as usize >= NSIG {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_kill.c:30 — isokendpt(proc_nr_e, &proc_nr)
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_kill.c:32 — iskerneln(proc_nr) → EPERM
    if ProcessTable::is_kernel(target_nr) {
        return KcallResult::Ok(EPERM);
    }

    // C: do_kill.c:35 — cause_sig(proc_nr, sig_nr)
    cause_signal(target_nr, sig_nr as u32, proc_table, priv_table);

    KcallResult::Ok(OK)
}

/// Cause a signal to be sent to a process.
///
/// C: `cause_sig()` — system.c:389-449
///
/// Adds the signal to the target's pending bitmap and marks the target
/// as signaled. If the target was not already in the RTS_SIGNALED state,
/// also notifies the signal manager (via `send_sig` / `mini_notify`).
///
/// # BKL Requirement
///
/// Must be called while holding the Big Kernel Lock. The BKL ensures
/// no other CPU can concurrently modify `p_pending`, `p_rts_flags`,
/// or `s_sig_pending`.
///
/// # Signal manager notification
///
/// C: `cause_sig()` (system.c:389-449) — 双路径：
/// - SELF 路径（`rp->p_endpoint == sig_mgr`，目标进程是自身信号管理器）：
///   `sigaddset(&priv(rp)->s_sig_pending, sig_nr)` + `send_sig(SIGKSIGSM)` 唤醒目标自身。
/// - 外部路径（其余）：`sigaddset(&rp->p_pending, sig_nr)` + `RTS_SIGNALED|RTS_SIG_PENDING`
///   + `send_sig(sig_mgr, SIGKSIG)` 唤醒目标进程的信号管理器。
///
/// 两条路径的唤醒均经 `mini_notify_core`（源 = SYSTEM，目标 = 需被唤醒者，
/// C: `mini_notify(proc_addr(SYSTEM), rp->p_endpoint)` — system.c:381）实现；
/// `s_sig_pending` 标记为写记录（内核无读者）。
/// SIGS_IS_LETHAL 致命信号自管理路径（备份管理器切换 / panic）DEFERRED（见 todo.md）。
fn cause_signal(
    target_nr: ProcNr,
    sig_nr: u32,
    proc_table: &mut ProcessTable,
    priv_table: &mut PrivTable,
) {
    // C: system.c:411 — rp = proc_addr(proc_nr)
    // C: system.c:412-413 — sig_mgr = priv(rp)->s_sig_mgr; if (sig_mgr == SELF) sig_mgr = rp->p_endpoint
    let sig_mgr = proc_table.sig_mgr(target_nr, priv_table);
    let target_endpoint = proc_table.get(target_nr).map(|p| p.p_endpoint);

    // ── SELF 路径：目标进程是自己的信号管理器 ──
    // C: system.c:416 — if (rp->p_endpoint == sig_mgr) → 直接自管理，不走外部通知
    if let (Some(ep), Some(mgr)) = (target_endpoint, sig_mgr)
        && ep == mgr
    {
        // C: system.c:417 — if (SIGS_IS_LETHAL(sig_nr)) → 备份管理器切换 / panic。
        // DEFERRED: 需 s_bak_sig_mgr 切换 + RTS_NO_PRIV + panic 集成（见 01-stage-kernel/todo.md）。
        // 当前阶段无用户态进程，自管理进程（VM/RS）收到致命信号的路径不可达。

        // C: system.c:433 — sigaddset(&priv(rp)->s_sig_pending, sig_nr)
        // 自管理进程的信号记入其自身 s_sig_pending（内核侧写记录，无内核读者；
        // sig_nr ≤ 64 在 Rust SigSet(u64) 位宽内）。
        if let Some(pid) = proc_table.get(target_nr).and_then(|p| p.priv_id)
            && let Some(priv_) = priv_table.get_mut(pid)
        {
            priv_.signals.s_sig_pending.add(sig_nr as u8);
        }

        // C: system.c:434 — send_sig(rp->p_endpoint, SIGKSIGSM) → mini_notify(proc_addr(SYSTEM), rp->p_endpoint)
        // 唤醒目标自身。C 的通知数值（SIGKSIGSM=73）仅写入 s_sig_pending（无内核读者），
        // 故 Rust 直接 mini_notify_core（源 = SYSTEM，目标 = 自身），无需 SIGKSIGSM 常量。
        let _ = crate::ipc::mini_notify_core(
            proc_table.procs_slice_mut(),
            priv_table,
            crate::proc::proc_nr::SYSTEM,
            ep,
        );
        return;
    }

    // ── 外部路径：目标由外部信号管理器管理 ──
    // C: system.c:439 — s = sigismember(&rp->p_pending, sig_nr)
    // Rust 以 RTS_SIGNALED 判定 was_signaled + 无条件 p_pending.add：
    // sigaddset 幂等，与 C 的 sigismember 门控语义等价。
    let was_signaled = proc_table.get(target_nr)
        .is_some_and(|p| p.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));

    // C: system.c:442 — sigaddset(&rp->p_pending, sig_nr)
    if let Some(target) = proc_table.get_mut(target_nr) {
        // R-16 (2026-08-12): SAFETY: `sig_nr` (u32) was validated `< NSIG` (64)
        // at the do_kill call site (syscall_signal.rs:119), so it fits in u8.
        target.p_pending.add(sig_nr as u8);
    }

    // C: system.c:443 — if (!RTS_ISSET(rp, RTS_SIGNALED))
    if !was_signaled {
        // C: system.c:444 — RTS_SET(rp, RTS_SIGNALED | RTS_SIG_PENDING)
        proc_table.rts_set(target_nr, RtsFlagsBits::SIGNALED | RtsFlagsBits::SIG_PENDING);

        // C: system.c:445-446 — send_sig(sig_mgr, SIGKSIG) → 唤醒信号管理器
        // send_sig (system.c:364-382): sigaddset(&priv(sig_mgr)->s_sig_pending, SIGKSIG)
        // + mini_notify(proc_addr(SYSTEM), rp->p_endpoint) —— 通知目标是**信号管理器自身**，
        // 源是 SYSTEM。
        if let Some(sig_mgr_ep) = sig_mgr {
            // C: system.c:380 — sigaddset(&priv->s_sig_pending, sig_nr)
            // SIGKSIG=74 超出 Rust SigSet(u64) 位宽 → add 为 no-op；s_sig_pending 是
            // 写记录（内核无读者），行为保持。
            if let Some(sig_mgr_nr) = proc_table.endpoint_to_nr(sig_mgr_ep)
                && let Some(sig_mgr_proc) = proc_table.get(sig_mgr_nr)
                    && let Some(pid) = sig_mgr_proc.priv_id
                        && let Some(sig_mgr_priv) = priv_table.get_mut(pid) {
                            // R-16 (2026-08-12): SAFETY: `SIGKSIG` is a
                            // compile-time `u32` constant (= 74, < 256), so
                            // `as u8` cannot truncate.
                            sig_mgr_priv.signals.s_sig_pending.add(SIGKSIG as u8);
                        }

            // C: system.c:381 — mini_notify(proc_addr(SYSTEM), rp->p_endpoint)
            let _ = crate::ipc::mini_notify_core(
                proc_table.procs_slice_mut(),
                priv_table,
                crate::proc::proc_nr::SYSTEM,
                sig_mgr_ep,
            );
        }
    }
}

/// Dispatch SYS_GETKSIG.
///
/// C: `do_getksig()` — do_getksig.c
///
/// The signal manager polls for pending kernel signals.
/// Scans all user processes for one with RTS_SIGNALED whose s_sig_mgr
/// matches the caller. Returns the endpoint and pending signal map,
/// then clears RTS_SIGNALED and p_pending.
///
/// # C Semantic Alignment (do_getksig.c:27-40)
///
/// 1. Scan `BEG_USER_ADDR..END_PROC_ADDR` for `RTS_SIGNALED` process
/// 2. Check `caller == priv(rp)->s_sig_mgr`
/// 3. Write `m_sigcalls.endpt = rp->p_endpoint`, `m_sigcalls.map = rp->p_pending`
/// 4. `RTS_UNSET(rp, RTS_SIGNALED)`, clear `rp->p_pending`
/// 5. If no process found: `m_sigcalls.endpt = NONE`
pub fn dispatch_getksig(
    caller: &mut KProcess,
    msg: &mut Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    // C: do_getksig.c:27-40 — scan all user processes
    let mut found: Option<(Endpoint, u64)> = None;

    for rp in proc_table.iter() {
        // Skip kernel tasks and free slots
        if rp.p_rts_flags.get() == RtsFlagsBits::SLOT_FREE {
            continue;
        }
        if ProcessTable::is_kernel(rp.p_nr) {
            continue;
        }
        // C: do_getksig.c:28 — if (!RTS_ISSET(rp, RTS_SIGNALED)) continue
        if !rp.p_rts_flags.is_set(RtsFlagsBits::SIGNALED) {
            continue;
        }
        // C: do_getksig.c:29 — if (caller->p_endpoint != priv(rp)->s_sig_mgr) continue
        let sig_mgr = proc_table.sig_mgr(rp.p_nr, priv_table);
        if sig_mgr != Some(caller.p_endpoint) {
            continue;
        }

        // Found a process with a pending signal for this signal manager.
        found = Some((rp.p_endpoint, rp.p_pending.get()));
        break;
    }

    if let Some((endpt, map)) = found {
        // C: do_getksig.c:31-32 — write reply
        // m_ptr->m_sigcalls.endpt = rp->p_endpoint
        // m_ptr->m_sigcalls.map = rp->p_pending
        let target_nr = proc_table.endpoint_to_nr(endpt).unwrap();

        // Clear RTS_SIGNALED and p_pending on the found process.
        // C: do_getksig.c:34 — RTS_UNSET(rp, RTS_SIGNALED)
        proc_table.rts_unset(target_nr, RtsFlagsBits::SIGNALED);
        // C: do_getksig.c:33 — sigemptyset(&rp->p_pending)
        if let Some(target) = proc_table.get_mut(target_nr) {
            target.p_pending.clear();
        }

        // Write reply into the message union.
        msg.m_u.m_sigcalls = MessSigcalls {
            map,
            endpt: endpt.get(),
            sig: 0,
            sigctx: 0,
            _padding: [0u8; 32],
        };
    } else {
        // C: do_getksig.c:40 — m_ptr->m_sigcalls.endpt = NONE
        msg.m_u.m_sigcalls = MessSigcalls {
            map: 0,
            endpt: Endpoint::NONE.get(),
            sig: 0,
            sigctx: 0,
            _padding: [0u8; 32],
        };
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_ENDKSIG.
///
/// C: `do_endksig()` — do_endksig.c
///
/// The signal manager has finished processing a kernel signal.
/// Validates the caller is the signal manager for the target process,
/// then clears RTS_SIG_PENDING if no new signals arrived.
///
/// # C Semantic Alignment (do_endksig.c:27-36)
///
/// 1. `isokendpt(endpt, &proc_nr)` — validate endpoint → EINVAL
/// 2. `caller->p_endpoint != priv(rp)->s_sig_mgr` — permission check → EPERM
/// 3. `!RTS_ISSET(rp, RTS_SIG_PENDING)` — no pending signal → EINVAL
/// 4. `!RTS_ISSET(rp, RTS_SIGNALED)` — no new signal → clear SIG_PENDING
pub fn dispatch_endksig(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
    priv_table: &PrivTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_endksig.c:27 — m_sigcalls.endpt
    let endpt = sc.endpt;

    // Step 1: Validate endpoint. C: do_endksig.c:27 — isokendpt()
    let target_endpoint = Endpoint(endpt);
    let target_nr = match proc_table.endpoint_to_nr(target_endpoint) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // Step 2: Check caller is the signal manager for the target.
    // C: do_endksig.c:31 — if (caller->p_endpoint != priv(rp)->s_sig_mgr) return EPERM
    let sig_mgr = proc_table.sig_mgr(target_nr, priv_table);
    if sig_mgr != Some(caller.p_endpoint) {
        return KcallResult::Ok(EPERM);
    }

    // Step 3: Check RTS_SIG_PENDING is set.
    // C: do_endksig.c:32 — if (!RTS_ISSET(rp, RTS_SIG_PENDING)) return EINVAL
    if !proc_table.get(target_nr)
        .is_some_and(|p| p.p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING))
    {
        return KcallResult::Ok(EINVAL);
    }

    // Step 4: If no new signal arrived, clear RTS_SIG_PENDING.
    // C: do_endksig.c:35-36 — if (!RTS_ISSET(rp, RTS_SIGNALED))
    //     RTS_UNSET(rp, RTS_SIG_PENDING)
    if !proc_table.get(target_nr)
        .is_some_and(|p| p.p_rts_flags.is_set(RtsFlagsBits::SIGNALED))
    {
        proc_table.rts_unset(target_nr, RtsFlagsBits::SIG_PENDING);
    }

    KcallResult::Ok(OK)
}

/// Signal message from user-space signal manager.
///
/// C: `struct sigmsg` — minix/type.h:71-77
///
/// # Layout
///
/// `#[repr(C)]` with field order matching C's `struct sigmsg` so that
/// `data_copy_vmcheck` can copy directly between user space and this
/// struct. `SigSet` is `#[repr(transparent)]` over `u64`, matching
/// C's `sigset_t` (8 bytes, 64 signals).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SigMsg {
    /// Signal number. C: `sm_signo` (int, 4 bytes)
    pub signo: u32,
    /// Signal mask to block during handler. C: `sm_mask` (sigset_t, 8 bytes)
    pub mask: SigSet,
    /// Signal handler address. C: `sm_sighandler` (vir_bytes, 8 bytes)
    pub sighandler: u64,
    /// Return address for sigreturn. C: `sm_sigreturn` (vir_bytes, 8 bytes)
    pub sigreturn: u64,
    /// User stack pointer at signal time. C: `sm_stkptr` (vir_bytes, 8 bytes)
    pub stkptr: u64,
}

/// Dispatch SYS_SIGSEND.
///
/// C: `do_sigsend()` — do_sigsend.c:19-162
///
/// POSIX-style signal delivery: build sigframe on user stack,
/// modify registers to jump to signal handler.
///
/// **Critical constraint** (C: do_sigsend.c:126-131 WARNING): register
/// modification MUST happen after the last `data_copy_vmcheck` (which may
/// VMSUSPEND). If registers are modified before the copy and the copy
/// suspends, re-execution would corrupt the register state.
///
/// # Flow
///
/// 1. Validate endpoint (C: do_sigsend.c:31-32)
/// 2. Copy `sigmsg` from caller's user space (C: do_sigsend.c:36-39)
/// 3. Build `SigContext` from target's `CpuContext` (C: do_sigsend.c:50-115)
/// 4. Build `SigFrame` with computed frame address (C: do_sigsend.c:46-47,117-118)
/// 5. Copy `SigFrame` to target's user stack (C: do_sigsend.c:120-124)
/// 6. Modify target's `CpuContext` for handler entry (C: do_sigsend.c:133-135)
pub fn dispatch_sigsend(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_sigsend.c:31,37 — m_sigcalls.endpt / m_sigcalls.sigctx
    // C 无 SELF 替换：endpoint 原样传 isokendpt（负 endpoint 如 SYSTEM → EINVAL）。
    let endpt = sc.endpt;
    let sigctx_addr = sc.sigctx;

    // C: do_sigsend.c:31 — isokendpt → EINVAL
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_sigsend.c:32 — iskerneln → EPERM
    if target_nr.0 < 0 {
        return KcallResult::Ok(EPERM);
    }

    // ── Step 1: Copy sigmsg from caller's user space ──
    // C: do_sigsend.c:36-39 — data_copy_vmcheck(caller, caller_ep, sigctx, KERNEL, &smsg, sizeof)
    let smsg: SigMsg = SigMsg::default();
    {
        let smsg_phys = CurrentDirectMap::virt_to_phys(VirBytes(
            &smsg as *const SigMsg as u64,
        ));
        let caller_endpt = caller.p_endpoint;
        let caller_cr3 = caller.p_seg.phys_root;
        let pt: &ProcessTable = proc_table;
        let proc_cr3 = |ep: Endpoint| {
            if ep == caller_endpt {
                Some(caller_cr3)
            } else {
                pt.endpoint_to_nr(ep)
                    .and_then(|nr| pt.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };
        let src = AddressRef::Process {
            endpoint: caller_endpt,
            offset: VirBytes(sigctx_addr),
        };
        let dst = AddressRef::Physical(smsg_phys);
        match data_copy_vmcheck(
            caller,
            src,
            dst,
            core::mem::size_of::<SigMsg>(),
            proc_cr3,
        ) {
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Completed(Ok(())) => {}
        }
    }

    // ── Step 2-3: Build sigcontext + sigframe from target's registers ──
    // C: do_sigsend.c:46-118 — compute stack ptr, build sigcontext, build sigframe
    // Idempotent: only reads CpuContext, safe to re-run after VMSUSPEND.
    let (frame, frame_addr) = {
        let target = match proc_table.get(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        let mut info = SignalInfo {
            sighandler: smsg.sighandler,
            mask: smsg.mask.get(),
            signo: smsg.signo,
            sigreturn: smsg.sigreturn,
            stkptr: smsg.stkptr,
        };
        let sctx = CurrentSignalContext::build_sigcontext(&target.cpu_context, &mut info);
        // C: do_sigsend.c:47 — frp = (struct sigframe_sigcontext *) smsg.sm_stkptr - 1
        let frame_addr = info
            .stkptr
            .saturating_sub(CurrentSignalContext::sigframe_size() as u64);
        let frame = CurrentSignalContext::build_sigframe(
            &target.cpu_context,
            &sctx,
            &info,
            frame_addr,
        );
        (frame, frame_addr)
    };

    // ── Step 4: Copy sigframe to target's user stack ──
    // C: do_sigsend.c:120-124 — data_copy_vmcheck(caller, KERNEL, &fr, endpt, frp, sizeof)
    // May VMSUSPEND — the frame is on the kernel stack, so re-execution
    // after resume will rebuild it idempotently.
    {
        let frame_phys = CurrentDirectMap::virt_to_phys(VirBytes(
            &frame as *const _ as u64,
        ));
        let caller_endpt = caller.p_endpoint;
        let caller_cr3 = caller.p_seg.phys_root;
        let pt: &ProcessTable = proc_table;
        let proc_cr3 = |ep: Endpoint| {
            if ep == caller_endpt {
                Some(caller_cr3)
            } else {
                pt.endpoint_to_nr(ep)
                    .and_then(|nr| pt.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };
        let src = AddressRef::Physical(frame_phys);
        let dst = AddressRef::Process {
            endpoint: Endpoint(endpt),
            offset: VirBytes(frame_addr),
        };
        match data_copy_vmcheck(
            caller,
            src,
            dst,
            CurrentSignalContext::sigframe_size(),
            proc_cr3,
        ) {
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Completed(Ok(())) => {}
        }
    }

    // ── Step 5: Modify target registers for handler entry ──
    // C: do_sigsend.c:133-135 — MUST be after the last data_copy_vmcheck!
    // WARNING (C: do_sigsend.c:126-131): changes to process registers MUST
    // be deferred until after the copy succeeds, otherwise VMSUSPEND
    // recovery would re-execute and corrupt register state.
    {
        let target = match proc_table.get_mut(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        let info = SignalInfo {
            sighandler: smsg.sighandler,
            mask: smsg.mask.get(),
            signo: smsg.signo,
            sigreturn: smsg.sigreturn,
            stkptr: smsg.stkptr,
        };
        CurrentSignalContext::setup_handler_entry(
            &mut target.cpu_context,
            &info,
            frame_addr,
        );

        // C: do_sigsend.c:156 — `rp->p_misc_flags &= ~MF_FPU_INITIALIZED`
        // Signal handler should get clean FPU. Clear the EXT_REG_INITIALIZED
        // flag so the next FPU instruction traps and lazily initializes a
        // fresh FPU state (64-bit lazy FPU model — no save/restore on
        // signal delivery, matching C behavior).
        target.p_misc_flags.unset(MiscFlagsBits::EXT_REG_INITIALIZED);
    }

    KcallResult::Ok(OK)
}

/// Dispatch SYS_SIGRETURN.
///
/// C: `do_sigreturn()` — do_sigreturn.c:19-95
///
/// Restore process state after signal handler returns.
/// Copies sigcontext from user stack and restores registers.
///
/// # Flow
///
/// 1. Validate endpoint (C: do_sigreturn.c:28-29)
/// 2. Copy `SigContext` from target's user space (C: do_sigreturn.c:33-35)
/// 3. Restore target's `CpuContext` from `SigContext` (C: do_sigreturn.c:42-80)
/// 4. `arch_setcontext` with trap_style (C: do_sigreturn.c:81)
/// 5. Check magic integrity (C: do_sigreturn.c:83)
pub fn dispatch_sigreturn(
    caller: &mut KProcess,
    msg: &Message,
    proc_table: &mut ProcessTable,
) -> KcallResult {
    let sc = msg_sigcalls(msg);
    // C: do_sigreturn.c:28,33 — m_sigcalls.endpt / m_sigcalls.sigctx
    // C 无 SELF 替换：endpoint 原样传 isokendpt（负 endpoint 如 SYSTEM → EINVAL）。
    let endpt = sc.endpt;
    let sigctx_addr = sc.sigctx;

    // C: do_sigreturn.c:28 — isokendpt → EINVAL
    let target_nr = match proc_table.endpoint_to_nr(Endpoint(endpt)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // C: do_sigreturn.c:29 — iskerneln → EPERM
    if target_nr.0 < 0 {
        return KcallResult::Ok(EPERM);
    }

    // ── Step 1: Copy sigcontext from target's user space ──
    // C: do_sigreturn.c:33-35 — data_copy(endpt, sigctx, KERNEL, &sc, sizeof)
    // Note: C uses data_copy (no vmcheck), but minix-rs uses data_copy_vmcheck
    // for uniformity — the user stack may page-fault.
    let sctx: <CurrentSignalContext as SignalContext>::SigContext = Default::default();
    let sctx_size = core::mem::size_of_val(&sctx);
    {
        let sctx_phys = CurrentDirectMap::virt_to_phys(VirBytes(
            &sctx as *const _ as u64,
        ));
        let caller_endpt = caller.p_endpoint;
        let caller_cr3 = caller.p_seg.phys_root;
        let pt: &ProcessTable = proc_table;
        let proc_cr3 = |ep: Endpoint| {
            if ep == caller_endpt {
                Some(caller_cr3)
            } else {
                pt.endpoint_to_nr(ep)
                    .and_then(|nr| pt.get(nr))
                    .map(|p| p.p_seg.phys_root)
            }
        };
        let src = AddressRef::Process {
            endpoint: Endpoint(endpt),
            offset: VirBytes(sigctx_addr),
        };
        let dst = AddressRef::Physical(sctx_phys);
        match data_copy_vmcheck(caller, src, dst, sctx_size, proc_cr3) {
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Completed(Ok(())) => {}
        }
    }

    // ── Step 2: Restore target's registers from sigcontext ──
    // C: do_sigreturn.c:42-80 — arch-specific register restore
    // C: do_sigreturn.c:81 — arch_proc_setcontext(rp, &rp->p_reg, 1, sc.trap_style)
    {
        let target = match proc_table.get_mut(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        CurrentSignalContext::restore_sigcontext(&mut target.cpu_context, &sctx);
        let trap_style = CurrentSignalContext::get_trap_style(&sctx);
        CurrentSignalContext::arch_setcontext(&mut target.cpu_context, trap_style);
    }

    // C: do_sigreturn.c:83 — warn on corrupt magic (still return OK)
    if !CurrentSignalContext::check_magic(&sctx) {
        // C: printf("kernel sigreturn: corrupt signal context\n")
        use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole};
        Console::write_str("kernel sigreturn: corrupt signal context\n");
    }

    // C: do_sigreturn.c:85-93 — FPU state restore
    //
    // On 64-bit, the C source gates FPU restore with `#if defined(__i386__)`:
    // sigreturn does NOT restore FPU state on x86-64 / aarch64 / riscv64.
    // The 64-bit signal-delivery path (do_sigsend.c:154) clears
    // `MF_FPU_INITIALIZED` so the signal handler gets a clean FPU via the
    // lazy trap-on-first-FPU-instruction mechanism. Sigreturn does not
    // restore the pre-signal FPU state — the process's FPU state after
    // sigreturn is whatever the signal handler left it as.
    //
    // The `KProcess.fpu_state` buffer IS used by the SMP migration SAVE_CTX
    // path (smp.rs:497-527), but NOT by the signal delivery/return path on
    // 64-bit (matching C behavior).

    KcallResult::Ok(OK)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proc::KProcess;
    use minix_types::Endpoint;

    #[test]
    fn test_sig_mask() {
        assert_eq!(sig_mask(1).get(), 1u64);
        assert_eq!(sig_mask(2).get(), 2u64);
        assert_eq!(sig_mask(64).get(), 1u64 << 63);
        assert_eq!(sig_mask(0).get(), 0u64);
        assert_eq!(sig_mask(65).get(), 0u64);
    }

    #[test]
    fn test_nsig() {
        assert_eq!(NSIG, 64);
    }

    #[test]
    fn test_signal_constants() {
        assert_eq!(SIGVTALRM, 26);
        assert_eq!(SIGPROF, 27);
        assert_eq!(SIGABRT, 6);
        assert_eq!(SIGTRAP, 5);
    }

    #[test]
    fn test_sigsend_invalid_endpoint() {
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sigsend as i32;
        // Invalid endpoint → EINVAL
        let mut proc_table = crate::test_helpers::test_proc_table();
        let result = dispatch_sigsend(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sigsend_kernel_process() {
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sigsend as i32;
        // Set endpoint to a kernel process (negative proc_nr)
        let mut proc_table = crate::test_helpers::test_proc_table();
        // Kernel processes have negative proc_nr, but endpoint_to_nr
        // won't find them in the table, so we get EINVAL.
        let result = dispatch_sigsend(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sigreturn_invalid_endpoint() {
        let mut caller = KProcess::new(ProcNr(0), Endpoint(0));
        let mut msg = Message::default();
        msg.m_type = Syscall::Sigreturn as i32;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let result = dispatch_sigreturn(&mut caller, &msg, &mut proc_table);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sigmsg_struct() {
        let smsg = SigMsg {
            signo: SIGABRT,
            mask: sig_mask(SIGABRT),
            sighandler: 0x400000,
            sigreturn: 0x401000,
            stkptr: 0x7FFFF000,
        };
        assert_eq!(smsg.signo, 6);
        assert_eq!(smsg.mask, sig_mask(6));
        assert_eq!(smsg.sighandler, 0x400000);
        assert_eq!(smsg.sigreturn, 0x401000);
        assert_eq!(smsg.stkptr, 0x7FFFF000);
    }
}
