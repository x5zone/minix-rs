//! Receive ring cursor: page wrap, packet length guard, boundary move.
//!
//! C correspondence: the page geometry (`DP_PAGESIZE 256`,
//! `dp8390.h:163`, `de_startpage`/`de_stoppage`, `dp8390.h:223-224`),
//! the receive walk (`do_recv`, `dp8390.c:598-720`: next page is
//! boundary plus one, wrapping from stop back to start, `dp8390.c:610-611`;
//! stop when current equals the next page, `dp8390.c:616-622`), the
//! length guard (drop packets shorter than the minimum or longer than
//! the maximum, `dp8390.c:639`), and the boundary advance (boundary
//! becomes next minus one, `dp8390.c:668-671`).
//!
//! Register reads stay in the service binary; this module owns the
//! pure cursor half: which page comes next and which length counts.

/// Bytes in one network-chip page (`DP_PAGESIZE`).
pub const PAGE_SIZE: u16 = 256;

/// Smallest Ethernet packet the driver keeps (`NDEV_ETH_PACKET_MIN`).
pub const MIN_PACKET_LEN: u16 = 60;

/// Receive cursor: where the chip writes, where the driver reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecvCursor {
    /// First page of the receive ring.
    start_page: u8,
    /// One past the last page of the receive ring.
    stop_page: u8,
    /// Last page the driver has consumed (boundary).
    boundary: u8,
}

impl RecvCursor {
    /// A fresh ring: boundary starts one before the start page.
    pub fn new(start_page: u8, stop_page: u8) -> Self {
        RecvCursor { start_page, stop_page, boundary: start_page.wrapping_sub(1) }
    }

    /// Next page to read: boundary plus one, wrapping from stop
    /// back to start (`dp8390.c:610-611`).
    pub fn next_page(&self) -> u8 {
        let next = self.boundary.wrapping_add(1);
        if next >= self.stop_page { self.start_page } else { next }
    }

    /// Whether a packet waits: current differs from the next page
    /// (`dp8390.c:616-622`; equal means suspend).
    pub fn packet_waiting(&self, current: u8) -> bool {
        current != self.next_page()
    }

    /// Advance the boundary past a consumed packet (`dp8390.c:668-671`).
    pub fn advance(&mut self, next: u8) {
        self.boundary = next.wrapping_sub(1);
    }

    /// Current boundary value (for tests and the service layer).
    pub fn boundary(&self) -> u8 {
        self.boundary
    }
}

/// Whether a received length is worth keeping (`dp8390.c:639`).
pub fn length_allowed(length: u16, max: u16) -> bool {
    length >= MIN_PACKET_LEN && length <= max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_starts_before_ring() {
        let cursor = RecvCursor::new(0x46, 0x60);
        assert_eq!(cursor.next_page(), 0x46);
        assert_eq!(PAGE_SIZE, 256);
    }

    #[test]
    fn test_cursor_wraps_at_stop_page() {
        let mut cursor = RecvCursor::new(0x46, 0x60);
        cursor.advance(0x60);
        assert_eq!(cursor.next_page(), 0x46);
    }

    #[test]
    fn test_current_equal_next_means_suspend() {
        let cursor = RecvCursor::new(0x46, 0x60);
        assert!(!cursor.packet_waiting(0x46));
        assert!(cursor.packet_waiting(0x47));
    }

    #[test]
    fn test_length_guard_drops_runt_and_giant() {
        assert!(length_allowed(60, 1514));
        assert!(length_allowed(1514, 1514));
        assert!(!length_allowed(59, 1514));
        assert!(!length_allowed(1515, 1514));
        assert_eq!(MIN_PACKET_LEN, 60);
    }
}
