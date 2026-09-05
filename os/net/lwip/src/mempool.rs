//! Packet-buffer pool: slice sizes, statistics, exhaustion mapping.
//!
//! C correspondence: the buffer size (`MEMPOOL_BUFSIZE 512`,
//! `lwipopts.h:49`), the pool statistics (`mempool_cur_buffers`,
//! `mempool.c:493-497`, and `mempool_max_buffers`,
//! `mempool.c:505-516`), the chain tools (`pchain_end` and
//! `pchain_size`, `pchain.c:108-129`), and the exhaustion mapping
//! (pool empty becomes no-buffers at the socket layer,
//! `udpsock.c:496-497`, `rawsock.c:716-717`).
//!
//! Pool storage stays in the service binary; this module owns the
//! pure sizing half: how big each slice is and what empty means.

/// Bytes in one pool slice (`MEMPOOL_BUFSIZE`).
pub const SLICE_SIZE: u64 = 512;

/// Pool statistics: current and maximum buffer counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStats {
    /// Buffers currently checked out.
    pub current: u32,
    /// Most buffers ever checked out at once.
    pub peak: u32,
}

impl PoolStats {
    /// Whether the pool is under pressure (three quarters of the
    /// send-buffer rule, cf. `TCP_MAX_SENDBUFS`, `tcpsock.c:101`).
    pub fn under_pressure(&self, limit: u32) -> bool {
        self.current * 4 >= limit * 3
    }
}

/// Split a request of `total` bytes into slice lengths (512-byte
/// chaining, `lwipopts.h:18-28`).
pub fn split_slices(total: u64) -> alloc::vec::Vec<u64> {
    let mut out = alloc::vec::Vec::new();
    let mut remaining = total;
    while remaining > 0 {
        let step = remaining.min(SLICE_SIZE);
        out.push(step);
        remaining -= step;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_size_matches_options() {
        assert_eq!(SLICE_SIZE, 512);
    }

    #[test]
    fn test_split_chains_at_512() {
        assert_eq!(split_slices(1200), alloc::vec![512, 512, 176]);
        assert_eq!(split_slices(0), alloc::vec![]);
    }

    #[test]
    fn test_pressure_at_three_quarters() {
        let stats = PoolStats { current: 75, peak: 80 };
        assert!(stats.under_pressure(100));
        assert!(!stats.under_pressure(200));
    }
}
