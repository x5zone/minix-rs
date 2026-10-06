//! The production `FsTransport`: SEF receive, birth, replies, data plane.
//!
//! C correspondence: the loop verbs of `fsdriver_task`/`fsdriver_process`
//! (`minix3/minix/lib/libfsdriver/fsdriver.c:17-97`) plus the birth face
//! every server's `sef_startup` carries (`minix3/minix/lib/libsys/sef.c`
//! tail: wait for the init request from RS, run the init callback, report
//! `RS_INIT` with the result back). One struct, injected with
//! [`RtIpc`](crate::ipc::RtIpc), shared by every file server binary.

use alloc::boxed::Box;

use minix_fs::protocol::{RequestNumber, TransactionId};
use minix_fs::task::{Envelope, FsReply, FsTransport, Incoming};
use minix_types::{Endpoint, Message};

use crate::ipc::{Receipt, RtIpc};
use crate::wire;

/// The hooks a specific server contributes to the common runtime shape
/// (`sef_setcb_signal_handler` + `sef_setcb_init_fresh`,
/// `minix3/minix/fs/mfs/main.c:34-38`).
pub struct ServerHooks {
    /// Signal decision (see [`SignalDecision`]).
    pub on_signal: SignalDecision,
    /// Birth callback: `Fresh` runs the server's init sequence; the
    /// stateful kinds never reach it (refused upstream).
    pub init: Box<dyn FnMut(InitKind) -> Result<(), i32>>,
}

/// Run one file server to completion over the production transports.
///
/// This is the `main` shape every file server shares — C:
/// `env_setargs` + `sef_local_startup` + `fsdriver_task` (`mfs/main.c:
/// 31-45`); the argument-parsing half lands with the runtime-boot batch,
/// the rest is here. Returns when the task loop ends (termination decided,
/// receive cancelled, or the unmount drained the loop).
pub fn serve_with<D: minix_fs::driver::FsDriver, I: RtIpc>(
    ipc: I,
    vfs: Endpoint,
    driver: D,
    hooks: ServerHooks,
) -> Result<(), i32> {
    let mut transport = FsRt::new(ipc, vfs, hooks.on_signal, hooks.init);
    let mut server = minix_fs::driver::Server::new(driver);
    minix_fs::task::run(&mut server, &mut transport);
    Ok(())
}

/// [`serve_with`] over the production trap transports.
pub fn serve<D: minix_fs::driver::FsDriver>(
    driver: D,
    hooks: ServerHooks,
) -> Result<(), i32> {
    serve_with(crate::ipc::SysRtIpc, Endpoint::VFS, driver, hooks)
}

/// The SYSTEM notification's signal decision hook.
///
/// C shape: `do_sef_signal_request` (sef_signal.c:88-) walks the kernel
/// signal range `SIGK_FIRST..=SIGK_LAST` (71..=74,
/// `sys/sys/signal.h:276-277`) and calls the registered handler once per
/// pending signal; a signal manager delivers one process signal per message
/// (`sef_signal.c:117-128`). Either way the handler's argument is a signal
/// number — so this hook takes one number and answers what to do with it.
///
/// The file servers built on this runtime differ in one step only, and the
/// difference is C's, not ours:
/// - `mfs` syncs then terminates (`mfs/main.c:38` registers
///   `sef_cb_signal_handler`, whose body `main.c:70-78` is "ignore anything
///   but SIGTERM, then `fs_sync()`, then `fsdriver_terminate()`");
/// - `pfs` terminates without syncing (`pfs.c:416` registers `pfs_signal`,
///   body `pfs.c:381-388`), and `ptyfs` is the same shape
///   (`ptyfs.c:407` + `ptyfs.c:392-397`);
/// - `procfs` has no handler of its own because its main loop comes from the
///   virtual tree file system library, which registers `got_signal`
///   (`libvtreefs/vtreefs.c:57`, body `vtreefs.c:39-46`) — SIGTERM only,
///   no sync.
///
/// Terminating here means the loop leaves without a reply: C's
/// `fsdriver_terminate()` clears the running flag and cancels the pending
/// receive (`fsdriver.c:68-74`) — that cancel is the library's escape hatch
/// ([`minix_sef::SefCancel`], PD-27): the turn cancels from inside the
/// callback, the library answers `Err(EINTR)`, and the decision maps to the
/// [`Incoming::SyncThenCancelled`]/[`Incoming::Cancelled`] return without
/// consuming a further message.
pub type SignalDecision = Box<dyn FnMut(i32) -> SignalAction>;

