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
use minix_types::Message;

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
}
