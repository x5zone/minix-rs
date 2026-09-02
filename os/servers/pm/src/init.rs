//! PM 启动链：SEF 回调注册 + `sef_cb_init_fresh` 等价实现。
//!
//! C 对应: `minix3/minix/servers/pm/main.c:49-268`（main / sef_local_startup /
//!         sef_cb_init_fresh）+ `minix3/minix/servers/pm/schedule.c:36-69`
//!         （sched_init 调用点）。
//! 文档: `notes/rewrite/fork-syscall-rewrite/04-stage-pm/01-pm-init-main.md`
//!
//! 主循环（run）细节见 04-ipc-dispatch.md；VFS 异步回复状态机见
//! 05-vfs-interaction.md；调度协议见 16-scheduling.md。
//!
//! # 单线程模型
//!
//! PM 是用户态服务器（单线程事件循环）。本模块全部状态由
//! `PmServer` 独占持有，无跨线程共享；`Cell`/`&mut` 在单线程下安全。

use crate::event::EventRegistry;
use crate::ipc::{
    IpcTransport, KernelIpcTransport, PmServices, ReplyIntent, dispatch_message,
    handle_vfs_reply, is_vfs_pm_rs,
};
use crate::mproc::{Guardianship, INIT_PID, Lifecycle, Privilege, ProcTable, SigSet};
use minix_types::{ProcEventMask, VirBytes, PROC_EVENT_REPLY};
use alloc::vec::Vec;
use minix_types::{Endpoint, Message, NR_BOOT_PROCS, NR_PROCS, UserSlot};

// ── 常量（C: minix3/minix/servers/pm + include）──

/// C: `MULTIBOOT_PARAM_BUF_SIZE` — include/arch/earm/include/multiboot.h:240。
pub const MULTIBOOT_PARAM_BUF_SIZE: usize = 1024;

/// C: `VFS_PM_RQ_BASE` / `VFS_PM_INIT` — include/minix/com.h:513/520。
///
/// 协议常量与 `VfsPmInit` 编解码已上移 minix-types（PM/VFS 单一事实源）。
pub use minix_types::{VFS_PM_INIT, VFS_PM_RQ_BASE, VfsPmInit};

/// C: `VFS_PROC_NR` — include/minix/com.h:61。
pub const VFS_PROC_NR: i32 = 1;
/// C: `RS_PROC_NR` — include/minix/com.h:62。
pub const RS_PROC_NR: i32 = 2;
/// C: `INIT_PROC_NR` — include/minix/com.h:72（= LAST_SPECIAL_PROC_NR）。
pub const INIT_PROC_NR: i32 = 11;

/// C: `NR_SCHED_QUEUES` — include/minix/config.h:66。
pub const NR_SCHED_QUEUES: i32 = 16;
/// C: `MAX_USER_Q` — include/minix/config.h:68。
pub const MAX_USER_Q: i32 = 0;
/// C: `MIN_USER_Q` — include/minix/config.h:71（= NR_SCHED_QUEUES - 1）。
pub const MIN_USER_Q: i32 = NR_SCHED_QUEUES - 1;
/// C: `USER_Q` — include/minix/config.h:69（默认用户队列）。
pub const USER_Q: i32 = (MIN_USER_Q - MAX_USER_Q) / 2 + MAX_USER_Q;
/// C: `USR_Q` — include/minix/priv.h:95（用户进程，= USER_Q）。
pub const USR_Q: i32 = USER_Q;
/// C: `SRV_Q` — include/minix/priv.h:93（系统服务，= USER_Q）。
pub const SRV_Q: i32 = USER_Q;

/// C: `PRIO_MIN` / `PRIO_MAX` — sys/sys/resource.h:43-44（libc 提供）。
pub const PRIO_MIN: i32 = -20;
pub const PRIO_MAX: i32 = 20;

// ── 信号编号（C: sys/sys/signal.h）──
//
// 仅供本模块构建编译期信号集合使用；完整信号语义见 11-signal-core.md /
// 12-signal-handlers.md。

/// C: `SIGQUIT` — signal.h:48。
const SIGQUIT: i32 = 3;
/// C: `SIGILL` — signal.h:49。
const SIGILL: i32 = 4;
/// C: `SIGTRAP` — signal.h:50。
const SIGTRAP: i32 = 5;
/// C: `SIGABRT` — signal.h:51。
const SIGABRT: i32 = 6;
/// C: `SIGEMT` — signal.h:52。
const SIGEMT: i32 = 7;
/// C: `SIGFPE` — signal.h:53。
const SIGFPE: i32 = 8;
/// C: `SIGBUS` — signal.h:56。
const SIGBUS: i32 = 10;
/// C: `SIGSEGV` — signal.h:57。
const SIGSEGV: i32 = 11;
/// C: `SIGCONT` — signal.h:63。
const SIGCONT: i32 = 19;
/// C: `SIGCHLD` — signal.h:64。
const SIGCHLD: i32 = 20;
/// C: `SIGWINCH` — signal.h:72。
const SIGWINCH: i32 = 28;
/// C: `SIGINFO` — signal.h:73。
const SIGINFO: i32 = 29;

/// 构造单个信号的位图（SigSet = u64，bit i 对应信号 i；_NSIG = 64）。
const fn sig_bit(sig: i32) -> SigSet {
    1u64 << sig
}

/// 引发 core dump 的信号集合。
///
/// C: `core_sigs[]` — main.c:137-138：SIGQUIT/SIGILL/SIGTRAP/SIGABRT/SIGEMT/
/// SIGFPE/SIGBUS/SIGSEGV。
#[allow(dead_code)] // 消费方：11-signal-core.md（signal.rs sig_proc_exit）。
pub(crate) const CORE_SIGSET: SigSet = sig_bit(SIGQUIT)
    | sig_bit(SIGILL)
    | sig_bit(SIGTRAP)
    | sig_bit(SIGABRT)
    | sig_bit(SIGEMT)
    | sig_bit(SIGFPE)
    | sig_bit(SIGBUS)
    | sig_bit(SIGSEGV);

