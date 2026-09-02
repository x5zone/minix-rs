//! Physical memory statistics module.
//!
//! # Single-threaded Assumption
//!
//! All fields are plain `usize`. The VM server is single-threaded;
//! atomic operations are unnecessary and misleading.

use alloc::format;
use alloc::string::String;

#[derive(Debug, Clone)]
pub(crate) struct MemStats {
    total_allocations: usize,
    total_deallocations: usize,
    active_allocations: usize,
    allocation_failures: usize,
    total_allocated_bytes: usize,
    total_freed_bytes: usize,
    current_allocated_bytes: usize,
    peak_allocated_bytes: usize,
}

impl MemStats {
    pub(crate) const fn new() -> Self {
        Self {
            total_allocations: 0,
            total_deallocations: 0,
            active_allocations: 0,
            allocation_failures: 0,
            total_allocated_bytes: 0,
            total_freed_bytes: 0,
            current_allocated_bytes: 0,
            peak_allocated_bytes: 0,
        }
    }

    pub(crate) fn record_alloc(&mut self, bytes: usize) {
        self.total_allocations += 1;
        self.active_allocations += 1;
        self.total_allocated_bytes += bytes;
        self.current_allocated_bytes += bytes;

        if self.current_allocated_bytes > self.peak_allocated_bytes {
            self.peak_allocated_bytes = self.current_allocated_bytes;
        }
    }

    pub(crate) fn record_free(&mut self, bytes: usize) {
        debug_assert!(self.active_allocations > 0, "record_free underflow: no active allocations");
        debug_assert!(self.current_allocated_bytes >= bytes, "record_free underflow: current={}, freeing={}", self.current_allocated_bytes, bytes);
        self.total_deallocations += 1;
        self.active_allocations -= 1;
        self.total_freed_bytes += bytes;
        self.current_allocated_bytes -= bytes;
    }

    pub(crate) fn record_failure(&mut self) {
        self.allocation_failures += 1;
    }

    // V10-P2-1: getters + report are test-only today; production only
    // drives `record_*` (the allocators' hot path).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn total_allocations(&self) -> usize {
        self.total_allocations
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn total_deallocations(&self) -> usize {
        self.total_deallocations
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn active_allocations(&self) -> usize {
        self.active_allocations
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn allocation_failures(&self) -> usize {
        self.allocation_failures
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn total_allocated_bytes(&self) -> usize {
        self.total_allocated_bytes
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn total_freed_bytes(&self) -> usize {
        self.total_freed_bytes
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn current_allocated_bytes(&self) -> usize {
        self.current_allocated_bytes
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn peak_allocated_bytes(&self) -> usize {
        self.peak_allocated_bytes
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn generate_report(&self) -> String {
        format!(
            "Memory Statistics:\n\
             Total allocations: {}\n\
             Total deallocations: {}\n\
             Active allocations: {}\n\
             Allocation failures: {}\n\
             Current allocated: {} bytes\n\
             Peak allocated: {} bytes\n\
             Total allocated (cumulative): {} bytes\n\
             Total freed: {} bytes",
            self.total_allocations(),
            self.total_deallocations(),
            self.active_allocations(),
            self.allocation_failures(),
            self.current_allocated_bytes(),
            self.peak_allocated_bytes(),
            self.total_allocated_bytes(),
            self.total_freed_bytes(),
        )
    }
}

impl Default for MemStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_basic() {
        let mut stats = MemStats::new();

        stats.record_alloc(4096);
        assert_eq!(stats.total_allocations(), 1);
        assert_eq!(stats.active_allocations(), 1);
        assert_eq!(stats.current_allocated_bytes(), 4096);

        stats.record_free(4096);
        assert_eq!(stats.total_deallocations(), 1);
        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.current_allocated_bytes(), 0);
    }

    #[test]
    fn test_peak_tracking() {
        let mut stats = MemStats::new();

        stats.record_alloc(1000);
        assert_eq!(stats.peak_allocated_bytes(), 1000);

        stats.record_alloc(500);
        assert_eq!(stats.peak_allocated_bytes(), 1500);

        stats.record_free(500);
        assert_eq!(stats.peak_allocated_bytes(), 1500);
    }

    #[test]
    fn test_failure_tracking() {
        let mut stats = MemStats::new();
        stats.record_failure();
        stats.record_failure();
        assert_eq!(stats.allocation_failures(), 2);
    }
}
