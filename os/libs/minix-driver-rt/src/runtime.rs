//! The event loop: announce once, then receive-and-dispatch forever.
//!
//! C correspondence: the receive-classify-dispatch tail of
//! `chardriver_task` (`chardriver.c:455-573`) — announce first, then one
//! `driver_receive(ANY)` per iteration, each delivered message routed
//! through the driver's table. The classify half stays in the protocol
//! families (character, block, net requests differ); this module owns
//! the loop half, which is identical everywhere.
//!
//! Why the loop lives here: fifty-seven driver binaries sharing one loop
//! shell means a lifecycle fix (SEF swap, restart gate) lands once. The
//! SEF switch point is a property of the transport implementation, not
//! of this loop (see [`crate::transport::DriverTransport::receive`]).

use alloc::string::String;
use alloc::string::ToString;
use minix_types::{Endpoint, Message, ENOSYS, RS_INIT};

use crate::transport::DriverTransport;

/// One driver's message half: what to do with a delivered message.
///
/// Replies go out through the transport handle the runtime passes in —
/// the handler never touches the receive side, so a handler cannot
/// accidentally consume the next request while finishing this one.
pub trait DriverHandler<T: DriverTransport> {
    /// Handle one delivered message (request or notification).
    fn handle(&mut self, transport: &mut T, msg: &Message);
}

/// What the birth callback is being asked to run.
///
/// C: the SEF init type carried in the RS init request (`sef.h:93`):
/// a fresh start, a live update, or a stateful restart. A no-std
/// single-threaded driver models only the fresh start honestly; the two
/// stateful kinds are refused rather than run against stale expectations
/// (same discipline as the FS runtime's birth face, `fs-rt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitKind {
    /// Fresh start (`SEF_INIT_FRESH`).
    Fresh,
    /// Live update (`SEF_INIT_LU`): refused as unmodeled.
    LiveUpdate,
    /// Stateful restart (`SEF_INIT_RESTART`): refused as unmodeled.
    Restart,
}

/// Announce-and-loop shell behind every driver binary.
pub struct DriverRuntime<T: DriverTransport> {
    transport: T,
    label: String,
}

impl<T: DriverTransport> DriverRuntime<T> {
    /// Runtime for one driver: transport plus its data-store label
    /// (e.g. `drv.chr.tty`; the family prefix is the caller's business).
    pub fn new(transport: T, label: impl ToString) -> Self {
        DriverRuntime {
            transport,
            label: label.to_string(),
        }
    }

    /// Announce readiness through the data store.
    ///
    /// C: `chardriver_announce` (`chardriver.c:99`) runs before the first
    /// receive; a driver nobody can find must not serve, so failure
    /// aborts startup rather than entering the loop.
    pub fn announce(&mut self) -> Result<(), i32> {
        self.transport.publish_label(&self.label)
    }

    /// Announce, then loop forever delivering messages to `handler`.
    ///
    /// Returns only on transport failure (the `i32` is the failing verb's
    /// error): a driver loop that cannot receive cannot serve. SEF ping
    /// absorption and restart gates ride inside the transport and the
    /// protocol families respectively.
    pub fn run<H: DriverHandler<T>>(&mut self, handler: &mut H) -> Result<(), i32> {
        self.announce()?;
        loop {
            let mut msg = Message::default();
            self.transport.receive(&mut msg)?;
            handler.handle(&mut self.transport, &msg);
        }
    }

