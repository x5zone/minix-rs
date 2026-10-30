//! Exit path: `do_exit → exit_proc → exit_restart` + zombie/reap chain.
//!
//! C ground truth: `minix3/minix/servers/pm/forkexit.c:242-469` (do_exit/exit_proc/exit_restart)
//! + `590-807` (zombify/check_parent/tracer_died/cleanup) + `mproc.h:86-104`
//!   flags + `main.c:365` publish_event.
//!
//! Design: explicit orchestrator + `Lifecycle` enum (`mproc/lifecycle.rs:27`)
//! + `Guardianship` (`mproc/guardianship.rs`) + `BlockState`.
//!   Single-threaded event loop — `&mut ProcTable` without `Arc`/`Mutex`.

use minix_types::{Endpoint, Message, UserSlot, VfsCall, VirBytes};
use crate::ipc::ReplyIntent;
use crate::mproc::{ProcTable, Lifecycle};
use alloc::vec::Vec;

/// Exit status truncation: Minix3 `mp_exitstatus` is `char` (`mproc.h:25`).
fn trunc_status(status: i32) -> i8 {
    status as i8
}

/// PM's kernel-call outlet (the PRIV_PROC violation branch of `do_exit`).
///
/// Same shape as `KernelGateway` in VM's `kernel_gateway.rs`: handlers program
/// against a trait, the production implementation goes through the real kernel
/// call wire (before E1 the trap stub honestly returns `-EIO`), and tests
/// inject scripted mocks. Later SYS_TIMES/SYS_CLEAR calls extend along the
/// same pattern.
pub trait KernelGateway {
    /// C: `sys_kill(proc_ep, signr)` (libsys `sys_kill.c:3-13`) —
    /// `_kernel_call(SYS_KILL, &m)`, payload `m_sigcalls.{endpt,sig}`,
    /// return value = kernel reply (OK or negative errno).
    fn sys_kill(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32>;

    /// C: `sys_clear(proc_ep)` (libsys `sys_clear.c:8-14`) —
    /// `_kernel_call(SYS_CLEAR, &m)`, payload m1i1 = target endpoint, no
    /// reply payload; return value = kernel reply (OK or negative errno).
    fn sys_clear(&mut self, ep: Endpoint) -> Result<(), i32>;

    /// C: `sys_statectl(request, address, length)` (libsys
    /// `sys_statectl.c:3-11`) — state control plane (clear/attach IPC filters,
    /// state-table registration). Phase one of the birth protocol
    /// (`process_init`) sends its clear-filter request through this. The
    /// default implementation models an accepting kernel (test doubles need no
    /// per-call override); production [`TrapKernelGateway`] goes through the
    /// trap channel.
    fn statectl(&mut self, _request: i32, _address: u64, _length: i32) -> Result<i32, i32> {
        Ok(minix_types::OK)
    }

    /// C: `sys_abort(how)` (libsys `sys_abort.c:8-13`) —
    /// `_kernel_call(SYS_ABORT, &m)`, payload m1i1 = `how` (RB_* bit set).
    /// On success the machine halts outright; failure returns a negative errno
    /// (the C caller ignores it).
    fn sys_abort(&mut self, how: i32) -> Result<(), i32>;

    /// C: `sys_times(proc_ep, &user, &sys, NULL, NULL)` (libsys
    /// `sys_times.c:3-23`) — read the target process's user/system CPU ticks.
    /// Failure returns a negative errno.
    fn proc_times(&mut self, ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32>;

    /// C: `sys_datacopy(src_ep, src, dst_ep, dst, len)` (libsys
    /// `sys_datacopy.c`; kernel `dispatch_vircopy` = `Syscall::Vircopy = 15`)
    /// — write `bytes` to virtual address `dst_addr` in process `dst_ep`.
    /// Failure returns a negative errno (e.g. an invalid parent buffer).
    fn copy_to_user(&mut self, bytes: &[u8], dst_ep: Endpoint, dst_addr: u64) -> Result<(), i32>;

    /// C: `sys_resume(proc_ep)` (`syslib.h:48` = `sys_runctl(ep, RC_RESUME, 0)`)
    /// — clear the kernel-side `PROC_STOPPED` and resume the stopped process.
    /// Returns the raw kernel reply (OK = 0 / negative errno); the caller
    /// (signal.c:285 `try_resume_proc`) panics on anything but OK.
    fn sys_resume(&mut self, ep: Endpoint) -> Result<(), i32>;

    /// C: `sys_delay_stop(proc_ep)` (libsys `delay_stop.c`; inside `stop_proc`
    /// at `signal.c:239`) — stop the target process on the kernel side
    /// (`PROC_STOPPED`). Returns OK = stopped / `EBUSY` = a kernel delay call
    /// is in flight (acceptable only with `may_delay=TRUE`) / any other
    /// negative errno = failure (`stop_proc` panics on failure, as in C).
    /// The production implementation awaits a minix-sys wrapper.
    fn sys_delay_stop(&mut self, ep: Endpoint) -> Result<(), i32>;

    /// Restore a signal context (C: `sys_sigreturn(ep, ctx)` — libsys
    /// `sys_sigreturn.c`; wired up in a later batch). The default pre-wire
    /// implementation fails honestly; production `TrapKernelGateway` delegates
    /// to minix-sys `sys_sigreturn`.
    fn sys_sigreturn(&mut self, ep: Endpoint, ctx: VirBytes) -> Result<(), i32> {
        //[SD19-gate] PM send/notify/reply results are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = (ep, ctx);
        Err(-minix_types::ENOSYS)
    }

    /// PM diagnostic output: one diagnostic line reaches the kernel console
    /// via sys_diagctl code 1 (the Rust counterpart of C printf). The default
    /// pre-wire implementation fails honestly; production TrapKernelGateway
    /// delegates to minix-sys sys_diagctl_write.
    fn diag_write(&mut self, text: &str) -> Result<(), i32> {
        //[SD19-gate] PM send/notify/reply results are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = text;
        Err(-minix_types::ENOSYS)
    }

    /// C: `sys_trace(req, proc_ep, addr, &data)` (libsys `sys_trace.c:3-18`)
    /// — the kernel SYS_TRACE channel: trace_stop's T_STOP (trace.c:263),
    /// do_trace's command passthrough (trace.c:244-248), and READB/WRITEB_INS
    /// (trace.c:106/129). The kernel writes read values back into `data`.
    /// Failure returns a negative errno (the caller passes it through as C
    /// does, without folding).
    fn sys_trace(&mut self, req: i32, ep: Endpoint, addr: u64, data: &mut i64) -> Result<(), i32>;

    /// C: `sys_vircopy(src_ep, src, dst_ep, dst, len)` (libsys
    /// `sys_vircopy.c:8-16`) — cross-process virtual-address copy with the
    /// direction chosen by the caller: T_GETRANGE/T_SETRANGE moving data
    /// between the traced process and the debugger buffer (trace.c:176-183).
    /// Unlike [`Self::copy_to_user`], this method implies no SELF endpoint.
    fn sys_vircopy(
        &mut self,
        src_ep: Endpoint,
        src: u64,
        dst_ep: Endpoint,
        dst: u64,
        len: u64,
    ) -> Result<(), i32>;

    /// C: `sys_datacopy(src_ep, src, SELF, dst, len)` (libsys
    /// `sys_datacopy.c`; the kernel side has the same shape as VIRCOPY) —
    /// read bytes from a virtual address in process `src_ep` into a PM-local
    /// buffer. T_GETRANGE's `ptrace_range` parameter block comes out through
    /// this channel (trace.c:169-171).
    fn copy_from_user(&mut self, src_ep: Endpoint, src: u64, bytes: &mut [u8]) -> Result<(), i32>;

    /// C: `sys_getksig(&target, &set)` (libsys `sys_getsig.c:6-17`) —
    /// fetch one process with a pending kernel signal (endpoint + bitmap);
    /// `None` = no more pending kernel signals (C: endpt == NONE). Fetching
    /// consumes the kernel's RTS_SIGNALED. This is the fetch half of the
    /// SIGKSIG pull loop (sef_signal.c:27-63). The bitmap comes back in C wire
    /// form (`sigset_t` = 4×u32, 128 bits): kernel-internal signals
    /// (SIGSNDELAY/SIGKSIG family, 71..=74) land in word 2.
    fn get_ksig(&mut self) -> Result<Option<(Endpoint, minix_types::SigSetBits)>, i32>;

    /// C: `sys_endksig(proc_nr_e, sig_nr)` (libsys `sys_endsig.c:6-15`)
    /// — acknowledge consumption of one kernel signal (clear SIG_PENDING).
    /// The caller must be the target process's signal manager (else EPERM).
    /// This is the acknowledge half of the pull loop.
    fn end_ksig(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32>;

    /// C: `sys_sigsend(proc_nr_e, smp)` (libsys `sys_sigsend.c:8-18`) —
    /// the kernel builds a sigframe from `sigmsg` and wakes the target
    /// process (the delivery action for a caught signal, signal.c:818).
    /// EFAULT/ENOMEM = process memory cannot hold the handler (a legal
    /// failure); any other negative errno = PM/kernel mismatch (the caller
    /// panics, as in C).
    fn sys_sigsend(&mut self, ep: Endpoint, sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32>;

    /// C: `sys_diagctl_stacktrace(proc_nr_e)` (`syslib.h:167` =
    /// `sys_diagctl(DIAGCTL_CODE_STACKTRACE, NULL, ep)`) — ask the kernel to
    /// print a stack trace of the target process. The C callers
    /// (`signal.c:464-466`, RS `main.c:681-683`) ignore the return value;
    /// diagnostic-channel failures are silent.
    fn sys_diagctl_stacktrace(&mut self, ep: Endpoint) -> Result<(), i32>;
}

/// Production implementation: kernel calls go through minix-sys's trap channel
/// (returns `-EIO` before E1).
pub struct TrapKernelGateway<T: minix_sys::syscall::KernelCallTransport> {
    pub transport: T,
}

impl<T: minix_sys::syscall::KernelCallTransport> TrapKernelGateway<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }
}

impl<T: minix_sys::syscall::KernelCallTransport> KernelGateway for TrapKernelGateway<T> {
    fn sys_kill(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_kill(&self.transport, ep.0, sig);
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn sys_clear(&mut self, ep: Endpoint) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_clear(&self.transport, ep.0);
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn sys_abort(&mut self, how: i32) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_abort(&self.transport, how);
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn statectl(&mut self, request: i32, address: u64, length: i32) -> Result<i32, i32> {
        let r = minix_sys::syscall::sys_statectl(&self.transport, request, address, length);
        if r < 0 { Err(r) } else { Ok(r) }
    }

    fn proc_times(&mut self, ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
        let times = minix_sys::syscall::sys_times(&self.transport, ep.0)?;
        Ok((times.user_time as minix_types::Clock, times.system_time as minix_types::Clock))
    }

    fn copy_to_user(&mut self, bytes: &[u8], dst_ep: Endpoint, dst_addr: u64) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_vircopy(
            &self.transport,
            minix_sys::syscall::SELF,
            bytes.as_ptr() as u64,
            dst_ep.0,
            dst_addr,
            bytes.len() as u64,
        );
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn sys_resume(&mut self, ep: Endpoint) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_runctl(&self.transport, ep.0, minix_sys::syscall::RC_RESUME, 0);
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> {
        // Honest pre-E6 placeholder: minix-sys has no sys_delay_stop wrapper
        // yet, so every call fails with -EIO — `stop_proc` panics on failure
        // (same shape as C signal.c:245 "sys_delay_stop failed"), and no stop
        // state is ever faked.
        Err(-minix_types::EIO)
    }

    fn sys_trace(&mut self, req: i32, ep: Endpoint, addr: u64, data: &mut i64) -> Result<(), i32> {
        // Real SYS_TRACE channel (the kernel-side dispatch_trace exists; the
        // minix-sys sys_trace wrapper is an E6 slice).
        minix_sys::syscall::sys_trace(&self.transport, req, ep.0, addr, data)
    }

    fn sys_vircopy(
        &mut self,
        src_ep: Endpoint,
        src: u64,
        dst_ep: Endpoint,
        dst: u64,
        len: u64,
    ) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_vircopy(
            &self.transport,
            src_ep.0,
            src,
            dst_ep.0,
            dst,
            len,
        );
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn copy_from_user(&mut self, src_ep: Endpoint, src: u64, bytes: &mut [u8]) -> Result<(), i32> {
        let r = minix_sys::syscall::sys_vircopy(
            &self.transport,
            src_ep.0,
            src,
            minix_sys::syscall::SELF,
            bytes.as_mut_ptr() as u64,
            bytes.len() as u64,
        );
        if r < 0 {
            Err(r)
        } else {
            Ok(())
        }
    }

    fn get_ksig(&mut self) -> Result<Option<(Endpoint, minix_types::SigSetBits)>, i32> {
        // Real SYS_GETKSIG channel (kernel dispatch_getksig, syscall_signal.rs:393).
        let (endpt, map) = minix_sys::syscall::sys_getksig(&self.transport)?;
        if endpt == Endpoint::NONE.0 {
            Ok(None)
        } else {
            Ok(Some((Endpoint(endpt), map)))
        }
    }

    fn end_ksig(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32> {
        minix_sys::syscall::sys_endksig(&self.transport, ep.0, sig)
    }

    fn sys_sigsend(&mut self, ep: Endpoint, sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> {
        minix_sys::syscall::sys_sigsend(&self.transport, ep.0, sigmsg)
    }

    fn sys_diagctl_stacktrace(&mut self, ep: Endpoint) -> Result<(), i32> {
        minix_sys::syscall::sys_diagctl_stacktrace(&self.transport, ep.0)
    }

    fn sys_sigreturn(&mut self, ep: Endpoint, ctx: VirBytes) -> Result<(), i32> {
        minix_sys::syscall::sys_sigreturn(&self.transport, ep.0, ctx.0)
    }

    fn diag_write(&mut self, text: &str) -> Result<(), i32> {
        minix_sys::syscall::sys_diagctl_write(&self.transport, text)
    }
}

/// Handles `PM_EXIT` (`do_exit`, `forkexit.c:245-262`).
///
/// - `PRIV_PROC` (system service) → `sys_kill(endpoint, SIGKILL)` + `NoReply`
/// - otherwise → `exit_proc` + `NoReply` (beyond the grave: the `SUSPEND`
///   no-reply class)
pub fn do_exit<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    caller: UserSlot,
    status: i32,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) -> ReplyIntent {
    let proc = &table.procs[caller.get()];
    if proc.is_kernel_process() {
        // C: forkexit.c:250-256 — a system process must not be terminated
        // through PM's exit() ("System processes do not use PM's exit()"):
        // after a printf warning C calls `sys_kill(mp->mp_endpoint, SIGKILL)`
        // and does not check the return value — the actual termination later
        // completes when the kernel signal path returns to PM via
        // process_ksig (11). So exit_proc is deliberately **not** called here:
        // the offending process stays Running in the PM table, waiting for the
        // SIGKILL kernel-signal round trip. The printf at C 254-255 is part of
        // the observable surface (visible to operators on real hardware), and
        // goes through the kernel diagctl channel via pm_diag!.
        let name_raw = proc.identity.name;
        let name_len = name_raw.iter().position(|&b| b == 0).unwrap_or(name_raw.len());
        let name_str = core::str::from_utf8(&name_raw[..name_len]).unwrap_or("");
        pm_diag!(
            "PM: system process {} ({}) tries to exit(), sending SIGKILL",
            proc.endpoint().get(),
            name_str
        );
        //[SD19-gate] PM send/notify/reply results are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = kern.sys_kill(proc.endpoint(), crate::signal::SIGKILL);
        return ReplyIntent::NoReply;
    }
    exit_proc(
        table,
        caller,
        trunc_status(status),
        0,
        false,
        transport,
        kern,
    );
    ReplyIntent::NoReply
}

/// First half of exit: 9 steps (`forkexit.c:267-413`).
///
/// Caller must be `!PRIV_PROC` (system case handled in `do_exit`).
/// Sets `VFS_CALL` on exiting slot via `tell_vfs`, marks `EXITING`, `zombify` if
/// `!dump_core`, `disinherit` loop (INIT adoption + `NEW_PARENT`), `SIGHUP` for
/// session leader. Leaves `procs_in_use` unchanged (still counted).
///
/// `sig_status` is the counterpart of C `mp_sigstatus` (`signal.c:552` —
/// `sig_proc_exit` writes the terminating signal number before entering
/// `exit_proc`; `forkexit.c:354` passes it to VFS as `VFS_PM_TERM_SIG` along
/// with DUMPCORE, and it survives with EXITING until zombify/wait). Non-signal
/// deaths (`do_exit`, VFS-driven destruction, trace termination) pass 0. The
/// signal number must be fixed before the call: the EXITING lifecycle is only
/// established in step 10, so reading it back from the lifecycle here would
/// always yield 0 (a constant term_sig of 0 makes VFS reject DUMPCORE and the
/// exit chain stalls).
pub fn exit_proc<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    status: i8,
    sig_status: i8,
    mut dump_core: bool,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    // ---- 1. dump_core double gate (285-292) ----
    {
        let proc = &table.procs[slot.get()];
        // forkexit.c:285-286: a setuid program (real uid != effective uid)
        // does not dump core. The test applies uniformly to every mproc
        // (including PRIV_PROC); credentials always exist.
        if dump_core {
            let creds = proc.resources.privilege.credentials();
            if creds.user.real != creds.user.effective {
                dump_core = false;
            }
        }
        // forkexit.c:291-292: a PRIV_PROC system process does not dump core.
        if dump_core && proc.is_kernel_process() {
            dump_core = false;
        }
    }

    let proc_nr = slot.get();
    let proc_ep = table.procs[proc_nr].endpoint();
    // ---- 2. session leader procgrp memory (298) ----
    let procgrp = {
        let proc = &table.procs[proc_nr];
        if proc.identity.id.pid == proc.identity.procgrp {
            proc.identity.procgrp
        } else {
            0
        }
    };

    // ---- 3. ALARM_ON → set_alarm(0) (301) ----
    {
        let proc = &mut table.procs[proc_nr];
        if proc.resources.flags.contains(crate::mproc::RemainingFlags::ALARM_ON) {
            proc.resources.flags.remove(crate::mproc::RemainingFlags::ALARM_ON);
            proc.resources.timer = None;
        }
    }

    // ---- 4. sys_times accounting (305-310) ----
    // C: read the dying process's own user/system CPU ticks and accumulate
    // them into its child bucket (`rmp->mp_child_utime += user_time`); they
    // merge again when the parent waits (tell_parent, forkexit.c:722-723).
    // Failure panics — missing accounting is unrecoverable.
    match kern.proc_times(proc_ep) {
        Ok((user, sys)) => {
            table.procs[proc_nr].resources.child_utime += user;
            table.procs[proc_nr].resources.child_stime += sys;
        }
        Err(r) => panic!("exit_proc: sys_times failed: {}", r),
    }

    // ---- 5. PROC_STOPPED forced (326-330) ----
    {
        let proc = &mut table.procs[proc_nr];
        if !proc.state.block.stopped {
            // `sys_stop` would stop scheduling; we set flag and rely on main.c:80-82 EXITING drop
            proc.state.block.stopped = true;
        }
    }

    // ---- 6. vm_willexit (332-334) ----
    // C: `if((r=vm_willexit(proc_nr_e)) != OK) panic("exit_proc: vm_willexit
    // failed: %d", r);` — VM's memory accounting depends on this
    // notification; failure is unrecoverable.
    if let Err(r) = crate::ipc::vm_willexit(transport, proc_ep) {
        panic!("exit_proc: vm_willexit failed: {}", r);
    }

    // ---- 7. INIT/VFS special (336-345) ----
    // In C: INIT dies → stacktrace + return (no VFS); VFS dies → panic
    // For Rust, we handle via early return for INIT (slot of INIT_PROC_NR) and panic for VFS
    const INIT_PROC_NR: usize = 11;
    const VFS_PROC_NR: i32 = 1;
    if proc_ep == Endpoint::from_generation_slot(0, INIT_PROC_NR as i32) || proc_nr == INIT_PROC_NR {
        // INIT died — C 336-341: printf + sys_diagctl_stacktrace + return
        // (no VFS: VFS has no fproc for INIT). The diagnostic line reaches the
        // kernel console via pm_diag!, and a failed stack trace does not block
        // — C does not check the return value either.
        // Difference from C: C leaves INIT's mp_flags untouched (the table
        // slot stays in a running state nobody manages); Rust additionally
        // marks it Exiting so the lifecycle state machine stays observable
        // (see the difference table in doc 09).
        pm_diag!(
            "PM: INIT died with exit status {}; showing stacktrace",
            status
        );
        //[SD19-gate] PM send/notify/reply results are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = kern.sys_diagctl_stacktrace(proc_ep);
        table.procs[proc_nr].state.lifecycle = Lifecycle::Exiting { exit_code: status, sig_status };
        return;
    }
    if proc_ep.get() == VFS_PROC_NR {
        panic!("exit_proc: VFS died");
    }

    // ---- 8. VFS tell (350-359) ----
    {
        let call = if dump_core {
            // C: forkexit.c:354-357 — `m.VFS_PM_PATH = rmp->mp_name` (an m7p1
            // pointer; VFS safecopies it later). This line carries the name
            // **by value** instead: `mp_name`'s Rust counterpart
            // `identity.name` ([u8; PROC_NAME_LEN], NUL padded) goes directly
            // into payload tail bytes 40..56, with name_len = bytes before the
            // first NUL (m7_i3 slot) — Rust has no stable raw pointer into a
            // movable table row, and the by-value protocol also removes
            // cross-async pointer-stability concerns.
            let name = table.procs[proc_nr].identity.name;
            let name_len = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            // C: forkexit.c:354 — m.VFS_PM_TERM_SIG = rmp->mp_sigstatus.
            // The signal number comes from sig_proc_exit's write (this
            // function's sig_status argument), not from the lifecycle:
            // EXITING only exists from step 10 on, so a read-back would always
            // yield 0, and VFS's DUMPCORE arm refuses term_sig=0 (BadEndpoint),
            // stalling the exit chain from then on.
            VfsCall::DumpCore {
                endpoint: proc_ep,
                term_sig: sig_status as i32,
                name_len: name_len as u32,
                name,
            }
        } else {
            VfsCall::Exit { endpoint: proc_ep }
        };
        // tell_vfs on exiting slot (utility.c:123-139) → VFS_CALL
        // A failed tell_vfs panics (same shape as utility.c) — losing the
        // death notification to VFS is unacceptable; continuing silently would
        // leave VFS holding the dead process's state forever.
        crate::ipc::tell_vfs(table, slot, call, transport);
    }

    // ---- 9. PRIV_PROC immediate sys_clear (361-369) ----
    // System process (driver) destroyed without waiting for VFS (deadlock avoidance)
    if table.procs[proc_nr].is_kernel_process() {
        // C: forkexit.c:366-368 — failure panics (a terminated process the
        // kernel never reclaims is a permanent leak, hence unrecoverable).
        if let Err(r) = kern.sys_clear(proc_ep) {
            panic!("exit_proc: sys_clear failed: {}", r);
        }
    }

    // ---- 10. Mark EXITING (374-375) — retain IN_USE|VFS_CALL|PRIV_PROC|TRACE_EXIT|PROC_STOPPED
    {
        let proc = &mut table.procs[proc_nr];
        // Preserve VFS_CALL (set by tell_vfs), PROC_STOPPED, TRACE_EXIT, PRIV_PROC, IN_USE
        // In Rust, IN_USE is lifecycle != Unused, VFS_CALL is BlockState, etc.
        // We just set lifecycle to Exiting, keeping other states as is
        // C only sets the EXITING flag and leaves mp_sigstatus alone
        // (forkexit.c:374-375); our sigstatus field lives inside the
        // lifecycle, so construction must carry the caller-provided
        // sig_status (the terminating signal number for signal deaths).
        // Otherwise zombify/wait would report 0 — a signal death would be
        // misreported in the wait status.
        proc.state.lifecycle = Lifecycle::Exiting {
            exit_code: status,
            sig_status,
        };
        // mp_exitstatus char truncation already via status param
    }

    // ---- 11. Zombify if !dump_core (384-385) ----
    if !dump_core {
        zombify(table, slot, transport, kern);
    }

    // ---- 12. Disinherit loop (388-409) ----
    disinherit(table, slot, transport, kern);

    // ---- 13. SIGHUP for session leader (411-412) ----
    if procgrp != 0 {
        // Session leader death -> broadcast SIGHUP to its process group. C
        // reuses check_sig's negative-pid group scan (the mp_procgrp match at
        // signal.c:601-604) with the dying leader itself as caller (so the
        // permission check matches C); the leader is already EXITING, so
        // sig_proc's exit guard skips delivery. C does not check the return
        // value (412).
        //[SD19-gate] PM send/notify/reply results are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::check_sig(
            table,
            slot,
            -procgrp,
            crate::signal::SIGHUP,
            false,
            kern,
            transport,
        );
    }
}

/// Second half of exit: 5 steps (`forkexit.c:418-469`).
///
/// Called after `VFS_PM_EXIT/CORE_REPLY` → `publish_event` → `EventRegistry::resume_event`'s
/// `exit_restart` branch (06). In C, `handle_vfs_reply`'s EXIT branch does
/// `publish_event` then `return` (no tail `restart_sigs`), and `resume_event`'s
/// `Exit` termination calls `exit_restart`.
pub fn exit_restart<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    let scheduler = table.procs[slot.get()].resources.scheduler;
    // 1. sched_stop (forkexit.c:425-441): send SCHEDULING_STOP only when a
    //    user-space scheduler is in charge; a rejection just prints (C's
    //    observable surface) and the process returns to kernel scheduling
    //    as usual.
    if scheduler != Endpoint::KERNEL && scheduler != Endpoint::NONE {
        let mut msg = Message::default();
        msg.m_u.m_lsys_sched_scheduling_stop =
            minix_types::ipc::MessLsysSchedSchedulingStop {
                endpoint: table.procs[slot.get()].endpoint().0,
                _padding: [0; 52],
            };
        msg.m_type = minix_types::SCHEDULING_STOP;
        let rv = match transport.sendrec(scheduler, &mut msg) {
            Ok(()) => msg.m_type,
            Err(_) => -minix_types::EIO,
        };
        if rv != 0 {
            // C forkexit.c:429-433: just printf and continue on to zombify.
            // An unreachable transport on host builds lands here too (-EIO),
            // with identical behavior.
        }
    }
    // 2. scheduler = NONE (441)
    table.procs[slot.get()].resources.scheduler = Endpoint::NONE;

    // 3. Core dump first zombify (444-445) — if not yet ZOMBIE|TRACE_ZOMBIE|TOLD_PARENT
    {
        let lc = table.procs[slot.get()].state.lifecycle;
        match lc {
            Lifecycle::TraceZombie { .. } | Lifecycle::Zombie { .. } | Lifecycle::ToldParent { .. } => {}
            _ => {
                // For dump_core path, this is first zombify
                // For normal path, already zombified, but we check again for safety
                // In Rust we only zombify if currently Exiting
                if matches!(lc, Lifecycle::Exiting { .. }) {
                    zombify(table, slot, transport, kern);
                }
            }
        }
    }

    // 4. sys_clear for !PRIV_PROC (447-452) — user process destroyed after VFS
    // C: forkexit.c:449-451 — failure panics (same unrecoverable semantics as
    // exit_proc step 9).
    if !table.procs[slot.get()].is_kernel_process() {
        let ep = table.procs[slot.get()].endpoint();
        if let Err(r) = kern.sys_clear(ep) {
            panic!("exit_restart: sys_clear failed: {}", r);
        }
    }

    // 5. vm_exit (455-457) — VM free page tables
    // C: `if((r=vm_exit(rmp->mp_endpoint)) != OK) panic("exit_restart:
    // vm_exit failed: %d", r);` — page tables die with the process; if VM
    // never reclaims them they leak permanently.
    {
        let ep = table.procs[slot.get()].endpoint();
        if let Err(r) = crate::ipc::vm_exit(transport, ep) {
            panic!("exit_restart: vm_exit failed: {}", r);
        }
    }

    // 6. TRACE_EXIT → reply(tracer, OK) (459-464, 18-trace.md)
    // C: `mproc[mp_tracer].mp_reply.m_pm_lc_ptrace.data = 0; reply(tracer, OK)`
    // — wake the tracer blocked in ptrace(T_EXIT) and complete that call's
    // reply. Rust counterpart: TRACE_EXIT is modeled as
    // `TraceState::exit_pending` (mproc/trace.rs) and answered directly with
    // an async send via transport.send (same shape as tell_tracer/tell_parent).
    {
        if let Some(tracer_slot) = table.procs[slot.get()]
            .state
            .guardianship
            .tracer()
            && table.procs[slot.get()].state.trace.exit_pending
        {
            let tracer_ep = table.procs[tracer_slot.get()].endpoint();
            let mut reply_msg = Message {
                m_type: minix_types::OK,
                ..Default::default()
            };
            reply_msg.m_u.m_pm_lc_ptrace.data = 0;
            //[SD19-gate] PM send/notify/reply results are fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = transport.send(tracer_ep, &reply_msg);
        }
    }

    // 7. TOLD_PARENT → cleanup (467-468) — parent already reaped
    if matches!(table.procs[slot.get()].state.lifecycle, Lifecycle::ToldParent { .. }) {
        cleanup(table, slot);
    }
}

/// Zombify a process (`forkexit.c:593-624`).
///
/// - `TRACE_ZOMBIE|ZOMBIE` already → panic
/// - `tracer != NO_TRACER && tracer != parent` → `TRACE_ZOMBIE` else `ZOMBIE`
/// - `!wait_test(tracer) → return` else `tell_tracer` + `check_parent(FALSE)`
pub(crate) fn zombify<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    let lc = table.procs[slot.get()].state.lifecycle;
    if matches!(lc, Lifecycle::TraceZombie { .. } | Lifecycle::Zombie { .. }) {
        panic!("zombify: process was already a zombie");
    }
    let (parent, tracer) = {
        let proc = &table.procs[slot.get()];
        (proc.state.guardianship.parent(), proc.state.guardianship.tracer())
    };
    let (exit_code, sig_status) = match table.procs[slot.get()].state.lifecycle {
        Lifecycle::Exiting { exit_code, sig_status } => (exit_code, sig_status),
        _ => (0, 0),
    };

