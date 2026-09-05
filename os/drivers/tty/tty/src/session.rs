//! Open sessions: counts, control, suspended calls, and readiness.
//!
//! C correspondence: the open-count and controlling-endpoint handling in
//! `do_open` and `do_close` (`tty.c:721-776`), the three-way cancel in
//! `do_cancel` (`tty.c:778-814`), the readiness probe `select_try` plus
//! the retry `select_retry` (`tty.c:816-863`), the select registration in
//! `do_select` (`tty.c:865-899`), and the hangup plus canonical-newline
//! rules inside `select_try`.

use super::line::LineId;
use super::termios::LineConfig;
use minix_types::{EAGAIN, EBADF, EINTR, EINVAL, ENXIO, OK};

/// Endpoint value meaning "nobody" (no suspended caller).
///
/// C: `NONE` as used for `tty_incaller` and friends.
pub const NO_CALLER: i64 = -1;

/// Watch flag: the poller wants a later notification.
///
/// C: `CDEV_NOTIFY 0x08` (`com.h:952`).
pub const WATCH_LATER: u32 = 0x08;

/// Operation bit: read readiness.
///
/// C: `CDEV_OP_RD 0x01` (`com.h:949`).
pub const OP_READ: u32 = 0x01;

/// Operation bit: write readiness.
///
/// C: `CDEV_OP_WR 0x02` (`com.h:950`).
pub const OP_WRITE: u32 = 0x02;

/// Operation bit: error condition.
///
/// C: `CDEV_OP_ERR 0x04` (`com.h:951`).
pub const OP_ERROR: u32 = 0x04;

/// Mask of the three operation bits.
pub const OP_MASK: u32 = OP_READ | OP_WRITE | OP_ERROR;

/// Open access bit: read access requested.
///
/// C: `CDEV_R_BIT 0x01` (`com.h:940`).
pub const ACCESS_READ: i32 = 0x01;

/// Open access bit: the line must not become the controlling terminal.
///
/// C: `CDEV_NOCTTY 0x04` (`com.h:942`).
pub const ACCESS_NOCTTY: i32 = 0x04;

/// Controlling-terminal marker returned by a successful open.
///
/// C: `CDEV_CTTY` (`com.h:956`): the line became the caller's controlling
/// terminal.
pub const CONTROLLED: i32 = 0x4000_0000;

/// Which suspended call a cancel matched, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancelled {
    /// A suspended read: answer bytes so far, or "try again".
    Read(i32),
    /// A suspended write: answer bytes so far, or "try again".
    Write(i32),
    /// A suspended drain wait: answer "interrupted".
    Drain,
}

/// One terminal line's open session: counts, control, and suspended calls.
///
/// C: the per-line fields `tty_openct`, `tty_pgrp`, the in/out/io caller
/// triplets, and the select registration (`tty.h`). Unknown lines never
/// reach this type: [`LineId::decode`] filters them first, and every
/// method re-checks through the stored line.
pub struct TtySession {
    line: LineId,
    opens: u32,
    controlling: i64,
    config: LineConfig,
    reader: Option<Suspended>,
    writer: Option<Suspended>,
    drain: Option<Suspended>,
    watcher: Option<Watcher>,
    queue_len: usize,
    queue_breaks: usize,
    output_pending: bool,
    device_writable: bool,
}

/// One suspended call: who waits, under which identifier, how much arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Suspended {
    caller: i64,
    id: u32,
    arrived: u64,
}

/// One select watch: which operations, who to tell, under which minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Watcher {
    ops: u32,
    caller: i64,
    minor: u32,
}

impl TtySession {
    /// Fresh session on this line with default configuration.
    pub fn new(line: LineId) -> TtySession {
        TtySession {
            line,
            opens: 0,
            controlling: 0,
            config: LineConfig::defaults(),
            reader: None,
            writer: None,
            drain: None,
            watcher: None,
            queue_len: 0,
            queue_breaks: 0,
            output_pending: false,
            device_writable: true,
        }
    }

    /// Line served by this session.
    pub const fn line(self) -> LineId {
        self.line
    }

    /// Open the line: refuse the log alias for reading, adopt control
    /// unless forbidden, count up, notify the device on first open.
    ///
    /// C: `do_open` (`tty.c:721-752`). Returns the controlling-terminal
    /// marker when the line becomes the caller's controlling terminal.
    /// `device_opened` reports whether the device-level open hook must run
    /// (first open of a non-log line).
    pub fn open(&mut self, raw_minor: u32, access: i32, caller: i64) -> (i32, bool) {
        if LineId::is_log_alias(raw_minor) && access & ACCESS_READ != 0 {
            return (-eacces_code(), false);
        }
        let mut result = OK;
        if access & ACCESS_NOCTTY == 0 {
            self.controlling = caller;
            result = CONTROLLED;
        }
        self.opens += 1;
        (result, self.opens == 1)
    }

