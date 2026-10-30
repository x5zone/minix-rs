//! PM ↔ VFS async protocol: request send (`tell_vfs`) and the reply state
//! machine (`handle_vfs_reply`).
//!
//! C ground truth:
//! - `minix3/minix/servers/pm/utility.c:120-139` — `tell_vfs` (sets VFS_CALL,
//!   sends asynchronously)
//! - `minix3/minix/servers/pm/main.c:294-424` — `handle_vfs_reply` (11-way
//!   reply state machine)
//!
//! Design contract: `rewrite-notes/04-stage-pm/05-vfs-interaction.md`.
//!
//! # Contrast with the microkernel event model
//!
//! Minix3's PM is a **single-threaded event loop** (established by
//! `rewrite-notes/04-stage-pm/04-ipc-dispatch.md`: a blocking handler stalls the
//! whole server). The requests it sends to VFS (exec image loading, fork fd
//! copy, setuid, exit, ...) are slow and unpredictable (disk reads and writes,
//! waiting on tty/pipe). Waiting synchronously would stall PM; and if VFS then
//! sent a message back to PM via `ipc_send(PM_PROC_NR, ...)` (blocking), both
//! sides would wait on each other → deadlock. PM therefore "posts" the request
//! with `asynsend3` (non-blocking, `AMF_NOREPLY`) and returns to the main loop
//! at once; VFS, once done (possibly via a worker thread, see the comment in
//! `vfs/main.c:895-899`), sends the reply asynchronously, and PM collects it in
//! the first branch of the main loop (`main.c:84-87`).
//!
//! This is isomorphic to Redox's `Scheme` model in shape but not in unit:
//! - **Same shape**: after the request is "posted" the caller does not get a
//!   result immediately; the callee replies "later".
//! - **Different unit**: Redox hangs deferred completion off a `handle` table
//!   (the kernel suspends the caller, `doc.redox-os.org/
//!   book/scheme-operation.html`); its unit is the *scheme handle*. Minix3
//!   hangs the continuation off flags on the *process* (`mproc`); its unit is
//!   the *process*, because PM's requests are all process-state transitions.
//!   The Rust side keeps the "per process" semantics but lifts C's implicit
//!   flag-bit continuation into an explicit port call
//!   ([`VfsReplyServices`]), eliminating the state inference scattered across
//!   four sites in `main.c`/`event.c`/`signal.c` and backed by `panic` (an
//!   extension of the explicit-port design).


use minix_types::{
    Endpoint, Message, Pid, UserSlot, VirBytes, VfsCall, VfsReply, VfsReplyError,
    VFS_PM_REBOOT_REPLY,
};

use crate::ipc::transport::IpcTransport;
use crate::mproc::{IpcBlockReason, ProcTable};

/// Minix3 success code (`minix3/sys/sys/errno.h:190` `#define OK 0`).
pub const OK: i32 = 0;

/// Core dump flag bit (`minix3/sys/sys/wait.h:63`, value `0200`).
///
/// Set only on a successful `VfsReply::Core` (main.c:357-358); see
/// [`VfsReplyServices::set_core_flag`].
#[allow(dead_code)]
const WCOREFLAG: i32 = 0o200;

/// The send error of `tell_vfs`.
///
/// Process event type (used by [`VfsReplyServices::publish_event`]).
///
/// Corresponds to the two events C infers from the `EXITING` / `UNPAUSED`
/// flags in `event.c:86-91`; here the inference moves up into the state
/// machine (the call sites of [`VfsReplyServices`]), removing the
/// `panic("unknown event for flags")` inside `publish_event`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcEvent {
    /// Process exit event (EXITING set).
    Exit,
    /// Signal/unpause event (UNPAUSED set).
    Signal,
}

/// Arguments of `exec_restart` (from `VFS_PM_EXEC_REPLY`, main.c:350-353).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecRestartArgs {
    /// Execution status (OK or failure, `VFS_PM_STATUS = m7_i2`).
    pub status: i32,
    /// Program counter (`VFS_PM_PC = m7_p1`).
    pub pc: u64,
    /// Possibly updated user stack pointer (`VFS_PM_NEWSP = m7_p2`).
    pub newsp: u64,
    /// Possibly updated ps_strings pointer (`VFS_PM_NEWPS_STR`). Full 64 bits
    /// under LP64 (an earlier `i32` truncation plus a downstream `as u64`
    /// sign extension would destroy the x86-64 user ps_strings address).
    pub newps_str: u64,
}

/// Effect port for PM's replies to VFS (hexagonal architecture: the state
/// machine is pure logic, effects are applied through this port).
///
/// Read methods let the state machine branch; effect methods apply at once.
/// Two implementations:
/// - [`PmServices`]: production (some methods are `unimplemented!` until their
///   design lands).
/// - [`RecordingServices`]: test implementation (records the call sequence for
///   assertions).
pub trait VfsReplyServices {
    // ── Reads (state-machine decisions) ──

    /// Resolve the reply's endpoint to a slot (C: `pm_isokendpt`, main.c:317-319).
    ///
    /// `None` makes the state machine produce [`VfsReplyError::BadEndpoint`].
    fn slot_of_endpoint(&self, endpoint: Endpoint) -> Option<UserSlot>;

    /// Extract and clear `VFS_CALL | NEW_PARENT` (main.c:327-328).
    ///
    /// Returns the old `NEW_PARENT`; panics if `VFS_CALL` is clear (main.c:324-325
    /// "reply without request"). Clearing must happen after reading and before
    /// use, matching C order.
    fn take_vfs_call(&mut self, slot: UserSlot) -> bool;

    /// Whether `UNPAUSED` is set on entry (main.c:330-331 entry invariant).
    fn is_unpaused(&self, slot: UserSlot) -> bool;

    /// Whether the process is exiting (for the tail `restart_sigs` condition, main.c:422).
    fn is_exiting(&self, slot: UserSlot) -> bool;

    /// Whether the process is in use (for the tail `restart_sigs` condition, main.c:422).
    fn is_in_use(&self, slot: UserSlot) -> bool;

    /// Process group number (SETSID reply code, main.c:345 → `reply(caller, mp_procgrp)`).
    fn procgrp_of(&self, slot: UserSlot) -> Pid;

    /// Process PID (reply code when a FORK success reply goes to the parent, main.c:393 → `rmp->mp_pid`).
    fn pid_of(&self, slot: UserSlot) -> Pid;

    /// Parent process slot (used inside `reply_to_guardian`).
    fn parent_slot(&self, slot: UserSlot) -> UserSlot;

    // ── Effects (applied immediately) ──

    /// Reply to the process itself (main.c:339/345/389).
    fn reply(&mut self, slot: UserSlot, code: i32);

    /// Reply to the parent; suppressed when `suppress` (`new_parent` true) (main.c:384/392).
    fn reply_to_guardian(&mut self, slot: UserSlot, suppress: bool, code: i32);

