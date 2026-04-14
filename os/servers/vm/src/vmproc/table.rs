//! VM 进程表
//!
//! 提供虚拟内存管理器的进程表实现，使用 `MaybeUninit` 避免初始化开销。

use std::mem::MaybeUninit;
use minix_types::{Bitmap, Endpoint, NR_PROCS, UserSlot};
use super::VmProc;

/// VM 进程表大小
///
/// 包含所有用户进程槽位。内核任务由 Kernel 管理，不在 VM 进程表中。
/// 对应 Minix3: `VMP_NR` 的用户进程部分
pub const VM_PROC_COUNT: usize = NR_PROCS;

/// exec 临时槽位
///
/// exec 过程中临时存储旧进程状态，防止 exec 失败时丢失信息。
/// slot 0 通常保留给 init 进程，但在 exec 时用作临时存储。
/// 对应 Minix3: `VMP_EXECTMP`
pub const VM_EXEC_TMP_SLOT: UserSlot = UserSlot(0);

/// VM 进程表
///
/// 使用 `MaybeUninit` 避免：
/// 1. 要求 `VmProc` 实现 `Default`
/// 2. 初始化开销（`NR_PROCS` 可能很大）
///
/// # 设计原则
///
/// - **地址稳定**：对象位置固定，不随操作改变
/// - **支持部分初始化**：允许部分槽位未初始化
/// - **避免隐式 Drop**：防止意外释放资源
/// - **缓存友好**：状态标记与数据分离
///
/// # 对应 Minix3 源码
///
/// [`vmproc.h`](../../../../minix3/minix/servers/vm/vmproc.h) 中的 `vmproc[]` 数组
pub struct VmProcTable {
    /// 物理层：原始内存占位，布局紧凑
    /// 使用静态数组保证地址稳定性，存储在 BSS 段
    slots: [MaybeUninit<VmProc>; VM_PROC_COUNT],

    /// 索引层：快速扫描，单 cache line（32 字节）
    in_use: Bitmap,
}

impl VmProcTable {
    /// 创建新的进程表
    ///
    /// # Safety
    ///
    /// 使用 `MaybeUninit::uninit()` 创建未初始化数组。
    /// 这是安全的，因为我们通过 `in_use` 位图跟踪哪些槽位已初始化。
    ///
    /// # 地址稳定性
    ///
    /// 静态数组在 BSS 段分配，地址编译期确定，永不移动。
    /// 这是内核核心表的要求，支持安全的引用传递。
    pub fn new() -> Self {
        Self {
            // SAFETY: MaybeUninit 不需要初始化，这是安全的
            slots: unsafe { MaybeUninit::uninit().assume_init() },
            in_use: Bitmap::new(VM_PROC_COUNT),
        }
    }

    /// 获取指定槽位的进程引用
    ///
    /// 返回 `None` 如果槽位未初始化或索引越界。
    ///
    /// # 示例
    ///
    /// ```
    /// use minix_vm::VmProcTable;
    /// use minix_types::UserSlot;
    ///
    /// let table = VmProcTable::new();
    /// assert!(table.get_proc(UserSlot::new(0)).is_none());
    /// ```
    pub fn get_proc(&self, slot: UserSlot) -> Option<&VmProc> {
        let index = slot.get();
        if index >= NR_PROCS {
            return None;
        }
        if self.in_use.get(index) {
            // SAFETY: in_use 为 true 表示槽位已初始化
            unsafe { Some(self.slots[index].assume_init_ref()) }
        } else {
            None
        }
    }

    /// 获取指定槽位的进程可变引用
    ///
    /// 返回 `None` 如果槽位未初始化或索引越界。
    pub fn get_proc_mut(&mut self, slot: UserSlot) -> Option<&mut VmProc> {
        let index = slot.get();
        if index >= NR_PROCS {
            return None;
        }
        if self.in_use.get(index) {
            // SAFETY: in_use 为 true 表示槽位已初始化
            unsafe { Some(self.slots[index].assume_init_mut()) }
        } else {
            None
        }
    }

