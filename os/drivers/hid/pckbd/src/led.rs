//! LED outbox: two-byte commands queued for the keyboard.
//!
//! C correspondence: `kbdout` with `offset`, `avail`, `expect_ack`
//! (`pckbd.c:12-17`), `set_leds` (`pckbd.c:175-192`), and the LED mapping
//! in `pckbd_leds` (`pckbd.c:418-428`): command byte `LED_CODE 0xED`
//! plus the mask built from the three lock bits (`pckbd.h:20`).
//!
//! Port writes stay in the service crate; this outbox owns the queue
//! policy (two bytes per command, drop on overflow with the ack flag
//! cleared).

/// Command byte asking the keyboard to set LEDs.
///
/// C: `LED_CODE 0xED` (`pckbd.h:20`).
pub const LED_COMMAND: u8 = 0xED;

/// Output queue capacity in bytes.
///
/// C: `KBD_OUT_BUFSZ 16` (`pckbd.h:27`).
pub const OUTBOX_SIZE: usize = 16;

/// Lock-bit positions in the input-server mask.
pub const LOCK_NUM: u32 = 0;
/// Caps-lock bit position.
pub const LOCK_CAPS: u32 = 1;
/// Scroll-lock bit position.
pub const LOCK_SCROLL: u32 = 2;

/// Keyboard mask bits for the three locks.
pub const MASK_NUM: u8 = 0x02;
/// Caps-lock mask bit.
pub const MASK_CAPS: u8 = 0x04;
/// Scroll-lock mask bit.
pub const MASK_SCROLL: u8 = 0x01;

/// Translate an input-server LED mask into a keyboard mask.
///
/// C: `pckbd_leds` (`pckbd.c:418-428`).
pub const fn translate_leds(mask: u32) -> u8 {
    let mut out = 0;
    if mask & (1 << LOCK_NUM) != 0 {
        out |= MASK_NUM;
    }
    if mask & (1 << LOCK_CAPS) != 0 {
        out |= MASK_CAPS;
    }
    if mask & (1 << LOCK_SCROLL) != 0 {
        out |= MASK_SCROLL;
    }
    out
}

/// Acknowledgment byte from the keyboard.
///
/// C: `KB_ACK 0xFA` (`pckbd.h:10`).
pub const ACK_BYTE: u8 = 0xFA;

/// Queued LED commands: offset, available count, ack expectation.
///
/// C: `kbdout` (`pckbd.c:12-17`). `set_leds` appends the two bytes when
/// they fit and clears the ack flag when they do not (`pckbd.c:179-189`).
#[derive(Debug, Clone)]
pub struct LedOutbox {
    buffer: [u8; OUTBOX_SIZE],
    offset: usize,
    available: usize,
    expect_ack: bool,
}

impl LedOutbox {
    /// Empty outbox.
    pub const fn new() -> LedOutbox {
        LedOutbox {
            buffer: [0; OUTBOX_SIZE],
            offset: 0,
            available: 0,
            expect_ack: false,
        }
    }

    /// Queued bytes waiting to send.
    pub fn pending(&self) -> usize {
        self.available
    }

    /// True while waiting for the keyboard's acknowledgment.
    pub const fn expects_ack(&self) -> bool {
        self.expect_ack
    }

    /// Queue one LED command from the input-server mask.
    ///
    /// C: `set_leds` (`pckbd.c:175-192`): restart at zero when empty,
    /// drop with the ack flag cleared when the two bytes do not fit.
    pub fn queue(&mut self, server_mask: u32) {
        if self.available == 0 {
            self.offset = 0;
        }
        if self.offset + self.available + 2 > OUTBOX_SIZE {
            self.expect_ack = false;
        } else {
            self.buffer[self.offset + self.available] = LED_COMMAND;
            self.buffer[self.offset + self.available + 1] = translate_leds(server_mask);
            self.available += 2;
        }
    }

    /// Take the next byte to send, if any and no ack is outstanding.
    pub fn take(&mut self) -> Option<u8> {
        if self.available == 0 || self.expect_ack {
            return None;
        }
        let byte = self.buffer[self.offset];
        self.offset += 1;
        self.available -= 1;
        self.expect_ack = true;
        Some(byte)
    }

    /// Note the keyboard's acknowledgment of the outstanding byte.
    ///
    /// C: the ack branch of `scan_keyboard` (`pckbd.c:129-134`).
    pub fn note_ack(&mut self, byte: u8) -> bool {
        if byte == ACK_BYTE && self.expect_ack {
            self.expect_ack = false;
            true
        } else {
            false
        }
    }
}

impl Default for LedOutbox {
    fn default() -> Self {
        LedOutbox::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_translation_matches_c_mapping() {
        assert_eq!(translate_leds(0), 0);
        assert_eq!(translate_leds(1 << LOCK_NUM), MASK_NUM);
        assert_eq!(
            translate_leds((1 << LOCK_NUM) | (1 << LOCK_CAPS) | (1 << LOCK_SCROLL)),
            MASK_NUM | MASK_CAPS | MASK_SCROLL
        );
    }

    #[test]
    fn test_queue_take_ack_cycle() {
        let mut outbox = LedOutbox::new();
        outbox.queue(1 << LOCK_CAPS);
        assert_eq!(outbox.pending(), 2);
        let first = outbox.take().unwrap();
        assert_eq!(first, LED_COMMAND);
        assert!(outbox.expects_ack());
        assert_eq!(outbox.take(), None);
        assert!(outbox.note_ack(ACK_BYTE));
        assert!(!outbox.expects_ack());
        let second = outbox.take().unwrap();
        assert_eq!(second, MASK_CAPS);
    }

    #[test]
    fn test_overflow_drops_and_clears_ack() {
        let mut outbox = LedOutbox::new();
        for _ in 0..8 {
            outbox.queue(0);
        }
        assert_eq!(outbox.pending(), OUTBOX_SIZE);
        outbox.queue(0);
        assert_eq!(outbox.pending(), OUTBOX_SIZE);
        assert!(!outbox.expects_ack());
        assert_eq!(LED_COMMAND, 0xED);
    }
}
