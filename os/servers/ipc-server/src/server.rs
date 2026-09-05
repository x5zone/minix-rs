//! IPC server event loop.
//!
//! Mirrors C `main` (main.c:216-284): wait for a message, route it through
//! the branch ladder, optionally reply, then run the end-of-cycle hook.
//!
//! The loop is single-threaded — one message at a time, no locks. Shared
//! state (transport handle, counters) sits behind `RefCell`, matching the
//! VM server's single-threaded model (not kernel SMP: no `Arc`/`Mutex`).
//! Document `01-ipc-init-main.md` §3 (decisions D5/D6/D7) and §4.
//!
//! Handlers for the seven calls (documents 05-08), process events (09),
//! MIB requests (03), and the reference-count hook (08) are not landed
//! yet; they arrive through [`CallHandler`]. The loop itself is complete
//! and testable today via the stub and recording implementations below.

use alloc::boxed::Box;
use core::cell::{Cell, RefCell};

use minix_types::{ENOSYS, Endpoint, IpcCall, Message, ProcEventIn};

use crate::dispatch::{
    Incoming, classify, proc_event_reply_type, should_reply, unknown_call_result,
};

// ============================================================================
// Transport
// ============================================================================

/// Kernel receive status for one message.
///
/// C: `ipc_status` filled by `sef_receive_status` (main.c:228). The only
/// bit the main loop reads is "was this a notification"
/// (`is_ipc_notify`, com.h:92). The production transport fills this from
/// the kernel status word; the test transport sets it directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IpcStatus {
    /// True when the arrival is an asynchronous notification, not a request.
    pub notify: bool,
}

impl IpcStatus {
    /// Request arrival (not a notification).
    pub const fn request() -> Self {
        Self { notify: false }
    }

    /// Notification arrival.
    pub const fn notification() -> Self {
        Self { notify: true }
    }

    /// C: `is_ipc_notify(ipc_status)` — com.h:92.
    pub const fn is_notify(self) -> bool {
        self.notify
    }
}

/// Opaque transport failure.
///
/// The loop cannot diagnose transport faults (a broken channel looks the
/// same regardless of cause), so the error carries no details — it only
/// marks fallibility for the type system. Occurrences are counted, never
/// matched on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportError;

/// Message transport between the server and the kernel.
///
/// Production: `sef_receive_status` / `ipc_sendnb`. Tests: an in-memory
/// queue (see `TestTransport` in the test module). Failures surface as
/// [`TransportError`] and are only counted (a broken transport is not
/// diagnosable from inside the loop; see the consecutive-failure bound
/// in `run`).
pub trait IpcTransport {
    /// Wait for the next message (C: `sef_receive_status(ANY, …)`).
    fn receive(&mut self) -> Result<(Message, IpcStatus), TransportError>;
    /// Send a reply (C: `ipc_sendnb`).
    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), TransportError>;
}

// ============================================================================
// Deferred work (documents 03/05-09)
// ============================================================================

/// Business logic behind the dispatch branches.
///
/// Each method corresponds to work owned by a later document; the loop calls
/// them at exactly the point where C calls the matching function. Two
/// implementations exist: [`StubHandler`] (production placeholder until
/// 05-09 land) and the recording test double in the test module.
pub trait CallHandler {
    /// Handle one of the seven calls. Returns the result code, or SUSPEND
    /// when the caller stays blocked (document 06).
    /// C: `call_vec[call_index](&m)` — main.c:259. Documents 05-08.
    fn handle_call(&mut self, call: IpcCall, msg: &Message) -> i32;
    /// Handle a process event. Returns the reply type to echo to PM.
    /// C: `got_proc_event` — main.c:191-210. Document 09.
    fn handle_proc_event(&mut self, event: ProcEventIn) -> i32;
    /// Handle a MIB request. C: `rmib_process` — main.c:250. Document 03.
    fn handle_mib(&mut self, msg: &Message);
    /// End-of-cycle hook: refresh shared-memory reference counts and destroy
    /// due segments. C: `update_refcount_and_destroy` — main.c:279.
    /// Document 08.
    fn on_cycle_end(&mut self);
}

