//! End-to-end integration tests for the PM main loop (driving
//! `PmServer::run_once` from outside the crate).
//!
//! # Why this layer is needed
//!
//! Unit tests check the logic of each handler (given table state -> expected
//! side effects), and dispatch-layer unit tests check the arm routing of
//! `dispatch_pm_call`; but the complete path "from receiving a `Message` to
//! sending a `Reply`" — `run_once`'s caller validation, VFS reply interception,
//! the single dispatch table (04-ipc-dispatch.md §4.2), and the `ReplyIntent` ->
//! `reply()` mapping — can only be covered by this layer.  The old cross-crate
//! test (`os/tests/pm_vm_fork.rs`) was retired because it touched VM-internal
//! types directly; this layer goes through the message interface instead (the
//! direction its file header suggested).
//!
//! # Seeding
//!
//! `BootParams::placeholder()`'s boot image contains only empty entries (it
//! creates no processes), so test processes are seeded by hand through
//! `table_mut()` — matching what C does by filling the `mproc` table directly
//! before the main loop.  All wire assertions rest on the (destination, message)
//! sequence recorded by `TestIpcTransport`.

use minix_pm::init::{BootParams, RunStep};
use minix_pm::ipc::{IpcStatus, TestIpcTransport};
use minix_pm::mproc::{Lifecycle, WaitTarget};
use minix_types::{Endpoint, Message};

/// Kernel gateway mock for integration tests: sys_times returns scripted
/// accounting values, and sys_kill/sys_clear/sys_abort always return OK (their
/// real hardware paths are outside this test's scope).
struct MockKernelGateway {
    pub user: minix_types::Clock,
    pub sys: minix_types::Clock,
    pub ksig_script: Vec<Option<(i32, minix_types::SigSetBits)>>,
}

impl minix_pm::exit::KernelGateway for MockKernelGateway {
    fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
    fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, minix_types::SigSetBits)>, i32> {
        Ok(self.ksig_script.remove(0).map(|(ep, mask)| (Endpoint(ep), mask)))
    }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
    fn sys_diagctl_stacktrace(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
    fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
    fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
    fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
    fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> { Ok(()) }
    fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
    fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
    fn sys_trace(&mut self, _req: i32, _ep: Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: Endpoint, _src: u64, _dst_ep: Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
    fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
        Ok((self.user, self.sys))
    }
}

/// Build a server with an empty parameter table (`placeholder()` seeds no processes).
fn server() -> minix_pm::init::PmServer<TestIpcTransport> {
    minix_pm::init::PmServer::with_kernel_gateway(
        BootParams::placeholder(),
        TestIpcTransport::new(),
        Box::new(MockKernelGateway { user: 0, sys: 0, ksig_script: Vec::new() }),
    )
}

/// Seed a Running process into `slot` (the endpoint carries a generation; the PID is unique).
fn seed_running(server: &mut minix_pm::init::PmServer<TestIpcTransport>, slot: usize, pid: i32) -> Endpoint {
    let ep = Endpoint::from_generation_slot(1, slot as i32);
    let table = server.table_mut();
    table.procs[slot].identity.endpoint = ep;
    table.procs[slot].identity.id.pid = pid;
    table.procs[slot].state.lifecycle = Lifecycle::Running;
    ep
}

/// Build a request message originating from `source`.
fn request(m_type: i32, source: Endpoint) -> Message {
    Message {
        m_type,
        m_source: source,
        ..Default::default()
    }
}

