//! PM 进程表：槽位分配/释放、endpoint 验证、PID 查找。
//!
//! C 对应: `minix3/minix/servers/pm/{glo.h, utility.c, forkexit.c}` + 内核
//! generation 语义（`minix3/minix/kernel/system/do_fork.c:69-72`）。
//! 文档: `notes/rewrite/fork-syscall-rewrite/04-stage-pm/03-mproc-table.md`
//!
//! # 表的三层身份
//!
//! - **槽位（slot）**：`[0, NR_PROCS)`，与内核/VM/VFS 三表共享的物理索引；
//! - **endpoint**：`generation << 15 + slot`（跨服务 IPC 身份，`minix-types`
//!   [`Endpoint`]），generation 由内核在槽位复用（fork）时递增
//!   （`do_fork.c:69-72`），**PM 只验证不生成**；
//! - **pid**：POSIX 可见身份，`[INIT_PID+1, NR_PIDS]`，PM 经
//!   [`PidGenerator`] 分配。
//!
//! # 单线程模型
//!
//! PM 是用户态服务器（单线程事件循环）。`Cell<usize>` 计数/轮转指针在
//! 单线程下安全；跨线程共享需改 `AtomicUsize`。
//!
//! # Minix3 对照点
//!
//! - `mproc[NR_PROCS]`（mproc.h:83）+ `procs_in_use`（glo.h:9）→ 本结构
//! - `pm_isokendpt`（utility.c:108-121）→ [`ProcTable::pm_isokendpt`]
//! - `find_proc`（utility.c:76-85）→ [`ProcTable::find_proc`]
//! - `cleanup`（forkexit.c:795-806）→ [`ProcTable::release_slot`]
//!   （**不 bump generation**，见 03-mproc-table.md §3.3）
//! - `do_fork` 容量检查（forkexit.c:60-62）→ [`ProcTable::can_alloc_for_user`]

use core::cell::Cell;
use minix_types::{Endpoint, Errno, NR_PROCS, LAST_FEW, Pid, UserSlot};
use crate::mproc::{Process, PidGenerator};

/// PM 进程表。
///
/// C: `mproc[NR_PROCS]`（mproc.h:83）+ `procs_in_use`（glo.h:9）+
/// `do_fork` 的 `static next_child`（forkexit.c:51）+
/// `get_free_pid` 的 `static next_pid`（utility.c:36）四个分散全局的聚合
/// （ARCH A-3：全局变量 → `ProcTable`/`PmContext`）。
#[derive(Debug)]
pub struct ProcTable {
    /// 进程数组（256 槽，约 120 KB，`size_of::<Process>()` 实测 480 B/槽）。
    ///
    /// 槽位索引 = endpoint 的 slot 部分 = 四表（内核/VM/VFS/PM）共享的坐标。
    pub procs: [Process; NR_PROCS],
    /// 活进程计数。
    ///
    /// C: `procs_in_use`（glo.h:9）。与 `procs[]` 中 `is_in_use()` 槽数恒等。
    pub procs_in_use: Cell<usize>,
    /// 下一子进程槽（轮转分配指针）。
    ///
    /// C: `do_fork` 内 `static unsigned int next_child`（forkexit.c:51）。
    pub next_child: Cell<usize>,
    /// PID 生成器。
    ///
    /// C: `get_free_pid` 内 `static pid_t next_pid`（utility.c:36）。
    pub pid_generator: PidGenerator,
}

impl ProcTable {
    /// 创建空进程表：全部槽位 `Unused`、计数 0、轮转指针 0、
    /// PID 生成器就绪（`next_pid = INIT_PID + 1`）。
    ///
    /// C: 第 1 步（main.c:146-152）mproc 表初始化 + 全局零初始化。
    pub fn new() -> Self {
        Self {
            procs: core::array::from_fn(|_| Process::default()),
            procs_in_use: Cell::new(0),
            next_child: Cell::new(0),
            pid_generator: PidGenerator::new(),
        }
    }

    /// 获取槽位进程引用。
    pub fn get(&self, index: usize) -> Option<&Process> {
        self.procs.get(index)
    }

    /// 获取槽位进程可变引用。
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Process> {
        self.procs.get_mut(index)
    }

    /// 活进程计数。
    ///
    /// C: `procs_in_use`。
    pub fn count(&self) -> usize {
        self.procs_in_use.get()
    }

    /// 表是否已满（无空槽）。
    pub fn is_full(&self) -> bool {
        self.procs_in_use.get() >= NR_PROCS
    }

    /// 遍历全部活进程。
    ///
    /// 只产生活进程（`is_in_use()`），供 PID 冲突检测等表级扫描使用。
    pub fn iter_active(&self) -> impl Iterator<Item = &Process> {
        self.procs.iter().filter(|p| p.is_in_use())
    }

