use alloc::vec;
use alloc::vec::Vec;
use super::alloc_trait::PhysMemAlloc;
use super::stats::MemStats;
use super::types::{AllocError, AllocParams, PageAllocFlags, PhysAddr};
use super::{clicks_to_bytes, CLICK_SIZE, BootMemRegion};

#[derive(Debug, Clone, Copy)]
struct Node {
    max_free: usize,
    left_free: usize,
    right_free: usize,
    len: usize,
}

impl Node {
    const fn empty() -> Self {
        Node { max_free: 0, left_free: 0, right_free: 0, len: 0 }
    }

    const fn free(len: usize) -> Self {
        Node { max_free: len, left_free: len, right_free: len, len }
    }

    const fn used(len: usize) -> Self {
        Node { max_free: 0, left_free: 0, right_free: 0, len }
    }
}

fn merge(left: Node, right: Node) -> Node {
    let len = left.len + right.len;
    if len == 0 {
        return Node::empty();
    }
    let left_free = if left.left_free == left.len {
        left.len + right.left_free
    } else {
        left.left_free
    };
    let right_free = if right.right_free == right.len {
        right.len + left.right_free
    } else {
        right.right_free
    };
    let cross = left.right_free + right.left_free;
    let max_free = left.max_free.max(right.max_free).max(cross);

    Node { max_free, left_free, right_free, len }
}

pub(crate) struct SegmentTreeAllocator {
    n: usize,
    offset: usize,
    tree: Vec<Node>,
    total_pages: usize,
    free_pages: usize,
    stats: MemStats,
    mem_low: usize,
    mem_high: usize,
}

impl SegmentTreeAllocator {
    pub(crate) fn init(regions: &[BootMemRegion]) -> Self {
        let mut mem_low = usize::MAX;
        let mut mem_high = 0;

        for region in regions {
            if region.size == 0 {
                continue;
            }
            region.validate();
            let from = region.base;
            let to = region.base + region.size - 1;
            if from < mem_low {
                mem_low = from;
            }
            if to > mem_high {
                mem_high = to;
            }
        }

        if mem_low == usize::MAX {
            mem_low = 0;
        }

        let n = if mem_high > 0 { mem_high / CLICK_SIZE + 1 } else { 0 };
        let offset = n.next_power_of_two();
        let tree_size = if n > 0 { 2 * offset } else { 2 };

        let mut alloc = Self {
            n,
            offset,
            tree: vec![Node::used(1); tree_size],
            total_pages: 0,
            free_pages: 0,
            stats: MemStats::new(),
            mem_low,
            mem_high,
        };

        for i in 0..n {
            alloc.tree[offset + i] = Node::used(1);
        }

        for region in regions {
            if region.size == 0 {
                continue;
            }
            let base_page = region.base / CLICK_SIZE;
            let num_pages = region.size / CLICK_SIZE;
            for i in base_page..base_page + num_pages {
                if i < n {
                    alloc.tree[offset + i] = Node::free(1);
                }
            }
            alloc.total_pages += num_pages;
            alloc.free_pages += num_pages;
        }

        for i in (1..offset).rev() {
            alloc.tree[i] = merge(alloc.tree[i * 2], alloc.tree[i * 2 + 1]);
        }

        alloc
    }

    fn pull_up(&mut self, mut idx: usize) {
        while idx > 1 {
            idx /= 2;
            self.tree[idx] = merge(self.tree[idx * 2], self.tree[idx * 2 + 1]);
        }
    }

    fn set_page(&mut self, page: usize, free: bool) {
        if page >= self.n {
            return;
        }
        let leaf = self.offset + page;
        self.tree[leaf] = if free { Node::free(1) } else { Node::used(1) };
        self.pull_up(leaf);
    }

    fn set_range(&mut self, start: usize, count: usize, free: bool) {
        for i in start..start + count {
            if i < self.n {
                let leaf = self.offset + i;
                self.tree[leaf] = if free { Node::free(1) } else { Node::used(1) };
            }
        }
        if count == 0 || start >= self.n {
            return;
        }
        let end = (start + count).min(self.n);
        let mut l = self.offset + start;
        let mut r = self.offset + end - 1;

        while l > 1 {
            l /= 2;
            r /= 2;
            for i in l..=r {
                self.tree[i] = merge(self.tree[i * 2], self.tree[i * 2 + 1]);
            }
        }
    }

    fn find_first_fit(&self, k: usize) -> Option<usize> {
        if self.n == 0 || k == 0 {
            return None;
        }
        if self.tree[1].max_free < k {
            return None;
        }
        self.find_in_subtree(1, k)
    }