    /// 分配一个空槽位
    ///
    /// 返回第一个未使用的槽位索引，如果没有可用槽位则返回 `None`。
    ///
    /// # 示例
    ///
    /// ```
    /// use minix_vm::VmProcTable;
    ///
    /// let mut table = VmProcTable::new();
    /// let slot = table.alloc_slot();
    /// assert!(slot.is_some());
    /// ```
    pub fn alloc_slot(&mut self) -> Option<UserSlot> {
        self.find_free_slot()
    }

    /// 查找第一个空槽位
    ///
    /// 返回第一个未使用的槽位索引。
    pub fn find_free_slot(&self) -> Option<UserSlot> {
        self.in_use.find_first_zero().map(UserSlot::new)
    }

    /// 初始化槽位
    ///
    /// 将进程写入指定槽位并标记为已使用。
    ///
    /// # Panics
    ///
    /// 如果槽位已被使用，会触发 panic。
    pub fn init_slot(&mut self, proc: VmProc) {
        let index = proc.slot.get();
        assert!(index < NR_PROCS, "slot index out of bounds");
        assert!(!self.in_use.get(index), "slot already in use");

        // SAFETY: 我们已验证槽位未使用，写入是安全的
        self.slots[index].write(proc);
        self.in_use.set(index, true);
    }

    /// 移除槽位
    ///
    /// 清除指定槽位的进程并标记为未使用。
    ///
    /// # Safety
    ///
    /// 调用者必须确保没有其他引用指向该进程。
    pub fn remove_slot(&mut self, slot: UserSlot) {
        let index = slot.get();
        if index >= NR_PROCS {
            return;
        }
        if self.in_use.get(index) {
            // SAFETY: 槽位已初始化，可以 drop
            unsafe {
                self.slots[index].assume_init_drop();
            }
            self.in_use.set(index, false);
        }
    }

    /// 检查槽位是否在使用中
    #[inline]
    pub fn is_slot_in_use(&self, slot: UserSlot) -> bool {
        let index = slot.get();
        index < NR_PROCS && self.in_use.get(index)
    }

    /// 获取已使用的槽位数量
    pub fn used_count(&self) -> usize {
        self.in_use.count_ones()
    }

    /// 获取空闲的槽位数量
    pub fn free_count(&self) -> usize {
        self.in_use.count_zeros()
    }

    /// 检查进程表是否为空
    pub fn is_empty(&self) -> bool {
        self.in_use.is_empty()
    }

    /// 检查进程表是否已满
    pub fn is_full(&self) -> bool {
        self.in_use.is_full()
    }

    /// 验证 endpoint 是否有效并返回对应进程
    ///
    /// 对应 Minix3 的 `vm_isokendpt()` 函数。
    /// 执行完整的 TOCTOU 防护检查：
    /// 1. 检查 slot 范围
    /// 2. 检查 endpoint 是否匹配（防止 slot 重用后的旧 endpoint）
    /// 3. 检查进程是否活跃（IN_USE 标志）
    ///
    /// # 参数
    /// - `endpoint`: 要验证的端点
    ///
    /// # 返回值
    /// - `Some(&VmProc)`: endpoint 有效，返回进程引用
    /// - `None`: endpoint 无效（范围错误、不匹配、未激活）
    ///
    /// # 示例
    /// ```
    /// use minix_vm::{VmProcTable, VmProc};
    /// use minix_types::{Endpoint, UserSlot};
    /// use minix_vm::VmFlags;
    ///
    /// let mut table = VmProcTable::new();
    /// let mut proc = VmProc::empty(UserSlot::new(5));
    /// proc.endpoint = Endpoint::from_generation_slot(1, 5);
    /// proc.flags |= VmFlags::IN_USE;
    /// table.init_slot(proc);
    ///
    /// let verified = table.vm_isokendpt(Endpoint::from_generation_slot(1, 5));
    /// assert!(verified.is_some());
    /// ```
    pub fn vm_isokendpt(&self, endpoint: Endpoint) -> Option<&VmProc> {
        // 1. 提取 slot 并检查范围
        let slot = endpoint.slot();
        if slot < 0 || slot as usize >= VM_PROC_COUNT {
            return None;
        }
        let slot_idx = UserSlot(slot as usize);

        // 2. 获取进程（检查是否已初始化）
        let proc = self.get_proc(slot_idx)?;

        // 3. 检查 endpoint 是否匹配（防止 slot 重用后的旧 endpoint）
        if proc.endpoint != endpoint {
            return None;
        }

        // 4. 检查进程是否活跃
        if !proc.is_in_use() {
            return None;
        }

        Some(proc)
    }