    if let Some(tracer_slot) = tracer
        && tracer_slot != parent {
            table.procs[slot.get()].state.lifecycle = Lifecycle::TraceZombie { exit_code, sig_status };
            // Do not send SIGCHLD signals to tracers (forkexit.c:611-614)
            if !wait_test(table, tracer_slot, slot) {
                // C 613-614: the tracer is not waiting -> return directly and
                // do not notify the real parent — the "tracer first, real
                // parent second" order is upheld by the later chain once the
                // tracer reaps (wait4 reaping walks tell_tracer->check_parent
                // again).
                return;
            }
            tell_tracer(table, slot, transport);
            // The tracer has the death notice; the real parent is handled
            // next (forkexit.c:623)
            check_parent(table, slot, false, transport, kern);
            return;
        }
    table.procs[slot.get()].state.lifecycle = Lifecycle::Zombie { exit_code, sig_status };
    check_parent(table, slot, false, transport, kern);
}

/// Check if parent is waiting and tell or SIGCHLD (`forkexit.c:626-665`).
///
/// `try_cleanup` saves ordering in exit_proc/exit_restart.
pub(crate) fn check_parent<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child_slot: UserSlot,
    try_cleanup: bool,
    transport: &mut T,
    kern: &mut dyn KernelGateway,
) {
    let parent_slot = table.procs[child_slot.get()].state.guardianship.parent();
    if parent_slot.get() >= table.procs.len() {
        return;
    }
    let parent = &table.procs[parent_slot.get()];
    if parent.state.lifecycle.is_exiting() {
        // child of dead parent → INIT will reassigned, do nothing (646-650)
        return;
    }
    if wait_test(table, parent_slot, child_slot) {
        let addr = table.procs[parent_slot.get()].state.wait.rusage_addr;
        let waited = tell_parent(table, child_slot, addr, transport, kern);
        let mut try_cleanup = try_cleanup;
        if !waited {
            try_cleanup = false;
        }
        if try_cleanup && !is_vfs_or_event_blocked(table, child_slot) {
            cleanup(table, child_slot);
        }
    } else {
        // Parent not waiting -> SIGCHLD. C check_parent tail:
        // `sig_proc(p_mp, SIGCHLD, TRUE /*trace*/, FALSE /*ksig*/)` — under
        // the default disposition SIGCHLD is in ign_sset and ignored; a parent
        // with a handler installed gets the notification (the sigframe
        // delivery chain lands with a later batch).
        //[SD19-gate] PM send/notify/reply results are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = child_slot;
        //[SD19-gate] PM send/notify/reply results are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::sig_proc(
            table,
            parent_slot,
            crate::signal::SIGCHLD,
            true,
            false,
            kern,
            transport,
        );
    }
}

