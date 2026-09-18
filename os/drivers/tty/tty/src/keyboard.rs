//! Keyboard-side input consumption: the TTY end of the input protocol.
//!
//! C correspondence: `minix3/minix/drivers/tty/tty/arch/i386/keyboard.c`
//! — the input-up handshake (:131-146), the relayed-event filter
//! (:148-176), and the LED write-back (:369-385) — plus the 32-slot
//! scancode ring (:30-40). The input SERVER side (handshake initiation,
//! event relay, setleds fan-out) already exists in `os/servers/input`;
//! this module is the TTY consumer that the protocol has been missing.
//!
//! Message construction and transmission stay out: every handler returns
//! plain effects, and the service pump (driver main assembly, 16-stage
//! G6) performs the sends over a real transport. That keeps the whole
//! protocol logic host-testable.

use alloc::vec::Vec;
use minix_types::ipc::{
    decode_tty_event, setleds_msg, EventPage, TTY_INPUT_EVENT, TTY_INPUT_UP,
};
use minix_types::{Endpoint, Message};

/// Scancode ring capacity. C: `KB_IN_BYTES 32` (`keyboard.c:30`).
const KB_IN_BYTES: usize = 32;
/// Release flag or-ed into a stored scancode. C: `RELEASE_BIT 0x8000`
/// (`keyboard.c:35`) — a u16 flag on top of the 0x0000..=0x00E7 code range,
/// unrelated to the PC hardware release bit (0x80) pckbd consumes.
const RELEASE_BIT: u16 = 0x8000;
/// Keymap row count: codes at or above it are dropped. C:
/// `NR_SCAN_CODES 0xE8` (`minix3/minix/include/minix/keymap.h:163`).
const NR_SCAN_CODES: i32 = 0xE8;
/// Lock mask bits, one per keyboard lock, sent as the LED mask. C:
/// `NUM_LOCK 0x2 / CAPS_LOCK 0x4 / SCROLL_LOCK 0x8 / ALT_LOCK 0x10`
/// (`keyboard.c:57-61`) — chosen to equal the input LED mask bits
/// (`1 << LedCode`).
mod lock_bits {
    pub const NUM_LOCK: u32 = 0x2;
    pub const CAPS_LOCK: u32 = 0x4;
    pub const SCROLL_LOCK: u32 = 0x8;
    /// Pseudo-lock kept out of every LED mask. C: `ALT_LOCK 0x10`.
    pub const ALT_LOCK: u32 = 0x10;
}

/// One side effect a handler asks the pump to perform.
#[derive(Debug, Clone)]
pub enum InputEffect {
    /// Send this message asynchronously to the stored input endpoint
    /// (C: `asynsend3(input_endpt, &m, AMF_NOREPLY)`, `keyboard.c:379`).
    SendSetLeds(Message),
}

/// The keyboard half of the input protocol: handshake state, the scancode
/// ring, and the LED lock bits.
///
/// `input_endpoint` starts absent — until the handshake lands, every
/// relayed event is rejected (C: `input_endpt == NONE`, `keyboard.c:75`).
pub struct KeyboardInput {
    input_endpoint: Option<Endpoint>,
    locks: u32,
    ring_head: usize,
    ring_count: usize,
    ring: [u16; KB_IN_BYTES],
}

impl Default for KeyboardInput {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardInput {
    /// Creates the pre-handshake state: no peer, empty ring.
    pub fn new() -> Self {
        KeyboardInput {
            input_endpoint: None,
            locks: 0,
            ring_head: 0,
            ring_count: 0,
            ring: [0; KB_IN_BYTES],
        }
    }

    /// Handles the input-up handshake (C: `do_input`'s `TTY_INPUT_UP`
    /// branch, `keyboard.c:131-146`).
    ///
    /// `ds_input_endpoint` is the caller's DS lookup of the `"input"`
    /// label — `None` (lookup failed) rejects the request, and a mismatch
    /// with the kernel-stamped source rejects it just as C's anti-spoofing
    /// check does. On success the endpoint is stored and the initial LED
    /// synchronization is scheduled (C `keyboard.c:144` calls `set_leds`
    /// immediately).
    pub fn handle_input_up(
        &mut self,
        message: &Message,
        ds_input_endpoint: Option<Endpoint>,
    ) -> Option<InputEffect> {
        if message.m_type != TTY_INPUT_UP {
            return None;
        }
        let expected = ds_input_endpoint?;
        if message.m_source != expected {
            return None;
        }
        self.input_endpoint = Some(expected);
        self.set_leds_effect()
    }