#[test]
fn fork_request_runs_full_chain_and_suspends() {
    // C: do_fork (forkexit.c:44-143) — vm_fork (sendrec to VM) succeeds ->
    // tell_vfs (VFS_PM_FORK) -> SUSPEND (the main loop does not reply to the
    // parent, forkexit.c:139).  What is asserted end to end is the wire
    // sequence: VM_FORK precedes VFS_PM_FORK and the caller gets no reply.
    let mut srv = server();
    let parent_ep = seed_running(&mut srv, 5, 100);

    // Scripted VM_FORK reply: empty table + cursor 0 -> child slot 1, child
    // generation = parent (1) + 1.
    let mut vm_reply = Message {
        m_type: minix_types::OK,
        ..Default::default()
    };
    vm_reply.m_u.m_m1.m1i3 = Endpoint::from_generation_slot(2, 1).0;
    srv.transport_mut()
        .queue_sendrec_reply(vm_reply);
    srv.transport_mut()
        .queue_receive(request(2, parent_ep), IpcStatus::default()); // PM_FORK = 2

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 2, "expected exactly [VM_FORK, VFS_PM_FORK]");
    assert_eq!(sent[0].0, Endpoint::VM);
    assert_eq!(sent[0].1.m_type, minix_types::VM_FORK as i32);
    assert_eq!(sent[1].0, Endpoint::VFS);
    assert_eq!(sent[1].1.m_type, minix_types::VFS_PM_FORK);
    // SUSPEND: no reply is sent to the parent endpoint.
    assert!(
        !sent.iter().any(|(dest, _)| *dest == parent_ep),
        "fork must not sync-reply the parent"
    );
    // Child slot 1 is in VFS_CALL (the continuation hangs off the child, forkexit.c:130).
    assert!(srv.table().procs[1].state.block.is_vfs_blocked());
}

#[test]
fn exit_request_never_replies_and_zombifies_child() {
    // C: do_exit (forkexit.c:245-266) — "beyond the grave": never replies; if
    // the parent is waiting, tell_parent moves the child to TOLD_PARENT,
    // otherwise it stays ZOMBIE.
    let mut srv = server();
    let _parent_ep = seed_running(&mut srv, 2, 10);
    let child_ep = seed_running(&mut srv, 3, 11);
    // The parent waits for any child (wait_test hits -> the tell_parent chain).
    srv.table_mut().procs[2].state.wait.waiting = true;
    srv.table_mut().procs[2].state.wait.target = WaitTarget::AnyChild;

    srv.transport_mut()
        .queue_receive(request(1, child_ep), IpcStatus::default()); // PM_EXIT = 1, status defaults to 0

    assert_eq!(srv.run_once(), RunStep::Handled);

    // The exiting process itself is never replied to.
    let sent = srv.transport().sent();
    assert!(
        !sent.iter().any(|(dest, _)| *dest == child_ep),
        "exit must never reply the exiting process"
    );
    // The child has completed the zombie chain (parent waiting -> ToldParent).
    let child = &srv.table().procs[3];
    assert!(
        matches!(
            child.state.lifecycle,
            Lifecycle::ToldParent { .. } | Lifecycle::Zombie { .. }
        ),
        "child should be reaped or zombie, got {:?}",
        child.state.lifecycle
    );
}

#[test]
fn wait4_zombie_replies_pid_tag_with_status_payload() {
    // End to end: a zombie child plus a waiting parent -> the ZOMBIE arm's
    // tell_parent -> a wire reply with m_type = child pid (tag) and payload
    // m_pm_lc_wait4.status = W_EXITCODE (body).  libc's wait4 reads this layout
    // (forkexit.c:707-709).
    let mut srv = server();
    let parent_ep = seed_running(&mut srv, 5, 100);
    // Zombie child: exit(7) (W_EXITCODE(7,0) = 0x0700).
    let table = srv.table_mut();
    table.procs[6].state.lifecycle = Lifecycle::Zombie { exit_code: 7, sig_status: 0 };
    table.procs[6].identity.id.pid = 101;
    table.procs[6].identity.endpoint = Endpoint::from_generation_slot(1, 6);
    table.procs[6].state.guardianship = minix_pm::mproc::Guardianship::Normal {
        parent: minix_types::UserSlot::new(5),
    };
    table.procs[5].state.wait.waiting = true;
    table.procs[5].state.wait.target = minix_pm::mproc::WaitTarget::AnyChild;

    let mut wait_msg = request(3, parent_ep); // PM_WAIT4 = 3
    wait_msg.m_u.m_lc_pm_wait4.pid = -1;
    srv.transport_mut()
        .queue_receive(wait_msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, parent_ep);
    assert_eq!(sent[0].1.m_type, 101, "tag = child pid");
    let status = unsafe { sent[0].1.m_u.m_pm_lc_wait4.status };
    assert_eq!(status, 7 << 8, "body = W_EXITCODE(7, 0)");
    // The zombie has been reaped into TOLD_PARENT.
    assert!(matches!(
        srv.table().procs[6].state.lifecycle,
        Lifecycle::ToldParent { .. }
    ));
}

