//! Ptrace: `do_trace` + `trace_stop`.
//!
//! C ground truth: `minix3/minix/servers/pm/trace.c` (276 lines).
//! The full `T_*` constant set = `sys/sys/ptrace.h:226-250` (PT_* aliases)
//! + `:238-250` (Minix-specific commands).  `T_OK/T_ATTACH/T_EXIT/T_SETOPT`
//! are handled entirely inside PM; `T_RESUME/T_STEP/T_SYSCALL/T_DETACH` are
//! half local and half kernel (after the `break` they fall through to
//! `sys_trace`, trace.c:244-249); the remaining commands are passed straight
//! through.
//! Wire = `MessLcPmPtrace`/`MessPmLcPtrace` (minix-types).
//! Single-threaded — `&mut ProcTable` without `Arc`.

use crate::ipc::ReplyIntent;
use crate::mproc::{ProcTable, Guardianship, TraceOptions, WaitTarget};
use minix_types::{Endpoint, UserSlot, Pid, EINVAL, EPERM, EBUSY, ESRCH, OK};
use minix_types::Message;

use core::mem::size_of;

/// The full `T_*` command set (`sys/sys/ptrace.h:226-250`; the values are the
/// PT_* aliases plus the Minix-specific range).  **Reconciled value by value
/// against C** (exhaustively asserted by `test_constants_match_c`).
pub const T_OK: i32 = 0; // PT_TRACE_ME
pub const T_GETINS: i32 = 1; // PT_READ_I
pub const T_GETDATA: i32 = 2; // PT_READ_D
pub const T_SETINS: i32 = 4; // PT_WRITE_I
pub const T_SETDATA: i32 = 5; // PT_WRITE_D
pub const T_RESUME: i32 = 7; // PT_CONTINUE
pub const T_EXIT: i32 = 8; // PT_KILL
pub const T_ATTACH: i32 = 9; // PT_ATTACH
pub const T_DETACH: i32 = 10; // PT_DETACH
pub const T_SYSCALL: i32 = 14; // PT_SYSCALL
pub const T_STOP: i32 = -1;
pub const T_READB_INS: i32 = 100;
pub const T_WRITEB_INS: i32 = 101;
pub const T_GETUSER: i32 = 102;
pub const T_SETUSER: i32 = 103;
pub const T_STEP: i32 = 104;
pub const T_SETOPT: i32 = 105;
pub const T_GETRANGE: i32 = 106;
pub const T_SETRANGE: i32 = 107;

/// C `mproc.h:100` `TRACE_EXIT` (a state bit of `mp_trace_flags`, not a TO_*
/// option bit).  C keeps it in the same word as the options (reachable through
/// T_SETOPT's whole-word assignment); Rust splits the options (`TraceOptions`)
/// from the state (`TraceState.exit_pending`), so this constant is used to
/// extract the state bit from that word on T_SETOPT.
pub const TRACE_EXIT_FLAG: u32 = 0x8000;

/// C `_NSIG` (`sys/signal.h:45`) — upper bound on signal numbers (exclusive).
const NSIG: i64 = 64;

/// trace space values from C `sys/ptrace.h:216-225` (`ptrace_range.pr_space`).
const TS_INS: i32 = 0;
const TS_DATA: i32 = 1;

/// `W_STOPCODE` (`sys/wait.h`) — wait-status encoding for the stopped state.
pub fn w_stopcode(sig: i32) -> i32 {
    (sig << 8) | 0x7f
}

/// Errors for `do_trace`.  `Kernel` carries the raw return value of the kernel
/// SYS_TRACE — C passes `r` through everywhere (trace.c:108/131/246) and never
/// collapses it into EINVAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceError {
    Busy,
    Srch,
    Perm,
    Inval,
    Kernel(i32),
}

impl TraceError {
    pub fn to_errno(self) -> i32 {
        match self {
            Self::Busy => EBUSY,
            Self::Srch => ESRCH,
            Self::Perm => EPERM,
            Self::Inval => EINVAL,
            Self::Kernel(r) => r,
        }
    }
}

/// `ptrace` request (`m_lc_pm_ptrace`, `ipc.h:492-501`; wire =
/// `minix_types::MessLcPmPtrace`).  `data` is C `long` (signed): a negative
/// value for T_RESUME/T_DETACH means "deliver no signal" and is a legal input.
#[derive(Debug, Clone, Copy)]
pub struct PtraceReq {
    pub req: i32,
    pub pid: Pid,
    pub addr: u64,
    pub data: i64,
}

/// C `struct ptrace_range` (`sys/ptrace.h:216-225`, 32 bytes in the x86-64
/// layout): T_GETRANGE/T_SETRANGE copy it out of debugger memory via
/// `sys_datacopy` (trace.c:169).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
struct PtraceRange {
    pr_space: i32,
    _pad: u32,
    pr_addr: i64,
    pr_ptr: u64,
    pr_size: u64,
}

