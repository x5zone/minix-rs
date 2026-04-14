//! AVL 树实现 - 用于管理虚拟区域
//!
//! AVL 树是一种自平衡二叉搜索树，用于高效地查找、插入和删除虚拟区域。
//! 按键（虚拟地址）排序，支持 O(log n) 的操作复杂度。
//!
//! 对应 Minix3: `region.c` 中的 AVL 树操作

use super::vir_region::VirRegion;
use minix_types::VirBytes;
use std::cmp::Ordering;

/// AVL 搜索类型
///
/// 对应 Minix3: `avl_search_type` 枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchType(u8);

impl SearchType {
    /// 精确匹配键值
    pub const EQUAL: Self = Self(1);
    /// 小于指定键
    pub const LESS: Self = Self(2);
    /// 大于指定键
    pub const GREATER: Self = Self(4);
    /// 小于等于
    pub const LESS_EQUAL: Self = Self(3);
    /// 大于等于
    pub const GREATER_EQUAL: Self = Self(5);

    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

impl Default for SearchType {
    fn default() -> Self {
        Self::EQUAL
    }
}

/// AVL 树结构
///
/// 管理进程的虚拟区域集合，按虚拟地址排序。
#[derive(Debug, Default)]
pub struct RegionAvl {
    /// 树根节点
    root: Option<Box<VirRegion>>,
    /// 节点数量
    count: usize,
}

impl RegionAvl {
    /// 创建新的空 AVL 树
    pub fn new() -> Self {
        Self {
            root: None,
            count: 0,
        }
    }

    /// 获取节点数量
    pub fn len(&self) -> usize {
        self.count
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// 查找包含指定地址的区域
    ///
    /// 这是最常用的查找操作，用于：
    /// - 缺页处理：查找发生缺页的区域
    /// - 内存访问：验证地址是否在有效区域内
    ///
    /// 时间复杂度: O(log n)
    pub fn find(&self, addr: VirBytes) -> Option<&VirRegion> {
        Self::find_containing(&self.root, addr)
    }

    fn find_containing(node: &Option<Box<VirRegion>>, addr: VirBytes) -> Option<&VirRegion> {
        let n = node.as_ref()?;

        if addr < n.vaddr {
            Self::find_containing(&n.lower, addr)
        } else if addr >= n.end_addr() {
            Self::find_containing(&n.higher, addr)
        } else {
            Some(n)
        }
    }

    /// 查找指定地址的区域（可变）
    pub fn find_mut(&mut self, addr: VirBytes) -> Option<&mut VirRegion> {
        Self::find_containing_mut(&mut self.root, addr)
    }

    fn find_containing_mut(
        node: &mut Option<Box<VirRegion>>,
        addr: VirBytes,
    ) -> Option<&mut VirRegion> {
        let n = node.as_mut()?;

        if addr < n.vaddr {
            Self::find_containing_mut(&mut n.lower, addr)
        } else if addr >= n.end_addr() {
            Self::find_containing_mut(&mut n.higher, addr)
        } else {
            Some(n)
        }
    }

    /// 通用搜索函数
    ///
    /// 支持多种搜索类型，对应 Minix3 的 `region_search`
    pub fn search(&self, key: VirBytes, st: SearchType) -> Option<&VirRegion> {
        Self::search_node(self.root.as_ref(), key, st)
    }

    fn search_node(
        mut node: Option<&Box<VirRegion>>,
        key: VirBytes,
        st: SearchType,
    ) -> Option<&VirRegion> {
        let target_cmp = if st.contains(SearchType::LESS) {
            1i32
        } else if st.contains(SearchType::GREATER) {
            -1i32
        } else {
            0i32
        };

        let mut match_h: Option<&VirRegion> = None;

        while let Some(h) = node {
            let cmp = key.0.cmp(&h.vaddr.0);

            if cmp == Ordering::Equal {
                if st.contains(SearchType::EQUAL) {
                    return Some(h);
                }
                return if target_cmp < 0 {
                    Self::search_node(h.lower.as_ref(), key, st)
                } else {
                    Self::search_node(h.higher.as_ref(), key, st)
                };
            }

            let cmp_val = if cmp == Ordering::Less { -1i32 } else { 1i32 };
            if target_cmp != 0 && (cmp_val ^ target_cmp) >= 0 {
                match_h = Some(h);
            }

            node = if cmp == Ordering::Less {
                h.lower.as_ref()
            } else {
                h.higher.as_ref()
            };
        }

        match_h
    }

    /// 查找小于指定键的最大区域
    pub fn find_less(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS)
    }

