//! PM main loop's three-way dispatch (C: `main.c:84-103`).
//!
//! Document: `rewrite-notes/04-stage-pm/04-ipc-dispatch.md`.
//!
//! # Correspondence with C
//!
//! - `IS_VFS_PM_RS(call_nr) && who_e == VFS_PROC_NR` -> `handle_vfs_reply()`
//!   (main.c:84-87) -- that interception happens on the first leg of
//!   `PmServer::run_once`'s main loop (see `crate::init`) and does not enter
//!   this three-way dispatch; the state machine belongs to
//!   05-vfs-interaction.md.
//! - `call_nr == PROC_EVENT_REPLY` -> `do_proc_event_reply()`
//!   (main.c:88-89) -- this layer only builds the hook; the semantics belong
//!   to 06-event-subscription.md.
//! - `IS_PM_CALL(call_nr)` -> `call_vec[call_nr - PM_BASE]()` (main.c:90-101)
//!   -- see [`crate::ipc::calls::dispatch_pm_call`].
//! - Everything else -> ENOSYS (main.c:102-103).
//!
//! `ReplyIntent` models C's `result != SUSPEND -> reply()` (main.c:106):
//! SUSPEND (com.h:1151, value -998) is made explicit as
//! [`ReplyIntent::ReplyLater`] / [`ReplyIntent::NoReply`] (`[ARCH: A-6]`,
//! plan.md section 7.3).

use super::transport::IpcTransport;
use crate::ipc::calls::{PmCall, dispatch_pm_call};
use crate::mproc::ProcTable;
use minix_types::{ENOSYS, Endpoint, Message, UserSlot};

/// VFS->PM reply message type base. C: `VFS_PM_RS_BASE` -- com.h:514.
///
/// Converged into minix-types (single truth with com.h:514), see
/// 05-design.v1.md.
pub use minix_types::VFS_PM_RS_BASE;

/// Process-event subscriber reply message type. C: `PROC_EVENT_REPLY` --
/// `COMMON_RS_BASE + 0` -- com.h:619 (COMMON_RS_BASE = 0xE80, com.h:598).
/// Converged into the single minix-types truth (the same shape as the
/// `VFS_PM_RS_BASE` re-export above), removing the local 0xE80 duplicate
/// address.
pub use minix_types::PROC_EVENT_REPLY;

/// The reply intent the main loop derives from one dispatch (`[ARCH: A-6]`:
/// C's `SUSPEND` made explicit).
///
/// In C the handler returns `result`, and the main loop does
/// `if (result != SUSPEND) reply(...)` (main.c:106); SUSPEND is the -998 magic
/// number that covers three sub-cases at once: reply while waiting (do_wait4
/// -> tell_parent), asynchronous reply (do_exec/do_set -> handle_vfs_reply),
/// and never reply (do_exit). Rust distinguishes them with an enum:
///
/// - [`ReplyIntent::Reply`]: the main loop immediately does
///   `reply(slot, code)` (C: result is not SUSPEND).
/// - [`ReplyIntent::ReplyLater`]: no reply now; an asynchronous path replies
///   later (C: result == SUSPEND with a later replier, such as VFS's reply in
///   05).
/// - [`ReplyIntent::NoReply`]: never reply (C: do_exit returns SUSPEND with no
///   later replier, forkexit.c:246-266; the 04 layer does not produce it yet,
///   it serves 09).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyIntent {
    /// Send `code` as the reply `m_type` at once (code may be OK=0, an errno,
    /// or a payload value such as a pid).
    Reply(i32),
    /// No reply now; an asynchronous path replies later (C: SUSPEND).
    ReplyLater,
    /// Never reply (C: do_exit's sub-case; the 04 layer has no producer yet).
    NoReply,
}

/// C: `IS_VFS_PM_RS(type)` -- com.h:517: `((type) & ~0x7f) == VFS_PM_RS_BASE`.
///
/// Delegates to the minix-types function of the same name (single truth), see
/// 05-design.v1.md.
pub fn is_vfs_pm_rs(nr: i32) -> bool {
    minix_types::is_vfs_pm_rs(nr)
}

/// C: `IS_PM_CALL(type)` -- callnr.h:11: `((type) & ~0xff) == PM_BASE`.
pub fn is_pm_call(nr: i32) -> bool {
    (nr & !0xff) == 0
}

