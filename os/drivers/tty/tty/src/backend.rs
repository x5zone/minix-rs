//! Device backends: what sits below a terminal line.
//!
//! C correspondence: the per-line function pointers `tty_devread`,
//! `tty_devwrite`, `tty_icancel`, `tty_ocancel`, `tty_ioctl`, `tty_open`,
//! `tty_close` (`tty.h`), normally wired to the console, serial, or
//! pseudo-terminal device code. Consoles draw on video memory, serial
//! lines program the serial chip, pseudo slaves move bytes to the master
//! side; this trait keeps the line policy above those details.

/// Device behavior below one terminal line.
///
/// Every method defaults to ignoring the event (the null device): lines
/// without hardware behave like an unplugged terminal rather than failing
/// at bind time.
pub trait LineBackend {
    /// Pull bytes from the device into the line (returns bytes moved).
    fn dev_read(&mut self, max: usize) -> usize {
        let _ = max;
        0
    }

    /// Push bytes toward the device (returns bytes accepted).
    fn dev_write(&mut self, count: usize) -> usize {
        let _ = count;
        0
    }

    /// Cancel pending input.
    fn input_cancel(&mut self) {}

    /// Cancel pending output.
    fn output_cancel(&mut self) {}

    /// Device-level control (speed, break, ...); false means unhandled.
    fn control(&mut self, request: u64) -> bool {
        let _ = request;
        false
    }

    /// The line was opened (first open).
    fn opened(&mut self) {}

    /// The line was closed (last close).
    fn closed(&mut self) {}
}

/// Null backend: an unplugged terminal; every operation is a no-op.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullBackend;

impl LineBackend for NullBackend {}

/// Loop backend: written bytes become readable (echo chamber).
///
/// Models a line whose output loops back to its input, as a serial line
/// with its plug turned around behaves. Capacity is bounded; excess writes
/// report only what fit.
#[derive(Debug, Clone)]
pub struct LoopBackend {
    buffer: alloc::vec::Vec<u8>,
    capacity: usize,
}

impl LoopBackend {
    /// Fresh loop with room for this many bytes.
    pub fn new(capacity: usize) -> LoopBackend {
        LoopBackend {
            buffer: alloc::vec::Vec::new(),
            capacity,
        }
    }

    /// Bytes currently looped back and unread.
    pub fn pending(&self) -> usize {
        self.buffer.len()
    }

    /// Take up to `max` looped bytes.
    pub fn take(&mut self, max: usize) -> alloc::vec::Vec<u8> {
        let count = max.min(self.buffer.len());
        self.buffer.drain(..count).collect()
    }
}

impl LineBackend for LoopBackend {
    fn dev_write(&mut self, count: usize) -> usize {
        let room = self.capacity.saturating_sub(self.buffer.len());
        let accepted = count.min(room);
        self.buffer.extend(core::iter::repeat_n(0, accepted));
        accepted
    }

    fn dev_read(&mut self, max: usize) -> usize {
        max.min(self.buffer.len())
    }

    fn input_cancel(&mut self) {
        self.buffer.clear();
    }

    fn output_cancel(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_null_backend_ignores_everything() {
        let mut backend = NullBackend;
        assert_eq!(backend.dev_write(100), 0);
        assert_eq!(backend.dev_read(100), 0);
        assert!(!backend.control(1));
        backend.opened();
        backend.closed();
    }

    #[test]
    fn test_loop_backend_echoes_up_to_capacity() {
        let mut backend = LoopBackend::new(8);
        assert_eq!(backend.dev_write(5), 5);
        assert_eq!(backend.dev_write(5), 3);
        assert_eq!(backend.pending(), 8);
        assert_eq!(backend.take(3).len(), 3);
        backend.input_cancel();
        assert_eq!(backend.pending(), 0);
    }
}