    /// 查找大于指定键的最小区域
    pub fn find_greater(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER)
    }

    /// 查找小于等于指定键的最大区域
    pub fn find_less_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::LESS_EQUAL)
    }

    /// 查找大于等于指定键的最小区域
    pub fn find_greater_equal(&self, key: VirBytes) -> Option<&VirRegion> {
        self.search(key, SearchType::GREATER_EQUAL)
    }

    /// 查找与指定范围重叠的任意区域
    ///
    /// 用于 mmap 检查新区域是否与现有区域冲突。
    /// 重叠条件: region.vaddr < end && region.end_addr() > start
    ///
    /// 时间复杂度: O(log n) 平均
    pub fn find_overlap(&self, start: VirBytes, end: VirBytes) -> Option<&VirRegion> {
        Self::find_overlap_node(self.root.as_ref(), start, end)
    }

    fn find_overlap_node(
        node: Option<&Box<VirRegion>>,
        start: VirBytes,
        end: VirBytes,
    ) -> Option<&VirRegion> {
        let n = node?;

        if n.vaddr < end && n.end_addr() > start {
            return Some(n);
        }

        if end <= n.vaddr {
            Self::find_overlap_node(n.lower.as_ref(), start, end)
        } else if start >= n.end_addr() {
            Self::find_overlap_node(n.higher.as_ref(), start, end)
        } else {
            None
        }
    }

    /// 查找所有与指定范围重叠的区域
    pub fn find_all_overlaps<'a>(
        &'a self,
        start: VirBytes,
        end: VirBytes,
    ) -> impl Iterator<Item = &'a VirRegion> {
        self.iter().filter(move |r| r.overlaps(start, end))
    }

    /// 在指定范围内查找足够大的空闲槽位
    ///
    /// 对应 Minix3: `region_find_slot_range`
    ///
    /// 参数:
    /// - minv: 最小起始地址
    /// - maxv: 最大结束地址（0 表示使用 minv + length）
    /// - length: 需要的空间大小
    ///
    /// 返回: 可用起始地址，或 None 表示无合适空间
    pub fn find_slot(
        &self,
        minv: VirBytes,
        maxv: VirBytes,
        length: VirBytes,
    ) -> Option<VirBytes> {
        if length.0 == 0 {
            return None;
        }

        let maxv = if maxv.0 == 0 {
            VirBytes(minv.0.checked_add(length.0)?)
        } else {
            maxv
        };

        if minv.0 >= maxv.0 || minv.0.checked_add(length.0)? > maxv.0 {
            return None;
        }

        let mut prev_end = minv;

        for region in self.iter() {
            if region.vaddr >= prev_end {
                let gap_start = prev_end.max(minv);
                let gap_end = region.vaddr.min(maxv);

                if gap_end.0 > gap_start.0
                    && gap_end.0.saturating_sub(gap_start.0) >= length.0
                {
                    return Some(VirBytes(gap_end.0 - length.0));
                }
            }

            prev_end = region.end_addr().max(prev_end);
        }

        let gap_start = prev_end.max(minv);
        if maxv.0 > gap_start.0
            && maxv.0.saturating_sub(gap_start.0) >= length.0
        {
            return Some(VirBytes(maxv.0 - length.0));
        }

        None
    }

    /// 插入区域
    pub fn insert(&mut self, region: VirRegion) {
        let was_inserted = Self::insert_node(&mut self.root, region);
        if was_inserted {
            self.count += 1;
        }
    }

    fn insert_node(node: &mut Option<Box<VirRegion>>, mut region: VirRegion) -> bool {
        match node {
            None => {
                *node = Some(Box::new(region));
                true
            }
            Some(n) => {
                if region.vaddr < n.vaddr {
                    Self::insert_node(&mut n.lower, region)
                } else if region.vaddr > n.vaddr {
                    Self::insert_node(&mut n.higher, region)
                } else {
                    region.lower = n.lower.take();
                    region.higher = n.higher.take();
                    region.factor = n.factor;
                    **n = region;
                    false
                }
            }
        }
    }

    /// 删除指定地址的区域
    pub fn remove(&mut self, addr: VirBytes) -> Option<VirRegion> {
        Self::remove_node(&mut self.root, addr).map(|region| {
            self.count -= 1;
            *region
        })
    }

    fn remove_node(node: &mut Option<Box<VirRegion>>, addr: VirBytes) -> Option<Box<VirRegion>> {
        let n = node.as_mut()?;

        if addr < n.vaddr {
            return Self::remove_node(&mut n.lower, addr);
        } else if addr > n.vaddr {
            return Self::remove_node(&mut n.higher, addr);
        }

        let mut removed = node.take().unwrap();

        match (removed.lower.take(), removed.higher.take()) {
            (None, None) => Some(removed),
            (Some(left), None) => {
                *node = Some(left);
                Some(removed)
            }
            (None, Some(right)) => {
                *node = Some(right);
                Some(removed)
            }
            (Some(left), Some(right)) => {
                *node = Some(right);
                let mut current = node.as_mut().unwrap();
                while current.lower.is_some() {
                    current = current.lower.as_mut().unwrap();
                }
                current.lower = Some(left);
                Some(removed)
            }
        }
    }

    /// 遍历所有区域（中序遍历，按地址排序）
    pub fn traverse<F>(&self, mut f: F)
    where
        F: FnMut(&VirRegion),
    {
        Self::traverse_node(&self.root, &mut f);
    }

    fn traverse_node<F>(node: &Option<Box<VirRegion>>, f: &mut F)
    where
        F: FnMut(&VirRegion),
    {
        if let Some(n) = node {
            Self::traverse_node(&n.lower, f);
            f(n);
            Self::traverse_node(&n.higher, f);
        }
    }

    /// 创建中序迭代器
    pub fn iter(&self) -> RegionIter<'_> {
        let mut stack = Vec::new();
        let mut node = self.root.as_ref();
        while let Some(n) = node {
            stack.push(n.as_ref());
            node = n.lower.as_ref();
        }
        RegionIter { stack }
    }

    /// 创建可变迭代器
    pub fn iter_mut(&mut self) -> RegionIterMut<'_> {
        let mut stack = Vec::new();
        let mut node = self.root.as_mut();
        while let Some(n) = node {
            stack.push(n.as_mut() as *mut VirRegion);
            node = n.lower.as_mut();
        }
        RegionIterMut { stack, _marker: std::marker::PhantomData }
    }
}

