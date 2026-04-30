//! Physical memory statistics module.

use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub(crate) struct MemStats {
    total_allocations: AtomicUsize,
    total_deallocations: AtomicUsize,
    active_allocations: AtomicUsize,
    allocation_failures: AtomicUsize,
    total_allocated_bytes: AtomicUsize,
    total_freed_bytes: AtomicUsize,
    current_allocated_bytes: AtomicUsize,
    peak_allocated_bytes: AtomicUsize,
}

impl MemStats {
    pub(crate) const fn new() -> Self {
        Self {
            total_allocations: AtomicUsize::new(0),
            total_deallocations: AtomicUsize::new(0),
            active_allocations: AtomicUsize::new(0),
            allocation_failures: AtomicUsize::new(0),
            total_allocated_bytes: AtomicUsize::new(0),
            total_freed_bytes: AtomicUsize::new(0),
            current_allocated_bytes: AtomicUsize::new(0),
            peak_allocated_bytes: AtomicUsize::new(0),
        }
    }

    pub(crate) fn record_alloc(&self, bytes: usize) {
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
        self.active_allocations.fetch_add(1, Ordering::Relaxed);
        self.total_allocated_bytes.fetch_add(bytes, Ordering::Relaxed);

        let current = self.current_allocated_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;

        let mut peak = self.peak_allocated_bytes.load(Ordering::Relaxed);
        while current > peak {
            match self.peak_allocated_bytes.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => peak = actual,
            }
        }
    }

    pub(crate) fn record_free(&self, bytes: usize) {
        self.total_deallocations.fetch_add(1, Ordering::Relaxed);
        self.active_allocations.fetch_sub(1, Ordering::Relaxed);
        self.total_freed_bytes.fetch_add(bytes, Ordering::Relaxed);
        self.current_allocated_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    pub(crate) fn record_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn total_allocations(&self) -> usize {
        self.total_allocations.load(Ordering::Relaxed)
    }

    pub(crate) fn total_deallocations(&self) -> usize {
        self.total_deallocations.load(Ordering::Relaxed)
    }

    pub(crate) fn active_allocations(&self) -> usize {
        self.active_allocations.load(Ordering::Relaxed)
    }

    pub(crate) fn allocation_failures(&self) -> usize {
        self.allocation_failures.load(Ordering::Relaxed)
    }

    pub(crate) fn total_allocated_bytes(&self) -> usize {
        self.total_allocated_bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn total_freed_bytes(&self) -> usize {
        self.total_freed_bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn current_allocated_bytes(&self) -> usize {
        self.current_allocated_bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn peak_allocated_bytes(&self) -> usize {
        self.peak_allocated_bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn total_allocated(&self) -> usize {
        self.current_allocated_bytes()
    }

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

pub(crate) struct MemStatsReporter {
    stats: MemStats,
    #[allow(dead_code)]
    name: String,
}

impl MemStatsReporter {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self { stats: MemStats::new(), name: name.into() }
    }

    pub(crate) fn stats(&self) -> &MemStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stats_basic() {
        let stats = MemStats::new();

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
        let stats = MemStats::new();

        stats.record_alloc(1000);
        assert_eq!(stats.peak_allocated_bytes(), 1000);

        stats.record_alloc(500);
        assert_eq!(stats.peak_allocated_bytes(), 1500);

        stats.record_free(500);
        assert_eq!(stats.peak_allocated_bytes(), 1500);
    }

    #[test]
    fn test_failure_tracking() {
        let stats = MemStats::new();
        stats.record_failure();
        stats.record_failure();
        assert_eq!(stats.allocation_failures(), 2);
    }
}