/// 默认忽略的信号集合。
///
/// C: `ign_sigs[]` — main.c:139：SIGCHLD/SIGWINCH/SIGCONT/SIGINFO。
#[allow(dead_code)] // 消费方：11-signal-core.md（signal.rs check_sig）。
pub(crate) const IGN_SIGSET: SigSet =
    sig_bit(SIGCHLD) | sig_bit(SIGWINCH) | sig_bit(SIGCONT) | sig_bit(SIGINFO);

/// 不可忽略的信号集合（即使被设置为忽略也强制默认处理）。
///
/// C: `noign_sigs[]` — main.c:140-141：SIGILL/SIGTRAP/SIGEMT/SIGFPE/SIGBUS/SIGSEGV。
#[allow(dead_code)] // 消费方：11-signal-core.md（signal.rs check_sig）。
pub(crate) const NOIGN_SIGSET: SigSet = sig_bit(SIGILL)
    | sig_bit(SIGTRAP)
    | sig_bit(SIGEMT)
    | sig_bit(SIGFPE)
    | sig_bit(SIGBUS)
    | sig_bit(SIGSEGV);

// ── 启动参数 ──

/// PM 启动参数。
///
/// C 中由 `sys_getmonparams`（main.c:167-170）+ `sys_getimage`
/// （main.c:172-176）+ `sys_hz`（main.c:238）三个内核 IPC 填充全局/静态
/// 缓冲；Rust 侧显式聚合为启动契约（与 VM `BootParams` 同型）。
#[derive(Debug, Clone)]
pub struct BootParams {
    /// Boot monitor 参数缓冲。
    ///
    /// C: `monitor_params`（glo.h:10，`MULTIBOOT_PARAM_BUF_SIZE` 字节）。
    /// 语义（find_param）见 20-misc-queries.md。
    pub monitor_params: [u8; MULTIBOOT_PARAM_BUF_SIZE],
    /// 内核 boot image 表（kernel/table.c 定义，sys_getimage 拷贝）。
    ///
    /// C: `static struct boot_image image[NR_BOOT_PROCS]` — main.c:135。
    pub boot_image: [minix_types::BootImage; NR_BOOT_PROCS],
    /// 系统时钟频率（HZ）。
    ///
    /// C: `system_hz = sys_hz()` — main.c:238；定时器语义见 14-itimer.md。
    pub system_hz: u32,
    /// VFS 进程 endpoint（VFS_PROC_NR）。
    pub vfs_endpoint: Endpoint,
}

impl BootParams {
    /// 占位参数（内核 IPC 落地前的测试/开发用值）。
    ///
    /// boot image 全为空条目（`endpoint == NONE`，`fill_boot_procs` 全部
    /// 跳过——占位参数不会产生任何进程）。真实启动路径必须用
    /// `sys_getimage` 结果替换（minix-sys 落地后）。
    pub fn placeholder() -> Self {
        Self {
            monitor_params: [0; MULTIBOOT_PARAM_BUF_SIZE],
            boot_image: [minix_types::BootImage::empty(); NR_BOOT_PROCS],
            system_hz: 100,
            vfs_endpoint: Endpoint::VFS,
        }
    }
}

// ── PM 服务器 ──

/// PM 服务器：进程表 + 启动参数 + IPC 传输。
///
/// C: 全局 `mproc[]` + `monitor_params` + `system_hz`（glo.h）。
/// Rust: 显式聚合为单一结构（A-2 全局变量 → PmContext/PmServer）。
///
/// `T` 为 IPC 传输实现（生产 `KernelIpcTransport`，测试 mock）。
pub struct PmServer<T: IpcTransport = KernelIpcTransport> {
    /// PM 进程表（mproc）。
    table: ProcTable,
    /// 进程事件注册表（`event.c:60-67` `subs/nsubs/nested` 聚合，ARCH A-3）。
    ///
    /// 文档：`notes/rewrite/fork-syscall-rewrite/04-stage-pm/06-event-subscription.md`。
    event_registry: EventRegistry,
    /// 启动参数。
    params: BootParams,
    /// IPC 传输（VFS_PM_INIT 同步等）。
    transport: T,
    /// `init()` 是否已完成（run() 前置断言）。
    initialized: bool,
    /// 内核中止标志（C: `glo.h:26` `abort_flag`；由 do_reboot 写入，归 20-misc-queries.md）。
    ///
    /// 仅在 `VFS_PM_REBOOT_REPLY` 特例经 [`crate::ipc::vfs::PmServices`] 读取并传给
    /// `sys_abort`（见 05-design.v1.md D8）。
    abort_flag: i32,
}

impl PmServer<KernelIpcTransport> {
    /// 使用生产 IPC 传输创建服务器（空进程表 + 占位参数）。
    pub fn new(params: BootParams) -> Self {
        Self::with_transport(params, KernelIpcTransport::new())
    }
}

impl<T: IpcTransport> PmServer<T> {
    /// 使用显式传输创建服务器（测试注入 mock）。
    pub fn with_transport(params: BootParams, transport: T) -> Self {
        Self {
            // C: 第一步（main.c:146-152）mproc 表初始化——ProcTable::new()
            // 保证空槽（Lifecycle::Unused）+ PID 生成器就绪。
            table: ProcTable::new(),
            event_registry: EventRegistry::new(),
            params,
            transport,
            initialized: false,
            // 内核中止标志初始为 0（无中止）；do_reboot 在 20-misc-queries.md 写入。
            abort_flag: 0,
        }
    }

    /// 进程表引用（测试/断言用）。
    pub fn table(&self) -> &ProcTable {
        &self.table
    }