/// AVL 树中序迭代器
///
/// 按虚拟地址升序遍历所有区域。
/// 对应 Minix3: `region_iter` 结构体
pub struct RegionIter<'a> {
    stack: Vec<&'a VirRegion>,
}

impl<'a> Iterator for RegionIter<'a> {
    type Item = &'a VirRegion;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;

        let mut right = node.higher.as_ref();
        while let Some(r) = right {
            self.stack.push(r.as_ref());
            right = r.lower.as_ref();
        }

        Some(node)
    }
}

/// 可变迭代器
pub struct RegionIterMut<'a> {
    stack: Vec<*mut VirRegion>,
    _marker: std::marker::PhantomData<&'a mut VirRegion>,
}

impl<'a> Iterator for RegionIterMut<'a> {
    type Item = &'a mut VirRegion;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;

        let node_ref = unsafe { &mut *node };
        let mut right = node_ref.higher.as_mut();
        while let Some(r) = right {
            self.stack.push(r.as_mut() as *mut VirRegion);
            right = r.lower.as_mut();
        }

        Some(unsafe { &mut *node })
    }
}

impl<'a> IntoIterator for &'a RegionAvl {
    type Item = &'a VirRegion;
    type IntoIter = RegionIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::vir_region::{VrFlags, VirRegion};

    fn make_region(vaddr: u64, length: u64) -> VirRegion {
        VirRegion::new(VirBytes(vaddr), VirBytes(length), VrFlags::empty())
    }

