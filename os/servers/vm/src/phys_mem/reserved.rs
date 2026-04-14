//! 保留页队列 (Reserved Page Queue)
//!
//! 为系统关键路径预留的内存池，确保在内存紧张时仍能分配小块内存。
//! 用于 fork、exec 等不能失败的操作。
//!
//! 对应 Minix3: `alloc.c` 中的 `reservedqueues` 机制

use super::{AllocFlags, PhysAddr};

/// 保留页队列魔数
///
/// 用于检测内存损坏
const RESERVED_MAGIC: u32 = 0x6e4c74d5;

/// 最大保留页数
///
/// 单个队列最多保留的页槽数量
const MAX_RESERVED_PAGES: usize = 300;

/// 最大保留队列数
///
/// 系统最多支持的保留队列数量
const MAX_RESERVED_QUEUES: usize = 15;

/// 保留页槽
///
/// 存储单个预留页的物理地址和虚拟地址
#[derive(Debug, Clone, Copy)]
struct ReservedSlot {
    /// 物理地址
    phys: PhysAddr,
    /// 虚拟地址（如果已映射）
    vir: Option<usize>,
}

impl ReservedSlot {
    /// 创建空槽
    const fn empty() -> Self {
        Self {
            phys: PhysAddr(0),
            vir: None,
        }
    }
}

/// 保留页队列
///
/// 管理一组预留的物理页，用于紧急内存分配。
/// 对应 Minix3: `struct reserved_pages`
#[derive(Debug)]
pub struct ReservedQueue {
    /// 下一个在使用的队列（链表）
    next: Option<usize>,

    /// 队列深度（最大可用槽数）
    max_available: usize,

    /// 每次分配的连续页数
    npages: usize,

    /// 是否需要映射到虚拟地址
    mapped: bool,

    /// 当前可用槽数
    n_available: usize,

    /// 分配标志
    alloc_flags: AllocFlags,

    /// 页槽数组
    slots: [ReservedSlot; MAX_RESERVED_PAGES],

    /// 魔数（用于检测内存损坏）
    magic: u32,
}

impl ReservedQueue {
    /// 创建新的空队列
    const fn new() -> Self {
        Self {
            next: None,
            max_available: 0,
            npages: 0,
            mapped: false,
            n_available: 0,
            alloc_flags: AllocFlags::empty(),
            slots: [ReservedSlot::empty(); MAX_RESERVED_PAGES],
            magic: 0,
        }
    }

    /// 检查队列是否有效
    fn sanity_check(&self) {
        assert_eq!(self.magic, RESERVED_MAGIC, "ReservedQueue magic mismatch");
        assert!(self.n_available <= MAX_RESERVED_PAGES);
        assert!(self.n_available <= self.max_available);
    }

    /// 检查是否是有效的队列引用
    fn is_valid(&self) -> bool {
        self.magic == RESERVED_MAGIC && self.max_available > 0
    }
}

/// 保留页队列管理器
///
/// 管理所有保留页队列，提供紧急内存分配功能。
#[derive(Debug)]
pub struct ReservedQueueManager {
    /// 队列数组
    queues: [ReservedQueue; MAX_RESERVED_QUEUES],

    /// 第一个在使用的队列索引
    first_in_use: Option<usize>,

    /// 缺少的备用页数
    ///
    /// 需要补充的页槽数量，用于触发补充分配
    missing_spares: usize,
}

impl ReservedQueueManager {
    /// 创建新的保留页队列管理器
    pub const fn new() -> Self {
        Self {
            queues: [const { ReservedQueue::new() }; MAX_RESERVED_QUEUES],
            first_in_use: None,
            missing_spares: 0,
        }
    }