    /// Start scheduling the new process (main.c:373). `Err` means scheduling failed (main.c:378-386).
    fn sched_start_user(&mut self, slot: UserSlot) -> Result<(), i32>;

    /// Detach the child's scheduler ownership (set `scheduler = NONE`), main.c:380
    /// `rmp->mp_scheduler = NONE; /* don't try to stop scheduling */`.
    /// Called only on the fork teardown leg where `sched_start_user` failed:
    /// the child never registered with the scheduler, so the later
    /// `exit_restart` sched_stop gate (forkexit.c:425 / exit.rs:499) must not
    /// send it `SCHEDULING_STOP` (otherwise the scheduler receives a stop
    /// request for an unknown process). NONE must survive until the
    /// `exit_restart` after the VFS EXIT reply, so it must be applied before
    /// `exit_proc`, matching C statement order.
    fn detach_scheduler(&mut self, slot: UserSlot);

    /// Advance the process exit (main.c:381/379 → `exit_proc(rmp, -1, FALSE)` or the CORE/EXIT path).
    fn exit_proc(&mut self, slot: UserSlot, status: i32, dump_core: bool);

    /// Set `WCOREFLAG` (only a `Core` reply with status==OK, main.c:357-358).
    fn set_core_flag(&mut self, slot: UserSlot);

    /// Assert the process is exiting (main.c:362 `assert(mp_flags & EXITING)`, EXIT/CORE path).
    fn assert_exiting(&mut self, slot: UserSlot);

    /// Assert the process is stopped (main.c:407 `assert(mp_flags & PROC_STOPPED)`, UNPAUSE path).
    fn assert_stopped(&mut self, slot: UserSlot);

    /// Publish a process event (main.c:365/413 `publish_event`; the concrete
    /// subscriptions live in `rewrite-notes/04-stage-pm/06-event-subscription.md`).
    fn publish_event(&mut self, slot: UserSlot, event: ProcEvent);

    /// Restart execution (main.c:350 `exec_restart`, implemented by
    /// `rewrite-notes/04-stage-pm/17-exec.md`).
    fn exec_restart(&mut self, slot: UserSlot, args: ExecRestartArgs);

    /// Re-post pending signals (tail, main.c:422-423 `restart_sigs`).
    fn restart_signals(&mut self, slot: UserSlot);

    /// Set `UNPAUSED` (main.c:410, UNPAUSE path).
    fn set_unpaused(&mut self, slot: UserSlot);

    /// Ask the kernel to abort (main.c:309 `sys_abort(abort_flag)`, REBOOT special case).
    fn sys_abort(&mut self);
}

/// Send a VFS request (C: `utility.c:120-139` `tell_vfs`).
///
/// Order matches C **exactly**: (1) not-idle check → (2) async send →
/// (3) set `VFS_CALL` only after success. Setting the flag first would leave
/// an unclearable `VFS_CALL` behind on a send failure, and that process could
/// never issue another VFS call.
///
/// `call` carries the target endpoint ([`VfsCall::endpoint`]); `slot` is the
/// PM-side process slot. The two usually denote the same process; for requests
/// not tied to a process, such as `Reboot`, the caller borrows the `VFS` slot
/// per C convention (misc.c:230).
pub fn tell_vfs<T: IpcTransport + ?Sized>(
    table: &mut ProcTable,
    slot: UserSlot,
    call: VfsCall,
    transport: &mut T,
) {
    // ① not-idle (utility.c:122-123): neither VFS_CALL nor EVENT_CALL may be
    // set — one process cannot have two outstanding VFS calls (the flag is
    // the continuation slot, see §2.1 of
    // rewrite-notes/04-stage-pm/05-vfs-interaction.md). Violation means PM
    // state corruption; panic and let RS restart us.
    if table.procs[slot.get()].state.block.ipc_blocked.is_some() {
        panic!("tell_vfs: not idle: {call:?}");
    }

    // ② async send (utility.c:127 asynsend3(VFS_PROC_NR, AMF_NOREPLY)).
    // Send failure means a broken transport; panic, same shape as C
    // (utility.c:127-129).
    let msg = call.encode();
    if let Err(e) = transport.send(Endpoint::VFS, &msg) {
        panic!("tell_vfs: unable to send to VFS: {e:?}");
    }

    // ③ set VFS_CALL (utility.c:128) — only after a successful send
    table.procs[slot.get()].state.block.ipc_blocked =
        Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
}

/// Handle a VFS reply (C: `main.c:294-424` `handle_vfs_reply`).
///
/// Four-part structure: (1) REBOOT special case → (2) endpoint resolution →
/// (3) invariants and `NEW_PARENT` extraction → (4) 11-way switch plus the
/// tail `restart_sigs`. Pure logic; all effects and reads go through
/// [`VfsReplyServices`].
pub fn handle_vfs_reply<S: VfsReplyServices>(
    svc: &mut S,
    msg: &Message,
) -> Result<(), VfsReplyError> {
    // ① REBOOT special case: not tied to any process, ask the kernel to abort
    // directly (main.c:304-312)
    if msg.m_type == VFS_PM_REBOOT_REPLY {
        svc.sys_abort();
        return Ok(());
    }

    // ② Decode the reply (includes RS-family validation; non-RS family → NotAReply)
    let reply = VfsReply::decode(msg)?;

    // ③ endpoint resolution (main.c:315-321)
    let endpt_raw = unsafe { msg.m_u.m_m7.m7i1 };
    let endpt = Endpoint(endpt_raw);
    let slot = match svc.slot_of_endpoint(endpt) {
        Some(s) => s,
        None => return Err(VfsReplyError::BadEndpoint(endpt)),
    };

    // ④ invariants + NEW_PARENT extraction (main.c:324-328)
    let new_parent = svc.take_vfs_call(slot);
    if svc.is_unpaused(slot) {
        // main.c:330-331
        panic!(
            "handle_vfs_reply: UNPAUSED set on entry for reply code {}",
            msg.m_type
        );
    }

    // ⑤ 11-way (main.c:334-419)
    match reply {
        VfsReply::SetUid | VfsReply::SetGid | VfsReply::SetGroups => {
            // main.c:339: wake the original caller
            svc.reply(slot, OK);
        }
        VfsReply::SetSid => {
            // main.c:345: the reply code is the process group number
            svc.reply(slot, svc.procgrp_of(slot));
        }
        VfsReply::Exec { status, pc, newsp, newps_str } => {
            // main.c:350-353: exec_restart replies to the caller itself
            svc.exec_restart(slot, ExecRestartArgs { status, pc, newsp, newps_str });
        }
        VfsReply::Core { status } => {
            // main.c:357-360: status==OK sets WCOREFLAG, then falls through to EXIT
            if status == OK {
                svc.set_core_flag(slot);
            }
            svc.assert_exiting(slot);
            svc.publish_event(slot, ProcEvent::Exit);
            return Ok(()); // early return, skipping the tail
        }
        VfsReply::Exit => {
            // main.c:362-367: publish the exit event and return (no tail restart_sigs)
            svc.assert_exiting(slot);
            svc.publish_event(slot, ProcEvent::Exit);
            return Ok(());
        }
        VfsReply::Fork => {
            // main.c:369-396: start scheduling the new process, then reply to
            // the child and the parent
            if svc.sched_start_user(slot).is_err() {
                // Scheduling failed: detach scheduler ownership first
                // (main.c:380 `mp_scheduler = NONE` — the child never
                // registered, so the later exit_restart must not send it
                // SCHEDULING_STOP), then tear down the new process and reply
                // failure to the parent (unless the parent is dead). Reply
                // code -1 matches C main.c:385 `reply(..., -1)`.
                svc.detach_scheduler(slot);
                svc.exit_proc(slot, -1, false);
                svc.reply_to_guardian(slot, new_parent, -1);
                return Ok(()); // early return
            }
            // Scheduling succeeded: reply to the process itself (caller slot),
            // then to the parent (unless the parent is dead)
            svc.reply(slot, OK);
            svc.reply_to_guardian(slot, new_parent, svc.pid_of(slot));
            // fall through to the tail restart_sigs
        }
        VfsReply::SrvFork => {
            // main.c:398-401: server-process fork, nothing to do
        }
        VfsReply::Unpause => {
            // main.c:403-415: set UNPAUSED, publish the signal event, then
            // return (no tail)
            svc.assert_stopped(slot);
            svc.set_unpaused(slot);
            svc.publish_event(slot, ProcEvent::Signal);
            return Ok(());
        }
        VfsReply::Reboot => unreachable!("REBOOT_REPLY 已在特例分支处理"),
    }

    // ⑥ Tail: restart_sigs (main.c:421-423)
    // Condition (IN_USE|EXITING) == IN_USE: in use and not exiting.
    if svc.is_in_use(slot) && !svc.is_exiting(slot) {
        svc.restart_signals(slot);
    }
    Ok(())
}

