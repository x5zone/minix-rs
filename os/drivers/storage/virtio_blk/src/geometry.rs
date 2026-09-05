//! Drive geometry: single drive, capacity from configuration.
//!
//! C correspondence: `virtio_blk_config` plus the single-partition setup
//! (`part[0].dv_size = blk_config.capacity * VIRTIO_BLK_BLOCK_SIZE`,
//! `virtio_blk.c:128-137`), `virtio_blk_part`
//! (`virtio_blk.c:396-...`), and `virtio_blk_geometry`
//! (`virtio_blk.c:421-...`).

use super::request::BLOCK_SIZE;

/// One drive: capacity in 512-byte sectors plus read-only flag.
///
/// C: `blk_config.capacity` with the read-only feature check
/// (`virtio_blk.c:128-137`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriveGeometry {
    /// Capacity in sectors.
    pub sectors: u64,
    /// True when the host offers the drive read-only.
    pub read_only: bool,
}

impl DriveGeometry {
    /// Total bytes on the drive.
    pub const fn bytes(self) -> u64 {
        self.sectors * BLOCK_SIZE
    }

    /// True when this write must be refused before touching the queue.
    ///
    /// C: read-only drives refuse writes at the transfer entry (the
    /// feature check in `virtio_blk_feature_setup` plus the write path).
    pub const fn refuses_write(self) -> bool {
        self.read_only
    }
}

/// Open-count policy: the driver tracks opens for the control query.
///
/// C: `open_count` with `DIOCOPENCT` (`virtio_blk.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenCount {
    count: u32,
}

impl OpenCount {
    /// Fresh counter (nothing open).
    pub const fn new() -> OpenCount {
        OpenCount { count: 0 }
    }

    /// Record one open.
    pub const fn opened(mut self) -> OpenCount {
        self.count += 1;
        OpenCount { count: self.count }
    }

    /// Record one close (saturates at zero: closes beyond opens are a
    /// caller bug, not a counter underflow).
    pub const fn closed(mut self) -> OpenCount {
        if self.count > 0 {
            self.count -= 1;
        }
        OpenCount { count: self.count }
    }

    /// Current count.
    pub const fn count(self) -> u32 {
        self.count
    }
}

impl Default for OpenCount {
    fn default() -> Self {
        OpenCount::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capacity_multiplies_by_block_size() {
        let drive = DriveGeometry {
            sectors: 2048,
            read_only: false,
        };
        assert_eq!(drive.bytes(), 2048 * 512);
        assert!(!drive.refuses_write());
    }

    #[test]
    fn test_read_only_refuses_writes_upfront() {
        let drive = DriveGeometry {
            sectors: 100,
            read_only: true,
        };
        assert!(drive.refuses_write());
    }

    #[test]
    fn test_open_count_saturates_at_zero() {
        let count = OpenCount::new().opened().opened().closed();
        assert_eq!(count.count(), 1);
        let empty = OpenCount::new().closed();
        assert_eq!(empty.count(), 0);
    }
}
