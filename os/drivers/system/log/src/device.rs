//! Log device: the single suspended reader, watchers, and cancel rules.
//!
//! C correspondence: `log_read`, `log_write`, `log_open`, `log_cancel`,
//! `log_select` (`log.c:244-360`), the single-minor rule (`NR_DEVS 1`,
//! `MINOR_KLOG 0`, `log.c:16-17`), the one-hanging-reader rule
//! (`log.c:256`), and the select bits (`CDEV_OP_RD/WR/ERR`, `CDEV_NOTIFY`,
//! `com.h:949-952`).

use super::ring::LogRing;
use minix_types::{EAGAIN, EBADF, EINTR, EIO, ENXIO, OK};

/// Endpoint value meaning "nobody waiting".
pub const NO_WAITER: i64 = -1;

/// Operation bit: read readiness.
pub const OP_READ: u32 = 0x01;
/// Operation bit: write readiness.
pub const OP_WRITE: u32 = 0x02;
/// Operation bit: error condition.
pub const OP_ERROR: u32 = 0x04;
/// Flag: the poller wants a later notification.
pub const WATCH_LATER: u32 = 0x08;
/// Mask of the three operation bits.
pub const OP_MASK: u32 = OP_READ | OP_WRITE | OP_ERROR;

/// The only minor device of this driver.
///
/// C: `MINOR_KLOG 0` (`log.c:17`): `/dev/klog`.
pub const KLOG_MINOR: u32 = 0;

/// What a read request decides: answer now or park.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadVerdict {
    /// Answer this many bytes now.
    Answer(u64),
    /// Park the reader (wake on the next write).
    Park,
}

/// What a cancel matched, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancelled {
    /// The parked read is dropped; the original answers "interrupted".
    Read,
}

/// One log device: ring plus the single suspended reader plus watchers.
///
/// C: `struct logdevice` (`log.h:14-26`) with `log_source NONE` meaning
/// nobody waits (`log.c:97`).
pub struct LogDevice {
    ring: LogRing,
    waiter: Option<Waiter>,
    watched: u32,
    watch_caller: i64,
}

/// One parked reader: who waits and under which identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Waiter {
    caller: i64,
    id: u32,
    want: u64,
}

impl LogDevice {
    /// Fresh device: empty ring, nobody waiting, nothing watched.
    ///
    /// C: `sef_cb_init_fresh` zeroes the device and clears the waiter
    /// (`log.c:93-98`).
    pub fn new() -> LogDevice {
        LogDevice {
            ring: LogRing::new(),
            waiter: None,
            watched: 0,
            watch_caller: NO_WAITER,
        }
    }

    /// Borrow the ring (writes and diagnostics feed it).
    pub fn ring(&self) -> &LogRing {
        &self.ring
    }

    /// Mutably borrow the ring.
    pub fn ring_mut(&mut self) -> &mut LogRing {
        &mut self.ring
    }

    /// Open the single minor; anything else is "no such device".
    ///
    /// C: `log_open` (`log.c:295-301`).
    pub const fn open(minor: u32) -> i32 {
        if minor != KLOG_MINOR {
            return -ENXIO;
        }
        OK
    }

    /// Read: refuse unknown minors; refuse a second waiter while one
    /// hangs; answer at once when data waits; park (or "try again" for
    /// non-blocking) when empty.
    ///
    /// C: `log_read` (`log.c:244-273`). A second waiter while one hangs is
    /// answered success-zero (not an error): the C code returns `OK`
    /// without parking, and the service crate turns that into an empty
    /// answer. [`ReadVerdict::Answer`] carries that zero.
    pub fn read(
        &mut self,
        minor: u32,
        want: u64,
        caller: i64,
        id: u32,
        nonblocking: bool,
    ) -> Result<ReadVerdict, i32> {
        if minor != KLOG_MINOR {
            return Err(-EIO);
        }
        if self.waiter.is_some() {
            return Ok(ReadVerdict::Answer(0));
        }
        if self.ring.is_empty() && want > 0 {
            if nonblocking {
                return Err(-EAGAIN);
            }
            self.waiter = Some(Waiter { caller, id, want });
            return Ok(ReadVerdict::Park);
        }
        Ok(ReadVerdict::Answer(
            self.ring.note_read(want as usize) as u64
        ))
    }

    /// Take the parked reader, if any (wake after a write).
    pub fn take_waiter(&mut self) -> Option<(i64, u32, u64)> {
        self.waiter
            .take()
            .map(|waiter| (waiter.caller, waiter.id, waiter.want))
    }