impl PtraceRange {
    /// View the struct as bytes for reads and writes (C's
    /// `sys_datacopy(..., &pr, sizeof(pr))` semantics: the parameter block is
    /// foreign bytes validated by the caller, so type safety is not required).
    fn as_bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: repr(C) POD struct (all-integer fields); arbitrary bytes are a legal write.
        unsafe { core::slice::from_raw_parts_mut(self as *mut Self as *mut u8, size_of::<Self>()) }
    }
}

/// Write `data` into the caller's pre-filled reply payload (C:
/// `mp_reply.m_pm_lc_ptrace.data`, trace.c:59/92/110/133/164/187/233/248); the
/// main loop then sends it whole through `reply(caller, result)` with the return
/// code carried in m_type (the same tag + typed-body contract as wait4).
fn trace_reply_data(table: &mut ProcTable, caller: UserSlot, data: i64) {
    let mut reply = Message::default();
    reply.m_u.m_pm_lc_ptrace.data = data;
    table.procs[caller.get()].ipc.reply = Some(reply);
}

/// Adapts `sig_proc` (in its trace=FALSE redelivery form) to the `SignalDeliver`
/// injection point of `check_pending` — the check_pending tail of
/// T_DETACH/T_RESUME (trace.c:213/240 call check_pending directly).
struct TraceDeliver<'a, T: crate::ipc::IpcTransport + ?Sized> {
    kern: &'a mut dyn crate::exit::KernelGateway,
    transport: &'a mut T,
}

impl<T: crate::ipc::IpcTransport + ?Sized> crate::signal_flow::SignalDeliver for TraceDeliver<'_, T> {
    fn sig_proc(&mut self, table: &mut ProcTable, target: UserSlot, signo: i32, ksig: bool) {
        // [SD19-gate] PM send/notify/reply are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::sig_proc(table, target, signo, false, ksig, self.kern, self.transport);
    }
}

/// The fall-through arm of C do_trace's switch (trace.c:244-249): pass through
/// to `sys_trace`; failures propagate the original errno (uncollapsed), and read
/// values are written back into the reply payload.
fn passthrough(
    kern: &mut dyn crate::exit::KernelGateway,
    table: &mut ProcTable,
    caller: UserSlot,
    child: UserSlot,
    req: PtraceReq,
) -> Result<ReplyIntent, TraceError> {
    let ep = table.procs[child.get()].endpoint();
    let mut data = req.data;
    kern.sys_trace(req.req, ep, req.addr, &mut data)
        .map_err(TraceError::Kernel)?;
    trace_reply_data(table, caller, data);
    Ok(ReplyIntent::Reply(OK))
}

