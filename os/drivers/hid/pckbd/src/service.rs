//! Service assembly: the pckbd driver's message pump over the shared runtime.
//!
//! C correspondence: `inputdriver_process` (`inputdriver.c:141-170`) — every
//! delivered message is classified (notifications by sender, plain messages
//! by number) and routed to `do_conf`, `do_setleds`, or a hook. The loop
//! shell and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`]; this module owns only the
//! one-message-at-a-time decision.
//!
//! The judgment core is [`minix_sys::inputdriver`] — the single authority
//! for the driver side of the input protocol ([`classify_incoming`],
//! [`verify_conf_sender`], [`accept_setleds`], [`decide_report`]), so this
//! file re-implements no routing rule. Two properties of the protocol shape
//! the pump: it has no replies at all (every message is one-way, so no
//! branch answers a caller), and event reports ride a *blocking* send whose
//! failure means the server died.
//!
//! Hardware seam: a real boot reads scan-code bytes from the keyboard
//! controller port inside the hardware-interrupt branch and feeds them to
//! [`PckbdService::feed_keyboard_byte`]. That port read is not wired here —
//! it belongs to the same I/O-privilege family as the console output backend
//! (document `06-tty-driver.md` §3.6) and is a registered gap, not a
//! fabricated success. The production, gating, and report path beyond the
//! port is complete and host-tested: a test injects bytes directly and
//! asserts what the driver sends the server.

use crate::char_face::{PckbdFace, WireEvent};
use crate::scancode::KeyMap;
use crate::tables::FullMap;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_sys::inputdriver::{
    accept_setleds, classify_incoming, decide_report, verify_conf_sender, ConfOutcome,
    DriverIncoming, NotifyKind, ReportVerdict, SetledsOutcome,
};
use minix_types::{decode_conf, decode_setleds, input_event_msg, Endpoint, Message};

/// The data-store label the input server publishes itself under. C:
/// `do_conf` resolves `"input"` and compares it to the sender before
/// trusting a configuration (`inputdriver.c:84`).
const INPUT_LABEL: &str = "input";

/// The pckbd service: the driver face plus the pump that answers arrivals.
///
/// Given a delivered message it classifies through the input-driver
/// authority, stores configuration, queues lights, and reports events
/// through the transport. `T` is generic so a host test injects a scripted
/// transport and asserts exactly what the driver sends the server.
pub struct PckbdService {
    /// The face over the scan-code and mouse state machines, the
    /// registration, and the LED outbox.
    pub face: PckbdFace,
}

impl PckbdService {
    /// Wrap a driver face as a service.
    pub fn new(face: PckbdFace) -> Self {
        PckbdService { face }
    }

    /// The notification family shares the 0x1000 base with the source in
    /// the low byte (C `is_ipc_notify`, `const.h`); the transport folds a
    /// notification into that type family on delivery.
    const fn is_ipc_notify(message_type: i32) -> bool {
        (message_type & !0xff) == 0x1000
    }

    /// Which notification arrived, from the sender endpoint. C sorts the
    /// hardware and clock tasks to their hooks and everything else to the
    /// catch-all (`inputdriver.c:144-163`).
    fn notify_kind(msg: &Message) -> NotifyKind {
        if msg.m_source == Endpoint::HARDWARE {
            NotifyKind::Hardware
        } else if msg.m_source == Endpoint::CLOCK {
            NotifyKind::Clock
        } else {
            NotifyKind::Other
        }
    }

