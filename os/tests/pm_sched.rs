//! E5(e) — PM↔SCHED 调度链联调（宿主态，走 wire 契约）。
//!
//! edge_todo.md 2026-09-09 节登记的验收面：sched 是全仓唯一"每个行为都
//! 依赖他方主动来电"的服务（PM 的 START/INHERIT/SET_NICE、内核的
//! NO_QUANTUM、自身的 5 秒平衡闹钟），三链此前只有各 crate 内部单测，
//! 跨服务 wire 零覆盖。本文件即其宿主半（new_edge4 §5），形态沿 E5(a)
//! 的 pm_vm_fork.rs 判例：三侧真代码对跑，只有不可宿主驱动的一端以
//! wire 契约代替。
//!
//! # 链路面
//!
//! 1. **PM**：`minix_pm::sched::{sched_init, sched_start_user}` 真状态机
//!    跑全步，出站走真实 `MinixSchedCtl`（而非 trait 替身）——SCHEDULING_*
//!    消息的组包、`sendrec` 往返与 rv 判定全在生产代码路径上。PM 侧传输
//!    是自带的 `PmSide`（实现 PM 的 `IpcTransport`）：`sendrec` 把请求投
//!    进共享 bus 后当场泵一轮 SCHED 主循环再取回执，等价 C 阻塞
//!    `ipc_sendrec` 的时序（minix3/minix/lib/libsys/taskcall.c:16）。
//! 2. **SCHED**：`minix_sched::server::SchedServer::run_once` 真循环消费
//!    bus 上的请求——五字母分发、出生门/占用门/封印门、扇出环全在生
//!    产路径上。它的 IPC 侧是自带的 `SchedSide`（实现 SCHED 的
//!    `IpcTransport`），内核侧是自带的 `CapturedKernel`（实现 SCHED 的
//!    `KernelApi`）。
//! 3. **内核**：`minix-kernel` 不进依赖（C-46 记录的 workspace 特性统
//!    一三错为预存基线，且 SYS_SCHEDCTL/SYS_SCHEDULE 的解码半已由
//!    K1/K2 在内核 crate 单测钉死）。对账物是 SCHED 生产渲染函数产出
//!    的 minix-types 线束：`SchedctlCall::wire()`（对位内核
//!    `dispatch_schedctl` 的读序，os/kernel/src/syscall_process.rs:630）
//!    与 `Fanout::wire_*`（对位 `do_schedule` 的 SchedParams 读序，
//!    os/kernel/src/sched.rs:272-277）；NO_QUANTUM 到达侧按内核
//!    `notify_scheduler` 的线形构造（os/kernel/src/proc_table.rs:969-999，
//!    m_source = 耗尽者、FROM_KERNEL 封印、m_krn_lsys_schedule 账目载荷）。
//!
//! # 覆盖（三链 + 一断环守卫）
//!
//! - **START 链**：PM `sched_init` → SCHEDULING_START → `sys_schedctl`
//!   注册接管 + `sys_schedule` 全字段下发 → 回执 OK 且命名 SCHED。
//! - **INHERIT 链**：fork 子进程 `sched_start_user` → SCHEDULING_INHERIT
//!   → 子槽继承父槽现值（优先级/时间片，C schedule.c:199-211）。
//! - **NO_QUANTUM 回环**：内核通知（带封印）→ 降一级 → `sys_schedule`
//!   原地回写（LOCAL：CPU KEEP）且永不回复（C main.c:73-77）；伪造封印
//!   → EPERM 回复（C main.c:78-83）。
//! - **拒绝透传**：二次 START 撞 SCHED 占用门（EDEADEPT）→ PM 的 rv =
//!   回复 m_type（C taskcall.c:17-20），拒绝不再被读成成功。

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use minix_pm::ipc::IpcTransport as PmIpc;
use minix_pm::ipc::IpcTransportError;
use minix_pm::mproc::{Credentials, Guardianship, Lifecycle, Privilege, ProcTable};
use minix_pm::sched::{
    MinixSchedCtl, SCHED_PROC_NR, USER_Q, USER_QUANTUM, sched_init, sched_start_user,
};
use minix_sched::cpu::MachineTopology;
use minix_sched::kernel_api::schedctl::{SchedctlCall, SchedulerAssignment};
use minix_sched::kernel_api::schedule::Fanout;
use minix_sched::kernel_api::transport::{IpcTransport as SchedIpc, KernelApi};
use minix_sched::sef::MachineInfo;
use minix_sched::server::{SchedServer, Step};
use minix_sys::ipc::{CALL_RECEIVE, IpcStatus, STATUS_FLAG_FROM_KERNEL};
use minix_types::ipc::{MessKrnLsysSchedule, MessLsysKrnSchedule};
use minix_types::{EDEADEPT, EPERM, Endpoint, Message, SCHEDULING_NO_QUANTUM, UserSlot};