    /// Close the line: last close clears control, cancels calls, resets
    /// configuration to defaults.
    ///
    /// C: `do_close` (`tty.c:754-776`).
    pub fn close(&mut self) -> bool {
        if self.opens == 0 {
            return false;
        }
        self.opens -= 1;
        if self.opens == 0 {
            self.controlling = 0;
            self.reader = None;
            self.writer = None;
            self.drain = None;
            self.config = LineConfig::defaults();
            return true;
        }
        false
    }

    /// Open count.
    pub fn open_count(&self) -> u32 {
        self.opens
    }

    /// Suspend a read: fails when one is already parked or the size is
    /// not positive.
    ///
    /// C: the head of `do_read` (`tty.c:481-489`): a second parked read is
    /// "input-output error", a non-positive size is "invalid argument".
    pub fn park_read(&mut self, caller: i64, id: u32, size: u64) -> Result<(), i32> {
        if self.reader.is_some() {
            return Err(-eio_code());
        }
        if size == 0 {
            return Err(-EINVAL);
        }
        self.reader = Some(Suspended {
            caller,
            id,
            arrived: 0,
        });
        Ok(())
    }

    /// Note arrived bytes on the parked read.
    pub fn read_arrived(&mut self, count: u64) {
        if let Some(reader) = self.reader.as_mut() {
            reader.arrived += count;
        }
    }

    /// Suspend a write (same double-park and size rules as reads).
    pub fn park_write(&mut self, caller: i64, id: u32, size: u64) -> Result<(), i32> {
        if self.writer.is_some() {
            return Err(-eio_code());
        }
        if size == 0 {
            return Err(-EINVAL);
        }
        self.writer = Some(Suspended {
            caller,
            id,
            arrived: 0,
        });
        Ok(())
    }

    /// Cancel the suspended call matching this caller and identifier.
    ///
    /// C: `do_cancel` (`tty.c:778-814`) checks read, write, then drain in
    /// order and wakes the event pump (`tty_events = 1`) on a match. The
    /// wake itself stays in the service crate; the match is returned.
    pub fn cancel(&mut self, caller: i64, id: u32) -> Option<Cancelled> {
        if let Some(reader) = self.reader
            && reader.caller == caller
            && reader.id == id
        {
            self.reader = None;
            let answer = if reader.arrived > 0 {
                reader.arrived as i32
            } else {
                -EAGAIN
            };
            return Some(Cancelled::Read(answer));
        }
        if let Some(writer) = self.writer
            && writer.caller == caller
            && writer.id == id
        {
            self.writer = None;
            let answer = if writer.arrived > 0 {
                writer.arrived as i32
            } else {
                -EAGAIN
            };
            return Some(Cancelled::Write(answer));
        }
        if let Some(drain) = self.drain
            && drain.caller == caller
            && drain.id == id
        {
            self.drain = None;
            return Some(Cancelled::Drain);
        }
        None
    }

    /// Feed queue and output facts for readiness probes.
    pub fn note_input(&mut self, queued: usize, breaks: usize) {
        self.queue_len = queued;
        self.queue_breaks = breaks;
    }

    /// Note whether output is still draining and the device can take more.
    pub fn note_output(&mut self, pending: bool, writable: bool) {
        self.output_pending = pending;
        self.device_writable = writable;
    }

    /// Probe readiness now (never parks).
    ///
    /// C: `select_try` (`tty.c:816-848`): hung-up lines report everything
    /// ready; reads are ready when a call is parked (it cannot block) or
    /// when queued input plus the canonical-newline rule allow; writes
    /// are ready when output drains or the device takes more.
    pub fn probe(&self, ops: u32) -> u32 {
        let mut ready = 0;
        if self.config.is_hung_up() {
            return ops;
        }
        if ops & OP_READ != 0
            && (self.reader.is_some()
                || (self.queue_len > 0 && (!self.config.flags.canonical || self.queue_breaks > 0)))
        {
            ready |= OP_READ;
        }
        if ops & OP_WRITE != 0 && (self.output_pending || self.device_writable) {
            ready |= OP_WRITE;
        }
        ready
    }

    /// Register a watch for the not-yet-ready remainder.
    ///
    /// C: `do_select` (`tty.c:865-899`): watching the same object through
    /// two different minors is refused ("bad file"); otherwise the
    /// remainder accumulates. Returns the immediately ready subset.
    pub fn select(&mut self, raw_minor: u32, ops: u32, caller: i64) -> Result<u32, i32> {
        let mut ops = ops & OP_MASK;
        let ready = self.probe(ops);
        ops &= !ready;
        if ops != 0 {
            if let Some(watcher) = self.watcher
                && watcher.ops != 0
                && watcher.minor != raw_minor
            {
                return Err(-EBADF);
            }
            let combined = self.watcher.map(|watcher| watcher.ops).unwrap_or(0) | ops;
            self.watcher = Some(Watcher {
                ops: combined,
                caller,
                minor: raw_minor,
            });
        }
        Ok(ready)
    }

