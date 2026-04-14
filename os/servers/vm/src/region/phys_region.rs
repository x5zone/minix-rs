//! 物理区域 (phys_region) 实现
//!
//! 连接虚拟区域和物理块的桥梁，管理虚拟到物理的映射关系。
//! 对应 Minix3: `phys_region.h`

use super::vir_region::VirRegion;
use minix_types::VirBytes;
use crate::memtype::MemType;

/// 页表标志
///
/// 对应 Minix3: `PTF_*` 宏定义
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtFlags {
    /// 只读
    ReadOnly,
    /// 可写
    Writable,
    /// 可执行
    Executable,
    /// 用户模式
    User,
    /// 存在
    Present,
}

/// Mock 页表实现
///
/// 用于测试和开发阶段，模拟页表操作。
/// 对应 Minix3: `pt_writemap()` 等函数
pub struct MockPageTable;

impl MockPageTable {
    /// 设置页表标志
    ///
    /// # Safety
    ///
    /// 这是一个 mock 实现，仅用于测试。
    /// 实际实现需要硬件支持。
    pub fn set_page_flags(_vaddr: u64, _paddr: u64, _flags: &[PtFlags]) {
        // Mock 实现：不做任何实际操作
        // 实际实现需要：
        // 1. 找到对应的页表项
        // 2. 清除旧标志
        // 3. 设置新标志
        // 4. 刷新 TLB
    }

    /// 获取页表标志
    ///
    /// # Safety
    ///
    /// 这是一个 mock 实现，仅用于测试。
    pub fn get_page_flags(_vaddr: u64) -> Vec<PtFlags> {
        // Mock 实现：返回空列表
        Vec::new()
    }
}

/// 物理块 (phys_block)
///
/// 代表一段连续的物理内存，可以被多个物理区域引用（共享内存）。
/// 对应 Minix3: `struct phys_block`
#[derive(Debug)]
pub struct PhysBlock {
    /// 物理内存起始地址
    pub phys: u64,

    /// 引用计数
    ///
    /// 记录有多少个 phys_region 引用这个物理块。
    /// 当 refcount 降为 0 时，物理内存可以被释放。
    pub refcount: u8,

    /// 物理块标志
    pub flags: PhysBlockFlags,

    /// 引用此物理块的第一个物理区域
    ///
    /// 用于遍历所有引用此物理块的区域（CoW 时需要）。
    pub first_region: Option<*mut PhysRegion>,
}

impl PhysBlock {
    /// 创建新的物理块
    ///
    /// 注意：初始引用计数为 0，需要通过 pb_link() 关联到 phys_region 后才会增加。
    /// 对应 Minix3: `pb_new()`
    pub fn new(phys: u64) -> Self {
        Self {
            phys,
            refcount: 0,
            flags: PhysBlockFlags::empty(),
            first_region: None,
        }
    }

    /// 增加引用计数
    pub fn add_ref(&mut self) {
        self.refcount = self.refcount.saturating_add(1);
    }

    /// 减少引用计数，返回是否还有引用
    pub fn release_ref(&mut self) -> bool {
        if self.refcount > 0 {
            self.refcount -= 1;
        }
        self.refcount > 0
    }

    /// 检查是否在缓存中
    pub fn is_in_cache(&self) -> bool {
        self.flags.contains(PhysBlockFlags::IN_CACHE)
    }
}

bitflags::bitflags! {
    /// 物理块标志
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PhysBlockFlags: u8 {
        /// 在缓存中
        const IN_CACHE = 0x01;
    }
}

/// 物理区域 (phys_region)
///
/// 连接虚拟区域和物理块的桥梁。每个 phys_region 代表 vir_region 中的一段
/// 虚拟地址到物理地址的映射。
///
/// 对应 Minix3: `struct phys_region`
pub struct PhysRegion {
    /// 指向物理块
    pub ph: Option<*mut PhysBlock>,

    /// 父虚拟区域
    pub parent: Option<*mut VirRegion>,

    /// 在虚拟区域中的偏移量（字节）
    pub offset: VirBytes,

    /// 内存类型回调
    ///
    /// 指向内存类型处理器，用于处理特定类型的操作（如 CoW、页错误等）。
    /// 对应 Minix3: `pr->memtype`
    ///
    /// TODO: 完整实现（待 10-memtype.md 文档完善）
    pub memtype: Option<&'static dyn MemType>,