/// Production service assembly for `exec_restart` (C: the exec_restart call at
/// main.c:350-352).
///
/// Holds only the transport — the process table needed by
/// `KernelExec::reply`/`TracerSig::send` is passed as a method parameter
/// (`exec_restart` itself owns the table), so two `&mut ProcTable` never
/// coexist.
pub(crate) struct ExecServices<'a, T: IpcTransport> {
    pub(crate) transport: &'a mut T,
    pub(crate) kern: &'a mut dyn crate::exit::KernelGateway,
}

impl<T: IpcTransport> crate::exec::KernelExec for ExecServices<'_, T> {
    fn exec(&mut self, ep: Endpoint, sp: VirBytes, pc: VirBytes, ps: VirBytes, name: &[u8]) -> i32 {
        // C: sys_exec (exec.c:197; libsys sys_exec.c's m_lsys_krn_sys_exec
        // {endpt, stack, name, ip, ps_str}). The kernel SYS_EXEC face is in
        // place (kernel syscall_process.rs `dispatch_exec` + the minix-sys
        // `sys_exec` wrapper); the earlier stub that returned ENOSYS was a
        // leftover from an unwired kernel-call face (since obsolete). `name`
        // points at the name buffer inside PM's process table and the kernel
        // reads it with a data_copy under caller(PM)'s CR3 — matching C
        // passing `(vir_bytes)rmp->mp_name`. On failure the original errno
        // (the negative reply from perform_kernel_call) is propagated to
        // exec_restart's tail panic (same shape as C exec.c:198); success is
        // never fabricated.
        match minix_sys::syscall::sys_exec(
            &minix_sys::syscall::DirectKernelCallTransport,
            ep.0,
            pc.0,
            sp.0,
            name.as_ptr() as u64,
            ps.0,
        ) {
            Ok(()) => OK,
            Err(e) => e,
        }
    }

    fn kill(&mut self, _ep: Endpoint, _sig: i32) {
        // C: sys_kill (exec.c:195, the PARTIAL_EXEC teardown branch) — the
        // kernel call face is not implemented yet; C does not check the
        // return value, so this is an explicit no-op.
    }

    fn reply(&mut self, table: &mut ProcTable, slot: UserSlot, code: i32) {
        // C: reply(who_p, result) (exec.c:167 failure reply).
        // Semantic note: `send` on this transport has sendnb semantics (see
        // the trait comment: most PM send sites are replies, which never
        // block in C) — the blocking form is `send_blocking` only, reserved
        // for the VFS_PM_INIT handshake.
        let msg = Message {
            m_type: code,
            ..Default::default()
        };
        let ep = table.procs[slot.get()].endpoint();
        //[SD19-gate] PM send/notify/reply: fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = self.transport.send(ep, &msg);
    }
}

impl<T: IpcTransport> crate::exec::TracerSig for ExecServices<'_, T> {
    fn send(&mut self, table: &mut ProcTable, caller: UserSlot, pid: Pid, sig: i32) {
        //[SD19-gate] PM send/notify/reply: fire-and-forget
        #[allow(clippy::let_underscore_must_use)]
        let _ = crate::signal::check_sig(table, caller, pid, sig, false, self.kern, self.transport);
    }
}

impl<T: IpcTransport> crate::exec::ExecRestartServices for ExecServices<'_, T> {}

/// Production implementation (methods still DEFERRED are `unimplemented!`
/// until their design lands; the self-describing string names the owner).
pub struct PmServices<'a, T: IpcTransport> {
    table: &'a mut ProcTable,
    transport: &'a mut T,
    event_registry: &'a mut crate::event::EventRegistry,
    /// Kernel call exit (e.g. step-9 `sys_clear`).
    kern: &'a mut dyn crate::exit::KernelGateway,
    abort_flag: i32,
}

impl<'a, T: IpcTransport> PmServices<'a, T> {
    /// Construct the production port.
    pub fn new(
        table: &'a mut ProcTable,
        transport: &'a mut T,
        event_registry: &'a mut crate::event::EventRegistry,
        kern: &'a mut dyn crate::exit::KernelGateway,
        abort_flag: i32,
    ) -> Self {
        Self {
            table,
            transport,
            event_registry,
            kern,
            abort_flag,
        }
    }

    fn send_reply(&mut self, slot: UserSlot, code: i32) {
        let ep = self.table.procs[slot.get()].endpoint();
        let msg = Message {
            m_type: code,
            ..Default::default()
        };
        // C's reply() only warns on a send failure; it does not panic. Keep
        // that behavior.
        if let Err(e) = self.transport.send(ep, &msg) {
            pm_diag!("PM: vfs reply to slot {} failed: {:?}", slot.get(), e);
        }
    }
}