/// What a file server makes of one signal number — re-exported from
/// [`minix_sef`] (PD-25: one vocabulary definition lives with the SEF
/// contract; the FS runtime and its four servers consume it unchanged).
pub use minix_sef::SignalAction;

/// What the birth callback is being asked to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitKind {
    /// Fresh start (`SEF_INIT_FRESH`, sef.h:93).
    Fresh,
    /// Live update (`SEF_INIT_LU`): the stateful restart this runtime does
    /// not model yet — reported back to RS as refused, never faked.
    LiveUpdate,
    /// Stateful restart (`SEF_INIT_RESTART`): same refusal.
    Restart,
}

/// The production transport over any [`RtIpc`] wiring.
pub struct FsRt<I: RtIpc> {
    ipc: I,
    /// The virtual file system service: the only legitimate request source
    /// (`fsdriver_process`, fsdriver.c:22 — everyone else lands in `other`).
    vfs_endpoint: Endpoint,
    /// Sender of the request being served; reply and data-plane target.
    peer: Endpoint,
    /// Data-plane grant of the request being served.
    grant: i32,
    /// Signal decision hook (see [`SignalDecision`]).
    on_signal: SignalDecision,
    /// Birth callback (`sef_cb_init_fresh` and friends): runs when RS sends
    /// the init request, before the loop serves anything else.
    init: Box<dyn FnMut(InitKind) -> Result<(), i32>>,
    /// The request being served: node-shaped replies pick their payload
    /// layout by it (mount/new-node carry the device; create does not).
    last_request: Option<RequestNumber>,
}

impl<I: RtIpc> FsRt<I> {
    /// Wire the runtime around an IPC implementation.
    ///
    /// `on_signal` mirrors the C `sef_cb_signal_handler` decision (return
    /// `true` to terminate); `init` mirrors `sef_cb_init_fresh`. The init
    /// callback runs when RS's init request arrives — C runs it inside
    /// `sef_startup`, before the task loop ever sees a request.
    pub fn new(
        ipc: I,
        vfs_endpoint: Endpoint,
        on_signal: SignalDecision,
        init: Box<dyn FnMut(InitKind) -> Result<(), i32>>,
    ) -> Self {
        Self {
            ipc,
            vfs_endpoint,
            peer: Endpoint::NONE,
            grant: minix_types::GRANT_INVALID,
            on_signal,
            init,
            last_request: None,
        }
    }

    /// Birth face (`do_sef_init_request`, sef_init.c:193-215): run the
    /// callback by kind, report `RS_INIT` with the result to RS. Returns
    /// `false` when the server must not enter the loop (the C side panics
    /// on init failure; a single-threaded no_std server fails closed by
    /// stopping instead of panicking, which the restart supervisor treats
    /// the same way).
    fn run_birth(&mut self, msg: &Message) -> bool {
        // SAFETY: the birth request's active union arm is `m_rs_init`
        // (m_type == RS_INIT from RS; the guard checked both).
        let init = unsafe { msg.m_u.m_rs_init };
        let kind = match init.type_ {
            0 => InitKind::Fresh,
            1 => InitKind::LiveUpdate,
            _ => InitKind::Restart,
        };
        let result = match kind {
            InitKind::Fresh => (self.init)(InitKind::Fresh).map(|_| 0).unwrap_or_else(|e| e),
            other => {
                // Stateful starts are unmodeled: refuse honestly instead of
                // running a fresh init with stale state expectations.
                let _ = other;
                minix_types::ENOSYS
            }
        };
        let mut reply = Message {
            m_type: minix_sef::SEF_INIT_REQUEST_TYPE,
            ..Message::default()
        };
        // SAFETY: the birth report's active arm is `m_rs_init`
        // (process_init tail: `m.m_type = RS_INIT;
        // m.m_rs_init.result = result;` — sef_init.c).
        reply.m_u.m_rs_init.result = result;
        let _ = self.ipc.send(Endpoint::RS, &reply);
        result == 0
    }
}