// ---------------------------------------------------------------------------
// 共享 bus：PM 的 sendrec 与 SCHED 的 receive/send 在此会合
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BusInner {
    /// PM → SCHED 的请求队列，随消息携带到达状态字（普通调用无旗标、
    /// 内核通知带 FROM_KERNEL 封印——SCHED 的 noquantum_trust 读它）。
    to_sched: RefCell<VecDeque<(Message, IpcStatus)>>,
    /// SCHED → PM 的回执队列（`sendrec` 的回程腿）。
    to_pm: RefCell<VecDeque<Message>>,
    /// SCHED 每次 `send` 的落地记录（回复面断言用）。
    sched_sent: RefCell<Vec<(Endpoint, Message)>>,
}

#[derive(Clone, Default)]
struct Bus(Rc<BusInner>);

impl Bus {
    /// 内核侧投递：绕过 PM 直接把一条到达放进 SCHED 的收件队列
    /// （NO_QUANTUM 链用——内核 `mini_send` 不经过 PM）。
    fn deliver_from_kernel(&self, message: Message) {
        self.0
            .to_sched
            .borrow_mut()
            .push_back((message, IpcStatus::with_flags(STATUS_FLAG_FROM_KERNEL)));
    }
}

/// SCHED 侧传输：收件出队、发送落账并入回执队列。
struct SchedSide {
    bus: Bus,
}

impl SchedIpc for SchedSide {
    fn receive(&self) -> Result<(Message, IpcStatus), i32> {
        self.bus.0.to_sched.borrow_mut().pop_front().ok_or(0)
    }

    fn send(&self, to: Endpoint, message: &Message) -> Result<(), i32> {
        self.bus.0.sched_sent.borrow_mut().push((to, *message));
        self.bus.0.to_pm.borrow_mut().push_back(*message);
        Ok(())
    }
}

/// SCHED 侧内核缝：按生产 `SysKernelApi` 的同一渲染函数族记账
/// （transport.rs:356-364 的 wire 顺序），应答可脚本化（默认 OK）。
#[derive(Default)]
struct CapturedKernel {
    schedctl_calls: RefCell<Vec<SchedctlCall>>,
    schedctl_rvs: RefCell<VecDeque<i32>>,
    schedule_calls: RefCell<Vec<MessLsysKrnSchedule>>,
    schedule_rvs: RefCell<VecDeque<i32>>,
    setalarm_calls: RefCell<Vec<u32>>,
    hz: u32,
}

impl CapturedKernel {
    fn with_hz(hz: u32) -> Self {
        Self {
            hz,
            ..Default::default()
        }
    }
}

impl KernelApi for CapturedKernel {
    fn get_machine(&mut self) -> Result<MachineInfo, i32> {
        Ok(MachineInfo {
            processors_count: 2,
            bsp_id: 0,
        })
    }

    fn get_hz(&mut self) -> Result<u32, i32> {
        Ok(self.hz)
    }