/// `do_trace` (trace.c:42-250).
///
/// Structural correspondence with C: T_OK/T_ATTACH/T_STOP/T_READB_INS/
/// T_WRITEB_INS are each handled independently; the remaining commands pass the
/// common guards first (trace.c:140-143: exists / not exiting / tracer match /
/// TRACE_STOPPED) and are then dispatched; T_DETACH/T_RESUME/T_STEP/T_SYSCALL
/// end by falling through to `sys_trace` (C's switch `break` semantics).
pub fn do_trace<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    caller: UserSlot,
    req: PtraceReq,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) -> Result<ReplyIntent, TraceError> {
    // Whether the caller is root: C's `mp_effuid != SUPER_USER` (trace.c:67/74/102/114).
    // The kernel privilege slot corresponds to C's system processes (their
    // credential effuid is always 0), so reading credentials directly covers both
    // kinds of process.
    let caller_root = table.procs[caller.get()]
        .resources
        .privilege
        .credentials()
        .user
        .effective
        == 0;
    let caller_is_kernel = table.procs[caller.get()].is_kernel_process();

    match req.req {
        // T_OK: tracer already exists -> EBUSY; tracer = parent; reply.data = 0
        // (trace.c:55-60).  Routed through `try_set_tracer`.
        T_OK => {
            let parent = table.procs[caller.get()].parent();
            table.procs[caller.get()]
                .state
                .guardianship
                .try_set_tracer(parent)
                .map_err(|_| TraceError::Busy)?;
            trace_reply_data(table, caller, 0);
            Ok(ReplyIntent::Reply(OK))
        }
        // T_ATTACH: attach to an existing process (trace.c:62-93).
        T_ATTACH => {
            let child = table.find_proc(req.pid).ok_or(TraceError::Srch)?;
            if table.procs[child.get()].is_exiting() {
                return Err(TraceError::Srch);
            }
            // Non-root: effective uid/gid must match and the target must not be
            // setuid/setgid (trace.c:67-71).
            let caller_creds = table.procs[caller.get()]
                .resources.privilege.credentials().clone();
            let child_creds = table.procs[child.get()]
                .resources.privilege.credentials().clone();
            if !caller_root
                && (caller_creds.user.effective != child_creds.user.effective
                    || caller_creds.group.effective != child_creds.group.effective
                    || child_creds.user.effective != child_creds.user.real
                    || child_creds.group.effective != child_creds.group.real)
            {
                return Err(TraceError::Perm);
            }
            // Only root may trace system processes (trace.c:74-75).
            if !caller_root && table.procs[child.get()].is_kernel_process() {
                return Err(TraceError::Perm);
            }
            // System processes may not trace anyone (they call sys_trace directly,
            // trace.c:77-78).
            if caller_is_kernel {
                return Err(TraceError::Perm);
            }
            // Cannot trace yourself, PM, or VM (trace.c:81-82).
            if child == caller
                || table.procs[child.get()].endpoint() == Endpoint::PM
                || table.procs[child.get()].endpoint() == Endpoint::VM
            {
                return Err(TraceError::Perm);
            }
            // Already traced -> EBUSY (trace.c:85).
            if table.procs[child.get()].tracer().is_some() {
                return Err(TraceError::Busy);
            }
            let parent = table.procs[child.get()].parent();
            table.procs[child.get()].state.guardianship = Guardianship::Traced {
                parent,
                tracer: caller,
                trace_options: TraceOptions::NOEXEC, // C: mp_trace_flags = TO_NOEXEC (trace.c:88)
            };
            // sig_proc(SIGSTOP, trace=TRUE) (trace.c:90) — goes through sig_proc's
            // TRACE branch into trace_stop: the kernel stops the child, and a
            // waiting tracer receives W_STOPCODE.
            // [SD19-gate] PM send/notify/reply are fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = crate::signal::sig_proc(table, child, crate::signal::SIGSTOP, true, false, kern, transport);
            trace_reply_data(table, caller, 0);
            Ok(ReplyIntent::Reply(OK))
        }
        // T_STOP: not exposed to user programs (its effect = kill + trace), so it
        // is always EINVAL (trace.c:95-99).  Internal stops use trace_stop() and
        // do not come through the message entry point.
        T_STOP => Err(TraceError::Inval),
        // READB/WRITEB_INS: root-only text-segment read/write hack (trace.c:101-134).
        // Note: the root gate sits **before** the common guards (140-143) and
        // requires neither tracer nor TRACE_STOPPED — C returns from the switch
        // early, before 140-143.
        preq if preq == T_READB_INS || preq == T_WRITEB_INS => {
            if !caller_root {
                return Err(TraceError::Perm);
            }
            let child = table.find_proc(req.pid).ok_or(TraceError::Srch)?;
            if table.procs[child.get()].is_exiting() {
                return Err(TraceError::Srch);
            }
            let ep = table.procs[child.get()].endpoint();
            let mut data = req.data;
            kern.sys_trace(preq, ep, req.addr, &mut data)
                .map_err(TraceError::Kernel)?;
            trace_reply_data(table, caller, data);
            Ok(ReplyIntent::Reply(OK))
        }
        // All other commands: the common guards (trace.c:140-143).
        _ => {
            let child = table.find_proc(req.pid).ok_or(TraceError::Srch)?;
            if table.procs[child.get()].is_exiting() {
                return Err(TraceError::Srch);
            }
            if table.procs[child.get()].tracer() != Some(caller) {
                return Err(TraceError::Srch);
            }
            if !table.procs[child.get()].state.trace.stopped {
                return Err(TraceError::Busy);
            }
            match req.req {
                // T_EXIT: set TRACE_EXIT; if a VFS/EVENT call is in flight, save
                // exitstatus and wait for the reply, else call exit_proc
                // immediately; SUSPEND until the tail completes (trace.c:146-159).
                T_EXIT => {
                    table.procs[child.get()].state.trace.exit_pending = true;
                    let blocked = table.procs[child.get()].state.block.is_vfs_blocked()
                        || table.procs[child.get()].state.block.is_event_blocked();
                    if blocked {
                        // C: mp_exitstatus = data (truncated to char, mproc.h:25).
                        table.procs[child.get()].state.lifecycle =
                            crate::mproc::Lifecycle::Exiting { exit_code: req.data as i8, sig_status: 0 };
                    } else {
                        crate::exit::exit_proc(table, child, req.data as i8, 0, false, transport, kern);
                    }
                    Ok(ReplyIntent::ReplyLater) // SUSPEND
                }
                // T_SETOPT: whole-word assignment (trace.c:162) — TO_* option bits
                // go to TraceOptions; the TRACE_EXIT state bit (0x8000) is
                // extracted into TraceState.exit_pending; the remaining state bits
                // (STOPPED/ZOMBIE) belong to C's mixed flags word and are modelled
                // in Rust by Lifecycle/TraceState (reaching them through setopt is
                // undefined usage; this comment records the divergence).
                T_SETOPT => {
                    let bits = req.data as u32;
                    table.procs[child.get()].state.guardianship.set_trace_options(bits);
                    table.procs[child.get()].state.trace.exit_pending =
                        bits & TRACE_EXIT_FLAG != 0;
                    trace_reply_data(table, caller, 0);
                    Ok(ReplyIntent::Reply(OK))
                }
                // T_GETRANGE/T_SETRANGE: fetch the ptrace_range parameter block
                // from debugger memory, validate space and size, then vircopy in
                // the requested direction (trace.c:167-188).
                T_GETRANGE | T_SETRANGE => {
                    let mut pr = PtraceRange::default();
                    let caller_ep = table.procs[caller.get()].endpoint();
                    kern.copy_from_user(caller_ep, req.addr, pr.as_bytes_mut())
                        .map_err(TraceError::Kernel)?;
                    if pr.pr_space != TS_INS && pr.pr_space != TS_DATA {
                        return Err(TraceError::Inval);
                    }
                    // C: `pr_size == 0 || pr_size > LONG_MAX -> EINVAL`.
                    if pr.pr_size == 0 || pr.pr_size > i64::MAX as u64 {
                        return Err(TraceError::Inval);
                    }
                    let child_ep = table.procs[child.get()].endpoint();
                    if req.req == T_GETRANGE {
                        // Child -> debugger (trace.c:176-179).
                        kern.sys_vircopy(child_ep, pr.pr_addr as u64, caller_ep, pr.pr_ptr, pr.pr_size)
                            .map_err(TraceError::Kernel)?;
                    } else {
                        // Debugger -> child (trace.c:180-183).
                        kern.sys_vircopy(caller_ep, pr.pr_ptr, child_ep, pr.pr_addr as u64, pr.pr_size)
                            .map_err(TraceError::Kernel)?;
                    }
                    trace_reply_data(table, caller, 0);
                    Ok(ReplyIntent::Reply(OK))
                }
                // T_DETACH (trace.c:190-215): clear tracer -> replay sigtrace
                // (full check_sig semantics) -> deliver the data signal
                // (trace=TRUE) -> clear TRACE_STOPPED -> check_pending -> fall
                // through to the kernel passthrough (the kernel side completes
                // the detach).  trace_flags disappear with the Traced->Normal
                // variant swap (= the clearing at C trace.c:211).
                T_DETACH => {
                    // data < 0 || >= _NSIG -> EINVAL (trace.c:191) — the i64
                    // payload preserves C's negative half.
                    if req.data < 0 || req.data >= NSIG {
                        return Err(TraceError::Inval);
                    }
                    let child_pid = table.procs[child.get()].identity.id.pid;
                    table.procs[child.get()].state.guardianship =
                        Guardianship::Normal { parent: table.procs[child.get()].parent() };
                    let sigtrace = table.procs[child.get()].resources.signals.trace_mask;
                    for i in 1..NSIG {
                        if sigtrace >> (i - 1) & 1 != 0 {
                            table.procs[child.get()].resources.signals.trace_mask &= !(1u64 << (i - 1));
                            // C: check_sig(child->mp_pid, i, FALSE) (trace.c:200).
                            // [SD19-gate] PM send/notify/reply are fire-and-forget
                            #[allow(clippy::let_underscore_must_use)]
                            let _ = crate::signal::check_sig(table, caller, child_pid, i as i32, false, kern, transport);
                        }
                    }
                    if req.data > 0 {
                        // [SD19-gate] PM send/notify/reply are fire-and-forget
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = crate::signal::sig_proc(table, child, req.data as i32, true, false, kern, transport);
                    }
                    table.procs[child.get()].state.trace.stopped = false;
                    {
                        let mut deliver = TraceDeliver { kern, transport };
                        // [SD19-gate] PM send/notify/reply are fire-and-forget
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = crate::signal_flow::check_pending(table, child, &mut deliver);
                    }
                    passthrough(kern, table, caller, child, req)
                }
                // T_RESUME/T_STEP/T_SYSCALL (trace.c:217-242): deliver the data
                // signal (trace=FALSE) -> sigtrace short-circuit (feign a
                // successful resume) -> clear TRACE_STOPPED -> check_pending ->
                // fall through to the kernel passthrough.
                T_RESUME | T_STEP | T_SYSCALL => {
                    if req.data < 0 || req.data >= NSIG {
                        return Err(TraceError::Inval);
                    }
                    if req.data > 0 {
                        // [SD19-gate] PM send/notify/reply are fire-and-forget
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = crate::signal::sig_proc(table, child, req.data as i32, false, false, kern, transport);
                    }
                    // sigtrace still has bits -> feign success (reply.data=0) and
                    // do not really resume (trace.c:228-236).
                    if table.procs[child.get()].resources.signals.trace_mask != 0 {
                        trace_reply_data(table, caller, 0);
                        return Ok(ReplyIntent::Reply(OK));
                    }
                    table.procs[child.get()].state.trace.stopped = false;
                    {
                        let mut deliver = TraceDeliver { kern, transport };
                        // [SD19-gate] PM send/notify/reply are fire-and-forget
                        #[allow(clippy::let_underscore_must_use)]
                        let _ = crate::signal_flow::check_pending(table, child, &mut deliver);
                    }
                    passthrough(kern, table, caller, child, req)
                }
                // Remaining commands (T_GETINS/T_GETDATA/T_GETUSER/T_SETINS/
                // T_SETDATA/T_SETUSER): pass straight through to the kernel and
                // write read values back (trace.c:244-249).
                _ => passthrough(kern, table, caller, child, req),
            }
        }
    }
}