    /// 执行启动初始化。
    ///
    /// C: `sef_cb_init_fresh` — main.c:131-243。步骤顺序与 C 一一对应：
    ///
    /// ```text
    /// main.c:146-152  mproc 表 + 定时器初始化 → ProcTable::new()（构造时）
    /// main.c:154-165  信号集合构建            → 编译期常量（CORE_SIGSET 等）
    /// main.c:167-170  sys_getmonparams        → BootParams::monitor_params（占位）
    /// main.c:172-176  sys_getimage            → BootParams::boot_image（占位）
    /// main.c:177-229  boot image 填充 mproc    → fill_boot_procs()
    /// main.c:220-236  VFS_PM_INIT 同步         → vfs_init_sync()
    /// main.c:238      system_hz = sys_hz()     → BootParams::system_hz（占位）
    /// main.c:241      sched_init()             → init_scheduling()
    /// ```
    pub fn init(&mut self) {
        // 第 5 步：boot image 填充（INIT + 系统进程）。
        self.fill_boot_procs();

        // 第 6 步：与 VFS 交换进程表（逐条 send + 末条 sendrec 屏障）。
        self.vfs_init_sync();

        // 第 8 步：为 INIT 指定用户态调度器（SCHED 协议细节归 16）。
        self.init_scheduling();

        self.initialized = true;
    }

    /// 进入主循环（永不返回）。
    ///
    /// C: `main()` 主循环 — main.c:59-110。
    ///
    /// 单轮处理在 [`Self::run_once`]（测试可单轮驱动）；`run` 只提供
    /// 无限循环 + receive 失败上限（防 busy-spin，与 VM
    /// `vm_server.rs::run` 同型）。
    pub fn run(&mut self) -> ! {
        assert!(self.initialized, "PmServer::run() called before init()");

        let mut consecutive_recv_failures: u32 = 0;
        loop {
            match self.run_once() {
                RunStep::Handled => consecutive_recv_failures = 0,
                RunStep::ReceiveFailed => {
                    // C: sef_receive_status 阻塞直到消息到达（main.c:61），
                    // 因此 receive Err 意味着传输本身损坏——fail-fast。
                    consecutive_recv_failures = consecutive_recv_failures.saturating_add(1);
                    if consecutive_recv_failures >= MAX_CONSECUTIVE_RECV_FAILURES {
                        panic!(
                            "IPC transport permanently broken: {} consecutive receive failures",
                            consecutive_recv_failures
                        );
                    }
                }
            }
        }
    }

