use core::sync::atomic::{AtomicUsize, Ordering};

pub(crate) struct VmAllocStats {
    total_allocations: AtomicUsize,
    total_deallocations: AtomicUsize,
    allocation_failures: AtomicUsize,
}

impl VmAllocStats {
    pub(crate) const fn new() -> Self {
        Self {
            total_allocations: AtomicUsize::new(0),
            total_deallocations: AtomicUsize::new(0),
            allocation_failures: AtomicUsize::new(0),
        }
    }

    pub(crate) fn record_alloc(&self) {
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_dealloc(&self) {
        self.total_deallocations.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn active_allocations(&self) -> usize {
        self.total_allocations.load(Ordering::Relaxed)
            - self.total_deallocations.load(Ordering::Relaxed)
    }

    pub(crate) fn check_leak(&self) -> Option<usize> {
        let active = self.active_allocations();
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
        let stats = VmAllocStats::new();

        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.check_leak(), None);

        stats.record_alloc();
        stats.record_alloc();
        assert_eq!(stats.active_allocations(), 2);

        stats.record_dealloc();
        assert_eq!(stats.active_allocations(), 1);

        stats.record_dealloc();
        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.check_leak(), None);
    }

    #[test]
    fn test_alloc_stats_leak_detection() {
        let stats = VmAllocStats::new();

        stats.record_alloc();
        stats.record_alloc();
        stats.record_dealloc();

        assert_eq!(stats.check_leak(), Some(1));
    }

    #[test]
    fn test_alloc_stats_failure_tracking() {
        let stats = VmAllocStats::new();

        stats.record_failure();
        stats.record_failure();

        assert_eq!(stats.active_allocations(), 0);
    }

    #[test]
    fn test_alloc_stress() {
        let stats = VmAllocStats::new();
        let mut ptrs = Vec::new();

        for _ in 0..10000 {
            stats.record_alloc();
            ptrs.push(0usize);
        }

        for _ in 0..10000 {
            stats.record_dealloc();
            ptrs.pop();
        }

        assert_eq!(stats.active_allocations(), 0);
        assert_eq!(stats.check_leak(), None);
    }
}