    /// Handle one delivered message: classify, then route. No branch sends
    /// a reply — the input protocol is entirely one-way.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let notification = Self::is_ipc_notify(msg.m_type);
        let kind = if notification {
            Self::notify_kind(msg)
        } else {
            NotifyKind::Other
        };
        match classify_incoming(msg.m_type, notification, kind) {
            DriverIncoming::Configure => self.handle_configure(transport, msg),
            DriverIncoming::SetLights => self.handle_set_lights(msg),
            // A hardware interrupt reads the controller port and feeds the
            // bytes (the registered hardware seam — see module doc). The
            // clock and catch-all notifications, and any other message,
            // have no handler: pckbd registers no alarm or other hook, so
            // they are absorbed exactly as C absorbs a NULL hook.
            DriverIncoming::HardwareInterrupt
            | DriverIncoming::ClockAlarm
            | DriverIncoming::OtherNotify
            | DriverIncoming::OtherMessage => {}
        }
    }

    /// Accept a configuration, but only from the resolved input server.
    ///
    /// C: `do_conf` looks up `"input"`, ignores the message when the lookup
    /// fails or the sender is a stranger, and otherwise stores the sender
    /// and both slots (`inputdriver.c:82-111`).
    fn handle_configure<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let looked_up = transport.lookup_label(INPUT_LABEL);
        match verify_conf_sender(looked_up, msg.m_source) {
            ConfOutcome::Accept => {
                // The type is `INPUT_CONF` (classify guaranteed it), so a
                // corrupt reserved lane is the only reason to decode to
                // `None`; such a message is dropped rather than stored.
                if let Some((keyboard_slot, mouse_slot)) = decode_conf(msg) {
                    self.face.configure(msg.m_source, keyboard_slot, mouse_slot);
                }
            }
            // A stranger or an unresolvable label: ignore quietly. C logs a
            // line here; logging stays in the transport layer.
            ConfOutcome::IgnoreForeign | ConfOutcome::IgnoreLookupFailed => {}
        }
    }

    /// Queue a light command, but only when the server asked for it.
    ///
    /// C: `do_setleds` ignores a request from anyone but the configured
    /// server, then drives the lights (`inputdriver.c:119-135`). pckbd owns
    /// lights, so the callback always exists and the mask reaches the
    /// outbox.
    fn handle_set_lights(&mut self, msg: &Message) {
        let Some(requested) = decode_setleds(msg) else {
            return;
        };
        if let SetledsOutcome::Invoke { mask } =
            accept_setleds(self.face.registration, msg.m_source, requested)
        {
            self.face.outbox.queue(mask);
        }
    }

    /// Report one accepted event to the input server.
    ///
    /// C: `inputdriver_send_event` drops the event when no server is
    /// configured or the kind has no slot, then uses a blocking send and,
    /// on failure, forgets the server (`inputdriver.c:49-73`).
    pub fn report<T: DriverTransport>(&mut self, transport: &mut T, event: &WireEvent) {
        let slot = match decide_report(self.face.registration, event.mouse) {
            ReportVerdict::Send { slot } => slot,
            // Unconnected or unassigned: dropped before any transport call.
            ReportVerdict::DropUnconnected | ReportVerdict::DropUnassigned => return,
        };
        // A `Send` verdict only exists when a server is configured; guard
        // defensively rather than unwrap so the pump never panics.
        let Some(server) = self.face.registration.server else {
            return;
        };
        let mut message = input_event_msg(
            slot,
            i32::from(event.page),
            i32::from(event.code),
            event.value,
            event.flags,
        );
        if transport.send(server, &mut message).is_err() {
            self.face.note_server_lost();
        }
    }

    /// Feed one keyboard scan-code byte: run the state machine and report
    /// any event it produces. The hardware-interrupt branch calls this after
    /// reading the controller port (the registered hardware seam).
    pub fn feed_keyboard_byte<T: DriverTransport>(&mut self, transport: &mut T, byte: u8) {
        let map = FullMap;
        self.feed_keyboard(transport, &map, byte);
    }

    /// Feed one keyboard byte through an explicit key map, so a host test
    /// can drive the production-and-report path with the reference table.
    pub fn feed_keyboard<T: DriverTransport, M: KeyMap>(
        &mut self,
        transport: &mut T,
        map: &M,
        byte: u8,
    ) {
        if let Some(event) = self.face.keyboard_event(map, byte) {
            self.report(transport, &event);
        }
    }

    /// Feed one mouse byte: run the packet assembler and report every event
    /// of any completed packet.
    pub fn feed_mouse_byte<T: DriverTransport>(&mut self, transport: &mut T, byte: u8) {
        if let Some(events) = self.face.mouse_event(byte) {
            for event in events {
                self.report(transport, &event);
            }
        }
    }
}