    /// 处理恰好一条 IPC 消息（或一次 receive 失败）。
    ///
    /// 镜像 C 主循环单轮（main.c:59-106）：
    ///
    /// ```text
    /// sef_receive_status      → receive()（transport）
    /// is_ipc_notify           → 跳过（CLOCK → expire_timers 归 14）
    /// pm_isokendpt            → caller 槽位验证（03）
    /// EXITING 丢弃            → 退出中进程的延迟调用直接丢弃
    /// 三路分发                → dispatch_message（VFS/事件/PM 调用）
    /// result != SUSPEND       → reply()（ReplyIntent::Reply）
    /// ```
    ///
    /// [ARCH: A-3] C 全局 `m_in`/`who_p`/`who_e`/`call_nr` 隐式上下文 →
    /// Rust 显式参数（`msg`/`rcv_sts`）+ `UserSlot` 槽位（详见 04 文档 §3.5）。
    fn run_once(&mut self) -> RunStep {
        // C: main.c:61 — sef_receive_status(ANY, &m_in, &ipc_status)。
        let (msg, rcv_sts) = match self.transport.receive() {
            Ok(v) => v,
            Err(_) => return RunStep::ReceiveFailed,
        };

        // C: main.c:65-71 — is_ipc_notify：CLOCK → expire_timers（14）。
        // 通知是异步信号（时钟 tick / 内核中断），不是请求消息，跳过
        // endpoint 验证直接 continue。
        if rcv_sts.is_notify() {
            // CLOCK notify 的 expire_timers 处理归 14-itimer.md（A-7）；
            // 04 只建模"通知跳过"。
            return RunStep::Handled;
        }

        // C: main.c:74-77 — who_e = m_in.m_source；pm_isokendpt 验证。
        // C 对非法 endpoint panic（"PM got message from invalid endpoint"）；
        // Rust 等价 fail-fast（与 C 行为一致，见 04 文档 §3.6 D6）。
        let caller = match self.table.pm_isokendpt(msg.m_source) {
            Ok(slot) => slot,
            Err(_) => panic!(
                "PM got message from invalid endpoint: {}",
                msg.m_source.get()
            ),
        };

        // C: main.c:80-82 — EXITING 进程的延迟调用直接丢弃（continue）。
        let caller_proc = self
            .table
            .get(caller.get())
            .expect("pm_isokendpt validated slot");
        if caller_proc.is_exiting() {
            return RunStep::Handled;
        }

        // C: main.c:84-87 — 第一路：VFS 异步回复（状态机归 05-vfs-interaction.md）。
        // 注意：必须在 dispatch_message 之前拦截——VFS 回复不与普通 PM 调用同路。
        if is_vfs_pm_rs(msg.m_type) && msg.m_source == Endpoint::VFS {
            let mut svc = PmServices::new(
                &mut self.table,
                &mut self.transport,
                &mut self.event_registry,
                self.abort_flag,
            );
            if let Err(e) = handle_vfs_reply(&mut svc, &msg) {
                // C: main.c:317-319 / 324-325 / 417-418 以 panic 兜底四类损坏。
                panic!("handle_vfs_reply failed: {:?}", e);
            }
            return RunStep::Handled;
        }

        // C: main.c:88-89 — 第二路：进程事件订阅者回复（06-event-subscription.md）。
        // 必须在普通 PM 调用之前拦截（与 main.c 84-89 顺序一致：VFS 第一、PROC_EVENT_REPLY 第二）。
        if msg.m_type == PROC_EVENT_REPLY {
            let intent = self.event_registry.do_proc_event_reply(
                &msg,
                caller,
                &mut self.table,
                &mut self.transport,
            );
            // C: main.c:88-89 `result = do_proc_event_reply()` → `SUSPEND` 不回复；
            // 仅 ENOSYS 时回复调用者（误调用的普通进程）。
            if let ReplyIntent::Reply(code) = intent {
                self.reply(caller, code);
            }
            return RunStep::Handled;
        }

        // C: main.c:90-103 — 第三路分发前：PM_PROCEVENTMASK 的掩码更新（06）。
        // `dispatch_pm_call` 当前对未实现调用返回 ENOSYS 占位，但 proceventmask
        // 需经 EventRegistry 真正更新订阅表（否则掩码更新无 side-effect）。
        if msg.m_type == minix_types::PM_PROCEVENTMASK {
            // C: event.c:179 — mask 在 m_lsys_pm_proceventmask.mask
            let mask_bits = unsafe { msg.m_u.m_lsys_pm_proceventmask.mask };
            let mask = ProcEventMask::from_bits_truncate(mask_bits);
            let intent = self.event_registry.do_proceventmask_mut(
                caller,
                mask,
                &mut self.table,
                &mut self.transport,
            );
            if let ReplyIntent::Reply(code) = intent {
                self.reply(caller, code);
            }
            return RunStep::Handled;
        }

        // C: main.c:90-101 — PM_FORK handler（07-pm-fork.md, table.c:23）
        // fork 的容量/EAGAIN 与 SUSPEND 需在分发前处理，以区分同步失败 vs 异步投递
        if msg.m_type == 2 {
            match crate::fork::handle_fork(&mut self.table, msg.m_source, &mut self.transport) {
                Ok(_child_pid) => return RunStep::Handled, // SUSPEND (ReplyLater)
                Err(e) => {
                    let pm_err: minix_types::PmError = e.into();
                    self.reply(caller, pm_err.to_errno());
                    return RunStep::Handled;
                }
            }
        }

        // C: main.c:90-101 — PM_SRV_FORK handler（08-pm-srv-fork.md, table.c:23, forkexit.c:142）
        // RS 专用，EPERM 门 + 立即双回复（reply(child,OK) + return pid），vs fork 的 SUSPEND
        if msg.m_type == 41 {
            // 解码 SrvForkParams (uid/gid) from MessLsysPmSrvFork
            let params = {
                let pl = unsafe { msg.m_u.m_lsys_pm_srv_fork };
                crate::mproc::SrvForkParams {
                    uid: pl.uid,
                    gid: pl.gid,
                }
            };
            match crate::fork::handle_srv_fork(
                &mut self.table,
                msg.m_source,
                params,
                &mut self.transport,
            ) {
                Ok(child_pid) => {
                    // 同步返父 pid (Reply) — handle_srv_fork 已 reply(child,OK)
                    self.reply(caller, child_pid);
                    return RunStep::Handled;
                }
                Err(e) => {
                    let pm_err: minix_types::PmError = e.into();
                    self.reply(caller, pm_err.to_errno());
                    return RunStep::Handled;
                }
            }
        }

        // C: main.c:90-101 — PM_EXIT handler（09-pm-exit.md, table.c:23, forkexit.c:245）
        // do_exit 永不回复（SUSPEND 的 NoReply 子类），PRIV_PROC→SIGKILL 门
        if msg.m_type == 1 {
            let status = unsafe { msg.m_u.m_lc_pm_exit.status };
            let _ = crate::exit::handle_exit(&mut self.table, caller, status, &mut self.transport);
            return RunStep::Handled; // NoReply (beyond the grave)
        }

        // C: main.c:90-101 — PM_WAIT4 handler（10-pm-wait.md, table.c:23, forkexit.c:471）
        // do_wait4 三环 + WNOHANG/ECHILD + SUSPEND（WAITING）
        if msg.m_type == 3 {
            let pidarg = unsafe { msg.m_u.m_lc_pm_wait4.pid };
            let options = unsafe { msg.m_u.m_lc_pm_wait4.options };
            let addr = unsafe { msg.m_u.m_lc_pm_wait4.addr };
            let intent = crate::wait::handle_wait4(
                &mut self.table,
                caller,
                pidarg,
                options as u32,
                VirBytes(addr),
                &mut self.transport,
            );
            // handle_wait4's Reply(pid/0/ECHILD) are synchronous replies (W_STOPCODE, WNOHANG, ECHILD)
            // ReplyLater are async (tell_parent/tell_tracer already replied or WAITING set)
            if let ReplyIntent::Reply(code) = intent {
                self.reply(caller, code);
            }
            return RunStep::Handled;
        }

        // C: main.c:90-101 — PM_KILL handler（11-signal-core.md, signal.c:197）
        if msg.m_type == 11 {
            let pid = unsafe { msg.m_u.m_lc_pm_kill.pid };
            let signo = unsafe { msg.m_u.m_lc_pm_kill.signo };
            match crate::signal::handle_kill(&mut self.table, caller, pid, signo, &mut self.transport) {
                Ok(count) => {
                    // Self-kill SUSPEND check is inside handle_kill (caller Exiting → SUSPEND)
                    if self.table.procs[caller.get()].state.lifecycle.is_exiting() {
                        return RunStep::Handled; // SUSPEND
                    }
                    self.reply(caller, 0);
                    let _ = count;
                    return RunStep::Handled;
                }
                Err(e) => {
                    self.reply(caller, e.to_errno());
                    return RunStep::Handled;
                }
            }
        }

        // C: main.c:90-101 — PM_SRV_KILL handler（11-signal-core.md, signal.c:204）
        if msg.m_type == 42 {
            let pid = unsafe { msg.m_u.m_rs_pm_srv_kill.pid };
            let signo = unsafe { msg.m_u.m_rs_pm_srv_kill.signo };
            match crate::signal::handle_srv_kill(&mut self.table, caller, pid, signo, &mut self.transport) {
                Ok(count) => {
                    if self.table.procs[caller.get()].state.lifecycle.is_exiting() {
                        return RunStep::Handled;
                    }
                    self.reply(caller, 0);
                    let _ = count;
                    return RunStep::Handled;
                }
                Err(e) => {
                    self.reply(caller, e.to_errno());
                    return RunStep::Handled;
                }
            }
        }

        // C: main.c:90-103 — 第三路：普通 PM 调用（剩余 43 个）。
        let intent = dispatch_message(&mut self.table, &msg);

        // C: main.c:106 — result != SUSPEND → reply(who_p, result)。
        if let ReplyIntent::Reply(code) = intent {
            self.reply(caller, code);
        }

        RunStep::Handled
    }

