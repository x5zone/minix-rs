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

use crate::proc::{MiscFlagsBits, ProcNr, RtsFlagsBits, SigSet};
use crate::proc_table::ProcessTable;
use crate::kpriv::PrivTable;
use crate::syscall::{KcallResult, Syscall};
use crate::cross_space::data_copy_vmcheck;
use crate::vm::{AddressRef, CrossSpaceResult};

use minix_arch::{
    CurrentSignalContext, CurrentDirectMap, DirectMapArch, SignalContext, SignalInfo, TrapStyle,
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

/// Signal number for SIGILL. C: `SIGILL` — signal.h:55
pub const SIGILL: u32 = 4;

/// Signal number for SIGTRAP. C: `SIGTRAP` — signal.h
pub const SIGTRAP: u32 = 5;

/// Signal number for SIGABRT. C: `SIGABRT` — signal.h
pub const SIGABRT: u32 = 6;

/// Signal number for SIGEMT. C: `SIGEMT` — signal.h:59
pub const SIGEMT: u32 = 7;

/// Signal number for SIGFPE. C: `SIGFPE` — signal.h:60
pub const SIGFPE: u32 = 8;

/// Signal number for SIGBUS. C: `SIGBUS` — signal.h:62
pub const SIGBUS: u32 = 10;

/// Signal number for SIGSEGV. C: `SIGSEGV` — signal.h:63
pub const SIGSEGV: u32 = 11;

/// Kernel signal notification. C: `SIGKSIG = 74` — signal.h
/// Used by cause_sig() to notify the signal manager that a kernel signal
/// is pending. This is outside the _NSIG range (1-64) because it is an
/// internal kernel-to-sm notification, not a POSIX signal.
pub const SIGKSIG: u32 = 74;

/// End of delay for signal delivery. C: `SIGSNDELAY = 70` — signal.h:264.
///
/// Sent to a process whose stop was requested with `RC_DELAY` while it was
/// still sending a message (or in a deferred syscall): once the process
/// reaches a quiescent point (no longer sending), the kernel notifies the
/// signal manager that the delayed stop can now proceed. Consumed by PM
/// (`pm/signal.c:344-369`, `DELAY_CALL` handling) to resume the stop.
///
/// This is a "system signal" (outside the 1-64 POSIX range, like
/// [`SIGKSIG`]). `cause_signal` delivers it through the external path:
/// `RTS_SIGNALED | RTS_SIG_PENDING` + `mini_notify` to the manager, which
/// drives the manager to `SYS_GETKSIG`.
///
/// # Known limitation (`SigSet(u64)`)
///
/// C's `sigset_t` is 128 bits (`__uint32_t __bits[4]`, sigtypes.h:60-62),
/// so bit 70 lands in `p_pending` and rides the GETKSIG map to the
/// manager. The Rust `SigSet(u64)` (`_NSIG = 64`) cannot encode it:
/// `p_pending.add(70)` is a no-op, so the signal *number* is not visible
/// to the manager — only the wakeup is (same shared limitation as
/// [`SIGKSIG`]/`SIGKSIGSM`). Widening `SigSet` to 128 bits is a
/// cross-cutting wire change (GET_PROCTAB/GET_PRIVTAB/notify/GETKSIG
/// message fields + PM mirror), tracked separately.
pub const SIGSNDELAY: u32 = 70;

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

/// Check whether a signal is "lethal" for a self-managing system process.
///
/// C: `SIGS_IS_LETHAL(sig)` — signal.h:280-282. A self-managing process
/// (its own signal manager) that receives one of these cannot handle the
/// signal itself: the kernel must either fail over to the designated
/// backup manager or panic (see [`cause_signal`]).
pub(crate) const fn is_lethal(sig_nr: u32) -> bool {
    matches!(sig_nr, SIGILL | SIGBUS | SIGFPE | SIGSEGV | SIGEMT | SIGABRT)
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
    _caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    msg: &Message,
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
/// C: `cause_sig()` (system.c:389-449) — 双路径 + 致命子路径：
/// - SELF 路径（`rp->p_endpoint == sig_mgr`，目标进程是自身信号管理器）：
///   致命信号（`SIGS_IS_LETHAL`，见 [`is_lethal`]）先尝试提升 `s_bak_sig_mgr`
///   为 primary 后递归走外部路径；无 backup 则 panic（system.c:417-432）。
///   非致命信号：`sigaddset(&priv(rp)->s_sig_pending, sig_nr)` +
///   `send_sig(SIGKSIGSM)` 唤醒目标自身（system.c:433-436）。
/// - 外部路径（其余）：`sigaddset(&rp->p_pending, sig_nr)` + `RTS_SIGNALED|RTS_SIG_PENDING`
///   + `send_sig(sig_mgr, SIGKSIG)` 唤醒目标进程的信号管理器。
///
/// 两条路径的唤醒均经 `mini_notify_core`（源 = SYSTEM，目标 = 需被唤醒者，
/// C: `mini_notify(proc_addr(SYSTEM), rp->p_endpoint)` — system.c:381）实现。
/// `s_sig_pending` 在本模块只写不读：其消费方在 ipc 通知投递路径
///（SYSTEM 源通知送达时编码进 `m_notify.sigset` 并清空，ipc.rs:1776-1782）；
/// 超过 64 的内核信号（SIGKSIG=74 等）因 `SigSet(u64)` 位宽限制 add 为 no-op，
/// 唤醒动作本身即 C 语义中该信号的完整内核行为。
pub(crate) fn cause_signal(
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
        // 自管理进程收到致命信号（SIGILL/ABRT/EMT/FPE/BUS/SEGV）意味着它无法再
        // 自我管理——它是"没有人能替我处理信号"的角色。要么把预设的 backup
        // 管理器提升为 primary 后重投（走外部路径，让 backup 收割），要么 panic。
        if is_lethal(sig_nr) {
            // C: system.c:418-419 — sig_mgr = priv(rp)->s_bak_sig_mgr
            let pid = proc_table.get(target_nr).and_then(|p| p.priv_id);
            let backup = pid
                .and_then(|pid| priv_table.get(pid))
                .map(|priv_| priv_.signals.s_bak_sig_mgr);

            // C: system.c:420 — if (sig_mgr != NONE && isokendpt(sig_mgr, &sig_mgr_proc_nr))
            // endpoint_to_nr 仅在 slot 非 SLOT_FREE 时命中（proc_table.rs:435-437），
            // 等价 C 的 isokendpt + isemptyn。
            if let Some(bak_ep) = backup
                && bak_ep != Endpoint::NONE
                && let Some(bak_nr) = proc_table.endpoint_to_nr(bak_ep)
            {
                // C: system.c:421-422 — priv(rp)->s_sig_mgr = sig_mgr;
                //                        priv(rp)->s_bak_sig_mgr = NONE;
                if let Some(pid) = pid
                    && let Some(priv_) = priv_table.get_mut(pid)
                {
                    priv_.signals.s_sig_mgr = bak_ep;
                    priv_.signals.s_bak_sig_mgr = Endpoint::NONE;
                }

                // C: system.c:424 — RTS_UNSET(sig_mgr_rp, RTS_NO_PRIV)
                // backup 进程可能因 RTS_NO_PRIV 被挂起（提升前不参与调度），
                // 接管前必须解除，它才能作为管理器被唤醒。
                proc_table.rts_unset(bak_nr, RtsFlagsBits::NO_PRIV);

                // C: system.c:425-426 — cause_sig(proc_nr, sig_nr); return
                // 递归重投：target 的 s_sig_mgr 现为 backup（≠ target），走外部路径
                //（p_pending + RTS_SIGNALED + 通知 backup）。递归深度恒为 1。
                cause_signal(target_nr, sig_nr, proc_table, priv_table);
                return;
            }

            // C: system.c:429 — proc_stacktrace(rp)
            // 自管理进程是 KERNEL/SYSTEM 角色（目前仅 SYSTEM = proc_nr(-2) 可见，
            // 它的栈在内核 Direct Map；本路径在用户态进程接入前不可达——见
            // 19-design.v2 §D-3b）。栈回溯的"诊断路径必须比崩溃更健壮"原则保证
            // proc_stacktrace 不会因读取失败二次崩溃，最多打印占位符后停止。
            // SAFETY-equivalent: proc_stacktrace 在 BKL 持有期内调用，读路径
            // （Direct Map alias 对内核栈；cross_space_copy 对用户栈）受 BKL
            // 保护的可见性约束。
            //
            // Production-only: `proc_stacktrace` invokes the kernel
            // EarlyConsole, which on x86_64 writes to the COM1 UART
            // via `inb/outb` instructions. Unit tests run as a hosted
            // Linux process and would SIGSEGV on those instructions
            // (no `iopl`/`ioperm`). The diagnostic is therefore
            // production-only — tests verify the panic itself, not
            // the diagnostic output (test mode shares the panic
            // branch's existence and the routing logic). The
            // diagnostic is exercised end-to-end via the
            // `#[ignore]`-d `proc_stacktrace_empty_chain_does_not_panic`
            // test on real hardware.
            #[cfg(not(test))]
            if let Some(rp) = proc_table.get(target_nr) {
                crate::stacktrace::proc_stacktrace(rp);
            }

            // C: system.c:430-431 — panic(...)
            panic!(
                "cause_sig: sig manager {} gets lethal signal {} for itself",
                ep.0, sig_nr
            );
        }

        // C: system.c:433 — sigaddset(&priv(rp)->s_sig_pending, sig_nr)
        // 自管理进程的信号记入其自身 s_sig_pending（内核侧写记录，无内核读者；
        // sig_nr ≤ 64 在 Rust SigSet(u64) 位宽内）。
        if let Some(pid) = proc_table.get(target_nr).and_then(|p| p.priv_id)
            && let Some(priv_) = priv_table.get_mut(pid)
        {
            priv_.signals.s_sig_pending.add(sig_nr as u8);
        }

        // C: system.c:434 — send_sig(rp->p_endpoint, SIGKSIGSM) → mini_notify(proc_addr(SYSTEM), rp->p_endpoint)
        // SIGKSIGSM=73 的内核动作仅是"唤醒自管理进程去处理自身待决状态"。
        // Rust SigSet(u64) 无法编码 >64 的信号（add 为 no-op），而唤醒本身
        // 由 mini_notify 完成，故直接 mini_notify_core（源 = SYSTEM，目标 = 自身），
        // 无需 SIGKSIGSM 常量。
        let _ = crate::ipc::mini_notify_core(
            proc_table.procs_slice_mut(),
            priv_table,
            crate::proc::proc_nr::SYSTEM,
            ep,
        );
        // C: `mini_notify`'s `RTS_UNSET(..., RTS_RECEIVING)` carries the
        // enqueue half; `mini_notify_core` is the primitive slice clear, so
        // supply it here (NK4-C B6 family — a woken-but-not-enqueued process
        // is `runnable=yes queued=no` and never picked).
        if let Some(nr) = proc_table.endpoint_to_nr(ep) {
            proc_table.enqueue_if_woken(nr);
        }
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
        // NK4-C 1.9 取证探针（task1-close 裁决删除）：cause_signal 现场——
        // 谁被挂 SIGNALED|SIG_PENDING、什么信号。s14b/c 尾态 pm+9 服务器全
        // 停 0x30 无人交付，本探针裁决信号源。
        #[cfg(not(feature = "mock"))]
        {
            use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrd};
            static CSIG_N: AtomicUsize = AtomicUsize::new(0);
            if CSIG_N.fetch_add(1, AtomicOrd::Relaxed) < 24 {
                use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
                C0::write_str("nk4a: csig tgt=");
                C0::write_hex(target_nr.0 as u64);
                C0::write_str(" sig=");
                C0::write_hex(sig_nr as u64);
                C0::write_str("\n");
            }
        }
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
            // Same enqueue half as C's RTS_UNSET (B6 family): the notify may
            // have cleared the sig manager's RECEIVING, so it must be pushed
            // onto the run queue if it just became runnable.
            if let Some(sig_mgr_nr) = proc_table.endpoint_to_nr(sig_mgr_ep) {
                proc_table.enqueue_if_woken(sig_mgr_nr);
            }
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
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    msg: &mut Message,
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
        if sig_mgr != proc_table.get(caller_nr).map(|p| p.p_endpoint) {
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
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    msg: &Message,
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
    if sig_mgr != proc_table.get(caller_nr).map(|p| p.p_endpoint) {
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
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    msg: &Message,
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

    // C: do_sigsend.c:79-82 — sigsend of an unsaved process is EINVAL.
    // A process with no recorded kernel entry has no return path to resume
    // after the handler finishes, so delivery is rejected before any state
    // is captured or copied. (C checks after filling the in-register frame
    // at :77 but before its copy at :120; rejecting here is the same
    // observable contract with no wasted work.)
    if proc_table
        .get(target_nr)
        .is_none_or(|p| p.trap_style == TrapStyle::NoEntry)
    {
        return KcallResult::Ok(EINVAL);
    }

    // ── Step 1: Copy sigmsg from caller's user space ──
    // C: do_sigsend.c:36-39 — data_copy_vmcheck(caller, caller_ep, sigctx, KERNEL, &smsg, sizeof)
    let smsg: SigMsg = SigMsg::default();
    {
        let smsg_phys = CurrentDirectMap::virt_to_phys(VirBytes(
            &smsg as *const SigMsg as u64,
        ));
        let caller_endpt = proc_table
            .get(caller_nr)
            .map(|p| p.p_endpoint)
            .expect("dispatch_sigreturn: caller slot must exist");
        let caller_cr3 = proc_table
            .get(caller_nr)
            .map(|p| p.p_seg.phys_root)
            .expect("dispatch_sigreturn: caller slot must exist");
        let proc_cr3 = |pt: &crate::proc_table::ProcessTable, ep: Endpoint| {
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
            caller_nr, proc_table,
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
        let mut sctx = CurrentSignalContext::build_sigcontext(&target.cpu_context, &mut info);
        // C: do_sigsend.c:77 — fr.sf_sc.trap_style = rp->p_seg.p_kern_trap_style.
        // The NoEntry case was rejected above; stamp before build_sigframe
        // copies the sigcontext into the frame.
        let entry_style = target.trap_style;
        CurrentSignalContext::set_trap_style(&mut sctx, entry_style);
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
        let caller_endpt = proc_table
            .get(caller_nr)
            .map(|p| p.p_endpoint)
            .expect("dispatch_sigreturn: caller slot must exist");
        let caller_cr3 = proc_table
            .get(caller_nr)
            .map(|p| p.p_seg.phys_root)
            .expect("dispatch_sigreturn: caller slot must exist");
        let proc_cr3 = |pt: &crate::proc_table::ProcessTable, ep: Endpoint| {
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
            caller_nr, proc_table,
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
/// 3. Validate the sigcontext's trap style (MINIX3 BUG fix — C records it
///    unchecked at do_sigreturn.c:81 and panics at return on bad values)
/// 4. Restore target's `CpuContext` from `SigContext` (C: do_sigreturn.c:42-80)
///    and re-record the validated style (C: arch_system.c:563)
/// 5. Check magic integrity (C: do_sigreturn.c:83)
pub fn dispatch_sigreturn(
    caller_nr: ProcNr,
    proc_table: &mut ProcessTable,
    msg: &Message,
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
        let caller_endpt = proc_table
            .get(caller_nr)
            .map(|p| p.p_endpoint)
            .expect("dispatch_sigreturn: caller slot must exist");
        let caller_cr3 = proc_table
            .get(caller_nr)
            .map(|p| p.p_seg.phys_root)
            .expect("dispatch_sigreturn: caller slot must exist");
        let proc_cr3 = |pt: &crate::proc_table::ProcessTable, ep: Endpoint| {
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
        match data_copy_vmcheck(caller_nr, proc_table, src, dst, sctx_size, proc_cr3) {
            CrossSpaceResult::Suspended(_) => return KcallResult::VmSuspend,
            CrossSpaceResult::Completed(Err(_)) => return KcallResult::Ok(EFAULT),
            CrossSpaceResult::Completed(Ok(())) => {}
        }
    }

    // ── Step 2: Restore target's registers from sigcontext ──
    // C: do_sigreturn.c:42-80 — arch-specific register restore
    // C: do_sigreturn.c:81 — arch_proc_setcontext(rp, &rp->p_reg, 1, sc.trap_style)
    //
    // MINIX3 BUG: C copies the user-controlled `sc.trap_style` unchecked
    // into `p_kern_trap_style`; the return path later panics on unknown
    // values ("unknown trap style recorded" / "no entry trap style known",
    // arch_system.c:597-605) — both arms are reachable from a crafted
    // sigframe, i.e. a user process can panic the kernel. Rust validates
    // before touching the context: unknown or entry-less styles are
    // rejected with EINVAL and the target's register state is left
    // untouched. Valid styles behave exactly as in C.
    {
        let style = match TrapStyle::from_raw(CurrentSignalContext::get_trap_style(&sctx)) {
            Some(s) if s != TrapStyle::NoEntry => s,
            _ => return KcallResult::Ok(EINVAL),
        };
        let target = match proc_table.get_mut(target_nr) {
            Some(p) => p,
            None => return KcallResult::Ok(EINVAL),
        };
        CurrentSignalContext::restore_sigcontext(&mut target.cpu_context, &sctx);
        // C: arch_system.c:563 — p->p_seg.p_kern_trap_style = trap_style
        target.trap_style = style;
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
        let mut msg = Message::default();
        msg.m_type = Syscall::Sigsend as i32;
        // Invalid endpoint → EINVAL
        let mut proc_table = crate::test_helpers::test_proc_table();
        let result = dispatch_sigsend(ProcNr(0), &mut proc_table, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sigsend_kernel_process() {
        let mut msg = Message::default();
        msg.m_type = Syscall::Sigsend as i32;
        // Set endpoint to a kernel process (negative proc_nr)
        let mut proc_table = crate::test_helpers::test_proc_table();
        // Kernel processes have negative proc_nr, but endpoint_to_nr
        // won't find them in the table, so we get EINVAL.
        let result = dispatch_sigsend(ProcNr(0), &mut proc_table, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    #[test]
    fn test_sigreturn_invalid_endpoint() {
        let mut msg = Message::default();
        msg.m_type = Syscall::Sigreturn as i32;
        let mut proc_table = crate::test_helpers::test_proc_table();
        let result = dispatch_sigreturn(ProcNr(0), &mut proc_table, &msg);
        assert_eq!(result, KcallResult::Ok(EINVAL));
    }

    /// C: do_sigsend.c:79-82 — sigsend of an unsaved process (no recorded
    /// kernel entry) is EINVAL. The check sits before every memory copy, so
    /// it is reachable hosted: the result is EINVAL, not VmSuspend.
    #[test]
    fn test_sigsend_unsaved_process_returns_einval() {
        let mut proc_table = crate::test_helpers::test_proc_table();
        proc_table
            .get_mut(ProcNr(0))
            .unwrap()
            .p_rts_flags
            .clear(crate::proc::RtsFlagsBits::SLOT_FREE);
        let target_endpoint = proc_table.get(ProcNr(0)).unwrap().p_endpoint;
        // Fresh process: trap_style is NoEntry — it has never entered.

        let mut msg = Message::default();
        msg.m_type = Syscall::Sigsend as i32;
        msg.m_u.m_sigcalls.endpt = target_endpoint.0;
        msg.m_u.m_sigcalls.sigctx = 0x7000;

        let result = dispatch_sigsend(ProcNr(0), &mut proc_table, &msg);
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

    #[test]
    fn test_is_lethal() {
        // C: SIGS_IS_LETHAL — signal.h:280-282（SIGILL/ABRT/EMT/FPE/BUS/SEGV）
        assert!(is_lethal(SIGILL)); // 4
        assert!(is_lethal(SIGABRT)); // 6
        assert!(is_lethal(SIGEMT)); // 7
        assert!(is_lethal(SIGFPE)); // 8
        assert!(is_lethal(SIGBUS)); // 10
        assert!(is_lethal(SIGSEGV)); // 11
        // 非致命（含 SIGKILL=9 / SIGTERM=15 / SIGSYS=12）与内核信号（SIGKSIG=74）
        assert!(!is_lethal(SIGTRAP)); // 5
        assert!(!is_lethal(9)); // SIGKILL
        assert!(!is_lethal(12)); // SIGSYS
        assert!(!is_lethal(15)); // SIGTERM
        assert!(!is_lethal(SIGKSIG)); // 74
    }

    // ── cause_signal 行为测试（外部路径 / 去重 / SELF / 致命 SELF）──
    //
    // 通知可观测性说明：cause_signal 的 mini_notify_core 副作用在目标
    // "正在 RECEIVE 且 p_getfrom_e == ANY" 时是直接投递（置 DELIVERMSG +
    // 清 RECEIVING）。测试把 manager 置于监听态来观测"通知发生过"，用
    // DELIVERMSG 的置位与否判定 cause_signal 的 RTS_SIGNALED 门控。

    /// 占用 target/manager 两个进程槽并绑定 priv 0/1。
    /// target 默认自管理（s_sig_mgr = 自身 endpoint）。
    fn occupy_two_slots(
        target_nr: ProcNr,
        manager_nr: ProcNr,
    ) -> (Endpoint, Endpoint, crate::test_helpers::TestProcTable, crate::test_helpers::TestPrivTable) {
        let mut procs = crate::test_helpers::test_proc_table();
        let mut privs = crate::test_helpers::test_priv_table();

        {
            let p = procs.get_mut(target_nr).expect("target slot");
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.priv_id = Some(0);
        }
        let target_ep = procs.get(target_nr).unwrap().p_endpoint;
        privs.get_mut(0).unwrap().identity.s_proc_nr = Some(target_nr);
        privs.get_mut(0).unwrap().signals.s_sig_mgr = target_ep;

        {
            let p = procs.get_mut(manager_nr).expect("manager slot");
            p.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
            p.priv_id = Some(1);
        }
        let manager_ep = procs.get(manager_nr).unwrap().p_endpoint;
        privs.get_mut(1).unwrap().identity.s_proc_nr = Some(manager_nr);

        (target_ep, manager_ep, procs, privs)
    }

    /// 让进程进入监听态：RECEIVING + p_getfrom_e == ANY（可被 mini_notify 直接投递）。
    fn listen(procs: &mut ProcessTable, nr: ProcNr) {
        let p = procs.get_mut(nr).unwrap();
        p.p_rts_flags.set(RtsFlagsBits::RECEIVING);
        p.p_getfrom_e = Endpoint::ANY;
    }

    #[test]
    fn test_cause_signal_external_path_notifies_manager() {
        let (target_ep, manager_ep, mut procs, mut privs) =
            occupy_two_slots(ProcNr(0), ProcNr(1));
        // 目标由外部 manager 管理（C system.c:412：s_sig_mgr = manager endpoint）
        privs.get_mut(0).unwrap().signals.s_sig_mgr = manager_ep;
        // manager 监听 → 通知以直接投递（DELIVERMSG）形式可观测
        listen(&mut procs, ProcNr(1));

        cause_signal(ProcNr(0), SIGTRAP, &mut procs, &mut privs);

        // C system.c:442,444 — sigaddset(&rp->p_pending) + RTS_SIGNALED|RTS_SIG_PENDING
        let target = procs.get(ProcNr(0)).unwrap();
        assert!(target.p_pending.contains(SIGTRAP as u8));
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING));

        // C system.c:445-446 + system.c:381 — send_sig(manager, SIGKSIG) → mini_notify
        let manager = procs.get(ProcNr(1)).unwrap();
        assert!(manager.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert_eq!(manager.p_endpoint, manager_ep);
        assert_eq!(target.p_endpoint, target_ep);
    }

    #[test]
    fn test_cause_signal_dedup_does_not_notify_twice() {
        let (_, manager_ep, mut procs, mut privs) =
            occupy_two_slots(ProcNr(0), ProcNr(1));
        privs.get_mut(0).unwrap().signals.s_sig_mgr = manager_ep;

        // 第一次投递：RTS_SIGNALED 未置 → 通知 manager
        listen(&mut procs, ProcNr(1));
        cause_signal(ProcNr(0), SIGTRAP, &mut procs, &mut privs);
        assert!(procs.get(ProcNr(1)).unwrap()
            .p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));

        // 清掉可观测状态并重新置 manager 为监听
        procs.get_mut(ProcNr(1)).unwrap()
            .p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
        listen(&mut procs, ProcNr(1));

        // C system.c:443 — if (!RTS_ISSET(rp, RTS_SIGNALED)) 才通知；
        // 第二次 cause_signal（不同信号）只加 p_pending 位，不重复通知。
        cause_signal(ProcNr(0), SIGPROF, &mut procs, &mut privs);

        let target = procs.get(ProcNr(0)).unwrap();
        assert!(target.p_pending.contains(SIGTRAP as u8));
        assert!(target.p_pending.contains(SIGPROF as u8));
        let manager = procs.get(ProcNr(1)).unwrap();
        assert!(!manager.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(manager.p_rts_flags.is_set(RtsFlagsBits::RECEIVING),
            "未重复通知时 manager 应保持监听态");
    }

    #[test]
    fn test_cause_signal_self_path_non_lethal() {
        // fixture 默认 target 自管理（s_sig_mgr = 自身 endpoint）
        let (_, _, mut procs, mut privs) = occupy_two_slots(ProcNr(0), ProcNr(1));

        // C system.c:416 — rp->p_endpoint == sig_mgr → 自管理路径
        // SIGTERM = 15（signal.h:67）
        cause_signal(ProcNr(0), 15, &mut procs, &mut privs);

        // C system.c:433 — sigaddset(&priv(rp)->s_sig_pending, sig)
        let priv0 = privs.get(0).unwrap();
        assert!(priv0.signals.s_sig_pending.contains(15_u8),
            "自管理非致命信号应记入进程自身 priv 的 s_sig_pending");
        // C 自管理路径不设 RTS_SIGNALED（进程不被挂起，由自身处理）
        let target = procs.get(ProcNr(0)).unwrap();
        assert!(!target.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert!(!target.p_pending.contains(15_u8),
            "自管理路径不写 p_pending（那是外部管理器轮询的位图）");
    }

    #[test]
    fn test_cause_signal_self_lethal_promotes_backup() {
        let (_, manager_ep, mut procs, mut privs) =
            occupy_two_slots(ProcNr(0), ProcNr(1));
        // 自管理 + 预设 backup（C system.c:418-419 — s_bak_sig_mgr）
        privs.get_mut(0).unwrap().signals.s_bak_sig_mgr = manager_ep;
        // backup 以 RTS_NO_PRIV 挂起（C system.c:424 提升时解除）
        procs.get_mut(ProcNr(1)).unwrap()
            .p_rts_flags.set(RtsFlagsBits::NO_PRIV);
        listen(&mut procs, ProcNr(1));

        cause_signal(ProcNr(0), SIGABRT, &mut procs, &mut privs);

        // C system.c:421-422 — 提升：s_sig_mgr ← backup，s_bak_sig_mgr ← NONE
        let priv0 = privs.get(0).unwrap();
        assert_eq!(priv0.signals.s_sig_mgr, manager_ep);
        assert_eq!(priv0.signals.s_bak_sig_mgr, Endpoint::NONE);

        // C system.c:424 — RTS_UNSET(backup, RTS_NO_PRIV)
        let backup = procs.get(ProcNr(1)).unwrap();
        assert!(!backup.p_rts_flags.is_set(RtsFlagsBits::NO_PRIV));

        // 递归重投走外部路径：p_pending + RTS_SIGNALED + 通知新管理器
        let target = procs.get(ProcNr(0)).unwrap();
        assert!(target.p_pending.contains(SIGABRT as u8));
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert!(target.p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING));
        assert!(backup.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG),
            "提升后 backup 应收到 SIGKSIG 通知");
    }

    #[test]
    #[should_panic(expected = "cause_sig: sig manager")]
    fn test_cause_signal_self_lethal_no_backup_panics() {
        // 自管理 + 致命信号 + 无 backup → C system.c:429-431 panic
        let (_, _, mut procs, mut privs) = occupy_two_slots(ProcNr(0), ProcNr(1));
        cause_signal(ProcNr(0), SIGABRT, &mut procs, &mut privs);
    }

    // ── T-1: GETKSIG / ENDKSIG / sigsend / sigreturn 行为矩阵 ──

    #[test]
    fn test_getksig_delivers_pending_then_reports_none() {
        // C do_getksig.c:27-40 — 第一次调用：找到 SIGNALED 目标，回填
        // (endpt, map) 并清 SIGNALED + p_pending；第二次调用：无待处理 →
        // endpt = NONE。
        let (target_ep, manager_ep, mut procs, mut privs) =
            occupy_two_slots(ProcNr(0), ProcNr(1));
        // 目标的信号管理器改为 manager（C: priv(rp)->s_sig_mgr）
        privs.get_mut(0).unwrap().signals.s_sig_mgr = manager_ep;
        {
            let t = procs.get_mut(ProcNr(0)).unwrap();
            t.p_rts_flags.set(RtsFlagsBits::SIGNALED);
            t.p_pending.add(SIGTRAP as u8);
        }
        // K20 caller-by-nr: the caller is slot ProcNr(1); its endpoint must
        // live on the slot (the standalone handle no longer applies).
        procs.get_mut(ProcNr(1)).unwrap().p_endpoint = manager_ep;
        let mut msg = Message::default();
        msg.m_type = Syscall::Getksig as i32;

        let result = dispatch_getksig(ProcNr(1), &mut procs, &mut msg, &privs);
        assert_eq!(result, KcallResult::Ok(OK));
        // SAFETY: 回填变体由 dispatch_getksig 写入；测试侧 unsafe 读。
        let (got_endpt, got_map) = unsafe { (msg.m_u.m_sigcalls.endpt, msg.m_u.m_sigcalls.map) };
        assert_eq!(got_endpt, target_ep.get());
        // SigSet 位编码与 C sigset_t 同款：信号号 s → bit (s-1)（POSIX sigaddset）。
        assert_eq!(got_map, 1u64 << (SIGTRAP - 1));
        // C do_getksig.c:33-34 — 消费后清 p_pending + RTS_SIGNALED
        let t = procs.get(ProcNr(0)).unwrap();
        assert!(!t.p_rts_flags.is_set(RtsFlagsBits::SIGNALED));
        assert_eq!(t.p_pending.get(), 0);

        // 第二次调用：无 SIGNALED 目标 → endpt = NONE（C do_getksig.c:40）
        let mut msg2 = Message::default();
        msg2.m_type = Syscall::Getksig as i32;
        let result2 = dispatch_getksig(ProcNr(1), &mut procs, &mut msg2, &privs);
        assert_eq!(result2, KcallResult::Ok(OK));
        let none_endpt = unsafe { msg2.m_u.m_sigcalls.endpt };
        assert_eq!(none_endpt, Endpoint::NONE.get());
    }

    #[test]
    fn test_endksig_clears_sig_pending_when_no_new_signal() {
        // C do_endksig.c:35-36 — SIG_PENDING 置位且无新信号（SIGNALED 清）→
        // 清 RTS_SIG_PENDING，返回 OK。
        let (target_ep, manager_ep, mut procs, mut privs) =
            occupy_two_slots(ProcNr(0), ProcNr(1));
        privs.get_mut(0).unwrap().signals.s_sig_mgr = manager_ep;
        procs.get_mut(ProcNr(0)).unwrap()
            .p_rts_flags.set(RtsFlagsBits::SIG_PENDING);
        // K20 caller-by-nr: the caller is slot ProcNr(1); its endpoint must
        // live on the slot (the standalone handle no longer applies).
        procs.get_mut(ProcNr(1)).unwrap().p_endpoint = manager_ep;
        let mut msg = Message::default();
        msg.m_type = Syscall::Endksig as i32;
        // SAFETY: m_type set above; test-only union write (msg_sigcalls 读侧同款).
        unsafe { msg.m_u.m_sigcalls.endpt = target_ep.get(); }

        let result = dispatch_endksig(ProcNr(1), &mut procs, &msg, &privs);
        assert_eq!(result, KcallResult::Ok(OK));
        assert!(!procs.get(ProcNr(0)).unwrap()
            .p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING),
            "无新信号时 ENDKSIG 必须清 RTS_SIG_PENDING");
    }

    #[test]
    fn test_endksig_keeps_sig_pending_when_new_signal_arrived() {
        // C do_endksig.c:35 — 若 ENDKSIG 处理期间新信号到达（SIGNALED 置位），
        // RTS_SIG_PENDING 保留（管理器需再次 GETKSIG）。
        let (target_ep, manager_ep, mut procs, mut privs) =
            occupy_two_slots(ProcNr(0), ProcNr(1));
        privs.get_mut(0).unwrap().signals.s_sig_mgr = manager_ep;
        {
            let t = procs.get_mut(ProcNr(0)).unwrap();
            t.p_rts_flags.set(RtsFlagsBits::SIG_PENDING);
            t.p_rts_flags.set(RtsFlagsBits::SIGNALED);
        }
        // K20 caller-by-nr: the caller is slot ProcNr(1); its endpoint must
        // live on the slot (the standalone handle no longer applies).
        procs.get_mut(ProcNr(1)).unwrap().p_endpoint = manager_ep;
        let mut msg = Message::default();
        msg.m_type = Syscall::Endksig as i32;
        // SAFETY: m_type set above; test-only union write (msg_sigcalls 读侧同款).
        unsafe { msg.m_u.m_sigcalls.endpt = target_ep.get(); }

        let result = dispatch_endksig(ProcNr(1), &mut procs, &msg, &privs);
        assert_eq!(result, KcallResult::Ok(OK));
        assert!(procs.get(ProcNr(0)).unwrap()
            .p_rts_flags.is_set(RtsFlagsBits::SIG_PENDING),
            "新信号已到达（SIGNALED 置位）时 SIG_PENDING 必须保留");
    }

    #[test]
    fn test_sigsend_unmapped_sigctx_returns_vmsuspend() {
        // C do_sigsend.c:36-39 — Step 1 从调用者用户空间拷贝 sigmsg；
        // sigctx 指向未映射地址 → PTE walk 页失败 → VmSuspend（VM 协助）。
        let (target_ep, _, mut procs, _privs) = occupy_two_slots(ProcNr(0), ProcNr(1));
        // 目标必须有已记录的内核入口（NoEntry → EINVAL，到不了拷贝）
        procs.get_mut(ProcNr(0)).unwrap().trap_style =
            minix_arch::TrapStyle::IntHard;
        procs.get_mut(ProcNr(1)).unwrap().p_endpoint = Endpoint(200);
        let mut msg = Message::default();
        msg.m_type = Syscall::Sigsend as i32;
        // SAFETY: m_type set above; test-only union write.
        unsafe {
            msg.m_u.m_sigcalls.endpt = target_ep.get();
            msg.m_u.m_sigcalls.sigctx = 0x1000; // 未映射
        }

        let result = dispatch_sigsend(ProcNr(0), &mut procs, &msg);
        assert_eq!(result, KcallResult::VmSuspend,
            "未映射 sigctx 的拷贝必须挂起等待 VM 协助");
    }

    #[test]
    fn test_sigreturn_unmapped_sigctx_returns_vmsuspend() {
        // C do_sigreturn.c:33-35 — Step 1 从目标用户空间拷回 sigcontext；
        // sigctx 未映射 → VmSuspend（Rust 统一用 data_copy_vmcheck，
        // 用户栈可页失败——doc 注释明示与 C data_copy 的差异）。
        let (target_ep, _, mut procs, _privs) = occupy_two_slots(ProcNr(0), ProcNr(1));
        procs.get_mut(ProcNr(1)).unwrap().p_endpoint = Endpoint(200);
        let mut msg = Message::default();
        msg.m_type = Syscall::Sigreturn as i32;
        // SAFETY: m_type set above; test-only union write.
        unsafe {
            msg.m_u.m_sigcalls.endpt = target_ep.get();
            msg.m_u.m_sigcalls.sigctx = 0x1000; // 未映射
        }

        let result = dispatch_sigreturn(ProcNr(0), &mut procs, &msg);
        assert_eq!(result, KcallResult::VmSuspend,
            "未映射 sigctx 的 sigcontext 拷回必须挂起等待 VM 协助");
    }
}
