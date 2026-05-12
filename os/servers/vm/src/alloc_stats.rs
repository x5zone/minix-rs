//! VM page allocation statistics module.
//!
//! # Single-threaded Assumption
//!
//! All fields are plain `usize`. The VM server is single-threaded;
//! atomic operations are unnecessary and misleading.

pub(crate) struct VmAllocStats {
    total_allocations: usize,
    total_deallocations: usize,
    total_alloc_clicks: usize,
    total_dealloc_clicks: usize,
    allocation_failures: usize,
}

impl VmAllocStats {
    pub(crate) const fn new() -> Self {
        Self {
            total_allocations: 0,
            total_deallocations: 0,
            total_alloc_clicks: 0,
            total_dealloc_clicks: 0,
            allocation_failures: 0,
        }
    }

    pub(crate) fn record_alloc(&mut self, clicks: usize) {
        self.total_allocations += 1;
        self.total_alloc_clicks += clicks;
    }

    pub(crate) fn record_dealloc(&mut self, clicks: usize) {
        debug_assert!(
            self.total_deallocations < self.total_allocations,
            "record_dealloc underflow: more deallocs than allocs"
        );
        self.total_deallocations += 1;
        self.total_dealloc_clicks += clicks;
    }

    pub(crate) fn record_failure(&mut self) {
        self.allocation_failures += 1;
    }

    pub(crate) fn active_allocations(&self) -> usize {
        self.total_allocations - self.total_deallocations
    }

    pub(crate) fn active_pages(&self) -> usize {
        self.total_alloc_clicks - self.total_dealloc_clicks
    }

    pub(crate) fn check_leak(&self) -> Option<usize> {
        let active = self.active_pages();
        if active > 0 {
            Some(active)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_stats_basic() {
        let mut stats = VmAllocStats::new();

        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.active_pages(), 0);
        assert_eq!(stats.check_leak(), None);

        stats.record_alloc(1);
        stats.record_alloc(4);
        assert_eq!(stats.active_allocations(), 2);
        assert_eq!(stats.active_pages(), 5);

        stats.record_dealloc(1);
        assert_eq!(stats.active_allocations(), 1);
        assert_eq!(stats.active_pages(), 4);

        stats.record_dealloc(4);
        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.active_pages(), 0);
        assert_eq!(stats.check_leak(), None);
    }

    #[test]
    fn test_alloc_stats_leak_detection() {
        let mut stats = VmAllocStats::new();

        stats.record_alloc(1);
        stats.record_alloc(4);
        stats.record_dealloc(1);

        assert_eq!(stats.check_leak(), Some(4));
    }

    #[test]
    fn test_alloc_stats_failure_tracking() {
        let mut stats = VmAllocStats::new();

        stats.record_failure();
        stats.record_failure();

        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.active_pages(), 0);
    }

    #[test]
    fn test_alloc_stress() {
        let mut stats = VmAllocStats::new();

        for _ in 0..10000 {
            stats.record_alloc(1);
        }

        for _ in 0..10000 {
            stats.record_dealloc(1);
        }

        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.active_pages(), 0);
        assert_eq!(stats.check_leak(), None);
    }
}
