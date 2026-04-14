//! 内存类型系统
//!
//! 提供多态内存类型支持，不同类型的内存（匿名内存、文件映射、物理内存等）
//! 有不同的行为特征。
//!
//! 对应 Minix3: `memtype.h` 中的 `mem_type_t`
//!
//! # 设计说明
//!
//! Minix3 使用函数指针表实现多态，Rust 使用 trait 实现更安全的多态。
//! 每种内存类型实现此 trait，提供特定行为。
//!
//! # 内存类型
//!
//! - **匿名内存 (Anonymous)**: 普通堆内存，支持 CoW
//! - **直接物理映射**: 设备内存映射
//! - **文件映射**: mmap 文件
//! - **共享内存**: 进程间共享

use minix_types::VirBytes;

/// 内存类型 trait
///
/// 定义内存类型的核心操作接口。
/// 对应 Minix3: `struct mem_type`
///
/// TODO: 完整实现所有回调方法（待 10-memtype.md 文档完善）
pub trait MemType: Send + Sync {
    /// 获取类型名称
    fn name(&self) -> &'static str;

    /// 创建新区域时的回调
    ///
    /// # 参数
    /// - `region`: 新创建的虚拟区域
    ///
    /// # 返回值
    /// - `Ok(())`: 成功
    /// - `Err(e)`: 失败
    ///
    /// 对应 Minix3: `ev_new`
    fn on_new(&self, _region: &mut crate::region::VirRegion) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 删除区域时的回调
    ///
    /// # 参数
    /// - `region`: 要删除的虚拟区域
    ///
    /// 对应 Minix3: `ev_delete`
    fn on_delete(&self, _region: &mut crate::region::VirRegion) {}

    /// 引用物理区域时的回调
    ///
    /// 当 fork 或共享内存时调用，增加引用。
    ///
    /// # 参数
    /// - `src`: 源物理区域
    /// - `dst`: 新物理区域
    ///
    /// 对应 Minix3: `ev_reference`
    fn on_reference(
        &self,
        _src: &crate::region::PhysRegion,
        _dst: &mut crate::region::PhysRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 取消引用物理区域时的回调
    ///
    /// 当释放内存或 CoW 时调用，减少引用。
    /// 返回 true 表示应该释放物理内存。
    ///
    /// # 参数
    /// - `pr`: 物理区域
    ///
    /// # 返回值
    /// - `Ok(true)`: 应该释放物理内存
    /// - `Ok(false)`: 还有其他引用，不应释放
    /// - `Err(e)`: 错误
    ///
    /// 对应 Minix3: `ev_unreference`
    fn on_unreference(&self, _pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        Ok(false)
    }

    /// 页错误处理回调
    ///
    /// 当发生页错误时调用，处理按需分配、CoW 等。
    ///
    /// # 参数
    /// - `vmp`: 进程
    /// - `region`: 虚拟区域
    /// - `pr`: 物理区域
    /// - `write`: 是否为写操作
    ///
    /// 对应 Minix3: `ev_pagefault`
    ///
    /// TODO: 完整实现（待 14-pagefault.md 文档完善）
    fn on_pagefault(
        &self,
        _vmp: &crate::vmproc::VmProc,
        _region: &mut crate::region::VirRegion,
        _pr: &mut crate::region::PhysRegion,
        _write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        Ok(PagefaultResult::Handled)
    }

    /// 区域大小调整回调
    ///
    /// 对应 Minix3: `ev_resize`
    fn on_resize(
        &self,
        _vmp: &mut crate::vmproc::VmProc,
        _region: &mut crate::region::VirRegion,
        _new_len: VirBytes,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 区域分割回调
    ///
    /// 当区域被分割时调用。
    ///
    /// # 参数
    /// - `vmp`: 进程
    /// - `original`: 原始区域（将被分割）
    /// - `left`: 左半部分
    /// - `right`: 右半部分
    ///
    /// 对应 Minix3: `ev_split`
    fn on_split(
        &self,
        _vmp: &crate::vmproc::VmProc,
        _original: &crate::region::VirRegion,
        _left: &mut crate::region::VirRegion,
        _right: &mut crate::region::VirRegion,
    ) {
    }

    /// 检查是否可写
    ///
    /// 对应 Minix3: `writable`
    fn is_writable(&self, _pr: &crate::region::PhysRegion) -> bool {
        false
    }

    /// 复制区域时的回调
    ///
    /// 对应 Minix3: `ev_copy`
    fn on_copy(
        &self,
        _src: &crate::region::VirRegion,
        _dst: &mut crate::region::VirRegion,
    ) -> Result<(), MemTypeError> {
        Ok(())
    }

    /// 获取区域 ID
    ///
    /// 对应 Minix3: `regionid`
    fn region_id(&self, _region: &crate::region::VirRegion) -> u32 {
        0
    }

    /// 获取引用计数
    ///
    /// 对应 Minix3: `refcount`
    fn ref_count(&self, _region: &crate::region::VirRegion) -> i32 {
        0
    }

    /// 获取页表标志
    ///
    /// 对应 Minix3: `pt_flags`
    fn pt_flags(&self, _region: &crate::region::VirRegion) -> i32 {
        0
    }
}

/// 内存类型错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemTypeError {
    /// 内存不足
    NoMemory,
    /// 无效参数
    InvalidParam,
    /// 不支持的操作
    NotSupported,
    /// IO 错误
    IoError,
    /// 复制失败
    CopyFailed,
}

impl core::fmt::Display for MemTypeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoMemory => write!(f, "Out of memory"),
            Self::InvalidParam => write!(f, "Invalid parameter"),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::IoError => write!(f, "IO error"),
            Self::CopyFailed => write!(f, "Copy failed"),
        }
    }
}

