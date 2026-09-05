//! Kernel-message delta: which bytes are new since the last signal.
//!
//! C correspondence: `do_new_kmess` in
//! `minix3/minix/drivers/system/log/diag.c:18-54` (and its twin in the
//! terminal driver, `tty.c:389-449`). The kernel keeps a circular message
//! buffer with a `next` write mark; each signal carries no data, only the
//! news that `next` moved. The driver copies the delta between the
//! previous mark and the current one, wrapping around.
//!
//! Shared memory stays in the service crate; this module owns the delta
//! arithmetic, which is pure and fully testable.

/// Size of the kernel message buffer.
///
/// The C code sizes it with `_KMESS_BUF_SIZE`; the arithmetic only needs
/// the value to be consistent between the two marks, so it is a const
/// generic and each side instantiates its own width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagCursor<const WIDTH: usize> {
    previous: usize,
}

impl<const WIDTH: usize> DiagCursor<WIDTH> {
    /// Fresh cursor: nothing seen yet.
    pub const fn new() -> DiagCursor<WIDTH> {
        DiagCursor { previous: 0 }
    }

    /// Start from a known mark (restart recovery).
    pub const fn from_mark(mark: usize) -> DiagCursor<WIDTH> {
        DiagCursor {
            previous: mark % WIDTH,
        }
    }

    /// Bytes that arrived since the last signal, and advance past them.
    ///
    /// C: `bytes = ((next + SIZE) - prev_next) % SIZE` (`diag.c:33`). Zero
    /// new bytes means the buffer was emptied concurrently: nothing to
    /// copy, and the mark still advances (the C code stores `next`
    /// unconditionally at the end).
    pub fn take_delta(&mut self, next: usize) -> usize {
        let next = next % WIDTH;
        let delta = (next + WIDTH - self.previous) % WIDTH;
        self.previous = next;
        delta
    }

    /// Current mark (test inspection).
    pub const fn mark(self) -> usize {
        self.previous
    }
}

impl<const WIDTH: usize> Default for DiagCursor<WIDTH> {
    fn default() -> Self {
        DiagCursor::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fresh_delta_counts_forward() {
        let mut cursor = DiagCursor::<1024>::new();
        assert_eq!(cursor.take_delta(100), 100);
        assert_eq!(cursor.mark(), 100);
    }

    #[test]
    fn test_wraparound_counts_through_zero() {
        let mut cursor = DiagCursor::<1024>::from_mark(1000);
        assert_eq!(cursor.take_delta(50), 74);
        assert_eq!(cursor.mark(), 50);
    }

    #[test]
    fn test_empty_buffer_advances_without_copy() {
        let mut cursor = DiagCursor::<1024>::from_mark(300);
        assert_eq!(cursor.take_delta(300), 0);
        assert_eq!(cursor.mark(), 300);
    }

    #[test]
    fn test_full_wrap_reports_zero_not_full() {
        let mut cursor = DiagCursor::<1024>::new();
        assert_eq!(cursor.take_delta(0), 0);
    }
}