    /// 向调用者发送回复。
    ///
    /// C: `reply(proc_nr, result)` — main.c:250-270：`mp_reply.m_type =
    /// result` 后 `ipc_sendnb(mp_endpoint, &mp_reply)`。
    ///
    /// 与 C 的差异（见 04 文档 §3.5）：C 复用每进程持久 `mp_reply`
    /// 缓冲（handler 可预填载荷字段）；Rust 每次构造 `Message`，载荷
    /// 扩展由各 handler 文档（07~20）落地。发送失败只打印警告不 panic
    /// （C: main.c:267-269 同语义）。
    ///
    /// [ARCH: A-3] 槽位以 `UserSlot` 传递（替代 C 裸 `proc_nr` 索引）。
    fn reply(&mut self, slot: UserSlot, result: i32) {
        let endpoint = self.table.procs[slot.get()].endpoint();
        // C: `rmp->mp_reply.m_type = result` 后 `ipc_sendnb(rmp->mp_endpoint, &rmp->mp_reply)`
        // Rust: if handler pre-filled `ipc.reply` (e.g., wait's W_STOPCODE status), reuse it
        let mut msg = self.table.procs[slot.get()]
            .ipc
            .reply
            .take()
            .unwrap_or_default();
        msg.m_type = result;
        // m_source is set by kernel on receive, not needed for send
        if let Err(e) = self.transport.send(endpoint, &msg) {
            // C: printf("PM can't reply to %d (%s): %d") — 警告不 panic。
            let _ = e;
            #[cfg(test)]
            eprintln!("PM can't reply to {}: {:?}", endpoint.get(), e);
        }
    }

    /// 第 5 步：boot image 填充 mproc 表。
    ///
    /// C: main.c:177-229。只处理 `proc_nr >= 0` 的条目（负值为内核 task）；
    /// INIT 与系统进程两条身份路径分开。
    fn fill_boot_procs(&mut self) {
        let mut procs_in_use = 0usize;
        for ip in self.params.boot_image.iter() {
            if ip.proc_nr < 0 || ip.endpoint.is_none() {
                // C: main.c:179 — task 有负 proc_nr，跳过。
                // Rust 额外跳过 padding 条目：boot image 拷贝进定长
                // `[BootImage; NR_BOOT_PROCS]` 后空槽 `endpoint == NONE`
                // （C 数组精确填充，minix-types 的不精确；与 VM
                // init_boot_procs 同规则）。
                continue;
            }
            let slot = ip.proc_nr as usize;
            procs_in_use += 1;

            let pid = if ip.proc_nr == INIT_PROC_NR {
                INIT_PID
            } else {
                // C: main.c:209 — 系统进程用 get_free_pid() 分配。
                self.table.pid_generator.get_free_pid(&self.table)
            };

            let proc = self
                .table
                .get_mut(slot)
                .expect("boot image proc_nr within NR_PROCS");

            // C: main.c:183-186 — 名字 + 信号集合清零（Rust 默认空位图）。
            proc.identity.name = ip.proc_name;
            proc.identity.id.index = UserSlot::new(slot);
            proc.identity.endpoint = ip.endpoint;

            if ip.proc_nr == INIT_PROC_NR {
                // INIT 分支：C main.c:188-201。
                proc.identity.id.pid = INIT_PID;
                proc.identity.procgrp = INIT_PID;
                // C: main.c:189-194 — INIT 是"自己的父亲"（PM 假设
                // mp_parent 恒指向有效槽位；见 09-pm-exit.md 收养语义）。
                proc.state.guardianship = Guardianship::Normal {
                    parent: UserSlot::new(INIT_PROC_NR as usize),
                };
                proc.state.lifecycle = Lifecycle::Running; // IN_USE
                proc.resources.privilege = Privilege::User(crate::mproc::Credentials::default());
                // C: main.c:199-200 — INIT 初始由内核调度。
                proc.resources.scheduler = Endpoint::KERNEL;
                proc.resources.nice = nice_from_queue(USR_Q);
            } else {
                // 系统进程分支：C main.c:202-215。
                proc.identity.id.pid = pid;
                // C: main.c:203-208 — RS 自身的父亲是 INIT，其余是 RS。
                let parent = if ip.proc_nr == RS_PROC_NR {
                    INIT_PROC_NR as usize
                } else {
                    RS_PROC_NR as usize
                };
                proc.state.guardianship = Guardianship::Normal {
                    parent: UserSlot::new(parent),
                };
                proc.state.lifecycle = Lifecycle::Running; // IN_USE
                proc.resources.privilege = Privilege::Kernel; // PRIV_PROC
                // C: main.c:213-214 — 系统进程由 RS 调度（NONE = 未指定）。
                proc.resources.scheduler = Endpoint::NONE;
                proc.resources.nice = nice_from_queue(SRV_Q);
            }
        }
        self.table.procs_in_use.set(procs_in_use);
    }

    /// 第 6 步：VFS_PM_INIT 进程表同步。
    ///
    /// C: main.c:220-236。逐条 `ipc_send`（每个 boot 进程一条）+ 末条
    /// `ipc_sendrec` 屏障（endpoint = NONE，VFS 回复必须 OK）。
    ///
    /// 与 C 的差异：C 在填充循环内逐条发送；Rust 先完成全部填充再统一
    /// 发送——发送失败都会中止启动（C panic ↔ Rust expect），可观测
    /// 行为一致。
    fn vfs_init_sync(&mut self) {
        let messages = self.vfs_init_messages();
        for msg in &messages[..messages.len() - 1] {
            // C: main.c:226 — ipc_send(VFS_PROC_NR, &mess)，失败 panic。
            self.transport
                .send(self.params.vfs_endpoint, msg)
                .expect("PM: can't sync up with VFS (per-process send)");
        }
        // C: main.c:231-236 — 末条 sendrec，回复 m_type 必须为 OK。
        let mut barrier = *messages.last().expect("at least final barrier message");
        self.transport
            .sendrec(self.params.vfs_endpoint, &mut barrier)
            .expect("PM: can't sync up with VFS (final barrier)");
        assert_eq!(
            barrier.m_type, 0, /* OK */
            "VFS did not confirm PM init (m_type = {})",
            barrier.m_type
        );
    }