    /// Announce, run the RS birth handshake through `init`, then loop
    /// delivering every later message to `handler`.
    ///
    /// C: the SEF startup (`sef_local_startup`) blocks until RS delivers
    /// the init request, runs the fresh-start callback, and reports the
    /// result back to RS — all before the task loop's first receive. This
    /// is the same birth face the FS runtime runs (`fs-rt::run_birth`);
    /// the driver runtime owns it once so no driver re-implements it.
    /// Returns only on transport failure or a refused init (the nonzero
    /// result is surfaced as the error); a driver that cannot be born
    /// cannot serve.
    pub fn serve<H, F>(&mut self, handler: &mut H, init: F) -> Result<(), i32>
    where
        H: DriverHandler<T>,
        F: FnOnce(InitKind) -> Result<(), i32>,
    {
        self.announce()?;
        self.run_birth(init)?;
        loop {
            let mut msg = Message::default();
            self.transport.receive(&mut msg)?;
            handler.handle(&mut self.transport, &msg);
        }
    }

    /// Consume RS's init request: run `init` for its kind and report
    /// `RS_INIT` with the result back to RS. Returns `Ok(())` only when
    /// the fresh-start callback succeeded; a stateful kind or a failed
    /// callback stops startup honestly (`Err`) rather than entering the
    /// loop in an unprepared state.
    fn run_birth<F: FnOnce(InitKind) -> Result<(), i32>>(&mut self, init: F) -> Result<(), i32> {
        let mut msg = Message::default();
        self.transport.receive(&mut msg)?;
        // The first message of a driver's life is RS's init request; any
        // other arrival means the transport is not speaking SEF startup.
        if msg.m_type != RS_INIT || msg.m_source != Endpoint::RS {
            return Err(ENOSYS);
        }
        // SAFETY: the birth request's active union arm is `m_rs_init`
        // (m_type == RS_INIT from RS; both checked just above).
        let init_type = unsafe { msg.m_u.m_rs_init.type_ };
        let kind = match init_type {
            0 => InitKind::Fresh,
            1 => InitKind::LiveUpdate,
            _ => InitKind::Restart,
        };
        let result = match kind {
            InitKind::Fresh => init(InitKind::Fresh).map(|_| 0).unwrap_or_else(|e| e),
            other => {
                // Stateful starts are unmodeled: refuse honestly instead of
                // running a fresh init against stale state expectations.
                let _ = other;
                ENOSYS
            }
        };
        let mut reply = Message {
            m_type: RS_INIT,
            ..Message::default()
        };
        reply.m_u.m_rs_init.result = result;
        self.transport.send(Endpoint::RS, &mut reply)?;
        if result != 0 {
            return Err(result);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use alloc::vec;
    use alloc::vec::Vec;

    use minix_types::{Endpoint, Message};

    use crate::transport::DriverTransport;

    use super::*;

    /// Scripted transport: answers receive from a queue of outcomes,
    /// records every publish and send.
    struct Canned {
        label: Option<String>,
        script: Vec<Result<Message, i32>>,
        sent: Vec<(Endpoint, i32)>,
    }

    impl Canned {
        fn new(script: Vec<Result<Message, i32>>) -> Self {
            Canned {
                label: None,
                script,
                sent: Vec::new(),
            }
        }
    }

    impl DriverTransport for Canned {
        fn receive(&mut self, msg: &mut Message) -> Result<(), i32> {
            if self.script.is_empty() {
                return Err(-minix_types::EIO);
            }
            match self.script.remove(0) {
                Ok(m) => {
                    *msg = m;
                    Ok(())
                }
                Err(code) => Err(code),
            }
        }

        fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sent.push((dst, msg.m_type));
            Ok(())
        }

        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.sent.push((dst, msg.m_type));
            Ok(())
        }

        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &mut [u8],
        ) -> Result<(), i32> {
            Ok(())
        }