/// Production placeholder: every call is unimplemented, every event is
/// accepted silently, the cycle hook does nothing.
///
/// Each arm names the document that will replace it, so the placeholder
/// reads as a work list, not a finished implementation.
pub struct StubHandler;

impl CallHandler for StubHandler {
    fn handle_call(&mut self, _call: IpcCall, _msg: &Message) -> i32 {
        // Documents 05-08 land the seven handlers; until then ENOSYS
        // (the same code C returns for an empty table slot — main.c:261).
        ENOSYS
    }

    fn handle_proc_event(&mut self, _event: ProcEventIn) -> i32 {
        // Document 09 lands subscription-aware handling; the acknowledgement
        // itself (PROC_EVENT_REPLY echo) is main.c's own code — main.c:207.
        proc_event_reply_type()
    }

    fn handle_mib(&mut self, _msg: &Message) {
        // Document 03 lands subtree dispatch; rmib_process replies from
        // inside the library call, so there is nothing to return here.
    }

    fn on_cycle_end(&mut self) {
        // Document 08 lands update_refcount_and_destroy.
    }
}

// ============================================================================
// Server
// ============================================================================

/// Outcome of one [`IpcServer::run_once`] iteration.
///
/// Public because `run_once` is public (integration tests match on it to
/// drive multi-round scenarios without spawning the infinite loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStep {
    /// A message was received and routed (dispatched, skipped, or replied).
    Handled,
    /// The transport reported a receive error; counted, loop continues.
    ReceiveFailed,
}

/// Consecutive receive failures before `run` treats the transport as
/// permanently broken and stops (C `sef_receive_status` blocks, so errors
/// cannot be transient "no message" conditions — same bound as the VM
/// server's main loop).
const MAX_CONSECUTIVE_RECV_FAILURES: u32 = 32;

/// The IPC server event loop.
///
/// Type parameters inject the transport and the business logic so tests can
/// drive the loop without a kernel (VM-server pattern: `run_once` owns one
/// iteration, `run` owns the infinite loop plus the failure bound).
pub struct IpcServer<T: IpcTransport, H: CallHandler> {
    transport: RefCell<Box<T>>,
    handler: RefCell<H>,
    /// Set by `init` (C: `sef_local_startup` + `sef_startup` — main.c:224).
    /// `run` refuses to start before it, catching wiring mistakes in tests.
    /// `Cell`: the whole server API takes `&self` so `run` can loop without
    /// reborrowing; sound on the single-threaded event loop.
    initialized: Cell<bool>,
    /// Messages dropped at the loop boundary (receive errors + send errors
    /// are only logged in C; here both are counted so tests can observe them).
    /// `Cell`: single-threaded event loop, never shared across threads.
    dropped_messages: Cell<u64>,
    /// Completed cycle-hook runs (== loop iterations that reached main.c:279).
    completed_cycles: Cell<u64>,
}

impl<T: IpcTransport, H: CallHandler> IpcServer<T, H> {
    /// Build a server around a transport and a handler. Not ready until
    /// [`Self::init`] (mirrors C: constructing state, then `sef_startup`).
    pub fn new(transport: T, handler: H) -> Self {
        Self {
            transport: RefCell::new(Box::new(transport)),
            handler: RefCell::new(handler),
            initialized: Cell::new(false),
            dropped_messages: Cell::new(0),
            completed_cycles: Cell::new(0),
        }
    }

    /// Startup registration point.
    ///
    /// C: `env_setargs` + `sef_local_startup` (main.c:223-224): register the
    /// fresh-start, restart, and signal callbacks, then `sef_startup`. The
    /// real callback registration needs the SEF library (lands with the
    /// kernel transport); this method marks the server ready so the loop
    /// invariant ("never run before startup") is enforceable today.
    pub fn init(&self) {
        self.initialized.set(true);
    }