impl<T: DriverTransport> DriverHandler<T> for PckbdService {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scancode::ReferenceMap;
    use alloc::vec::Vec;
    use minix_types::{conf_msg, setleds_msg, INVALID_INPUT_ID};

    /// Scripted transport: optionally resolves the `"input"` label, feeds
    /// queued messages to `receive`, and records every blocking send and
    /// one-way reply (destination, type, decoded slot/page/code/value).
    struct Scripted {
        server: Option<Endpoint>,
        incoming: Vec<Message>,
        sends: Vec<(Endpoint, i32, i32, i32, i32, i32)>,
        fail_sends: bool,
    }

    impl Scripted {
        fn new(server: Option<Endpoint>) -> Self {
            Scripted {
                server,
                incoming: Vec::new(),
                sends: Vec::new(),
                fail_sends: false,
            }
        }
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, msg: &mut Message) -> Result<(), i32> {
            if self.incoming.is_empty() {
                return Err(-minix_types::EIO);
            }
            *msg = self.incoming.remove(0);
            Ok(())
        }
        fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            if self.fail_sends {
                return Err(-minix_types::EIO);
            }
            // Only input-event reports reach `send` in these tests.
            let (id, page, code, value, _flags) =
                minix_types::decode_input_event(msg).expect("scripted send carries an input event");
            self.sends.push((dst, msg.m_type, id, page, code, value));
            Ok(())
        }
        fn asynsend(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            // The input protocol never sends a one-way reply.
            Ok(())
        }
        fn copy_from_grant(&mut self, _g: Endpoint, _gr: i32, _o: u64, _b: &mut [u8]) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_grant(&mut self, _g: Endpoint, _gr: i32, _o: u64, _b: &[u8]) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
        fn lookup_label(&mut self, _label: &str) -> Option<Endpoint> {
            self.server
        }
    }

    fn service() -> PckbdService {
        PckbdService::new(PckbdFace::new())
    }

    /// A configuration from the resolved server stores the endpoint and
    /// both slots, so a later keyboard event flows.
    #[test]
    fn test_configure_from_server_is_stored() {
        let server = Endpoint(6);
        let mut svc = service();
        let mut msg = conf_msg(1, INVALID_INPUT_ID);
        msg.m_source = server;
        let mut transport = Scripted::new(Some(server));
        svc.dispatch(&mut transport, &msg);
        assert_eq!(svc.face.registration.server, Some(server));
        assert_eq!(svc.face.registration.keyboard_slot, Some(1));
        assert_eq!(svc.face.registration.mouse_slot, None);
    }

    /// A configuration from a stranger (sender does not match the resolved
    /// label) is ignored: nothing is stored.
    #[test]
    fn test_configure_from_stranger_is_ignored() {
        let mut svc = service();
        let mut msg = conf_msg(1, 2);
        msg.m_source = Endpoint(99); // not the resolved "input" endpoint
        let mut transport = Scripted::new(Some(Endpoint(6)));
        svc.dispatch(&mut transport, &msg);
        assert!(svc.face.registration.server.is_none());
    }

    /// A light request from the configured server queues two outbox bytes
    /// (the command byte plus the mask).
    #[test]
    fn test_setleds_from_server_queues_command() {
        let server = Endpoint(6);
        let mut svc = service();
        svc.face.configure(server, 0, 1);
        let mut msg = setleds_msg(1 << crate::led::LOCK_CAPS);
        msg.m_source = server;
        let mut transport = Scripted::new(Some(server));
        svc.dispatch(&mut transport, &msg);
        assert_eq!(svc.face.outbox.pending(), 2);
    }

    /// A light request from a stranger leaves the outbox untouched.
    #[test]
    fn test_setleds_from_stranger_is_ignored() {
        let mut svc = service();
        svc.face.configure(Endpoint(6), 0, 1);
        let mut msg = setleds_msg(1 << crate::led::LOCK_NUM);
        msg.m_source = Endpoint(7); // not the configured server
        let mut transport = Scripted::new(Some(Endpoint(6)));
        svc.dispatch(&mut transport, &msg);
        assert_eq!(svc.face.outbox.pending(), 0);
    }

    /// A configured keyboard scan-code is produced, gated, and sent to the
    /// server with the assigned slot.
    #[test]
    fn test_configured_key_event_is_sent_to_server() {
        let server = Endpoint(6);
        let mut svc = service();
        svc.face.configure(server, 3, INVALID_INPUT_ID);
        let mut transport = Scripted::new(Some(server));
        // Reference map: 0x1C is Enter, a down event.
        svc.feed_keyboard(&mut transport, &ReferenceMap, 0x1C);
        assert_eq!(transport.sends.len(), 1);
        let (dst, kind, id, page, code, _) = transport.sends[0];
        assert_eq!((dst, kind), (server, minix_types::INPUT_EVENT));
        assert_eq!((id, page, code), (3, 0x0007, 0x0028));
    }

    /// The hardware-seam entry `feed_keyboard_byte` drives the production
    /// full table (`FullMap`): a configured Enter scan-code reaches the
    /// server with the keyboard slot, exactly as the reference-map path.
    #[test]
    fn test_feed_keyboard_byte_uses_full_table() {
        let server = Endpoint(6);
        let mut svc = service();
        svc.face.configure(server, 3, INVALID_INPUT_ID);
        let mut transport = Scripted::new(Some(server));
        // FullMap: 0x1C is Enter (page 0x0007, code 0x0028), a down event.
        svc.feed_keyboard_byte(&mut transport, 0x1C);
        assert_eq!(transport.sends.len(), 1);
        let (dst, kind, id, page, code, value) = transport.sends[0];
        assert_eq!((dst, kind), (server, minix_types::INPUT_EVENT));
        assert_eq!((id, page, code, value), (3, 0x0007, 0x0028, 1));
    }

    /// An event before any configuration is dropped without a send.
    #[test]
    fn test_unconfigured_event_is_dropped() {
        let mut svc = service();
        let mut transport = Scripted::new(None);
        svc.feed_keyboard(&mut transport, &ReferenceMap, 0x1C);
        assert!(transport.sends.is_empty());
    }

    /// A failed blocking send makes the driver forget the server, so later
    /// events stop until a fresh configuration arrives.
    #[test]
    fn test_send_failure_forgets_server() {
        let server = Endpoint(6);
        let mut svc = service();
        svc.face.configure(server, 3, INVALID_INPUT_ID);
        let mut transport = Scripted::new(Some(server));
        transport.fail_sends = true;
        svc.feed_keyboard(&mut transport, &ReferenceMap, 0x1C);
        assert!(svc.face.registration.server.is_none());
        // Now unconnected: a second event is dropped (no send attempted).
        transport.fail_sends = false;
        svc.feed_keyboard(&mut transport, &ReferenceMap, 0x1C);
        assert!(transport.sends.is_empty());
    }

    /// A completed mouse packet's events all reach the server, each with
    /// the mouse slot.
    #[test]
    fn test_mouse_packet_reports_all_events() {
        let server = Endpoint(6);
        let mut svc = service();
        svc.face.configure(server, INVALID_INPUT_ID, 4);
        let mut transport = Scripted::new(Some(server));
        svc.feed_mouse_byte(&mut transport, 0x18);
        svc.feed_mouse_byte(&mut transport, 0xFF);
        svc.feed_mouse_byte(&mut transport, 0x00);
        assert_eq!(transport.sends.len(), 1);
        assert_eq!(transport.sends[0].0, server);
        assert_eq!(transport.sends[0].2, 4, "mouse slot");
    }

    /// A hardware notification is absorbed without a send or a reply.
    #[test]
    fn test_hardware_notify_takes_no_action() {
        let mut svc = service();
        let msg = Message {
            m_type: 0x1000,
            m_source: Endpoint::HARDWARE,
            ..Message::default()
        };
        let mut transport = Scripted::new(None);
        svc.dispatch(&mut transport, &msg);
        assert!(transport.sends.is_empty());
    }
}
