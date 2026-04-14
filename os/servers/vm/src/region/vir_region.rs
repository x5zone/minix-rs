//! 虚拟区域 (vir_region) 实现
//!
//! 管理进程的虚拟地址空间布局，使用 AVL 树组织。
//! 对应 Minix3: `region.h` 中的 `vir_region` 结构体

use super::phys_region::{PhysBlock, PhysRegion};
use minix_types::VirBytes;
use std::ptr::NonNull;
use crate::memtype::MemType;

/// 虚拟区域标志
///
/// 对应 Minix3: `VR_*` 宏定义
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VrFlags(pub u16);

impl VrFlags {
    /// 可写
    pub const WRITABLE: u16 = 0x001;
    /// 物理内存必须 64K 对齐
    pub const PHYS64K: u16 = 0x004;
    /// 低端内存 (<16MB，用于 DMA)
    pub const LOWER16MB: u16 = 0x008;
    /// 低端内存 (<1MB，用于 BIOS)
    pub const LOWER1MB: u16 = 0x010;
    /// 共享内存
    pub const SHARED: u16 = 0x040;
    /// 不清零（分配后不初始化）
    pub const UNINITIALIZED: u16 = 0x080;

    /// 匿名内存（需要清零和分配）
    pub const ANON: u16 = 0x100;
    /// 直接映射（不由 VM 管理）
    pub const DIRECT: u16 = 0x200;
    /// 预分配映射
    pub const PREALLOC_MAP: u16 = 0x400;

    /// 创建空标志
    pub const fn empty() -> Self {
        Self(0)
    }

    /// 检查是否包含指定标志
    pub const fn contains(&self, flag: u16) -> bool {
        (self.0 & flag) != 0
    }

    /// 添加标志
    pub fn insert(&mut self, flag: u16) {
        self.0 |= flag;
    }

    /// 移除标志
    pub fn remove(&mut self, flag: u16) {
        self.0 &= !flag;
    }
}

impl Default for VrFlags {
    fn default() -> Self {
        Self::empty()
    }
}

/// 虚拟区域参数（联合体）
///
/// 对应 Minix3: `vir_region` 中的 `param` 联合体
#[derive(Debug, Clone)]
pub enum VrParam {
    /// 直接物理映射（VR_DIRECT）
    Direct { phys: u64 },

    /// 共享内存
    Shared {
        ep: i32,
        vaddr: VirBytes,
        id: i32,
    },

    /// 物理块缓存
    PbCache { pb: Option<NonNull<PhysBlock>> },

    /// 文件映射
    File {
        inited: bool,
        offset: u64,
        clearend: u16,
    },
}

impl Default for VrParam {
    fn default() -> Self {
        Self::Direct { phys: 0 }
    }
}

/// 虚拟区域 (vir_region)
///
/// 代表进程虚拟地址空间中的一段连续区域，具有相同的属性。
/// 对应 Minix3: `struct vir_region` (typedef 为 `region_t`)
pub struct VirRegion {
    /// 虚拟地址（页表偏移）
    pub vaddr: VirBytes,

    /// 长度（字节）
    pub length: VirBytes,

    /// 物理块指针数组
    ///
    /// 每个元素指向一个 phys_region，表示该虚拟区域的物理映射。
    /// 数组大小 = length / PAGE_SIZE
    pub physblocks: Vec<Option<Box<PhysRegion>>>,

    /// 区域标志
    pub flags: VrFlags,

    /// 拥有此区域的进程
    pub parent: Option<NonNull<crate::vmproc::VmProc>>,

    /// 默认内存类型
    ///
    /// 此区域内新分配的物理区域默认使用此内存类型。
    /// 对应 Minix3: `vr->def_memtype`
    ///
    /// TODO: 完整实现（待 10-memtype.md 文档完善）
    pub def_memtype: Option<&'static dyn MemType>,

    /// 重映射计数
    pub remaps: i32,

    /// 唯一 ID
    pub id: i32,

    /// 参数（联合体）
    pub param: VrParam,

    // AVL 树字段
    /// 左子树（低地址）
    pub lower: Option<Box<VirRegion>>,
    /// 右子树（高地址）
    pub higher: Option<Box<VirRegion>>,
    /// 平衡因子
    pub factor: i8,
}

impl std::fmt::Debug for VirRegion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirRegion")
            .field("vaddr", &self.vaddr)
            .field("length", &self.length)
            .field("physblocks_len", &self.physblocks.len())
            .field("flags", &self.flags)
            .field("parent", &self.parent)
            .field("def_memtype", &self.def_memtype.map(|m| m.name()))
            .field("remaps", &self.remaps)
            .field("id", &self.id)
            .field("param", &self.param)
            .field("factor", &self.factor)
            .finish()
    }
}