/// `trace_stop` (trace.c:255-276): stops a traced process once it receives a
/// signal.
///
/// A failing `sys_trace(T_STOP)` panics (trace.c:264, same text as C); sets
/// TRACE_STOPPED; when the tracer is waiting and the target matches: consumes
/// the sigtrace bit, clears WAITING, and immediately replies to the tracer with
/// the wait4 payload (W_STOPCODE) plus the pid tag (C goes through `reply()`;
/// Rust sends directly over the transport — an asynchronous reply in the middle
/// of message handling, the same shape as tell_parent).
pub fn trace_stop<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child: UserSlot,
    signo: i32,
    kern: &mut dyn crate::exit::KernelGateway,
    transport: &mut T,
) {
    let ep = table.procs[child.get()].endpoint();
    let mut data: i64 = 0;
    kern.sys_trace(T_STOP, ep, 0, &mut data)
        .unwrap_or_else(|r| panic!("sys_trace failed: {}", r));
    table.procs[child.get()].state.trace.stopped = true;

    let tracer_slot = match table.procs[child.get()].tracer() {
        Some(t) => t,
        None => return,
    };
    let child_pid = table.procs[child.get()].identity.id.pid;
    let child_procgrp = table.procs[child.get()].identity.procgrp;
    // wait_test (the matching half of forkexit.c:580-601): WAITING + target
    // match (AnyChild/SpecificChild/Group).  Take the tracer-side snapshot
    // before mutating the child to avoid overlapping borrows on the same
    // ProcTable.
    let (waiting, target, tracer_ep) = {
        let rpmp = &table.procs[tracer_slot.get()];
        (rpmp.state.wait.waiting, rpmp.state.wait.target, rpmp.endpoint())
    };
    if waiting {
        let matched = match target {
            WaitTarget::AnyChild => true,
            WaitTarget::SpecificChild(p) => p == child_pid,
            WaitTarget::Group(g) => -g == child_procgrp,
        };
        if matched {
            table.procs[child.get()].resources.signals.trace_mask &= !(1u64 << (signo as u64 - 1));
            table.procs[tracer_slot.get()].state.wait.waiting = false;
            // C: mp_reply.m_pm_lc_wait4.status = W_STOPCODE(signo);
            // reply(tracer, child_pid) (trace.c:273-274).
            let mut reply = Message { m_type: child_pid, ..Default::default() };
            reply.m_u.m_pm_lc_wait4.status = w_stopcode(signo);
            transport.send(tracer_ep, &reply).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{Lifecycle, Privilege, Credentials};

    /// Scriptable kernel mock: records SYS_TRACE/vircopy calls and lets the
    /// caller script return values, the SYS_TRACE read value, and the parameter
    /// block content delivered by copy_from_user.
    struct MockKernel {
        traces: Vec<(i32, i32, u64)>,
        trace_reply: Result<i64, i32>,
        vircopy_calls: Vec<(i32, u64, i32, u64, u64)>,
        range_block: Option<PtraceRange>,
    }
    impl Default for MockKernel {
        fn default() -> Self {
            Self { traces: Vec::new(), trace_reply: Ok(0), vircopy_calls: Vec::new(), range_block: None }
        }
    }

    impl crate::exit::KernelGateway for MockKernel {
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, req: i32, ep: Endpoint, addr: u64, data: &mut i64) -> Result<(), i32> {
            self.traces.push((req, ep.0, addr));
            match self.trace_reply {
                Ok(v) => {
                    *data = v;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        fn sys_vircopy(&mut self, src_ep: Endpoint, src: u64, dst_ep: Endpoint, dst: u64, len: u64) -> Result<(), i32> {
            self.vircopy_calls.push((src_ep.0, src, dst_ep.0, dst, len));
            Ok(())
        }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
        fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn copy_from_user(&mut self, _src_ep: Endpoint, _src: u64, bytes: &mut [u8]) -> Result<(), i32> {
            match &self.range_block {
                Some(pr) => {
                    let src = unsafe {
                        core::slice::from_raw_parts(pr as *const PtraceRange as *const u8, size_of::<PtraceRange>())
                    };
                    bytes[..src.len()].copy_from_slice(src);
                }
                None => bytes.fill(0),
            }
            Ok(())
        }
    }

    fn mk_proc(table: &mut ProcTable, slot: usize, pid: Pid) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        table.procs[slot].state.guardianship = Guardianship::Normal { parent: UserSlot::new(0) };
        table.procs[slot].state.trace = crate::mproc::TraceState::default();
    }

    /// Build the setup where caller 0 is the tracer of the process in slot 5 and
    /// that process is stopped.
    fn traced_stopped(table: &mut ProcTable, caller: usize, target: usize) {
        table.procs[target].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(caller),
            tracer: UserSlot::new(caller),
            trace_options: TraceOptions::empty(),
        };
        table.procs[target].state.trace.stopped = true;
    }

    // ---- Full constant-set reconciliation against C (the exhaustive version of
    // this test: an earlier same-named test asserted only the 2 constants that
    // happened to be right, leaving 15 wrong values uncovered — the test name
    // misreported what it checked).----

    #[test]
    fn test_constants_match_c() {
        // Value-by-value reconciliation with sys/sys/ptrace.h:226-250
        // (PT_* aliases + the Minix-specific range).
        assert_eq!(T_OK, 0); // PT_TRACE_ME
        assert_eq!(T_GETINS, 1); // PT_READ_I
        assert_eq!(T_GETDATA, 2); // PT_READ_D
        assert_eq!(T_SETINS, 4); // PT_WRITE_I
        assert_eq!(T_SETDATA, 5); // PT_WRITE_D
        assert_eq!(T_RESUME, 7); // PT_CONTINUE
        assert_eq!(T_EXIT, 8); // PT_KILL
        assert_eq!(T_ATTACH, 9); // PT_ATTACH
        assert_eq!(T_DETACH, 10); // PT_DETACH
        assert_eq!(T_SYSCALL, 14); // PT_SYSCALL
        assert_eq!(T_STOP, -1);
        assert_eq!(T_READB_INS, 100);
        assert_eq!(T_WRITEB_INS, 101);
        assert_eq!(T_GETUSER, 102);
        assert_eq!(T_SETUSER, 103);
        assert_eq!(T_STEP, 104);
        assert_eq!(T_SETOPT, 105);
        assert_eq!(T_GETRANGE, 106);
        assert_eq!(T_SETRANGE, 107);
    }

    #[test]
    fn test_w_stopcode() {
        assert_eq!(w_stopcode(11), (11 << 8) | 0x7f);
        assert_eq!(w_stopcode(5), (5 << 8) | 0x7f);
    }

    // ---- do_trace branch matrix ----

    #[test]
    fn test_t_ok_sets_tracer_and_zero_payload() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_OK, pid: 0, addr: 0, data: 0 };
        // The first T_OK succeeds (tracer = parent) with reply payload data = 0.
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        assert!(table.procs[0].tracer().is_some());
        assert_eq!(
            unsafe { table.procs[0].ipc.reply.take().unwrap().m_u.m_pm_lc_ptrace.data },
            0,
            "C trace.c:59: reply.data = 0"
        );
        // A second T_OK -> EBUSY (trace.c:56).
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t);
        assert_eq!(res.unwrap_err(), TraceError::Busy);
    }

    #[test]
    fn test_t_attach_sets_noexec_and_stops_child() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_ATTACH, pid: 42, addr: 0, data: 0 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        // C trace.c:88: trace_flags = TO_NOEXEC.
        assert_eq!(
            table.procs[5].state.guardianship.trace_options(),
            TraceOptions::NOEXEC,
            "attach must set TO_NOEXEC (C trace.c:88)"
        );
        // C trace.c:90: sig_proc(SIGSTOP, TRUE) -> TRACE branch -> trace_stop:
        // the kernel receives T_STOP and the child becomes stopped.
        assert!(table.procs[5].state.trace.stopped, "attach must stop the child");
        assert!(
            kern.traces.iter().any(|&(req, _, _)| req == T_STOP),
            "trace_stop must issue sys_trace(T_STOP)"
        );
        // sigtrace records SIGSTOP (the sigaddset done in the trace branch).
        assert_eq!(table.procs[5].resources.signals.trace_mask, 1u64 << (17 - 1));
    }

    #[test]
    fn test_t_stop_is_einval() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_STOP, pid: 0, addr: 0, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Inval
        );
    }

    #[test]
    fn test_readb_ins_requires_root_not_tracer() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10); // caller, not root (uid 1000)
        mk_proc(&mut table, 5, 42);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // Non-root -> EPERM (trace.c:102), even with no tracer/TRACE_STOPPED —
        // the root gate precedes the common guards.
        let req = PtraceReq { req: T_READB_INS, pid: 42, addr: 0x100, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Perm
        );
        // As root: passes through even without a tracer (the guards do not
        // apply), and the read value is filled in (trace.c:110).
        table.procs[0].resources.privilege = Privilege::User(Credentials::new(0, 0));
        kern.trace_reply = Ok(0x55);
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        assert_eq!(unsafe { table.procs[0].ipc.reply.take().unwrap().m_u.m_pm_lc_ptrace.data }, 0x55);
    }

    #[test]
    fn test_t_exit_defers_when_vfs_blocked_else_exits() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // A VFS_CALL in flight -> save exitstatus + SUSPEND (trace.c:150-151/159).
        table.procs[5].state.block.ipc_blocked =
            Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let req = PtraceReq { req: T_EXIT, pid: 42, addr: 0, data: 7 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
        assert!(table.procs[5].state.trace.exit_pending);
        assert!(table.procs[5].is_exiting(), "exitstatus saved as Exiting");
        // Not blocked -> exit_proc runs directly (C trace.c:153-154): the process
        // is EXITING and VFS receives VFS_PM_EXIT (the tell_vfs at exit_proc's tail).
        let mut table2 = ProcTable::new();
        mk_proc(&mut table2, 0, 10);
        mk_proc(&mut table2, 5, 42);
        traced_stopped(&mut table2, 0, 5);
        let mut t2 = crate::ipc::TestIpcTransport::default();
        let mut kern2 = MockKernel::default();
        let req = PtraceReq { req: T_EXIT, pid: 42, addr: 0, data: 7 };
        let res = do_trace(&mut table2, UserSlot::new(0), req, &mut kern2, &mut t2).unwrap();
        assert_eq!(res, ReplyIntent::ReplyLater);
        // exit_proc runs the full chain: Exiting -> zombify (exit.rs steps 10-11);
        // the final state is a zombie (handled by zombify in the TRACE_EXIT case),
        // no longer Exiting.
        assert!(
            matches!(
                table2.procs[5].state.lifecycle,
                Lifecycle::Zombie { .. } | Lifecycle::TraceZombie { .. }
            ),
            "exit_proc must run the full chain to zombify, got {:?}",
            table2.procs[5].state.lifecycle
        );
        assert!(
            t2.sent().iter().any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
            "exit_proc must tell VFS"
        );
    }

    #[test]
    fn test_t_setopt_sets_options_and_exit_flag() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // TO_TRACEFORK|TO_NOEXEC + the TRACE_EXIT state bit (C's whole-word
        // assignment).
        let req = PtraceReq { req: T_SETOPT, pid: 42, addr: 0, data: (0x1 | 0x4 | TRACE_EXIT_FLAG) as i64 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        assert_eq!(
            table.procs[5].state.guardianship.trace_options(),
            TraceOptions::TRACEFORK | TraceOptions::NOEXEC
        );
        assert!(table.procs[5].state.trace.exit_pending, "TRACE_EXIT bit must reach exit_pending");
    }

    #[test]
    fn test_t_getrange_setrange_validate_and_direction() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // A zero-filled parameter block -> pr_space = 0 = TS_INS is valid, but
        // pr_size = 0 -> EINVAL (the size check at trace.c:174).
        let req = PtraceReq { req: T_GETRANGE, pid: 42, addr: 0x9000, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Inval,
            "pr_size == 0 must be EINVAL"
        );
        // Invalid space -> EINVAL (trace.c:173).
        kern.range_block = Some(PtraceRange { pr_space: 9, _pad: 0, pr_addr: 0, pr_ptr: 0, pr_size: 8 });
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Inval
        );
        // Valid parameter block: SETRANGE direction = debugger -> child
        // (trace.c:180-183).
        kern.range_block = Some(PtraceRange {
            pr_space: TS_DATA,
            _pad: 0,
            pr_addr: 0x5000,
            pr_ptr: 0x9000,
            pr_size: 64,
        });
        let req = PtraceReq { req: T_SETRANGE, pid: 42, addr: 0x9000, data: 0 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        let caller_ep = table.procs[0].identity.endpoint;
        let child_ep = table.procs[5].identity.endpoint;
        assert_eq!(kern.vircopy_calls, vec![(caller_ep.0, 0x9000, child_ep.0, 0x5000, 64)]);
    }

    #[test]
    fn test_t_detach_replays_sigtrace_and_falls_through_to_kernel() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        // sigtrace holds SIGTRAP(5) and SIGILL(4): the replay must deliver them
        // through check_sig.
        table.procs[5].resources.signals.trace_mask = (1u64 << (5 - 1)) | (1u64 << (4 - 1));
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_DETACH, pid: 42, addr: 0, data: 0 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        // Tracer cleared (trace.c:194).
        assert!(table.procs[5].tracer().is_none());
        // Replay: the sigtrace bits are consumed (check_sig -> sig_proc).
        assert_eq!(table.procs[5].resources.signals.trace_mask, 0);
        // C trace.c:215 break -> 244: the kernel passthrough must happen
        // (T_DETACH reaches the kernel).
        assert!(
            kern.traces.iter().any(|&(req, _, _)| req == T_DETACH),
            "detach must fall through to sys_trace (C trace.c:244)"
        );
        // stopped cleared (trace.c:210).
        assert!(!table.procs[5].state.trace.stopped);
    }

    #[test]
    fn test_t_resume_feigns_success_when_sigtrace_pending() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        table.procs[5].resources.signals.trace_mask = 1u64 << (5 - 1); // SIGTRAP
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        let req = PtraceReq { req: T_RESUME, pid: 42, addr: 0, data: 0 };
        let res = do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap();
        assert_eq!(res, ReplyIntent::Reply(OK));
        // Feigned success: no resume and no passthrough (trace.c:231-236).
        assert!(table.procs[5].state.trace.stopped);
        assert!(kern.traces.is_empty());
        // reply.data = 0 (trace.c:233).
        assert_eq!(unsafe { table.procs[0].ipc.reply.take().unwrap().m_u.m_pm_lc_ptrace.data }, 0);
    }

    #[test]
    fn test_t_resume_negative_data_is_einval() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // C trace.c:220: data < 0 -> EINVAL (the i64 payload preserves the
        // negative half; an older u64 model made this branch unreachable).
        let req = PtraceReq { req: T_RESUME, pid: 42, addr: 0, data: -1 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Inval
        );
    }

    #[test]
    fn test_generic_guard_tracer_mismatch_is_esrch() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        traced_stopped(&mut table, 0, 5);
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        // Nonexistent pid -> ESRCH (trace.c:140).
        let req = PtraceReq { req: T_GETDATA, pid: 43, addr: 0, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Srch
        );
        // Tracer matches but the child is not stopped -> EBUSY (trace.c:143).
        table.procs[5].state.trace.stopped = false;
        let req = PtraceReq { req: T_GETDATA, pid: 42, addr: 0, data: 0 };
        assert_eq!(
            do_trace(&mut table, UserSlot::new(0), req, &mut kern, &mut t).unwrap_err(),
            TraceError::Busy
        );
    }

    // ---- trace_stop ----

    #[test]
    fn test_trace_stop_replies_waiting_tracer_with_payload() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(0),
            tracer: UserSlot::new(0),
            trace_options: TraceOptions::empty(),
        };
        table.procs[5].resources.signals.trace_mask = 1u64 << 10; // bit for signal 11
        table.procs[0].state.wait.waiting = true;
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        trace_stop(&mut table, UserSlot::new(5), 11, &mut kern, &mut t);
        assert!(table.procs[5].state.trace.stopped);
        // Wire contract: the status is in the m_pm_lc_wait4.status payload and
        // m_type is the pid.
        assert_eq!(t.sent().len(), 1, "tracer waiting → immediate reply");
        let (dest, msg) = &t.sent()[0];
        assert_eq!(*dest, table.procs[0].identity.endpoint);
        assert_eq!(msg.m_type, 42, "reply(tracer, child_pid) — C trace.c:274");
        assert_eq!(unsafe { msg.m_u.m_pm_lc_wait4.status }, w_stopcode(11), "C trace.c:273 payload");
        // sigtrace consumed (trace.c:270).
        assert_eq!(table.procs[5].resources.signals.trace_mask & (1u64 << 10), 0);
    }

    #[test]
    fn test_trace_stop_not_waiting_only_stops() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(0),
            tracer: UserSlot::new(0),
            trace_options: TraceOptions::empty(),
        };
        let mut kern = MockKernel::default();
        let mut t = crate::ipc::TestIpcTransport::default();
        trace_stop(&mut table, UserSlot::new(5), 11, &mut kern, &mut t);
        assert!(table.procs[5].state.trace.stopped);
        assert!(t.sent().is_empty(), "tracer not waiting → no reply");
        assert!(kern.traces.iter().any(|&(req, _, _)| req == T_STOP));
    }

    #[test]
    fn test_trace_stop_kernel_failure_panics() {
        let mut table = ProcTable::new();
        mk_proc(&mut table, 0, 10);
        mk_proc(&mut table, 5, 42);
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(0),
            tracer: UserSlot::new(0),
            trace_options: TraceOptions::empty(),
        };
        let mut kern = MockKernel { trace_reply: Err(-5), ..MockKernel::default() }; // EIO
        let mut t = crate::ipc::TestIpcTransport::default();
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            trace_stop(&mut table, UserSlot::new(5), 11, &mut kern, &mut t);
        }));
        assert!(res.is_err(), "sys_trace(T_STOP) failure must panic (C trace.c:264)");
    }
}
