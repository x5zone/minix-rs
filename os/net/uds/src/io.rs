//! UNIX-domain data plane policy: ring arithmetic, segment kinds, caps.
//!
//! C correspondence: `minix3/minix/net/uds/io.c` (1803 lines) with the
//! buffer sizes in `minix3/minix/net/uds/uds.h:33-36`. There is no send
//! buffer, only a per-socket receive buffer mapped while the socket is in
//! use (`io.c:122`). Buffer storage, descriptor queues, and credential
//! handling stay in the service binary. This module owns the portion that
//! can be decided from numbers alone: ring positions, segment kinds, and
//! the two capacity caps.
//!
//! Data and metadata share one ring: ordinary bytes, ancillary descriptor
//! sets, sender credentials, and segment headers interleave, so the usable
//! payload is typically less than the raw buffer size.

/// Per-socket receive buffer in bytes (`UDS_BUF`, 32768, `uds.h:33`; kept
/// a multiple of the page size, and changing it past 65535 would require
/// widening several fields).
pub const RECEIVE_BUFFER: usize = 32768;

/// Largest control data per send or receive (`UDS_CTL_MAX`, 4096,
/// `uds.h:36`).
pub const CONTROL_MAX: usize = 4096;

/// Segment header length (`UDS_HDRLEN`, 5, `io.c:70`).
pub const SEGMENT_HEADER: usize = 5;

/// Receive-buffer segment kinds (one segment carries ordinary data, or
/// ancillary data, or both, or neither as a pure marker).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    /// Ordinary payload bytes only.
    Data,
    /// Ancillary descriptor or credential bytes only.
    Control,
    /// Both payload and ancillary bytes.
    DataAndControl,
    /// Neither, a pure queue marker.
    Empty,
}

/// Advance a ring position (`uds_advance`, `io.c:82`).
pub fn ring_advance(position: usize, amount: usize) -> usize {
    (position + amount) % RECEIVE_BUFFER
}

/// Free bytes in a receive buffer holding `used` bytes.
pub fn ring_free(used: usize) -> usize {
    RECEIVE_BUFFER.saturating_sub(used)
}

/// Largest payload that fits alongside a header of `header` bytes
/// (`io.c:205`: usable space is the buffer minus one header).
pub fn max_payload(header: usize) -> usize {
    RECEIVE_BUFFER.saturating_sub(header)
}

/// Whether a control length passes (`io.c` control paths reject anything
/// above the control cap).
pub fn control_length_allowed(length: usize) -> bool {
    length <= CONTROL_MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_caps_match_header() {
        assert_eq!(RECEIVE_BUFFER, 32768);
        assert_eq!(CONTROL_MAX, 4096);
        assert_eq!(SEGMENT_HEADER, 5);
    }

    #[test]
    fn test_ring_wraps_at_buffer_size() {
        assert_eq!(ring_advance(32767, 1), 0);
        assert_eq!(ring_advance(100, 200), 300);
        assert_eq!(ring_advance(0, 32768), 0);
    }

    #[test]
    fn test_free_and_payload_account_for_header() {
        assert_eq!(ring_free(0), 32768);
        assert_eq!(ring_free(32768), 0);
        assert_eq!(ring_free(99999), 0);
        assert_eq!(max_payload(5), 32763);
    }

    #[test]
    fn test_control_cap_is_enforced() {
        assert!(control_length_allowed(4096));
        assert!(!control_length_allowed(4097));
    }

    #[test]
    fn test_segment_kinds_cover_buffer_use() {
        let kinds = [
            SegmentKind::Data,
            SegmentKind::Control,
            SegmentKind::DataAndControl,
            SegmentKind::Empty,
        ];
        assert_eq!(kinds.len(), 4);
    }
}