impl<I: RtIpc> FsTransport for FsRt<I> {
    fn receive(&mut self) -> Incoming {
        loop {
            let mut msg = Message::default();
            // The signal hook has to travel *into* the receive call: C runs
            // the registered handler inside `sef_receive_status`, once per
            // signal number (`sef_signal.c:94-128`), so a decision can arrive
            // without any receipt value. Only `ipc` and `on_signal` are
            // borrowed here, leaving the rest of the runtime untouched.
            let FsRt { ipc, on_signal, .. } = self;
            // The escape hatch (PD-27, C `sef_cancel` — sef.c:291-297 +
            // sef.c:161-162): a non-Ignore decision cancels the pending
            // receive from inside the callback, so the library answers
            // `Err(EINTR)` and the decision maps to the leaving receipt right
            // below. C's `fsdriver_terminate()` is exactly this pair
            // (`running = FALSE` + `sef_cancel()`, fsdriver.c:68-74).
            let cancel = minix_sef::SefCancel::new();
            let mut decision = SignalAction::Ignore;
            let receipt = match ipc.receive(
                Endpoint::ANY,
                &mut msg,
                &mut |signo| {
                    // C's bit walk does not break when the handler asks to
                    // terminate, but at most one number in a single delivery can
                    // answer anything else: the kernel window 71..=74 holds no
                    // SIGTERM, and a manager-shaped message carries exactly one
                    // number (`sef_signal.c:117`). The first non-Ignore answer is
                    // therefore the whole decision — no queue needed.
                    if decision == SignalAction::Ignore {
                        decision = on_signal(signo);
                        if decision != SignalAction::Ignore {
                            cancel.cancel();
                        }
                    }
                },
                &cancel,
            ) {
                Ok(r) => r,
                // The escape hatch's answer: the decision (if any) was made
                // inside the callback — map it to the leaving receipt. Any
                // other receive failure has no recovery face in C (panic
                // inside `sef_receive`); the loop stops rather than spinning.
                Err(minix_types::EINTR) => match decision {
                    SignalAction::SyncThenTerminate => return Incoming::SyncThenCancelled,
                    _ => return Incoming::Cancelled,
                },
                Err(_) => return Incoming::Cancelled,
            };
            // A decision can no longer reach here: any non-Ignore answer
            // cancelled the receive above, and the library answered `EINTR`
            // before surfacing another frame — the same control flow C's
            // `fsdriver_terminate()` produces (the frame is swallowed, the
            // loop leaves without dispatching it).
            match receipt {
                Receipt::Signal => {
                    // The C dispatcher walked the kernel-signal bits and the
                    // handler's terminate decision corresponds to
                    // `fsdriver_terminate` cancelling the pending receive
                    // (fsdriver.c:67-73) — handled above, before this point.
                    continue;
                }
                Receipt::Call { status } => {
                    // Birth: RS's init request never reaches the task loop.
                    if msg.m_type == minix_sef::SEF_INIT_REQUEST_TYPE && msg.m_source == Endpoint::RS {
                        if !self.run_birth(&msg) {
                            return Incoming::Cancelled;
                        }
                        continue;
                    }
                    let envelope = Envelope {
                        source: msg.m_source.get(),
                        is_notification: minix_sef::is_ipc_notify(status),
                        message_type: msg.m_type,
                    };
                    if envelope.is_notification || msg.m_source != self.vfs_endpoint {
                        return Incoming::Other(envelope);
                    }
                    let (call, transaction) = TransactionId::decode(msg.m_type);
                    let index = call.wrapping_sub(minix_types::FS_BASE) as u32;
                    let Some(request) = RequestNumber::from_index(index) else {
                        return Incoming::Unserved(envelope);
                    };
                    self.peer = msg.m_source;
                    match wire::decode_body(request, &msg, &mut self.ipc, self.peer) {
                        Ok(body) => {
                            self.grant = wire::data_grant(request, &msg);
                            self.last_request = Some(request);
                            return Incoming::Request(envelope, body);
                        }
                        Err(e) => {
                            // Name fetch failed: the C adapters answer
                            // "invalid" through the same reply path
                            // (`fsdriver_getname` failures inside each
                            // adapter); here the classification stage
                            // answers directly. The driver's post-call hook
                            // does not run for this reply — it is a counting
                            // hook with no state, so the observable
                            // behavior is the reply code.
                            let reply = Message {
                                m_type: TransactionId::encode_reply(e.to_i32(), transaction),
                                ..Message::default()
                            };
                            let _ = self.ipc.send(self.peer, &reply);
                            continue;
                        }
                    }
                }
            }
        }
    }

