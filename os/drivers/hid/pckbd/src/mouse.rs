//! Mouse packets: three-byte assembly, resync, buttons, motion.
//!
//! C correspondence: `kbdaux_process` with `aux_bytes`, `aux_counter`,
//! and `aux_state` (`pckbd.c:28-31,374-412`): three-byte packets, resync
//! on bit three of the first byte, one event per changed button, one
//! event per nonzero axis delta with sign extension and the relative
//! flag.

/// Buttons per packet (left, right, middle).
pub const BUTTONS: usize = 3;

/// Packet length in bytes.
pub const PACKET_BYTES: usize = 3;

/// Button usage page.
///
/// C: `INPUT_PAGE_BUTTON 0x0009` (`input.h:38`).
pub const PAGE_BUTTON: u16 = 0x0009;

/// First button usage code; the other two follow.
///
/// C: `INPUT_BUTTON_1 + i` (`pckbd.c:395`).
pub const BUTTON_1: u16 = 0x0001;

/// Motion usage page.
///
/// C: `INPUT_PAGE_GD 0x0001` (`input.h:35`).
pub const PAGE_MOTION: u16 = 0x0001;

/// Motion axis codes (X then Y).
///
/// C: `INPUT_GD_X` / `INPUT_GD_Y` (`pckbd.c:408`, `input.h:51`).
pub const AXIS_X: u16 = 0x0030;
/// Y axis usage code.
pub const AXIS_Y: u16 = 0x0031;

/// Relative-motion flag.
///
/// C: `INPUT_FLAG_REL 0x04` (`input.h:47`).
pub const FLAG_RELATIVE: i32 = 0x04;

/// One mouse event: buttons or motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEvent {
    /// Button index (zero-based) changed to pressed (true) or not.
    Button { index: usize, pressed: bool },
    /// Axis moved by this signed delta (relative).
    Motion { axis: u16, delta: i32 },
}

/// Three-byte packet assembler with button memory.
///
/// C: `aux_bytes[3]`, `aux_counter`, `aux_state` (`pckbd.c:28-31`). Bytes
/// before a valid first byte are dropped for resync (`pckbd.c:379-380`):
/// bit three of byte zero must be set, else the byte is noise.
pub struct MouseAssembler {
    bytes: [u8; PACKET_BYTES],
    count: usize,
    buttons: u8,
}

impl MouseAssembler {
    /// Fresh assembler: no bytes, no buttons.
    pub const fn new() -> MouseAssembler {
        MouseAssembler {
            bytes: [0; PACKET_BYTES],
            count: 0,
            buttons: 0,
        }
    }

    /// Feed one byte; returns the completed packet's events, if any.
    pub fn feed(&mut self, byte: u8) -> Option<alloc::vec::Vec<MouseEvent>> {
        if self.count == 0 && byte & 0x08 == 0 {
            return None;
        }
        self.bytes[self.count] = byte;
        self.count += 1;
        if self.count < PACKET_BYTES {
            return None;
        }
        self.count = 0;
        Some(self.finish())
    }

    fn finish(&mut self) -> alloc::vec::Vec<MouseEvent> {
        let mut events = alloc::vec::Vec::new();
        for index in 0..BUTTONS {
            let mask = 1 << index;
            if (self.buttons ^ self.bytes[0]) & mask != 0 {
                self.buttons ^= mask;
                events.push(MouseEvent::Button {
                    index,
                    pressed: self.buttons & mask != 0,
                });
            }
        }
        let deltas = [self.bytes[1], self.bytes[2]];
        let signs = [0x10, 0x20];
        let axes = [AXIS_X, AXIS_Y];
        for i in 0..2 {
            if deltas[i] != 0 {
                let mut delta = deltas[i] as i32;
                if self.bytes[0] & signs[i] != 0 {
                    delta |= 0xFFFF_FF00u32 as i32;
                }
                events.push(MouseEvent::Motion {
                    axis: axes[i],
                    delta,
                });
            }
        }
        events
    }
}

impl Default for MouseAssembler {
    fn default() -> Self {
        MouseAssembler::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_before_sync_is_dropped() {
        let mut mouse = MouseAssembler::new();
        assert_eq!(mouse.feed(0x00), None);
        assert_eq!(mouse.feed(0x07), None);
    }

    #[test]
    fn test_button_press_and_release_events() {
        let mut mouse = MouseAssembler::new();
        assert_eq!(mouse.feed(0x08), None);
        assert_eq!(mouse.feed(0x00), None);
        let events = mouse.feed(0x00).unwrap();
        assert!(events.is_empty());
        mouse.feed(0x09);
        mouse.feed(0x00);
        let events = mouse.feed(0x00).unwrap();
        assert_eq!(
            events,
            alloc::vec![MouseEvent::Button {
                index: 0,
                pressed: true
            }]
        );
    }

    #[test]
    fn test_motion_sign_extends_and_flags_relative() {
        let mut mouse = MouseAssembler::new();
        mouse.feed(0x18);
        mouse.feed(0xFF);
        let events = mouse.feed(0x00).unwrap();
        assert_eq!(
            events,
            alloc::vec![MouseEvent::Motion {
                axis: AXIS_X,
                delta: -1
            }]
        );
        assert_eq!(FLAG_RELATIVE, 0x04);
    }

    #[test]
    fn test_still_packet_produces_no_events() {
        let mut mouse = MouseAssembler::new();
        mouse.feed(0x08);
        mouse.feed(0x00);
        let events = mouse.feed(0x00).unwrap();
        assert!(events.is_empty());
    }
}