    fn find_in_subtree(&self, node: usize, k: usize) -> Option<usize> {
        if node >= self.offset {
            let page = node - self.offset;
            if self.tree[node].max_free >= k {
                return Some(page);
            }
            return None;
        }

        let left = node * 2;
        let right = node * 2 + 1;

        if self.tree[left].max_free >= k {
            return self.find_in_subtree(left, k);
        }

        let cross = self.tree[left].right_free + self.tree[right].left_free;
        if cross >= k {
            let cross_start = self.node_page_range(left).1.saturating_sub(self.tree[left].right_free);
            return Some(cross_start);
        }

        if self.tree[right].max_free >= k {
            return self.find_in_subtree(right, k);
        }

        None
    }

    fn node_page_range(&self, node: usize) -> (usize, usize) {
        if node >= self.offset {
            let p = node - self.offset;
            return (p, p + 1);
        }
        let (l_start, l_end) = self.node_page_range(node * 2);
        let (r_start, r_end) = self.node_page_range(node * 2 + 1);
        (l_start.min(r_start), l_end.max(r_end))
    }

    // TODO: 当前实现是线性扫描 [low..high]，应利用线段树的区间查询能力
    // 实现 O(log n) 的范围查询，而非 O(n) 逐页扫描。
    fn find_first_fit_in_range(&self, k: usize, low: usize, high: usize) -> Option<usize> {
        if self.n == 0 || k == 0 || low > high {
            return None;
        }
        let high = high.min(self.n - 1);
        if low > high {
            return None;
        }

        let mut best: Option<usize> = None;
        let mut run_start = None;
        let mut run_len = 0usize;

        for page in low..=high {
            let leaf = self.offset + page;
            if self.tree[leaf].max_free > 0 {
                if run_start.is_none() {
                    run_start = Some(page);
                    run_len = 1;
                } else {
                    run_len += 1;
                }
                if run_len >= k {
                    best = run_start;
                    break;
                }
            } else {
                run_start = None;
                run_len = 0;
            }
        }

        best
    }

    pub(crate) fn stats(&self) -> &MemStats {
        &self.stats
    }

    pub(crate) fn total_memory(&self) -> usize {
        self.total_pages * CLICK_SIZE
    }

    pub(crate) fn free_memory(&self) -> usize {
        self.free_pages * CLICK_SIZE
    }

    pub(crate) fn largest_free(&self) -> usize {
        if self.n == 0 { 0 } else { self.tree[1].max_free }
    }

    pub(crate) fn page_is_free(&self, page: usize) -> bool {
        if page >= self.n {
            return false;
        }
        self.tree[self.offset + page].max_free > 0
    }
}

impl PhysMemAlloc for SegmentTreeAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysAddr, AllocError> {
        if clicks == 0 {
            self.stats.record_failure();
            return Err(AllocError::OutOfMemory);
        }

        let params = AllocParams::compute(clicks, flags, self.n);

        let mem = if params.is_low_mem {
            self.find_first_fit_in_range(params.alloc_clicks, 0, params.max_page.saturating_sub(1))
        } else {
            self.find_first_fit(params.alloc_clicks)
        };

        let mem = mem.ok_or_else(|| {
            self.stats.record_failure();
            params.error_type()
        })?;

        if mem + params.alloc_clicks > self.n {
            self.stats.record_failure();
            return Err(AllocError::OutOfMemory);
        }

        let (result_page, leading) = params.aligned_result(mem);

        self.set_range(mem, params.alloc_clicks, false);
        self.free_pages -= params.alloc_clicks;

        if leading > 0 {
            self.set_range(mem, leading, true);
            self.free_pages += leading;
        }

        // TODO(stage-3): 处理 PAF_CLEAR flag。Minix3 在 alloc_pages 末尾调用
        // sys_memset(NONE, 0, CLICK_SIZE*mem, VM_PAGE_SIZE*pages) 清零分配的页。
        // 当前阶段缺少 kernel IPC 基础设施，无法调用 sys_memset。
        // if flags.contains(PageAllocFlags::CLEAR) { ... }