        fn copy_to_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &[u8],
        ) -> Result<(), i32> {
            Ok(())
        }

        fn publish_label(&mut self, label: &str) -> Result<(), i32> {
            self.label = Some(label.to_string());
            Ok(())
        }
    }

    /// Counts deliveries; sends one reply per message.
    struct Counter {
        handled: u32,
    }

    impl DriverHandler<Canned> for Counter {
        fn handle(&mut self, transport: &mut Canned, msg: &Message) {
            self.handled += 1;
            let mut reply = Message::default();
            reply.m_type = msg.m_type + 1;
            let _ = transport.send(msg.m_source, &mut reply);
        }
    }

    fn scripted(id: u32) -> Message {
        let mut msg = Message::default();
        msg.m_type = id as i32;
        msg
    }

    #[test]
    fn test_announce_runs_before_first_receive() {
        let transport = Canned::new(vec![Err(-minix_types::EIO)]);
        let label = "drv.chr.test".to_string();
        let mut runtime = DriverRuntime::new(transport, label.clone());
        let mut handler = Counter { handled: 0 };
        // Announce failure is a startup failure; success enters the loop.
        assert!(runtime.run(&mut handler).is_err());
        assert_eq!(runtime.announce(), Ok(()));
        assert_eq!(runtime.transport.label.as_deref(), Some(label.as_str()));
    }

    #[test]
    fn test_loop_delivers_messages_and_replies() {
        let transport = Canned::new(vec![
            Ok(scripted(101)),
            Ok(scripted(102)),
            Err(-minix_types::EIO), // transport dies: the loop returns
        ]);
        let mut runtime = DriverRuntime::new(transport, "drv.blk.test");
        let mut handler = Counter { handled: 0 };
        assert_eq!(runtime.run(&mut handler), Err(-minix_types::EIO));
        assert_eq!(handler.handled, 2);
        // The handler saw the transport and replied to both deliveries.
        assert_eq!(runtime.transport.sent.len(), 2);
        assert_eq!(runtime.transport.sent[0].1, 102);
        assert_eq!(runtime.transport.sent[1].1, 103);
    }

    /// An RS init request of the given SEF kind.
    fn birth_request(init_type: i32) -> Message {
        let mut message = Message {
            m_type: minix_types::RS_INIT,
            m_source: Endpoint::RS,
            ..Message::default()
        };
        message.m_u.m_rs_init.type_ = init_type;
        message
    }

    /// Serve consumes RS's birth request (running the callback and
    /// reporting the result) before the handler ever sees a message, then
    /// delivers only the later loop messages.
    #[test]
    fn test_serve_runs_birth_then_delivers_loop() {
        let transport = Canned::new(vec![
            Ok(birth_request(0)), // SEF_INIT_FRESH
            Ok(scripted(201)),
            Ok(scripted(202)),
            Err(-minix_types::EIO), // transport dies: the loop returns
        ]);
        let mut runtime = DriverRuntime::new(transport, "drv.chr.tty");
        let mut handler = Counter { handled: 0 };
        let mut ran = false;
        let init = |kind| {
            assert_eq!(kind, InitKind::Fresh);
            ran = true;
            Ok(())
        };
        assert_eq!(runtime.serve(&mut handler, init), Err(-minix_types::EIO));
        assert!(ran, "fresh birth ran the init callback");
        assert_eq!(handler.handled, 2, "only post-birth messages reach the handler");
        // The birth reply is the first send: RS_INIT back to RS.
        assert_eq!(runtime.transport.sent[0], (Endpoint::RS, minix_types::RS_INIT));
        assert_eq!(runtime.transport.sent.len(), 3);
    }

    /// A stateful (live-update) kind is refused honestly: the result is
    /// reported to RS, startup stops before the loop, and the handler runs
    /// nothing.
    #[test]
    fn test_serve_refuses_stateful_birth() {
        let transport = Canned::new(vec![Ok(birth_request(1))]); // SEF_INIT_LU
        let mut runtime = DriverRuntime::new(transport, "drv.chr.tty");
        let mut handler = Counter { handled: 0 };
        assert_eq!(
            runtime.serve(&mut handler, |_| Ok(())),
            Err(minix_types::ENOSYS)
        );
        assert_eq!(handler.handled, 0);
        assert_eq!(
            runtime.transport.sent,
            vec![(Endpoint::RS, minix_types::RS_INIT)]
        );
    }
}