/// Tracer died (`forkexit.c:759-790`).
pub fn tracer_died<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child_slot: UserSlot,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
) {
    let old = table.procs[child_slot.get()].state.guardianship.clone();
    table.procs[child_slot.get()].state.guardianship = match old {
        crate::mproc::Guardianship::Traced { parent, .. } => crate::mproc::Guardianship::Normal { parent },
        other => other,
    };
    // TRACE_EXIT cleared (768-769): the Rust counterpart of
    // `mp_flags &= ~TRACE_EXIT` is clearing `TraceState::exit_pending` — the
    // tracer is dead, so nobody waits for the T_EXIT reply anymore.
    table.procs[child_slot.get()].state.trace.exit_pending = false;
    // If !EXITING -> SIGKILL cascade (775-777): the tracer died while the
    // child was still running or stopped, so its state is unknown (C comment
    // "we have no idea what state the child is in"); the only option is to
    // kill it and avoid a trainwreck, which may cascade further exits
    // (C 775-777). C: `sig_proc(child, SIGKILL, TRUE /*trace*/, FALSE /*ksig*/)`.
    if !table.procs[child_slot.get()].state.lifecycle.is_exiting() {
        //[SD19-gate] PM send/notify/reply results are fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::sig_proc(
            table,
            child_slot,
            crate::signal::SIGKILL,
            true,
            false,
            kern,
            transport,
        );
        return;
    }
    // TRACE_ZOMBIE → ZOMBIE + check_parent (784-788)
    if matches!(
        table.procs[child_slot.get()].state.lifecycle,
        Lifecycle::TraceZombie { .. }
    ) {
        let (ec, ss) = table.procs[child_slot.get()].state.lifecycle.exit_code().unwrap();
        table.procs[child_slot.get()].state.lifecycle = Lifecycle::Zombie { exit_code: ec, sig_status: ss };
        check_parent(table, child_slot, true, transport, kern);
    }
}

