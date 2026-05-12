use super::alloc_trait::{PhysAllocator, PhysAllocatorStats, PhysMemStats};
use super::types::{AllocError, PageAllocFlags, AlignedPhysBytes};
use super::{BumpBuf, BootMemRegion, METADATA_ALIGN_PADDING};

#[cfg(feature = "segment_tree_alloc")]
#[derive(Debug, Clone, Copy)]
struct SegmentNode {
    max_free: usize,
    left_free: usize,
    right_free: usize,
    len: usize,
}

#[cfg(feature = "segment_tree_alloc")]
impl SegmentNode {
    const fn empty() -> Self {
        SegmentNode { max_free: 0, left_free: 0, right_free: 0, len: 0 }
    }

    const fn free(len: usize) -> Self {
        SegmentNode { max_free: len, left_free: len, right_free: len, len }
    }

    const fn used(len: usize) -> Self {
        SegmentNode { max_free: 0, left_free: 0, right_free: 0, len }
    }
}

#[cfg(feature = "segment_tree_alloc")]
fn merge(left: SegmentNode, right: SegmentNode) -> SegmentNode {
    let len = left.len + right.len;
    if len == 0 {
        return SegmentNode::empty();
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

    SegmentNode { max_free, left_free, right_free, len }
}

#[cfg(feature = "segment_tree_alloc")]
pub struct SegmentTreeAllocator {
    n: usize,
    offset: usize,
    tree: &'static mut [SegmentNode],
    total_pages: usize,
    free_pages: usize,
    stats: MemStats,
}

#[cfg(feature = "segment_tree_alloc")]
impl SegmentTreeAllocator {
    pub fn init(metadata: &mut [u8], total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
        let n = total_pages;
        let offset = n.next_power_of_two();
        let tree_size = if n > 0 { 2 * offset } else { 2 };

        let mut buf = BumpBuf::new(metadata);
        let tree = buf.alloc_slice::<SegmentNode>(tree_size);

        for node in tree.iter_mut() {
            *node = SegmentNode::used(1);
        }

        let mut alloc = Self {
            n,
            offset,
            tree,
            total_pages,
            free_pages: 0,
            stats: MemStats::new(),
        };

        for region in free_regions {
            if region.size == 0 {
                continue;
            }
            region.validate();
            let base_page = region.base / CLICK_SIZE;
            let num_pages = region.size / CLICK_SIZE;
            for i in base_page..base_page + num_pages {
                if i < n {
                    alloc.tree[offset + i] = SegmentNode::free(1);
                }
            }
            alloc.free_pages += num_pages;
        }

        for i in (1..offset).rev() {
            alloc.tree[i] = merge(alloc.tree[i * 2], alloc.tree[i * 2 + 1]);
        }

        alloc
    }

    pub fn metadata_size(total_pages: usize) -> usize {
        let offset = if total_pages == 0 {
            1
        } else {
            total_pages.next_power_of_two()
        };
        let tree_size = if total_pages > 0 { 2 * offset } else { 2 };
        tree_size * core::mem::size_of::<SegmentNode>() + METADATA_ALIGN_PADDING
    }

    pub fn total_memory(&self) -> usize {
        self.total_pages * CLICK_SIZE
    }

    pub fn free_memory(&self) -> usize {
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

    fn pull_up(&mut self, mut idx: usize) {
        while idx > 1 {
            idx /= 2;
            self.tree[idx] = merge(self.tree[idx * 2], self.tree[idx * 2 + 1]);
        }
    }

    fn set_range(&mut self, start: usize, count: usize, free: bool) {
        for i in start..start + count {
            if i < self.n {
                let leaf = self.offset + i;
                self.tree[leaf] = if free { SegmentNode::free(1) } else { SegmentNode::used(1) };
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
}

#[cfg(feature = "segment_tree_alloc")]
impl PhysAllocator for SegmentTreeAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<AlignedPhysBytes, AllocError> {
        if clicks == 0 {
            return Err(AllocError::OutOfMemory);
        }

        let mut alloc_clicks = clicks;
        let mut align_clicks = 0usize;

        if flags.contains(PageAllocFlags::ALIGN64K) {
            align_clicks = (64 * 1024) / CLICK_SIZE;
            alloc_clicks += align_clicks;
        } else if flags.contains(PageAllocFlags::ALIGN16K) {
            align_clicks = (16 * 1024) / CLICK_SIZE;
            alloc_clicks += align_clicks;
        }

        let max_page = if flags.contains(PageAllocFlags::LOWER1MB) {
            (1 * 1024 * 1024) / CLICK_SIZE
        } else if flags.contains(PageAllocFlags::LOWER16MB) {
            (16 * 1024 * 1024) / CLICK_SIZE
        } else {
            self.total_pages
        };

        let mem = match self.find_first_fit(alloc_clicks) {
            Some(m) => m,
            None => {
                self.stats.record_failure();
                return Err(super::oom_error(flags));
            }
        };

        if mem + alloc_clicks > max_page {
            self.stats.record_failure();
            return Err(AllocError::LowMemoryExhausted);
        }

        self.set_range(mem, alloc_clicks, false);
        self.free_pages -= alloc_clicks;

        // Handle alignment: over-allocated to guarantee an aligned boundary.
        // Free the unused prefix or suffix; no record_free since these are
        // internal bookkeeping, not logical allocations.
        if align_clicks > 0 {
            let offset = mem % align_clicks;
            if offset > 0 {
                let excess = align_clicks - offset;
                self.set_range(mem, excess, true);
                self.free_pages += excess;
                let aligned_mem = mem + excess;
                self.stats.record_alloc(clicks * CLICK_SIZE);
                return Ok(AlignedPhysBytes::from_page_index(aligned_mem));
            } else {
                self.set_range(mem + clicks, align_clicks, true);
                self.free_pages += align_clicks;
            }
        }

        if flags.contains(PageAllocFlags::CLEAR) {
            let virt = crate::direct_map::vm_phys_to_virt(AlignedPhysBytes::from_page_index(mem));
            unsafe {
                let ptr = virt.0 as *mut u64;
                let words = clicks * CLICK_SIZE / 8;
                for i in 0..words {
                    core::ptr::write_volatile(ptr.add(i), 0);
                }
            }
        }

        self.stats.record_alloc(clicks * CLICK_SIZE);
        Ok(AlignedPhysBytes::from_page_index(mem))
    }

    fn free_mem(&mut self, base: AlignedPhysBytes, clicks: usize) {
        if clicks == 0 {
            return;
        }
        let start_page = base.page_index();
        let end_page = (start_page + clicks).min(self.n);
        let actual_count = end_page - start_page;
        if actual_count > 0 {
            self.set_range(start_page, actual_count, true);
            self.free_pages += actual_count;
            self.stats.record_free(actual_count * CLICK_SIZE);
        }
    }

    fn total_count(&self) -> usize {
        self.total_pages
    }

    fn reserve_pages(&mut self, base_page: usize, count: usize) {
        let mut reserved = 0usize;
        let end = (base_page + count).min(self.n);
        for i in base_page..end {
            if self.page_is_free(i) {
                self.tree[self.offset + i] = SegmentNode::used(1);
                self.free_pages -= 1;
                reserved += 1;
            }
        }
        for i in (1..self.offset).rev() {
            self.tree[i] = merge(self.tree[i * 2], self.tree[i * 2 + 1]);
        }
        if reserved > 0 {
            self.stats.record_alloc(reserved * CLICK_SIZE);
        }
    }
}

#[cfg(feature = "segment_tree_alloc")]
impl PhysAllocatorStats for SegmentTreeAllocator {
    fn memstats(&self) -> PhysMemStats {
        PhysMemStats {
            free_nodes: 0,
            free_pages: self.free_pages,
            largest_free: self.largest_free(),
        }
    }
}

#[cfg(not(feature = "segment_tree_alloc"))]
pub struct SegmentTreeAllocator {
    _private: (),
}

#[cfg(not(feature = "segment_tree_alloc"))]
impl SegmentTreeAllocator {
    pub fn init(_metadata: &mut [u8], _total_pages: usize, _free_regions: &[BootMemRegion]) -> Self {
        unreachable!("SegmentTreeAllocator requires 'segment_tree_alloc' feature flag")
    }

    pub fn metadata_size(_total_pages: usize) -> usize {
        0
    }

    pub(crate) fn largest_free(&self) -> usize {
        0
    }

    pub(crate) fn page_is_free(&self, _page: usize) -> bool {
        false
    }
}

#[cfg(not(feature = "segment_tree_alloc"))]
impl PhysAllocator for SegmentTreeAllocator {
    fn alloc_mem(&mut self, _clicks: usize, _flags: PageAllocFlags) -> Result<AlignedPhysBytes, AllocError> {
        unreachable!("SegmentTreeAllocator requires 'segment_tree_alloc' feature flag")
    }

    fn free_mem(&mut self, _base: AlignedPhysBytes, _clicks: usize) {
        unreachable!("SegmentTreeAllocator requires 'segment_tree_alloc' feature flag")
    }

    fn total_count(&self) -> usize {
        0
    }

    fn reserve_pages(&mut self, _base_page: usize, _count: usize) {
        unreachable!("SegmentTreeAllocator requires 'segment_tree_alloc' feature flag")
    }
}

#[cfg(not(feature = "segment_tree_alloc"))]
impl PhysAllocatorStats for SegmentTreeAllocator {
    fn memstats(&self) -> PhysMemStats {
        PhysMemStats { free_nodes: 0, free_pages: 0, largest_free: 0 }
    }
}