    fn schedctl(&mut self, call: &SchedctlCall) -> Result<(), i32> {
        self.schedctl_calls.borrow_mut().push(*call);
        match self.schedctl_rvs.borrow_mut().pop_front() {
            Some(0) | None => Ok(()),
            Some(errno) => Err(errno),
        }
    }

    fn schedule(&mut self, fanout: &Fanout) -> Result<(), i32> {
        self.schedule_calls.borrow_mut().push(MessLsysKrnSchedule {
            endpoint: fanout.endpoint.0,
            quantum: fanout.wire_quantum_ms(),
            priority: fanout.wire_priority(),
            cpu: fanout.wire_cpu(),
            niced: fanout.wire_niced(),
            _padding: [0; 36],
        });
        match self.schedule_rvs.borrow_mut().pop_front() {
            Some(0) | None => Ok(()),
            Some(errno) => Err(errno),
        }
    }

    fn setalarm(&mut self, ticks: u32) -> Result<(), i32> {
        self.setalarm_calls.borrow_mut().push(ticks);
        Ok(())
    }
}

/// PM 侧传输：`sendrec` 投递请求后当场泵一轮 SCHED 主循环，再取回执——
/// 等价 C 阻塞 sendrec 的时序（taskcall.c:16 `ipc_sendrec`）。
struct PmSide<'a> {
    bus: Bus,
    pump: RefCell<&'a mut dyn FnMut()>,
}

impl PmIpc for PmSide<'_> {
    fn receive(&mut self) -> Result<(Message, minix_pm::ipc::IpcStatus), IpcTransportError> {
        Err(IpcTransportError::WouldBlock) // 链上 PM 不收消息
    }

    fn send(&mut self, _dest: Endpoint, _msg: &Message) -> Result<(), IpcTransportError> {
        panic!("E5(e) 链上 PM 只对 SCHED 做 sendrec，无异步发送")
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcTransportError> {
        assert_eq!(dest, SCHED_PROC_NR, "E5(e) 链上 PM 只对 SCHED sendrec");
        // 真内核在投递时以发送者身份盖章 m_source（os/kernel/src/ipc.rs
        // :1067 `p_delivermsg.m_source = caller_endpoint`）；宿主 bus 由
        // 本传输半代行这枚章，SCHED 的来路门（valid.rs accept_message）
        // 靠它认出 PM。
        msg.m_source = Endpoint::PM;
        self.bus
            .0
            .to_sched
            .borrow_mut()
            .push_back((*msg, IpcStatus::from_call(CALL_RECEIVE)));
        (self.pump.borrow_mut())();
        *msg = self
            .bus
            .0
            .to_pm
            .borrow_mut()
            .pop_front()
            .expect("SCHED 必须对 taskcall 回执");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

const INIT_SLOT: usize = 11;
const CHILD_SLOT: usize = 12;

fn init_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, INIT_SLOT as i32)
}

fn child_endpoint() -> Endpoint {
    Endpoint::from_generation_slot(1, CHILD_SLOT as i32)
}

/// INIT 夹具：slot 11 占用、self-parent、scheduler 尚为 KERNEL
/// （C main.c:199 的初始态，schedule.c:37-43 的 START 把它交给 SCHED）。
fn table_with_init() -> ProcTable {
    let mut table = ProcTable::new();
    let p = &mut table.procs[INIT_SLOT];
    p.identity.endpoint = init_endpoint();
    p.identity.id.pid = 1;
    p.state.lifecycle = Lifecycle::Running;
    p.state.guardianship = Guardianship::Normal {
        parent: UserSlot::new(INIT_SLOT),
    };
    p.resources.nice = 0;
    p.resources.scheduler = Endpoint::KERNEL;
    p.resources.privilege = Privilege::User(Credentials::new(0, 0));
    table
}

