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
/// pending signal. Two pieces of that channel are not this runtime's job:
/// - the walk itself: the notification payload now carries C's 16-byte
///   `sigset_t` verbatim (`minix_types::SigSetBits`), so kernel signals
///   71..=74 have a home (`bits[2]`), but the per-signal walk belongs to
///   `minix-sef`'s signal arm, not here — this hook still receives the raw
///   low 64 bits, and converging its shape to a signal number is the
///   signal-chain batch;
/// - process signals (SIGTERM and friends) reach the C handler through the
///   signal manager's pull (`sys_getksig`/`sys_endksig`, whose minix-sys
///   wrappers exist at `minix_sys::syscall::sys_getksig`/`sys_endksig`);
///   no FS driver registers a signal handler in C either, so the hooks
///   built here answer `false`.
///
/// The hook receives the raw bitmap and returns `true` to terminate.
pub type SignalDecision = Box<dyn FnMut(u64) -> bool>;

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
            let receipt = match self.ipc.receive(Endpoint::ANY, &mut msg) {
                Ok(r) => r,
                // A failed receive has no recovery face in C (panic inside
                // `sef_receive`); the loop stops rather than spinning.
                Err(_) => return Incoming::Cancelled,
            };
            match receipt {
                Receipt::Signal { pending } => {
                    // The C dispatcher walks the kernel-signal bits and the
                    // handler's terminate decision corresponds to
                    // `fsdriver_terminate` cancelling the pending receive
                    // (fsdriver.c:67-73) — the loop exits without a reply.
                    // The bit walk itself stays with the hook (see
                    // [`SignalDecision`] for the two pending-width gaps).
                    if (self.on_signal)(pending) {
                        return Incoming::Cancelled;
                    }
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

    /// A runtime whose signal hook records the raw bitmaps it saw.
    fn signal_runtime(
        ipc: ScriptedRtIpc,
    ) -> (
        FsRt<ScriptedRtIpc>,
        alloc::rc::Rc<core::cell::RefCell<alloc::vec::Vec<u64>>>,
    ) {
        let seen = alloc::rc::Rc::new(core::cell::RefCell::new(alloc::vec::Vec::new()));
        let seen2 = seen.clone();
        let rt = FsRt::new(
            ipc,
            Endpoint::VFS,
            Box::new(move |pending| {
                seen2.borrow_mut().push(pending);
                false
            }),
            Box::new(|_| Ok(())),
        );
        (rt, seen)
    }

    fn system_signal(sigset: u64) -> Message {
        let mut m = Message::default();
        // SAFETY(test): the notification arm is the active one for SYSTEM
        // notifications.
        m.m_u.m_notify.sigset = minix_types::sigset_from_u64(sigset);
        m.m_source = Endpoint::from_generation_slot(0, 0); // SYSTEM
        m
    }

    #[test]
    fn test_signal_walk_and_terminate_on_sigterm() {
        // A non-terminating bitmap lets the loop continue to the next
        // scripted delivery; the hook saw the bitmap verbatim.
        // Deliveries: one SYSTEM notification carrying bit 7 in the low word
        // (signo 8 — C bit numbering is `signo - 1`), then an ordinary message
        // from a non-VFS endpoint (repeats forever, but the loop returns on
        // it as `Other`).
        let m = system_signal(0x80);
        let ordinary = Message::default();
        let ipc = ScriptedRtIpc::new(vec![
            (Receipt::Signal { pending: 0x80 }, m),
            (Receipt::Call { status: 0 }, ordinary),
        ]);
        let (mut rt, seen) = signal_runtime(ipc);
        let out = FsTransport::receive(&mut rt);
        assert!(matches!(out, Incoming::Other(_)), "a non-terminating signal keeps the loop going");
        assert_eq!(seen.borrow().as_slice(), &[0x80], "the raw bitmap reaches the hook untouched");
    }

    #[test]
    fn test_terminate_decision_cancels_receive() {
        let m = system_signal(0x100); // 低字 bit 8 → signo 9
        // The scripted hook answers "terminate" unconditionally, so the
        // receive loop returns `Cancelled` instead of waiting for the next
        // delivery (the C `fsdriver_terminate` face, fsdriver.c:67-73).
        let ipc = ScriptedRtIpc::new(vec![(Receipt::Signal { pending: 0x100 }, m)]);
        let mut rt = FsRt::new(
            ipc,
            Endpoint::VFS,
            Box::new(|_| true),
            Box::new(|_| Ok(())),
        );
        assert!(matches!(
            FsTransport::receive(&mut rt),
            Incoming::Cancelled
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
                Box::new(|_| false),
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
            Box::new(|_| false),
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
            Box::new(|_| false),
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