    fn reply(&mut self, to: i32, reply: FsReply) {
        // An absolute-symlink restart carries the rewritten path through
        // the same grant the request came in on, before the reply lands
        // (the VFS restarts resolution from the rewritten path).
        if let Some(bytes) = wire::lookup_rewrite_bytes(&reply)
            && self.grant != minix_types::GRANT_INVALID
        {
            let _ = self.ipc.copy_to(self.peer, self.grant, 0, &bytes);
        }
        // The last served request decides the node-payload layout (mount/
        // new-node replies carry the device; create does not).
        let request = self.last_request.unwrap_or(RequestNumber::GetNode);
        let msg = wire::encode_reply(request, &reply);
        let _ = self.ipc.send(Endpoint(to), &msg);
    }

    fn copy_in(&mut self, offset: usize, out: &mut [u8]) -> Result<(), minix_types::Errno> {
        if self.grant == minix_types::GRANT_INVALID {
            return Err(minix_types::Errno::from_i32(minix_types::EIO));
        }
        self.ipc
            .copy_from(self.peer, self.grant, offset as u64, out)
            .map_err(|_| minix_types::Errno::from_i32(minix_types::EIO))
    }

    fn copy_out(&mut self, offset: usize, bytes: &[u8]) {
        if self.grant == minix_types::GRANT_INVALID {
            return;
        }
        let _ = self.ipc.copy_to(self.peer, self.grant, offset as u64, bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ScriptedRtIpc;
    use alloc::vec;
    use minix_fs::task::RequestBody;
    use minix_types as off;

    /// A runtime whose signal hook records the signal numbers it was called
    /// with and always answers "keep serving".
    fn signal_runtime(
        ipc: ScriptedRtIpc,
    ) -> (
        FsRt<ScriptedRtIpc>,
        alloc::rc::Rc<core::cell::RefCell<alloc::vec::Vec<i32>>>,
    ) {
        let seen = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::vec::Vec::new()));
        let seen2 = seen.clone();
        let rt = FsRt::new(
            ipc,
            Endpoint::VFS,
            Box::new(move |signo| {
                seen2.borrow_mut().push(signo);
                SignalAction::Ignore
            }),
            Box::new(|_| Ok(())),
        );
        (rt, seen)
    }

    /// A SYSTEM notification carrying `sigset` verbatim — C's 16-byte
    /// `sigset_t` in the notification arm.
    fn system_notify(sigset: minix_types::SigSetBits) -> Message {
        let mut m = Message::default();
        // SAFETY(test): the notification arm is the active one for SYSTEM
        // notifications.
        m.m_u.m_notify.sigset = sigset;
        m.m_source = Endpoint::from_generation_slot(0, 0); // SYSTEM
        m
    }