    /// Re-probe a registered watch; returns the newly ready subset and
    /// clears those bits (the service crate sends the late reply).
    ///
    /// C: `select_retry` (`tty.c:850-863`).
    pub fn retry_watch(&mut self) -> Option<(i64, u32, u32)> {
        let watcher = self.watcher?;
        let ready = self.probe(watcher.ops);
        if ready == 0 {
            return None;
        }
        let remaining = watcher.ops & !ready;
        if remaining == 0 {
            self.watcher = None;
        } else {
            self.watcher = Some(Watcher {
                ops: remaining,
                ..watcher
            });
        }
        Some((watcher.caller, watcher.minor, ready))
    }
}

/// Access error for log-alias reads.
const fn eacces_code() -> i32 {
    minix_types::EACCES
}

/// Input-output error for double-parked calls.
const fn eio_code() -> i32 {
    minix_types::EIO
}

/// Success marker.
pub const SUCCESS: i32 = OK;

/// Reply for a cancelled drain wait.
pub const DRAIN_INTERRUPTED: i32 = -EINTR;

/// Reply when no suspended call matched a cancel.
pub const CANCEL_STALE: i32 = -minix_types::EDONTREPLY;

/// Error for an unknown line (decode failure at the boundary).
pub const NO_SUCH_LINE: i32 = -ENXIO;

#[cfg(test)]
mod tests {
    use super::super::line::{LOG_MINOR, LineId};
    use super::*;

    fn session() -> TtySession {
        TtySession::new(LineId::Console(0))
    }

    #[test]
    fn test_open_adopts_control_and_counts() {
        let mut session = session();
        let (result, first) = session.open(0, 0, 42);
        assert_eq!(result, CONTROLLED);
        assert!(first);
        assert_eq!(session.open_count(), 1);
        let (second, first) = session.open(0, ACCESS_NOCTTY, 43);
        assert_eq!(second, OK);
        assert!(!first);
    }

    #[test]
    fn test_log_alias_refuses_read_access() {
        let mut session = session();
        let (result, _) = session.open(LOG_MINOR, ACCESS_READ, 42);
        assert!(result < 0);
        let (result, _) = session.open(LOG_MINOR, 0, 42);
        assert_eq!(result, CONTROLLED);
    }

    #[test]
    fn test_last_close_resets_configuration() {
        let mut session = session();
        session.open(0, 0, 42);
        assert!(session.close());
        assert_eq!(session.open_count(), 0);
        assert!(!session.close());
    }

    #[test]
    fn test_double_parked_read_is_refused() {
        let mut session = session();
        assert!(session.park_read(7, 1, 100).is_ok());
        assert!(session.park_read(7, 2, 100).is_err());
        assert!(session.park_write(7, 1, 0).is_err());
    }

    #[test]
    fn test_cancel_matches_in_order_read_write_drain() {
        let mut session = session();
        session.park_read(7, 1, 100).unwrap();
        session.read_arrived(30);
        assert_eq!(session.cancel(7, 1), Some(Cancelled::Read(30)));
        session.park_write(7, 2, 100).unwrap();
        assert_eq!(session.cancel(7, 2), Some(Cancelled::Write(-EAGAIN)));
        assert_eq!(session.cancel(7, 9), None);
    }

    #[test]
    fn test_probe_honors_hangup_and_canonical_rule() {
        let mut session = session();
        session.note_input(5, 0);
        assert_eq!(session.probe(OP_READ), 0);
        session.note_input(5, 1);
        assert_eq!(session.probe(OP_READ), OP_READ);
        session.config.output_speed = 0;
        assert_eq!(session.probe(OP_READ | OP_WRITE), OP_READ | OP_WRITE);
    }

    #[test]
    fn test_select_registers_remainder_and_retries() {
        let mut session = session();
        session.note_output(false, true);
        let ready = session.select(0, OP_READ | OP_WRITE, 9).unwrap();
        assert_eq!(ready, OP_WRITE);
        session.note_input(3, 1);
        let fired = session.retry_watch().unwrap();
        assert_eq!(fired, (9, 0, OP_READ));
        assert!(session.retry_watch().is_none());
    }

    #[test]
    fn test_select_on_two_minors_is_refused() {
        let mut session = session();
        session.note_output(false, false);
        session.select(0, OP_READ, 9).unwrap();
        assert_eq!(session.select(1, OP_READ, 9), Err(-EBADF));
    }

    #[test]
    fn test_constants_match_c_headers() {
        assert_eq!(OP_READ, 0x01);
        assert_eq!(OP_WRITE, 0x02);
        assert_eq!(WATCH_LATER, 0x08);
        assert_eq!(DRAIN_INTERRUPTED, -EINTR);
    }
}