    /// Handles one relayed input event (C: `do_input`'s
    /// `TTY_INPUT_EVENT` branch, `keyboard.c:148-176`).
    ///
    /// Rejections, in C order: unknown message type, no handshaked peer or
    /// a foreign source, non-keyboard usage page, code at or beyond the
    /// keymap row count. A released key enters the ring with the release
    /// flag or-ed in; a full ring silently drops (C `incount <
    /// KB_IN_BYTES` guard). Returns whether a scancode entered the ring.
    pub fn handle_tty_event(&mut self, message: &Message) -> bool {
        if message.m_type != TTY_INPUT_EVENT {
            return false;
        }
        let Some(peer) = self.input_endpoint else {
            return false;
        };
        if message.m_source != peer {
            return false;
        }
        let Some((_id, page, code, value, _flags)) = decode_tty_event(message) else {
            return false;
        };
        if page != EventPage::Keyboard as i32 {
            return false;
        }
        if code >= NR_SCAN_CODES {
            return false;
        }
        let mut scancode = code as u16;
        if value == 0 {
            // C: `value == INPUT_RELEASE` (0) — release rides the flag bit.
            scancode |= RELEASE_BIT;
        }
        self.push(scancode)
    }

    /// Builds the LED write-back for the current lock bits.
    ///
    /// C: `set_leds` (`keyboard.c:369-385`) — nothing sends before the
    /// handshake, and the pseudo `ALT_LOCK` bit is stripped from the mask.
    pub fn set_leds_effect(&self) -> Option<InputEffect> {
        self.input_endpoint?;
        Some(InputEffect::SendSetLeds(setleds_msg(self.locks & !lock_bits::ALT_LOCK)))
    }

    /// The stored peer, for the pump's addressing.
    pub fn input_endpoint(&self) -> Option<Endpoint> {
        self.input_endpoint
    }

    /// Records the caller's lock-bit change (C keeps `locks[ccurrent]`;
    /// the toggle sites are the line discipline's business).
    pub fn set_locks(&mut self, locks: u32) {
        self.locks = locks;
    }

    /// Current lock bits masked for sending.
    pub fn led_mask(&self) -> u32 {
        self.locks & !lock_bits::ALT_LOCK
    }

    /// Consumes the scancores queued so far, in FIFO order.
    ///
    /// C: `kb_read`'s ring walk (`keyboard.c:184-278` reduced to the
    /// dequeue the line discipline needs).
    pub fn drain_scancodes(&mut self, out: &mut Vec<u16>) {
        while self.ring_count > 0 {
            let scancode = self.ring[self.ring_head];
            self.ring_head = (self.ring_head + 1) % KB_IN_BYTES;
            self.ring_count -= 1;
            out.push(scancode);
        }
    }

