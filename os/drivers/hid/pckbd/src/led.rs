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

/// Input-server mask bit position for Num lock.
///
/// C: the server builds its mask as `1 << code` with the `INPUT_LED_*`
/// codes (`input.h:293-295`) — Num is code 1, so mask bit `0x2`. Positions
/// 0/1/2 would shift every light by one and make Scroll unreachable.
pub const LOCK_NUM: u32 = 1;
/// Caps-lock bit position (code 2, mask bit `0x4`).
pub const LOCK_CAPS: u32 = 2;
/// Scroll-lock bit position (code 3, mask bit `0x8`).
pub const LOCK_SCROLL: u32 = 3;

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

/// Controller status bit: last read timed out.
///
/// C: literal `0x40` in the ack branch (`pckbd.c:129`) — an ack read with
/// this bit set is discarded.
pub const STATUS_TIMEOUT: u8 = 0x40;

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
    /// C: the ack branch of `scan_keyboard` (`pckbd.c:129-134`). The
    /// acknowledgment only counts when the status byte carries no timeout
    /// bit (`0x40`): a timed-out read's data is stale controller noise,
    /// not a real ack.
    pub fn note_ack(&mut self, status: u8, byte: u8) -> bool {
        if status & STATUS_TIMEOUT == 0 && byte == ACK_BYTE && self.expect_ack {
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
        // Wire values spelled literally: the server mask uses `1 << code`
        // with codes 1/2/3 (input.h:293-295), the keyboard byte uses the
        // pckbd.h LED bits (scroll 0x01, num 0x02, caps 0x04).
        assert_eq!(translate_leds(0), 0);
        assert_eq!(translate_leds(0x2), MASK_NUM);
        assert_eq!(translate_leds(0x4), MASK_CAPS);
        assert_eq!(translate_leds(0x8), MASK_SCROLL);
        assert_eq!(
            translate_leds(0x2 | 0x4 | 0x8),
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
        assert!(outbox.note_ack(0, ACK_BYTE));
        assert!(!outbox.expects_ack());
        let second = outbox.take().unwrap();
        assert_eq!(second, MASK_CAPS);
    }

    #[test]
    fn test_ack_with_timeout_status_is_discarded() {
        // C checks `!(sb & 0x40)` before trusting the ack (pckbd.c:129):
        // a timed-out read's byte is stale noise, and the outstanding
        // command stays pending for the resend logic.
        let mut outbox = LedOutbox::new();
        outbox.queue(1 << LOCK_NUM);
        assert!(outbox.take().is_some());
        assert!(!outbox.note_ack(STATUS_TIMEOUT, ACK_BYTE));
        assert!(outbox.expects_ack());
        assert!(outbox.note_ack(0, ACK_BYTE));
        assert!(!outbox.expects_ack());
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