/// Cleanup: release slot (`forkexit.c:795-806`).
///
/// `mp_pid=0`, `mp_flags=0`, `child_utime/stime=0`, `procs_in_use--`.
/// In Rust: `ProcTable::release_slot` (table.rs:172) does `Process::default()` + `procs_in_use--`.
pub fn cleanup(table: &mut ProcTable, slot: UserSlot) {
    table.release_slot(slot.get());
}

/// Helper: is child blocked on VFS or EVENT (for check_parent try_cleanup)
fn is_vfs_or_event_blocked(table: &ProcTable, slot: UserSlot) -> bool {
    table.procs[slot.get()].state.block.ipc_blocked.is_some()
}

/// Wait test: `wait_test` (`forkexit.c:wait_test` 569-588).
///
/// `parent_waiting && right_child` — the parent must be waiting for **this**
/// child (exact pid or process-group match) before delivery; otherwise the
/// caller takes the SIGCHLD path. The match rule comes from the same source as
/// `do_wait4`'s `pidarg` filtering (`WaitState::is_waiting_for`), so the
/// `pidarg == 0 → -procgrp` normalization must happen at `do_wait4`'s entry,
/// and `is_waiting_for` only consumes an already-normalized `WaitTarget`.
fn wait_test(table: &ProcTable, parent_slot: UserSlot, child_slot: UserSlot) -> bool {
    let parent = &table.procs[parent_slot.get()];
    let child = &table.procs[child_slot.get()];
    parent
        .state
        .wait
        .is_waiting_for(child.identity.id.pid, child.identity.procgrp)
}

