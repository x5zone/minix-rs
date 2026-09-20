//! Chardriver wiring: the tty driver as a chardriver device.
//!
//! C correspondence: the eight-callback tty table (`tty.c`, `tty_tab`:
//! open/close/read/write/ioctl/cancel/select) over the line table
//! (`line2tty`, `tty.c:264-284`). One session per line; minors decode
//! through [`LineId::decode`] with the configured console line.
//!
//! Data-movement split: read/write hooks answer counts and park or wake
//! through the session; the byte copies between the caller's grant and
//! the input queue (or the device backend) are the service's job. This
//! keeps the hook half testable with no transport at all.

use crate::backend::LineBackend;
use crate::line::LineId;
use crate::session::TtySession;
use minix_chardriver::driver::CharDriver;
use minix_chardriver::protocol::{DeviceMinor, OpenDeviceSet, RequestId};
use minix_types::Errno;

/// The tty driver's chardriver face over a set of per-line sessions.
pub struct TtyDriver<B: LineBackend> {
    /// One session per line-table slot (consoles then serials).
    pub sessions: alloc::vec::Vec<TtySession>,
    /// Configured console line the aliases redirect to.
    pub console_line: u32,
    /// The device backend (console renderer, serial ports).
    pub backend: B,
    /// Opened minors since the last announce (the restart gate).
    pub opened: OpenDeviceSet,
}

impl<B: LineBackend> TtyDriver<B> {
    /// A driver over the given number of line slots.
    pub fn new(line_slots: usize, console_line: u32, backend: B) -> Self {
        let mut sessions = alloc::vec::Vec::new();
        for index in 0..line_slots {
            let line = if index < 4 {
                LineId::Console(index as u32)
            } else {
                LineId::Serial((index - 4) as u32)
            };
            sessions.push(TtySession::new(line));
        }
        TtyDriver {
            sessions,
            console_line,
            backend,
            opened: OpenDeviceSet::new(),
        }
    }

    /// Resolve a raw minor to its session slot.
    fn slot(&self, minor: u32) -> Option<usize> {
        match LineId::decode(minor, self.console_line)? {
            LineId::Console(index) => Some(index as usize),
            LineId::Serial(index) => Some(4 + index as usize),
            LineId::Video => None,
        }
    }

    /// The write byte lane: actual bytes from the writer's grant reach
    /// the device here (NL1 批B pipeline).
    ///
    /// C: `do_write` parks the request and `tty_devwrite` drains the
    /// caller's bytes through the device hook; the framework replies with
    /// the moved count. The count-only [`CharDriver::write`] stays as the
    /// readiness seam; this method is the data path the service feeds.
    pub fn write_bytes(&mut self, minor: u32, bytes: &[u8]) -> Result<usize, Errno> {
        let Some(slot) = self.slot(minor) else {
            return Err(Errno::from_i32(minix_types::ENXIO));
        };
        // C: do_write.c:553-556 — `if (size <= 0) return EINVAL`.
        if bytes.is_empty() {
            return Err(Errno::from_i32(minix_types::EINVAL));
        }
        let accepted = self.backend.write_bytes(bytes);
        self.sessions[slot].note_output(accepted < bytes.len(), accepted < bytes.len());
        Ok(accepted)
    }
}

impl<B: LineBackend> CharDriver for TtyDriver<B> {
    fn open(&mut self, minor: DeviceMinor, access: i32, user: i64) -> i32 {
        let Some(slot) = self.slot(minor.0) else {
            return -minix_types::ENXIO;
        };
        let (result, _device_opened) = self.sessions[slot].open(minor.0, access, user);
        if result >= 0 {
            self.opened.insert_raw(minor.0);
        }
        result
    }

    fn close(&mut self, minor: DeviceMinor) -> i32 {
        let Some(slot) = self.slot(minor.0) else {
            return -minix_types::ENXIO;
        };
        if self.sessions[slot].close(minor.0) {
            0
        } else {
            minix_types::OK
        }
    }

    fn read(
        &mut self,
        minor: DeviceMinor,
        _position: u64,
        _grant: u64,
        size: usize,
        flags: i32,
        _id: RequestId,
    ) -> Result<usize, Errno> {
        // The service-level read path: park when empty (or answer
        // try-again non-blocking), otherwise hand out queued bytes. The
        // grant copy stays with the transport.
        let Some(slot) = self.slot(minor.0) else {
            return Err(Errno::from_i32(minix_types::ENXIO));
        };
        if size == 0 {
            return Ok(0);
        }
        // Pull from the session's own queue (canonical lines hand out
        // whole lines; non-canonical hands out whatever is queued).
        let mut handed = alloc::vec::Vec::new();
        let available = self.sessions[slot].input_mut().drain_ready(size, &mut handed);
        if available > 0 {
            // The service copies `handed` into the caller's grant.
            return Ok(available);
        }
        if flags & 0o4_000 == 0 { // O_NONBLOCK (no named constant in minix-types yet)
            // Park: the service remembers the grant and completes it when
            // feed_input lands a line break. The parked read answers the
            // EDONTREPLY sentinel on the error lane.
            return Err(Errno::from_i32(minix_types::EDONTREPLY));
        }
        Err(Errno::from_i32(minix_types::EAGAIN))
    }

    fn write(
        &mut self,
        minor: DeviceMinor,
        _position: u64,
        _grant: u64,
        size: usize,
        _flags: i32,
        _id: RequestId,
    ) -> Result<usize, Errno> {
        let Some(slot) = self.slot(minor.0) else {
            return Err(Errno::from_i32(minix_types::ENXIO));
        };
        // The backend takes what it can; the session learns the facts for
        // readiness (A8 composition keeps probe honest).
        let accepted = self.backend.dev_write(size);
        self.sessions[slot].note_output(accepted < size, accepted < size);
        Ok(accepted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SilentBackend;

    impl LineBackend for SilentBackend {}

    fn driver() -> TtyDriver<SilentBackend> {
        TtyDriver::new(8, 0, SilentBackend)
    }

    #[test]
    fn test_open_close_flow_through_sessions() {
        let mut driver = driver();
        let console = DeviceMinor(0);
        // First open adopts the controlling terminal: the CONTROLLED bit
        // rides on success (session.open returns it).
        assert!(driver.open(console, 0, 42) >= 0);
        assert_eq!(driver.sessions[0].open_count(), 1);
        assert!(driver.opened.contains_raw(0));
        assert_eq!(driver.close(console), 0);
        assert_eq!(driver.sessions[0].open_count(), 0);
    }

    #[test]
    fn test_log_minor_write_open_does_not_count() {
        // B1 semantics flow through the face: the log alias on a console
        // line opens without counting.
        let mut driver = driver();
        assert_eq!(driver.open(DeviceMinor(15), 0, 42), 0);
        assert_eq!(driver.sessions[0].open_count(), 0);
    }

    #[test]
    fn test_read_returns_queued_line_bytes() {
        let mut driver = driver();
        driver.open(DeviceMinor(0), 0, 42);
        driver.sessions[0].feed_input(&[0x61, 0x62, 0x0A]);
        let moved = CharDriver::read(
            &mut driver,
            DeviceMinor(0),
            0,
            0,
            64,
            0,
            RequestId(1),
        );
        // The completed line (a, b, newline) is three bytes.
        assert_eq!(moved, Ok(3));
    }

    #[test]
    fn test_unknown_minor_is_enxio() {
        let mut driver = driver();
        assert_eq!(
            CharDriver::read(&mut driver, DeviceMinor(125), 0, 0, 8, 0, RequestId(1)),
            Err(Errno::from_i32(minix_types::ENXIO))
        );
    }
}