/// fork 后子进程的 mproc 状态（main.c:373 调用点看到的形状：scheduler
/// 与 nice 都从父槽拷贝）。
fn add_forked_child(table: &mut ProcTable) {
    let c = &mut table.procs[CHILD_SLOT];
    c.identity.endpoint = child_endpoint();
    c.identity.id.pid = 2;
    c.state.lifecycle = Lifecycle::Running;
    c.state.guardianship = Guardianship::Normal {
        parent: UserSlot::new(INIT_SLOT),
    };
    c.resources.nice = 0;
    c.resources.scheduler = Endpoint::SCHED;
    c.resources.privilege = Privilege::User(Credentials::new(0, 0));
}

/// 一台 2 CPU 的 SCHED 服务器 + 已上铃的内核缝（5s × 60Hz = 300 tick）。
fn server_with_kernel() -> (SchedServer, CapturedKernel) {
    let mut kernel = CapturedKernel::with_hz(60);
    let mut server = SchedServer::new(MachineTopology {
        processors_count: 2,
        bsp_id: 0,
    });
    server
        .init_scheduling(&mut kernel)
        .expect("balance bell armed");
    (server, kernel)
}

/// 内核 NO_QUANTUM 通知的线形（proc_table.rs:969-999：m_source = 耗尽者、
/// m_krn_lsys_schedule 账目载荷；FROM_KERNEL 封印由 deliver_from_kernel
/// 的状态字携带）。
fn no_quantum_notify(spender: Endpoint) -> Message {
    let mut message = Message {
        m_type: SCHEDULING_NO_QUANTUM,
        m_source: spender,
        ..Message::default()
    };
    message.m_u.m_krn_lsys_schedule = MessKrnLsysSchedule {
        acnt_queue: 10,
        acnt_deqs: 2,
        acnt_ipc_sync: 1,
        acnt_ipc_async: 0,
        acnt_preempt: 0,
        acnt_cpu: 0,
        acnt_cpu_load: 42,
        _padding: [0; 24],
    };
    message
}

// ---------------------------------------------------------------------------
// E5(e).1 START 链：PM sched_init → SCHED 接管 → 内核下发 → 回执命名 SCHED
// ---------------------------------------------------------------------------

#[test]
fn start_chain_pm_sched_takeover_and_kernel_fanout() {
    let bus = Bus::default();
    let sched_side = SchedSide { bus: bus.clone() };
    let (mut server, mut kernel) = server_with_kernel();
    let mut table = table_with_init();

    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        let results = sched_init(&mut table, &mut ctl);
        assert_eq!(
            results.len(),
            1,
            "用户进程只有 INIT 一个（schedule.c:26-31）"
        );
        assert_eq!(results[0].0, UserSlot::new(INIT_SLOT));
        assert_eq!(
            results[0].1, 0,
            "START 成功：rv = 回复 m_type（taskcall.c:19-20）"
        );
    }

    // PM 侧：INIT 的调度器已由 KERNEL 交给 SCHED（schedule.c:40-48 的
    // 成功半；拒绝半由 refuse 测试钉住）。
    assert_eq!(table.procs[INIT_SLOT].resources.scheduler, Endpoint::SCHED);

    // 内核 wire 一：sys_schedctl 注册接管（flags=0 的 plain 注册，
    // do_schedctl.c:40-43；解码半 os/kernel/src/syscall_process.rs:630）。
    let schedctls = kernel.schedctl_calls.borrow();
    assert_eq!(schedctls.len(), 1);
    assert_eq!(schedctls[0].target, init_endpoint());
    assert_eq!(schedctls[0].assignment, SchedulerAssignment::Server);
    assert_eq!(schedctls[0].wire().flags, 0, "注册分支不携带 KERNEL 旗标");

    // 内核 wire 二：sys_schedule 全字段下发（USER_Q/USER_QUANTUM 出厂
    // 值；解码半 os/kernel/src/sched.rs:272-277 的 SchedParams）。
    let schedules = kernel.schedule_calls.borrow();
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].endpoint, init_endpoint().0);
    assert_eq!(schedules[0].priority, USER_Q, "maxprio = nice 0 → 队列 7");
    assert_eq!(schedules[0].quantum, USER_QUANTUM);
    assert_eq!(schedules[0].cpu, 1, "2 CPU 拓扑：非 BSP 座位承接用户进程");
    assert_eq!(schedules[0].niced, 0, "天花板=USER_Q 不算 niced");

    // 回复面：OK，且回执载荷命名 SCHED——C sched_start.c:73-75 把这个
    // 字段读回 *newscheduler_e，正是 PM 侧回写 Endpoint::SCHED 的权威源。
    let sent = bus.0.sched_sent.borrow();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, Endpoint::PM);
    assert_eq!(sent[0].1.m_type, 0);
    // SAFETY(test): 回执的活跃 union 臂是 m_sched_lsys_scheduling_start
    //（server.rs do_start 的 payload_mut 写入同臂）。
    unsafe {
        assert_eq!(
            sent[0].1.m_u.m_sched_lsys_scheduling_start.scheduler,
            Endpoint::SCHED.0
        );
    }

    // 平衡铃已上（5s × 60Hz），先于一切业务到达。
    assert_eq!(kernel.setalarm_calls.borrow().as_slice(), [300]);
}