        self.stats.record_alloc(clicks_to_bytes(params.clicks));
        Ok(PhysAddr::from_page_index(result_page))
    }

    fn free_mem(&mut self, base: PhysAddr, clicks: usize) {
        if clicks == 0 {
            return;
        }
        let start_page = base.page_index();
        let end_page = (start_page + clicks).min(self.n);
        let actual_clicks = end_page - start_page;
        if actual_clicks > 0 {
            self.set_range(start_page, actual_clicks, true);
            self.free_pages += actual_clicks;
        }
        self.stats.record_free(clicks_to_bytes(clicks));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_regions() -> Vec<BootMemRegion> {
        vec![BootMemRegion { base: 0, size: 128 * 1024 * 1024 }]
    }

    #[test]
    fn test_alloc_free_basic() {
        let mut alloc = SegmentTreeAllocator::init(&make_test_regions());

        let addr = alloc.alloc_mem(4, PageAllocFlags::empty()).unwrap();
        alloc.free_mem(addr, 4);
    }

    #[test]
    fn test_alloc_zero_clicks() {
        let mut alloc = SegmentTreeAllocator::init(&make_test_regions());
        assert!(alloc.alloc_mem(0, PageAllocFlags::empty()).is_err());
    }

    #[test]
    fn test_alloc_exhaustion() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * CLICK_SIZE }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let a = alloc.alloc_mem(4, PageAllocFlags::empty());
        assert!(a.is_ok());

        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());

        alloc.free_mem(a.unwrap(), 4);

        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }

    #[test]
    fn test_free_and_realloc() {
        let regions = vec![BootMemRegion { base: 0, size: 20 * CLICK_SIZE }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let a = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 10);

        let c = alloc.alloc_mem(10, PageAllocFlags::empty()).unwrap();
        assert_eq!(c.page_index(), a.page_index());

        alloc.free_mem(b, 10);
        alloc.free_mem(c, 10);
    }

    #[test]
    fn test_contiguous_allocation() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let a = alloc.alloc_mem(10, PageAllocFlags::CONTIG).unwrap();
        let start = a.page_index();
        for i in 0..10 {
            assert!(!alloc.page_is_free(start + i));
        }

        alloc.free_mem(a, 10);
    }

    #[test]
    fn test_lower16mb_flag() {
        let regions = vec![BootMemRegion { base: 0, size: 32 * 1024 * 1024 }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let addr = alloc.alloc_mem(1, PageAllocFlags::LOWER16MB).unwrap();
        assert!(addr.as_usize() < 16 * 1024 * 1024);

        alloc.free_mem(addr, 1);
    }

    #[test]
    fn test_align64k_flag() {
        let regions = vec![BootMemRegion { base: 0, size: 4 * 1024 * 1024 }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let addr = alloc.alloc_mem(1, PageAllocFlags::ALIGN64K).unwrap();
        assert_eq!(addr.as_usize() % (64 * 1024), 0);

        alloc.free_mem(addr, 1);
    }

    #[test]
    fn test_largest_free() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        assert_eq!(alloc.largest_free(), 100);

        let a = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        assert_eq!(alloc.largest_free(), 50);

        alloc.free_mem(a, 50);
        assert_eq!(alloc.largest_free(), 100);
    }

    #[test]
    fn test_fragmentation() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let a = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();
        let c = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();
        let d = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 25);
        alloc.free_mem(c, 25);

        assert_eq!(alloc.largest_free(), 25);

        let e = alloc.alloc_mem(25, PageAllocFlags::empty()).unwrap();
        assert_eq!(e.page_index(), a.page_index());

        alloc.free_mem(b, 25);
        alloc.free_mem(d, 25);
        alloc.free_mem(e, 25);
    }

    #[test]
    fn test_merge_on_free() {
        let regions = vec![BootMemRegion { base: 0, size: 100 * CLICK_SIZE }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let a = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();
        let b = alloc.alloc_mem(50, PageAllocFlags::empty()).unwrap();

        alloc.free_mem(a, 50);
        assert_eq!(alloc.largest_free(), 50);

        alloc.free_mem(b, 50);
        assert_eq!(alloc.largest_free(), 100);
    }

    #[test]
    fn test_multiple_regions() {
        let regions = vec![
            BootMemRegion { base: 0x100000, size: 4 * 1024 * 1024 },
            BootMemRegion { base: 0x10000000, size: 8 * 1024 * 1024 },
        ];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());

        alloc.free_mem(a.unwrap(), 1);
    }

    #[test]
    fn test_single_page() {
        let regions = vec![BootMemRegion { base: 0, size: CLICK_SIZE }];
        let mut alloc = SegmentTreeAllocator::init(&regions);

        let a = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(a.is_ok());

        let b = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(b.is_err());

        alloc.free_mem(a.unwrap(), 1);

        let c = alloc.alloc_mem(1, PageAllocFlags::empty());
        assert!(c.is_ok());
    }
}
