//! Kernel process management module
//!
//! 这是 Minix3 `proc` 结构体的 Rust 实现基本字段和调度字段。
//!
//! # Minix3 多进程表架构
//!
//! Minix3 采用分布式进程表设计，共有 4 份进程表：
//! - **Kernel/proc**: 调度、IPC、寄存器保存（本模块）
//! - **PM/mproc**: 进程管理、信号、权限 → 在 `minix-pm` crate 中
//! - **VM/vmproc**: 虚拟内存、页表 → 在 `minix-vm` crate 中
//! - **VFS/fproc**: 文件描述符、目录 → 在 `minix-vfs` crate 中
//!
//! 各进程表通过 `endpoint` 关联。

use minix_types::{Endpoint, Message, VirBytes};
use core::sync::atomic::{AtomicU32, AtomicU64, AtomicI8, Ordering};

/// 进程号类型（对应 C 的 `proc_nr_t`）
pub type ProcNr = i32;

/// 时钟滴答类型
pub type ClockTicks = u64;

/// CPU 周期类型
pub type CpuCycles = u64;

/// 进程号常量
pub mod proc_nr {
    use super::ProcNr;
    pub const NONE: ProcNr = -1;
    pub const KERNEL: ProcNr = -2;
}

/// 运行时状态标志位
pub mod rts {
    pub const SLOT_FREE: u32 = 0x01;
    pub const PROC_STOP: u32 = 0x02;
    pub const SENDING: u32 = 0x04;
    pub const RECEIVING: u32 = 0x08;
    pub const SIGNALED: u32 = 0x10;
    pub const SIG_PENDING: u32 = 0x20;
    pub const P_STOP: u32 = 0x40;
    pub const NO_PRIV: u32 = 0x80;
    pub const NO_ENDPOINT: u32 = 0x100;
    pub const VMINHIBIT: u32 = 0x200;
    pub const PAGEFAULT: u32 = 0x400;
    pub const VMREQUEST: u32 = 0x800;
    pub const VMREQTARGET: u32 = 0x1000;
    pub const PREEMPTED: u32 = 0x4000;
    pub const NO_QUANTUM: u32 = 0x8000;
    pub const BOOTINHIBIT: u32 = 0x10000;
}

/// 杂项标志位
pub mod mf {
    pub const REPLY_PEND: u32 = 0x001;
    pub const VIRT_TIMER: u32 = 0x002;
    pub const PROF_TIMER: u32 = 0x004;
    pub const KCALL_RESUME: u32 = 0x008;
    pub const DELIVERMSG: u32 = 0x040;
    pub const SIG_DELAY: u32 = 0x080;
    pub const SC_ACTIVE: u32 = 0x100;
    pub const SC_DEFER: u32 = 0x200;
    pub const SC_TRACE: u32 = 0x400;
    pub const FPU_INITIALIZED: u32 = 0x1000;
    pub const SENDING_FROM_KERNEL: u32 = 0x2000;
    pub const CONTEXT_SET: u32 = 0x4000;
    pub const SPROF_SEEN: u32 = 0x8000;
    pub const FLUSH_TLB: u32 = 0x10000;
    pub const SENDA_VM_MISS: u32 = 0x20000;
    pub const STEP: u32 = 0x40000;
    pub const MSGFAILED: u32 = 0x80000;
    pub const NICED: u32 = 0x100000;
}

/// 优先级范围常量
pub mod priority {
    pub const TASK_Q: i8 = 0;
    pub const MAX_USER_Q: i8 = 0;
    pub const USER_Q: i8 = 7;
    pub const MIN_USER_Q: i8 = 15;
    pub const NR_SCHED_QUEUES: usize = 16;
}

/// 运行时状态标志（封装原子操作）
#[derive(Debug)]
pub struct RtsFlags(AtomicU32);

/// 杂项标志（封装原子操作）
#[derive(Debug)]
pub struct MiscFlags(AtomicU32);

/// 优先级新类型（封装有效性检查）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Priority(i8);

impl Priority {
    pub fn new(value: i8) -> Option<Self> {
        if value >= priority::TASK_Q && value <= priority::MIN_USER_Q {
            Some(Self(value))
        } else {
            None
        }
    }