// ---------------------------------------------------------------------------
// E5(e).2 INHERIT 链：fork 子进程经 sched_start_user 继承父槽现值
// ---------------------------------------------------------------------------

#[test]
fn inherit_chain_fork_child_copies_parent_slot() {
    let bus = Bus::default();
    let sched_side = SchedSide { bus: bus.clone() };
    let (mut server, mut kernel) = server_with_kernel();
    let mut table = table_with_init();

    // 先走完 START 链：SCHED 的 slot 11 现在持有 INIT 的行
    //（priority 7 / slice 200）——INHERIT 的父槽探针读的正是它。
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        assert_eq!(sched_init(&mut table, &mut ctl)[0].1, 0);
    }
    let schedctls = kernel.schedctl_calls.borrow();
    assert_eq!(schedctls.len(), 1, "前链恰好一次注册");
    drop(schedctls);

    // fork：子进程落 slot 12，scheduler/nice 从父槽拷贝（main.c:371-373
    // 的调用点状态），随后 sched_start_user 发 INHERIT。
    add_forked_child(&mut table);
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        sched_start_user(
            &mut table,
            child_endpoint(),
            UserSlot::new(CHILD_SLOT),
            &mut ctl,
        )
        .expect("INHERIT 应成功");
    }

    // PM 侧：子的调度器保持 SCHED（C sched_start.c:32-36 读回语义在
    // 本线等值：SCHED 不转发，回执 scheduler = 接收方自己）。
    assert_eq!(table.procs[CHILD_SLOT].resources.scheduler, Endpoint::SCHED);

    // 内核 wire：第二次下发携带子槽继承值——优先级/时间片来自父槽现值
    //（C schedule.c:199-211），天花板来自消息的 nice→queue。
    let schedules = kernel.schedule_calls.borrow();
    assert_eq!(schedules.len(), 2, "START + INHERIT 两次下发");
    assert_eq!(schedules[1].endpoint, child_endpoint().0);
    assert_eq!(schedules[1].priority, 7, "子继承父槽当前优先级");
    assert_eq!(schedules[1].quantum, USER_QUANTUM, "子继承父槽时间片");
    assert_eq!(schedules[1].niced, 0);
    drop(schedules);

    // 注册面：接管对象是子端点。
    let schedctls = kernel.schedctl_calls.borrow();
    assert_eq!(schedctls[1].target, child_endpoint());
}

// ---------------------------------------------------------------------------
// E5(e).3 NO_QUANTUM 回环：内核通知 → 降级 → 原地回写；伪造封印 → EPERM
// ---------------------------------------------------------------------------

