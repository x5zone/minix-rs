//! PM call dispatch table (C: `table.c:call_vec` + the 47 call numbers of
//! `callnr.h`).
//!
//! Document: `rewrite-notes/04-stage-pm/04-ipc-dispatch.md`.
//!
//! # Correspondence with C
//!
//! - The 47 `PM_*` call numbers in `callnr.h:14-60` → the [`PmCall`] enum
//!   discriminants (`#[repr(i32)]`, values 1..=47, exactly matching C's
//!   `PM_BASE + N`).
//! - The `call_vec[NR_PM_CALLS]` function-pointer table in `table.c:23-59` →
//!   the match dispatch in [`dispatch_pm_call`] (ARCH A-5: C's table-driven
//!   pointers → Rust's compile-time exhaustive match).
//! - C returns ENOSYS for a NULL or out-of-range slot (main.c:94-101) → Rust
//!   returns ENOSYS for an unregistered call number; a registered call whose
//!   handler is not implemented returns ENOSYS as a placeholder too (DEFERRED,
//!   owned by documents 07~20).
//!
//! The call-number constants are not listed separately (no repeating the 47
//! `pub const`s of callnr.h): the [`PmCall`] discriminants are the single
//! source of truth, authoritatively defined in minix-types' `ipc/pm.rs` (same
//! library and origin as the `Mess*` payloads); this module only re-exports
//! them and dispatches.

use crate::event::{EventRegistry, PmEventServices};
use crate::ipc::{IpcTransport, ReplyIntent};
use crate::mproc::ProcTable;
use crate::misc::MiscError;
use crate::credentials::{
    do_get, do_set, CopyGroups, GetOp, GetResult, SetOp, SetError, VfsForwarder,
};
use minix_types::{ENOSYS, EINVAL, Endpoint, Gid, Message, PmError, ProcEventMask, UserSlot, VirBytes};
use alloc::string::String;
use alloc::vec::Vec;

/// The PM system call enum has moved up to the shared type library (the call
/// numbers and the `Mess*` payloads live in one library from one origin, and
/// the value-pinning tests moved with it to `pm_call_nr_tests` in
/// `minix-types/src/ipc/pm.rs`). Re-exported here so the `PmCall` path under
/// `crate::ipc` and the [`dispatch_pm_call`] signature stay unchanged.
pub use minix_types::PmCall;