    pub fn get(&self) -> i8 {
        self.0
    }

    pub fn is_kernel(&self) -> bool {
        self.0 == priority::TASK_Q
    }
}

impl Default for Priority {
    fn default() -> Self {
        Self(priority::USER_Q)
    }
}

/// 时间片管理
#[derive(Debug)]
pub struct Quantum {
    pub cpu_time_left: AtomicU64,
    pub size_ms: AtomicU32,
}

impl Quantum {
    pub fn new(size_ms: u32) -> Self {
        Self {
            cpu_time_left: AtomicU64::new(0),
            size_ms: AtomicU32::new(size_ms),
        }
    }

    pub fn allocate(&self, cycles_per_ms: u64) {
        let total = self.size_ms.load(Ordering::Relaxed) as u64 * cycles_per_ms;
        self.cpu_time_left.store(total, Ordering::Release);
    }

    pub fn consume(&self, cycles: u64) -> bool {
        let left = self.cpu_time_left.load(Ordering::Acquire);
        if left <= cycles {
            self.cpu_time_left.store(0, Ordering::Release);
            true
        } else {
            self.cpu_time_left.store(left - cycles, Ordering::Release);
            false
        }
    }
}

/// CPU ID
pub type CpuId = u32;

/// 调度字段扩展
#[derive(Debug)]
pub struct SchedFields {
    pub priority: AtomicI8,
    pub quantum: Quantum,
    pub cpu: AtomicU32,
}

impl SchedFields {
    pub fn new() -> Self {
        Self {
            priority: AtomicI8::new(priority::USER_Q),
            quantum: Quantum::new(200),
            cpu: AtomicU32::new(0),
        }
    }

    pub fn with_priority(priority: i8) -> Self {
        Self {
            priority: AtomicI8::new(priority),
            quantum: Quantum::new(200),
            cpu: AtomicU32::new(0),
        }
    }
}

impl Default for SchedFields {
    fn default() -> Self {
        Self::new()
    }
}

/// 调度统计结构体
#[derive(Debug)]
pub struct Accounting {
    pub enter_queue: AtomicU64,
    pub time_in_queue: AtomicU64,
    pub dequeues: AtomicU32,
    pub ipc_sync: AtomicU32,
    pub ipc_async: AtomicU32,
    pub preempted: AtomicU32,
}

impl Accounting {
    pub fn new() -> Self {
        Self {
            enter_queue: AtomicU64::new(0),
            time_in_queue: AtomicU64::new(0),
            dequeues: AtomicU32::new(0),
            ipc_sync: AtomicU32::new(0),
            ipc_async: AtomicU32::new(0),
            preempted: AtomicU32::new(0),
        }
    }

    pub fn reset(&self) {
        self.enter_queue.store(0, Ordering::Release);
        self.time_in_queue.store(0, Ordering::Release);
        self.dequeues.store(0, Ordering::Release);
        self.ipc_sync.store(0, Ordering::Release);
        self.ipc_async.store(0, Ordering::Release);
        self.preempted.store(0, Ordering::Release);
    }

    pub fn record_enqueue(&self, tsc: CpuCycles) {
        self.enter_queue.store(tsc, Ordering::Release);
    }

    pub fn record_dequeue(&self, tsc: CpuCycles) {
        let enter = self.enter_queue.load(Ordering::Acquire);
        if enter > 0 && tsc > enter {
            let delta = tsc - enter;
            self.time_in_queue.fetch_add(delta, Ordering::AcqRel);
        }
        self.enter_queue.store(0, Ordering::Release);
        self.dequeues.fetch_add(1, Ordering::AcqRel);
    }

    pub fn record_ipc_sync(&self) {
        self.ipc_sync.fetch_add(1, Ordering::AcqRel);
    }

    pub fn record_ipc_async(&self) {
        self.ipc_async.fetch_add(1, Ordering::AcqRel);
    }

    pub fn record_preempt(&self) {
        self.preempted.fetch_add(1, Ordering::AcqRel);
    }
}