    /// 非 root 用户是否还能分配槽位。
    ///
    /// C: `do_fork` 容量检查（forkexit.c:60-62）：
    /// ```c
    /// if ((procs_in_use == NR_PROCS) ||
    ///     (procs_in_use >= NR_PROCS-LAST_FEW && rmp->mp_effuid != 0))
    ///     return(EAGAIN);
    /// ```
    ///
    /// `is_root` 按 C 语义应为 effective uid == 0（`mp_effuid`，
    /// 见 `PmContext::is_root`）。
    pub fn can_alloc_for_user(&self, is_root: bool) -> bool {
        let count = self.procs_in_use.get();
        if count >= NR_PROCS {
            return false;
        }
        if count >= NR_PROCS - LAST_FEW && !is_root {
            return false;
        }
        true
    }

    /// 找空槽（轮转扫描）。
    ///
    /// C: `do_fork` 的槽位扫描（forkexit.c:68-74）：
    /// ```c
    /// do {
    ///     next_child = (next_child+1) % NR_PROCS;
    ///     n++;
    /// } while((mproc[next_child].mp_flags & IN_USE) && n <= NR_PROCS);
    /// ```
    ///
    /// 先递增 `next_child` 再检查（与 C 相同）；`next_child` 保持"最后检查的
    /// 槽位"。满表返回 `None`（C 对应 `panic("do_fork can't find child slot")`
    /// 的不可达路径——容量检查已保证存在空槽）。
    pub fn find_free_slot(&self) -> Option<usize> {
        for _ in 0..NR_PROCS {
            let next = (self.next_child.get() + 1) % NR_PROCS;
            self.next_child.set(next);
            if !self.procs[next].is_in_use() {
                return Some(next);
            }
        }
        None
    }

    /// 分配槽位：找空槽 + 活进程计数加一。
    ///
    /// 只负责"占位 + 计数"；槽位内容（endpoint/pid/状态）由调用方
    /// （fork/init 路径）填充。满表返回 `None`。
    pub fn alloc_slot(&self) -> Option<usize> {
        let slot = self.find_free_slot()?;
        self.procs_in_use.set(self.procs_in_use.get() + 1);
        Some(slot)
    }

    /// 释放槽位：重置槽位 + 活进程计数减一。
    ///
    /// C: `cleanup`（forkexit.c:795-806）——只清 `mp_pid`/`mp_flags`/child 时间
    /// + `procs_in_use--`。
    ///
    /// # 与 C 的差异（ARCH，03-mproc-table.md §3.3）
    ///
    /// C 保留陈旧 `mp_endpoint`，靠 `!IN_USE` 检查挡陈旧引用（EDEADEPT）；
    /// Rust 重置整个槽位为 `Process::default()`（endpoint → `Endpoint::NONE`），
    /// 陈旧引用在 endpoint 比较处即失败——errno 契约相同（EDEADEPT）。
    ///
    /// **generation 的递增属于内核**（`do_fork.c:69-72`，槽位复用时 +1，
    /// 经 VM 传回 PM），PM 侧不得 bump——本方法不修改 endpoint 代数。
    pub fn release_slot(&mut self, index: usize) {
        if index >= NR_PROCS || self.procs_in_use.get() == 0 {
            return;
        }
        self.procs[index] = Process::default();
        self.procs_in_use.set(self.procs_in_use.get() - 1);
    }

    /// 验证 endpoint 并返回对应槽位。
    ///
    /// C: `pm_isokendpt`（utility.c:108-121）：
    /// ```c
    /// *proc = _ENDPOINT_P(endpoint);
    /// if (*proc < 0 || *proc >= NR_PROCS)
    ///     return EINVAL;
    /// if (endpoint != mproc[*proc].mp_endpoint)
    ///     return EDEADEPT;
    /// if (!(mproc[*proc].mp_flags & IN_USE))
    ///     return EDEADEPT;
    /// return OK;
    /// ```
    ///
    /// 三层检查顺序与 C 一致：槽位范围（EINVAL）→ endpoint 代数
    /// （EDEADEPT）→ 存活（EDEADEPT）。负槽位（内核 task）与
    /// ANY/NONE/SELF 特殊值都落在范围检查 → EINVAL。
    pub fn pm_isokendpt(&self, endpoint: Endpoint) -> Result<UserSlot, EndpointError> {
        let slot = endpoint.slot();
        if slot < 0 || slot as usize >= NR_PROCS {
            return Err(EndpointError::InvalidSlot);
        }
        let idx = slot as usize;
        if self.procs[idx].endpoint() != endpoint {
            return Err(EndpointError::DeadEndpoint);
        }
        if !self.procs[idx].is_in_use() {
            return Err(EndpointError::DeadEndpoint);
        }
        Ok(UserSlot::new(idx))
    }

