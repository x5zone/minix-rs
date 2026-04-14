//! VM 访问控制列表 (ACL)
//!
//! 控制进程可以调用哪些 VM 系统调用。
//! 对应 Minix3: `minix3/minix/servers/vm/acl.c`

use minix_types::{Bitmap, Endpoint};
use crate::vmproc::{AclIndex, VmProc};

/// ACL 常量定义
///
/// 对应 Minix3 acl.c 中的宏定义
pub const NO_ACL: i32 = -1;
pub const USER_ACL: i32 = 0;
pub const FIRST_SYS_ACL: i32 = 1;

/// 系统进程 ACL 数量
///
/// 对应 Minix3: `NR_SYS_PROCS`
/// 预留足够的槽位给系统进程（RS、DS、VM 等）
pub const NR_SYS_PROCS: usize = 32;

/// VM 调用掩码大小（位图块数）
///
/// 假设最多支持 64 个不同的 VM 调用
pub const VM_CALL_MASK_SIZE: usize = 2; // 2 * 32 = 64 bits

/// ACL 管理器
///
/// 管理所有进程的 VM 调用权限
pub struct AclManager {
    /// 权限位图表
    ///
    /// acl_mask[acl_index][chunk] 表示某个 ACL 索引的权限位
    /// 每个位对应一个 VM 调用号
    masks: [[u32; VM_CALL_MASK_SIZE]; NR_SYS_PROCS],

    /// ACL 使用状态位图
    ///
    /// 标记哪些系统 ACL 槽位已被占用
    in_use: Bitmap,
}

impl AclManager {
    /// 创建新的 ACL 管理器
    pub fn new() -> Self {
        Self {
            masks: [[0; VM_CALL_MASK_SIZE]; NR_SYS_PROCS],
            in_use: Bitmap::new(NR_SYS_PROCS),
        }
    }

    /// 初始化 ACL 系统
    ///
    /// 将所有进程的 ACL 设置为 NO_ACL
    /// 清空所有权限位图
    ///
    /// 对应 Minix3: `acl_init()`
    pub fn init(&mut self) {
        // 清空权限位图
        for i in 0..NR_SYS_PROCS {
            for j in 0..VM_CALL_MASK_SIZE {
                self.masks[i][j] = 0;
            }
        }

        // 清空使用状态
        self.in_use.clear();

        // 标记 USER_ACL 为已使用（所有普通进程共享）
        self.in_use.set(USER_ACL as usize, true);
    }

    /// 检查进程是否有权限执行指定 VM 调用
    ///
    /// # 参数
    /// - `proc`: 要检查的进程
    /// - `call`: VM 调用号（从 0 开始）
    ///
    /// # 返回值
    /// - `Ok(())`: 有权限
    /// - `Err(())`: 无权限 (EPERM)
    ///
    /// 对应 Minix3: `acl_check()`
    pub fn check(&self, proc: &VmProc, call: u32) -> Result<(), ()> {
        // VM 进程自身的调用总是允许
        if proc.endpoint == Endpoint::VM {
            return Ok(());
        }

        let acl = proc.acl.get();

        // NO_ACL: 目前暂时允许所有调用（兼容现有行为）
        if acl == NO_ACL {
            return Ok(());
        }

        // 检查权限位
        if !self.get_bit(acl, call) {
            return Err(()); // EPERM
        }

        Ok(())
    }

    /// 为进程设置 ACL
    ///
    /// # 参数
    /// - `proc`: 目标进程
    /// - `mask`: 权限位图（可选）
    /// - `is_sys_proc`: 是否为系统进程
    ///
    /// 对应 Minix3: `acl_set()`
    pub fn set(&mut self, proc: &mut VmProc, mask: Option<&[u32; VM_CALL_MASK_SIZE]>, is_sys_proc: bool) {
        // 先清除现有的 ACL
        self.clear(proc);

        // 分配 ACL 索引
        let acl_idx = if is_sys_proc {
            // 系统进程：分配独立的 ACL 槽位
            self.alloc_sys_acl()
        } else {
            // 普通进程：使用共享的 USER_ACL
            USER_ACL
        };

        if acl_idx == NO_ACL {
            // 没有可用的系统 ACL 槽位
            // 在实际实现中应该记录错误或 panic
            return;
        }

        // 设置 ACL 索引
        proc.acl = AclIndex::new(acl_idx);

        // 如果提供了权限掩码，复制它
        if let Some(m) = mask {
            let idx = acl_idx as usize;
            self.masks[idx].copy_from_slice(m);
        }

        // 标记为已使用
        if acl_idx >= FIRST_SYS_ACL {
            self.in_use.set(acl_idx as usize, true);
        }
    }

    /// fork 时处理 ACL 继承
    ///
    /// # 规则
    /// - USER_ACL: 子进程继承 USER_ACL
    /// - 其他 ACL: 子进程获得 NO_ACL
    ///
    /// 对应 Minix3: `acl_fork()`
    pub fn fork(&self, parent: &VmProc, child: &mut VmProc) {
        if parent.acl.get() == USER_ACL {
            child.acl = AclIndex::new(USER_ACL);
        } else {
            child.acl = AclIndex::new(NO_ACL);
        }
    }

    /// 清除进程的 ACL
    ///
    /// 进程退出时调用，释放系统 ACL 槽位
    ///
    /// 对应 Minix3: `acl_clear()`
    pub fn clear(&mut self, proc: &mut VmProc) {
        let acl = proc.acl.get();

        if acl != NO_ACL && acl != USER_ACL {
            // 释放系统 ACL 槽位
            self.in_use.set(acl as usize, false);
        }

        proc.acl = AclIndex::new(NO_ACL);
    }