/// 页错误处理结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagefaultResult {
    /// 已处理
    Handled,
    /// 需要分配新页
    NeedNewPage,
    /// 需要 CoW
    NeedCow,
    /// 访问违规
    AccessViolation,
}

/// 匿名内存类型
///
/// 普通堆内存，支持 CoW（写时复制）。
/// 对应 Minix3: `mem_type_anon`
///
/// TODO: 完整实现（待 10-memtype.md 文档完善）
pub struct AnonymousMemory;

impl AnonymousMemory {
    /// 创建匿名内存类型实例
    pub const fn new() -> Self {
        Self
    }
}

impl Default for AnonymousMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for AnonymousMemory {
    fn name(&self) -> &'static str {
        "anonymous memory"
    }

    fn is_writable(&self, pr: &crate::region::PhysRegion) -> bool {
        if let Some(refcount) = pr.get_refcount() {
            refcount == 1
        } else {
            false
        }
    }

    fn on_unreference(&self, pr: &mut crate::region::PhysRegion) -> Result<bool, MemTypeError> {
        if let Some(refcount) = pr.get_refcount() {
            if refcount == 0 {
                if let Some(_phys) = pr.get_phys_addr() {
                    // TODO: 调用物理内存分配器释放内存
                    // free_mem(ABS2CLICK(phys), 1)
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn on_pagefault(
        &self,
        _vmp: &crate::vmproc::VmProc,
        region: &mut crate::region::VirRegion,
        pr: &mut crate::region::PhysRegion,
        write: bool,
    ) -> Result<PagefaultResult, MemTypeError> {
        let refcount = pr.get_refcount().unwrap_or(0);

        if refcount < 2 || !write {
            return Ok(PagefaultResult::Handled);
        }

        if !region.is_writable() {
            return Ok(PagefaultResult::AccessViolation);
        }

        Ok(PagefaultResult::NeedCow)
    }

    fn region_id(&self, _region: &crate::region::VirRegion) -> u32 {
        1
    }

    fn ref_count(&self, region: &crate::region::VirRegion) -> i32 {
        let mut count = 0i32;
        for pb in &region.physblocks {
            if let Some(pr) = pb {
                if let Some(rc) = pr.get_refcount() {
                    count += rc as i32;
                }
            }
        }
        count
    }
}

/// 直接物理映射类型
///
/// 设备内存映射，不由 VM 管理。
/// 对应 Minix3: `mem_type_directphys`
///
/// TODO: 完整实现（待 10-memtype.md 文档完善）
pub struct DirectPhysical;

impl DirectPhysical {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for DirectPhysical {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for DirectPhysical {
    fn name(&self) -> &'static str {
        "direct physical"
    }

    fn is_writable(&self, _pr: &crate::region::PhysRegion) -> bool {
        true
    }
}

/// 共享内存类型
///
/// 进程间共享内存。
/// 对应 Minix3: `mem_type_shared`
///
/// TODO: 完整实现（待 10-memtype.md 文档完善）
pub struct SharedMemory;

impl SharedMemory {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for SharedMemory {
    fn default() -> Self {
        Self::new()
    }
}

impl MemType for SharedMemory {
    fn name(&self) -> &'static str {
        "shared memory"
    }

    fn is_writable(&self, _pr: &crate::region::PhysRegion) -> bool {
        true
    }
}

/// 全局内存类型实例
///
/// 提供默认的内存类型实例。
/// TODO: 考虑使用 lazy_static 或 OnceLock
pub static MEM_TYPE_ANON: AnonymousMemory = AnonymousMemory::new();
pub static MEM_TYPE_DIRECT: DirectPhysical = DirectPhysical::new();
pub static MEM_TYPE_SHARED: SharedMemory = SharedMemory::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous_memory_name() {
        let anon = AnonymousMemory::new();
        assert_eq!(anon.name(), "anonymous memory");
    }

    #[test]
    fn test_direct_physical_name() {
        let direct = DirectPhysical::new();
        assert_eq!(direct.name(), "direct physical");
    }

    #[test]
    fn test_shared_memory_name() {
        let shared = SharedMemory::new();
        assert_eq!(shared.name(), "shared memory");
    }

    #[test]
    fn test_static_instances() {
        assert_eq!(MEM_TYPE_ANON.name(), "anonymous memory");
        assert_eq!(MEM_TYPE_DIRECT.name(), "direct physical");
        assert_eq!(MEM_TYPE_SHARED.name(), "shared memory");
    }
}