    /// Whether `init` has run.
    pub fn is_ready(&self) -> bool {
        self.initialized.get()
    }

    /// Messages dropped at the loop boundary so far.
    pub fn dropped_messages(&self) -> u64 {
        self.dropped_messages.get()
    }

    /// Loop iterations that reached the end-of-cycle hook so far.
    pub fn completed_cycles(&self) -> u64 {
        self.completed_cycles.get()
    }

    /// Run the server forever. Never returns.
    ///
    /// C: `for (;;)` — main.c:227-280.
    ///
    /// [ARCH: IPC-01-01] C panics on the first receive failure (main.c:229);
    /// here failures are counted and the loop continues, stopping only after
    /// sustained failure — a user-space server should survive one transport
    /// hiccup. See document 01 §3 D7.
    pub fn run(&self) -> ! {
        assert!(
            self.initialized.get(),
            "IpcServer::run() called before init()"
        );
        let mut consecutive_failures: u32 = 0;
        loop {
            match self.run_once() {
                RunStep::Handled => consecutive_failures = 0,
                RunStep::ReceiveFailed => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    if consecutive_failures >= MAX_CONSECUTIVE_RECV_FAILURES {
                        panic!(
                            "IPC transport permanently broken: {} consecutive receive failures",
                            consecutive_failures
                        );
                    }
                }
            }
        }
    }

    /// Process exactly one message (or one receive failure).
    ///
    /// Mirrors one iteration of C main.c:227-280. `pub` (not private like the
    /// VM server's) so integration tests in `tests/` can drive whole
    /// scenarios one round at a time without spawning the infinite loop.
    pub fn run_once(&self) -> RunStep {
        // C: sef_receive_status(ANY, &m, &ipc_status) — main.c:228.
        let (msg, status) = match self.transport.borrow_mut().receive() {
            Ok(v) => v,
            Err(TransportError) => {
                self.note_dropped();
                return RunStep::ReceiveFailed;
            }
        };

        // C: is_ipc_notify → log + continue — main.c:234-238. The `continue`
        // skips the end-of-cycle hook (:279): notifications carry no state
        // change that the hook would need to sweep. (Logging lands with the
        // syslog sink; until then the branch is observable only by the
        // absence of a reply.)
        if status.is_notify() {
            return RunStep::Handled;
        }

        match classify(false, msg.m_source, msg.m_type) {
            Incoming::Notify => {
                // Unreachable: notifications return above via the status word.
                // Kept for exhaustiveness — classify also takes the bool form
                // for pure-function testing (dispatch.rs).
                self.end_cycle();
            }
            Incoming::ProcEvent => {
                // C: got_proc_event + continue — main.c:241-245. The echo
                // reply is main.c's own code (:207): rebuild it here rather
                // than inside the 09 handler. The `continue` skips the
                // end-of-cycle hook (:279).
                let event = ProcEventIn::decode_message(&msg);
                let reply_type = self.handler.borrow_mut().handle_proc_event(event);
                let mut reply = msg;
                reply.m_type = reply_type;
                if self
                    .transport
                    .borrow_mut()
                    .send(msg.m_source, &reply)
                    .is_err()
                {
                    // C: printf + continue — main.c:208-209. Send failure
                    // means PM is gone; counted, loop continues.
                    self.note_dropped();
                }
            }
            Incoming::Mib => {
                // C: rmib_process + continue — main.c:248-252. The library
                // replies from inside the call; nothing to send here. The
                // `continue` skips the end-of-cycle hook (:279).
                self.handler.borrow_mut().handle_mib(&msg);
            }
            Incoming::Dispatch(call) => {
                // C: r = call_vec[call_index](&m) — main.c:259.
                let result = self.handler.borrow_mut().handle_call(call, &msg);
                // C: if (r != SUSPEND) { m.m_type = r; ipc_sendnb(...); }
                // — main.c:264-276. Other fields stay as the handler left
                // them (the reply reuses the request buffer — main.c:269-272).
                if should_reply(result) {
                    let mut reply = msg;
                    reply.m_type = result;
                    if self
                        .transport
                        .borrow_mut()
                        .send(msg.m_source, &reply)
                        .is_err()
                    {
                        // C: printf("IPC: send error") + continue — :274-275.
                        self.note_dropped();
                    }
                }
                // C: update_refcount_and_destroy() — main.c:279. Reached only
                // on the dispatch path (known and unknown calls): the three
                // dedicated branches above `continue` past it.
                self.end_cycle();
            }
            Incoming::Unknown => {
                // C: r = ENOSYS, then the normal reply path — main.c:260-276.
                let mut reply = msg;
                reply.m_type = unknown_call_result();
                if self
                    .transport
                    .borrow_mut()
                    .send(msg.m_source, &reply)
                    .is_err()
                {
                    self.note_dropped();
                }
                self.end_cycle();
            }
        }
        RunStep::Handled
    }

    /// Count one dropped message (saturating: the counter must never wrap).
    /// `Cell` gives single-threaded interior mutability — sound because the
    /// event loop never shares the server across threads (same model as the
    /// VM server's single-threaded counters).
    fn note_dropped(&self) {
        self.dropped_messages
            .set(self.dropped_messages.get().saturating_add(1));
    }

    /// End-of-cycle hook point (C: main.c:279 — dispatch path only; the
    /// three dedicated branches `continue` past it).
    fn end_cycle(&self) {
        self.handler.borrow_mut().on_cycle_end();
        self.completed_cycles
            .set(self.completed_cycles.get().saturating_add(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use minix_types::{IpcSemgetIn, PROC_EVENT};

    /// In-memory transport: preloaded arrivals, recorded replies.
    struct TestTransport {
        arrivals: Vec<(Message, IpcStatus)>,
        replies: Vec<(Endpoint, Message)>,
        fail_next_receives: usize,
        fail_sends: bool,
    }

    impl TestTransport {
        fn new() -> Self {
            Self {
                arrivals: Vec::new(),
                replies: Vec::new(),
                fail_next_receives: 0,
                fail_sends: false,
            }
        }

        fn push(&mut self, msg: Message, status: IpcStatus) {
            self.arrivals.push((msg, status));
        }
    }

    impl IpcTransport for TestTransport {
        fn receive(&mut self) -> Result<(Message, IpcStatus), TransportError> {
            if self.fail_next_receives > 0 {
                self.fail_next_receives -= 1;
                return Err(TransportError);
            }
            self.arrivals.pop().ok_or(TransportError)
        }

        fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), TransportError> {
            if self.fail_sends {
                return Err(TransportError);
            }
            self.replies.push((dest, *msg));
            Ok(())
        }
    }

    /// Recording handler: canned call results, counted branch visits.
    struct RecordingHandler {
        call_result: i32,
        proc_events: u32,
        mibs: u32,
        cycles: u32,
        last_call: Option<IpcCall>,
    }

    impl RecordingHandler {
        fn new(call_result: i32) -> Self {
            Self {
                call_result,
                proc_events: 0,
                mibs: 0,
                cycles: 0,
                last_call: None,
            }
        }
    }

    impl CallHandler for RecordingHandler {
        fn handle_call(&mut self, call: IpcCall, _msg: &Message) -> i32 {
            self.last_call = Some(call);
            self.call_result
        }

        fn handle_proc_event(&mut self, _event: ProcEventIn) -> i32 {
            self.proc_events += 1;
            proc_event_reply_type()
        }

        fn handle_mib(&mut self, _msg: &Message) {
            self.mibs += 1;
        }

        fn on_cycle_end(&mut self) {
            self.cycles += 1;
        }
    }

    fn request_msg(source: Endpoint, call_type: i32) -> Message {
        Message {
            m_source: source,
            m_type: call_type,
            ..Message::default()
        }
    }

    #[test]
    fn run_once_notify_is_skipped() {
        // C: main.c:234-238 — a notification sends nothing and skips the
        // cycle hook (the `continue` bypasses :279).
        let mut transport = TestTransport::new();
        transport.push(
            request_msg(Endpoint(42), minix_types::IPC_SEMGET),
            IpcStatus::notification(),
        );
        let server = IpcServer::new(transport, StubHandler);
        server.init();
        assert_eq!(server.run_once(), RunStep::Handled);
        assert_eq!(server.completed_cycles(), 0);
        assert_eq!(server.dropped_messages(), 0);
    }

    #[test]
    fn run_once_unknown_replies_enosys() {
        // C: main.c:260-276 — unknown numbers reply ENOSYS; the cycle hook
        // still runs.
        let mut transport = TestTransport::new();
        transport.push(request_msg(Endpoint(42), 0xD00), IpcStatus::request());
        let server = IpcServer::new(transport, StubHandler);
        server.init();
        assert_eq!(server.run_once(), RunStep::Handled);
        assert_eq!(server.completed_cycles(), 1);
        let transport = server.transport.borrow();
        assert_eq!(transport.replies.len(), 1);
        assert_eq!(transport.replies[0].1.m_type, ENOSYS);
    }

    #[test]
    fn run_once_receive_failure_counts() {
        // [ARCH: IPC-01-01] — C panics (main.c:229); here the failure is
        // counted and the loop survives.
        let mut transport = TestTransport::new();
        transport.fail_next_receives = 2;
        let server = IpcServer::new(transport, StubHandler);
        server.init();
        assert_eq!(server.run_once(), RunStep::ReceiveFailed);
        assert_eq!(server.run_once(), RunStep::ReceiveFailed);
        assert_eq!(server.dropped_messages(), 2);
        assert_eq!(server.completed_cycles(), 0);
    }

    #[test]
    fn run_once_dispatch_replies_result() {
        // C: main.c:259-276 — the handler result becomes the reply type.
        let mut transport = TestTransport::new();
        transport.push(
            request_msg(Endpoint(42), minix_types::IPC_SEMGET),
            IpcStatus::request(),
        );
        let server = IpcServer::new(transport, RecordingHandler::new(0));
        server.init();
        assert_eq!(server.run_once(), RunStep::Handled);
        assert_eq!(server.handler.borrow().last_call, Some(IpcCall::Semget));
        let transport = server.transport.borrow();
        assert_eq!(transport.replies.len(), 1);
        assert_eq!(transport.replies[0].0, Endpoint(42));
        assert_eq!(transport.replies[0].1.m_type, 0);
        assert_eq!(server.completed_cycles(), 1);
    }

    #[test]
    fn run_once_suspend_sends_nothing() {
        // C: main.c:264 — SUSPEND suppresses the reply (document 01 §1.4).
        let mut transport = TestTransport::new();
        transport.push(
            request_msg(Endpoint(42), minix_types::IPC_SEMOP),
            IpcStatus::request(),
        );
        let server = IpcServer::new(transport, RecordingHandler::new(minix_types::SUSPEND));
        server.init();
        assert_eq!(server.run_once(), RunStep::Handled);
        assert!(server.transport.borrow().replies.is_empty());
        // The cycle still ends: the refcount hook runs on the SUSPEND path too.
        assert_eq!(server.completed_cycles(), 1);
    }

    #[test]
    fn run_once_proc_event_acknowledges() {
        // C: main.c:241-245 + :207 — process events are acknowledged with
        // PROC_EVENT_REPLY and the loop continues past the cycle hook
        // (no handler dispatch, no refcount sweep).
        let mut msg = Message {
            m_source: Endpoint::PM,
            m_type: PROC_EVENT,
            ..Message::default()
        };
        // Writing a union field is safe (only reading one needs `unsafe`):
        // every byte pattern written is a valid value of the field's type.
        msg.m_u.m_pm_lsys_proc_event = minix_types::MessPmLsysProcEvent {
            endpt: 33,
            event: ProcEventIn::EXIT,
            ..Default::default()
        };
        let mut transport = TestTransport::new();
        transport.push(msg, IpcStatus::request());
        let server = IpcServer::new(transport, RecordingHandler::new(0));
        server.init();
        assert_eq!(server.run_once(), RunStep::Handled);
        assert_eq!(server.handler.borrow().proc_events, 1);
        assert_eq!(server.handler.borrow().last_call, None);
        let transport = server.transport.borrow();
        assert_eq!(transport.replies.len(), 1);
        assert_eq!(transport.replies[0].1.m_type, proc_event_reply_type());
        assert_eq!(server.completed_cycles(), 0);
    }

    #[test]
    fn run_once_mib_takes_no_reply_path() {
        // C: main.c:248-252 — MIB requests are consumed by rmib_process,
        // which replies from inside; the loop sends nothing itself and
        // continues past the cycle hook.
        let mut transport = TestTransport::new();
        transport.push(
            request_msg(Endpoint::MIB, minix_types::IPC_SEMGET),
            IpcStatus::request(),
        );
        let server = IpcServer::new(transport, RecordingHandler::new(0));
        server.init();
        assert_eq!(server.run_once(), RunStep::Handled);
        assert_eq!(server.handler.borrow().mibs, 1);
        assert!(server.transport.borrow().replies.is_empty());
        assert_eq!(server.completed_cycles(), 0);
    }

    #[test]
    fn run_once_send_failure_is_counted() {
        // C: main.c:274-275 — send failure only logs; here it is counted.
        let mut transport = TestTransport::new();
        transport.fail_sends = true;
        transport.push(
            request_msg(Endpoint(42), minix_types::IPC_SEMGET),
            IpcStatus::request(),
        );
        let server = IpcServer::new(transport, RecordingHandler::new(0));
        server.init();
        assert_eq!(server.run_once(), RunStep::Handled);
        assert_eq!(server.dropped_messages(), 1);
        assert_eq!(server.completed_cycles(), 1);
    }

    #[test]
    fn run_requires_init() {
        // The loop invariant is enforced: running before startup panics
        // instead of serving messages with unregistered callbacks.
        let server = IpcServer::new(TestTransport::new(), StubHandler);
        assert!(!server.is_ready());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            server.run();
        }));
        assert!(result.is_err());
    }

    #[test]
    fn semget_request_decodes_through_loop() {
        // Wire-level spot check: a real semget payload survives the trip
        // from message bytes to the handler's view (guards the transport
        // overlay wiring in minix-types).
        let mut msg = Message {
            m_source: Endpoint(42),
            m_type: minix_types::IPC_SEMGET,
            ..Message::default()
        };
        // Writing a union field is safe (only reading one needs `unsafe`).
        msg.m_u.m_lc_ipc_semget = minix_types::MessLcIpcSemget {
            key: 0x1234,
            nr: 3,
            flag: 0o1000,
            ..Default::default()
        };
        let req = IpcSemgetIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint(42));
        assert_eq!((req.key, req.count, req.flag), (0x1234, 3, 0o1000));
        // And the loop routes exactly this message to the Semget handler.
        let mut transport = TestTransport::new();
        transport.push(msg, IpcStatus::request());
        let server = IpcServer::new(transport, RecordingHandler::new(0));
        server.init();
        assert_eq!(server.run_once(), RunStep::Handled);
        assert_eq!(server.handler.borrow().last_call, Some(IpcCall::Semget));
    }
}