impl Default for Accounting {
    fn default() -> Self {
        Self::new()
    }
}

/// 时间统计结构体
#[derive(Debug)]
pub struct TimeStats {
    pub user_time: AtomicU64,
    pub sys_time: AtomicU64,
    pub virt_left: AtomicU64,
    pub prof_left: AtomicU64,
}

impl TimeStats {
    pub fn new() -> Self {
        Self {
            user_time: AtomicU64::new(0),
            sys_time: AtomicU64::new(0),
            virt_left: AtomicU64::new(0),
            prof_left: AtomicU64::new(0),
        }
    }

    pub fn add_user_time(&self, ticks: ClockTicks) {
        self.user_time.fetch_add(ticks, Ordering::AcqRel);
    }

    pub fn add_sys_time(&self, ticks: ClockTicks) {
        self.sys_time.fetch_add(ticks, Ordering::AcqRel);
    }

    pub fn tick_virt_timer(&self) -> bool {
        let left = self.virt_left.load(Ordering::Acquire);
        if left > 0 {
            self.virt_left.store(left - 1, Ordering::Release);
            left == 1
        } else {
            false
        }
    }

    pub fn tick_prof_timer(&self) -> bool {
        let left = self.prof_left.load(Ordering::Acquire);
        if left > 0 {
            self.prof_left.store(left - 1, Ordering::Release);
            left == 1
        } else {
            false
        }
    }
}

impl Default for TimeStats {
    fn default() -> Self {
        Self::new()
    }
}

/// CPU 周期统计结构体
#[derive(Debug)]
pub struct CyclesStats {
    pub total: AtomicU64,
    pub kcall: AtomicU64,
    pub kipc: AtomicU64,
    pub tick: AtomicU64,
}

impl CyclesStats {
    pub fn new() -> Self {
        Self {
            total: AtomicU64::new(0),
            kcall: AtomicU64::new(0),
            kipc: AtomicU64::new(0),
            tick: AtomicU64::new(0),
        }
    }

    pub fn add_cycles(&self, cycles: CpuCycles) {
        self.total.fetch_add(cycles, Ordering::AcqRel);
    }

    pub fn add_kcall_cycles(&self, cycles: CpuCycles) {
        self.kcall.fetch_add(cycles, Ordering::AcqRel);
    }

    pub fn add_kipc_cycles(&self, cycles: CpuCycles) {
        self.kipc.fetch_add(cycles, Ordering::AcqRel);
    }
}

impl Default for CyclesStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Kernel 进程结构体
#[derive(Debug)]
pub struct KProcess {
    /// 进程号（槽位索引）
    pub p_nr: ProcNr,
    /// 端点标识符
    pub p_endpoint: Endpoint,
    /// 运行时状态标志
    pub p_rts_flags: RtsFlags,
    /// 杂项标志
    pub p_misc_flags: MiscFlags,
    /// 调度字段
    pub p_sched: SchedFields,
    /// 调度统计
    pub p_accounting: Accounting,
    /// 时间统计
    pub p_time: TimeStats,
    /// 周期统计
    pub p_cycles: CyclesStats,

    // IPC 队列指针
    /// 就绪队列中的下一个进程指针
    /// 用于调度器管理同一优先级的就绪进程链表
    pub p_nextready: Option<ProcNr>,

    /// 发送者队列头部指针
    /// 指向等待向本进程发送消息的进程队列的头部
    pub p_caller_q: Option<ProcNr>,

    /// 发送者队列链接指针
    /// 链接同一发送者队列中的下一个进程
    pub p_q_link: Option<ProcNr>,

    // IPC 端点字段
    /// 接收消息的来源端点
    /// 进程调用 receive() 时期望接收消息的来源
    pub p_getfrom_e: Endpoint,

    /// 发送消息的目标端点
    /// 进程调用 send() 或 sendrec() 时的目标端点
    pub p_sendto_e: Endpoint,

    // 信号字段
    /// 待处理的内核信号位图
    /// 记录该进程有哪些信号正在等待处理
    pub p_pending: SigSet,

    // 进程名称字段
    /// 进程名称，用于调试和日志
    /// 最大长度 PROC_NAME_LEN (16字节，包含结尾的\0)
    pub p_name: ProcName,

