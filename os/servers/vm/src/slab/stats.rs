//! Slab allocator statistics.

use core::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub(crate) struct SlabStats {
    total_allocations: AtomicUsize,
    total_deallocations: AtomicUsize,
    active_allocations: AtomicUsize,
    allocation_failures: AtomicUsize,
    fast_path_hits: AtomicUsize,
    slow_path_hits: AtomicUsize,
    pages_in_use: AtomicUsize,
    total_pages_allocated: AtomicUsize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LeakReport {
    pub active_allocations: usize,
    pub total_allocations: usize,
    pub total_deallocations: usize,
    pub leaked_bytes: usize,
}

impl SlabStats {
    pub(crate) const fn new() -> Self {
        Self {
            total_allocations: AtomicUsize::new(0),
            total_deallocations: AtomicUsize::new(0),
            active_allocations: AtomicUsize::new(0),
            allocation_failures: AtomicUsize::new(0),
            fast_path_hits: AtomicUsize::new(0),
            slow_path_hits: AtomicUsize::new(0),
            pages_in_use: AtomicUsize::new(0),
            total_pages_allocated: AtomicUsize::new(0),
        }
    }

    pub(crate) fn record_alloc(&self, fast_path: bool) {
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
        self.active_allocations.fetch_add(1, Ordering::Relaxed);
        if fast_path {
            self.fast_path_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.slow_path_hits.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_free(&self) {
        self.total_deallocations.fetch_add(1, Ordering::Relaxed);
        self.active_allocations.fetch_sub(1, Ordering::Relaxed);
    }

    pub(crate) fn record_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_page_alloc(&self, count: usize) {
        self.pages_in_use.fetch_add(count, Ordering::Relaxed);
        self.total_pages_allocated.fetch_add(count, Ordering::Relaxed);
    }

    pub(crate) fn record_page_free(&self, count: usize) {
        self.pages_in_use.fetch_sub(count, Ordering::Relaxed);
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

    pub(crate) fn fast_path_hits(&self) -> usize {
        self.fast_path_hits.load(Ordering::Relaxed)
    }

    pub(crate) fn slow_path_hits(&self) -> usize {
        self.slow_path_hits.load(Ordering::Relaxed)
    }

    pub(crate) fn pages_in_use(&self) -> usize {
        self.pages_in_use.load(Ordering::Relaxed)
    }

    pub(crate) fn total_pages_allocated(&self) -> usize {
        self.total_pages_allocated.load(Ordering::Relaxed)
    }

    pub(crate) fn check_leak(&self, object_size: usize) -> Option<LeakReport> {
        let active = self.active_allocations.load(Ordering::Relaxed);

        if active > 0 {
            Some(LeakReport {
                active_allocations: active,
                total_allocations: self.total_allocations.load(Ordering::Relaxed),
                total_deallocations: self.total_deallocations.load(Ordering::Relaxed),
                leaked_bytes: active * object_size,
            })
        } else {
            None
        }
    }

    pub(crate) fn reset(&self) {
        self.total_allocations.store(0, Ordering::Relaxed);
        self.total_deallocations.store(0, Ordering::Relaxed);
        self.active_allocations.store(0, Ordering::Relaxed);
        self.allocation_failures.store(0, Ordering::Relaxed);
        self.fast_path_hits.store(0, Ordering::Relaxed);
        self.slow_path_hits.store(0, Ordering::Relaxed);
        self.pages_in_use.store(0, Ordering::Relaxed);
        self.total_pages_allocated.store(0, Ordering::Relaxed);
    }
}

impl Default for SlabStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_stats() {
        let stats = SlabStats::new();

        stats.record_alloc(true);
        assert_eq!(stats.total_allocations(), 1);
        assert_eq!(stats.active_allocations(), 1);
        assert_eq!(stats.fast_path_hits(), 1);

        stats.record_free();
        assert_eq!(stats.total_deallocations(), 1);
        assert_eq!(stats.active_allocations(), 0);
    }

    #[test]
    fn test_leak_detection() {
        let stats = SlabStats::new();

        assert!(stats.check_leak(64).is_none());

        stats.record_alloc(true);
        stats.record_alloc(true);

        let report = stats.check_leak(64).expect("should detect leak");
        assert_eq!(report.active_allocations, 2);
        assert_eq!(report.leaked_bytes, 128);

        stats.record_free();
        stats.record_free();
        assert!(stats.check_leak(64).is_none());
    }

    #[test]
    fn test_page_stats() {
        let stats = SlabStats::new();

        stats.record_page_alloc(5);
        assert_eq!(stats.pages_in_use(), 5);
        assert_eq!(stats.total_pages_allocated(), 5);

        stats.record_page_free(2);
        assert_eq!(stats.pages_in_use(), 3);
        assert_eq!(stats.total_pages_allocated(), 5);
    }

    #[test]
    fn test_reset() {
        let stats = SlabStats::new();

        stats.record_alloc(true);
        stats.record_page_alloc(10);
        stats.record_failure();

        stats.reset();

        assert_eq!(stats.total_allocations(), 0);
        assert_eq!(stats.pages_in_use(), 0);
        assert_eq!(stats.allocation_failures(), 0);
    }
}