    /// Cancel the parked read matching this caller and identifier.
    ///
    /// C: `log_cancel` (`log.c:306-318`): a mismatch is "let it finish"
    /// (no reply to the cancel); a match clears the waiter and the
    /// original answers "interrupted".
    pub fn cancel(&mut self, minor: u32, caller: i64, id: u32) -> Result<Option<Cancelled>, i32> {
        if minor != KLOG_MINOR {
            return Err(-einval_code());
        }
        match self.waiter {
            Some(waiter) if waiter.caller == caller && waiter.id == id => {
                self.waiter = None;
                Ok(Some(Cancelled::Read))
            }
            _ => Ok(None),
        }
    }

    /// Poll readiness: reads are ready while data waits, writes never
    /// block; register the remainder when a later notice is wanted.
    ///
    /// C: `log_select` (`log.c:323-360`).
    pub fn select(&mut self, minor: u32, ops: u32, caller: i64) -> Result<u32, i32> {
        if minor != KLOG_MINOR {
            return Err(-ENXIO);
        }
        let mut ready = 0;
        let mut want = ops & OP_MASK;
        if want & OP_READ != 0 && !self.ring.is_empty() {
            ready |= OP_READ;
        }
        if want & OP_WRITE != 0 {
            ready |= OP_WRITE;
        }
        want &= !ready;
        if ops & WATCH_LATER != 0 && want != 0 {
            self.watched |= want;
            self.watch_caller = caller;
        }
        Ok(ready)
    }

    /// Watch bits currently registered.
    pub fn watched(&self) -> u32 {
        self.watched
    }

    /// Who to notify for watches.
    pub fn watch_caller(&self) -> i64 {
        self.watch_caller
    }

    /// Clear watch bits (after the late notice goes out).
    pub fn clear_watched(&mut self, bits: u32) {
        self.watched &= !bits;
    }
}

impl Default for LogDevice {
    fn default() -> Self {
        LogDevice::new()
    }
}

/// Invalid-argument code for bad cancel minors.
const fn einval_code() -> i32 {
    minix_types::EINVAL
}

/// Success marker.
pub const SUCCESS: i32 = OK;
/// Original-read answer after a matched cancel.
pub const READ_INTERRUPTED: i32 = -EINTR;
/// Bad-file code for misuse (kept for call sites).
pub const BAD_USE: i32 = -EBADF;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_accepts_only_klog() {
        assert_eq!(LogDevice::open(0), OK);
        assert_eq!(LogDevice::open(1), -ENXIO);
    }

    #[test]
    fn test_read_answers_at_once_when_data_waits() {
        let mut device = LogDevice::new();
        device.ring_mut().note_written(50);
        assert_eq!(
            device.read(0, 100, 7, 1, false),
            Ok(ReadVerdict::Answer(50))
        );
    }

    #[test]
    fn test_empty_read_parks_or_refuses() {
        let mut device = LogDevice::new();
        assert_eq!(device.read(0, 100, 7, 1, false), Ok(ReadVerdict::Park));
        assert_eq!(device.read(0, 100, 7, 2, false), Ok(ReadVerdict::Answer(0)));
        let mut fresh = LogDevice::new();
        assert_eq!(fresh.read(0, 100, 7, 1, true), Err(-EAGAIN));
    }

    #[test]
    fn test_wake_and_cancel_pair() {
        let mut device = LogDevice::new();
        device.read(0, 100, 7, 1, false).unwrap();
        assert_eq!(device.take_waiter(), Some((7, 1, 100)));
        device.read(0, 100, 7, 1, false).unwrap();
        assert_eq!(device.cancel(0, 7, 1), Ok(Some(Cancelled::Read)));
        assert_eq!(device.cancel(0, 7, 9), Ok(None));
        assert!(device.cancel(1, 7, 1).is_err());
    }

    #[test]
    fn test_select_read_needs_data_write_never_blocks() {
        let mut device = LogDevice::new();
        assert_eq!(device.select(0, OP_READ | OP_WRITE, 9), Ok(OP_WRITE));
        device.ring_mut().note_written(5);
        assert_eq!(
            device.select(0, OP_READ | OP_WRITE, 9),
            Ok(OP_READ | OP_WRITE)
        );
        assert_eq!(device.select(1, OP_READ, 9), Err(-ENXIO));
    }

    #[test]
    fn test_watch_registers_remainder() {
        let mut device = LogDevice::new();
        let ready = device.select(0, OP_READ | WATCH_LATER, 9).unwrap();
        assert_eq!(ready, 0);
        assert_eq!(device.watched(), OP_READ);
        assert_eq!(device.watch_caller(), 9);
        device.clear_watched(OP_READ);
        assert_eq!(device.watched(), 0);
    }
}