    /// 创建新的保留页队列
    ///
    /// # 参数
    /// - `max_available`: 队列深度（最大可用槽数）
    /// - `npages`: 每次分配的连续页数
    /// - `mapped`: 是否需要映射到虚拟地址
    /// - `alloc_flags`: 分配标志
    ///
    /// # 返回值
    /// - `Some(queue_id)`: 队列ID
    /// - `None`: 队列槽位已满
    ///
    /// 对应 Minix3: `reservedqueue_new()`
    pub fn create_queue(
        &mut self,
        max_available: usize,
        npages: usize,
        mapped: bool,
        alloc_flags: AllocFlags,
    ) -> Option<usize> {
        assert!(max_available > 0 && max_available < MAX_RESERVED_PAGES);
        assert!(npages > 0 && npages < 10);

        // 查找空闲队列槽位
        let queue_id = (0..MAX_RESERVED_QUEUES)
            .find(|&i| self.queues[i].max_available == 0)?;

        let queue = &mut self.queues[queue_id];

        // 初始化队列
        queue.next = self.first_in_use;
        self.first_in_use = Some(queue_id);

        queue.max_available = max_available;
        queue.npages = npages;
        queue.mapped = mapped;
        queue.alloc_flags = alloc_flags;
        queue.magic = RESERVED_MAGIC;

        self.missing_spares += max_available;

        Some(queue_id)
    }

    /// 从保留队列分配页
    ///
    /// # 参数
    /// - `queue_id`: 队列ID
    ///
    /// # 返回值
    /// - `Some((phys, vir))`: 物理地址和虚拟地址
    /// - `None`: 队列已空
    ///
    /// 对应 Minix3: `reservedqueue_alloc()`
    pub fn alloc(&mut self, queue_id: usize) -> Option<(PhysAddr, Option<usize>)> {
        if queue_id >= MAX_RESERVED_QUEUES {
            return None;
        }

        let queue = &mut self.queues[queue_id];
        queue.sanity_check();

        if queue.n_available < 1 {
            return None;
        }

        queue.n_available -= 1;
        self.missing_spares += 1;

        let slot = &queue.slots[queue.n_available];
        let result = (slot.phys, slot.vir);

        queue.sanity_check();
        Some(result)
    }

    /// 填充队列槽位
    ///
    /// 从物理内存分配器获取页填充队列。
    /// 通常在内存分配周期中调用。
    ///
    /// 对应 Minix3: `reservedqueue_fill()`
    fn fill_queue(&mut self, queue_id: usize, phys: PhysAddr, vir: Option<usize>) -> bool {
        if queue_id >= MAX_RESERVED_QUEUES {
            return false;
        }

        let queue = &mut self.queues[queue_id];
        queue.sanity_check();

        if queue.n_available >= queue.max_available {
            return false;
        }

        queue.slots[queue.n_available] = ReservedSlot { phys, vir };
        queue.n_available += 1;
        self.missing_spares -= 1;

        queue.sanity_check();
        true
    }