    /// 按 pid 查找进程槽位。
    ///
    /// C: `find_proc`（utility.c:76-85）——扫描 `(mp_flags & IN_USE) &&
    /// mp_pid == lpid` 的第一个槽位；无匹配返回 `None`（调用方映射 ESRCH）。
    ///
    /// 返回槽位而非 `&Process`：调用方（trace/misc）需要可变访问时按需
    /// `get_mut()`，表保持"索引权威"。
    pub fn find_proc(&self, pid: Pid) -> Option<UserSlot> {
        self.procs
            .iter()
            .enumerate()
            .find(|(_, p)| p.is_in_use() && p.pid() == pid)
            .map(|(idx, _)| UserSlot::new(idx))
    }
}

impl Default for ProcTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Endpoint 验证错误。
///
/// C: `pm_isokendpt` 的两种失败 errno（utility.c:108-121）：
/// - `InvalidSlot` → EINVAL（槽位越界：负槽位/≥ NR_PROCS/特殊 endpoint）；
/// - `DeadEndpoint` → EDEADEPT（endpoint 代数不匹配或槽位未使用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointError {
    /// 槽位越界（C: EINVAL，sys/errno.h:64）。
    InvalidSlot,
    /// endpoint 陈旧或槽位未使用（C: EDEADEPT，sys/errno.h:211）。
    DeadEndpoint,
}