#[test]
fn wait4_without_children_replies_echild() {
    // C: the tail of do_wait4 (forkexit.c:550-563) — no matching child ->
    // ECHILD, independent of WNOHANG.
    let mut srv = server();
    let caller_ep = seed_running(&mut srv, 5, 100);

    let mut wait_msg = request(3, caller_ep); // PM_WAIT4 = 3
    wait_msg.m_u.m_lc_pm_wait4.pid = -1; // any child
    wait_msg.m_u.m_lc_pm_wait4.options = 0;
    wait_msg.m_u.m_lc_pm_wait4.addr = 0;
    srv.transport_mut()
        .queue_receive(wait_msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, caller_ep);
    // Error replies carry m_type < 0 = -errno (the C _syscall client contract,
    // errno.h:189 _SIGN=- for _SYSTEM; the fork arm uses the same convention).
    // This test previously pinned a positive value, i.e. an old convention
    // opposite to ground truth (it had been red ever since it was written); it
    // is corrected here together with the sign alignment across this file.
    assert_eq!(sent[0].1.m_type, -minix_types::ECHILD);
}

#[test]
fn kill_unknown_pid_replies_esrch() {
    // C: do_kill -> check_sig (signal.c:597+) — no matching process -> ESRCH,
    // returned through the main loop's reply(caller, errno).
    let mut srv = server();
    let caller_ep = seed_running(&mut srv, 5, 100);

    let mut kill_msg = request(11, caller_ep); // PM_KILL = 11
    kill_msg.m_u.m_lc_pm_kill.pid = 9999; // nonexistent PID
    kill_msg.m_u.m_lc_pm_kill.signo = 15; // SIGTERM
    srv.transport_mut()
        .queue_receive(kill_msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, caller_ep);
    // Error replies carry m_type < 0 = -errno (same as above).
    assert_eq!(sent[0].1.m_type, -minix_types::ESRCH);
}

#[test]
fn kill_on_fork_suspended_child_stops_it_and_records_pending() {
    // kill(child) inside the fork suspension window (child slot in VFS_CALL,
    // forkexit.c:130) -> sig_proc's VFS_CALL branch marks the signal pending and
    // stops the child via sys_delay_stop (C signal.c:425-443): the child must
    // not issue another call before PM re-examines its signals.
    let mut srv = server();
    let parent_ep = seed_running(&mut srv, 5, 100);

    // Full fork chain: scripted VM_FORK OK -> child slot 1 hangs in VFS_CALL.
    let mut vm_reply = Message {
        m_type: minix_types::OK,
        ..Default::default()
    };
    vm_reply.m_u.m_m1.m1i3 = Endpoint::from_generation_slot(2, 1).0;
    srv.transport_mut().queue_sendrec_reply(vm_reply);
    srv.transport_mut()
        .queue_receive(request(2, parent_ep), IpcStatus::default());
    assert_eq!(srv.run_once(), RunStep::Handled);
    assert!(srv.table().procs[1].state.block.is_vfs_blocked());

    // The parent kills the child with SIGTERM.
    let child_pid = srv.table().procs[1].identity.id.pid;
    let mut kill_msg = request(11, parent_ep);
    kill_msg.m_u.m_lc_pm_kill.pid = child_pid;
    kill_msg.m_u.m_lc_pm_kill.signo = 15; // SIGTERM
    srv.transport_mut()
        .queue_receive(kill_msg, IpcStatus::default());
    assert_eq!(srv.run_once(), RunStep::Handled);

    // Child: SIGTERM pending + PROC_STOPPED (C signal.c:426/441-443).
    let bit = 1u64 << (15 - 1);
    assert_eq!(srv.table().procs[1].resources.signals.pending & bit, bit);
    assert!(srv.table().procs[1].state.block.stopped, "stop_proc must stop the VFS-suspended child");
}