    // 消息字段
    /// 发送消息缓冲区
    /// 当进程调用 send() 被阻塞时，存储要发送的消息内容
    pub p_sendmsg: Message,

    /// 消息投递缓冲区
    /// 存储准备投递给本进程的消息内容
    pub p_delivermsg: Message,

    /// 消息投递虚拟地址
    /// 用户空间消息缓冲区的虚拟地址
    pub p_delivermsg_vir: VirBytes,
}

/// 进程名称最大长度（包含结尾的\0）
pub const PROC_NAME_LEN: usize = 16;

/// 进程名称类型
/// 固定大小的字节数组，对应 C 的 char[PROC_NAME_LEN]
#[derive(Clone, Copy)]
pub struct ProcName {
    data: [u8; PROC_NAME_LEN],
}

impl ProcName {
    /// 创建空的进程名称
    pub const fn new() -> Self {
        Self { data: [0; PROC_NAME_LEN] }
    }

    /// 从字符串创建进程名称
    /// 如果字符串超过最大长度，会被截断
    pub fn from_str(s: &str) -> Self {
        let mut name = Self::new();
        let bytes = s.as_bytes();
        let len = bytes.len().min(PROC_NAME_LEN - 1);
        name.data[..len].copy_from_slice(&bytes[..len]);
        // 确保以\0结尾（数组已初始化为0）
        name
    }

    /// 获取名称字符串（去除结尾的\0）
    pub fn as_str(&self) -> &str {
        let len = self.data.iter().position(|&b| b == 0).unwrap_or(PROC_NAME_LEN);
        core::str::from_utf8(&self.data[..len]).unwrap_or("<invalid>")
    }

    /// 添加后缀到名称
    /// 如果添加后超过最大长度，会被截断
    pub fn push_suffix(&mut self, suffix: &str) {
        let current_len = self.data.iter().position(|&b| b == 0).unwrap_or(PROC_NAME_LEN);
        let suffix_bytes = suffix.as_bytes();
        let suffix_len = suffix_bytes.len();
        
        // 计算可以复制多少字节（保留1字节给\0）
        let available = PROC_NAME_LEN.saturating_sub(current_len + 1);
        let copy_len = suffix_len.min(available);
        
        // 复制后缀
        if copy_len > 0 {
            self.data[current_len..current_len + copy_len].copy_from_slice(&suffix_bytes[..copy_len]);
        }
        // 确保以\0结尾（数组已初始化为0，新复制的位置后面已经是0）
    }

    /// 获取原始字节数组
    pub fn as_bytes(&self) -> &[u8; PROC_NAME_LEN] {
        &self.data
    }
}

impl Default for ProcName {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for ProcName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl core::fmt::Display for ProcName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 信号集类型（位图）
/// 对应 C 的 sigset_t，用 64 位表示 64 个信号
#[derive(Debug, Clone, Copy, Default)]
pub struct SigSet(pub u64);

impl SigSet {
    /// 创建空的信号集
    pub const fn empty() -> Self {
        Self(0)
    }

    /// 检查信号是否在集合中
    pub fn contains(self, sig: u8) -> bool {
        if sig == 0 || sig > 64 {
            return false;
        }
        (self.0 >> (sig - 1)) & 1 == 1
    }

    /// 添加信号到集合
    pub fn add(&mut self, sig: u8) {
        if sig == 0 || sig > 64 {
            return;
        }
        self.0 |= 1 << (sig - 1);
    }

    /// 从集合中移除信号
    pub fn remove(&mut self, sig: u8) {
        if sig == 0 || sig > 64 {
            return;
        }
        self.0 &= !(1 << (sig - 1));
    }

    /// 清空信号集
    pub fn clear(&mut self) {
        self.0 = 0;
    }

