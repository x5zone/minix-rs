//! Slab allocator comprehensive tests.
//!
//! Provides boundary condition tests, stress tests, and memory leak detection tests.

use super::{SlabCache, SlabStats};
use alloc::collections::BTreeSet;
use alloc::vec::Vec;

mod boundary_tests {
    use super::*;

    #[test]
    fn test_zero_size_not_allowed() {
        let result = std::panic::catch_unwind(|| {
            let _cache = SlabCache::new(0);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_non_power_of_two_size() {
        let result = std::panic::catch_unwind(|| {
            let _cache = SlabCache::new(100);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_oversized_object() {
        let result = std::panic::catch_unwind(|| {
            let _cache = SlabCache::new(4096);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_interleaved_alloc_free() {
        let mut cache = SlabCache::new(64);
        let mut ptrs = Vec::new();

        for _ in 0..10 {
            ptrs.push(cache.allocate().unwrap());
        }

        for i in (0..10).step_by(2) {
            unsafe {
                cache.free(ptrs[i]);
            }
            ptrs[i] = core::ptr::null_mut();
        }

        for _ in 0..5 {
            let ptr = cache.allocate().unwrap();
            ptrs.push(ptr);
        }

        for ptr in ptrs {
            if !ptr.is_null() {
                unsafe {
                    cache.free(ptr);
                }
            }
        }

        assert_eq!(cache.stats().active_allocations(), 0);
    }

    #[test]
    fn test_large_object_alloc() {
        let mut cache = SlabCache::new(2048);

        let mut ptrs = Vec::new();
        for _ in 0..5 {
            let ptr = cache.allocate().expect("allocation failed");
            ptrs.push(ptr);
        }

        for ptr in &ptrs {
            assert!(!ptr.is_null());
        }
        let unique: std::collections::HashSet<_> = ptrs.iter().map(|p| *p as usize).collect();
        assert_eq!(unique.len(), ptrs.len(), "all pointers should be unique");

        unsafe {
            for ptr in ptrs {
                cache.free(ptr);
            }
        }
    }

    #[test]
    fn test_exact_slab_capacity() {
        let mut cache = SlabCache::new(64);

        let header_size = 80;
        let available = 4096 - header_size;
        let capacity = available / 64;

        let mut ptrs = Vec::new();
        for _ in 0..capacity {
            let ptr = cache.allocate().expect("allocation failed");
            ptrs.push(ptr);
        }

        assert_eq!(cache.slab_count(), 1);

        let extra = cache.allocate().expect("allocation failed");
        assert_eq!(cache.slab_count(), 2);

        unsafe {
            for ptr in ptrs {
                cache.free(ptr);
            }
            cache.free(extra);
        }
    }
}

mod stress_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_random_alloc_free() {
        let mut cache = SlabCache::new(64);
        let mut ptrs: Vec<Option<*mut u8>> = (0..100).map(|_| None).collect();

        let mut seed = 12345u64;
        let mut rng = || {
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            (seed / 65536) as usize % 100
        };

        for _ in 0..10000 {
            let idx = rng();

            if ptrs[idx].is_none() {
                if let Some(ptr) = cache.allocate() {
                    ptrs[idx] = Some(ptr);
                }
            } else {
                unsafe {
                    cache.free(ptrs[idx].unwrap());
                }
                ptrs[idx] = None;
            }
        }

        for ptr_opt in ptrs {
            if let Some(ptr) = ptr_opt {
                unsafe {
                    cache.free(ptr);
                }
            }
        }

        assert_eq!(cache.stats().active_allocations(), 0);
    }

    #[test]
    fn test_batch_alloc_free() {
        let mut cache = SlabCache::new(64);

        for round in 0..100 {
            let ptrs: Vec<_> = (0..100)
                .filter_map(|_| cache.allocate())
                .collect();

            assert_eq!(ptrs.len(), 100, "round {}: should allocate 100 objects", round);

            unsafe {
                cache.free_batch(&ptrs);
            }
        }

        assert_eq!(cache.stats().active_allocations(), 0);
    }

    #[test]
    fn test_memory_fragmentation() {
        let mut cache = SlabCache::new(64);
        let mut ptrs: Vec<Option<*mut u8>> = (0..1000).map(|_| None).collect();

        for i in 0..1000 {
            ptrs[i] = cache.allocate();
        }

        for i in (1..1000).step_by(2) {
            if let Some(ptr) = ptrs[i] {
                unsafe {
                    cache.free(ptr);
                }
                ptrs[i] = None;
            }
        }

        for i in (1..1000).step_by(2) {
            ptrs[i] = cache.allocate();
            assert!(ptrs[i].is_some(), "should reuse fragmented space");
        }

        for ptr_opt in ptrs {
            if let Some(ptr) = ptr_opt {
                unsafe {
                    cache.free(ptr);
                }
            }
        }

        assert_eq!(cache.stats().active_allocations(), 0);
    }

    #[test]
    fn test_high_churn() {
        let mut cache = SlabCache::new(64);

        for _ in 0..100000 {
            let ptr = cache.allocate().expect("allocation failed");
            unsafe {
                cache.free(ptr);
            }
        }

        assert_eq!(cache.stats().active_allocations(), 0);
        assert_eq!(
            cache.stats().total_allocations(),
            cache.stats().total_deallocations()
        );
    }

    #[test]
    fn test_unique_pointers() {
        let mut cache = SlabCache::new(64);
        let mut ptrs = Vec::new();

        for _ in 0..1000 {
            let ptr = cache.allocate().expect("allocation failed");
            ptrs.push(ptr);
        }

        let unique: HashSet<_> = ptrs.iter().map(|p| *p as usize).collect();
        assert_eq!(unique.len(), ptrs.len(), "all pointers should be unique");

        unsafe {
            for ptr in ptrs {
                cache.free(ptr);
            }
        }
    }
}

mod leak_tests {
    use super::*;

    #[test]
    fn test_no_leak() {
        let mut cache = SlabCache::new(64);

        let ptrs: Vec<_> = (0..100)
            .map(|_| cache.allocate().unwrap())
            .collect();

        unsafe {
            for ptr in ptrs {
                cache.free(ptr);
            }
        }

        assert_eq!(cache.stats().active_allocations(), 0);
        assert!(cache.stats().check_leak(64).is_none());
    }

    #[test]
    fn test_with_leak() {
        let mut cache = SlabCache::new(64);

        let _ptrs: Vec<_> = (0..10)
            .map(|_| cache.allocate().unwrap())
            .collect();

        let report = cache.stats().check_leak(64).expect("should detect leak");
        assert_eq!(report.active_allocations, 10);
        assert_eq!(report.leaked_bytes, 640);
    }

    #[test]
    fn test_partial_leak() {
        let mut cache = SlabCache::new(64);

        let ptrs: Vec<_> = (0..100)
            .map(|_| cache.allocate().unwrap())
            .collect();

        for i in 0..50 {
            unsafe {
                cache.free(ptrs[i]);
            }
        }

        let report = cache.stats().check_leak(64).expect("should detect leak");
        assert_eq!(report.active_allocations, 50);
        assert_eq!(report.leaked_bytes, 3200);
    }

    #[test]
    fn test_stats_accuracy() {
        let mut cache = SlabCache::new(64);

        for _ in 0..1000 {
            let ptr = cache.allocate().unwrap();
            unsafe {
                cache.free(ptr);
            }
        }

        assert_eq!(cache.stats().total_allocations(), 1000);
        assert_eq!(cache.stats().total_deallocations(), 1000);
        assert_eq!(cache.stats().active_allocations(), 0);
    }

    #[test]
    fn test_full_lifecycle() {
        let mut cache = SlabCache::new(64);

        let ptrs1: Vec<_> = (0..1000).map(|_| cache.allocate().unwrap()).collect();

        for i in 0..500 {
            unsafe {
                cache.free(ptrs1[i]);
            }
        }

        let ptrs2: Vec<_> = (0..500).map(|_| cache.allocate().unwrap()).collect();

        for i in 500..1000 {
            unsafe {
                cache.free(ptrs1[i]);
            }
        }
        for ptr in ptrs2 {
            unsafe {
                cache.free(ptr);
            }
        }

        let stats = cache.stats();
        assert_eq!(stats.active_allocations(), 0);
    }

    #[test]
    fn test_stress_with_leak_check() {
        let mut cache = SlabCache::new(64);

        for round in 0..100 {
            let ptrs: Vec<_> = (0..100)
                .map(|_| cache.allocate().unwrap())
                .collect();

            for i in 0..90 {
                unsafe {
                    cache.free(ptrs[i]);
                }
            }

            let stats = cache.stats();
            let active = stats.active_allocations();
            assert_eq!(active, 10, "round {}: should have 10 active", round);

            for i in 90..100 {
                unsafe {
                    cache.free(ptrs[i]);
                }
            }
        }

        let stats = cache.stats();
        assert_eq!(stats.active_allocations(), 0);
    }
}

mod multi_cache_tests {
    use super::*;

    #[test]
    fn test_multiple_caches() {
        let mut cache8 = SlabCache::new(8);
        let mut cache64 = SlabCache::new(64);
        let mut cache256 = SlabCache::new(256);

        let ptr8 = cache8.allocate().unwrap();
        let ptr64 = cache64.allocate().unwrap();
        let ptr256 = cache256.allocate().unwrap();

        assert_ne!(ptr8, ptr64);
        assert_ne!(ptr64, ptr256);

        unsafe {
            cache8.free(ptr8);
            cache64.free(ptr64);
            cache256.free(ptr256);
        }

        assert_eq!(cache8.stats().active_allocations(), 0);
        assert_eq!(cache64.stats().active_allocations(), 0);
        assert_eq!(cache256.stats().active_allocations(), 0);
    }

    #[test]
    fn test_cache_isolation() {
        let mut cache1 = SlabCache::new(64);
        let mut cache2 = SlabCache::new(64);

        let ptr = cache1.allocate().unwrap();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            unsafe {
                cache2.free(ptr);
            }
        }));

        assert!(result.is_err());

        unsafe {
            cache1.free(ptr);
        }
    }
}