/// Dispatch one registered PM call (C: `call_vec[call_index]()`, main.c:99).
///
/// **Single dispatch table** (ARCH A-5): C's `call_vec` is a 47-entry
/// function-pointer table and every registered call takes the same path; in
/// Rust that is this one exhaustive match — the handlers already in place (7)
/// decode the message payload and call through, and the ones not yet in place
/// (40) return a `Reply(ENOSYS)` placeholder (a transitional difference
/// documented in document 04, section 3.6 D6).
/// The main loop must not bypass this function and intercept a call inline
/// (the seven inline blocks of the earlier implementation are all folded in
/// here).
///
/// How the handler signature differs from C's `int (*)(void)` is described in
/// document 04, section 3.3 (D3/D5): C handlers read the globals `m_in`/`mp`;
/// Rust passes `table`/`events`/`transport` + the caller slot + the message
/// reference explicitly (ARCH A-3: implicit globals → explicit parameters).
// The parameter list is the deliberate shape of ARCH A-3: each parameter is
// its own seam (table / events / transport / kernel gateway / timer bundle /
// caller / message), and they must not be aggregated for a lint count.
#[allow(clippy::too_many_arguments)]
pub fn dispatch_pm_call<T: IpcTransport>(
    call: PmCall,
    table: &mut ProcTable,
    events: &mut EventRegistry,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
    timers: &mut crate::timer::TimerFaces,
    caller: UserSlot,
    msg: &Message,
) -> ReplyIntent {
    match call {
        // C: do_fork (forkexit.c:139 `return SUSPEND`) — fork's reply is not
        // synchronous: do_fork sends VFS_PM_FORK and returns SUSPEND, and the
        // parent/child replies complete asynchronously through
        // VFS_PM_FORK_REPLY (handle_vfs_reply, main.c:369-394). Failures (table
        // full / out of memory / VM refusal) reply with errno synchronously
        // (`return EAGAIN/ENOMEM/s`, forkexit.c:60-79).
        //
        // The error reply matches the C client `_syscall`/`_taskcall` contract
        // (m_type < 0 = −errno) and is isomorphic to the kernel's `reply_wire()`.
        // A positive errno would make the caller take `Ok(positive)` as valid data
        // (for example init treating EAGAIN=11 as a child_pid).
        PmCall::Fork => match crate::fork::do_fork(table, msg.m_source, transport, kern) {
            Ok(_child_pid) => ReplyIntent::ReplyLater,
            Err(e) => ReplyIntent::Reply(-PmError::from(e).to_errno()),
        },
        // C: do_srv_fork (forkexit.c:237/239) — the opposite of fork: reply
        // (child, OK) immediately, then return the pid synchronously.
        PmCall::SrvFork => {
            let (uid, gid) = super::decode::srv_fork(msg);
            let params = crate::mproc::SrvForkParams { uid, gid };
            match crate::fork::do_srv_fork(table, msg.m_source, params, transport, kern) {
                Ok(child_pid) => ReplyIntent::Reply(child_pid),
                Err(e) => ReplyIntent::Reply(-PmError::from(e).to_errno()),
            }
        }
        // C: do_getsetpriority (misc.c:239-286) — GET returns nice minus
        // PRIO_MIN; SET passes the permission gate, calls sched_nice to inform
        // the scheduler, and replies 0. SchedCtl uses the production
        // MinixSchedCtl (SCHEDULING_SET_NICE to mp_scheduler).
        PmCall::GetPriority | PmCall::SetPriority => {
            let (which, who, prio) = super::decode::getsetpriority(msg);
            let mut ctl = crate::sched::MinixSchedCtl::new(transport);
            match crate::sched::do_getsetpriority(
                table,
                caller,
                which,
                who,
                prio,
                call == PmCall::GetPriority,
                &mut ctl,
            ) {
                Ok(v) => ReplyIntent::Reply(v),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_exit (forkexit.c:246-266) — returns SUSPEND and **never replies**
        // (the process is gone, "beyond the grave"), the NoReply subcase of
        // plan.md section 7.3.
        PmCall::Exit => {
            let status = super::decode::exit(msg);
            // do_exit currently always returns NoReply — pass it through rather
            // than discard it, so a future semantic change cannot silently swallow
            // the reply intent.
            crate::exit::do_exit(table, caller, status, transport, kern)
        }
        // C: do_wait4 (forkexit.c:471-542) — reply synchronously
        // (W_STOPCODE/WNOHANG/ECHILD) or return SUSPEND (after wait_test hits,
        // WAITING is set and tell_parent/tell_tracer replies later). The handler
        // returns the reply intent directly.
        PmCall::Wait4 => {
            let (pidarg, options, addr) = super::decode::wait4(msg);
            crate::wait::do_wait4(
                table,
                caller,
                pidarg,
                options as u32,
                VirBytes(addr),
                transport,
                kern,
            )
        }
        // C: do_kill (signal.c:197-204) — reply 0 on success; when the caller
        // kills itself while already EXITING the result is SUSPEND (check_sig's
        // sig_proc_exit chain, signal.c:384) → no reply this time.
        PmCall::Kill => {
            let (pid, signo) = super::decode::kill(msg);
            match crate::signal::do_kill(table, caller, pid, signo, kern, transport) {
                Ok(_count) => {
                    if table.procs[caller.get()].state.lifecycle.is_exiting() {
                        ReplyIntent::ReplyLater
                    } else {
                        ReplyIntent::Reply(0)
                    }
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_srv_kill (signal.c:204-211) — the same reply pattern as do_kill.
        PmCall::SrvKill => {
            let (pid, signo) = super::decode::srv_kill(msg);
            match crate::signal::do_srv_kill(table, caller, pid, signo, kern, transport) {
                Ok(_count) => {
                    if table.procs[caller.get()].state.lifecycle.is_exiting() {
                        ReplyIntent::ReplyLater
                    } else {
                        ReplyIntent::Reply(0)
                    }
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_proceventmask (event.c:179-206) — the mask update goes through
        // EventRegistry (06-event-subscription.md), and the registry decides the
        // reply intent.
        PmCall::ProcEventMask => {
            let mask_bits = super::decode::proceventmask(msg);
            let mask = ProcEventMask::from_bits_truncate(mask_bits);
            events.do_proceventmask_mut(caller, mask, table, transport, kern)
        }
        // C: do_trace (trace.c:42-250) — the full ptrace command face. Guard and
        // permission errors reply with errno synchronously; for an OK reply the
        // data payload is pre-filled through ipc.reply (`m_pm_lc_ptrace.data`,
        // C trace.c:59 and similar), and the main loop's reply() sends the whole
        // thing with the return code carried in m_type.
        PmCall::Ptrace => {
            let (pid, preq, addr, data) = super::decode::ptrace(msg);
            let req = crate::trace::PtraceReq { req: preq, pid, addr, data };
            match crate::trace::do_trace(table, caller, req, kern, transport) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_setitimer. value/ovalue are user pointers to `struct itimerval`
        // (32 bytes: interval@0..16 + value@16..32, each field being
        // tv_sec@0..8 + tv_usec@8..16); the byte movement goes through the
        // gateway copy seam. The old value is written back only when ovalue is
        // non-null.
        PmCall::Itimer => {
            let (which, value_ptr, ovalue_ptr) = super::decode::itimer(msg);
            let mut raw = [0u8; 32];
            if let Err(e) = kern.copy_from_user(msg.m_source, value_ptr, &mut raw) {
                return ReplyIntent::Reply(e);
            }
            let le_i64 = |b: &[u8]| i64::from_le_bytes(b.try_into().unwrap());
            let op = crate::timer::ItimerOp {
                set: Some(crate::timer::Itimerval {
                    it_interval: crate::timer::Timeval {
                        tv_sec: le_i64(&raw[0..8]),
                        tv_usec: le_i64(&raw[8..16]),
                    },
                    it_value: crate::timer::Timeval {
                        tv_sec: le_i64(&raw[16..24]),
                        tv_usec: le_i64(&raw[24..32]),
                    },
                }),
                get: ovalue_ptr != 0,
            };
            let conv = crate::timer::TicksConv { hz: timers.system_hz };
            match crate::timer::do_itimer(
                table,
                caller,
                which,
                op,
                &conv,
                timers.vctl,
                timers.tctl,
            ) {
                Ok(old) => {
                    if let Some(old) = old
                        && ovalue_ptr != 0
                    {
                        let mut out = [0u8; 32];
                        out[0..8].copy_from_slice(&old.it_interval.tv_sec.to_le_bytes());
                        out[8..16].copy_from_slice(&old.it_interval.tv_usec.to_le_bytes());
                        out[16..24].copy_from_slice(&old.it_value.tv_sec.to_le_bytes());
                        out[24..32].copy_from_slice(&old.it_value.tv_usec.to_le_bytes());
                        if let Err(e) = kern.copy_to_user(&out, msg.m_source, ovalue_ptr) {
                            return ReplyIntent::Reply(e);
                        }
                    }
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_sigaction (signal.c:40-86). act/oact are user pointers to
        // `struct sigaction` (LP64, 32 bytes: handler@0/mask@8/flags@24); the
        // byte movement goes through the gateway copy seam. The old value is
        // snapshotted before the install (handle_sigaction); the only ordering
        // difference from C's copy-oact-first is fault ordering, and the C
        // anchor is noted on the handler.
        PmCall::SigAction => {
            let (nr, act_ptr, oact_ptr, ret) = super::decode::sigaction(msg);
            let mut svec = [0u8; 32];
            let act = if act_ptr != 0 {
                if let Err(e) = kern.copy_from_user(msg.m_source, act_ptr, &mut svec) {
                    return ReplyIntent::Reply(e);
                }
                let le_u64 = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap());
                Some(crate::mproc::SigAction {
                    sa_handler: le_u64(&svec[0..8]) as usize,
                    sa_mask: le_u64(&svec[8..16]),
                    sa_flags: i32::from_le_bytes(svec[24..28].try_into().unwrap()),
                })
            } else {
                None
            };
            match handle_sigaction(
                table,
                caller,
                SigActionReq { signo: nr, act, need_oact: oact_ptr != 0, sigreturn: VirBytes(ret) },
            ) {
                Ok(old) => {
                    if let (Some(old), false) = (old, oact_ptr == 0) {
                        let mut out = [0u8; 32];
                        out[0..8].copy_from_slice(&(old.sa_handler as u64).to_le_bytes());
                        out[8..16].copy_from_slice(&old.sa_mask.to_le_bytes());
                        out[24..28].copy_from_slice(&old.sa_flags.to_le_bytes());
                        if let Err(e) = kern.copy_to_user(&out, msg.m_source, oact_ptr) {
                            return ReplyIntent::Reply(e);
                        }
                    }
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_sigsuspend (signal.c:157-171) — store the mask, install the new
        // mask, then suspend.
        PmCall::SigSuspend => {
            let (_how, _ctx, set) = super::decode::sigset(msg);
            handle_sigsuspend(table, caller, set)
        }
        // C: do_sigpending (signal.c:88-97) — reply payload m_pm_lc_sigset.set.
        PmCall::SigPending => {
            let pending = handle_sigpending(table, caller);
            table.procs[caller.get()].ipc.reply = Some(Message {
                m_u: minix_types::MessageUnion {
                    m_pm_lc_sigset: minix_types::MessPmLcSigset {
                        set: [pending as u32, (pending >> 32) as u32, 0, 0],
                        _padding: [0; 40],
                    },
                },
                ..Message::default()
            });
            ReplyIntent::Reply(0)
        }
        // C: do_sigprocmask (signal.c:99-155) — the old mask comes back in the
        // reply payload; all three success branches end with an inline
        // check_pending(mp) (signal.c:122/131/139). `MaskOpEffect`'s
        // `needs_check` is the semantic equivalent of that same decision (Block
        // only adds mask bits, so it cannot create a newly deliverable signal —
        // a check there would spin immediately; Unblock/SetMask are true).
        PmCall::SigProcMask => {
            let (how, _ctx, set) = super::decode::sigset(msg);
            match handle_sigprocmask(table, caller, how, set) {
                Ok((old, effect)) => {
                    table.procs[caller.get()].ipc.reply = Some(Message {
                        m_u: minix_types::MessageUnion {
                            m_pm_lc_sigset: minix_types::MessPmLcSigset {
                                set: [old as u32, (old >> 32) as u32, 0, 0],
                                _padding: [0; 40],
                            },
                        },
                        ..Message::default()
                    });
                    // Production wiring: PmEventServices (RestartServices fully
                    // implemented, sig_proc production leg). BrokenOnVfs is C's
                    // same-named exit (VFS|EVENT suspended, left to the unpause
                    // path).
                    if let crate::mproc::MaskOpEffect::Changed { needs_check: true } = effect {
                        let mut svc = PmEventServices::new(transport, kern);
                        //[SD19-gate] PM send/notify/reply fire-and-forget
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = crate::signal_flow::check_pending(table, caller, &mut svc);
                    }
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_sigreturn (signal.c:173-192) — restore the mask + sys_sigreturn,
        // then an unconditional check_pending(mp) on success (190).
        PmCall::SigReturn => {
            let (_how, ctx, set) = super::decode::sigset(msg);
            let ok = {
                let mut sig_kern = SigReturnKern(kern);
                handle_sigreturn(table, caller, set, VirBytes(ctx), &mut sig_kern)
            };
            match ok {
                Ok(()) => {
                    let mut svc = PmEventServices::new(transport, kern);
                    //[SD19-gate] PM send/notify/reply fire-and-forget
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = crate::signal_flow::check_pending(table, caller, &mut svc);
                    ReplyIntent::Reply(0)
                }
                // The fault payload is sys_sigreturn's raw negative errno.
                Err(crate::signal_handlers::SigReturnError::Fault(code)) => {
                    ReplyIntent::Reply(code)
                }
            }
        }
        // ===== Credentials family, 13 calls (getset.c) =====
        PmCall::GetPid => {
            match do_get(table, caller, GetOp::GetPid, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::SetUid => {
            let id = super::decode::setid(msg);
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetUid(id), &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::GetUid => {
            match do_get(table, caller, GetOp::GetUid, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::SetGroups => {
            // C getset.c:184-206 — num<0 → EINVAL; the gid array is copied in
            // through the copy seam, then the whole set is installed and
            // forwarded to VFS.
            let (num, ptr) = super::decode::groups(msg);
            if num < 0 {
                return ReplyIntent::Reply(EINVAL);
            }
            let mut copier = SysCopyGroups { kern, who: msg.m_source };
            match copier.copy_from_user(VirBytes(ptr), num as usize) {
                Ok(gids) => {
                    let mut fwd = SysVfsForward { transport };
                    match do_set(
                        table,
                        caller,
                        SetOp::SetGroups { gids },
                        &mut NoopGroups,
                        &mut fwd,
                    ) {
                        Ok(intent) => intent,
                        Err(e) => ReplyIntent::Reply(-e.to_errno()),
                    }
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::GetGroups => {
            let (num, ptr) = super::decode::groups(msg);
            let mut copier = SysCopyGroups { kern, who: msg.m_source };
            match do_get(table, caller, GetOp::GetGroups { count: num, ptr: VirBytes(ptr) }, &mut copier) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::SetGid => {
            let id = super::decode::setid(msg);
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetGid(id), &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::GetGid => {
            match do_get(table, caller, GetOp::GetGid, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::SetSid => {
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetSid, &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::GetPgrp => {
            match do_get(table, caller, GetOp::GetPgrp, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::SetEUid => {
            let id = super::decode::setid(msg);
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetEUid(id), &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::SetEGid => {
            let id = super::decode::setid(msg);
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetEGid(id), &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::IsSetUid => {
            match do_get(table, caller, GetOp::Issetugid, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::GetSid => {
            let pid = super::decode::getsid(msg);
            match do_get(table, caller, GetOp::GetSid { pid }, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // ===== Exec family, three calls (exec.c) =====
        // C: do_exec (pm/exec.c:38-56) — exec is initiated by VFS/RS: forward
        // the five VFS_PM_EXEC fields, then tell_vfs, then return SUSPEND.
        PmCall::Exec => {
            let (path, path_len, frame, frame_len, ps_str) = super::decode::exec(msg);
            let req = crate::exec::ExecRequest {
                caller,
                endpoint: table.procs[caller.get()].endpoint(),
                path: VirBytes(path),
                path_len: path_len as usize,
                frame: VirBytes(frame),
                frame_len: frame_len as usize,
                ps_str: VirBytes(ps_str),
            };
            let mut vfs_fwd = SysVfsExec { transport };
            match crate::exec::do_exec(table, caller, req, &mut vfs_fwd) {
                Ok(intent) => intent,
                // C's PM defines _SYSTEM=1, which makes the errno constants
                // negative (errno.h:189 `_SIGN = -`, pm.h:4); do_exec returns a
                // negative EPERM and main.c:106 puts that value into m_type. The
                // wire contract is a negative errno (on the user side
                // `m_type < 0 → errno = -m_type`, measured the same in the compat
                // layer). Replying a positive number would drift from the wire
                // contract, so the value is negated, the same shape as the fork
                // arm (`Reply(-to_errno())`).
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_newexec (pm/exec.c:64-123) — endpt@0/ptr@8, where ptr points at
        // the exec_info in VFS memory (192 bytes copied in); the reply payload is
        // m_pm_lexec_exec_new.suid@0 (C :120-122).
        PmCall::ExecNew => {
            let (endpoint_raw, ptr) = super::decode::exec_new(msg);
            let mut info_raw = [0u8; EXEC_INFO_COPY_SIZE];
            if let Err(e) = kern.copy_from_user(msg.m_source, ptr, &mut info_raw) {
                return ReplyIntent::Reply(e);
            }
            let le_u64 = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap());
            let progname = {
                let mut n = [0u8; 16];
                n.copy_from_slice(&info_raw[32..48]);
                n
            };
            let info = crate::exec::ExecInfo {
                allow_setuid: i32::from_le_bytes(info_raw[56..60].try_into().unwrap()) != 0,
                new_uid: u32::from_le_bytes(info_raw[48..52].try_into().unwrap()),
                new_gid: u32::from_le_bytes(info_raw[52..56].try_into().unwrap()),
                progname,
                stack_high: VirBytes(le_u64(&info_raw[184..192])),
                frame_len: le_u64(&info_raw[24..32]) as usize,
            };
            match crate::exec::do_newexec(table, msg.m_source, Endpoint(endpoint_raw), info) {
                Ok(allow) => {
                    // C :120-122 — reply payload suid@0.
                    let mut reply = Message::default();
                    let suid = i32::from(allow);
                    // SAFETY: a raw-byte write into m_pm_lexec_exec_new.suid@0;
                    // forming an index slice is a read borrow of a union field, so
                    // it needs an unsafe block.
                    unsafe {
                        reply.m_u.raw[0..4].copy_from_slice(&suid.to_le_bytes());
                    }
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_execrestart (pm/exec.c:130-151) — RS only; the RS gate lives
        // inside do_execrestart. The service wiring follows the vfs.rs
        // ExecServices precedent.
        PmCall::ExecRestart => {
            let (endpt, result, pc, ps_str) = super::decode::exec_restart(msg);
            let info = crate::exec::ExecRestartInfo {
                endpoint: Endpoint(endpt),
                result,
                pc: VirBytes(pc),
                ps_str: VirBytes(ps_str),
            };
            let mut svc = crate::ipc::vfs::ExecServices { transport, kern };
            match crate::exec::do_execrestart(table, msg.m_source, info, &mut svc) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // ===== Time family, six calls (time.c) =====

        // C: do_stime (time.c:110-131) — SUPER_USER gate + boottime anchor reset.
        PmCall::Stime => {
            let (_clk, _now, sec, _nsec) = super::decode::time(msg);
            let src = crate::time::SysClockSource;
            let mut boot = crate::time::SysBootTimeCtl;
            match crate::time::do_stime(
                table,
                caller,
                sec as i64,
                timers.system_hz,
                &src,
                &mut boot,
            ) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_gettime (time.c:22-47) — clk dispatches to REALTIME/MONOTONIC,
        // and after decomposition the reply is pre-filled with
        // m_lc_pm_time{sec,nsec}.
        PmCall::GetTimeOfDay | PmCall::ClockGetTime => {
            let (clk, _now, _sec, _nsec) = super::decode::time(msg);
            let clk = match crate::time::ClockId::try_from(clk) {
                Ok(c) => c,
                Err(e) => return ReplyIntent::Reply(-e.to_errno()),
            };
            let src = crate::time::SysClockSource;
            match crate::time::do_gettime(&src, clk, timers.system_hz) {
                Ok(ts) => {
                    let mut reply = minix_types::Message::default();
                    // SAFETY: the reply overlay is C `mess_pm_lc_time`
                    // (ipc.h:1769-1773: sec@0/nsec@8, LP64 natural width) —
                    // C do_gettime writes mp_reply.m_pm_lc_time (time.c:43-46) and
                    // the libc client reads the same overlay (gettimeofday.c:18-20);
                    // it is not the request arm m_lc_pm_time (where nsec sits at
                    // @16).
                    unsafe {
                        reply.m_u.raw[0..8].copy_from_slice(&(ts.sec as u64).to_ne_bytes());
                        reply.m_u.raw[8..16].copy_from_slice(&ts.nsec.to_ne_bytes());
                    }
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_clock_getres — 1e9/hz (the same value for REALTIME/MONOTONIC).
        PmCall::ClockGetRes => {
            let (clk, _now, _sec, _nsec) = super::decode::time(msg);
            let clk = match crate::time::ClockId::try_from(clk) {
                Ok(c) => c,
                Err(e) => return ReplyIntent::Reply(-e.to_errno()),
            };
            match crate::time::do_getres(clk, timers.system_hz) {
                Ok(ts) => {
                    let mut reply = minix_types::Message::default();
                    // SAFETY: the reply overlay is the same as the GetTimeOfDay
                    // arm — C `mess_pm_lc_time` (ipc.h:1769-1773) sec@0/nsec@8.
                    unsafe {
                        reply.m_u.raw[0..8].copy_from_slice(&(ts.sec as u64).to_ne_bytes());
                        reply.m_u.raw[8..16].copy_from_slice(&ts.nsec.to_ne_bytes());
                    }
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_clock_settime (time.c:71-88) — SUPER_USER gate + MONOTONIC is
        // immutable.
        PmCall::ClockSetTime => {
            let (clk, now, sec, nsec) = super::decode::time(msg);
            let clk = match crate::time::ClockId::try_from(clk) {
                Ok(c) => c,
                Err(e) => return ReplyIntent::Reply(-e.to_errno()),
            };
            let mut ctl = crate::time::SysSetTimeCtl;
            match crate::time::do_settime(
                table,
                caller,
                crate::time::TimeRequest {
                    clk,
                    now,
                    sec: sec as i64,
                    nsec,
                },
                &mut ctl,
            ) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // ===== mcontext family, two calls (mcontext.c) =====
        // C do_getmcontext (mcontext.c:13-22): pass through sys_getmcontext,
        // collapsing errors to EINVAL (MiscError::Inval in do_getmcontext).
        PmCall::GetMContext => {
            let (endpt, ctx) = super::decode::mcontext(msg);
            let ctl = crate::misc::SysMcontextCtl;
            match crate::misc::do_getmcontext(&ctl, Endpoint(endpt), VirBytes(ctx)) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        PmCall::SetMContext => {
            let (endpt, ctx) = super::decode::mcontext(msg);
            let ctl = crate::misc::SysMcontextCtl;
            match crate::misc::do_setmcontext(&ctl, Endpoint(endpt), VirBytes(ctx)) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_getprocnr (misc.c:149-164) — RS asks for the endpoint of a pid;
        // the reply payload is m_pm_lsys_getprocnr.endpt.
        PmCall::GetProcNr => {
            let pid = super::decode::getprocnr(msg);
            match crate::misc::do_getprocnr(table, msg.m_source, pid) {
                Ok(ep) => {
                    let mut reply = minix_types::Message::default();
                    // SAFETY: the reply arm is endpt@0.
                    unsafe {
                        reply.m_u.raw[0..4].copy_from_slice(&ep.0.to_le_bytes());
                    }
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_getepinfo (misc.c:169-193) — a credential snapshot plus the
        // groups copied out through the copy seam; pid is the taskcall return
        // value.
        PmCall::GetEpInfo => {
            let (ep, groups_ptr, ngroups) = super::decode::getepinfo(msg);
            let mut cpy = KernCopyToUser { kern, who: msg.m_source };
            match crate::misc::do_getepinfo(
                table,
                Endpoint(ep),
                ngroups as usize,
                &mut cpy,
            ) {
                Ok(info) => {
                    let mut reply = minix_types::Message::default();
                    // SAFETY: the reply arm is m_pm_lsys_getepinfo (uid@0/euid@4/
                    // gid@8/egid@12/ngroups@16, C ipc.h:517-524).
                    let arm = unsafe { &mut reply.m_u.m_pm_lsys_getepinfo };
                    arm.uid = info.uid as i32;
                    arm.euid = info.euid as i32;
                    arm.gid = info.gid as i32;
                    arm.egid = info.egid as i32;
                    arm.ngroups = info.ngroups as i32;
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(info.pid)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_sysuname (misc.c:60-96) — the read-only half of the uname
        // namespace; bytes are copied to the caller's value buffer through the
        // copy seam, and the number of bytes copied is returned.
        PmCall::SysUname => {
            let (req, field, len, value) = super::decode::sysuname(msg);
            let mut cpy = KernCopyToUser { kern, who: msg.m_source };
            match crate::misc::do_sysuname(
                field as usize,
                req,
                len as usize,
                msg.m_source,
                &mut cpy,
            ) {
                Ok(n) => ReplyIntent::Reply(n as i32),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_sprofile (profile.c:22-45) — gated on the sprofile feature;
        // SprofCtl delegates straight to sys_sprof in production.
        PmCall::SProf => {
            let (action, freq, intr_type, ctl_ptr, mem_ptr, mem_size) =
                super::decode::sprof(msg);
            let ep = table.procs[caller.get()].endpoint();
            let mut ctl = crate::misc::SysSprofCtl { ep };
            match crate::misc::do_sprofile(
                action,
                mem_size as usize,
                freq as u32,
                intr_type,
                ep,
                VirBytes(ctl_ptr),
                VirBytes(mem_ptr),
                &mut ctl,
            ) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_reboot (misc.c:198-231) — EPERM gate → abort_flag →
        // RB_POWERDOWN readclock leg → check_sig(-1, SIGKILL) broadcast →
        // sys_stop(INIT) → tell VFS VFS_PM_REBOOT → SUSPEND without replying
        // (the reply goes through the VFS_PM_REBOOT_REPLY special case into
        // sys_abort(abort_flag), the reboot-reply arm of ipc/vfs.rs
        // handle_vfs_reply).
        PmCall::Reboot => {
            let how = super::decode::reboot(msg);
            let mut ctl = SysRebootCtl {
                abort_flag: &mut *timers.abort_flag,
                kern,
                transport,
            };
            match crate::misc::do_reboot(table, caller, how, &mut ctl) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_svrctl (misc.c:302-390) — IOCGROUP gate ('P'/'M') plus a 32-byte
        // sysgetenv copy-in (key/val pointers and lengths); SET stores the value
        // in the table, GET copies val back (misc.c:359-384).
        PmCall::SvrCtl => {
            let (request, arg) = super::decode::svrctl(msg);
            // C misc.c:311-312 — the IOCGROUP gate ('P'=0x50 / 'M'=0x4D).
            let group = ((request >> 8) & 0xff) as u8;
            if group != b'P' && group != b'M' {
                return ReplyIntent::Reply(EINVAL);
            }
            // A non-PARAM request code: PM has no branch for it → EINVAL (C's
            // switch default).
            let is_param = matches!(
                request as u32,
                x if x == crate::misc::PMGETPARAM as u32
                    || x == crate::misc::PMSETPARAM as u32
                    || x == crate::misc::OPMGETPARAM as u32
                    || x == crate::misc::OPMSETPARAM as u32
            );
            if !is_param {
                return ReplyIntent::Reply(EINVAL);
            }
            // struct sysgetenv {key pointer, val pointer, keylen, vallen} copied
            // in as 32 bytes.
            let mut env = [0u8; 32];
            if let Err(e) = kern.copy_from_user(msg.m_source, arg, &mut env) {
                return ReplyIntent::Reply(e);
            }
            let u64_at = |o: usize| u64::from_le_bytes(env[o..o + 8].try_into().unwrap());
            let (key_ptr, val_ptr, keylen, vallen) =
                (u64_at(0), u64_at(8), u64_at(16) as usize, u64_at(24) as usize);

            // Copy in the key string (search_key[64] bound, misc.c:313-316).
            let mut key_buf = [0u8; 64];
            let key_len = keylen.min(64);
            if key_len > 0
                && let Err(e) = kern.copy_from_user(msg.m_source, key_ptr, &mut key_buf[..key_len])
            {
                return ReplyIntent::Reply(e);
            }
            let key = String::from_utf8_lossy(&key_buf[..key_len]).into_owned();
            let key_opt = if key.is_empty() { None } else { Some(key) };

            match crate::misc::do_svrctl(
                timers.svrctl_store,
                request as i32,
                key_opt,
                None,
                vallen,
            ) {
                Ok(val) => {
                    // C misc.c:379-384 — GET: copy val back to the caller's
                    // buffer, m1i2 = vallen.
                    if let Some(v) = &val
                        && let Err(e) = kern.copy_to_user(
                            v.as_bytes(),
                            msg.m_source,
                            val_ptr,
                        )
                    {
                        return ReplyIntent::Reply(e);
                    }
                    let mut reply = minix_types::Message::default();
                    reply.m_u.m_m1.m1i2 = vallen as i32;
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_getsysinfo (misc.c:108-144) — SUPER_USER gate, SI_* classification,
        // whole tables and counts delivered through the copy seam.
        PmCall::GetSysInfo => {
            let (what, where_, size) = super::decode::getsysinfo(msg);
            struct StatsCtl<'a> {
                stats: &'a [u8],
            }
            impl crate::misc::SysInfoCtl for StatsCtl<'_> {
                fn proc_tab(&self) -> &[u8] {
                    // ProcTab goes through do_getsysinfo's per-slot cpy path, not
                    // through the ctl.
                    &[]
                }
                // call_stats is gated the same way as the trait: without the
                // feature, an SI_CALL_STATS request is already refused where
                // SysInfoWhat is classified.
                #[cfg(feature = "syscall_stats")]
                fn call_stats(&self) -> &[u8] {
                    self.stats
                }
            }
            let ctl = StatsCtl {
                stats: timers.call_stats,
            };
            let mut cpy = KernCopyToUser { kern, who: msg.m_source };
            let what = match crate::misc::SysInfoWhat::try_from(what) {
                Ok(w) => w,
                Err(e) => return ReplyIntent::Reply(-e.to_errno()),
            };
            match crate::misc::do_getsysinfo(
                table,
                caller,
                what,
                size as usize,
                VirBytes(where_),
                &ctl,
                &mut cpy,
            ) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
        // C: do_getrusage (misc.c:400-447) — the who 0/-1 gate: SysTimesVmCtl
        // has both legs (SYS_TIMES + a VM_GETRUSAGE taskcall), and the complete
        // 128-byte struct rusage is copied out through the copy seam.
        PmCall::GetRUsage => {
            let (who_raw, addr) = super::decode::rusage(msg);
            let who = match crate::misc::RusageWho::try_from(who_raw) {
                Ok(w) => w,
                Err(e) => return ReplyIntent::Reply(-e.to_errno()),
            };
            let mut ctl = crate::misc::SysTimesVmCtl { transport: &mut *transport };
            let mut cpy = KernCopyToUser { kern, who: msg.m_source };
            match crate::misc::do_getrusage(
                table,
                caller,
                who,
                VirBytes(addr),
                timers.system_hz,
                &mut ctl,
                &mut cpy,
            ) {
                Ok(_) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(-e.to_errno()),
            }
        }
    }
}

use crate::signal_handlers::{
    handle_sigaction, handle_sigpending, handle_sigprocmask, handle_sigreturn,
    handle_sigsuspend, SigActionReq,
};
use crate::mproc::SigMsg;

/// Copy-out adapter for getepinfo's group table: the caller's group buffer takes
/// u32 little-endian elements.
struct KernCopyToUser<'a> {
    kern: &'a mut dyn crate::exit::KernelGateway,
    who: Endpoint,
}

impl crate::misc::CopyToUser for KernCopyToUser<'_> {
    fn copy_to_user(&mut self, bytes: &[u8], ptr: VirBytes) -> Result<(), MiscError> {
        self.kern
            .copy_to_user(bytes, self.who, ptr.0)
            .map_err(|_| MiscError::Fault)
    }

    fn copy_from_user(&mut self, _src: VirBytes, _dst: &mut [u8]) -> Result<(), MiscError> {
        Err(MiscError::Fault)
    }
}

/// The no-copy group exit: calls that do not move a group table (GetPid/GetUid
/// and friends) still have to satisfy the do_get/do_set trait shape; the copy
/// path fails honestly (unreachable).
struct NoopGroups;
impl CopyGroups for NoopGroups {
    fn copy_to_user(&mut self, _gids: &[Gid], _ptr: VirBytes) -> Result<(), SetError> {
        Err(SetError::Fault)
    }
    fn copy_from_user(&mut self, _ptr: VirBytes, _ngroups: usize) -> Result<Vec<Gid>, SetError> {
        Err(SetError::Fault)
    }
}

/// The credential family's group-table copy adapter: the `CopyGroups` semantics
/// land on the gateway copy seam, moving gids element by element as u32
/// little-endian (`NGROUPS_MAX` slots, as documented in credentials.rs).
struct SysCopyGroups<'a> {
    kern: &'a mut dyn crate::exit::KernelGateway,
    who: Endpoint,
}

impl CopyGroups for SysCopyGroups<'_> {
    fn copy_to_user(&mut self, gids: &[Gid], ptr: VirBytes) -> Result<(), SetError> {
        let mut raw = alloc::vec![0u8; gids.len() * 4];
        for (i, g) in gids.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        self.kern
            .copy_to_user(&raw, self.who, ptr.0)
            .map_err(|_| SetError::Fault)
    }

    fn copy_from_user(&mut self, ptr: VirBytes, ngroups: usize) -> Result<Vec<Gid>, SetError> {
        let mut raw = alloc::vec![0u8; ngroups * 4];
        self.kern
            .copy_from_user(self.who, ptr.0, &mut raw)
            .map_err(|_| SetError::Fault)?;
        Ok(raw
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect())
    }
}

/// The exec arm's VFS forwarding adapter: encode the five VFS_PM_EXEC fields and
/// run the three tell_vfs steps (exec.c:44-54).
struct SysVfsExec<'a, T: IpcTransport + ?Sized> {
    transport: &'a mut T,
}

impl<T: IpcTransport + ?Sized> crate::exec::VfsExec for SysVfsExec<'_, T> {
    fn forward_exec(
        &mut self,
        table: &mut ProcTable,
        caller: UserSlot,
        req: crate::exec::ExecRequest,
    ) -> Result<ReplyIntent, crate::exec::ExecError> {
        let call = minix_types::VfsCall::Exec {
            endpoint: req.endpoint,
            path: req.path.0,
            path_len: req.path_len as i32,
            frame: req.frame.0,
            frame_len: req.frame_len as i32,
            ps_str: req.ps_str.0,
        };
        crate::ipc::vfs::tell_vfs(table, caller, call, self.transport);
        Ok(ReplyIntent::ReplyLater)
    }
}

/// Byte widths of the `struct exec_info` fields PM consumes (libexec.h:21-58,
/// LP64): proc_e@0/hdr@8/hdr_len@16/frame_len@24/progname@32 (16)/new_uid@48/
/// new_gid@52/allow_setuid@56/…/stack_high@184; the copy-in size is 192.
const EXEC_INFO_COPY_SIZE: usize = 192;

/// The Reboot arm's production ports: abort_flag written straight into the
/// PmServer field (C's `abort_flag` in glo.h:26 is the counterpart), kernel legs
/// through KernelGateway, and the VFS notification through IpcTransport (each leg
/// and its C counterpart is described in the `misc.rs::RebootCtl` documentation).
struct SysRebootCtl<'a, T: IpcTransport + ?Sized> {
    abort_flag: &'a mut i32,
    kern: &'a mut dyn crate::exit::KernelGateway,
    transport: &'a mut T,
}

impl<T: IpcTransport + ?Sized> crate::misc::RebootCtl for SysRebootCtl<'_, T> {
    fn set_abort(&mut self, how: i32) {
        *self.abort_flag = how;
    }

    fn try_power_off(&mut self) -> bool {
        // C misc.c:207-214: only after the DS lookup of "readclock.drv" succeeds
        // is the power-off `_taskcall` made; when the lookup fails the step is
        // skipped silently ("nothing we can do if it fails"). PM has no
        // DS label→endpoint client yet, and the current boot envelope carries no
        // readclock driver at all (C would take the same skip branch, so the
        // external behavior matches); the power-off leg stays honest by not
        // requesting it, and will be wired in once the driver and the DS client
        // exist.
        false
    }

    fn broadcast_kill(&mut self, table: &mut ProcTable, caller: UserSlot) {
        // C misc.c:221: check_sig(-1, SIGKILL, FALSE) ("kill all users except
        // init"), return value unchecked. The broadcaster's skip of PRIV_PROC
        // lives inside check_sig (mirroring signal.c:607-608).
        //[SD19-gate] PM send/notify/reply fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::check_sig(
            table,
            caller,
            -1,
            crate::signal::SIGKILL,
            false,
            self.kern,
            self.transport,
        );
    }

    fn stop_init(&mut self, table: &ProcTable) {
        // C misc.c:222: sys_stop(INIT_PROC_NR), return value unchecked. The
        // production KernelGateway::sys_delay_stop is an honest placeholder until
        // the E6 wrapper lands (-EIO, exit.rs), and staying silent about the
        // failure matches C; the window where "init is not yet frozen by the
        // kernel" heals once the E6 wrapper exists (recorded in the fix log).
        if let Ok(slot) = table.pm_isokendpt(Endpoint::INIT) {
            //[SD19-gate] PM send/notify/reply fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = self.kern.sys_delay_stop(table.procs[slot.get()].endpoint());
        }
    }

    fn tell_reboot(&mut self, table: &mut ProcTable) -> i32 {
        // C misc.c:228: tell_vfs(&mproc[VFS_PROC_NR], &m) — Reboot is not tied to
        // a user process, so the VFS slot carries the VFS_CALL (see the
        // ipc/vfs.rs tell_vfs documentation). When VFS is not in the table (dead)
        // neither a replier nor the abort delivery is possible, so skipping the
        // notification has the same observable result as C writing a fixed slot.
        if let Ok(slot) = table.pm_isokendpt(Endpoint::VFS) {
            crate::ipc::vfs::tell_vfs(table, slot, minix_types::VfsCall::Reboot, self.transport);
        }
        0
    }
}

/// The credential family's VFS forwarding adapter: encodes a VfsCall and then
/// runs tell_vfs's three steps (not-idle assertion / send / set VFS_CALL —
/// vfs.rs:166-175).
struct SysVfsForward<'a, T: IpcTransport + ?Sized> {
    transport: &'a mut T,
}

impl<T: IpcTransport + ?Sized> VfsForwarder for SysVfsForward<'_, T> {
    fn forward_set(
        &mut self,
        table: &mut ProcTable,
        slot: UserSlot,
        ep: Endpoint,
        op: &SetOp,
    ) -> Result<ReplyIntent, SetError> {
        let call = match op {
            SetOp::SetUid(uid) | SetOp::SetEUid(uid) => minix_types::VfsCall::SetUid {
                endpoint: ep,
                eid: *uid as i32,
                rid: *uid as i32,
            },
            SetOp::SetGid(gid) | SetOp::SetEGid(gid) => minix_types::VfsCall::SetGid {
                endpoint: ep,
                eid: *gid as i32,
                rid: *gid as i32,
            },
            SetOp::SetGroups { gids } => minix_types::VfsCall::SetGroups {
                endpoint: ep,
                group_no: gids.len() as i32,
                group_addr: 0,
            },
            SetOp::SetSid => minix_types::VfsCall::SetSid { endpoint: ep },
        };
        crate::ipc::vfs::tell_vfs(table, slot, call, self.transport);
        Ok(ReplyIntent::ReplyLater)
    }
}

/// `do_get` result → reply intent (C getset.c:52-81's two-value payload
/// semantics). Uid/Gid's real/eff pair is delivered by pre-filling the reply slots
/// (m1i1/m1i2).
fn get_result_intent(table: &mut ProcTable, caller: UserSlot, r: GetResult) -> ReplyIntent {
    let mut prefill = |real: i32, eff: i32| {
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = real;
        msg.m_u.m_m1.m1i2 = eff;
        table.procs[caller.get()].ipc.reply = Some(msg);
    };
    match r {
        GetResult::Uid { real, eff } => {
            prefill(real as i32, eff as i32);
            ReplyIntent::Reply(0)
        }
        GetResult::Gid { real, eff } => {
            prefill(real as i32, eff as i32);
            ReplyIntent::Reply(0)
        }
        GetResult::Groups { count } => ReplyIntent::Reply(count as i32),
        GetResult::Pid { self_pid, parent } => {
            prefill(self_pid, parent);
            ReplyIntent::Reply(0)
        }
        GetResult::Pgrp(p) | GetResult::Sid(p) => ReplyIntent::Reply(p),
        GetResult::Issetugid(t) => ReplyIntent::Reply(t as i32),
        GetResult::Error(e) => ReplyIntent::Reply(e),
    }
}

/// The sigreturn arm's gateway adapter: `handle_sigreturn` consumes only
/// `KernelSig::sigreturn` (C do_sigreturn does not call sys_sigsend), so sigsend
/// fails honestly inside this adapter.
struct SigReturnKern<'a>(&'a mut dyn crate::exit::KernelGateway);

impl crate::signal_handlers::KernelSig for SigReturnKern<'_> {
    fn sigsend(&mut self, _ep: Endpoint, _msg: &SigMsg) -> Result<(), i32> {
        Err(ENOSYS)
    }
    fn sigreturn(&mut self, ep: Endpoint, ctx: VirBytes) -> Result<(), i32> {
        self.0.sys_sigreturn(ep, ctx)
    }
}

/// From the gateway/wrapper negative-errno convention → the reply's positive
/// errno (error fidelity: take the absolute value).
#[cfg(test)]
fn leak_timers() -> crate::timer::TimerFaces<'static> {
    let tctl: &'static mut dyn crate::timer::TimerCtl = Box::leak(Box::new(crate::timer::SysTimerCtl::new()));
    let vctl: &'static mut dyn crate::timer::VTimerCtl = Box::leak(Box::new(crate::timer::SysVTimerCtl));
    let svrctl: &'static mut crate::misc::ParamStore =
        Box::leak(Box::new(crate::misc::ParamStore::new(alloc::string::String::new())));
    let call_stats: &'static mut [u8] = Box::leak(Box::new([0u8; 192]));
    let abort_flag: &'static mut i32 = Box::leak(Box::new(0));
    crate::timer::TimerFaces {
        tctl,
        vctl,
        system_hz: 100,
        svrctl_store: svrctl,
        call_stats,
        abort_flag,
    }
}



#[cfg(test)]
mod tests {
    /// `setpriority`'s dispatch-level wiring: after SET passes the permission
    /// gate it sends `SCHEDULING_SET_NICE` (two fields, endpoint/maxprio) to the
    /// process's scheduler through the production `MinixSchedCtl`, and the user
    /// side gets 0.
    #[test]
    fn test_setpriority_sends_set_nice_to_scheduler() {
        use minix_types::{Endpoint, Message};
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        // The scheduler is set to a user-space scheduler (not KERNEL/NONE — those
        // would be skipped).
        let sched = Endpoint::from_generation_slot(0, 4);
        table.procs[3].resources.scheduler = sched;
        let mut msg = Message { m_type: 27, ..Message::default() };
        msg.m_source = ep;
        // m1i1=which (PRIO_PROCESS=0), m1i2=who (0=itself), m1i3=prio (5).
        // SAFETY(test): the three m1 payload integers are written in misc.c:244's
        // field order.
        unsafe {
            let raw = &mut msg.m_u.raw;
            raw[0..4].copy_from_slice(&0i32.to_le_bytes());
            raw[4..8].copy_from_slice(&0i32.to_le_bytes());
            raw[8..12].copy_from_slice(&5i32.to_le_bytes());
        }
        let intent = dispatch_pm_call(
            PmCall::SetPriority,
            &mut table,
            &mut events,
            &mut transport,
            &mut NoopKernel,
            &mut leak_timers(),
            UserSlot::new(3),
            &msg,
        );
        assert_eq!(intent, ReplyIntent::Reply(0), "SET 成功回 0");
        // The production ctl sent SCHEDULING_SET_NICE over sendrec (two fields).
        let hit = transport
            .sent()
            .iter()
            .find(|(e, m)| *e == sched && m.m_type == minix_types::SCHEDULING_SET_NICE)
            .expect("SET 要向调度器发 SET_NICE");
        // SAFETY(test): endpoint@0, maxprio@4 (MessPmSchedSchedulingSetNice).
        let raw = unsafe { &hit.1.m_u.raw };
        assert_eq!(
            i32::from_le_bytes(raw[0..4].try_into().unwrap()),
            ep.0,
            "SET_NICE 带目标端点"
        );
    }

    use super::*;
    use crate::ipc::TestIpcTransport;
    use minix_types::Endpoint;

    /// Kernel gateway mock for tests (sys_kill/sys_clear always OK).
    #[derive(Default)]
    struct NoopKernel;
    impl crate::exit::KernelGateway for NoopKernel {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
    fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
            Ok(())
        }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn sys_sigreturn(&mut self, _ep: Endpoint, _ctx: VirBytes) -> Result<(), i32> { Ok(()) }
    }
        fn setup_with_caller(slot: usize, ep: Endpoint) -> (ProcTable, EventRegistry, TestIpcTransport) {
        let mut table = ProcTable::new();
        table.procs[slot].identity.endpoint = ep;
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].state.lifecycle = crate::mproc::Lifecycle::Running;
        (table, EventRegistry::new(), TestIpcTransport::new())
    }

    #[test]
    fn test_dispatch_fork_success_is_reply_later() {
        // C: do_fork returns SUSPEND (forkexit.c:139) — fork does not reply
        // synchronously; with the parent registered, do_fork reaches vm_fork (a
        // scripted OK reply) → ReplyLater.
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let _kern = NoopKernel;
        let mut vm_reply = Message { m_type: minix_types::OK, ..Message::default() };
        vm_reply.m_u.m_m1.m1i3 = Endpoint::from_generation_slot(2, 1).0;
        transport.queue_sendrec_reply(vm_reply);

        let mut msg = Message { m_type: 2, ..Message::default() };
        msg.m_source = ep;
        let intent = dispatch_pm_call(
            PmCall::Fork,
            &mut table,
            &mut events,
            &mut transport,
            &mut NoopKernel,
            &mut leak_timers(),
            UserSlot::new(3),
            &msg,
        );
        assert_eq!(intent, ReplyIntent::ReplyLater);
        // The fork path really sends: VM_FORK first (the sendrec recorded in
        // sent), VFS_PM_FORK after.
        assert_eq!(transport.sent()[0].0, Endpoint::VM);
        assert!(transport.sent().len() >= 2);
    }

    #[test]
    fn test_dispatch_fork_parent_unknown_is_error_reply() {
        // C: the synchronously-failing part of forkexit.c:59-79 — a parent that
        // does not exist yields an errno reply (the pm_isokendpt defense,
        // ESRCH), with no fictitious "silent ReplyLater" path.
        let (mut table, mut events, mut transport) =
            setup_with_caller(3, Endpoint::from_generation_slot(1, 3));
        let mut msg = Message { m_type: 2, ..Message::default() };
        msg.m_source = Endpoint::from_generation_slot(9, 9); // an unregistered parent
        let intent = dispatch_pm_call(
            PmCall::Fork,
            &mut table,
            &mut events,
            &mut transport,
            &mut NoopKernel,
            &mut leak_timers(),
            UserSlot::new(3),
            &msg,
        );
        assert_eq!(
            intent,
            ReplyIntent::Reply(-minix_types::PmError::InvalidEndpoint.to_errno())
        );
    }

    #[test]
    fn test_dispatch_exit_is_no_reply() {
        // C: do_exit returns SUSPEND and never replies (forkexit.c:246-266, the
        // NoReply subcase of plan.md section 7.3).
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let _kern = NoopKernel;
        let mut msg = Message { m_type: 1, ..Message::default() };
        msg.m_source = ep;
        let intent = dispatch_pm_call(
            PmCall::Exit,
            &mut table,
            &mut events,
            &mut transport,
            &mut NoopKernel,
            &mut leak_timers(),
            UserSlot::new(3),
            &msg,
        );
        assert_eq!(intent, ReplyIntent::NoReply);
    }

    /// Builds a sigset request payload (how@0/ctx@8/set@16, MessLcPmSigset).
    fn sigset_msg(m_type: i32, how: i32, set: u64) -> Message {
        let mut msg = Message { m_type, ..Message::default() };
        msg.m_u.m_lc_pm_sigset.how = how;
        msg.m_u.m_lc_pm_sigset.ctx = 0;
        msg.m_u.m_lc_pm_sigset.set = [set as u32, (set >> 32) as u32, 0, 0];
        msg
    }

    #[test]
    fn test_sigprocmask_unblock_redelivers_pending() {
        // C signal.c:131 — the SIG_UNBLOCK success branch ends with an inline
        // check_pending; the witness: after unblocking, the pending bit is
        // consumed (delivered by sig_proc).
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let usr1 = 30; // SIGUSR1(minix signal.h:82)
        let bit = 1u64 << (usr1 - 1);
        table.procs[3].resources.signals.pending |= bit;
        table.procs[3].resources.signals.mask |= bit; // block it first
        let mut msg = sigset_msg(23, crate::mproc::SIG_UNBLOCK, bit);
        msg.m_source = ep;
        assert_eq!(
            dispatch_pm_call(
                PmCall::SigProcMask,
                &mut table,
                &mut events,
                &mut transport,
                &mut NoopKernel,
                &mut leak_timers(),
                UserSlot::new(3),
                &msg
            ),
            ReplyIntent::Reply(0)
        );
        assert_eq!(
            table.procs[3].resources.signals.pending & bit,
            0,
            "check_pending 消费了 pending 位(S3 重投接线)"
        );
    }

    #[test]
    fn test_sigprocmask_block_keeps_pending() {
        // Control: Block has needs_check=false (it only adds mask bits and cannot
        // create a newly deliverable signal) — the pending bit stays; if the
        // implementation wrongly called check_pending, that pending signal
        // (unblocked) would be delivered and the bit cleared, which this test
        // catches.
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let usr1 = 30;
        let bit = 1u64 << (usr1 - 1);
        table.procs[3].resources.signals.pending |= bit; // not blocked
        let mut msg = sigset_msg(23, crate::mproc::SIG_BLOCK, 1 << 1); // block SIGINT, unrelated to usr1
        msg.m_source = ep;
        assert_eq!(
            dispatch_pm_call(
                PmCall::SigProcMask,
                &mut table,
                &mut events,
                &mut transport,
                &mut NoopKernel,
                &mut leak_timers(),
                UserSlot::new(3),
                &msg
            ),
            ReplyIntent::Reply(0)
        );
        assert_eq!(
            table.procs[3].resources.signals.pending & bit,
            bit,
            "Block 不重投(pending 保留)"
        );
    }

    #[test]
    fn test_sigreturn_redelivers_pending_unconditionally() {
        // C signal.c:190 — do_sigreturn calls check_pending unconditionally after
        // success; once the restored mask (empty here) unblocks it, the pending
        // bit is consumed.
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let usr1 = 30;
        let bit = 1u64 << (usr1 - 1);
        table.procs[3].resources.signals.pending |= bit;
        table.procs[3].resources.signals.mask |= bit;
        let mut msg = sigset_msg(24, 0, 0); // restored mask = empty set
        msg.m_source = ep;
        assert_eq!(
            dispatch_pm_call(
                PmCall::SigReturn,
                &mut table,
                &mut events,
                &mut transport,
                &mut NoopKernel,
                &mut leak_timers(),
                UserSlot::new(3),
                &msg
            ),
            ReplyIntent::Reply(0)
        );
        assert_eq!(
            table.procs[3].resources.signals.pending & bit,
            0,
            "sigreturn 后 check_pending 消费 pending 位"
        );
    }

    /// Kernel gateway that captures copy_to_user bytes (the witness for getrusage
    /// copy-out).
    struct CapKernel {
        copies: alloc::vec::Vec<(alloc::vec::Vec<u8>, Endpoint, u64)>,
    }
    impl crate::exit::KernelGateway for CapKernel {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
        fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
        fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
        fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, bytes: &[u8], dst_ep: Endpoint, dst_addr: u64) -> Result<(), i32> {
            self.copies.push((bytes.to_vec(), dst_ep, dst_addr));
            Ok(())
        }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn sys_sigreturn(&mut self, _ep: Endpoint, _ctx: VirBytes) -> Result<(), i32> { Ok(()) }
    }

    /// Builds a VM_GETRUSAGE reply (m1p1=max_rss_kb/m1i1=minor/m1i2=major, the
    /// mirror of VM encode.rs:258-268).
    fn vm_rusage_reply(max_rss_kb: u64, minor: i32, major: i32, m_type: i32) -> Message {
        let mut reply = Message::default();
        reply.m_type = m_type;
        // SAFETY: test construction — m1 is written in the field order of VM's
        // reply encoding.
        let m1 = unsafe { &mut reply.m_u.m_m1 };
        m1.m1p1 = max_rss_kb;
        m1.m1i1 = minor;
        m1.m1i2 = major;
        reply
    }

    #[test]
    fn test_dispatch_getrusage_children_roundtrip() {
        // The whole RUSAGE_CHILDREN(-1) chain: the CPU buckets come from
        // child_utime/stime (not the SYS_TIMES leg), the VM leg answers the three
        // fields by value, and 128 bytes land in the user buffer through the copy
        // seam.
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        table.procs[3].resources.child_utime = 200;
        table.procs[3].resources.child_stime = 100;
        transport.queue_sendrec_reply(vm_rusage_reply(4096, 33, 4, minix_types::OK));
        let mut kern = CapKernel { copies: alloc::vec::Vec::new() };
        let mut msg = Message { m_type: 36, ..Message::default() };
        msg.m_source = ep;
        {
            // SAFETY: test construction — the request is written in ipc.h:510-515's
            // field order.
            let raw = unsafe { &mut msg.m_u.raw };
            raw[0..4].copy_from_slice(&(-1i32).to_le_bytes());
            raw[8..16].copy_from_slice(&0x3000u64.to_le_bytes());
        }
        assert_eq!(
            dispatch_pm_call(
                PmCall::GetRUsage,
                &mut table,
                &mut events,
                &mut transport,
                &mut kern,
                &mut leak_timers(),
                UserSlot::new(3),
                &msg
            ),
            ReplyIntent::Reply(0)
        );
        // The VM leg: the request goes to VM, endpt=caller, children=1.
        assert_eq!(transport.sent().len(), 1);
        let (dst, req) = &transport.sent()[0];
        assert_eq!(*dst, Endpoint::VM);
        assert_eq!(req.m_type, minix_types::VM_GETRUSAGE as i32);
        // SAFETY: the assertion side reads in encoding field order.
        let req_pl = unsafe { &req.m_u.m_lsys_vm_rusage };
        assert_eq!(req_pl.endpt, ep.0);
        assert_eq!(req_pl.children, 1);
        // The copy-out face: 128 bytes of rusage, utime=2s (200/100), and
        // maxrss/minflt/majflt taking VM's three values (sys/resource.h:57-76
        // layout).
        assert_eq!(kern.copies.len(), 1);
        let (bytes, copy_ep, addr) = &kern.copies[0];
        assert_eq!(*copy_ep, ep);
        assert_eq!(*addr, 0x3000);
        assert_eq!(bytes.len(), 128);
        let i64_at = |off: usize| i64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
        assert_eq!(i64_at(0), 2); // ru_utime.tv_sec
        assert_eq!(i64_at(16), 1); // ru_stime.tv_sec
        assert_eq!(i64_at(32), 4096);
        assert_eq!(i64_at(64), 33);
        assert_eq!(i64_at(72), 4);
    }

    #[test]
    fn test_dispatch_clockgetres_reply_lands_on_c_lane() {
        // C's reply overlay is m_pm_lc_time (ipc.h:1769-1773, LP64
        // sec@0/nsec@8) — the reply's nsec lands at @8, not in the request arm
        // m_lc_pm_time's @16 slot. The ClockGetRes arm is chosen for the lane pin:
        // do_getres is a pure 1e9/hz computation (mirroring time.c:60), has no
        // kernel clock leg, and is deterministically assertable on a host. The
        // GetTimeOfDay arm encodes the same shape (SysClockSource hardcodes the
        // kernel leg and is unreachable from a host unit test — the injection-seam
        // gap is recorded in the fix log).
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let mut kern = CapKernel { copies: alloc::vec::Vec::new() };
        let mut msg = Message { m_type: 33, ..Message::default() };
        msg.m_source = ep;
        {
            // SAFETY: test construction — clk_id@8 written on the m_lc_pm_time lane.
            let raw = unsafe { &mut msg.m_u.raw };
            raw[8..12].copy_from_slice(&0i32.to_le_bytes()); // CLOCK_REALTIME
        }
        assert_eq!(
            dispatch_pm_call(
                PmCall::ClockGetRes,
                &mut table,
                &mut events,
                &mut transport,
                &mut kern,
                &mut leak_timers(),
                UserSlot::new(3),
                &msg
            ),
            ReplyIntent::Reply(0)
        );
        let reply = table.procs[3].ipc.reply.take().expect("应答已预填");
        // SAFETY: read bytes along the reply overlay lane.
        let raw = unsafe { &reply.m_u.raw };
        let sec = u64::from_ne_bytes(raw[0..8].try_into().unwrap());
        let nsec = i64::from_ne_bytes(raw[8..16].try_into().unwrap());
        assert_eq!(sec, 0);
        assert_eq!(nsec, 1_000_000_000 / 100); // hz=100 → 10ms resolution
        assert_eq!(raw[16..24], [0u8; 8], "旧 @16 槽不再被写");
    }

    #[test]
    fn test_dispatch_getrusage_invalid_who_is_einval() {
        // C misc.c:407-409 — who ∉ {0,-1} → EINVAL, and no VM leg is sent.
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let mut kern = CapKernel { copies: alloc::vec::Vec::new() };
        let mut msg = Message { m_type: 36, ..Message::default() };
        msg.m_source = ep;
        {
            // SAFETY: as above — who is written in decoding field order.
            let raw = unsafe { &mut msg.m_u.raw };
            raw[0..4].copy_from_slice(&5i32.to_le_bytes());
        }
        assert_eq!(
            dispatch_pm_call(
                PmCall::GetRUsage,
                &mut table,
                &mut events,
                &mut transport,
                &mut kern,
                &mut leak_timers(),
                UserSlot::new(3),
                &msg
            ),
            // Error replies are m_type<0 = −errno (the C _syscall client
            // contract, the same as the fork arm).
            ReplyIntent::Reply(-EINVAL)
        );
        assert!(transport.sent().is_empty());
        assert!(kern.copies.is_empty());
    }

    #[test]
    fn test_dispatch_getrusage_vm_error_passthrough() {
        // C misc.c:441-442 — a non-OK vm_getrusage is passed through unchanged
        // (a VM error reply carries a positive errno in m_type).
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        transport.queue_sendrec_reply(vm_rusage_reply(0, 0, 0, minix_types::ESRCH));
        let mut kern = CapKernel { copies: alloc::vec::Vec::new() };
        let mut msg = Message { m_type: 36, ..Message::default() };
        msg.m_source = ep;
        {
            // SAFETY: as above.
            let raw = unsafe { &mut msg.m_u.raw };
            raw[0..4].copy_from_slice(&(-1i32).to_le_bytes());
        }
        assert_eq!(
            dispatch_pm_call(
                PmCall::GetRUsage,
                &mut table,
                &mut events,
                &mut transport,
                &mut kern,
                &mut leak_timers(),
                UserSlot::new(3),
                &msg
            ),
            ReplyIntent::Reply(-minix_types::ESRCH)
        );
        // A failure writes nothing into the user buffer (C's datacopy at
        // misc.c:446 is unreachable).
        assert!(kern.copies.is_empty());
    }
}