impl EndpointError {
    /// 映射到 Minix3 errno。
    pub const fn to_errno(self) -> Errno {
        match self {
            Self::InvalidSlot => Errno::EINVAL,
            Self::DeadEndpoint => Errno::EDEADEPT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mproc::Lifecycle;

    #[test]
    fn test_proc_table_new() {
        let table = ProcTable::new();
        assert_eq!(table.count(), 0);
        assert!(!table.is_full());
        assert!(!table.procs[0].is_in_use());
    }

    #[test]
    fn test_find_free_slot_round_robin() {
        let mut table = ProcTable::new();
        // 首轮：从槽 1 开始检查（next_child 先递增，与 C forkexit.c:69 相同）。
        let slot = table.find_free_slot().unwrap();
        assert_eq!(slot, 1);
        assert_eq!(table.next_child.get(), 1);

        // 轮转：已占用的槽位被跳过。
        table.procs[2].state.lifecycle = Lifecycle::Running;
        let slot = table.find_free_slot().unwrap();
        assert_eq!(slot, 3);
        assert_eq!(table.next_child.get(), 3);
    }

    #[test]
    fn test_find_free_slot_full() {
        let mut table = ProcTable::new();
        for i in 0..NR_PROCS {
            table.procs[i].state.lifecycle = Lifecycle::Running;
        }
        assert_eq!(table.find_free_slot(), None);
    }

    #[test]
    fn test_alloc_slot() {
        let table = ProcTable::new();
        let slot = table.alloc_slot().unwrap();
        assert!(slot < NR_PROCS);
        assert_eq!(table.count(), 1);
    }

    #[test]
    fn test_alloc_slot_full_returns_none() {
        let mut table = ProcTable::new();
        for i in 0..NR_PROCS {
            table.procs[i].state.lifecycle = Lifecycle::Running;
        }
        table.procs_in_use.set(NR_PROCS);
        assert_eq!(table.alloc_slot(), None);
    }

    #[test]
    fn test_release_slot() {
        let mut table = ProcTable::new();
        let slot = table.alloc_slot().unwrap();
        assert_eq!(table.count(), 1);
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(2, slot as i32);

        table.release_slot(slot);
        assert_eq!(table.count(), 0);
        assert!(!table.procs[slot].is_in_use());
        // C cleanup 不 bump generation（forkexit.c:795-806）；Rust 重置为
        // default（endpoint → NONE）。新 endpoint 由内核在槽位复用时生成。
        assert_eq!(table.procs[slot].endpoint(), Endpoint::NONE);
    }

    #[test]
    fn test_release_slot_keeps_count() {
        let mut table = ProcTable::new();
        table.release_slot(0); // 未分配就释放：计数不变
        assert_eq!(table.count(), 0);
    }

    #[test]
    fn test_can_alloc_for_user() {
        let mut table = ProcTable::new();
        assert!(table.can_alloc_for_user(false));
        assert!(table.can_alloc_for_user(true));

        // 到达保留区：非 root 被拒，root 可分配。
        table.procs_in_use.set(NR_PROCS - LAST_FEW);
        assert!(!table.can_alloc_for_user(false));
        assert!(table.can_alloc_for_user(true));

        // 全满：root 也被拒。
        table.procs_in_use.set(NR_PROCS);
        assert!(!table.can_alloc_for_user(true));
    }

    #[test]
    fn test_pm_isokendpt_valid() {
        let mut table = ProcTable::new();
        table.procs[5].state.lifecycle = Lifecycle::Running;
        table.procs[5].identity.endpoint = Endpoint::from_generation_slot(3, 5);
        let ep = table.procs[5].endpoint();
        assert_eq!(table.pm_isokendpt(ep), Ok(UserSlot::new(5)));
    }

    #[test]
    fn test_pm_isokendpt_slot_out_of_range() {
        let table = ProcTable::new();
        // 负槽位（内核 task）→ EINVAL。
        assert_eq!(
            table.pm_isokendpt(Endpoint::CLOCK),
            Err(EndpointError::InvalidSlot)
        );
        // 槽位 ≥ NR_PROCS → EINVAL（与 C `*proc >= NR_PROCS` 相同）。
        assert_eq!(
            table.pm_isokendpt(Endpoint::from_generation_slot(0, NR_PROCS as i32)),
            Err(EndpointError::InvalidSlot)
        );
        // 特殊 endpoint（ANY/NONE/SELF）槽位远大于 NR_PROCS → EINVAL。
        assert_eq!(
            table.pm_isokendpt(Endpoint::ANY),
            Err(EndpointError::InvalidSlot)
        );
        assert_eq!(
            table.pm_isokendpt(Endpoint::NONE),
            Err(EndpointError::InvalidSlot)
        );
    }

    #[test]
    fn test_pm_isokendpt_generation_mismatch() {
        let mut table = ProcTable::new();
        table.procs[7].state.lifecycle = Lifecycle::Running;
        table.procs[7].identity.endpoint = Endpoint::from_generation_slot(2, 7);
        // 代数不匹配 → EDEADEPT（陈旧引用）。
        let stale = Endpoint::from_generation_slot(1, 7);
        assert_eq!(
            table.pm_isokendpt(stale),
            Err(EndpointError::DeadEndpoint)
        );
    }

    #[test]
    fn test_pm_isokendpt_not_in_use() {
        let mut table = ProcTable::new();
        // 未使用槽位持有匹配 endpoint（C cleanup 后陈旧 endpoint 场景的
        // Rust 同构：endpoint 已被重置为 NONE，此处模拟 C 陈旧值）。
        table.procs[7].identity.endpoint = Endpoint::from_generation_slot(2, 7);
        let ep = table.procs[7].endpoint();
        assert_eq!(
            table.pm_isokendpt(ep),
            Err(EndpointError::DeadEndpoint)
        );
    }

    #[test]
    fn test_pm_isokendpt_released_slot() {
        let mut table = ProcTable::new();
        let slot = table.alloc_slot().unwrap();
        table.procs[slot].state.lifecycle = Lifecycle::Running;
        table.procs[slot].identity.endpoint = Endpoint::from_generation_slot(1, slot as i32);
        let old_ep = table.procs[slot].endpoint();
        table.release_slot(slot);

        // 释放后：陈旧 endpoint → EDEADEPT（C: !IN_USE 检查；Rust: NONE 不匹配）。
        assert_eq!(
            table.pm_isokendpt(old_ep),
            Err(EndpointError::DeadEndpoint)
        );
    }

    #[test]
    fn test_find_proc() {
        let mut table = ProcTable::new();
        table.procs[3].state.lifecycle = Lifecycle::Running;
        table.procs[3].identity.id.pid = 42;
        table.procs[10].state.lifecycle = Lifecycle::Running;
        table.procs[10].identity.id.pid = 43;

        assert_eq!(table.find_proc(42), Some(UserSlot::new(3)));
        assert_eq!(table.find_proc(43), Some(UserSlot::new(10)));
        assert_eq!(table.find_proc(44), None);
    }

    #[test]
    fn test_find_proc_skips_released_slot() {
        let mut table = ProcTable::new();
        // 未使用槽位的陈旧 pid 不参与匹配（C: `mp_flags & IN_USE` 检查，
        // utility.c:82）。
        table.procs[5].identity.id.pid = 42;
        assert_eq!(table.find_proc(42), None);
    }

    #[test]
    fn test_endpoint_error_to_errno() {
        // C: sys/errno.h:64/211。
        assert_eq!(EndpointError::InvalidSlot.to_errno(), Errno::EINVAL);
        assert_eq!(EndpointError::DeadEndpoint.to_errno(), Errno::EDEADEPT);
    }
}