#[test]
fn unwired_call_replies_enosys() {
    // 04-ipc-dispatch.md §3.6 — an out-of-range unregistered call number ->
    // ENOSYS, sent by the main loop as reply(who_p, ENOSYS) (C main.c:102-106).
    // (All of 1..=47 is wired up, so the "registered but unimplemented" class no
    // longer exists; the anchor is now the unregistered number 48.)
    let mut srv = server();
    let caller_ep = seed_running(&mut srv, 5, 100);

    srv.transport_mut()
        .queue_receive(request(48, caller_ep), IpcStatus::default()); // unregistered (> NR_PM_CALLS)

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, caller_ep);
    // Error replies carry m_type < 0 = -errno (same as above).
    assert_eq!(sent[0].1.m_type, -minix_types::ENOSYS);
}

#[test]
#[should_panic(expected = "handle_vfs_reply failed")]
fn corrupt_vfs_reply_is_fail_fast_not_enosys() {
    // C: main.c:84-87 — IS_VFS_PM_RS && source==VFS takes the first
    // interception path into the handle_vfs_reply state machine; when the state
    // is corrupt (here: a FORK_REPLY with no matching pending VFS_CALL) C falls
    // back to panic (main.c:317-319/324-325), and Rust mirrors that.
    // Key contract: VFS replies are consumed by the first path and never fall
    // into the PM dispatch table's ENOSYS fallback (0x98x is not in the 0..=0xff
    // call family; landing there would only produce a misleading ENOSYS).
    let mut srv = server();
    // VFS itself must already be in the table (run_once's pm_isokendpt checks source==VFS).
    let vfs_ep = Endpoint::VFS;
    srv.table_mut().procs[1].identity.endpoint = vfs_ep;
    srv.table_mut().procs[1].identity.id.pid = 1;
    srv.table_mut().procs[1].state.lifecycle = Lifecycle::Running;

    // VFS_PM_FORK_REPLY (0x980+7) with no matching pending VFS_CALL -> the state
    // machine errors -> run_once panics (fail-fast, 04-ipc-dispatch.md §1.5).
    srv.transport_mut()
        .queue_receive(request(0x980 + 7, vfs_ep), IpcStatus::default());
    let _ = srv.run_once();
}