    /// 构造全部 VFS_PM_INIT 消息（逐条 + 末条屏障，可独立测试）。
    fn vfs_init_messages(&self) -> Vec<Message> {
        let mut msgs = Vec::new();
        for ip in self.params.boot_image.iter() {
            if ip.proc_nr < 0 || ip.endpoint.is_none() {
                // 与 fill_boot_procs 同规则：跳过内核 task 与 padding 条目。
                continue;
            }
            let slot = ip.proc_nr as usize;
            let proc = &self.table.procs[slot];
            msgs.push(
                VfsPmInit {
                    slot: ip.proc_nr,
                    pid: proc.identity.id.pid,
                    endpoint: proc.identity.endpoint,
                }
                .encode(),
            );
        }
        // C: main.c:231-236 — 末条 endpoint = NONE（无更多系统进程）。
        msgs.push(
            VfsPmInit {
                slot: 0,
                pid: 0,
                endpoint: Endpoint::NONE,
            }
            .encode(),
        );
        msgs
    }

    /// 第 8 步：为 INIT 指定用户态调度器（调用点）。
    ///
    /// C: `sched_init()` — schedule.c:36-69。遍历 mproc，对 `IN_USE` 且
    /// 非 `PRIV_PROC` 的槽（启动时只有 INIT 满足）调 `sched_start`
    /// （SCHEDULING_START 协议，SCHED_PROC_NR=4）。
    ///
    /// DEFERRED: 实际 SCHED 传输归 16-scheduling.md（minix_sched 客户端）。
    fn init_scheduling(&mut self) {
        for slot in 0..NR_PROCS {
            let (in_use, is_priv, endpoint, parent_slot) = {
                let p = &self.table.procs[slot];
                (
                    p.is_in_use(),
                    p.is_kernel_process(),
                    p.endpoint(),
                    p.parent(),
                )
            };
            if !in_use || is_priv {
                // C: schedule.c:43-44 — 系统进程（PRIV_PROC）不接管。
                continue;
            }
            // C: schedule.c:46 — assert(_ENDPOINT_P(endpoint) == INIT_PROC_NR)。
            assert_eq!(
                endpoint.slot(),
                INIT_PROC_NR,
                "only INIT is user-scheduled at boot (slot {} endpoint {:?})",
                slot,
                endpoint
            );
            let parent_endpoint = self.table.procs[parent_slot.get()].endpoint();
            // C: schedule.c:47-48 — assert(parent_e == schedulee_e)。
            assert_eq!(parent_endpoint, endpoint, "INIT parent must be itself");

            // C: schedule.c:49-58 — sched_start(SCHED_PROC_NR, schedulee,
            // parent, USER_Q, USER_QUANTUM, -1, &mp_scheduler)。
            //
            // DEFERRED: minix_sched 客户端（16-scheduling.md）。
            let new_sched = self.sched_start(endpoint, parent_endpoint);
            match new_sched {
                Ok(scheduler) => self.table.procs[slot].resources.scheduler = scheduler,
                Err(()) => {
                    // C: schedule.c:60-67 — 失败仅打印警告，不 panic。
                    #[cfg(test)]
                    eprintln!("PM: SCHED denied taking over scheduling of slot {}", slot);
                }
            }
        }
    }

    /// `sched_start` 调用点（DEFERRED 占位）。
    ///
    /// C: `sched_start` — libsys/sched_start.c:46。成功时返回调度器 endpoint
    /// （SCHED_PROC_NR）；当前返回 `Ok(Endpoint::SCHED)` 占位，真实传输归 16。
    fn sched_start(&mut self, _schedulee: Endpoint, _parent: Endpoint) -> Result<Endpoint, ()> {
        Ok(Endpoint::SCHED)
    }
}

/// queue → nice 转换（调用点）。
///
/// C: `get_nice_value` — main.c:276-295（static）。完整调度语义
/// （nice_to_priority 等）见 16-scheduling.md。
fn nice_from_queue(queue: i32) -> i32 {
    let nice_val = (queue - USER_Q) * (PRIO_MAX - PRIO_MIN + 1) / (MIN_USER_Q - MAX_USER_Q + 1);
    nice_val.clamp(PRIO_MIN, PRIO_MAX)
}

// ── 主循环（04-ipc-dispatch.md）──

/// 连续 receive 失败上限。
///
/// C: sef_receive_status 阻塞等待（main.c:61），失败即传输损坏；
/// 连续失败达到上限后 fail-fast panic（防 busy-spin，与 VM
/// `MAX_CONSECUTIVE_RECV_FAILURES = 64` 同值同语义）。
const MAX_CONSECUTIVE_RECV_FAILURES: u32 = 64;

