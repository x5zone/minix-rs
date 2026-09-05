//! Event bridge: binding to the input server and gating events.
//!
//! C correspondence: `inputdriver_announce`, `inputdriver_send_event`,
//! `do_conf`, and `do_setleds` in
//! `minix3/minix/lib/libinputdriver/inputdriver.c:20-135`, plus the
//! device flags `INPUT_DEV_KBD 0x01` and `INPUT_DEV_MOUSE 0x02`
//! (`input.h:9-10`) and the invalid identifier `INVALID_INPUT_ID -1`
//! (`input.h:13`).
//!
//! All bridge messages are one-way: the bridge never answers (the input
//! protocol has no replies). Sends are blocking in C (so a crashed
//! server is noticed at once); the service crate owns the send, this
//! module owns the gating.

/// Invalid device identifier: no device of this kind bound.
///
/// C: `INVALID_INPUT_ID (-1)` (`input.h:13`).
pub const INVALID_ID: i32 = -1;

/// Announced device flag: keyboard present.
///
/// C: `INPUT_DEV_KBD 0x01` (`input.h:9`).
pub const DEVICE_KEYBOARD: u32 = 0x01;
/// Announced device flag: mouse present.
///
/// C: `INPUT_DEV_MOUSE 0x02` (`input.h:10`).
pub const DEVICE_MOUSE: u32 = 0x02;

/// One input event: mouse or keyboard, page, code, value, flags.
///
/// C: the five fields of the `INPUT_EVENT` message
/// (`inputdriver.c:58-63`): identifier, page, code, value, flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    /// True for mouse events, false for keyboard events.
    pub mouse: bool,
    /// Usage page.
    pub page: u16,
    /// Usage code.
    pub code: u16,
    /// Press state, motion delta, or LED mask.
    pub value: i32,
    /// Modifier flags (relative motion and friends).
    pub flags: i32,
}

/// Bridge state: server endpoint plus per-kind device identifiers.
///
/// C: `input_endpt`, `kbd_id`, `mouse_id` (`inputdriver.c:11-13`).
/// `NONE` endpoint and invalid identifiers both gate events off; the
/// first send after a server crash unbinds (C resets `input_endpt` on a
/// failed send, `inputdriver.c:72-73`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputBridge {
    bound: bool,
    keyboard_id: i32,
    mouse_id: i32,
}

impl InputBridge {
    /// Fresh bridge: unbound, no identifiers.
    pub const fn new() -> InputBridge {
        InputBridge {
            bound: false,
            keyboard_id: INVALID_ID,
            mouse_id: INVALID_ID,
        }
    }

    /// True when events currently flow (bound with at least one id).
    pub const fn is_live(self) -> bool {
        self.bound && (self.keyboard_id != INVALID_ID || self.mouse_id != INVALID_ID)
    }

    /// Configure from the server's message, verified against the
    /// server endpoint the service crate resolved.
    ///
    /// C: `do_conf` (`inputdriver.c:82-111`): the sender must be the
    /// input server itself (verified by endpoint lookup in the service
    /// crate; `sender_is_server` carries that verdict), and two invalid
    /// identifiers disable the driver with a diagnostic.
    pub fn configure(&mut self, sender_is_server: bool, keyboard_id: i32, mouse_id: i32) -> bool {
        if !sender_is_server {
            return false;
        }
        self.bound = true;
        self.keyboard_id = keyboard_id;
        self.mouse_id = mouse_id;
        true
    }

    /// Note a failed send: unbind at once (the server is gone).
    ///
    /// C: `input_endpt = NONE` on send failure (`inputdriver.c:72-73`).
    pub fn note_send_failure(&mut self) {
        self.bound = false;
    }

    /// Resolve the device identifier for one event, if it may flow.
    ///
    /// C: the two gates in `inputdriver_send_event`
    /// (`inputdriver.c:49-54`): unbound, or invalid id for the kind,
    /// drops the event silently.
    pub const fn route(self, event: &InputEvent) -> Option<i32> {
        if !self.bound {
            return None;
        }
        let id = if event.mouse {
            self.mouse_id
        } else {
            self.keyboard_id
        };
        if id == INVALID_ID {
            return None;
        }
        Some(id)
    }

    /// Announced device flags for this hardware mix.
    ///
    /// C: `INPUT_DEV_KBD` always, plus `INPUT_DEV_MOUSE` when the
    /// auxiliary port exists (`pckbd.c:467-480`).
    pub const fn announce_flags(mouse_present: bool) -> u32 {
        if mouse_present {
            DEVICE_KEYBOARD | DEVICE_MOUSE
        } else {
            DEVICE_KEYBOARD
        }
    }
}

impl Default for InputBridge {
    fn default() -> Self {
        InputBridge::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_events_gate_off_until_configured() {
        let bridge = InputBridge::new();
        let event = InputEvent {
            mouse: false,
            page: 7,
            code: 0x28,
            value: 1,
            flags: 0,
        };
        assert!(!bridge.is_live());
        assert_eq!(bridge.route(&event), None);
    }

    #[test]
    fn test_configure_binds_and_routes_by_kind() {
        let mut bridge = InputBridge::new();
        assert!(!bridge.configure(false, 3, 4));
        assert!(bridge.configure(true, 3, INVALID_ID));
        assert!(bridge.is_live());
        let key = InputEvent {
            mouse: false,
            page: 7,
            code: 0x28,
            value: 1,
            flags: 0,
        };
        let mouse = InputEvent { mouse: true, ..key };
        assert_eq!(bridge.route(&key), Some(3));
        assert_eq!(bridge.route(&mouse), None);
    }

    #[test]
    fn test_send_failure_unbinds_at_once() {
        let mut bridge = InputBridge::new();
        bridge.configure(true, 3, 4);
        bridge.note_send_failure();
        assert!(!bridge.is_live());
    }

    #[test]
    fn test_announce_flags_cover_mouse_option() {
        assert_eq!(
            InputBridge::announce_flags(true),
            DEVICE_KEYBOARD | DEVICE_MOUSE
        );
        assert_eq!(InputBridge::announce_flags(false), DEVICE_KEYBOARD);
        assert_eq!(INVALID_ID, -1);
    }
}