/// Tell parent: `tell_parent` (`forkexit.c:670-726`).
///
/// rusage (144 bytes, utime/stime only) is really written into the parent's
/// user memory via `KernelGateway::copy_to_user`, then `reply(parent, pid)`,
/// clearing `WAITING`, `ZOMBIE→TOLD_PARENT`, and merging child times into the
/// parent buckets (722-723).
/// Returns `true` if wait succeeded (for check_parent try_cleanup).
pub(crate) fn tell_parent<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child_slot: UserSlot,
    addr: VirBytes,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
) -> bool {
    let parent_slot = table.procs[child_slot.get()].state.guardianship.parent();
    if parent_slot.get() >= table.procs.len() {
        return false;
    }
    let child_pid = table.procs[child_slot.get()].identity.id.pid;
    let parent_ep = table.procs[parent_slot.get()].endpoint();
    // C: forkexit.c:692-704 — first write the rusage into the parent's user
    // memory via sys_datacopy (only the ru_utime/ru_stime fields, utility.c
    // set_rusage_times); on failure -> reply(parent, errno) + FALSE, and the
    // child stays ZOMBIE so the parent can retry.

    // set_rusage_times (utility.c:144-157): ticks -> usec at system_hz.
    let hz: u64 = u64::from(table.system_hz);
    let (child_utime, child_stime) = {
        let child = &table.procs[child_slot.get()];
        (child.resources.child_utime, child.resources.child_stime)
    };
    let mut rusage = [0u8; 144]; // C: sizeof(struct rusage) x86-64
    {
        let u_usec = (child_utime.max(0) as u64 * 1_000_000) / hz;
        let s_usec = (child_stime.max(0) as u64 * 1_000_000) / hz;
        // ru_utime: tv_sec @0, tv_usec @8; ru_stime: tv_sec @16, tv_usec @24
        rusage[0..8].copy_from_slice(&(u_usec / 1_000_000).to_ne_bytes());
        rusage[8..16].copy_from_slice(&(u_usec % 1_000_000).to_ne_bytes());
        rusage[16..24].copy_from_slice(&(s_usec / 1_000_000).to_ne_bytes());
        rusage[24..32].copy_from_slice(&(s_usec % 1_000_000).to_ne_bytes());
    }
    // C guards the whole rusage copy with `if (addr)` at forkexit.c:692 —
    // rusage_addr==0 from waitpid(pid, NULL, 0) is legal input, and the copy
    // must be skipped in favor of a direct reply. Without that guard,
    // SYS_VIRCOPY against parent address 0x0 makes the kernel suspend PM to
    // ask VM for a page; VM refuses, this function's failure arm sends an
    // errno message to INIT, INIT's DELIVERMSG writes its own stale
    // p_delivermsg_vir=0x0, the memreq fails again, SIGSEGV follows, and the
    // whole system goes down (seen on real hardware: susp-krn mt=15 tgt=0xb
    // st=0x0 ln=0x90).
    if addr.0 != 0 {
        if let Err(r) = kern.copy_to_user(&rusage, parent_ep, addr.0) {
            // datacopy failure: reply(parent, errno) + FALSE
            // (forkexit.c:699-701); the child stays ZOMBIE and the parent can
            // retry the wait.
            //[SD19-gate] PM send/notify/reply results are fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = transport.send(
                parent_ep,
                &Message {
                    m_type: r,
                    ..Default::default()
                },
            );
            return false;
        }
    }

    // C: forkexit.c:707-709 — the status goes into
    // mp_reply.m_pm_lc_wait4.status (payload, per the wire contract) and the
    // pid is the return value of reply(parent, pid).
    let (ec, ss) = table.procs[child_slot.get()].state.lifecycle.exit_code().unwrap_or((0, 0));
    let mut reply_msg = Message {
        m_type: child_pid,
        ..Default::default()
    };
    // C forkexit.c:713 does not mask: `mp_exitstatus`/`mp_sigstatus` are
    // `char`, and integer promotion sign-extends (sigstatus is negative when
    // WCOREFLAG is set). Masking would make the raw status value differ from
    // C — libc macros (WIFSIGNALED/WEXITSTATUS) hide the difference, but the
    // integer on the wire is externally observable.
    reply_msg.m_u.m_pm_lc_wait4.status = crate::wait::w_exitcode(ec as i32, ss as i32);
    //[SD19-gate] PM send/notify/reply results are fire-and-forget
    #[allow(clippy::let_underscore_must_use)]
    let _ = transport.send(parent_ep, &reply_msg);

    table.procs[parent_slot.get()].state.wait.waiting = false;
    // ZOMBIE → TOLD_PARENT
    table.procs[child_slot.get()].state.lifecycle = Lifecycle::ToldParent { exit_code: ec, sig_status: ss };
    // Accumulate child times at parent (forkexit.c:722-723)
    table.procs[parent_slot.get()].resources.child_utime += child_utime;
    table.procs[parent_slot.get()].resources.child_stime += child_stime;

    true
}