#[test]
fn no_quantum_loop_kernel_notify_demotes_and_writes_back() {
    let bus = Bus::default();
    let sched_side = SchedSide { bus: bus.clone() };
    let (mut server, mut kernel) = server_with_kernel();
    let mut table = table_with_init();

    // 前链：INIT 已在 SCHED 的队列 7 / 200ms。
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        assert_eq!(sched_init(&mut table, &mut ctl)[0].1, 0);
    }

    // 内核量子耗尽通知到达（真实发送半 = notify_scheduler，
    // proc_table.rs:937-1013；宿主以同线形构造到达）。
    bus.deliver_from_kernel(no_quantum_notify(init_endpoint()));
    assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);

    // 回写面：LOCAL 扇出——降一级 7→8、时间片随行、CPU KEEP（C
    // schedule.c:99-105 + schedule_process_local :32-33），且对内核
    // 永不回复（C main.c:73-77）。
    let schedules = kernel.schedule_calls.borrow();
    assert_eq!(schedules.len(), 2, "START + 降级回写");
    assert_eq!(schedules[1].endpoint, init_endpoint().0);
    assert_eq!(schedules[1].priority, 8, "降一级");
    assert_eq!(schedules[1].quantum, USER_QUANTUM, "LOCAL 携带时间片");
    assert_eq!(schedules[1].cpu, -1, "KEEP：原地扇出不迁移");
    drop(schedules);
    assert!(
        bus.0.to_pm.borrow().is_empty(),
        "内核通知永不回复（C main.c:73-77 continue）"
    );

    // 断环分界守卫：同一消息不带封印 = 伪造 → EPERM 回复（C main.c:
    // 78-83），槽位不再降级（无新下发）。
    let forged = no_quantum_notify(init_endpoint());
    bus.0
        .to_sched
        .borrow_mut()
        .push_back((forged, IpcStatus::from_call(CALL_RECEIVE)));
    assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
    let sent = bus.0.sched_sent.borrow();
    assert_eq!(
        sent.len(),
        2,
        "前链 START 回执 + 伪造回复各一条（走普通回复面）"
    );
    assert_eq!(sent[1].1.m_type, EPERM);
    drop(sent);
    assert_eq!(
        kernel.schedule_calls.borrow().len(),
        2,
        "伪造通知不触发任何下发"
    );
}

// ---------------------------------------------------------------------------
// E5(e).4 拒绝透传：SCHED 占用门拒绝（EDEADEPT）经 rv 可见于 PM
// ---------------------------------------------------------------------------

#[test]
fn sched_refusal_surfaces_to_pm_as_nonzero_rv() {
    let bus = Bus::default();
    let sched_side = SchedSide { bus: bus.clone() };
    let (mut server, mut kernel) = server_with_kernel();
    let mut table = table_with_init();

    // 前链：START 成功，SCHED 的 slot 11 已被 INIT 占用。
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        assert_eq!(sched_init(&mut table, &mut ctl)[0].1, 0);
    }

    // 二次 sched_init：同一 INIT 再发 START → 撞出生门（C sched_isemtyendpt
    // → EDEADEPT，utility.c:46-56）。C taskcall.c:17-20 把拒绝码作为 rv
    // 交给 PM——若 taskcall 吞掉回复 m_type，这里会读成 0（成功），
    // schedule.c:44-47 的拒绝告警分支永远死掉。
    {
        let mut pump = || {
            assert_eq!(server.run_once(&sched_side, &mut kernel), Step::Handled);
        };
        let mut pm_side = PmSide {
            bus: bus.clone(),
            pump: RefCell::new(&mut pump),
        };
        let mut ctl = MinixSchedCtl::new(&mut pm_side);
        let results = sched_init(&mut table, &mut ctl);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].1, EDEADEPT,
            "拒绝码必须经 rv 透传（taskcall.c:19-20）"
        );
    }
    assert_eq!(
        table.procs[INIT_SLOT].resources.scheduler,
        Endpoint::SCHED,
        "拒绝分支只告警，不改写已成立的调度器（schedule.c:44-47）"
    );

    // 拒绝链不产生任何新内核调用（出生门在 schedctl 之前，C :154-157）。
    assert_eq!(kernel.schedctl_calls.borrow().len(), 1);
    assert_eq!(kernel.schedule_calls.borrow().len(), 1);
}