impl<'a, T: IpcTransport> VfsReplyServices for PmServices<'a, T> {
    fn slot_of_endpoint(&self, endpoint: Endpoint) -> Option<UserSlot> {
        self.table.pm_isokendpt(endpoint).ok()
    }

    fn take_vfs_call(&mut self, slot: UserSlot) -> bool {
        // main.c:324-328: first check that VFS_CALL is set, then extract
        // NEW_PARENT and clear.
        let proc = &mut self.table.procs[slot.get()];
        match proc.state.block.ipc_blocked.take() {
            Some(IpcBlockReason::VfsCall { reply_to_new_parent }) => reply_to_new_parent,
            _ => panic!(
                "handle_vfs_reply: reply without request (slot {})",
                slot.get()
            ),
        }
    }

    fn is_unpaused(&self, slot: UserSlot) -> bool {
        self.table.procs[slot.get()].state.block.unpaused
    }

    fn is_exiting(&self, slot: UserSlot) -> bool {
        self.table.procs[slot.get()].is_exiting()
    }

    fn is_in_use(&self, slot: UserSlot) -> bool {
        self.table.procs[slot.get()].is_in_use()
    }

    fn procgrp_of(&self, slot: UserSlot) -> Pid {
        self.table.procs[slot.get()].procgrp()
    }

    fn pid_of(&self, slot: UserSlot) -> Pid {
        self.table.procs[slot.get()].pid()
    }

    fn parent_slot(&self, slot: UserSlot) -> UserSlot {
        self.table.procs[slot.get()].parent()
    }

    fn reply(&mut self, slot: UserSlot, code: i32) {
        self.send_reply(slot, code);
    }

    fn reply_to_guardian(&mut self, slot: UserSlot, suppress: bool, code: i32) {
        if suppress {
            // main.c:384/392: the parent is dead (adopted by INIT), suppress
            // the reply.
            return;
        }
        let parent = self.table.procs[slot.get()].parent();
        self.send_reply(parent, code);
    }

    fn sched_start_user(&mut self, slot: UserSlot) -> Result<(), i32> {
        let sched = self.table.procs[slot.get()].resources.scheduler;
        // main.c:372: nothing to start explicitly when the scheduler is the
        // kernel or NONE.
        if sched == Endpoint::KERNEL || sched == Endpoint::NONE {
            return Ok(());
        }
        // main.c:373 — `sched_start_user(rmp->mp_scheduler, rmp)`: the policy
        // component performs the nice→queue conversion and sends
        // SCHEDULING_INHERIT (production MinixSchedCtl reaches the scheduler
        // over IPC; the placeholder face is now backed by a real
        // implementation).
        // C's rmp->mp_endpoint (the child's own endpoint) is child_ep here;
        // an earlier version mistakenly passed sched (the scheduler field) as
        // the schedulee, so SCHED issued schedctl against Endpoint::SCHED(slot4)
        // instead of the child (slot12+).
        let child_ep = self.table.procs[slot.get()].endpoint();
        let mut ctl = crate::sched::MinixSchedCtl::new(self.transport);
        crate::sched::sched_start_user(self.table, child_ep, slot, &mut ctl)
            .map_err(|e| e.to_errno())
    }

    fn detach_scheduler(&mut self, slot: UserSlot) {
        // C: main.c:380 — on the fork scheduling-failure leg, set NONE before
        // exit_proc so the exit chain's later `exit_restart` sched_stop gate
        // (forkexit.c:425 / exit.rs `if scheduler != KERNEL && != NONE`) is
        // skipped and no SCHEDULING_STOP is sent to a child that never
        // registered.
        self.table.procs[slot.get()].resources.scheduler = Endpoint::NONE;
    }

    fn exit_proc(&mut self, slot: UserSlot, status: i32, dump_core: bool) {
        // C: main.c:381 — fork scheduling-failure path `exit_proc(rmp, -1,
        // FALSE /*dump_core*/)`: enters the two-phase exit directly (the full
        // exit_proc chain: sys_stop → VFS_PM_EXIT → zombify → adoption; see
        // rewrite-notes/04-stage-pm/09-pm-exit.md), without re-reading the
        // message and without do_exit's entry gate. status truncates to i8,
        // matching C's exit_status semantics.
        crate::exit::exit_proc(self.table, slot, status as i8, 0, dump_core, self.transport, self.kern);
    }

    fn set_core_flag(&mut self, slot: UserSlot) {
        // C: main.c:357-358 — `rmp->mp_sigstatus |= WCOREFLAG` (only a Core
        // reply with status==OK; the VfsReply::Core branch already decided
        // that). The or happens in the u8 domain: 0o200 sets bit 7 of the
        // signal byte, and wait4 combines it via W_EXITCODE into "core
        // dumped" (the WIFCORED equivalent). C asserts EXITING before this
        // (main.c:362); the Rust side fails fast for a non-Exiting target.
        match &mut self.table.procs[slot.get()].state.lifecycle {
            crate::mproc::Lifecycle::Exiting { sig_status, .. } => {
                *sig_status = (*sig_status as u8 | WCOREFLAG as u8) as i8;
            }
            _ => panic!(
                "set_core_flag: process not Exiting (slot {})",
                slot.get()
            ),
        }
    }

    fn assert_exiting(&mut self, slot: UserSlot) {
        assert!(
            self.table.procs[slot.get()].is_exiting(),
            "handle_vfs_reply: EXIT/CORE reply but process is not EXITING (slot {})",
            slot.get()
        );
    }

    fn assert_stopped(&mut self, slot: UserSlot) {
        assert!(
            self.table.procs[slot.get()].state.block.stopped,
            "handle_vfs_reply: UNPAUSE reply but process not PROC_STOPPED (slot {})",
            slot.get()
        );
    }

    fn publish_event(&mut self, slot: UserSlot, event: ProcEvent) {
        // [ARCH: event-subscription] — see
        // rewrite-notes/04-stage-pm/06-event-subscription.md.
        // Validate that the passed event agrees with the one inferred from the
        // target's flags, then publish through EventRegistry (reusing
        // `publish_event`'s `PRIV_PROC|EXITING` cleanup and serialization).
        let inferred = {
            let proc = &self.table.procs[slot.get()];
            if proc.is_exiting() {
                ProcEvent::Exit
            } else if proc.state.block.unpaused {
                ProcEvent::Signal
            } else {
                panic!(
                    "publish_event: unknown event for slot {} flags {:?} unpaused {}",
                    slot.get(),
                    proc.state.lifecycle,
                    proc.state.block.unpaused
                );
            }
        };
        assert_eq!(
            inferred, event,
            "publish_event: caller event {:?} != inferred {:?} for slot {}",
            event, inferred, slot.get()
        );
        // Publish through EventRegistry (field-level reborrows satisfy the
        // borrow checker's disjoint-field requirement — the four `&mut`
        // fields are disjoint, as the way `PmServices::new` is constructed
        // already demonstrates; the earlier raw-pointer unsafe split was
        // unnecessary, and a no_std codebase should keep the unsafe surface
        // minimal).
        self.event_registry
            .publish_event(slot, self.table, &mut *self.transport, &mut *self.kern);
    }