    /// 检查信号集是否为空
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl RtsFlags {
    pub fn new(value: u32) -> Self {
        Self(AtomicU32::new(value))
    }

    pub fn load(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    pub fn store(&self, value: u32) {
        self.0.store(value, Ordering::Release);
    }

    pub fn is_runnable(&self) -> bool {
        self.load() == 0
    }

    pub fn is_set(&self, flags: u32) -> bool {
        (self.load() & flags) == flags
    }

    pub fn set(&self, flags: u32) {
        self.0.fetch_or(flags, Ordering::AcqRel);
    }

    pub fn clear(&self, flags: u32) {
        self.0.fetch_and(!flags, Ordering::AcqRel);
    }
}

impl MiscFlags {
    pub fn new(value: u32) -> Self {
        Self(AtomicU32::new(value))
    }

    pub fn load(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }

    pub fn is_set(&self, flags: u32) -> bool {
        (self.load() & flags) == flags
    }

    pub fn set(&self, flags: u32) {
        self.0.fetch_or(flags, Ordering::AcqRel);
    }

    pub fn clear(&self, flags: u32) {
        self.0.fetch_and(!flags, Ordering::AcqRel);
    }
}

impl KProcess {
    pub fn new(nr: ProcNr, endpoint: Endpoint) -> Self {
        Self {
            p_nr: nr,
            p_endpoint: endpoint,
            p_rts_flags: RtsFlags::new(rts::SLOT_FREE),
            p_misc_flags: MiscFlags::new(0),
            p_sched: SchedFields::new(),
            p_accounting: Accounting::new(),
            p_time: TimeStats::new(),
            p_cycles: CyclesStats::new(),
            // IPC 队列指针初始化为 None
            p_nextready: None,
            p_caller_q: None,
            p_q_link: None,
            // IPC 端点字段初始化为 NONE
            p_getfrom_e: Endpoint::NONE,
            p_sendto_e: Endpoint::NONE,
            // 信号字段初始化为空
            p_pending: SigSet::empty(),
            // 进程名称初始化为空
            p_name: ProcName::new(),
            // 消息字段初始化为空
            p_sendmsg: Message::default(),
            p_delivermsg: Message::default(),
            p_delivermsg_vir: VirBytes::new(0),
        }
    }

    pub fn is_runnable(&self) -> bool {
        self.p_rts_flags.is_runnable()
    }

    pub fn get_priority(&self) -> i8 {
        self.p_sched.priority.load(Ordering::Acquire)
    }

    pub fn set_priority(&self, priority: i8) {
        self.p_sched.priority.store(priority, Ordering::Release);
    }

    pub fn reset_accounting(&self) {
        self.p_accounting.reset();
    }
}

/// 创建进程
pub fn create_process() -> KProcess {
    KProcess::new(0, Endpoint::default())
}

/// 复制进程（fork）
pub fn copy_process(proc: &KProcess) -> KProcess {
    let new_proc = KProcess::new(proc.p_nr, proc.p_endpoint);
    new_proc.set_priority(proc.get_priority());
    new_proc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rts_flags_runnable() {
        let flags = RtsFlags::new(0);
        assert!(flags.is_runnable());

        flags.set(rts::PROC_STOP);
        assert!(!flags.is_runnable());

        flags.clear(rts::PROC_STOP);
        assert!(flags.is_runnable());
    }

    #[test]
    fn test_rts_flags_multiple() {
        let flags = RtsFlags::new(0);
        flags.set(rts::SENDING | rts::RECEIVING);

        assert!(flags.is_set(rts::SENDING));
        assert!(flags.is_set(rts::RECEIVING));
        assert!(!flags.is_runnable());

        flags.clear(rts::SENDING);
        assert!(!flags.is_set(rts::SENDING));
        assert!(flags.is_set(rts::RECEIVING));
    }

    #[test]
    fn test_kprocess_new() {
        let proc = KProcess::new(1, Endpoint(1));

        assert_eq!(proc.p_nr, 1);
        assert!(!proc.is_runnable());
    }

    #[test]
    fn test_kprocess_runnable() {
        let proc = KProcess::new(1, Endpoint(1));
        proc.p_rts_flags.clear(rts::SLOT_FREE);

        assert!(proc.is_runnable());
    }

    #[test]
    fn test_priority_valid() {
        assert!(Priority::new(0).is_some());
        assert!(Priority::new(7).is_some());
        assert!(Priority::new(15).is_some());
        assert!(Priority::new(-1).is_none());
        assert!(Priority::new(16).is_none());
    }

    #[test]
    fn test_priority_default() {
        let p = Priority::default();
        assert_eq!(p.get(), priority::USER_Q);
    }

    #[test]
    fn test_priority_is_kernel() {
        let kernel = Priority::new(priority::TASK_Q).unwrap();
        assert!(kernel.is_kernel());

        let user = Priority::new(priority::USER_Q).unwrap();
        assert!(!user.is_kernel());
    }

    #[test]
    fn test_quantum_allocate() {
        let q = Quantum::new(200);
        assert_eq!(q.size_ms.load(Ordering::Relaxed), 200);

        q.allocate(1_000_000);
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 200_000_000);
    }

    #[test]
    fn test_quantum_consume() {
        let q = Quantum::new(200);
        q.allocate(1);  // cpu_time_left = 200 * 1 = 200

        assert!(!q.consume(50));  // 200 - 50 = 150
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 150);

        assert!(q.consume(200));  // 150 < 200, 用完
        assert_eq!(q.cpu_time_left.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_sched_fields_new() {
        let sf = SchedFields::new();
        assert_eq!(sf.priority.load(Ordering::Relaxed), priority::USER_Q);
        assert_eq!(sf.cpu.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_kprocess_priority() {
        let proc = KProcess::new(1, Endpoint(1));
        assert_eq!(proc.get_priority(), priority::USER_Q);

        proc.set_priority(priority::TASK_Q);
        assert_eq!(proc.get_priority(), priority::TASK_Q);
    }

    #[test]
    fn test_accounting_new() {
        let acc = Accounting::new();
        assert_eq!(acc.enter_queue.load(Ordering::Relaxed), 0);
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_reset() {
        let acc = Accounting::new();
        acc.dequeues.store(10, Ordering::Release);
        acc.ipc_sync.store(5, Ordering::Release);
        acc.reset();
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 0);
        assert_eq!(acc.ipc_sync.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_enqueue_dequeue() {
        let acc = Accounting::new();
        acc.record_enqueue(1000);
        acc.record_dequeue(1500);

        assert_eq!(acc.time_in_queue.load(Ordering::Relaxed), 500);
        assert_eq!(acc.dequeues.load(Ordering::Relaxed), 1);
        assert_eq!(acc.enter_queue.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_accounting_ipc() {
        let acc = Accounting::new();
        acc.record_ipc_sync();
        acc.record_ipc_sync();
        acc.record_ipc_async();

        assert_eq!(acc.ipc_sync.load(Ordering::Relaxed), 2);
        assert_eq!(acc.ipc_async.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_time_stats() {
        let ts = TimeStats::new();
        ts.add_user_time(10);
        ts.add_sys_time(5);

        assert_eq!(ts.user_time.load(Ordering::Relaxed), 10);
        assert_eq!(ts.sys_time.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_virt_timer() {
        let ts = TimeStats::new();
        ts.virt_left.store(3, Ordering::Release);

        assert!(!ts.tick_virt_timer());
        assert!(!ts.tick_virt_timer());
        assert!(ts.tick_virt_timer());
        assert!(!ts.tick_virt_timer());
    }

    #[test]
    fn test_cycles_stats() {
        let cs = CyclesStats::new();
        cs.add_cycles(1000);
        cs.add_kcall_cycles(200);
        cs.add_kipc_cycles(50);

        assert_eq!(cs.total.load(Ordering::Relaxed), 1000);
        assert_eq!(cs.kcall.load(Ordering::Relaxed), 200);
        assert_eq!(cs.kipc.load(Ordering::Relaxed), 50);
    }

    #[test]
    fn test_kprocess_accounting() {
        let proc = KProcess::new(1, Endpoint(1));
        proc.p_accounting.record_ipc_sync();
        assert_eq!(proc.p_accounting.ipc_sync.load(Ordering::Relaxed), 1);

        proc.reset_accounting();
        assert_eq!(proc.p_accounting.ipc_sync.load(Ordering::Relaxed), 0);
    }
}