    /// 根据 endpoint 查找进程
    ///
    /// 遍历进程表查找匹配的 endpoint。
    /// 时间复杂度 O(N)，适用于进程数不多的场景。
    /// 注意：此方法不验证进程状态，仅做简单查找。
    pub fn find_by_endpoint(&self, endpoint: Endpoint) -> Option<&VmProc> {
        for i in 0..VM_PROC_COUNT {
            if self.in_use.get(i) {
                // SAFETY: in_use 为 true 表示槽位已初始化
                let proc = unsafe { self.slots[i].assume_init_ref() };
                if proc.endpoint == endpoint {
                    return Some(proc);
                }
            }
        }
        None
    }

    /// 遍历所有已使用的进程
    pub fn iter(&self) -> VmProcIter<'_> {
        VmProcIter {
            table: self,
            index: 0,
        }
    }
}

impl Default for VmProcTable {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for VmProcTable {
    fn drop(&mut self) {
        // 清理所有已初始化的槽位
        for i in 0..NR_PROCS {
            if self.in_use.get(i) {
                // SAFETY: 槽位已初始化，可以 drop
                unsafe {
                    self.slots[i].assume_init_drop();
                }
            }
        }
    }
}

/// 进程表迭代器
pub struct VmProcIter<'a> {
    table: &'a VmProcTable,
    index: usize,
}