    fn exec_restart(&mut self, slot: UserSlot, args: ExecRestartArgs) {
        // C: main.c:350-352 — exec_restart(rmp, VFS_PM_STATUS, PC, NEWSP,
        // NEWPS_STR). The production port ExecServices holds the transport
        // (borrowed disjointly from self.table); the full exec semantics
        // (PARTIAL_EXEC teardown / caught reset / tracer signal / sys_exec)
        // live in crate::exec::exec_restart.
        let mut svc = ExecServices {
            transport: self.transport,
            kern: self.kern,
        };
        crate::exec::exec_restart(
            self.table,
            slot,
            args.status,
            VirBytes(args.pc),
            VirBytes(args.newsp),
            VirBytes(args.newps_str),
            &mut svc,
        );
    }

    fn restart_signals(&mut self, slot: UserSlot) {
        // C: main.c:421-423 — the VFS reply tail `if ((IN_USE|EXITING)==IN_USE)
        // restart_sigs(rmp)`: once the reply settles, recheck pending signals
        // (PROC_STOPPED doubles as the recheck indicator; sig_proc's VFS|EVENT
        // branch has really set it since the signal-flow rework). Before that
        // this was a no-op scaffold whose premise ("PM does not model pending
        // signals yet") had gone stale, and the earlier DEFERRED sweep missed
        // it.
        // Reuse the event domain's `PmEventServices` (a full RestartServices
        // implementation).
        let mut ev = crate::event::PmEventServices::new(self.transport, self.kern);
        crate::signal_flow::restart_sigs(self.table, slot, &mut ev);
    }

    fn set_unpaused(&mut self, slot: UserSlot) {
        self.table.procs[slot.get()].state.block.unpaused = true;
    }

    fn sys_abort(&mut self) {
        // C: main.c:304-312 — REBOOT reply special case: `sys_abort(abort_flag)`
        // and then return to the main loop waiting for the HARD_STOP
        // notification. C does not check the return value: on success the
        // machine stops; on failure (pre-E1 returns -EIO) PM keeps looping and
        // does not fake a stopped state. The minix-sys `sys_abort` slice is in
        // place.
        if let Err(r) = self.kern.sys_abort(self.abort_flag) {
            //[SD19-gate] PM send/notify/reply: fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = r; // same shape as C: ignore the return value, keep looping
        }
    }
}

/// Test recorder: stores the port call sequence in [`RecordedEffect`] for
/// assertions.
///
/// Read methods return the state configured at construction (`endpoint_slot` /
/// `unpaused` / `exiting` / `in_use` / `procgrp` / `pid` / `parent` /
/// `next_new_parent` / `next_sched_result`), so every branch of the state
/// machine can be asserted deterministically without a real `ProcTable` /
/// `IpcTransport`.
#[cfg(test)]
pub struct RecordingServices {
    pub effects: Vec<RecordedEffect>,
    pub endpoint_slot: Option<(Endpoint, UserSlot)>,
    pub unpaused: bool,
    pub exiting: bool,
    pub in_use: bool,
    pub procgrp: Pid,
    pub pid: Pid,
    pub parent: UserSlot,
    pub next_new_parent: bool,
    pub next_sched_result: Result<(), i32>,
}

/// Port effects recorded by [`RecordingServices`].
///
/// Read-class variants (SlotOfEndpoint/IsUnpaused/...) are produced only by
/// pure read methods; the recorder does not record pure reads (see the `&self`
/// read method signatures in `VfsReplyServices`), hence `allow(dead_code)`.
#[cfg(test)]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordedEffect {
    SlotOfEndpoint { endpoint: Endpoint },
    TakeVfsCall { slot: UserSlot, new_parent: bool },
    IsUnpaused { slot: UserSlot },
    IsExiting { slot: UserSlot },
    IsInUse { slot: UserSlot },
    ProcgrpOf { slot: UserSlot },
    PidOf { slot: UserSlot },
    ParentSlot { slot: UserSlot },
    Reply { slot: UserSlot, code: i32 },
    ReplyToGuardian { slot: UserSlot, suppressed: bool, code: i32 },
    SchedStartUser { slot: UserSlot, result: Result<(), i32> },
    DetachScheduler { slot: UserSlot },
    ExitProc { slot: UserSlot, status: i32, dump_core: bool },
    SetCoreFlag { slot: UserSlot },
    AssertExiting { slot: UserSlot },
    AssertStopped { slot: UserSlot },
    PublishEvent { slot: UserSlot, event: ProcEvent },
    ExecRestart { slot: UserSlot, args: ExecRestartArgs },
    RestartSignals { slot: UserSlot },
    SetUnpaused { slot: UserSlot },
    SysAbort,
}

#[cfg(test)]
impl RecordingServices {
    /// Default configuration: a matching endpoint resolves to `slot(1)`; the
    /// remaining state holds typical runtime values.
    pub fn new() -> Self {
        Self {
            effects: Vec::new(),
            endpoint_slot: Some((Endpoint::from_generation_slot(1, 1), UserSlot::new(1))),
            unpaused: false,
            exiting: false,
            in_use: true,
            procgrp: 7,
            pid: 100,
            parent: UserSlot::new(0),
            next_new_parent: false,
            next_sched_result: Ok(()),
        }
    }
}

#[cfg(test)]
impl VfsReplyServices for RecordingServices {
    fn slot_of_endpoint(&self, endpoint: Endpoint) -> Option<UserSlot> {
        self.endpoint_slot
            .filter(|(ep, _)| *ep == endpoint)
            .map(|(_, s)| s)
    }

    fn take_vfs_call(&mut self, slot: UserSlot) -> bool {
        let np = self.next_new_parent;
        self.effects.push(RecordedEffect::TakeVfsCall { slot, new_parent: np });
        np
    }

    fn is_unpaused(&self, _slot: UserSlot) -> bool {
        self.unpaused
    }

    fn is_exiting(&self, _slot: UserSlot) -> bool {
        self.exiting
    }

    fn is_in_use(&self, _slot: UserSlot) -> bool {
        self.in_use
    }

    fn procgrp_of(&self, _slot: UserSlot) -> Pid {
        self.procgrp
    }

    fn pid_of(&self, _slot: UserSlot) -> Pid {
        self.pid
    }

    fn parent_slot(&self, _slot: UserSlot) -> UserSlot {
        self.parent
    }