/// Tell tracer: `tell_tracer` (`forkexit.c:732-754`).
pub(crate) fn tell_tracer<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    child_slot: UserSlot,
    transport: &mut T,
) {
    let tracer_slot = table.procs[child_slot.get()]
        .state
        .guardianship
        .tracer()
        .expect("tracer must exist");
    let child_pid = table.procs[child_slot.get()].identity.id.pid;
    // C: forkexit.c:748-749 — `tracer->mp_reply.m_pm_lc_wait4.status =
    // W_EXITCODE(ec, sigstatus & 0377)` followed by `reply(tracer, pid)`
    // (payload per the wire contract).
    let (ec, ss) = table.procs[child_slot.get()].state.lifecycle.exit_code().unwrap();
    let mut reply_msg = Message {
        m_type: child_pid,
        ..Default::default()
    };
    reply_msg.m_u.m_pm_lc_wait4.status =
        crate::wait::w_exitcode(ec as i32, (ss as i32) & 0o377); // C forkexit.c:749 masks sigstatus explicitly
    let tracer_ep = table.procs[tracer_slot.get()].endpoint();
    //[SD19-gate] PM send/notify/reply results are fire-and-forget
    #[allow(clippy::let_underscore_must_use)]
    let _ = transport.send(tracer_ep, &reply_msg);

    table.procs[tracer_slot.get()].state.wait.waiting = false;
    // TRACE_ZOMBIE → ZOMBIE (now zombie to parent)
    table.procs[child_slot.get()].state.lifecycle = Lifecycle::Zombie { exit_code: ec, sig_status: ss };
}