    #[test]
    fn test_notify_reaches_the_hook_as_a_signal_number() {
        // A bitmap with no kernel-signal bit is the production shape today:
        // the library hands the hook its wake-up value once, the hook answers
        // "keep serving", and the loop moves on to the next delivery (an
        // ordinary message from a non-VFS endpoint, returned as `Other`).
        // Bit 7 of the low word is signo 8 — outside the kernel window, so it
        // never reaches the hook as a number.
        let m = system_notify(minix_types::sigset_from_u64(0x80));
        let ordinary = Message::default();
        let ipc = ScriptedRtIpc::new(vec![
            (Receipt::Signal, m),
            (Receipt::Call { status: 0 }, ordinary),
        ]);
        let (mut rt, seen) = signal_runtime(ipc);
        let out = FsTransport::receive(&mut rt);
        assert!(
            matches!(out, Incoming::Other(_)),
            "a non-terminating signal keeps the loop going"
        );
        assert_eq!(
            seen.borrow().as_slice(),
            &[minix_sef::SEF_SIGNAL_REQUEST_TYPE],
            "the hook gets one wake-up number, not the raw bitmap"
        );
    }

    #[test]
    fn test_kernel_window_signal_reaches_the_hook_by_number() {
        // Word two, bit 6 is signo 71 (`SIGKMEM`): the shape a widened kernel
        // producer will deliver. The hook must see the number itself, in
        // ascending order, and one call per pending bit.
        let m = system_notify([0, 0, 1 << 6 | 1 << 9, 0]); // signo 71 and 74
        let ordinary = Message::default();
        let ipc = ScriptedRtIpc::new(vec![
            (Receipt::Signal, m),
            (Receipt::Call { status: 0 }, ordinary),
        ]);
        let (mut rt, seen) = signal_runtime(ipc);
        assert!(matches!(FsTransport::receive(&mut rt), Incoming::Other(_)));
        assert_eq!(
            seen.borrow().as_slice(),
            &[
                minix_types::SIGNAL_KERNEL_MEMORY,
                minix_types::SIGNAL_KERNEL_PENDING
            ],
            "every pending kernel signal reaches the hook by number, ascending"
        );
    }

    #[test]
    fn test_terminate_decision_cancels_receive() {
        let m = system_notify(minix_types::sigset_from_u64(0x100)); // signo 9
        // The scripted hook answers "terminate" for whatever number arrives,
        // so the receive loop returns `Cancelled` instead of waiting for the
        // next delivery (the C `fsdriver_terminate` face, fsdriver.c:67-73).
        let ipc = ScriptedRtIpc::new(vec![(Receipt::Signal, m)]);
        let mut rt = FsRt::new(
            ipc,
            Endpoint::VFS,
            Box::new(|_| SignalAction::Terminate),
            Box::new(|_| Ok(())),
        );
        assert!(matches!(
            FsTransport::receive(&mut rt),
            Incoming::Cancelled
        ));
    }

    #[test]
    fn test_sync_decision_cancels_after_the_flush_request() {
        // The `mfs` answer (`mfs/main.c:75-77`): sync first, then leave. The
        // runtime cannot flush a driver it does not own, so it reports the
        // order to the task loop, which holds the mounted server.
        let m = system_notify(minix_types::sigset_from_u64(0x100));
        let ipc = ScriptedRtIpc::new(vec![(Receipt::Signal, m)]);
        let mut rt = FsRt::new(
            ipc,
            Endpoint::VFS,
            Box::new(|_| SignalAction::SyncThenTerminate),
            Box::new(|_| Ok(())),
        );
        assert!(matches!(
            FsTransport::receive(&mut rt),
            Incoming::SyncThenCancelled
        ));
    }