/// The main loop's three-way dispatch (C: main.c:84-103).
///
/// Preconditions (already guaranteed by the caller `PmServer::run_once`): the
/// message is not a notification, `msg.m_source` has passed `pm_isokendpt`,
/// and the caller is not EXITING.
///
/// # Note
///
/// The VFS->PM asynchronous reply (`IS_VFS_PM_RS && source == VFS`) is already
/// intercepted at the entry of `run_once` and handled by the
/// `handle_vfs_reply` state machine (main.c:84-87 is the first leg of the main
/// loop), so it **does not** enter this dispatch. This function handles the
/// event-reply and PM-call legs -- every PM call goes through the single
/// dispatch table of [`dispatch_pm_call`] (`[ARCH: A-5]`); the main loop must
/// not inline-intercept it.
///
/// # Returns
///
/// [`ReplyIntent`]: the main loop decides from it whether to reply (equivalent
/// to main.c:106).
pub fn dispatch_message<T: IpcTransport>(
    table: &mut ProcTable,
    events: &mut crate::event::EventRegistry,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
    timers: &mut crate::timer::TimerFaces,
    caller: UserSlot,
    msg: &Message,
) -> ReplyIntent {
    let call_nr = msg.m_type;

    if call_nr == PROC_EVENT_REPLY {
        // C: main.c:88-89 -- do_proc_event_reply() (06-event-subscription.md).
        events.do_proc_event_reply(msg, caller, table, transport, kern)
    } else if is_pm_call(call_nr) {
        // C: main.c:90-101 -- call_index = call_nr - PM_BASE; out of range or
        // NULL -> ENOSYS.
        match PmCall::from_call_nr(call_nr) {
            Some(call) => {
                dispatch_pm_call(call, table, events, transport, kern, timers, caller, msg)
            }
            // An unwired PM number -> ENOSYS, negated the same way as C's
            // `_syscall` contract (m_type < 0 = errno).
            None => ReplyIntent::Reply(-ENOSYS),
        }
    } else {
        // C: main.c:102-103 -- not a PM call -> ENOSYS (same negation
        // convention).
        ReplyIntent::Reply(-ENOSYS)
    }
}

// ── PM -> VM service calls (synchronous task calls, aligned with C libsys) ──

/// Send a VM_FORK request to the VM service (synchronous).
///
/// C: `vm_fork` (libsys, `minix3/minix/lib/libsys/vm_fork.c:16-25`) --
/// `_taskcall(VM_PROC_NR, VM_FORK, &m)`: the request carries `VMF_ENDPOINT`
/// (parent endpoint, m1i1) and `VMF_SLOTNO` (child slot, m1i2) in the m1
/// payload, and the reply carries the child endpoint in `VMF_CHILD_ENDPOINT`
/// (m1i3). On vm_fork failure C's do_fork propagates the errno straight to the
/// fork caller (`forkexit.c:77-79`), and once vm_fork succeeds, fork may not
/// fail any more (`forkexit.c:83` "PM may not fail fork after call to
/// vm_fork()") -- so this function is the only VM dependency on the fork path,
/// and its failure must be discovered before any unrollbackable process-table
/// change.
///
/// # Errors
///
/// Both a transport failure and a non-OK VM reply (negative errno) converge to
/// [`ForkCoordError::VmError`]. Fine-grained errno propagation needs a carrier
/// variant on `PmError` (the minix-types shared layer, registered as E7 in
/// edge_todo.md); for now it fails with a uniform VmError -- the failure
/// semantics are correct, only the granularity waits on shared-layer support.
pub fn vm_fork<T: IpcTransport>(
    transport: &mut T,
    parent: Endpoint,
    child_slot: UserSlot,
) -> Result<Endpoint, crate::fork::ForkCoordError> {
    // Request encoding: m1i1 = VMF_ENDPOINT, m1i2 = VMF_SLOTNO (C's VMF_*
    // macros are the m1 field aliases; VmForkIn only implements the VM-side
    // DecodeFromM1, so the PM-side sender writes the same layout by hand,
    // matching the decode order at ipc/vm.rs:841-849).
    let mut msg = Message {
        m_type: minix_types::VM_FORK as i32,
        ..Default::default()
    };
    msg.m_u.m_m1.m1i1 = parent.0;
    msg.m_u.m_m1.m1i2 = child_slot.get() as i32;
    transport
        .sendrec(Endpoint::VM, &mut msg)
        .map_err(|_| crate::fork::ForkCoordError::VmError)?;
    // C: a non-OK taskcall return -> errno propagation (forkexit.c:78-79). A
    // negative errno is taken as a VM refusal; only m_type == OK reads m1i3.
    if msg.m_type != minix_types::OK {
        return Err(crate::fork::ForkCoordError::VmError);
    }
    // Reply decoding: VMF_CHILD_ENDPOINT in m1i3
    // (minix-types EncodeToM1 for VmForkOut, ipc/vm.rs:852-856).
    Ok(Endpoint(unsafe { msg.m_u.m_m1 }.m1i3))
}

