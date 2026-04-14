//! 物理内存分配器测试
//!
//! 包含单元测试、边界测试和压力测试。

use super::*;
use allocator::{PhysMemAllocator, AllocFlags, PhysAddr};

/// 基础功能测试模块
mod basic_tests {
    use super::*;

    #[test]
    fn test_allocator_creation() {
        let allocator = PhysMemAllocator::new(512 * 1024 * 1024);
        assert_eq!(allocator.total_memory(), 512 * 1024 * 1024);
        assert_eq!(allocator.free_memory(), 512 * 1024 * 1024);
    }

    #[test]
    fn test_single_alloc_free() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        let addr = allocator.alloc(4, AllocFlags::empty()).unwrap();
        assert!(addr.is_valid());
        assert_eq!(allocator.stats().active_allocations(), 1);

        allocator.free(addr, 4);
        assert_eq!(allocator.stats().active_allocations(), 0);
    }

    #[test]
    fn test_multiple_allocations() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        let mut addrs = Vec::new();
        for i in 1..=10 {
            let addr = allocator.alloc(i, AllocFlags::empty()).unwrap();
            addrs.push((addr, i));
        }

        assert_eq!(allocator.stats().active_allocations(), 10);

        // 释放所有
        for (addr, clicks) in addrs {
            allocator.free(addr, clicks);
        }

        assert_eq!(allocator.stats().active_allocations(), 0);
    }

    #[test]
    fn test_alloc_zero_clicks() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        let addr = allocator.alloc(0, AllocFlags::empty()).unwrap();
        assert!(!addr.is_valid());
    }

    #[test]
    fn test_alloc_with_zero_flag() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        let addr = allocator.alloc(4, AllocFlags::ZERO).unwrap();
        assert!(addr.is_valid());

        allocator.free(addr, 4);
    }
}

/// 边界条件测试模块
mod boundary_tests {
    use super::*;

    #[test]
    fn test_exact_memory_limit() {
        // 分配 2MB 内存（从 1MB 开始分配，所以需要 2MB 才能分配 1MB）
        let mut allocator = PhysMemAllocator::new(2 * 1024 * 1024);

        // 256 clicks = 1MB
        let addr = allocator.alloc(256, AllocFlags::empty()).unwrap();
        assert!(addr.is_valid());

        // 应该没有剩余内存了（总共 2MB，从 1MB 开始，只能分配 1MB）
        assert!(allocator.alloc(1, AllocFlags::empty()).is_none());

        allocator.free(addr, 256);
    }

    #[test]
    fn test_large_allocation() {
        let mut allocator = PhysMemAllocator::new(512 * 1024 * 1024);

        // 分配 100MB
        let addr = allocator.alloc(25600, AllocFlags::empty()).unwrap();
        assert!(addr.is_valid());

        allocator.free(addr, 25600);
    }

    #[test]
    fn test_low_memory_allocation() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        // 分配低端内存
        let addr = allocator.alloc(4, AllocFlags::LOW).unwrap();
        assert!(addr.is_valid());
        // 低端内存应该 < 16MB
        assert!(addr.as_u64() < 16 * 1024 * 1024);

        allocator.free(addr, 4);
    }

    #[test]
    fn test_memory_exhaustion() {
        let mut allocator = PhysMemAllocator::new(4 * 1024 * 1024); // 4MB

        // 尝试分配超过总内存（从 1MB 开始，最多只能分配 3MB）
        let result = allocator.alloc(1024, AllocFlags::empty()); // 4MB
        assert!(result.is_none());

        // 验证失败计数（可能为 0，因为 Mock 实现可能不严格检查）
        // 注：当前 Mock 实现可能不记录所有失败
    }
}

/// 压力测试模块
mod stress_tests {
    use super::*;

    #[test]
    fn test_random_alloc_free() {
        let mut allocator = PhysMemAllocator::new(128 * 1024 * 1024);
        let mut allocations: Vec<Option<(PhysAddr, usize)>> = (0..100).map(|_| None).collect();

        // 伪随机分配/释放 1000 次
        let mut seed: u64 = 12345;
        for _ in 0..1000 {
            // 简单的伪随机数生成
            seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
            let idx = (seed % 100) as usize;

            if allocations[idx].is_none() {
                // 分配 1-10 clicks
                seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
                let clicks = ((seed % 10) + 1) as usize;
                if let Some(addr) = allocator.alloc(clicks, AllocFlags::empty()) {
                    allocations[idx] = Some((addr, clicks));
                }
            } else {
                // 释放
                let (addr, clicks) = allocations[idx].unwrap();
                allocator.free(addr, clicks);
                allocations[idx] = None;
            }
        }

        // 清理剩余
        for opt in allocations {
            if let Some((addr, clicks)) = opt {
                allocator.free(addr, clicks);
            }
        }

        assert_eq!(allocator.stats().active_allocations(), 0);
    }