#[test]
fn kill_termination_tells_vfs_exit() {
    // A successful kill that terminates the target must send VFS_PM_EXIT to VFS
    // over the server's real channel (C forkexit.c:350-358 calls tell_vfs
    // unconditionally) — before the fix, that message was swallowed by a
    // one-shot mock built inside sig_proc_exit, so VFS never learned the process
    // had died.  SIGKILL's default disposition -> ordinary termination ->
    // zombify + VFS_PM_EXIT; the termination chain also tells VM (vm_willexit)
    // over the same channel.
    let mut srv = server();
    let caller_ep = seed_running(&mut srv, 5, 100);
    seed_running(&mut srv, 6, 101);

    let mut kill_msg = request(11, caller_ep); // PM_KILL = 11
    kill_msg.m_u.m_lc_pm_kill.pid = 101;
    kill_msg.m_u.m_lc_pm_kill.signo = 9; // SIGKILL
    srv.transport_mut()
        .queue_receive(kill_msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert!(
        sent.iter()
            .any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
        "kill must notify VFS with VFS_PM_EXIT, sent={:?}",
        sent.iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );
    // The termination chain also tells VM over the same channel (vm_willexit).
    assert!(
        sent.iter()
            .any(|(ep, m)| *ep == Endpoint::VM && m.m_type == minix_types::VM_WILLEXIT as i32),
        "exit chain must tell VM_WILLEXIT, sent={:?}",
        sent.iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );
    // The caller receives the kill success reply 0 (the dispatch Kill arm's fixed reply value).
    assert!(
        sent.iter().any(|(ep, m)| *ep == caller_ep && m.m_type == 0),
        "caller must receive success reply 0, sent={:?}",
        sent.iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );
}

#[test]
fn ptrace_attach_runs_full_chain_and_replies_with_payload() {
    // PM_PTRACE(8) -> the full do_trace(T_ATTACH) chain: permission gates ->
    // tracer installed + TO_NOEXEC (trace.c:88) -> sig_proc(SIGSTOP, TRUE)
    // (trace.c:90, entering trace_stop via sig_proc's TRACE branch: kernel T_STOP
    // + stopped set) -> OK reply with an m_pm_lc_ptrace.data=0 payload
    // (trace.c:92).
    let mut srv = server();
    let debugger_ep = seed_running(&mut srv, 5, 100);
    let _target_ep = seed_running(&mut srv, 6, 200);

    let mut msg = request(8, debugger_ep); // PM_PTRACE
    msg.m_u.m_lc_pm_ptrace.pid = 200;
    msg.m_u.m_lc_pm_ptrace.req = 9; // T_ATTACH
    srv.transport_mut().queue_receive(msg, IpcStatus::default());
    assert_eq!(srv.run_once(), RunStep::Handled);

    let table = srv.table();
    // Tracer installed + TO_NOEXEC.
    assert_eq!(table.procs[6].tracer(), Some(minix_types::UserSlot::new(5)));
    assert!(table.procs[6].state.guardianship.trace_options().contains(
        minix_pm::mproc::TraceOptions::NOEXEC
    ));
    // The child is stopped by trace_stop (the mock gateway's T_STOP succeeds).
    assert!(table.procs[6].state.trace.stopped);
    assert_eq!(table.procs[6].resources.signals.trace_mask, 1u64 << (17 - 1));
    // Reply: m_type = OK, payload data = 0.
    let sent = srv.transport().sent();
    let (_, reply) = sent
        .iter()
        .find(|(ep, _m)| *ep == debugger_ep)
        .expect("debugger must be replied");
    assert_eq!(reply.m_type, 0);
    assert_eq!(unsafe { reply.m_u.m_pm_lc_ptrace.data }, 0);
}

#[test]
fn kernel_sigksig_notify_drains_pending_kernel_signals() {
    // The kernel delivers the sigset bitmap through a SYSTEM-source notify (the
    // SIGKSIG bit, sef_signal.c:104-108) -> run_once's notification branch drives
    // the fetch loop -> getksig/endksig acknowledges each signal -> process_ksig
    // handles it (SIGTERM terminates).  Previously the notify branch recognised
    // only CLOCK and SIGKSIG was silently dropped.
    // Scripted: the first getksig returns (target, SIGTERM bit), then None.
    // The target process's endpoint is only known after seeding, so compute it
    // from the slot first (seed's endpoint = from_generation_slot(1, slot),
    // deterministic).
    let target_ep = Endpoint::from_generation_slot(1, 5);
    let mut srv = minix_pm::init::PmServer::with_kernel_gateway(
        BootParams::placeholder(),
        TestIpcTransport::new(),
        Box::new(MockKernelGateway {
            user: 0,
            sys: 0,
            ksig_script: vec![
                Some((target_ep.0, [1u32 << (15 - 1), 0, 0, 0])),
                None,
            ],
        }),
    );
    let _ = seed_running(&mut srv, 5, 42);

    // Kernel notification: a SYSTEM source is the fetch trigger (SigSet is now
    // 128 bits like C's sigset_t, so SIGKSIG bit 73 is itself encodable; the
    // SYSTEM notification remains the only carrier of "something accumulated",
    // and PM triggers on the SYSTEM source).
    let notify = Message { m_source: Endpoint::SYSTEM, ..Default::default() };
    srv.transport_mut()
        .queue_receive(notify, IpcStatus { flags: 4 });
    assert_eq!(srv.run_once(), RunStep::Handled);

    // The fetch took effect: the target process is terminated by SIGTERM (default disposition).
    let table = srv.table();
    assert!(
        matches!(
            table.procs[5].state.lifecycle,
            minix_pm::mproc::Lifecycle::Zombie { .. } | minix_pm::mproc::Lifecycle::TraceZombie { .. }
        ),
        "drained SIGTERM must terminate, got {:?}",
        table.procs[5].state.lifecycle
    );
}

// ---- Boundary pins for silent drops on the VFS lane ----
//
// To break the fb201 ping-pong (FS messages stamped status=-1 leaking into the
// PM/VFS lane, with PM's ENOSYS reflection acting as the amplifier), the guard
// once dropped every non-RS-family VFS source silently.  It was then narrowed
// to "drop only call numbers that are not registered PM calls" — otherwise a
// legitimate PM_EXEC_NEW(43, callnr.h:56) from VFS was swallowed too and the
// exec worker waited for ever (observed on real hardware).  The two pins below
// each protect one half:
// 1. 43 must reach dispatch (tightening the guard again breaks exec, which took
//    days to localise on real hardware);
// 2. unregistered numbers must produce zero replies (the ping-pong amplifier
//    must not come back).

/// Seed VFS as Running (needed so source==VFS passes pm_isokendpt).
fn seed_vfs(srv: &mut minix_pm::init::PmServer<TestIpcTransport>) -> Endpoint {
    let table = srv.table_mut();
    table.procs[1].identity.endpoint = Endpoint::VFS;
    table.procs[1].identity.id.pid = 1;
    table.procs[1].state.lifecycle = Lifecycle::Running;
    Endpoint::VFS
}

#[test]
fn vfs_exec_new_passes_drop_gate_reaches_dispatch() {
    let mut srv = server();
    let vfs_ep = seed_vfs(&mut srv);

    // PM_EXEC_NEW=43 (callnr.h:56): endpt@0 deliberately holds an invalid
    // endpoint value, so pm_isokendpt in the dispatch arm do_newexec must fail
    // and return an error; the pin's criterion is "VFS receives a reply" (the
    // message reached dispatch), not the particular errno.
    let mut msg = request(43, vfs_ep);
    msg.m_u.m_m1.m1i1 = 0x2ff; // endpt@0 (decode::exec_new raw[0..4]),
    // deliberately an invalid endpoint value — pm_isokendpt in the dispatch arm
    // do_newexec must fail and return an error; the pin's criterion is "VFS
    // receives a reply" (the message reached dispatch), not the particular errno.
    srv.transport_mut()
        .queue_receive(msg, IpcStatus::default());

    assert_eq!(srv.run_once(), RunStep::Handled);

    let sent = srv.transport().sent();
    assert!(
        sent.iter().any(|(ep, _)| *ep == vfs_ep),
        "PM_EXEC_NEW from VFS must reach dispatch and be replied, sent={:?}",
        sent.iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );
}

#[test]
fn vfs_unregistered_mtypes_silently_dropped() {
    let mut srv = server();
    let vfs_ep = seed_vfs(&mut srv);

    // fb201 shape (1): FS stamped status=-1 (negative m_type, the flood itself).
    srv.transport_mut()
        .queue_receive(request(-1, vfs_ep), IpcStatus::default());
    assert_eq!(srv.run_once(), RunStep::Handled);
    assert!(
        srv.transport().sent().is_empty(),
        "stamped m_type=-1 must be silently dropped, sent={:?}",
        srv.transport().sent().iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );

    // Shape (2): the out-of-range unregistered number 48 (> NR_PM_CALLS, the same
    // as unwired_call_replies_enosys for non-VFS sources — but a VFS source must
    // stay silent here and must not send an ENOSYS reflection back to VFS, which
    // would revive the ping-pong).
    srv.transport_mut()
        .queue_receive(request(48, vfs_ep), IpcStatus::default());
    assert_eq!(srv.run_once(), RunStep::Handled);
    assert!(
        srv.transport().sent().is_empty(),
        "unregistered m_type=48 from VFS must be silently dropped, sent={:?}",
        srv.transport().sent().iter().map(|(ep, m)| (ep.get(), m.m_type)).collect::<Vec<_>>()
    );
}