/// Send a VM_WILLEXIT notification to the VM service (synchronous).
///
/// C: `vm_willexit` (libsys, same file as `vm_exit`: `libsys/vm_exit.c:25-35`;
/// there is no separate `vm_willexit.c`) -- `_taskcall(VM_PROC_NR,
/// VM_WILLEXIT, &m)`: the payload is `VMWE_ENDPOINT` (m1i1, `com.h:644`) with
/// no reply payload. The taskcall return value is the result -- on non-OK C's
/// caller panics (`forkexit.c:332-334` "exit_proc: vm_willexit failed"),
/// because VM's memory accounting depends on that notification and losing it
/// would unbalance it forever.
///
/// # Errors
///
/// Transport failure or a non-OK VM reply -> `Err(errno value)`; the caller
/// (step 6 of `exit_proc` in `exit.rs`) panics with the same text to match C.
pub fn vm_willexit<T: IpcTransport + ?Sized>(transport: &mut T, endpoint: Endpoint) -> Result<(), i32> {
    let mut msg = Message {
        m_type: minix_types::VM_WILLEXIT as i32,
        ..Default::default()
    };
    msg.m_u.m_m1.m1i1 = endpoint.0;
    transport
        .sendrec(Endpoint::VM, &mut msg)
        .map_err(|_| minix_types::EIO)?;
    if msg.m_type != minix_types::OK {
        return Err(msg.m_type);
    }
    Ok(())
}