/// Disinherit loop: `for rmp=0..NR_PROCS` (`forkexit.c:388-409`).
///
/// - `tracer == proc_nr → tracer_died`
/// - `parent == proc_nr → parent = INIT_PROC_NR + VFS_CALL→NEW_PARENT + ZOMBIE→check_parent`
///
/// C's `SIGHUP` broadcast (411-412) happens after the loop but still inside
/// `exit_proc`, and is done by the caller.
fn disinherit<T: crate::ipc::IpcTransport + ?Sized>(
    table: &mut ProcTable,
    exiting_slot: UserSlot,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
) {
    let proc_nr = exiting_slot.get();
    // Collect affected slots first to avoid borrow conflicts
    let mut to_adopt = Vec::new();
    let mut tracer_died_slots = Vec::new();
    for (idx, proc) in table.procs.iter().enumerate() {
        if !proc.is_in_use() {
            continue;
        }
        if proc.state.guardianship.tracer() == Some(UserSlot::new(proc_nr)) {
            tracer_died_slots.push(idx);
        }
        if proc.state.guardianship.parent() == UserSlot::new(proc_nr) {
            to_adopt.push(idx);
        }
    }
    for idx in tracer_died_slots {
        tracer_died(table, UserSlot::new(idx), transport, kern);
    }
    for idx in to_adopt {
        let child_slot = UserSlot::new(idx);
        // Adopt to INIT
        {
            let child = &mut table.procs[idx];
            child.state.guardianship = match child.state.guardianship {
                crate::mproc::Guardianship::Normal { .. } => crate::mproc::Guardianship::Normal {
                    parent: UserSlot::new(11), // INIT_PROC_NR
                },
                crate::mproc::Guardianship::Traced { tracer, trace_options, .. } => crate::mproc::Guardianship::Traced {
                    parent: UserSlot::new(11),
                    tracer,
                    trace_options,
                },
            };
            if child.state.block.ipc_blocked.is_some() {
                // VFS_CALL → NEW_PARENT (block.rs:52)
                if let Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent }) =
                    child.state.block.ipc_blocked
                {
                    //[SD19-gate] PM send/notify/reply results are fire-and-forget
                    #[allow(clippy::let_underscore_must_use)]
                    let _ = reply_to_new_parent;
                }
                // For test we set reply_to_new_parent true via VfsCall
                if let Some(crate::mproc::IpcBlockReason::VfsCall { .. }) = child.state.block.ipc_blocked {
                    child.state.block.ipc_blocked =
                        Some(crate::mproc::IpcBlockReason::VfsCall { reply_to_new_parent: true });
                }
            }
        }
        // If already ZOMBIE, check_parent for INIT (C 406-407 checks only the
        // `ZOMBIE` bit: a TraceZombie child does not notify INIT when its
        // parent dies — its death notice goes to the tracer first and is
        // handled by the tell_tracer->check_parent chain after the tracer reaps).
        if matches!(table.procs[idx].state.lifecycle, Lifecycle::Zombie { .. }) {
            check_parent(table, child_slot, true, transport, kern);
        }
    }
    // SIGHUP is not in this function: C's 411-412 run after the disinherit
    // loop (still inside exit_proc's body) and are done by exit_proc's tail.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::{ProcTable, Lifecycle, Guardianship, Privilege, Credentials};
    use minix_types::{Endpoint, UserSlot};
    use crate::mproc::IpcBlockReason;

    fn running_proc(table: &mut ProcTable, slot: usize, pid: i32) {
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.id.pid = pid;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        table.procs[slot].identity.procgrp = pid;
    }


    /// Session leader death -> process-group SIGHUP broadcast
    /// (forkexit.c:411-412, check_sig(-procgrp) negative-pid group scan).
    /// Group members die under the default disposition, out-of-group processes
    /// survive; the leader is already EXITING, so sig_proc's exit guard skips
    /// duplicate delivery.
    #[test]
    fn test_session_leader_death_broadcasts_sighup() {
        let mut table = ProcTable::new();
        // Leader at slot 1: pid == procgrp == 100 (session leader).
        running_proc(&mut table, 1, 100);
        // Group member at slot 2; out-of-group process at slot 3.
        running_proc(&mut table, 2, 101);
        table.procs[2].identity.procgrp = 100;
        running_proc(&mut table, 3, 102);
        table.procs[3].identity.procgrp = 200;

        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        do_exit(&mut table, UserSlot::new(1), 0, &mut t, &mut kern);

        // The group member is terminated by the default SIGHUP disposition
        // (EXITING -> zombify).
        assert!(
            table.procs[2].is_exiting()
                || matches!(
                    table.procs[2].state.lifecycle,
                    Lifecycle::Zombie { .. } | Lifecycle::ToldParent { .. }
                ),
            "group member must be terminated by SIGHUP, got {:?}",
            table.procs[2].state.lifecycle
        );
        // The out-of-group process survives.
        assert!(table.procs[3].is_in_use() && !table.procs[3].is_exiting());
    }

    /// When the parent is not waiting, check_parent delivers SIGCHLD to it
    /// (C check_parent tail: sig_proc(p_mp, SIGCHLD, TRUE, FALSE)). The signal
    /// is masked so delivery is observable: the pending bit gets set and the
    /// parent does not exit.
    #[test]
    fn test_check_parent_sends_sigchld_when_parent_not_waiting() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // parent: not waiting
        running_proc(&mut table, 2, 101);
        table.procs[2].state.guardianship = Guardianship::Normal { parent: UserSlot::new(1) };
        table.procs[2].state.lifecycle = Lifecycle::Zombie { exit_code: 0, sig_status: 0 };
        // The parent blocks SIGCHLD, so delivery lands in pending (observable).
        table.procs[1].resources.signals.mask = crate::init::sig_bit(crate::signal::SIGCHLD);

        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        check_parent(&mut table, UserSlot::new(2), false, &mut t, &mut kern);

        assert!(
            table.procs[1].resources.signals.pending & crate::init::sig_bit(crate::signal::SIGCHLD) != 0,
            "SIGCHLD must be pending on the blocked parent"
        );
        assert!(table.procs[1].is_in_use() && !table.procs[1].is_exiting());
    }

    /// `wait_test`'s `right_child` (`forkexit.c:wait_test` 583-587): when the
    /// parent waits for a **different** child (`SpecificChild`), another
    /// child's exit must not be delivered — C takes the SIGCHLD path and that
    /// child stays ZOMBIE until the real parent reaps it. Regression guard for
    /// a placeholder that delivered whenever the parent was waiting at all.
    #[test]
    fn test_check_parent_waits_for_matching_child_only() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // parent: waiting for pid 999
        running_proc(&mut table, 2, 101); // exiting child: pid 101 != 999
        table.procs[2].state.guardianship = Guardianship::Normal { parent: UserSlot::new(1) };
        table.procs[2].state.lifecycle = Lifecycle::Zombie { exit_code: 0, sig_status: 0 };
        table.procs[1].state.wait.waiting = true;
        table.procs[1].state.wait.target = crate::mproc::WaitTarget::SpecificChild(999);
        table.procs[1].resources.signals.mask = crate::init::sig_bit(crate::signal::SIGCHLD);

        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        check_parent(&mut table, UserSlot::new(2), true, &mut t, &mut kern);

        assert!(
            matches!(table.procs[2].state.lifecycle, Lifecycle::Zombie { .. }),
            "child must stay Zombie: parent waits for another pid"
        );
        assert!(table.procs[1].state.wait.waiting, "parent keeps waiting");
        assert!(
            table.procs[1].resources.signals.pending & crate::init::sig_bit(crate::signal::SIGCHLD) != 0,
            "SIGCHLD must be delivered instead of the wait4 reply"
        );
        assert!(t.sent().is_empty(), "no wait4 reply may be sent");
    }

    /// Gateway mock that records sys_kill calls.
    #[derive(Default)]
    struct KillRecorder {
        killed: Option<(Endpoint, i32)>,
        copied_bytes: Option<alloc::vec::Vec<u8>>,
    }
    impl KernelGateway for KillRecorder {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, ep: Endpoint, sig: i32) -> Result<(), i32> {
            self.killed = Some((ep, sig));
            Ok(())
        }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> {
            Ok(())
        }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
            Ok((30, 12)) // scripted accounting values (used to verify accumulation)
        }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_user(&mut self, bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
            self.copied_bytes = Some(bytes.to_vec());
            Ok(())
        }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> {
            Ok(())
        }
        fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
    }

    #[test]
    fn test_do_exit_priv_proc() {
        let mut table = ProcTable::new();
        table.procs[0].state.lifecycle = Lifecycle::Running;
        table.procs[0].identity.endpoint = Endpoint::from_generation_slot(1, 0);
        table.procs[0].resources.privilege = Privilege::Kernel(Credentials::default());
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        let intent = do_exit(&mut table, UserSlot::new(0), 0, &mut transport, &mut kern);
        assert_eq!(intent, ReplyIntent::NoReply);
        // Priv proc should not become Exiting via exit_proc
        assert!(matches!(table.procs[0].state.lifecycle, Lifecycle::Running));
        // A violating exit goes through sys_kill(endpoint, SIGKILL) and is
        // left to the kernel signal path
        assert_eq!(kern.killed, Some((Endpoint::from_generation_slot(1, 0), crate::signal::SIGKILL)));
    }

    #[test]
    fn test_do_exit_user_process_skips_sys_kill() {
        // C: forkexit.c:258-260 — a non-PRIV_PROC process goes through
        // exit_proc and never touches sys_kill (sys_kill is exclusive to the
        // PRIV_PROC violation branch).
        let mut table = ProcTable::new();
        table.procs[5].state.lifecycle = Lifecycle::Running;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(1, 5);
        table.procs[5].identity.id.pid = 200;
        table.procs[5].resources.privilege = Privilege::User(Credentials::new(1000, 100));
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();

        let intent = do_exit(&mut table, UserSlot::new(5), 0, &mut transport, &mut kern);

        assert_eq!(intent, ReplyIntent::NoReply);
        assert!(kern.killed.is_none(), "user exit must not go through sys_kill");
        // After exit_proc finishes zombify: the parent is not waiting -> Zombie
        // (not ToldParent).
        assert!(matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { .. }));
    }

    #[test]
    fn test_tell_parent_delivers_rusage_via_datacopy() {
        // tell_parent writes the 144-byte rusage into the parent's user
        // memory via VIRCOPY (forkexit.c:692-704); only the two timevals
        // ru_utime/ru_stime carry values (set_rusage_times,
        // utility.c:144-157). A datacopy failure -> reply(parent, errno) +
        // FALSE, and the child stays ZOMBIE.
        let mut table = ProcTable::new();
        table.system_hz = 100; // explicit hz, isolates the assertions
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        // Child buckets: (30 ticks, 12 ticks) @ hz=100 -> (300000 usec, 120000 usec)
        table.procs[5].resources.child_utime = 30;
        table.procs[5].resources.child_stime = 12;
        table.procs[5].state.lifecycle = Lifecycle::Zombie { exit_code: 7, sig_status: 0 };
        let addr = VirBytes(0x7000);

        let told = tell_parent(&mut table, UserSlot::new(5), addr, &mut transport, &mut kern);

        assert!(told);
        let copied = kern.copied_bytes.as_ref().expect("rusage must be datacopied");
        assert_eq!(copied.len(), 144);
        let u_sec = u64::from_ne_bytes(copied[0..8].try_into().unwrap());
        let u_usec = u64::from_ne_bytes(copied[8..16].try_into().unwrap());
        let s_sec = u64::from_ne_bytes(copied[16..24].try_into().unwrap());
        let s_usec = u64::from_ne_bytes(copied[24..32].try_into().unwrap());
        assert_eq!((u_sec, u_usec), (0, 300000));
        assert_eq!((s_sec, s_usec), (0, 120000));
        // The other 112 bytes stay zero (C likewise memsets and fills only the
        // two fields)
        assert!(copied[32..].iter().all(|&b| b == 0));
    }

    #[test]
    fn test_tell_parent_null_rusage_addr_skips_datacopy() {
        // C forkexit.c:692 has an `if (addr)` guard — with
        // waitpid(pid, NULL, 0), rusage_addr==0 is legal input and the rusage
        // copy must be skipped for a direct reply. Sending VIRCOPY to 0x0
        // instead makes the kernel suspend PM to ask VM for a page; the
        // failure arm then sends an errno message to INIT, INIT's DELIVERMSG
        // writes the stale pdmv=0, and SIGSEGV brings the whole system down.
        let mut table = ProcTable::new();
        table.system_hz = 100;
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        table.procs[5].resources.child_utime = 30;
        table.procs[5].resources.child_stime = 12;
        table.procs[5].state.lifecycle = Lifecycle::Zombie {
            exit_code: 7,
            sig_status: 0,
        };

        let told = tell_parent(
            &mut table,
            UserSlot::new(5),
            VirBytes(0),
            &mut transport,
            &mut kern,
        );

        assert!(told, "NULL rusage must still tell parent");
        assert!(
            kern.copied_bytes.is_none(),
            "addr==0 must not issue any datacopy (C: if (addr) guard, forkexit.c:692)"
        );
        // The parent receives reply(pid) and the exit-status payload as usual
        // (C: forkexit.c:707-709).
        let pid = table.procs[5].identity.id.pid;
        assert!(
            transport.sent().iter().any(|(_, m)| m.m_type == pid),
            "parent must receive the pid reply"
        );
    }

    #[test]
    fn test_exit_proc_accumulates_sys_times() {
        // exit_proc reads the dying process's own CPU ticks and accumulates
        // them into its child bucket (forkexit.c:305-310); they merge into the
        // parent on wait (tell_parent 722-723). KillRecorder's proc_times
        // scripted value = (30, 12).
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        // KillRecorder's proc_times always returns (30, 12) (scripted
        // accounting values).

        exit_proc(
            &mut table,
            UserSlot::new(5),
            0,
            0,
            false,
            &mut transport,
            &mut kern,
        );

        // The parent does not wait -> the Zombie holds the buckets; they merge
        // into the parent on wait.
        assert_eq!(table.procs[5].resources.child_utime, 30);
        assert_eq!(table.procs[5].resources.child_stime, 12);
    }

    #[test]
    fn test_exit_proc_normal() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_proc(
            &mut table,
            UserSlot::new(5),
            0,
            0,
            false,
            &mut transport,
            &mut kern,
        );
        assert!(matches!(
            table.procs[5].state.lifecycle,
            Lifecycle::Zombie { .. } | Lifecycle::TraceZombie { .. } | Lifecycle::Exiting { .. }
        ));
        // VFS_CALL should be set (tell_vfs)
        assert!(table.procs[5].state.block.is_vfs_blocked());
    }

    #[test]
    fn test_exit_proc_dump_core_suppressed_for_priv() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        table.procs[5].resources.privilege = Privilege::Kernel(Credentials::default());
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_proc(
            &mut table,
            UserSlot::new(5),
            0,
            0,
            true,
            &mut transport,
            &mut kern,
        );
        // dump_core is suppressed for PRIV_PROC, so should still be Zombie not waiting for core
        // In C: dump_core && PRIV_PROC → FALSE, so !dump_core → zombify
        assert!(table.procs[5].state.lifecycle.is_zombie() || matches!(table.procs[5].state.lifecycle, Lifecycle::TraceZombie { .. }));
    }

    #[test]
    fn test_zombify_trace_zombie() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
                        trace_options: crate::mproc::TraceOptions::empty(),
        };
        // tracer at 2 is NOT waiting → stays TraceZombie (wait_test false → return)
        table.procs[2].state.lifecycle = Lifecycle::Running;
        table.procs[2].state.wait.waiting = false;
        table.procs[2].identity.endpoint = Endpoint::from_generation_slot(1, 2);
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern_rec = KillRecorder::default();
        zombify(&mut table, UserSlot::new(5), &mut t, &mut kern_rec);
        assert!(matches!(
            table.procs[5].state.lifecycle,
            Lifecycle::TraceZombie { .. }
        ));
    }

    #[test]
    fn test_disinherit_new_parent() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 10, 100);
        table.procs[10].state.lifecycle = Lifecycle::Running;
        // child at 11 with parent 10 and VFS_CALL
        running_proc(&mut table, 11, 101);
        table.procs[11].state.guardianship = Guardianship::Normal { parent: UserSlot::new(10) };
        table.procs[11].state.block.ipc_blocked = Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        table.procs[10].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        disinherit(&mut table, UserSlot::new(10), &mut t, &mut kern);
        assert_eq!(table.procs[11].state.guardianship.parent(), UserSlot::new(11));
        assert!(matches!(
            table.procs[11].state.block.ipc_blocked,
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: true })
        ));
    }

    /// `disinherit` changes only `mp_parent` (and `VFS_CALL → NEW_PARENT`);
    /// the `TO_*` option bits in `mp_trace_flags` are untouched
    /// (`forkexit.c:394-408`). A traced child adopted by `INIT` after its
    /// parent dies is still traced by the original tracer, so the trace
    /// options must survive verbatim; otherwise auto-attach/exec behavior
    /// configured by `T_SETOPT` would be lost at adoption.
    #[test]
    fn test_disinherit_preserves_trace_options() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 10, 100);
        table.procs[10].state.lifecycle = Lifecycle::Exiting { exit_code: 0, sig_status: 0 };
        running_proc(&mut table, 12, 102);
        table.procs[12].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(10),
            tracer: UserSlot::new(13),
            trace_options: crate::mproc::TraceOptions::TRACEFORK
                | crate::mproc::TraceOptions::ALTEXEC,
        };
        table.procs[12].state.block.ipc_blocked = Some(IpcBlockReason::VfsCall {
            reply_to_new_parent: false,
        });
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        disinherit(&mut table, UserSlot::new(10), &mut t, &mut kern);
        assert_eq!(table.procs[12].state.guardianship.parent(), UserSlot::new(11));
        assert_eq!(
            table.procs[12].state.guardianship.tracer(),
            Some(UserSlot::new(13))
        );
        assert_eq!(
            table.procs[12].state.guardianship.trace_options(),
            crate::mproc::TraceOptions::TRACEFORK | crate::mproc::TraceOptions::ALTEXEC
        );
    }

    #[test]
    fn test_cleanup_releases_slot() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        table.procs_in_use.set(1);
        table.procs[5].state.lifecycle = Lifecycle::ToldParent { exit_code: 0, sig_status: 0 };
        cleanup(&mut table, UserSlot::new(5));
        assert!(!table.procs[5].is_in_use());
        assert_eq!(table.procs_in_use.get(), 0);
    }

    #[test]
    fn test_exit_restart_cleans_priv() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 5, 42);
        table.procs[5].state.lifecycle = Lifecycle::ToldParent { exit_code: 0, sig_status: 0 };
        table.procs[5].resources.scheduler = Endpoint::SCHED;
        table.procs_in_use.set(1);
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_restart(&mut table, UserSlot::new(5), &mut transport, &mut kern);
        // For !PRIV_PROC, sys_clear + vm_exit would be called (stubbed), and TOLD_PARENT → cleanup
        // So slot should be released
        assert!(!table.procs[5].is_in_use());
    }

    /// Regression guard (forkexit.c:613-614): when the tracer is not waiting,
    /// `zombify` returns directly (C forkexit.c:613-614) and must not notify
    /// the real parent ahead of the tracer — the real parent's SIGCHLD must
    /// stay undelivered (masking the signal makes delivery observable).
    #[test]
    fn test_zombify_tracer_not_waiting_defers_parent() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // real parent: Running
        running_proc(&mut table, 2, 101); // tracer: not waiting
        table.procs[1].resources.signals.mask = crate::init::sig_bit(crate::signal::SIGCHLD);
        running_proc(&mut table, 5, 105);
        table.procs[5].state.lifecycle = Lifecycle::Exiting { exit_code: 7, sig_status: 0 };
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        zombify(&mut table, UserSlot::new(5), &mut t, &mut kern);
        assert!(matches!(table.procs[5].state.lifecycle, Lifecycle::TraceZombie { .. }));
        assert_eq!(
            table.procs[1].resources.signals.pending & crate::init::sig_bit(crate::signal::SIGCHLD),
            0,
            "真父不得在 tracer 之前收到死讯（C 613-614 先序）"
        );
        assert!(t.sent().is_empty(), "tracer 未等待时不应有任何异步回复发出");
    }

    /// Regression guard: `exit_restart` step 6 answers TRACE_EXIT
    /// (`TraceState::exit_pending`) with `reply(tracer, OK)` and
    /// `m_pm_lc_ptrace.data = 0` (C forkexit.c:459-464), waking the tracer
    /// blocked in ptrace(T_EXIT).
    #[test]
    fn test_exit_restart_trace_exit_replies_tracer() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // real parent
        running_proc(&mut table, 2, 101); // tracer
        running_proc(&mut table, 5, 105);
        table.procs[5].state.lifecycle = Lifecycle::ToldParent { exit_code: 0, sig_status: 0 };
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        table.procs[5].state.trace.exit_pending = true;
        let mut transport = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        exit_restart(&mut table, UserSlot::new(5), &mut transport, &mut kern);
        let tracer_ep = table.procs[2].identity.endpoint;
        let reply = transport
            .sent()
            .iter()
            .find(|(dst, _)| *dst == tracer_ep)
            .expect("必须向 tracer 发出回复");
        assert_eq!(reply.1.m_type, minix_types::OK, "C: reply(tracer, OK)");
        // SAFETY: reading Message union member m_pm_lc_ptrace.data (i64) — all
        // members are initialized and readable (the same member was just
        // written before sending); same approach as the trace.rs tests.
        assert_eq!(
            unsafe { reply.1.m_u.m_pm_lc_ptrace.data },
            0,
            "C: m_pm_lc_ptrace.data = 0"
        );
    }

    /// Regression guard (forkexit.c:775-777): the tracer died while the child
    /// was still running -> SIGKILL cascade (C forkexit.c:775-777); the
    /// TRACE_EXIT bit is also cleared (C 768-769). The child dies under the
    /// default disposition (observed the same way as in the SIGHUP test).
    #[test]
    fn test_tracer_died_kills_running_child() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100); // real parent
        running_proc(&mut table, 2, 101); // tracer (already dead; guaranteed by the caller)
        running_proc(&mut table, 5, 105); // traced child: still Running
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        table.procs[5].state.trace.exit_pending = true;
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        tracer_died(&mut table, UserSlot::new(5), &mut t, &mut kern);
        assert!(!table.procs[5].state.trace.exit_pending, "C 768-769：TRACE_EXIT 摘位");
        assert_eq!(
            table.procs[5].state.guardianship.tracer(),
            None,
            "tracer 死亡后监护回到 Normal"
        );
        assert!(
            table.procs[5].is_exiting(),
            "SIGKILL 默认处置必须终止还在运行的被跟踪子（C 775-777）"
        );
    }

    /// Regression guard (forkexit.c:784-788): the tracer died mid-death-report
    /// (TRACE_ZOMBIE) -> downgrade to ZOMBIE and retry check_parent for the
    /// real parent (C 784-788); SIGKILL must not be sent by mistake.
    #[test]
    fn test_tracer_died_trace_zombie_becomes_zombie() {
        let mut table = ProcTable::new();
        running_proc(&mut table, 1, 100);
        running_proc(&mut table, 2, 101);
        running_proc(&mut table, 5, 105);
        table.procs[1].resources.signals.mask = crate::init::sig_bit(crate::signal::SIGCHLD);
        table.procs[5].state.lifecycle = Lifecycle::TraceZombie { exit_code: 3, sig_status: 0 };
        table.procs[5].state.guardianship = Guardianship::Traced {
            parent: UserSlot::new(1),
            tracer: UserSlot::new(2),
            trace_options: crate::mproc::TraceOptions::empty(),
        };
        let mut t = crate::ipc::TestIpcTransport::default();
        let mut kern = KillRecorder::default();
        tracer_died(&mut table, UserSlot::new(5), &mut t, &mut kern);
        assert!(
            matches!(table.procs[5].state.lifecycle, Lifecycle::Zombie { exit_code: 3, sig_status: 0 }),
            "TRACE_ZOMBIE → ZOMBIE（C 784-788）"
        );
        assert_ne!(
            table.procs[1].resources.signals.pending & crate::init::sig_bit(crate::signal::SIGCHLD),
            0,
            "降级后真父未等待 → SIGCHLD 重试投递"
        );
    }
}