    /// 执行分配周期
    ///
    /// 尝试填充所有需要补充的队列槽位。
    /// 通常在内存释放后调用。
    ///
    /// 对应 Minix3: `alloc_cycle()`
    pub fn alloc_cycle<F>(&mut self, mut alloc_fn: F)
    where
        F: FnMut(usize, AllocFlags) -> Option<(PhysAddr, Option<usize>)>,
    {
        self.sanity_check_queues();

        // 收集所有需要填充的队列信息
        let mut to_fill: Vec<(usize, usize, AllocFlags)> = Vec::new();
        let mut queue_id = self.first_in_use;
        while let Some(id) = queue_id {
            let queue = &self.queues[id];
            if queue.is_valid() {
                let needed = queue.max_available - queue.n_available;
                if needed > 0 {
                    to_fill.push((id, queue.npages, queue.alloc_flags));
                }
            }
            queue_id = queue.next;
        }

        // 填充队列
        for (id, npages, alloc_flags) in to_fill {
            if self.missing_spares == 0 {
                break;
            }

            let queue = &self.queues[id];
            let needed = queue.max_available - queue.n_available;

            for _ in 0..needed {
                if self.missing_spares == 0 {
                    break;
                }

                if let Some((phys, vir)) = alloc_fn(npages, alloc_flags) {
                    if !self.fill_queue(id, phys, vir) {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        self.sanity_check_queues();
    }

    /// 检查队列完整性
    fn sanity_check_queues(&self) {
        let mut count = 0;
        let mut queue_id = self.first_in_use;

        while let Some(id) = queue_id {
            let queue = &self.queues[id];
            assert!(queue.max_available > 0);
            assert!(queue.max_available >= queue.n_available);
            count += queue.max_available - queue.n_available;
            queue_id = queue.next;
        }

        assert_eq!(count, self.missing_spares);
    }

    /// 获取缺少的备用页数
    pub fn missing_spares(&self) -> usize {
        self.missing_spares
    }

    /// 获取队列信息（用于调试）
    pub fn get_queue_info(&self, queue_id: usize) -> Option<QueueInfo> {
        if queue_id >= MAX_RESERVED_QUEUES {
            return None;
        }

        let queue = &self.queues[queue_id];
        if !queue.is_valid() {
            return None;
        }

        Some(QueueInfo {
            max_available: queue.max_available,
            n_available: queue.n_available,
            npages: queue.npages,
            mapped: queue.mapped,
        })
    }
}

/// 队列信息（用于调试和监控）
#[derive(Debug, Clone, Copy)]
pub struct QueueInfo {
    /// 最大可用槽数
    pub max_available: usize,
    /// 当前可用槽数
    pub n_available: usize,
    /// 每次分配的页数
    pub npages: usize,
    /// 是否需要映射
    pub mapped: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_queue() {
        let mut manager = ReservedQueueManager::new();

        let queue_id = manager.create_queue(10, 1, false, AllocFlags::empty());
        assert!(queue_id.is_some());

        let info = manager.get_queue_info(queue_id.unwrap()).unwrap();
        assert_eq!(info.max_available, 10);
        assert_eq!(info.npages, 1);
        assert!(!info.mapped);
    }

    #[test]
    fn test_fill_and_alloc() {
        let mut manager = ReservedQueueManager::new();
        let queue_id = manager.create_queue(5, 1, false, AllocFlags::empty()).unwrap();

        // 填充队列
        for i in 0..5 {
            assert!(manager.fill_queue(queue_id, PhysAddr(i as u64 * 0x1000), None));
        }

        // 验证状态
        let info = manager.get_queue_info(queue_id).unwrap();
        assert_eq!(info.n_available, 5);

        // 分配页
        let (phys, _) = manager.alloc(queue_id).unwrap();
        assert_eq!(phys, PhysAddr(4 * 0x1000)); // LIFO

        let info = manager.get_queue_info(queue_id).unwrap();
        assert_eq!(info.n_available, 4);
    }

    #[test]
    fn test_alloc_empty_queue() {
        let mut manager = ReservedQueueManager::new();
        let queue_id = manager.create_queue(5, 1, false, AllocFlags::empty()).unwrap();

        // 空队列分配应该失败
        assert!(manager.alloc(queue_id).is_none());
    }

    #[test]
    fn test_alloc_cycle() {
        let mut manager = ReservedQueueManager::new();
        let queue_id = manager.create_queue(3, 1, false, AllocFlags::empty()).unwrap();

        // 初始状态：缺少3个备用页
        assert_eq!(manager.missing_spares(), 3);

        // 执行分配周期
        let mut alloc_count = 0;
        manager.alloc_cycle(|npages, _flags| {
            alloc_count += 1;
            Some((PhysAddr(alloc_count as u64 * 0x1000), None))
        });

        // 应该填充所有槽位
        assert_eq!(manager.missing_spares(), 0);
        let info = manager.get_queue_info(queue_id).unwrap();
        assert_eq!(info.n_available, 3);
    }
}