    #[test]
    fn test_birth_fresh_runs_callback_and_reports_ok() {
        // RS sends the init request: type 0 (fresh) in the `m_rs_init` arm.
        let mut m = Message::default();
        // SAFETY(test): union-field *assignment* is safe; only reads are
        // unsafe (and the runtime's read is guarded by type+source).
        m.m_u.m_rs_init = minix_types::ipc::MessRsInit {
                result: 0,
                type_: 0,
                rproctab_gid: 0,
                old_endpoint: 0,
                restarts: 0,
                flags: 0,
                buff_addr: 0,
                buff_len: 0,
            prepare_state: 0,
            _padding: [0; 12],
        };
        m.m_type = minix_sef::SEF_INIT_REQUEST_TYPE;
        m.m_source = Endpoint::RS;
        let ran_ptr = alloc::rc::Rc::new(core::cell::Cell::new(false));
        {
            let ran2 = ran_ptr.clone();
            // Second delivery: an ordinary non-VFS message so the loop has
            // somewhere to land after the birth request is swallowed.
            let ipc = ScriptedRtIpc::new(vec![
                (Receipt::Call { status: 0 }, m),
                (Receipt::Call { status: 0 }, Message::default()),
            ]);
            let mut rt = FsRt::new(
                ipc,
                Endpoint::VFS,
                Box::new(|_| SignalAction::Ignore),
                Box::new(move |_| {
                    ran2.set(true);
                    Ok(())
                }),
            );
            let out = FsTransport::receive(&mut rt);
            assert!(matches!(out, Incoming::Other(_)), "birth is swallowed; the loop continues to the next delivery");
        }
        assert!(ran_ptr.get(), "fresh birth ran the init callback");
    }

    #[test]
    fn test_birth_refuses_stateful_kinds() {
        // Live update (type 1) is unmodeled: the runtime reports ENOSYS to
        // RS and stops the loop instead of faking a fresh start.
        let mut m = Message::default();
        m.m_u.m_rs_init.type_ = 1; // union-field assignment: safe
        m.m_type = minix_sef::SEF_INIT_REQUEST_TYPE;
        m.m_source = Endpoint::RS;
        let ipc = ScriptedRtIpc::new(vec![(Receipt::Call { status: 0 }, m)]);
        let mut rt = FsRt::new(
            ipc,
            Endpoint::VFS,
            Box::new(|_| SignalAction::Ignore),
            Box::new(|_| Ok(())),
        );
        assert!(matches!(
            FsTransport::receive(&mut rt),
            Incoming::Cancelled
        ));
    }

    #[test]
    fn test_request_decoded_and_replied() {
        // One lookup from VFS: decode path from the grant, reply status,
        // and see the reply on the wire with the transaction echoed.
        let mut m = Message::default();
        let trans = 7u16;
        // Wire shape: the request rides the high half, the transaction id
        // the low half (`trns_add_id`; minix-fs TransactionId::decode).
        m.m_type = (minix_types::REQ_LOOKUP << 16) | (trans as i32);
        m.m_source = Endpoint::VFS;
        // SAFETY(test): building the lookup payload per the shared table.
        unsafe {
            let raw = &mut m.m_u.raw;
            raw[off::lookup_req_off::DIR_INO..off::lookup_req_off::DIR_INO + 8]
                .copy_from_slice(&1u64.to_le_bytes());
            raw[off::lookup_req_off::ROOT_INO..off::lookup_req_off::ROOT_INO + 8]
                .copy_from_slice(&1u64.to_le_bytes());
            // "/x" plus the terminator — the length includes the NUL.
            raw[off::lookup_req_off::PATH_LEN..off::lookup_req_off::PATH_LEN + 8]
                .copy_from_slice(&3u64.to_le_bytes());
        }
        let mut ipc = ScriptedRtIpc::new(vec![(Receipt::Call { status: 0 }, m)]);
        ipc.grant_data = b"/x\0".to_vec();
        let mut rt = FsRt::new(
            ipc,
            Endpoint::VFS,
            Box::new(|_| SignalAction::Ignore),
            Box::new(|_| Ok(())),
        );
        match FsTransport::receive(&mut rt) {
            Incoming::Request(env, body) => {
                assert_eq!(env.source, Endpoint::VFS.get());
                match body {
                    RequestBody::Lookup { path, .. } => assert_eq!(path, "/x"),
                    other => panic!("wrong body: {other:?}"),
                }
                let reply = FsReply::status(minix_types::ENOENT, TransactionId(trans));
                FsTransport::reply(&mut rt, Endpoint::VFS.get(), reply);
            }
            other => panic!("expected a request, got {other:?}"),
        }
    }
}