    /// 同一物理块的其他引用者链表
    ///
    /// 用于实现 CoW（写时复制）：当需要写入时，遍历链表将所有引用者
    /// 的映射改为只读。
    pub next_ph_list: Option<*mut PhysRegion>,
}

impl core::fmt::Debug for PhysRegion {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PhysRegion")
            .field("ph", &self.ph)
            .field("parent", &self.parent)
            .field("offset", &self.offset)
            .field("memtype", &self.memtype.map(|m| m.name()))
            .field("next_ph_list", &self.next_ph_list)
            .finish()
    }
}

impl PhysRegion {
    /// 创建新的物理区域
    pub fn new(offset: VirBytes) -> Self {
        Self {
            ph: None,
            parent: None,
            offset,
            memtype: None,
            next_ph_list: None,
        }
    }

    /// 创建带有内存类型的物理区域
    pub fn with_memtype(offset: VirBytes, memtype: &'static dyn MemType) -> Self {
        Self {
            ph: None,
            parent: None,
            offset,
            memtype: Some(memtype),
            next_ph_list: None,
        }
    }

    /// 绑定物理块
    pub fn bind_block(&mut self, block: *mut PhysBlock) {
        self.ph = Some(block);
        unsafe {
            (*block).add_ref();
        }
    }

    /// 解绑物理块，返回是否还有其他引用
    pub fn unbind_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            unsafe {
                let has_more_refs = (*block).release_ref();
                self.ph = None;
                has_more_refs
            }
        } else {
            false
        }
    }

    /// 获取物理地址
    pub fn get_phys_addr(&self) -> Option<u64> {
        self.ph.map(|block| unsafe { (*block).phys })
    }

    /// 检查是否有物理块
    ///
    /// ph 为 NULL 表示该虚拟页尚未分配物理内存（延迟分配）
    pub fn has_phys_block(&self) -> bool {
        self.ph.is_some()
    }

    /// 获取引用计数
    ///
    /// 对应 Minix3: `pr->ph->refcount`
    /// 用于判断是否需要 CoW
    pub fn get_refcount(&self) -> Option<u8> {
        self.ph.map(|block| unsafe { (*block).refcount })
    }

    /// 检查是否可写
    ///
    /// 对应 Minix3: `anon_writable()`
    /// 只有引用计数为 1 时才可写（独占）
    pub fn is_writable(&self) -> bool {
        match self.get_refcount() {
            Some(1) => true,
            _ => false,
        }
    }

    /// 检查是否需要 CoW
    ///
    /// 引用计数 > 1 时需要 CoW
    pub fn needs_cow(&self) -> bool {
        match self.get_refcount() {
            Some(count) if count > 1 => true,
            _ => false,
        }
    }

    /// 获取物理块标志
    ///
    /// 对应 Minix3: `pr->ph->flags`
    pub fn get_block_flags(&self) -> Option<PhysBlockFlags> {
        self.ph.map(|block| unsafe { (*block).flags })
    }

    /// 检查物理块是否在缓存中
    pub fn is_in_cache(&self) -> bool {
        self.get_block_flags()
            .map(|flags| flags.contains(PhysBlockFlags::IN_CACHE))
            .unwrap_or(false)
    }

    /// 计算虚拟地址
    ///
    /// 虚拟地址 = vir_region.vaddr + offset
    /// 对应 Minix3: `vr->vaddr + pr->offset`
    pub fn get_virtual_addr(&self) -> Option<VirBytes> {
        self.parent.map(|parent| unsafe {
            VirBytes((*parent).vaddr.0 + self.offset.0)
        })
    }

    /// 获取页索引
    ///
    /// 页索引 = offset / PAGE_SIZE
    /// 用于在 vir_region.physblocks 数组中定位
    pub fn get_page_index(&self) -> usize {
        const PAGE_SIZE: u64 = 4096;
        (self.offset.0 / PAGE_SIZE) as usize
    }

    /// 验证 offset 的约束条件
    ///
    /// 1. 必须是页大小的倍数
    /// 2. 必须在 vir_region.length 范围内
    /// 3. 必须与数组索引一致
    pub fn validate_offset(&self, region_length: VirBytes) -> bool {
        const PAGE_SIZE: u64 = 4096;
        
        let offset = self.offset.0;
        
        if offset % PAGE_SIZE != 0 {
            return false;
        }
        
        if offset >= region_length.0 {
            return false;
        }
        
        true
    }

    /// 从虚拟地址计算 offset
    ///
    /// offset = virtual_addr - vaddr
    /// 对应 Minix3: `ph = offset - r->vaddr`
    pub fn offset_from_vaddr(virtual_addr: VirBytes, region_vaddr: VirBytes) -> VirBytes {
        VirBytes(virtual_addr.0 - region_vaddr.0)
    }

    /// 从页索引计算 offset
    ///
    /// offset = page_index * PAGE_SIZE
    pub fn offset_from_page_index(page_index: usize) -> VirBytes {
        const PAGE_SIZE: u64 = 4096;
        VirBytes((page_index as u64) * PAGE_SIZE)
    }

    /// 插入到物理块的引用链表（头插法）
    ///
    /// 对应 Minix3: `pb_link()`
    ///
    /// # Safety
    ///
    /// 调用者必须确保：
    /// - `block` 指针有效
    /// - 当前 `PhysRegion` 未在其他链表中（`self.ph.is_none()`）
    pub fn link_to_block(&mut self, block: *mut PhysBlock, parent: *mut VirRegion) {
        unsafe {
            debug_assert!(!block.is_null(), "block pointer must not be null");
            debug_assert!(self.ph.is_none(), "PhysRegion must not already be in a list");

            self.ph = Some(block);
            self.parent = Some(parent);

            self.next_ph_list = (*block).first_region;
            (*block).first_region = Some(self as *mut PhysRegion);
            (*block).refcount = (*block).refcount.saturating_add(1);

            debug_assert!((*block).refcount > 0, "refcount must be positive after link");
            debug_assert!(self.ph.is_some(), "ph must be set after link");
        }
    }

    /// 检查当前节点是否在指定物理块的链表中
    ///
    /// 用于调试和验证链表完整性
    fn is_in_list(&self, block: *mut PhysBlock) -> bool {
        unsafe {
            let mut current = (*block).first_region;
            while let Some(ptr) = current {
                if ptr == self as *const PhysRegion as *mut PhysRegion {
                    return true;
                }
                current = (*ptr).next_ph_list;
            }
            false
        }
    }

    /// 从物理块的引用链表中移除
    ///
    /// 对应 Minix3: `pb_unreferenced()`
    /// 返回是否应该释放物理块（refcount == 0）
    ///
    /// # Safety
    ///
    /// 调用者必须确保：
    /// - 当前 `PhysRegion` 在链表中
    /// - 物理块的引用计数大于 0
    pub fn unlink_from_block(&mut self) -> bool {
        if let Some(block) = self.ph {
            unsafe {
                debug_assert!((*block).refcount > 0, "refcount must be positive before unlink");
                debug_assert!(self.is_in_list(block), "PhysRegion must be in the list");

                (*block).refcount = (*block).refcount.saturating_sub(1);

                if (*block).first_region == Some(self as *mut PhysRegion) {
                    (*block).first_region = self.next_ph_list;
                } else if let Some(first) = (*block).first_region {
                    let mut current = first;
                    loop {
                        let next = (*current).next_ph_list;
                        if next == Some(self as *mut PhysRegion) {
                            (*current).next_ph_list = self.next_ph_list;
                            break;
                        }
                        match next {
                            Some(n) => current = n,
                            None => {
                                debug_assert!(false, "PhysRegion not found in list");
                                break;
                            }
                        }
                    }
                }

                self.ph = None;
                self.next_ph_list = None;

                debug_assert!(self.ph.is_none(), "ph must be None after unlink");
                debug_assert!(self.next_ph_list.is_none(), "next_ph_list must be None after unlink");

                (*block).refcount == 0
            }
        } else {
            false
        }
    }

    /// 遍历引用同一物理块的所有物理区域
    ///
    /// 对应 Minix3 中的链表遍历
    pub fn iterate_block_refs<F>(block: &PhysBlock, mut f: F)
    where
        F: FnMut(&PhysRegion),
    {
        let mut current = block.first_region;
        while let Some(ptr) = current {
            unsafe {
                let region = &*ptr;
                f(region);
                current = region.next_ph_list;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phys_block_refcount() {
        let mut block = PhysBlock::new(0x1000);
        assert_eq!(block.refcount, 0);

        block.add_ref();
        assert_eq!(block.refcount, 1);

        block.add_ref();
        assert_eq!(block.refcount, 2);

        assert!(block.release_ref());
        assert_eq!(block.refcount, 1);

        assert!(!block.release_ref());
        assert_eq!(block.refcount, 0);
    }

    #[test]
    fn test_phys_region_bind() {
        use minix_types::VirBytes;
        let mut region = PhysRegion::new(VirBytes(0));
        let mut block = PhysBlock::new(0x1000);

        region.bind_block(&mut block);
        assert_eq!(block.refcount, 1);

        assert!(!region.unbind_block());
        assert_eq!(block.refcount, 0);
    }

    #[test]
    fn test_link_to_block() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));

        let block_ptr = &mut block as *mut PhysBlock;

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            assert_eq!(block.refcount, 1);
            assert_eq!(block.first_region, Some(&mut *region1 as *mut PhysRegion));

            region2.link_to_block(block_ptr, std::ptr::null_mut());
            assert_eq!(block.refcount, 2);
            assert_eq!(block.first_region, Some(&mut *region2 as *mut PhysRegion));
            assert_eq!(region2.next_ph_list, Some(&mut *region1 as *mut PhysRegion));
        }
    }

    #[test]
    fn test_unlink_from_block() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));

        let block_ptr = &mut block as *mut PhysBlock;

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            region2.link_to_block(block_ptr, std::ptr::null_mut());

            assert_eq!(block.refcount, 2);

            let should_free = region2.unlink_from_block();
            assert!(!should_free);
            assert_eq!(block.refcount, 1);
            assert_eq!(block.first_region, Some(&mut *region1 as *mut PhysRegion));

            let should_free = region1.unlink_from_block();
            assert!(should_free);
            assert_eq!(block.refcount, 0);
            assert_eq!(block.first_region, None);
        }
    }

    #[test]
    fn test_iterate_block_refs() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));

        let block_ptr = &mut block as *mut PhysBlock;

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            region2.link_to_block(block_ptr, std::ptr::null_mut());
            region3.link_to_block(block_ptr, std::ptr::null_mut());
        }

        let mut count = 0;
        let mut offsets = Vec::new();
        PhysRegion::iterate_block_refs(&block, |region| {
            count += 1;
            offsets.push(region.offset.0);
        });

        assert_eq!(count, 3);
        assert_eq!(offsets, vec![0x3000, 0x2000, 0x1000]);
    }

    #[test]
    fn test_unlink_middle_node() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = Box::new(PhysRegion::new(VirBytes(0x1000)));
        let mut region2 = Box::new(PhysRegion::new(VirBytes(0x2000)));
        let mut region3 = Box::new(PhysRegion::new(VirBytes(0x3000)));

        let block_ptr = &mut block as *mut PhysBlock;

        unsafe {
            region1.link_to_block(block_ptr, std::ptr::null_mut());
            region2.link_to_block(block_ptr, std::ptr::null_mut());
            region3.link_to_block(block_ptr, std::ptr::null_mut());

            let should_free = region2.unlink_from_block();
            assert!(!should_free);
            assert_eq!(block.refcount, 2);

            let mut count = 0;
            PhysRegion::iterate_block_refs(&block, |_| count += 1);
            assert_eq!(count, 2);
        }
    }

    #[test]
    fn test_offset_calculations() {
        use minix_types::VirBytes;
        
        let region = PhysRegion::new(VirBytes(0x2000));
        
        assert_eq!(region.get_page_index(), 2);
        
        let vaddr = PhysRegion::offset_from_vaddr(VirBytes(0x401500), VirBytes(0x400000));
        assert_eq!(vaddr, VirBytes(0x1500));
        
        let offset = PhysRegion::offset_from_page_index(5);
        assert_eq!(offset, VirBytes(0x5000));
    }

    #[test]
    fn test_validate_offset() {
        use minix_types::VirBytes;
        
        let region1 = PhysRegion::new(VirBytes(0x1000));
        assert!(region1.validate_offset(VirBytes(0x3000)));
        
        let region2 = PhysRegion::new(VirBytes(0x1001));
        assert!(!region2.validate_offset(VirBytes(0x3000)));
        
        let region3 = PhysRegion::new(VirBytes(0x4000));
        assert!(!region3.validate_offset(VirBytes(0x3000)));
    }

    #[test]
    fn test_virtual_addr_calculation() {
        use minix_types::VirBytes;
        use crate::region::VirRegion;
        
        let mut vir_region = Box::new(VirRegion::new(
            VirBytes(0x400000),
            VirBytes(0x3000),
            crate::region::VrFlags(0),
        ));
        
        let mut phys_region = Box::new(PhysRegion::new(VirBytes(0x1000)));
        phys_region.parent = Some(&mut *vir_region as *mut VirRegion);
        
        let vaddr = phys_region.get_virtual_addr();
        assert_eq!(vaddr, Some(VirBytes(0x401000)));
    }

    #[test]
    fn test_has_phys_block() {
        use minix_types::VirBytes;
        
        let mut region = PhysRegion::new(VirBytes(0x1000));
        assert!(!region.has_phys_block());
        
        let mut block = PhysBlock::new(0x8000);
        region.bind_block(&mut block);
        assert!(region.has_phys_block());
    }

    #[test]
    fn test_refcount_methods() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        let mut region1 = PhysRegion::new(VirBytes(0x1000));
        let mut region2 = PhysRegion::new(VirBytes(0x2000));
        
        region1.bind_block(&mut block);
        assert_eq!(region1.get_refcount(), Some(1));
        assert!(region1.is_writable());
        assert!(!region1.needs_cow());
        
        region2.bind_block(&mut block);
        assert_eq!(region1.get_refcount(), Some(2));
        assert!(!region1.is_writable());
        assert!(region1.needs_cow());
    }

    #[test]
    fn test_block_flags() {
        use minix_types::VirBytes;
        
        let mut block = PhysBlock::new(0x8000);
        block.flags = PhysBlockFlags::IN_CACHE;
        
        let mut region = PhysRegion::new(VirBytes(0x1000));
        region.bind_block(&mut block);
        
        assert!(region.is_in_cache());
        assert_eq!(region.get_block_flags(), Some(PhysBlockFlags::IN_CACHE));
    }

    #[test]
    fn test_pt_flags() {
        let flags = vec![PtFlags::ReadOnly, PtFlags::Present, PtFlags::User];
        assert_eq!(flags.len(), 3);
        assert!(flags.contains(&PtFlags::ReadOnly));
        assert!(flags.contains(&PtFlags::Present));
        assert!(flags.contains(&PtFlags::User));
    }

    #[test]
    fn test_mock_page_table() {
        MockPageTable::set_page_flags(
            0x1000,
            0x8000,
            &[PtFlags::ReadOnly, PtFlags::Present, PtFlags::User],
        );

        let flags = MockPageTable::get_page_flags(0x1000);
        assert!(flags.is_empty());
    }

    #[test]
    fn test_prepare_cow_single_page() {
        use crate::region::{VirRegion, VrFlags};
        
        let mut vr = VirRegion::new(
            VirBytes(0x1000),
            VirBytes(4096),
            VrFlags(VrFlags::WRITABLE | VrFlags::ANON),
        );

        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut phys_region = Box::new(PhysRegion::new(VirBytes(0)));
        phys_region.link_to_block(block_ptr, std::ptr::null_mut());
        
        vr.physblocks[0] = Some(phys_region);

        assert!(vr.is_writable());
        assert_eq!(block.refcount, 1);
        assert!(vr.get_phys_region(VirBytes(0)).unwrap().is_writable());

        unsafe { vr.prepare_cow(); }

        assert!(vr.is_writable());
    }

    #[test]
    fn test_prepare_cow_shared_page() {
        use crate::region::{VirRegion, VrFlags};
        
        let mut vr1 = VirRegion::new(
            VirBytes(0x1000),
            VirBytes(4096),
            VrFlags(VrFlags::WRITABLE | VrFlags::ANON),
        );

        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut phys_region1 = Box::new(PhysRegion::new(VirBytes(0)));
        phys_region1.link_to_block(block_ptr, std::ptr::null_mut());
        vr1.physblocks[0] = Some(phys_region1);

        let mut phys_region2 = Box::new(PhysRegion::new(VirBytes(0)));
        phys_region2.link_to_block(block_ptr, std::ptr::null_mut());

        assert_eq!(block.refcount, 2);
        assert!(!vr1.get_phys_region(VirBytes(0)).unwrap().is_writable());

        unsafe { vr1.prepare_cow(); }

        assert!(vr1.is_writable());
    }

    #[test]
    fn test_anon_writable() {
        let mut block = Box::new(PhysBlock::new(0x8000));
        let block_ptr = Box::as_mut(&mut block) as *mut PhysBlock;

        let mut phys_region = PhysRegion::new(VirBytes(0));
        phys_region.link_to_block(block_ptr, std::ptr::null_mut());

        assert!(phys_region.is_writable());

        let mut phys_region2 = PhysRegion::new(VirBytes(4096));
        phys_region2.link_to_block(block_ptr, std::ptr::null_mut());

        assert!(!phys_region.is_writable());
        assert!(!phys_region2.is_writable());
    }
}