/// Send a VM_EXIT notification to the VM service (synchronous).
///
/// C: `vm_exit` (libsys, `minix3/minix/lib/libsys/vm_exit.c`) --
/// `_taskcall(VM_PROC_NR, VM_EXIT, &m)`: the payload is `VME_ENDPOINT` (m1i1,
/// `com.h:631`) with no reply payload. On failure C's caller panics
/// (`forkexit.c:455-457` "exit_restart: vm_exit failed") -- the page tables
/// ended with the process, so a VM that does not reclaim leaks them forever.
///
/// # Errors
///
/// Transport failure or a non-OK VM reply -> `Err(errno value)`; the caller
/// (step 5 of `exit_restart` in `exit.rs`) panics with the same text to match C.
pub fn vm_exit<T: IpcTransport + ?Sized>(transport: &mut T, endpoint: Endpoint) -> Result<(), i32> {
    let mut msg = Message {
        m_type: minix_types::VM_EXIT as i32,
        ..Default::default()
    };
    msg.m_u.m_m1.m1i1 = endpoint.0;
    transport
        .sendrec(Endpoint::VM, &mut msg)
        .map_err(|_| minix_types::EIO)?;
    if msg.m_type != minix_types::OK {
        return Err(msg.m_type);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::{TimerCtl, TimerFaces, VTimerCtl};
    use minix_types::{Clock, Endpoint};

    /// Idle timer outlet (tests only need dispatch to pass; they do not
    /// consume timer semantics).
    pub struct TestTimerCtl;
    impl TimerCtl for TestTimerCtl {
        fn set(&mut self, _ep: Endpoint, _ticks: Clock) {}
        fn cancel(&mut self, _ep: Endpoint) {}
        fn exptime(&self, _ep: Endpoint) -> Option<Clock> {
            None
        }
        fn now(&self) -> Clock {
            0
        }
    }
    pub struct TestVTimerCtl;
    impl crate::timer::VTimerCtl for TestVTimerCtl {
        fn vtimer(
            &mut self,
            _ep: Endpoint,
            _which: crate::timer::ItimerWhich,
            _set: Option<Clock>,
            _get: Option<&mut Clock>,
        ) -> i32 {
            0
        }
    }
    fn timers() -> TimerFaces<'static> {
        // Leaking construction for tests (the 'static borrow is carried by the
        // leaked Box).
        timers_with(Box::leak(Box::new(0i32)))
    }
    /// Inject an abort_flag the subject under test can read (for the Reboot
    /// arm's assertion).
    fn timers_with<'a>(abort_flag: &'a mut i32) -> TimerFaces<'a> {
        let tctl: &'static mut dyn TimerCtl = Box::leak(Box::new(TestTimerCtl));
        let vctl: &'static mut dyn crate::timer::VTimerCtl = Box::leak(Box::new(TestVTimerCtl));
        let svrctl: &'static mut crate::misc::ParamStore =
            Box::leak(Box::new(crate::misc::ParamStore::new(alloc::string::String::new())));
        let call_stats: &'static mut [u8] = Box::leak(Box::new([0u8; 192]));
        TimerFaces {
            tctl,
            vctl,
            system_hz: 100,
            svrctl_store: svrctl,
            call_stats,
            abort_flag,
        }
    }

    use crate::event::EventRegistry;
    use crate::ipc::TestIpcTransport;
    use crate::mproc::Lifecycle;
    use minix_types::Message;


    /// Kernel-gateway mock for tests (sys_kill/sys_clear always OK).
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
    }
    fn msg_with(m_type: i32, source: Endpoint) -> Message {
        let mut m = Message { m_type, ..Message::default() };
        m.m_source = source;
        m
    }

    /// Build the dispatch_message test environment: register a Running process
    /// at `slot` (with `kernel=true`, set Privilege::Kernel for the
    /// event-reply kernel gate).
    fn setup(slot: usize, ep: Endpoint, kernel: bool) -> (ProcTable, EventRegistry, TestIpcTransport) {
        let mut table = ProcTable::new();
        table.procs[slot].identity.endpoint = ep;
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        if kernel {
            table.procs[slot].resources.privilege = crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        }
        (table, EventRegistry::new(), TestIpcTransport::new())
    }

    #[test]
    fn test_is_vfs_pm_rs_matches_c() {
        // com.h:517 -- any of the low 7 bits within the 0x980 family matches.
        assert!(is_vfs_pm_rs(0x980 + 7)); // VFS_PM_FORK_REPLY
        assert!(is_vfs_pm_rs(0x980));
        assert!(!is_vfs_pm_rs(0x980 + 0x80)); // exceeds the low 7 bits
        assert!(!is_vfs_pm_rs(0x900)); // VFS_PM_RQ
        assert!(!is_vfs_pm_rs(0xE80)); // COMMON_RS
    }

    #[test]
    fn test_is_pm_call_matches_c() {
        // callnr.h:11 -- any type within 0x00..0xff "is the PM call family" (a
        // decoded but unregistered number still gets ENOSYS, matching C's
        // out-of-range slot).
        assert!(is_pm_call(2)); // PM_FORK
        assert!(is_pm_call(47)); // PM_GETSYSINFO
        assert!(is_pm_call(0)); // reserved value: in the family but unregistered
        assert!(!is_pm_call(0x100)); // exceeds the low 8 bits
        assert!(!is_pm_call(-1));
    }

    #[test]
    fn test_vfs_pm_rs_from_non_vfs_source_is_enosys() {
        // The VFS->PM asynchronous reply (IS_VFS_PM_RS && source == VFS) is
        // intercepted by `run_once` on the first leg of the main loop
        // (main.c:84-87) and does not enter this three-way dispatch; only a
        // non-VFS source falls through to dispatch_message here, and then it is
        // not in the PM call family -> ENOSYS (the main.c:102-103 fallback).
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        let mut kern = NoopKernel;
        let mut timers = timers();
        let intent = dispatch_message(
            &mut table,
            &mut events,
            &mut transport,
            &mut kern,
            &mut timers,
            UserSlot::new(3),
            &msg_with(0x980 + 7, Endpoint::RS),
        );
        assert_eq!(intent, ReplyIntent::Reply(-ENOSYS));
    }

    #[test]
    fn test_proc_event_reply_routes_to_reply_later() {
        // C: main.c:88-89 -- PROC_EVENT_REPLY -> do_proc_event_reply().
        // Kernel caller + default message (event bit 0 -> endpoint resolution
        // fails) -> the SUSPEND precondition (event.c:241-245 -> ReplyLater).
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, true);
        let mut kern = NoopKernel;
        let mut timers = timers();
        let intent = dispatch_message(
            &mut table,
            &mut events,
            &mut transport,
            &mut kern,
            &mut timers,
            UserSlot::new(3),
            &msg_with(PROC_EVENT_REPLY, Endpoint::RS),
        );
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_proc_event_reply_from_user_process_is_enosys() {
        // event.c:232-233 -- only system services may reply; misuse by an
        // ordinary process -> ENOSYS reply.
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        let mut kern = NoopKernel;
        let mut timers = timers();
        let intent = dispatch_message(
            &mut table,
            &mut events,
            &mut transport,
            &mut kern,
            &mut timers,
            UserSlot::new(3),
            &msg_with(PROC_EVENT_REPLY, Endpoint::RS),
        );
        assert_eq!(intent, ReplyIntent::Reply(-ENOSYS));
    }

    #[test]
    fn test_pm_call_routes_to_dispatch_pm_call() {
        // C: main.c:90-101 -- IS_PM_CALL -> call_vec (the single dispatch
        // table). GETPID(4) is wired: the direct witness of routing into
        // dispatch_pm_call is the deterministic answer (pid=100+slot, the value
        // setup registered).
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        let mut kern = NoopKernel;
        let mut timers = timers();
        assert_eq!(
            dispatch_message(
                &mut table,
                &mut events,
                &mut transport,
                &mut kern,
                &mut timers,
                UserSlot::new(3),
                &msg_with(4, ep)
            ),
            ReplyIntent::Reply(0)
        );
        // GETPID's reply protocol: the return value is 0 and the pid rides the
        // staged reply payload m1i1 (get_result_intent, calls.rs:1115-1121).
        let reply = table.procs[3].ipc.reply.take().expect("reply staged");
        // SAFETY: the assertion side reads m1i1 in the written field order.
        assert_eq!(unsafe { reply.m_u.m_m1 }.m1i1, 103);
    }

    #[test]
    fn test_unknown_type_returns_enosys() {
        // C: main.c:102-103 -- not a PM call -> ENOSYS.
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        let mut kern = NoopKernel;
        let mut timers = timers();
        assert_eq!(
            dispatch_message(
                &mut table,
                &mut events,
                &mut transport,
                &mut kern,
                &mut timers,
                UserSlot::new(3),
                &msg_with(0x100, ep)
            ),
            ReplyIntent::Reply(-ENOSYS)
        );
    }

    // ── vm_willexit (PM -> VM exit notice) ──

    #[test]
    fn test_vm_willexit_encodes_endpoint_and_ok() {
        // C: libsys vm_exit.c:25-35 -- payload VMWE_ENDPOINT (m1i1,
        // com.h:644), no reply payload; taskcall returns OK.
        let mut transport = crate::ipc::TestIpcTransport::new();
        let got = vm_willexit(&mut transport, Endpoint::from_generation_slot(1, 7));
        assert!(got.is_ok());
        let (dest, sent_msg) = &transport.sent()[0];
        assert_eq!(*dest, Endpoint::VM);
        assert_eq!(sent_msg.m_type, minix_types::VM_WILLEXIT as i32);
        assert_eq!(unsafe { sent_msg.m_u.m_m1 }.m1i1, Endpoint::from_generation_slot(1, 7).0);
    }

    #[test]
    fn test_vm_willexit_refusal_is_err() {
        let mut transport = crate::ipc::TestIpcTransport::new();
        let refusal = Message { m_type: -12, ..Message::default() }; // VM refuses (negative errno)
        transport.queue_sendrec_reply(refusal);
        let got = vm_willexit(&mut transport, Endpoint::from_generation_slot(1, 7));
        assert_eq!(got.unwrap_err(), -12);
    }

    // ── vm_exit (PM -> VM page-table reclaim notice) ──

    #[test]
    fn test_vm_exit_encodes_endpoint_and_ok() {
        // C: libsys vm_exit.c -- payload VME_ENDPOINT (m1i1, com.h:631).
        let mut transport = crate::ipc::TestIpcTransport::new();
        let got = vm_exit(&mut transport, Endpoint::from_generation_slot(3, 9));
        assert!(got.is_ok());
        let (dest, sent_msg) = &transport.sent()[0];
        assert_eq!(*dest, Endpoint::VM);
        assert_eq!(sent_msg.m_type, minix_types::VM_EXIT as i32);
        assert_eq!(unsafe { sent_msg.m_u.m_m1 }.m1i1, Endpoint::from_generation_slot(3, 9).0);
    }

    #[test]
    fn test_vm_exit_refusal_is_err() {
        let mut transport = crate::ipc::TestIpcTransport::new();
        let refusal = Message { m_type: -19, ..Message::default() };
        transport.queue_sendrec_reply(refusal);
        let got = vm_exit(&mut transport, Endpoint::from_generation_slot(3, 9));
        assert_eq!(got.unwrap_err(), -19);
    }

    // ── vm_fork (PM -> VM task call) ──

    /// Build a scripted VM_FORK reply (OK + child endpoint in m1i3).
    fn vm_fork_ok_reply(child: Endpoint) -> Message {
        let mut m = Message { m_type: minix_types::OK, ..Message::default() };
        m.m_u.m_m1.m1i3 = child.0;
        m
    }

    #[test]
    fn test_vm_fork_encodes_request_and_decodes_reply() {
        // C: libsys vm_fork.c:16-25 -- _taskcall(VM_PROC_NR, VM_FORK),
        // request VMF_ENDPOINT/VMF_SLOTNO (m1i1/m1i2), reply
        // VMF_CHILD_ENDPOINT (m1i3).
        let mut transport = crate::ipc::TestIpcTransport::new();
        let child = Endpoint::from_generation_slot(2, 7);
        transport.queue_sendrec_reply(vm_fork_ok_reply(child));

        let got = vm_fork(
            &mut transport,
            Endpoint::from_generation_slot(1, 3),
            UserSlot::new(7),
        )
        .expect("vm_fork should succeed on scripted OK reply");
        assert_eq!(got, child);

        // Request wire: to VM, m_type = VM_FORK, payload m1i1/m1i2.
        let (dest, sent_msg) = &transport.sent()[0];
        assert_eq!(*dest, Endpoint::VM);
        assert_eq!(sent_msg.m_type, minix_types::VM_FORK as i32);
        let m1 = unsafe { sent_msg.m_u.m_m1 };
        assert_eq!(m1.m1i1, Endpoint::from_generation_slot(1, 3).0);
        assert_eq!(m1.m1i2, 7);
    }

    #[test]
    fn test_vm_fork_vm_refusal_is_error() {
        // C: forkexit.c:78-79 -- vm_fork returns non-OK -> errno propagates as
        // failure.
        let mut transport = crate::ipc::TestIpcTransport::new();
        let refusal = Message { m_type: -12, ..Message::default() }; // negative errno (ENOMEM)
        transport.queue_sendrec_reply(refusal);

        let result = vm_fork(
            &mut transport,
            Endpoint::from_generation_slot(1, 3),
            UserSlot::new(7),
        );
        assert_eq!(result.unwrap_err(), crate::fork::ForkCoordError::VmError);
    }

    #[test]
    fn test_vm_fork_transport_failure_is_error() {
        // A transport-layer failure (sendrec Errs before real kernel IPC
        // lands) converges to VmError as well -- never fabricate success (a
        // regression guard against the old send_vm_fork fake-success seam).
        struct FailingSendrec;
        impl IpcTransport for FailingSendrec {
            fn receive(
                &mut self,
            ) -> Result<(Message, crate::ipc::IpcStatus), crate::ipc::transport::IpcTransportError> {
                Err(crate::ipc::transport::IpcTransportError::WouldBlock)
            }
            fn send(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), crate::ipc::transport::IpcTransportError> {
                Ok(())
            }
            fn send_blocking(
                &mut self,
                _dest: Endpoint,
                _msg: &Message,
            ) -> Result<(), crate::ipc::transport::IpcTransportError> {
                Ok(())
            }
            fn sendrec(
                &mut self,
                _dest: Endpoint,
                _msg: &mut Message,
            ) -> Result<(), crate::ipc::transport::IpcTransportError> {
                Err(crate::ipc::transport::IpcTransportError::NoPerm)
            }
        }

        let mut transport = FailingSendrec;
        let result = vm_fork(
            &mut transport,
            Endpoint::from_generation_slot(1, 3),
            UserSlot::new(7),
        );
        assert_eq!(result.unwrap_err(), crate::fork::ForkCoordError::VmError);
    }

    /// root's PM_REBOOT runs the full reboot sequence -- SUSPEND replies
    /// nothing, abort_flag records how (matching glo.h:26), and the VFS slot
    /// takes a VFS_CALL. The actual sys_abort emission is in the
    /// VFS_PM_REBOOT_REPLY special case (first leg of run_once, matching
    /// misc.c:309), not in this arm.
    #[test]
    fn test_reboot_root_suspends_sets_abort_flag_and_tells_vfs() {
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        // setup's default credentials are uid 0; set User(0,0) explicitly to
        // express "a root user process".
        table.procs[3].resources.privilege =
            crate::mproc::Privilege::User(crate::mproc::Credentials::new(0, 0));
        // The VFS row is in the table (C mproc[VFS_PROC_NR]): the slot source
        // for tell_reboot. PRIV_PROC makes the broadcast SIGKILL skip it
        // (signal.c:607-608).
        table.procs[1].identity.endpoint = Endpoint::VFS;
        table.procs[1].identity.id.pid = 2;
        table.procs[1].state.lifecycle = Lifecycle::Running;
        table.procs[1].resources.privilege =
            crate::mproc::Privilege::Kernel(crate::mproc::Credentials::default());
        let mut kern = NoopKernel;
        let mut abort_flag = 0i32;
        let mut timers = timers_with(&mut abort_flag);
        let mut msg = msg_with(37, ep); // PM_REBOOT (callnr.h)
        msg.m_u.m_lc_pm_reboot.how = 0x0808;
        let intent = dispatch_message(
            &mut table,
            &mut events,
            &mut transport,
            &mut kern,
            &mut timers,
            UserSlot::new(3),
            &msg,
        );
        assert_eq!(intent, ReplyIntent::ReplyLater, "SUSPEND：不回复调用者");
        assert_eq!(abort_flag, 0x0808, "abort_flag 必须记下 how 位组");
        match table.procs[1].state.block.ipc_blocked {
            Some(crate::mproc::IpcBlockReason::VfsCall { .. }) => {}
            ref other => panic!("VFS 槽位应挂 VFS_CALL，实为 {other:?}"),
        }
    }

    /// Non-root -> EPERM synchronous reply, zero side effects (the misc.c:202
    /// gate).
    #[test]
    fn test_reboot_non_root_is_eperm_without_side_effects() {
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup(3, ep, false);
        table.procs[3].resources.privilege =
            crate::mproc::Privilege::User(crate::mproc::Credentials::new(1000, 1000));
        table.procs[1].identity.endpoint = Endpoint::VFS;
        table.procs[1].identity.id.pid = 2;
        table.procs[1].state.lifecycle = Lifecycle::Running;
        let mut kern = NoopKernel;
        let mut abort_flag = 0i32;
        let mut timers = timers_with(&mut abort_flag);
        let mut msg = msg_with(37, ep);
        msg.m_u.m_lc_pm_reboot.how = 0x0808;
        let intent = dispatch_message(
            &mut table,
            &mut events,
            &mut transport,
            &mut kern,
            &mut timers,
            UserSlot::new(3),
            &msg,
        );
        // Negation convention: m_type < 0 = -errno.
        assert_eq!(intent, ReplyIntent::Reply(-minix_types::EPERM));
        assert_eq!(abort_flag, 0, "EPERM 路径不得写 abort_flag");
        assert!(
            table.procs[1].state.block.ipc_blocked.is_none(),
            "EPERM 路径不得告知 VFS"
        );
    }
}
