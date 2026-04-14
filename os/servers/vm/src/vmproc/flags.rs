//! VM 进程状态标志
//!
//! 使用 bitflags 定义进程状态，支持正交组合。

use bitflags::bitflags;

bitflags! {
    /// VM 进程状态标志
    ///
    /// VM 的状态是"多维正交"的，例如 `IN_USE | EXITING | VM_INSTANCE` 可以组合。
    /// 不使用 enum 是因为 enum 强制互斥，会退化成 struct。
    ///
    /// # 对应 Minix3 源码
    ///
    /// [`vmproc.h`](../../../../minix3/minix/servers/vm/vmproc.h) 中的 `vm_flags_t`
    ///
    /// # 示例
    ///
    /// ```
    /// use minix_vm::VmFlags;
    ///
    /// let flags = VmFlags::IN_USE | VmFlags::VM_INSTANCE;
    /// assert!(flags.contains(VmFlags::IN_USE));
    /// assert!(flags.contains(VmFlags::VM_INSTANCE));
    /// assert!(!flags.contains(VmFlags::EXITING));
    /// ```
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct VmFlags: u32 {
        /// 槽位包含一个进程
        ///
        /// 对应 Minix3 的 `VMF_INUSE`
        const IN_USE = 0x001;

        /// PM 正在清理此进程
        ///
        /// 对应 Minix3 的 `VMF_EXITING`
        const EXITING = 0x002;

        /// 这是 VM 进程实例
        ///
        /// 对应 Minix3 的 `VMF_VM_INSTANCE`
        const VM_INSTANCE = 0x010;
    }
}

impl VmFlags {
    /// 检查进程是否在使用中
    #[inline]
    pub fn is_in_use(self) -> bool {
        self.contains(Self::IN_USE)
    }

    /// 检查进程是否正在退出
    #[inline]
    pub fn is_exiting(self) -> bool {
        self.contains(Self::EXITING)
    }

    /// 检查是否为 VM 实例
    #[inline]
    pub fn is_vm_instance(self) -> bool {
        self.contains(Self::VM_INSTANCE)
    }
}

impl Default for VmFlags {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vmflags_basic() {
        let flags = VmFlags::IN_USE;
        assert!(flags.contains(VmFlags::IN_USE));
        assert!(!flags.contains(VmFlags::EXITING));
    }

    #[test]
    fn test_vmflags_combination() {
        let flags = VmFlags::IN_USE | VmFlags::EXITING;
        assert!(flags.contains(VmFlags::IN_USE));
        assert!(flags.contains(VmFlags::EXITING));
        assert!(!flags.contains(VmFlags::VM_INSTANCE));
    }

    #[test]
    fn test_vmflags_helpers() {
        let flags = VmFlags::IN_USE | VmFlags::VM_INSTANCE;
        assert!(flags.is_in_use());
        assert!(!flags.is_exiting());
        assert!(flags.is_vm_instance());
    }

    #[test]
    fn test_vmflags_default() {
        let flags: VmFlags = Default::default();
        assert!(flags.is_empty());
    }
}