impl VirRegion {
    /// 创建新的虚拟区域
    pub fn new(vaddr: VirBytes, length: VirBytes, flags: VrFlags) -> Self {
        let pages = ((length.get() + 4095) / 4096) as usize; // 假设 4KB 页
        let mut physblocks = Vec::with_capacity(pages);
        for _ in 0..pages {
            physblocks.push(None);
        }
        Self {
            vaddr,
            length,
            physblocks,
            flags,
            parent: None,
            def_memtype: None,
            remaps: 0,
            id: 0,
            param: VrParam::default(),
            lower: None,
            higher: None,
            factor: 0,
        }
    }

    /// 创建带有内存类型的虚拟区域
    pub fn with_memtype(
        vaddr: VirBytes,
        length: VirBytes,
        flags: VrFlags,
        memtype: &'static dyn MemType,
    ) -> Self {
        let mut region = Self::new(vaddr, length, flags);
        region.def_memtype = Some(memtype);
        region
    }

    /// 获取结束地址（不包含）
    pub fn end_addr(&self) -> VirBytes {
        self.vaddr + self.length
    }

    /// 检查地址是否在区域内
    pub fn contains(&self, addr: VirBytes) -> bool {
        addr >= self.vaddr && addr < self.end_addr()
    }

    /// 检查是否与指定范围重叠
    ///
    /// 重叠条件: self.vaddr < end && self.end_addr() > start
    pub fn overlaps(&self, start: VirBytes, end: VirBytes) -> bool {
        self.vaddr < end && self.end_addr() > start
    }

    /// 检查是否可写
    pub fn is_writable(&self) -> bool {
        self.flags.contains(VrFlags::WRITABLE)
    }

    /// 检查是否是匿名内存
    pub fn is_anon(&self) -> bool {
        self.flags.contains(VrFlags::ANON)
    }

    /// 检查是否是直接映射
    pub fn is_direct(&self) -> bool {
        self.flags.contains(VrFlags::DIRECT)
    }

    /// 设置区域可写性
    ///
    /// 对应 Minix3: 设置/清除 `VR_WRITABLE` 标志
    pub fn set_writable(&mut self, writable: bool) {
        if writable {
            self.flags.insert(VrFlags::WRITABLE);
        } else {
            self.flags.remove(VrFlags::WRITABLE);
        }
    }

    /// 准备 CoW: 设置共享页面为只读
    ///
    /// 遍历所有物理区域，将共享页面（refcount > 1）设置为只读，
    /// 以触发写时复制机制。
    ///
    /// 对应 Minix3: `map_writept()` 中的 CoW 准备逻辑
    ///
    /// # Safety
    ///
    /// 调用者必须确保:
    /// - 所有 PhysRegion 都已正确初始化
    /// - 页表操作是安全的
    pub unsafe fn prepare_cow(&mut self) {
        use super::phys_region::{MockPageTable, PtFlags};
        const PAGE_SIZE: u64 = 4096;
        let num_pages = (self.length.get() / PAGE_SIZE) as usize;

        for i in 0..num_pages {
            if let Some(phys_region) = &self.physblocks[i] {
                if let Some(block_ptr) = phys_region.ph {
                    unsafe {
                        let block = &*block_ptr;

                        if block.refcount > 1 && self.is_writable() && phys_region.is_writable() {
                            let vaddr = self.vaddr.get() + i as u64 * PAGE_SIZE;
                            let paddr = block.phys;

                            MockPageTable::set_page_flags(
                                vaddr,
                                paddr,
                                &[PtFlags::ReadOnly, PtFlags::Present, PtFlags::User],
                            );
                        }
                    }
                }
            }
        }
    }

    /// 获取指定偏移的物理区域
    pub fn get_phys_region(&self, offset: VirBytes) -> Option<&PhysRegion> {
        let page = (offset.get() / 4096) as usize;
        self.physblocks.get(page).and_then(|opt| opt.as_ref().map(|b| b.as_ref()))
    }

    /// 获取指定偏移的物理区域（可变）
    pub fn get_phys_region_mut(&mut self, offset: VirBytes) -> Option<&mut PhysRegion> {
        let page = (offset.get() / 4096) as usize;
        self.physblocks.get_mut(page).and_then(|opt| opt.as_mut().map(|b| b.as_mut()))
    }

    /// 设置指定偏移的物理区域
    pub fn set_phys_region(&mut self, offset: VirBytes, region: PhysRegion) {
        let page = (offset.get() / 4096) as usize;
        if page < self.physblocks.len() {
            self.physblocks[page] = Some(Box::new(region));
        }
    }

