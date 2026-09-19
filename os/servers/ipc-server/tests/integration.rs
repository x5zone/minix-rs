//! Whole-round scenarios through the crate's public surface.
//!
//! `IpcServer::run_once` is public precisely so tests can drive whole
//! message rounds one at a time, without spawning the infinite loop
//! (document 01, decision D5). The transport is scripted (preloaded
//! arrivals, recorded replies); the handler answers one canned code —
//! what is under test here is the loop's routing and reply verdicts as
//! the outside world sees them. The seven calls' business logic lands
//! with the service layer (IPC-P1-1) and grows its own scenarios then.
//!
//! These tests live in `tests/` (not the in-module suites) because they
//! use only the public API: nothing here may reach into crate internals.

use minix_ipc_server::{
    CallHandler, EventLoopTransport, IpcServer, IpcStatus, RunStep, TransportError,
    proc_event_reply_type,
};
use minix_types::{Endpoint, IpcCall, Message, PROC_EVENT, SUSPEND};

/// Which send verb carried an outbound message (IPC-P2-2: the two verbs
/// map to different kernel calls, so the public surface must keep them
/// apart).
struct Outbound {
    dest: Endpoint,
    msg: Message,
    verb: &'static str,
}

/// Scripted transport: preloaded arrivals, recorded replies with the verb
/// that carried them.
struct ScriptedTransport {
    arrivals: Vec<(Message, IpcStatus)>,
    outbound: Vec<Outbound>,
}

impl ScriptedTransport {
    fn new() -> Self {
        Self {
            arrivals: Vec::new(),
            outbound: Vec::new(),
        }
    }

    fn push(&mut self, msg: Message, status: IpcStatus) {
        self.arrivals.push((msg, status));
    }
}

impl EventLoopTransport for ScriptedTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), TransportError> {
        self.arrivals.pop().ok_or(TransportError)
    }

    fn send_reply(&mut self, dest: Endpoint, msg: &Message) -> Result<(), TransportError> {
        self.outbound.push(Outbound {
            dest,
            msg: *msg,
            verb: "send_reply",
        });
        Ok(())
    }

    fn send_async(&mut self, dest: Endpoint, msg: &Message) -> Result<(), TransportError> {
        self.outbound.push(Outbound {
            dest,
            msg: *msg,
            verb: "send_async",
        });
        Ok(())
    }
}

/// Handler answering one canned code and counting its visits.
struct CannedHandler {
    code: i32,
    calls: u32,
    proc_events: u32,
}

impl CannedHandler {
    fn answering(code: i32) -> Self {
        Self {
            code,
            calls: 0,
            proc_events: 0,
        }
    }
}

impl CallHandler for CannedHandler {
    fn handle_call(&mut self, _call: IpcCall, _msg: &mut Message) -> i32 {
        self.calls += 1;
        self.code
    }

    fn handle_proc_event(&mut self, _event: minix_types::ProcEventIn) -> i32 {
        self.proc_events += 1;
        proc_event_reply_type()
    }

    fn handle_mib(&mut self, _msg: &mut Message, _ipc_status: minix_ipc_server::IpcStatus) {}

    fn on_cycle_end(&mut self) {}
}

/// One semget request as a caller would send it.
fn semget_request(source: i32) -> Message {
    let mut msg = Message {
        m_source: Endpoint(source),
        m_type: minix_types::IPC_SEMGET,
        ..Message::default()
    };
    msg.m_u.m_lc_ipc_semget = minix_types::MessLcIpcSemget {
        key: 0x1234,
        nr: 3,
        flag: 0o1000 | 0o600,
        ..Default::default()
    };
    msg
}

#[test]
fn semget_round_trip_through_public_api() {
    // A real request survives decode → dispatch → reply at the public
    // boundary: the handler sees the Semget call, the reply carries the
    // handler's code back to the requester, over the send_reply verb.
    let mut transport = ScriptedTransport::new();
    transport.push(semget_request(42), IpcStatus::request());
    let server = IpcServer::new(transport, CannedHandler::answering(7));
    server.init();
    assert_eq!(server.run_once(), RunStep::Handled);
    let (transport, mut handler) = server.into_parts();
    assert_eq!(handler.calls, 1);
    assert_eq!(transport.outbound.len(), 1);
    assert_eq!(transport.outbound[0].dest, Endpoint(42));
    assert_eq!(transport.outbound[0].msg.m_type, 7);
    assert_eq!(transport.outbound[0].verb, "send_reply");
}

#[test]
fn suspend_round_produces_no_outbound() {
    // C: main.c:264 — SUSPEND suppresses the reply; the caller stays
    // blocked and the wakeup comes later as its own message.
    let mut transport = ScriptedTransport::new();
    transport.push(semget_request(42), IpcStatus::request());
    let server = IpcServer::new(transport, CannedHandler::answering(SUSPEND));
    server.init();
    assert_eq!(server.run_once(), RunStep::Handled);
    let (transport, _handler) = server.into_parts();
    assert!(transport.outbound.is_empty(), "SUSPEND must send nothing");
}

#[test]
fn proc_event_ack_travels_on_the_async_verb() {
    // C: main.c:207-208 — the process-event acknowledgement goes out via
    // asynsend3(AMF_NOREPLY), i.e. send_async (IPC-P2-2), and is echoed
    // even when the handler does nothing else.
    let mut msg = Message {
        m_source: Endpoint::PM,
        m_type: PROC_EVENT,
        ..Message::default()
    };
    msg.m_u.m_pm_lsys_proc_event = minix_types::MessPmLsysProcEvent {
        endpt: 33,
        event: minix_types::ProcEventIn::EXIT,
        ..Default::default()
    };
    let mut transport = ScriptedTransport::new();
    transport.push(msg, IpcStatus::request());
    let server = IpcServer::new(transport, CannedHandler::answering(0));
    server.init();
    assert_eq!(server.run_once(), RunStep::Handled);
    let (transport, mut handler) = server.into_parts();
    assert_eq!(handler.proc_events, 1);
    assert_eq!(transport.outbound.len(), 1);
    assert_eq!(transport.outbound[0].verb, "send_async");
    assert_eq!(
        transport.outbound[0].msg.m_type,
        proc_event_reply_type()
    );
}

#[test]
fn exhaustion_reports_receive_failure() {
    // An empty script means "transport broken": the loop counts the drop
    // and reports it instead of routing anything.
    let server = IpcServer::new(ScriptedTransport::new(), CannedHandler::answering(0));
    server.init();
    assert_eq!(server.run_once(), RunStep::ReceiveFailed);
    assert_eq!(server.dropped_messages(), 1);
    assert_eq!(server.completed_cycles(), 0);
}
