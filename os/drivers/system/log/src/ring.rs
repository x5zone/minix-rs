//! Log ring: the 50-kilobyte circular buffer with overwrite-oldest.
//!
//! C correspondence: `log_buffer[LOG_SIZE]` with `log_size`, `log_read`,
//! `log_write` (`log.h:15-19`), `LOG_SIZE (50*1024)` (`log.h:12`),
//! `subwrite` (`log.c:123-192`), and `subread` (`log.c:213-239`).
//!
//! Copies stay in the service crate; this ring moves counts. `write`
//! appends (overwriting the oldest when full, exactly like the C loop),
//! `read` drains up to the wanted length, and both report counts so the
//! device layer can wake sleepers.

/// Ring capacity in bytes.
///
/// C: `LOG_SIZE (50*1024)` (`log.h:12`).
pub const LOG_RING: usize = 50 * 1024;

/// Circular log buffer: counts only, bytes live in the service crate.
///
/// The C buffer holds bytes; this type holds the three numbers that
/// decide everything (write mark, read mark, size) plus a shadow copy of
/// at most one window for tests. Production feeds content through grant
/// copies; tests feed marker counts through [`LogRing::note_written`].
/// Overwrite-oldest is preserved: writing past capacity advances the read
/// mark by the overflow (`log.c:160-164`).
#[derive(Debug, Clone)]
pub struct LogRing {
    write: usize,
    read: usize,
    size: usize,
}

impl LogRing {
    /// Empty ring.
    pub const fn new() -> LogRing {
        LogRing {
            write: 0,
            read: 0,
            size: 0,
        }
    }

    /// Bytes currently stored.
    pub fn len(&self) -> usize {
        self.size
    }

    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Append this many bytes; returns how many were kept (all of them:
    /// overflow evicts the oldest instead of refusing).
    ///
    /// C: the `subwrite` loop (`log.c:137-167`) with the overflow branch
    /// (`log.c:160-164`).
    pub fn note_written(&mut self, mut count: usize) -> usize {
        if count > LOG_RING {
            let skip = count - LOG_RING;
            count -= skip;
        }
        let kept = count;
        self.write = (self.write + count) % LOG_RING;
        self.size += count;
        if self.size > LOG_RING {
            let overflow = self.size - LOG_RING;
            self.size -= overflow;
            self.read = (self.read + overflow) % LOG_RING;
        }
        kept
    }

    /// Drain up to `want` bytes; returns how many the reader takes.
    ///
    /// C: the `subread` loop (`log.c:221-236`).
    pub fn note_read(&mut self, want: usize) -> usize {
        let count = want.min(self.size);
        self.read = (self.read + count) % LOG_RING;
        self.size -= count;
        count
    }

    /// Read mark (test inspection).
    pub const fn read_mark(self) -> usize {
        self.read
    }

    /// Write mark (test inspection).
    pub const fn write_mark(self) -> usize {
        self.write
    }
}

impl Default for LogRing {
    fn default() -> Self {
        LogRing::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_then_read_round_trips_counts() {
        let mut ring = LogRing::new();
        assert_eq!(ring.note_written(100), 100);
        assert_eq!(ring.len(), 100);
        assert_eq!(ring.note_read(30), 30);
        assert_eq!(ring.len(), 70);
    }

    #[test]
    fn test_overflow_evicts_oldest_not_newest() {
        let mut ring = LogRing::new();
        ring.note_written(LOG_RING);
        assert_eq!(ring.len(), LOG_RING);
        ring.note_written(10);
        assert_eq!(ring.len(), LOG_RING);
        assert_eq!(ring.read_mark(), 10);
    }

    #[test]
    fn test_oversize_write_keeps_last_window() {
        let mut ring = LogRing::new();
        assert_eq!(ring.note_written(LOG_RING + 5), LOG_RING);
        assert_eq!(ring.len(), LOG_RING);
    }

    #[test]
    fn test_empty_read_takes_nothing() {
        let mut ring = LogRing::new();
        assert_eq!(ring.note_read(10), 0);
        assert!(ring.is_empty());
    }

    #[test]
    fn test_capacity_matches_c_header() {
        assert_eq!(LOG_RING, 50 * 1024);
    }
}