    /// 分割区域
    ///
    /// 将当前区域在指定位置分割成两个区域。
    /// 返回 (左区域, 右区域)，左区域保留原起始地址。
    ///
    /// 对应 Minix3: `split_region()`
    pub fn split(self, split_len: VirBytes) -> Result<(Self, Self), VmError> {
        let page_size: u64 = 4096;

        if split_len.get() % page_size != 0 {
            return Err(VmError::InvalidParam);
        }
        if split_len.get() >= self.length.get() {
            return Err(VmError::InvalidParam);
        }

        let rem_len = VirBytes(self.length.get() - split_len.get());

        let mut left = VirRegion::new(self.vaddr, split_len, self.flags);
        let mut right = VirRegion::new(
            VirBytes(self.vaddr.get() + split_len.get()),
            rem_len,
            self.flags,
        );

        left.remaps = self.remaps;
        left.id = self.id;
        right.remaps = self.remaps;
        right.id = self.id + 1;

        for (i, phys_opt) in self.physblocks.into_iter().enumerate() {
            if let Some(phys) = phys_opt {
                let offset = VirBytes((i as u64) * page_size);
                if offset.get() < split_len.get() {
                    left.set_phys_region(offset, *phys);
                } else {
                    let right_offset = VirBytes(offset.get() - split_len.get());
                    right.set_phys_region(right_offset, *phys);
                }
            }
        }

        Ok((left, right))
    }

    /// 从指定偏移开始释放区域
    ///
    /// 释放 [offset, offset+len) 范围内的物理页。
    /// 返回释放的页数。
    ///
    /// 对应 Minix3: `map_subfree()`
    pub fn free_range(&mut self, offset: VirBytes, len: VirBytes) -> usize {
        let page_size: u64 = 4096;
        let start_page = (offset.get() / page_size) as usize;
        let end_page = ((offset.get() + len.get() + page_size - 1) / page_size) as usize;
        let mut freed = 0;

        for page in start_page..end_page.min(self.physblocks.len()) {
            if self.physblocks[page].take().is_some() {
                freed += 1;
            }
        }

        freed
    }
}

/// VM 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    /// 无效参数
    InvalidParam,
    /// 内存不足
    NoMemory,
    /// 区域未找到
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::VirBytes;

    #[test]
    fn test_vir_region_creation() {
        let region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        assert_eq!(region.vaddr, VirBytes(0x1000));
        assert_eq!(region.length, VirBytes(0x4000));
        assert_eq!(region.end_addr(), VirBytes(0x5000));
    }

    #[test]
    fn test_vir_region_contains() {
        let region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        assert!(region.contains(VirBytes(0x1000)));
        assert!(region.contains(VirBytes(0x2000)));
        assert!(region.contains(VirBytes(0x4fff)));
        assert!(!region.contains(VirBytes(0x5000)));
        assert!(!region.contains(VirBytes(0x0fff)));
    }

    #[test]
    fn test_vir_region_flags() {
        let mut flags = VrFlags::empty();
        assert!(!flags.contains(VrFlags::WRITABLE));

        flags.insert(VrFlags::WRITABLE);
        assert!(flags.contains(VrFlags::WRITABLE));

        flags.insert(VrFlags::ANON);
        assert!(flags.contains(VrFlags::ANON));

        flags.remove(VrFlags::WRITABLE);
        assert!(!flags.contains(VrFlags::WRITABLE));
    }

    #[test]
    fn test_vir_region_split() {
        let region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());

        let (left, right) = region.split(VirBytes(0x2000)).unwrap();

        assert_eq!(left.vaddr, VirBytes(0x1000));
        assert_eq!(left.length, VirBytes(0x2000));
        assert_eq!(right.vaddr, VirBytes(0x3000));
        assert_eq!(right.length, VirBytes(0x2000));
    }

    #[test]
    fn test_vir_region_split_invalid() {
        let region1 = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        let result1 = region1.split(VirBytes(0x1001));
        assert!(matches!(result1, Err(VmError::InvalidParam)));

        let region2 = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());
        let result2 = region2.split(VirBytes(0x4000));
        assert!(matches!(result2, Err(VmError::InvalidParam)));
    }

    #[test]
    fn test_vir_region_free_range() {
        let mut region = VirRegion::new(VirBytes(0x1000), VirBytes(0x4000), VrFlags::empty());

        region.set_phys_region(VirBytes(0x0000), PhysRegion::new(VirBytes(0x0000)));
        region.set_phys_region(VirBytes(0x1000), PhysRegion::new(VirBytes(0x1000)));
        region.set_phys_region(VirBytes(0x2000), PhysRegion::new(VirBytes(0x2000)));
        region.set_phys_region(VirBytes(0x3000), PhysRegion::new(VirBytes(0x3000)));

        let freed = region.free_range(VirBytes(0x1000), VirBytes(0x1000));
        assert_eq!(freed, 1);

        assert!(region.get_phys_region(VirBytes(0x0000)).is_some());
        assert!(region.get_phys_region(VirBytes(0x1000)).is_none());
        assert!(region.get_phys_region(VirBytes(0x2000)).is_some());
        assert!(region.get_phys_region(VirBytes(0x3000)).is_some());
    }
}