    #[test]
    fn test_avl_insert_and_find() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));

        assert_eq!(avl.len(), 3);

        let found = avl.find(VirBytes(0x1500));
        assert!(found.is_some());
        assert_eq!(found.unwrap().vaddr, VirBytes(0x1000));

        assert!(avl.find(VirBytes(0x5000)).is_none());
    }

    #[test]
    fn test_avl_remove() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        assert_eq!(avl.len(), 3);

        avl.remove(VirBytes(0x2000));
        assert_eq!(avl.len(), 2);
        assert!(avl.find(VirBytes(0x2000)).is_none());
        assert!(avl.find(VirBytes(0x1000)).is_some());
        assert!(avl.find(VirBytes(0x3000)).is_some());
    }

    #[test]
    fn test_avl_find_overlap() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let overlap = avl.find_overlap(VirBytes(0x1500), VirBytes(0x2500));
        assert!(overlap.is_some());
        assert_eq!(overlap.unwrap().vaddr, VirBytes(0x1000));

        assert!(avl.find_overlap(VirBytes(0x5000), VirBytes(0x6000)).is_none());
    }

    #[test]
    fn test_avl_traverse() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));

        let mut addrs = Vec::new();
        avl.traverse(|r| addrs.push(r.vaddr));

        assert_eq!(
            addrs,
            vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_avl_iter() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));

        let addrs: Vec<_> = avl.iter().map(|r| r.vaddr).collect();

        assert_eq!(
            addrs,
            vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_search_type_less() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x5000, 0x1000));

        let result = avl.find_less(VirBytes(0x4000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = avl.find_less(VirBytes(0x1000));
        assert!(result.is_none());
    }

    #[test]
    fn test_search_type_greater() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));
        avl.insert(make_region(0x5000, 0x1000));

        let result = avl.find_greater(VirBytes(0x2000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = avl.find_greater(VirBytes(0x5000));
        assert!(result.is_none());
    }

    #[test]
    fn test_search_type_less_equal() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let result = avl.find_less_equal(VirBytes(0x3000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));

        let result = avl.find_less_equal(VirBytes(0x2500));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x1000));
    }

    #[test]
    fn test_search_type_greater_equal() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let result = avl.find_greater_equal(VirBytes(0x1000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x1000));

        let result = avl.find_greater_equal(VirBytes(0x2000));
        assert!(result.is_some());
        assert_eq!(result.unwrap().vaddr, VirBytes(0x3000));
    }

    #[test]
    fn test_find_slot_basic() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let slot = avl.find_slot(VirBytes(0), VirBytes(0x10000), VirBytes(0x800));
        assert!(slot.is_some());
        assert!(slot.unwrap().0 >= 0 && slot.unwrap().0 + 0x800 <= 0x10000);
    }

    #[test]
    fn test_find_slot_in_gap() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x0000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let slot = avl.find_slot(VirBytes(0x1000), VirBytes(0x3000), VirBytes(0x800));
        assert!(slot.is_some());
        let addr = slot.unwrap();
        assert!(addr.0 >= 0x1000);
        assert!(addr.0 + 0x800 <= 0x3000);
    }

    #[test]
    fn test_find_slot_no_space() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x0000, 0x1000));
        avl.insert(make_region(0x1000, 0x1000));

        let slot = avl.find_slot(VirBytes(0), VirBytes(0x2000), VirBytes(0x800));
        assert!(slot.is_none());
    }

    #[test]
    fn test_find_all_overlaps() {
        let mut avl = RegionAvl::new();

        avl.insert(make_region(0x1000, 0x1000));
        avl.insert(make_region(0x2000, 0x1000));
        avl.insert(make_region(0x3000, 0x1000));

        let overlaps: Vec<_> = avl
            .find_all_overlaps(VirBytes(0x1500), VirBytes(0x3500))
            .map(|r| r.vaddr)
            .collect();

        assert_eq!(
            overlaps,
            vec![VirBytes(0x1000), VirBytes(0x2000), VirBytes(0x3000)]
        );
    }

    #[test]
    fn test_search_type_flags() {
        assert!(SearchType::LESS_EQUAL.contains(SearchType::EQUAL));
        assert!(SearchType::LESS_EQUAL.contains(SearchType::LESS));
        assert!(!SearchType::LESS_EQUAL.contains(SearchType::GREATER));

        assert!(SearchType::GREATER_EQUAL.contains(SearchType::EQUAL));
        assert!(SearchType::GREATER_EQUAL.contains(SearchType::GREATER));
        assert!(!SearchType::GREATER_EQUAL.contains(SearchType::LESS));
    }
}