impl<'a> Iterator for VmProcIter<'a> {
    type Item = &'a VmProc;

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < NR_PROCS {
            let i = self.index;
            self.index += 1;
            if self.table.in_use.get(i) {
                // SAFETY: in_use 为 true 表示槽位已初始化
                unsafe {
                    return Some(self.table.slots[i].assume_init_ref());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VmFlags;

    // 使用 Box 将 VmProcTable 放在堆上，避免栈溢出
    // VmProcTable 包含 [MaybeUninit<VmProc>; NR_PROCS]，在栈上创建会导致溢出

    #[test]
    fn test_table_new() {
        let table = Box::new(VmProcTable::new());
        assert!(table.is_empty());
        assert_eq!(table.used_count(), 0);
        assert_eq!(table.free_count(), NR_PROCS);
    }

    #[test]
    fn test_table_alloc_slot() {
        let mut table = Box::new(VmProcTable::new());
        let slot = table.alloc_slot().unwrap();
        assert_eq!(slot.get(), 0);
    }

    #[test]
    fn test_table_init_slot() {
        let mut table = Box::new(VmProcTable::new());

        let mut proc = VmProc::empty(UserSlot::new(0));
        proc.endpoint = Endpoint::PM;
        proc.flags |= VmFlags::IN_USE;

        table.init_slot(proc);

        assert!(table.is_slot_in_use(UserSlot::new(0)));
        assert_eq!(table.used_count(), 1);

        let retrieved = table.get_proc(UserSlot::new(0)).unwrap();
        assert_eq!(retrieved.endpoint, Endpoint::PM);
    }

    #[test]
    fn test_table_remove_slot() {
        let mut table = Box::new(VmProcTable::new());

        let proc = VmProc::empty(UserSlot::new(0));
        table.init_slot(proc);

        assert!(table.is_slot_in_use(UserSlot::new(0)));

        table.remove_slot(UserSlot::new(0));

        assert!(!table.is_slot_in_use(UserSlot::new(0)));
        assert!(table.get_proc(UserSlot::new(0)).is_none());
    }

    #[test]
    fn test_table_find_by_endpoint() {
        let mut table = Box::new(VmProcTable::new());

        let mut proc = VmProc::empty(UserSlot::new(5));
        proc.endpoint = Endpoint::VM;
        proc.flags |= VmFlags::IN_USE;
        table.init_slot(proc);

        let found = table.find_by_endpoint(Endpoint::VM);
        assert!(found.is_some());
        assert_eq!(found.unwrap().slot.get(), 5);

        let not_found = table.find_by_endpoint(Endpoint::PM);
        assert!(not_found.is_none());
    }

    // === vm_isokendpt 测试 ===

    #[test]
    fn test_vm_isokendpt_valid() {
        let mut table = Box::new(VmProcTable::new());

        // 创建一个有效的进程
        let mut proc = VmProc::empty(UserSlot::new(5));
        proc.endpoint = Endpoint::from_generation_slot(1, 5);
        proc.flags |= VmFlags::IN_USE;
        table.init_slot(proc);

        // 验证有效的 endpoint
        let verified = table.vm_isokendpt(Endpoint::from_generation_slot(1, 5));
        assert!(verified.is_some());
        assert_eq!(verified.unwrap().slot.get(), 5);
    }

    #[test]
    fn test_vm_isokendpt_out_of_range() {
        let table = Box::new(VmProcTable::new());

        // slot 超出范围
        let invalid_ep = Endpoint::from_generation_slot(1, 9999);
        assert!(table.vm_isokendpt(invalid_ep).is_none());
    }

    #[test]
    fn test_vm_isokendpt_mismatch() {
        let mut table = Box::new(VmProcTable::new());

        // 创建一个进程
        let mut proc = VmProc::empty(UserSlot::new(5));
        proc.endpoint = Endpoint::from_generation_slot(1, 5);
        proc.flags |= VmFlags::IN_USE;
        table.init_slot(proc);

        // 使用不同的 generation（模拟 slot 重用后的旧 endpoint）
        let old_ep = Endpoint::from_generation_slot(0, 5);
        assert!(table.vm_isokendpt(old_ep).is_none());
    }

    #[test]
    fn test_vm_isokendpt_not_in_use() {
        let mut table = Box::new(VmProcTable::new());

        // 创建一个进程但不标记 IN_USE
        let mut proc = VmProc::empty(UserSlot::new(5));
        proc.endpoint = Endpoint::from_generation_slot(1, 5);
        // 注意：没有设置 IN_USE 标志
        table.init_slot(proc);

        // 应该验证失败（进程未激活）
        assert!(table.vm_isokendpt(Endpoint::from_generation_slot(1, 5)).is_none());
    }

    #[test]
    fn test_vm_isokendpt_uninitialized_slot() {
        let table = Box::new(VmProcTable::new());

        // 访问未初始化的槽位
        let ep = Endpoint::from_generation_slot(1, 10);
        assert!(table.vm_isokendpt(ep).is_none());
    }

    #[test]
    fn test_table_iter() {
        let mut table = Box::new(VmProcTable::new());

        // 初始化几个进程
        for i in 0..3 {
            let mut proc = VmProc::empty(UserSlot::new(i));
            proc.endpoint = Endpoint((i + 1) as i32);
            proc.flags |= VmFlags::IN_USE;
            table.init_slot(proc);
        }

        let count = table.iter().count();
        assert_eq!(count, 3);
    }

    #[test]
    #[should_panic(expected = "slot already in use")]
    fn test_table_double_init() {
        let mut table = Box::new(VmProcTable::new());

        let proc = VmProc::empty(UserSlot::new(0));
        table.init_slot(proc);

        let proc2 = VmProc::empty(UserSlot::new(0));
        table.init_slot(proc2); // 应该 panic
    }

    #[test]
    fn test_table_out_of_bounds() {
        let table = Box::new(VmProcTable::new());
        assert!(table.get_proc(UserSlot::new(NR_PROCS)).is_none());
        assert!(table.get_proc(UserSlot::new(NR_PROCS + 100)).is_none());
    }
}
