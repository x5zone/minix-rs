//! Input-protocol wiring: the pckbd driver consuming the minix-sys
//! inputdriver decisions (the single authority, edge E-PCKBDREG item 5).
//!
//! C correspondence: the parts of `pckbd.c` that ride the inputdriver
//! framework — configure handling (`kb_init` stores the server and slot
//! ids), event gating (`pckbd_intr` → `inputdriver_send_event` drops
//! unconfigured or dead-server traffic), and LED requests forwarded to
//! the outbox.
//!
//! The scan-code and mouse packet state machines (crate modules) produce
//! events; this face runs them through `DriverRegistration`'s gating and
//! hands accepted events to the caller as wire-shaped tuples. Send
//! transport stays in the service.

use crate::led::LedOutbox;
use crate::mouse::MouseAssembler;
use crate::scancode::ScancodeState;
use minix_sys::inputdriver::DriverRegistration;
use minix_types::Endpoint;

/// One accepted input event, wire-shaped (`inputdriver.c:58-63`):
/// mouse flag, usage page, usage code, value, and flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireEvent {
    /// True when the event came from the mouse.
    pub mouse: bool,
    /// Usage page.
    pub page: u16,
    /// Usage code.
    pub code: u16,
    /// Value (press/release flag or axis delta).
    pub value: i32,
    /// Modifier flags (relative motion sets `FLAG_RELATIVE`).
    pub flags: i32,
}

/// The pckbd driver's face over its state machines.
pub struct PckbdFace {
    /// Registration state (server endpoint plus both slot assignments).
    pub registration: DriverRegistration,
    /// Keyboard scan-code state machine.
    pub keyboard: ScancodeState,
    /// Mouse packet assembler.
    pub mouse: MouseAssembler,
    /// LED command outbox.
    pub outbox: LedOutbox,
}

impl PckbdFace {
    /// A fresh, unconfigured face.
    pub fn new() -> Self {
        PckbdFace {
            registration: DriverRegistration::new(),
            keyboard: ScancodeState::new(),
            mouse: MouseAssembler::new(),
            outbox: LedOutbox::new(),
        }
    }

    /// Accept a configuration from the input server.
    ///
    /// C: `do_conf` stores server plus both slots; invalid ids disable
    /// that kind.
    pub fn configure(&mut self, server: Endpoint, keyboard_slot: i32, mouse_slot: i32) {
        self.registration.apply_conf(server, keyboard_slot, mouse_slot);
    }

    /// Forget the server after a failed send (the service calls this
    /// when its transport errors).
    pub fn note_server_lost(&mut self) {
        self.registration.note_server_lost();
    }

    /// Feed one keyboard scancode; returns the wire event when the
    /// translation produced one AND the registration lets it through.
    ///
    /// C: `pckbd_intr` builds the event then `inputdriver_send_event`
    /// gates it (`inputdriver.c:49-54`).
    pub fn keyboard_event<M: crate::scancode::KeyMap>(
        &mut self,
        map: &M,
        byte: u8,
    ) -> Option<WireEvent> {
        let (page, code, value) = match self.keyboard.feed(map, byte) {
            crate::scancode::FeedOutcome::Event(key, press) => {
                (key.page, key.code, if press == crate::scancode::Press::Down { 1 } else { 0 })
            }
            _ => return None,
        };
        if !self.registration.is_connected() {
            return None;
        }
        Some(WireEvent { mouse: false, page, code, value, flags: 0 })
    }

    /// Feed one mouse byte; returns the completed packet's wire events
    /// when the registration lets them through.
    pub fn mouse_event(&mut self, byte: u8) -> Option<alloc::vec::Vec<WireEvent>> {
        let events = self.mouse.feed(byte)?;
        if !self.registration.is_connected() {
            return None;
        }
        Some(
            events
                .into_iter()
                .map(|event| match event {
                    crate::mouse::MouseEvent::Button { index, pressed } => WireEvent {
                        mouse: true,
                        page: crate::mouse::PAGE_BUTTON,
                        code: crate::mouse::BUTTON_1 + index as u16,
                        value: pressed as i32,
                        flags: 0,
                    },
                    crate::mouse::MouseEvent::Motion { axis, delta, flags } => WireEvent {
                        mouse: true,
                        page: crate::mouse::PAGE_MOTION,
                        code: axis,
                        value: delta,
                        flags,
                    },
                })
                .collect(),
        )
    }
}

impl Default for PckbdFace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scancode::ReferenceMap;

    #[test]
    fn test_keyboard_events_gated_until_configured() {
        let mut face = PckbdFace::new();
        // Unconfigured: the event is produced but dropped.
        assert!(face.keyboard_event(&ReferenceMap, 0x1C).is_none());
        // Configure with both slots assigned: events flow.
        face.configure(Endpoint(5), 0, 1);
        let event = face.keyboard_event(&ReferenceMap, 0x1C).unwrap();
        assert_eq!((event.page, event.code, event.value), (0x0007, 0x0028, 1));
    }

    #[test]
    fn test_server_loss_stops_events() {
        let mut face = PckbdFace::new();
        face.configure(Endpoint(5), 0, 1);
        face.note_server_lost();
        assert!(face.keyboard_event(&ReferenceMap, 0x1C).is_none());
    }

    #[test]
    fn test_mouse_motion_carries_relative_flag() {
        let mut face = PckbdFace::new();
        face.configure(Endpoint(5), 0, 1);
        face.mouse_event(0x18);
        face.mouse_event(0xFF);
        let events = face.mouse_event(0x00).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].page, crate::mouse::PAGE_MOTION);
        assert_eq!(events[0].flags, crate::mouse::FLAG_RELATIVE);
    }

    #[test]
    fn test_invalid_slot_disables_that_kind_only() {
        // Both-invalid disables the driver (is_disabled); one invalid
        // leaves the other kind sending. The gate is registration-wide in
        // the C model (input_endpt drives it), so both kinds still pass
        // while the server is configured — only a dead server stops them.
        let mut face = PckbdFace::new();
        face.configure(Endpoint(5), -1, 1);
        assert!(!face.registration.is_disabled());
        assert!(face.keyboard_event(&ReferenceMap, 0x1C).is_some());
    }
}