/// 主循环单轮结果。
///
/// `run()` 据此维护连续失败计数；测试单轮驱动 `run_once` 时用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStep {
    /// 一条消息已处理（或一次失败已计数）。
    Handled,
    /// receive 返回错误（传输损坏，`run()` 计数后可能 panic）。
    ReceiveFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::{IpcStatus, TestIpcTransport};
    use crate::mproc::IpcBlockReason;
    use minix_types::{BootImage, ENOSYS, OK, VFS_PM_SETUID_REPLY};

    fn boot_image_with(entries: &[(i32, [u8; 16], Endpoint)]) -> [BootImage; NR_BOOT_PROCS] {
        let mut image = [BootImage::empty(); NR_BOOT_PROCS];
        for (i, (nr, name, ep)) in entries.iter().enumerate() {
            image[i] = BootImage {
                proc_nr: *nr,
                proc_name: *name,
                endpoint: *ep,
                start_addr: 0,
                len: 0,
            };
        }
        image
    }

    fn name(s: &str) -> [u8; 16] {
        let mut n = [0u8; 16];
        n[..s.len()].copy_from_slice(s.as_bytes());
        n
    }

    fn test_params() -> BootParams {
        BootParams {
            monitor_params: [0; MULTIBOOT_PARAM_BUF_SIZE],
            boot_image: boot_image_with(&[
                (0, name("pm"), Endpoint::PM),
                (1, name("vfs"), Endpoint::VFS),
                (2, name("rs"), Endpoint::RS),
                (11, name("init"), Endpoint::INIT),
                (-3, name("clock"), Endpoint::CLOCK),
            ]),
            system_hz: 100,
            vfs_endpoint: Endpoint::VFS,
        }
    }

    #[test]
    fn test_signal_sets_match_c() {
        // C: main.c:154-165 + sys/sys/signal.h。
        assert_eq!(
            CORE_SIGSET,
            (1 << 3) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 8) | (1 << 10) | (1 << 11)
        );
        assert_eq!(IGN_SIGSET, (1 << 19) | (1 << 20) | (1 << 28) | (1 << 29));
        assert_eq!(
            NOIGN_SIGSET,
            (1 << 4) | (1 << 5) | (1 << 7) | (1 << 8) | (1 << 10) | (1 << 11)
        );
        // 全部信号编号 ≤ 29 < _NSIG = 64（u64 位图内）。
        let highest_used = CORE_SIGSET | IGN_SIGSET | NOIGN_SIGSET;
        assert_eq!(highest_used.ilog2() as i32, 29);
    }

    #[test]
    fn test_nice_from_queue_default_queues() {
        // USR_Q = SRV_Q = USER_Q = 7 → nice 0（main.c:200/214）。
        assert_eq!(nice_from_queue(USR_Q), 0);
        assert_eq!(nice_from_queue(SRV_Q), 0);
        // 线性缩放端点（main.c:281-289）：(q-7)*41/16，C 向零截断。
        assert_eq!(nice_from_queue(MAX_USER_Q), -17);
        assert_eq!(nice_from_queue(MIN_USER_Q), 20);
    }

    #[test]
    fn test_fill_boot_init_identity() {
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.fill_boot_procs();

        let init = server.table().get(INIT_PROC_NR as usize).unwrap();
        assert_eq!(init.identity.id.pid, 1); // INIT_PID
        assert_eq!(init.identity.procgrp, 1);
        assert_eq!(init.parent(), UserSlot::new(11)); // 自己的父亲
        assert!(init.is_in_use());
        assert_eq!(init.resources.scheduler, Endpoint::KERNEL);
        assert_eq!(init.resources.nice, 0);
        assert_eq!(init.identity.endpoint, Endpoint::INIT);
    }

    #[test]
    fn test_fill_boot_system_procs() {
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.fill_boot_procs();

        // 系统进程：parent = RS（slot 2），PRIV_PROC，scheduler = NONE。
        let pm = server.table().get(0).unwrap();
        assert_eq!(pm.parent(), UserSlot::new(2));
        assert!(pm.is_kernel_process());
        assert_eq!(pm.resources.scheduler, Endpoint::NONE);
        assert_eq!(pm.identity.endpoint, Endpoint::PM);

        // RS 自身的父亲是 INIT（main.c:204-206）。
        let rs = server.table().get(2).unwrap();
        assert_eq!(rs.parent(), UserSlot::new(11));

        // PID 分配：PM→2, VFS→3, RS→4（get_free_pid 顺序）。
        assert_eq!(server.table().get(0).unwrap().identity.id.pid, 2);
        assert_eq!(server.table().get(1).unwrap().identity.id.pid, 3);
        assert_eq!(server.table().get(2).unwrap().identity.id.pid, 4);

        // 负 proc_nr（内核 task）跳过，不计入 procs_in_use。
        assert_eq!(server.table().procs_in_use.get(), 4);
    }

    #[test]
    fn test_vfs_init_message_fields() {
        // com.h:520/547-551 — VFS_PM_ENDPT=m7_i1, VFS_PM_SLOT=m7_i2,
        // VFS_PM_PID=m7_i3。
        let msg = VfsPmInit {
            slot: 11,
            pid: 1,
            endpoint: Endpoint::INIT,
        }
        .encode();
        assert_eq!(msg.m_type, VFS_PM_INIT);
        // SAFETY: 刚由 encode() 完整写入 m_m7。
        let m7 = unsafe { msg.m_u.m_m7 };
        assert_eq!(m7.m7i1, Endpoint::INIT.get());
        assert_eq!(m7.m7i2, 11);
        assert_eq!(m7.m7i3, 1);
    }

    #[test]
    fn test_vfs_init_messages_order_and_final() {
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.fill_boot_procs();
        let msgs = server.vfs_init_messages();

        // 4 条逐条（PM/VFS/RS/INIT）+ 1 条末条屏障。
        assert_eq!(msgs.len(), 5);
        let last = msgs.last().unwrap();
        assert_eq!(last.m_type, VFS_PM_INIT);
        // SAFETY: 末条由 encode() 写入 m_m7。
        let m7 = unsafe { last.m_u.m_m7 };
        assert_eq!(m7.m7i1, Endpoint::NONE.get());
        assert_eq!(m7.m7i2, 0);
        assert_eq!(m7.m7i3, 0);
    }

    // ── 主循环（04-ipc-dispatch.md）──

    fn running_proc_at(table: &mut ProcTable, slot: usize, ep: Endpoint) {
        let p = &mut table.procs[slot];
        p.identity.endpoint = ep;
        p.state.lifecycle = Lifecycle::Running;
    }

    #[test]
    fn test_run_once_skips_notify() {
        // C: main.c:65-71 — is_ipc_notify → continue（通知不产生回复）。
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let mut msg = Message::default();
        msg.m_type = 0x1000; // NOTIFY_MESSAGE（com.h:90）
        msg.m_source = Endpoint::CLOCK;
        server.transport.queue_receive(msg, IpcStatus { flags: 4 });
        assert_eq!(server.run_once(), RunStep::Handled);
        assert!(server.transport.sent().is_empty());
    }

    #[test]
    #[should_panic(expected = "invalid endpoint")]
    fn test_run_once_invalid_endpoint_panics() {
        // C: main.c:75-76 — pm_isokendpt 失败 panic（fail-fast）。
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let mut msg = Message::default();
        msg.m_type = 2; // PM_FORK
        msg.m_source = Endpoint::from_generation_slot(9, 9); // 未注册槽位
        server.transport.queue_receive(msg, IpcStatus::default());
        let _ = server.run_once();
    }

    #[test]
    fn test_run_once_drops_exiting_caller() {
        // C: main.c:80-82 — EXITING 进程的延迟调用直接丢弃（continue）。
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let ep = Endpoint::from_generation_slot(1, 5);
        let p = &mut server.table.procs[5];
        p.identity.endpoint = ep;
        p.state.lifecycle = Lifecycle::Exiting {
            exit_code: 0,
            sig_status: 0,
        };
        let mut msg = Message::default();
        msg.m_type = 2; // PM_FORK
        msg.m_source = ep;
        server.transport.queue_receive(msg, IpcStatus::default());
        assert_eq!(server.run_once(), RunStep::Handled);
        assert!(server.transport.sent().is_empty());
    }

    #[test]
    fn test_run_once_replies_enosys_to_unimplemented_call() {
        // C: main.c:90-101 + table.c — 已注册但 handler 未实现的调用 →
        // ENOSYS 占位；主循环 reply(who_p, result)（main.c:106）。
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let ep = Endpoint::from_generation_slot(1, 5);
        running_proc_at(&mut server.table, 5, ep);
        let mut msg = Message::default();
        msg.m_type = 18; // PM_GETMCONTEXT（handler 归 20-misc-queries.md，尚未实现）
        msg.m_source = ep;
        server.transport.queue_receive(msg, IpcStatus::default());
        assert_eq!(server.run_once(), RunStep::Handled);
        let sent = server.transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, ep); // 回复发给 caller endpoint
        assert_eq!(sent[0].1.m_type, ENOSYS);
    }

    #[test]
    fn test_run_once_fork_no_sync_reply() {
        // C: do_fork 返回 SUSPEND（forkexit.c:139）——fork 同步不回复，
        // 回复经 05 的 VFS_PM_FORK_REPLY；但 PM 侧需先 tell_vfs 子进程（VFS_CALL）
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        let ep = Endpoint::from_generation_slot(1, 5);
        running_proc_at(&mut server.table, 5, ep);
        let mut msg = Message::default();
        msg.m_type = 2; // PM_FORK
        msg.m_source = ep;
        server.transport.queue_receive(msg, IpcStatus::default());
        assert_eq!(server.run_once(), RunStep::Handled);
        // handle_fork 已向 VFS 发送 VFS_PM_FORK（子进程 VFS_CALL），但未向父进程同步回复
        assert_eq!(server.transport.sent().len(), 1);
        assert_eq!(server.transport.sent()[0].0, Endpoint::VFS);
        assert_eq!(server.transport.sent()[0].1.m_type, minix_types::VFS_PM_FORK);
        // 子进程槽应处于 VFS_CALL（延续挂在子进程）
        assert!(server
            .table
            .procs
            .iter()
            .any(|p| p.state.block.is_vfs_blocked()));
    }

    #[test]
    fn test_run_once_vfs_reply_no_sync_reply() {
        // C: main.c:84-87 — VFS 异步回复经 handle_vfs_reply 状态机即时处理，
        // 不向 VFS 同步回复；状态机向"被回复进程"施加效果（此处 reply(OK)）。
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        // m_source 验证（pm_isokendpt）需要 VFS 槽位 endpoint 已登记。
        running_proc_at(&mut server.table, 1, Endpoint::VFS);
        // 被回复进程（slot 5）处于 VFS_CALL 状态，其 endpoint 写入回复 m7_i1。
        let ep = Endpoint::from_generation_slot(1, 5);
        running_proc_at(&mut server.table, 5, ep);
        server.table.procs[5].state.block.ipc_blocked =
            Some(IpcBlockReason::VfsCall { reply_to_new_parent: false });
        let mut msg = Message::default();
        msg.m_type = VFS_PM_SETUID_REPLY; // SETUID 分支：reply(OK) + 尾部 restart_signals
        msg.m_source = Endpoint::VFS;
        // SAFETY: 回复消息的 m7_i1 承载目标进程 endpoint（main.c:315-321）。
        unsafe { msg.m_u.m_m7.m7i1 = ep.get(); }
        server.transport.queue_receive(msg, IpcStatus::default());
        assert_eq!(server.run_once(), RunStep::Handled);
        // 状态机已即时施加效果：向 slot 5 进程回复 OK（而非向 VFS 同步回复）。
        let sent = server.transport.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, ep); // 回复发给被回复进程
        assert_eq!(sent[0].1.m_type, OK);
        // VFS_CALL 标志已被状态机清除（take_vfs_call）。
        assert_eq!(server.table.procs[5].state.block.ipc_blocked, None);
    }

    #[test]
    fn test_run_once_receive_failure_reported() {
        // receive 无消息 → ReceiveFailed（run() 计数，防 busy-spin）。
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        assert_eq!(server.run_once(), RunStep::ReceiveFailed);
    }

    #[test]
    #[should_panic(expected = "permanently broken")]
    fn test_run_panics_after_consecutive_receive_failures() {
        // C: sef_receive_status 阻塞等待（main.c:61）；传输损坏时
        // fail-fast（与 VM V10-P0-2 同型）。
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.initialized = true;
        let _ = server.run();
    }

    #[test]
    fn test_init_completes_with_mock_transport() {
        let mut server = PmServer::with_transport(test_params(), TestIpcTransport::new());
        server.init();
        assert!(server.initialized);

        // VFS 同步：4 条逐条 send + 1 条 sendrec（也记录在 sent）。
        let sent = server.transport.sent();
        assert_eq!(sent.len(), 5);
        assert_eq!(sent[0].0, Endpoint::VFS);
        assert_eq!(sent[0].1.m_type, VFS_PM_INIT);
    }
}
