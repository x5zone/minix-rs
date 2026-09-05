//! Output buffer: the 2048-byte ring from slave to master reader.
//!
//! C correspondence: `ocount`, `ohead`, `otail`, `obuf[TTY_OUT_BYTES]` in
//! `pty_t` (`pty.c:46-56`), `pty_start` (`pty.c:621-665`), `pty_finish`
//! (`pty.c:667-681`), and the packet-mode prefix in `pty_start`
//! (`pty.c:633-645`).

/// Output ring capacity in bytes.
///
/// C: `TTY_OUT_BYTES 2048` (`pty/tty.h:13`).
pub const OUTPUT_RING: usize = 2048;

/// Suspended master read: who waits, how much arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuspendedRead {
    /// Waiting endpoint.
    pub caller: i64,
    /// Request identifier.
    pub id: u32,
    /// Bytes delivered so far.
    pub arrived: u64,
    /// Bytes still wanted.
    pub wanted: u64,
}

/// Ring holding slave output for the master reader.
///
/// C: the `obuf` ring with `pty_start` pumping it into the waiting read
/// and `pty_finish` answering the reader once at least one byte moved.
/// Copies stay in the service crate; this type moves counts: `feed` adds
/// slave bytes, `pump` serves the waiting read, `finish` reports the
/// answer. Packet mode prepends one zero byte before the first payload
/// byte of a read (minimal receipt-mode support, `pty.c:638-645`).
#[derive(Debug, Clone)]
pub struct OutputRing {
    cells: [u8; OUTPUT_RING],
    head: usize,
    tail: usize,
    count: usize,
    reader: Option<SuspendedRead>,
}

impl OutputRing {
    /// Empty ring, nobody waiting.
    pub const fn new() -> OutputRing {
        OutputRing {
            cells: [0; OUTPUT_RING],
            head: 0,
            tail: 0,
            count: 0,
            reader: None,
        }
    }

    /// Bytes currently buffered.
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when nothing is buffered.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Feed slave bytes; returns how many fit (rest is dropped).
    pub fn feed(&mut self, bytes: &[u8]) -> usize {
        let mut taken = 0;
        for byte in bytes {
            if self.count >= OUTPUT_RING {
                break;
            }
            self.cells[self.head] = *byte;
            self.head = (self.head + 1) % OUTPUT_RING;
            self.count += 1;
            taken += 1;
        }
        taken
    }

    /// Park a master read; fails when one is already parked.
    pub fn park_read(&mut self, caller: i64, id: u32, wanted: u64) -> bool {
        if self.reader.is_some() {
            return false;
        }
        self.reader = Some(SuspendedRead {
            caller,
            id,
            arrived: 0,
            wanted,
        });
        true
    }

    /// Serve the parked read from the ring; packet mode prepends the zero
    /// byte before the first payload byte.
    ///
    /// Returns bytes served this call (zero when nothing moved).
    pub fn pump(&mut self, packet_mode: bool) -> u64 {
        let Some(reader) = self.reader.as_mut() else {
            return 0;
        };
        let mut served = 0u64;
        if packet_mode && reader.arrived == 0 && reader.wanted > 0 {
            reader.arrived += 1;
            reader.wanted -= 1;
            served += 1;
        }
        while self.count > 0 && reader.wanted > 0 {
            self.tail = (self.tail + 1) % OUTPUT_RING;
            self.count -= 1;
            reader.arrived += 1;
            reader.wanted -= 1;
            served += 1;
        }
        served
    }

    /// Finish the parked read once at least one byte moved.
    ///
    /// C: `pty_finish` answers only when `rdcum > 0`. Returns the answer
    /// (caller, id, bytes) and clears the park; `None` means keep waiting.
    pub fn finish(&mut self) -> Option<(i64, u32, u64)> {
        let reader = self.reader?;
        if reader.arrived == 0 {
            return None;
        }
        self.reader = None;
        Some((reader.caller, reader.id, reader.arrived))
    }

    /// Cancel the parked read matching this caller and id; answers bytes
    /// so far, or "interrupted" when nothing moved.
    ///
    /// C: the read branch of `pty_master_cancel` (`pty.c:428-436`).
    pub fn cancel(&mut self, caller: i64, id: u32) -> Option<i64> {
        let reader = self.reader?;
        if reader.caller != caller || reader.id != id {
            return None;
        }
        self.reader = None;
        Some(if reader.arrived > 0 {
            reader.arrived as i64
        } else {
            -(eintr_code() as i64)
        })
    }
}

impl Default for OutputRing {
    fn default() -> Self {
        OutputRing::new()
    }
}

/// Interrupted code for a cancelled empty read.
const fn eintr_code() -> i32 {
    minix_types::EINTR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feed_and_pump_serve_in_order() {
        let mut ring = OutputRing::new();
        assert_eq!(ring.feed(&[1, 2, 3]), 3);
        assert!(ring.park_read(7, 1, 10));
        assert_eq!(ring.pump(false), 3);
        assert_eq!(ring.finish(), Some((7, 1, 3)));
        assert!(ring.is_empty());
    }

    #[test]
    fn test_finish_waits_for_first_byte() {
        let mut ring = OutputRing::new();
        assert!(ring.park_read(7, 1, 10));
        assert_eq!(ring.pump(false), 0);
        assert_eq!(ring.finish(), None);
    }

    #[test]
    fn test_packet_mode_prepends_zero_byte() {
        let mut ring = OutputRing::new();
        ring.feed(&[9]);
        ring.park_read(7, 1, 10);
        assert_eq!(ring.pump(true), 2);
        assert_eq!(ring.finish(), Some((7, 1, 2)));
    }

    #[test]
    fn test_second_park_is_refused() {
        let mut ring = OutputRing::new();
        assert!(ring.park_read(7, 1, 10));
        assert!(!ring.park_read(8, 2, 10));
    }

    #[test]
    fn test_cancel_answers_arrived_or_interrupted() {
        let mut ring = OutputRing::new();
        ring.feed(&[1]);
        ring.park_read(7, 1, 10);
        ring.pump(false);
        assert_eq!(ring.cancel(7, 1), Some(1));
        ring.park_read(7, 2, 10);
        assert_eq!(ring.cancel(7, 2), Some(-(minix_types::EINTR as i64)));
        assert_eq!(ring.cancel(7, 9), None);
    }

    #[test]
    fn test_ring_capacity_matches_c_header() {
        assert_eq!(OUTPUT_RING, 2048);
    }
}