    fn reply(&mut self, slot: UserSlot, code: i32) {
        self.effects.push(RecordedEffect::Reply { slot, code });
    }

    fn reply_to_guardian(&mut self, slot: UserSlot, suppress: bool, code: i32) {
        self.effects
            .push(RecordedEffect::ReplyToGuardian { slot, suppressed: suppress, code });
    }

    fn sched_start_user(&mut self, slot: UserSlot) -> Result<(), i32> {
        let r = self.next_sched_result;
        self.effects.push(RecordedEffect::SchedStartUser { slot, result: r });
        r
    }

    fn detach_scheduler(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::DetachScheduler { slot });
    }

    fn exit_proc(&mut self, slot: UserSlot, status: i32, dump_core: bool) {
        self.effects
            .push(RecordedEffect::ExitProc { slot, status, dump_core });
    }

    fn set_core_flag(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::SetCoreFlag { slot });
    }

    fn assert_exiting(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::AssertExiting { slot });
    }

    fn assert_stopped(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::AssertStopped { slot });
    }

    fn publish_event(&mut self, slot: UserSlot, event: ProcEvent) {
        self.effects.push(RecordedEffect::PublishEvent { slot, event });
    }

    fn exec_restart(&mut self, slot: UserSlot, args: ExecRestartArgs) {
        self.effects.push(RecordedEffect::ExecRestart { slot, args });
    }

    fn restart_signals(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::RestartSignals { slot });
    }

    fn set_unpaused(&mut self, slot: UserSlot) {
        self.effects.push(RecordedEffect::SetUnpaused { slot });
    }

    fn sys_abort(&mut self) {
        self.effects.push(RecordedEffect::SysAbort);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{
        Message, VFS_PM_CORE_REPLY, VFS_PM_EXIT_REPLY, VFS_PM_EXEC_REPLY, VFS_PM_FORK_REPLY,
        VFS_PM_SETGID_REPLY, VFS_PM_SETGROUPS_REPLY, VFS_PM_SETSID_REPLY, VFS_PM_SETUID_REPLY,
        VFS_PM_SRV_FORK_REPLY, VFS_PM_UNPAUSE_REPLY,
    };

    fn reply_msg(m_type: i32, endpt: Endpoint) -> Message {
        let mut m = Message {
            m_type,
            ..Default::default()
        };
        m.m_u.m_m7.m7i1 = endpt.get();
        m
    }

    #[test]
    fn test_reboot_reply_is_special_cased() {
        let mut svc = RecordingServices::new();
        let msg = reply_msg(VFS_PM_REBOOT_REPLY, Endpoint::VFS);
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert_eq!(svc.effects, vec![RecordedEffect::SysAbort]);
    }

    #[test]
    fn test_setuid_setgid_setgroups_wake_caller() {
        for m_type in [VFS_PM_SETUID_REPLY, VFS_PM_SETGID_REPLY, VFS_PM_SETGROUPS_REPLY] {
            let mut svc = RecordingServices::new();
            let msg = reply_msg(m_type, Endpoint::from_generation_slot(1, 1));
            handle_vfs_reply(&mut svc, &msg).unwrap();
            // slot_of_endpoint → take_vfs_call → reply(OK) → is_in_use → restart_signals
            assert_eq!(
                svc.effects.last(),
                Some(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) })
            );
            assert!(svc.effects.contains(&RecordedEffect::Reply {
                slot: UserSlot::new(1),
                code: OK
            }));
        }
    }

    #[test]
    fn test_setsid_replies_procgrp() {
        let mut svc = RecordingServices::new();
        svc.procgrp = 42;
        let msg = reply_msg(VFS_PM_SETSID_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::Reply {
            slot: UserSlot::new(1),
            code: 42
        }));
    }

    #[test]
    fn test_exec_restart_receives_args() {
        let mut svc = RecordingServices::new();
        let mut msg = reply_msg(VFS_PM_EXEC_REPLY, Endpoint::from_generation_slot(1, 1));
        msg.m_u.m_m7.m7i2 = 0; // status OK
        msg.m_u.m_m7.m7p1 = 0x1000;
        msg.m_u.m_m7.m7p2 = 0x2000;
        msg.m_u.m_m7.m7i5 = 0x3000;
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::ExecRestart {
            slot: UserSlot::new(1),
            args: ExecRestartArgs { status: 0, pc: 0x1000, newsp: 0x2000, newps_str: 0x3000 },
        }));
    }

    #[test]
    fn test_core_sets_flag_then_exit_event_early_return() {
        let mut svc = RecordingServices::new();
        svc.exiting = true; // EXITING set so assert_exiting passes
        let mut msg = reply_msg(VFS_PM_CORE_REPLY, Endpoint::from_generation_slot(1, 1));
        msg.m_u.m_m7.m7i2 = 0; // status OK
        handle_vfs_reply(&mut svc, &msg).unwrap();
        // Core: set_core_flag → assert_exiting → publish(Exit) → early return (no restart_signals)
        assert!(svc.effects.contains(&RecordedEffect::SetCoreFlag { slot: UserSlot::new(1) }));
        assert!(svc.effects.contains(&RecordedEffect::PublishEvent {
            slot: UserSlot::new(1),
            event: ProcEvent::Exit,
        }));
        assert!(!svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_exit_publishes_event_early_return() {
        let mut svc = RecordingServices::new();
        svc.exiting = true;
        let msg = reply_msg(VFS_PM_EXIT_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::PublishEvent {
            slot: UserSlot::new(1),
            event: ProcEvent::Exit,
        }));
        assert!(!svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_fork_success_replies_child_and_parent_then_tail() {
        let mut svc = RecordingServices::new();
        svc.next_sched_result = Ok(());
        svc.pid = 1234;
        let msg = reply_msg(VFS_PM_FORK_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        // sched OK → reply(child, OK) + reply_to_guardian(parent, suppress=false, pid) + tail
        assert!(svc.effects.contains(&RecordedEffect::Reply {
            slot: UserSlot::new(1),
            code: OK,
        }));
        assert!(svc.effects.contains(&RecordedEffect::ReplyToGuardian {
            slot: UserSlot::new(1),
            suppressed: false,
            code: 1234,
        }));
        assert!(svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
        // The success leg must not detach scheduler ownership (detach belongs
        // to the failure leg only).
        assert!(!svc.effects.contains(&RecordedEffect::DetachScheduler { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_fork_success_suppressed_guardian_when_new_parent() {
        let mut svc = RecordingServices::new();
        svc.next_sched_result = Ok(());
        svc.next_new_parent = true; // the parent is dead
        svc.pid = 1234; // even when suppressed, the reply code passed is still the child PID
        let msg = reply_msg(VFS_PM_FORK_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::Reply {
            slot: UserSlot::new(1),
            code: OK,
        }));
        assert!(svc.effects.contains(&RecordedEffect::ReplyToGuardian {
            slot: UserSlot::new(1),
            suppressed: true,
            code: 1234, // pid defaults to 100, but is ignored under suppress
        }));
    }

    #[test]
    fn test_fork_sched_failure_tears_down_and_replies_parent() {
        let mut svc = RecordingServices::new();
        svc.next_sched_result = Err(-1);
        let msg = reply_msg(VFS_PM_FORK_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        // sched failure → detach_scheduler + exit_proc(slot, -1, false) + reply_to_guardian(parent, false, -1) + return
        assert!(svc.effects.contains(&RecordedEffect::ExitProc {
            slot: UserSlot::new(1),
            status: -1,
            dump_core: false,
        }));
        // C main.c:380 fidelity: the failure leg must detach_scheduler (set
        // scheduler=NONE) before exit_proc, otherwise exit_proc's sched_stop
        // gate wrongly sends SCHEDULING_STOP to a child that never registered.
        // The order assertion guards against regression.
        assert!(svc.effects.contains(&RecordedEffect::DetachScheduler {
            slot: UserSlot::new(1),
        }));
        let detach_at = svc.effects.iter().position(|e| {
            *e == RecordedEffect::DetachScheduler {
                slot: UserSlot::new(1),
            }
        });
        let exit_at = svc.effects.iter().position(|e| {
            *e == RecordedEffect::ExitProc {
                slot: UserSlot::new(1),
                status: -1,
                dump_core: false,
            }
        });
        assert!(
            detach_at.is_some() && exit_at.is_some() && detach_at < exit_at,
            "detach_scheduler 必须先于 exit_proc（C main.c:380-381 语句顺序）"
        );
        assert!(svc.effects.contains(&RecordedEffect::ReplyToGuardian {
            slot: UserSlot::new(1),
            suppressed: false,
            code: -1,
        }));
        // no reply to the child, no restart_signals
        assert!(!svc.effects.contains(&RecordedEffect::Reply {
            slot: UserSlot::new(1),
            code: OK,
        }));
        assert!(!svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_srv_fork_is_noop_then_tail() {
        let mut svc = RecordingServices::new();
        let msg = reply_msg(VFS_PM_SRV_FORK_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        // no reply/exit at all, only the tail restart_signals
        assert!(svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
        assert!(!svc.effects.iter().any(|e| matches!(e, RecordedEffect::Reply { .. })));
    }

    #[test]
    fn test_unpause_sets_flag_publishes_signal_early_return() {
        let mut svc = RecordingServices::new();
        let msg = reply_msg(VFS_PM_UNPAUSE_REPLY, Endpoint::from_generation_slot(1, 1));
        handle_vfs_reply(&mut svc, &msg).unwrap();
        assert!(svc.effects.contains(&RecordedEffect::SetUnpaused { slot: UserSlot::new(1) }));
        assert!(svc.effects.contains(&RecordedEffect::PublishEvent {
            slot: UserSlot::new(1),
            event: ProcEvent::Signal,
        }));
        assert!(!svc.effects.contains(&RecordedEffect::RestartSignals { slot: UserSlot::new(1) }));
    }

    #[test]
    fn test_bad_endpoint_replies_with_error() {
        let mut svc = RecordingServices::new();
        svc.endpoint_slot = None; // every endpoint fails to resolve
        let msg = reply_msg(VFS_PM_SETUID_REPLY, Endpoint::from_generation_slot(1, 1));
        let err = handle_vfs_reply(&mut svc, &msg).unwrap_err();
        assert_eq!(err, VfsReplyError::BadEndpoint(Endpoint::from_generation_slot(1, 1)));
    }

    #[test]
    fn test_unpaused_on_entry_panics() {
        let mut svc = RecordingServices::new();
        svc.unpaused = true; // UNPAUSED already set on entry → entry invariant fails
        let msg = reply_msg(VFS_PM_SETUID_REPLY, Endpoint::from_generation_slot(1, 1));
        let result = std::panic::catch_unwind(move || {
            let mut svc = svc;
            //[SD19-gate] PM send/notify/reply: fire-and-forget
            #[allow(clippy::let_underscore_must_use)]
            let _ = handle_vfs_reply(&mut svc, &msg);
        });
        assert!(result.is_err(), "入口 UNPAUSED 必须 panic");
    }

    #[test]
    fn test_decode_error_propagates() {
        // A non-RS-family reply → VfsReply::decode returns NotAReply, but
        // run_once only calls this function when is_vfs_pm_rs holds; here we
        // verify propagation of NotAReply directly (using a plain PM call code).
        let mut svc = RecordingServices::new();
        let msg = reply_msg(0 /* not RS family */, Endpoint::from_generation_slot(1, 1));
        // 0 is not in the RS family → decode fails. slot_of_endpoint would
        // still be called (decode first, resolve second).
        let err = handle_vfs_reply(&mut svc, &msg).unwrap_err();
        // family test for 0: is_vfs_pm_rs(0) = (0 & !0x7f)==0x980? No → NotAReply(0)
        assert_eq!(err, VfsReplyError::NotAReply(0));
    }

    #[test]
    fn test_tell_vfs_sets_vfs_call_on_success() {
        use crate::ipc::transport::TestIpcTransport;
        use crate::mproc::ProcTable;
        use minix_types::VfsCall;

        let mut table = ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let slot = UserSlot::new(1);
        let call = VfsCall::SetUid {
            endpoint: Endpoint::from_generation_slot(1, 1),
            eid: 0,
            rid: 0,
        };
        tell_vfs(&mut table, slot, call, &mut transport);
        assert_eq!(
            table.procs[slot.get()].state.block.ipc_blocked,
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: false })
        );
        // TestIpcTransport should record one message addressed to VFS
        assert_eq!(transport.sent().len(), 1);
        assert_eq!(transport.last_sent_dest(), Some(Endpoint::VFS));
    }

    /// Kernel gateway mock for these tests (always OK).
    #[derive(Default)]
    struct NoopKernelGateway;
    impl crate::exit::KernelGateway for NoopKernelGateway {
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
    }

    #[test]
    fn test_production_set_core_flag_sets_bit7_on_exiting() {
        // Core reply with status==OK → WCOREFLAG goes into bit 7 of sig_status
        // (main.c:357-358); a non-Exiting target fails fast (the Rust
        // equivalent of C's assert EXITING at main.c:362).
        use crate::ipc::transport::TestIpcTransport;
        use crate::mproc::Lifecycle;

        let mut table = ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let mut events = crate::event::EventRegistry::new();
        let mut kern = NoopKernelGateway;
        let mut svc = PmServices::new(&mut table, &mut transport, &mut events, &mut kern, 0);
        svc.table.procs[1].state.lifecycle =
            Lifecycle::Exiting { exit_code: 0, sig_status: 6 }; // SIGABRT

        svc.set_core_flag(UserSlot::new(1));
        match svc.table.procs[1].state.lifecycle {
            Lifecycle::Exiting { sig_status, .. } => {
                assert_eq!(sig_status as u8, 0o200 | 6, "bit7 set, low bits kept");
            }
            ref other => panic!("unexpected lifecycle {:?}", other),
        }

        // Non-Exiting → fail fast (same source as C's assert(mp_flags & EXITING))
        svc.table.procs[2].state.lifecycle = Lifecycle::Running;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            svc.set_core_flag(UserSlot::new(2));
        }));
        assert!(result.is_err(), "set_core_flag on Running must panic");
    }

    #[test]
    fn test_production_sys_abort_saves_how_to_kernel() {
        // REBOOT reply special case (main.c:304-312) — `sys_abort(abort_flag)`
        // goes out through KernelGateway; abort_flag is the how bits from the
        // original reboot request.
        use crate::ipc::transport::TestIpcTransport;

        struct RecordingKernel { last_how: Option<i32>, copied: Option<alloc::vec::Vec<u8>> }
        impl crate::exit::KernelGateway for RecordingKernel {
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
            fn sys_abort(&mut self, how: i32) -> Result<(), i32> { self.last_how = Some(how); Ok(()) }
            fn copy_to_user(&mut self, bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
                self.copied = Some(bytes.to_vec());
                Ok(())
            }
            fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((30, 12)) }
            fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        }

        let mut table = ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let mut events = crate::event::EventRegistry::new();
        let mut kern = RecordingKernel { last_how: None, copied: None };
        let mut svc = PmServices::new(&mut table, &mut transport, &mut events, &mut kern, 0x0808);

        svc.sys_abort();

        assert_eq!(kern.last_how, Some(0x0808), "abort_flag (how) must reach the kernel gateway");
    }

    #[test]
    fn test_production_exit_proc_delegates_to_exit_chain() {
        // The production port's exit_proc delegates to the two-phase exit —
        // on FORK scheduling failure (main.c:381 exit_proc(rmp, -1, FALSE))
        // the child zombifies and VFS is notified.
        use crate::ipc::transport::TestIpcTransport;
        use crate::mproc::Lifecycle;

        let mut table = ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let mut events = crate::event::EventRegistry::new();
        let mut kern = NoopKernelGateway;
        let mut svc = PmServices::new(&mut table, &mut transport, &mut events, &mut kern, 0);
        // Child to tear down (the RS-forked PRIV_PROC semantics live in
        // rewrite-notes/04-stage-pm/08-pm-srv-fork.md; a plain user process
        // here)
        svc.table.procs[1].identity.endpoint = Endpoint::from_generation_slot(2, 1);
        svc.table.procs[1].identity.id.pid = 42;
        svc.table.procs[1].state.lifecycle = Lifecycle::Running;

        svc.exit_proc(UserSlot::new(1), -1, false);

        // The child has entered exit/zombie state and VFS_PM_EXIT was sent.
        assert!(
            !matches!(svc.table.procs[1].state.lifecycle, Lifecycle::Running),
            "exit_proc must move the child out of Running, got {:?}",
            svc.table.procs[1].state.lifecycle
        );
        assert!(
            transport
                .sent()
                .iter()
                .any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
            "VFS_PM_EXIT must be sent"
        );
    }

    #[test]
    #[should_panic(expected = "tell_vfs: not idle")]
    fn test_tell_vfs_not_idle_when_blocked() {
        use crate::ipc::transport::TestIpcTransport;
        use crate::mproc::ProcTable;
        use minix_types::VfsCall;

        let mut table = ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let slot = UserSlot::new(1);
        // Pre-set (simulating an in-flight VFS call or event call)
        table.procs[slot.get()].state.block.ipc_blocked =
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let call = VfsCall::SetUid {
            endpoint: Endpoint::from_generation_slot(1, 1),
            eid: 0,
            rid: 0,
        };
        tell_vfs(&mut table, slot, call, &mut transport);
        unreachable!("not-idle must panic before sending");
    }

    /// Regression: the VFS reply tail's restart_signals delegation to
    /// `restart_sigs` — a pending, unblocked SIGKILL is delivered once the
    /// reply settles (termination chain).
    #[test]
    fn test_restart_signals_delivers_pending_sigkill() {
        use crate::ipc::transport::TestIpcTransport;
        use crate::mproc::Lifecycle;
        let mut table = crate::mproc::ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let mut events = crate::event::EventRegistry::new();
        let mut kern = NoopKernelGateway;
        let mut svc = PmServices::new(&mut table, &mut transport, &mut events, &mut kern, 0);
        svc.table.procs[1].identity.endpoint = Endpoint::from_generation_slot(2, 1);
        svc.table.procs[1].identity.id.pid = 42;
        svc.table.procs[1].state.lifecycle = Lifecycle::Running;
        // C restart_sigs preconditions: PROC_STOPPED (set by sig_proc's
        // VFS|EVENT branch) + a pending, unblocked SIGKILL.
        svc.table.procs[1].state.block.stopped = true;
        svc.table.procs[1].resources.signals.pending = 1u64 << (9 - 1);

        svc.restart_signals(UserSlot::new(1));

        // SIGKILL → sig_proc → termination chain → zombie.
        assert!(
            matches!(
                svc.table.procs[1].state.lifecycle,
                Lifecycle::Zombie { .. } | Lifecycle::TraceZombie { .. }
            ),
            "pending SIGKILL must terminate, got {:?}",
            svc.table.procs[1].state.lifecycle
        );
    }

    /// No stop state, no TRACE_EXIT → no-op: the signal stays pending (C
    /// restart_sigs early-exit branch, signal.c:693).
    #[test]
    fn test_restart_signals_noop_when_not_stopped() {
        use crate::ipc::transport::TestIpcTransport;
        use crate::mproc::Lifecycle;
        let mut table = crate::mproc::ProcTable::new();
        let mut transport = TestIpcTransport::default();
        let mut events = crate::event::EventRegistry::new();
        let mut kern = NoopKernelGateway;
        let mut svc = PmServices::new(&mut table, &mut transport, &mut events, &mut kern, 0);
        svc.table.procs[1].identity.endpoint = Endpoint::from_generation_slot(2, 1);
        svc.table.procs[1].identity.id.pid = 42;
        svc.table.procs[1].state.lifecycle = Lifecycle::Running;
        svc.table.procs[1].resources.signals.pending = 1u64 << (9 - 1);

        svc.restart_signals(UserSlot::new(1));

        // Recheck preconditions not met: the signal stays pending, the
        // process lives.
        assert_eq!(svc.table.procs[1].resources.signals.pending, 1u64 << (9 - 1));
        assert_eq!(svc.table.procs[1].state.lifecycle, Lifecycle::Running);
    }
}