    /// Pushes one scancode, silently dropping into a full ring.
    /// Returns whether it entered.
    fn push(&mut self, scancode: u16) -> bool {
        if self.ring_count >= KB_IN_BYTES {
            return false;
        }
        let tail = (self.ring_head + self.ring_count) % KB_IN_BYTES;
        self.ring[tail] = scancode;
        self.ring_count += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::ipc::{tty_event_msg, INPUT_SETLEDS};

    fn event_message(source: Endpoint, page: i32, code: i32, value: i32) -> Message {
        let mut message = tty_event_msg(0, page, code, value, 0);
        // The kernel stamps the sender; the tests stamp it themselves.
        message.m_source = source;
        message
    }

    fn up_message(source: Endpoint) -> Message {
        let mut message = minix_types::ipc::tty_up_msg();
        message.m_source = source;
        message
    }

    fn handshake(state: &mut KeyboardInput, source: Endpoint) -> Option<InputEffect> {
        state.handle_input_up(&up_message(source), Some(source))
    }

    /// Handshake: spoofed source rejected, DS miss rejected, genuine
    /// source stored and answered with the initial LED sync.
    #[test]
    fn test_input_up_handshake_validates_and_stores() {
        let input = Endpoint(7);
        let mut state = KeyboardInput::new();
        assert_eq!(state.input_endpoint(), None);
        // Spoofed source: DS says input lives elsewhere.
        assert!(state.handle_input_up(&up_message(Endpoint(9)), Some(input)).is_none());
        assert_eq!(state.input_endpoint(), None);
        // DS lookup failure: nothing to verify against, reject.
        assert!(state.handle_input_up(&up_message(input), None).is_none());
        // Unknown message type: ignored.
        assert!(state.handle_input_up(&event_message(input, 0, 0, 1), Some(input)).is_none());
        // Genuine handshake: stored, plus the initial set_leds effect.
        let effect = handshake(&mut state, input);
        assert_eq!(state.input_endpoint(), Some(input));
        match effect {
            Some(InputEffect::SendSetLeds(msg)) => {
                assert_eq!(msg.m_type, INPUT_SETLEDS);
            }
            _ => panic!("expected set_leds effect"),
        }
    }

    /// Event gate order: pre-handshake and foreign sources drop before
    /// any decoding; non-keyboard pages and out-of-range codes drop
    /// without entering the ring.
    #[test]
    fn test_tty_event_filter_chain() {
        let input = Endpoint(7);
        let mut state = KeyboardInput::new();
        assert!(!state.handle_tty_event(&event_message(input, EventPage::Keyboard as i32, 0x04, 1)));
        handshake(&mut state, input);
        // Foreign source.
        assert!(!state.handle_tty_event(&event_message(Endpoint(9), EventPage::Keyboard as i32, 0x04, 1)));
        // Mouse page dropped.
        assert!(!state.handle_tty_event(&event_message(input, 0x0001, 0x04, 1)));
        // Beyond the keymap rows.
        assert!(!state.handle_tty_event(&event_message(input, EventPage::Keyboard as i32, 0xE8, 1)));
        // A press passes.
        assert!(state.handle_tty_event(&event_message(input, EventPage::Keyboard as i32, 0x04, 1)));
    }

    /// Press stores the raw code, release stores code | 0x8000 — the
    /// released-key convention `kb_read` downstream expects.
    #[test]
    fn test_press_and_release_scancodes() {
        let input = Endpoint(7);
        let mut state = KeyboardInput::new();
        handshake(&mut state, input);
        assert!(state.handle_tty_event(&event_message(input, EventPage::Keyboard as i32, 0x1E, 1)));
        assert!(state.handle_tty_event(&event_message(input, EventPage::Keyboard as i32, 0x1E, 0)));
        let mut drained = Vec::new();
        state.drain_scancodes(&mut drained);
        assert_eq!(drained, alloc::vec![0x1E, 0x1E | 0x8000]);
    }

    /// The ring holds exactly 32 entries; overflow drops silently and the
    /// ring stays consistent.
    #[test]
    fn test_ring_capacity_drops_overflow() {
        let input = Endpoint(7);
        let mut state = KeyboardInput::new();
        handshake(&mut state, input);
        for code in 0..(KB_IN_BYTES as i32 + 5) {
            let entered =
                state.handle_tty_event(&event_message(input, EventPage::Keyboard as i32, code, 1));
            assert_eq!(entered, code < KB_IN_BYTES as i32);
        }
        let mut drained = Vec::new();
        state.drain_scancodes(&mut drained);
        assert_eq!(drained.len(), KB_IN_BYTES);
        assert_eq!(drained[0], 0);
        assert_eq!(drained[KB_IN_BYTES - 1], (KB_IN_BYTES - 1) as u16);
    }

    /// The LED mask strips the pseudo ALT_LOCK bit; the write-back is
    /// refused before the handshake exists.
    #[test]
    fn test_set_leds_mask_and_prehandshake_refusal() {
        let input = Endpoint(7);
        let mut state = KeyboardInput::new();
        assert!(state.set_leds_effect().is_none());
        handshake(&mut state, input);
        state.set_locks(lock_bits::NUM_LOCK | lock_bits::ALT_LOCK);
        match state.set_leds_effect() {
            Some(InputEffect::SendSetLeds(msg)) => {
                let mask =
                    minix_types::ipc::decode_setleds(&msg).expect("setleds payload decodes");
                assert_eq!(mask, lock_bits::NUM_LOCK);
            }
            _ => panic!("expected set_leds effect"),
        }
        assert_eq!(state.led_mask(), lock_bits::NUM_LOCK);
    }
}