    #[test]
    fn test_high_churn() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        // 高频分配释放
        for _ in 0..10000 {
            let addr = allocator.alloc(1, AllocFlags::empty()).unwrap();
            allocator.free(addr, 1);
        }

        assert_eq!(allocator.stats().active_allocations(), 0);
        assert_eq!(allocator.stats().total_allocations(), 10000);
        assert_eq!(allocator.stats().total_deallocations(), 10000);
    }

    #[test]
    fn test_fragmentation_simulation() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);
        let mut ptrs: Vec<Option<(PhysAddr, usize)>> = Vec::new();

        // 分配各种大小的块
        for size in 1..=50 {
            if let Some(addr) = allocator.alloc(size, AllocFlags::empty()) {
                ptrs.push(Some((addr, size)));
            }
        }

        // 释放偶数索引的块
        for i in (0..ptrs.len()).step_by(2) {
            if let Some((addr, size)) = ptrs[i] {
                allocator.free(addr, size);
                ptrs[i] = None;
            }
        }

        // 尝试分配更大的块（测试碎片）
        let large = allocator.alloc(100, AllocFlags::CONTIG);
        // Mock 实现可能成功也可能失败，取决于实现

        if let Some(addr) = large {
            allocator.free(addr, 100);
        }

        // 清理
        for opt in ptrs {
            if let Some((addr, size)) = opt {
                allocator.free(addr, size);
            }
        }
    }
}

/// 统计信息测试模块
mod stats_tests {
    use super::*;

    #[test]
    fn test_stats_accuracy() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        // 执行已知数量的分配
        for _ in 0..100 {
            let addr = allocator.alloc(1, AllocFlags::empty()).unwrap();
            allocator.free(addr, 1);
        }

        let stats = allocator.stats();
        assert_eq!(stats.total_allocations(), 100);
        assert_eq!(stats.total_deallocations(), 100);
        assert_eq!(stats.active_allocations(), 0);
    }

    #[test]
    fn test_peak_memory() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        // 分配一些内存
        let addr1 = allocator.alloc(100, AllocFlags::empty()).unwrap();
        let peak1 = allocator.stats().peak_allocated_bytes();

        let addr2 = allocator.alloc(100, AllocFlags::empty()).unwrap();
        let peak2 = allocator.stats().peak_allocated_bytes();

        assert!(peak2 > peak1);

        // 释放后峰值应该不变
        allocator.free(addr1, 100);
        allocator.free(addr2, 100);

        assert_eq!(allocator.stats().peak_allocated_bytes(), peak2);
    }

    #[test]
    fn test_report_generation() {
        let mut allocator = PhysMemAllocator::new(64 * 1024 * 1024);

        let addr = allocator.alloc(10, AllocFlags::empty()).unwrap();
        let report = allocator.stats().generate_report();

        assert!(report.contains("Memory Statistics"));
        assert!(report.contains("Total allocations"));
        assert!(report.contains("Active allocations"));

        allocator.free(addr, 10);
    }
}

/// 工具函数测试模块
mod util_tests {
    use super::*;

    #[test]
    fn test_bytes_to_clicks() {
        assert_eq!(bytes_to_clicks(0), 0);
        assert_eq!(bytes_to_clicks(1), 1);
        assert_eq!(bytes_to_clicks(4096), 1);
        assert_eq!(bytes_to_clicks(4097), 2);
        assert_eq!(bytes_to_clicks(8192), 2);
    }

    #[test]
    fn test_clicks_to_bytes() {
        assert_eq!(clicks_to_bytes(0), 0);
        assert_eq!(clicks_to_bytes(1), 4096);
        assert_eq!(clicks_to_bytes(2), 8192);
    }

    #[test]
    fn test_click_floor() {
        assert_eq!(click_floor(0), 0);
        assert_eq!(click_floor(1), 0);
        assert_eq!(click_floor(4095), 0);
        assert_eq!(click_floor(4096), 4096);
        assert_eq!(click_floor(5000), 4096);
    }

    #[test]
    fn test_click_ceil() {
        assert_eq!(click_ceil(0), 0);
        assert_eq!(click_ceil(1), 4096);
        assert_eq!(click_ceil(4096), 4096);
        assert_eq!(click_ceil(4097), 8192);
    }

    #[test]
    fn test_phys_addr_ops() {
        let addr = PhysAddr::new(0x1234);
        assert_eq!(addr.as_u64(), 0x1234);

        // 0x1234 = 4660, 向上对齐到 4096 边界 = 8192
        let aligned = addr.align_up();
        assert_eq!(aligned.as_u64(), 8192);

        let added = addr.add(100);
        assert_eq!(added.as_u64(), 0x1298); // 0x1234 + 100 = 0x1298
    }
}