    /// 设置指定 ACL 的权限位
    ///
    /// # 参数
    /// - `acl`: ACL 索引
    /// - `call`: VM 调用号
    /// - `allowed`: 是否允许
    pub fn set_permission(&mut self, acl: i32, call: u32, allowed: bool) {
        if acl < 0 || acl as usize >= NR_SYS_PROCS {
            return;
        }

        let idx = (call / 32) as usize;
        let bit = (call % 32) as usize;

        if idx >= VM_CALL_MASK_SIZE {
            return;
        }

        if allowed {
            self.masks[acl as usize][idx] |= 1 << bit;
        } else {
            self.masks[acl as usize][idx] &= !(1 << bit);
        }
    }

    /// 获取指定 ACL 的权限位
    fn get_bit(&self, acl: i32, call: u32) -> bool {
        if acl < 0 || acl as usize >= NR_SYS_PROCS {
            return false;
        }

        let idx = (call / 32) as usize;
        let bit = (call % 32) as usize;

        if idx >= VM_CALL_MASK_SIZE {
            return false;
        }

        (self.masks[acl as usize][idx] >> bit) & 1 != 0
    }

    /// 分配系统进程 ACL 槽位
    ///
    /// 返回分配的 ACL 索引，如果没有可用槽位返回 NO_ACL
    fn alloc_sys_acl(&self) -> i32 {
        for i in FIRST_SYS_ACL..NR_SYS_PROCS as i32 {
            if !self.in_use.get(i as usize) {
                return i;
            }
        }
        NO_ACL
    }
}

impl Default for AclManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vmproc::{VmProc, VmFlags};
    use minix_types::UserSlot;

    #[test]
    fn test_acl_manager_new() {
        let manager = AclManager::new();
        assert!(!manager.in_use.get(USER_ACL as usize));
    }

    #[test]
    fn test_acl_init() {
        let mut manager = AclManager::new();
        manager.init();
        assert!(manager.in_use.get(USER_ACL as usize));
    }

    #[test]
    fn test_acl_check_no_acl() {
        let manager = AclManager::new();
        let mut proc = VmProc::empty(UserSlot::new(0));
        proc.acl = AclIndex::new(NO_ACL);  // 显式设置为 NO_ACL

        // NO_ACL 应该允许所有调用（兼容行为）
        assert!(manager.check(&proc, 0).is_ok());
        assert!(manager.check(&proc, 5).is_ok());
    }

    #[test]
    fn test_acl_check_vm_proc() {
        let manager = AclManager::new();
        let mut proc = VmProc::empty(UserSlot::new(0));
        proc.endpoint = Endpoint::VM;

        // VM 进程自身总是允许
        assert!(manager.check(&proc, 0).is_ok());
    }

    #[test]
    fn test_acl_set_and_check() {
        let mut manager = AclManager::new();
        manager.init();

        let mut proc = VmProc::empty(UserSlot::new(0));
        let mut mask = [0u32; VM_CALL_MASK_SIZE];
        mask[0] = 0b1010; // 允许调用 1 和 3

        // 设置为普通进程
        manager.set(&mut proc, Some(&mask), false);

        assert_eq!(proc.acl.get(), USER_ACL);

        // 检查权限
        assert!(manager.check(&proc, 1).is_ok());
        assert!(manager.check(&proc, 3).is_ok());
        assert!(manager.check(&proc, 0).is_err());
        assert!(manager.check(&proc, 2).is_err());
    }

    #[test]
    fn test_acl_fork_user() {
        let mut manager = AclManager::new();
        manager.init();

        let mut parent = VmProc::empty(UserSlot::new(0));
        let mut child = VmProc::empty(UserSlot::new(1));

        // 设置父进程为 USER_ACL
        manager.set(&mut parent, None, false);
        assert_eq!(parent.acl.get(), USER_ACL);

        // fork
        manager.fork(&parent, &mut child);
        assert_eq!(child.acl.get(), USER_ACL);
    }

    #[test]
    fn test_acl_fork_sys() {
        let manager = AclManager::new();

        let mut parent = VmProc::empty(UserSlot::new(0));
        let mut child = VmProc::empty(UserSlot::new(1));

        // 设置父进程为系统 ACL
        parent.acl = AclIndex::new(5);

        // fork
        manager.fork(&parent, &mut child);
        assert_eq!(child.acl.get(), NO_ACL);
    }

    #[test]
    fn test_acl_clear() {
        let mut manager = AclManager::new();
        manager.init();

        let mut proc = VmProc::empty(UserSlot::new(0));

        // 设置为系统进程
        manager.set(&mut proc, None, true);
        let acl_before = proc.acl.get();
        assert!(acl_before >= FIRST_SYS_ACL);

        // 清除 ACL
        manager.clear(&mut proc);
        assert_eq!(proc.acl.get(), NO_ACL);
    }

    #[test]
    fn test_set_permission() {
        let mut manager = AclManager::new();
        manager.init();

        // 设置 USER_ACL 的权限
        manager.set_permission(USER_ACL, 5, true);
        manager.set_permission(USER_ACL, 10, true);

        let mut proc = VmProc::empty(UserSlot::new(0));
        proc.acl = AclIndex::new(USER_ACL);

        assert!(manager.check(&proc, 5).is_ok());
        assert!(manager.check(&proc, 10).is_ok());
        assert!(manager.check(&proc, 0).is_err());
    }
}
