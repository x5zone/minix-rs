//! minix3/minix/tests Rust 腿翻译 —— 信号域(test5/37/41/52/68)。
//!
//! # 翻译映射
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test37(sigaction 装置与读回) | 本文件 [`sigaction_installs_reads_back_and_rejects_invalid`] |
//! | test37/52(信号被掩码后停于 pending) | 本文件 [`masked_signal_parks_in_pending_until_unblocked`] |
//! | test5(用户身份与 kill 权限) | 本文件 [`kill_permission_eperm_esrch_and_probe`] |
//! | test41(alarm 置位/撤销/到期) | 本文件 [`alarm_set_cancel_and_expiry_paths`] |
//! | test38/68(信号与文件操作、exec 交错) | 挂起路径的续接面,待 VFS 挂起续接真链点亮后补;kill 终止链已由 `servers/pm/tests/run_once_integration.rs::kill_termination_tells_vfs_exit` 覆盖 |
//!
//! C 测试的断言锚点逐条给出(`minix3/minix/servers/pm/signal.c` 与
//! `alarm.c` 的函数锚),errno 断言一律对照 C 值(EINVAL/ESRCH/EPERM)。
//! 交付门 = 编译;全部测试 `#[ignore]`,点亮前提见各测试属性。

use minix_pm::TestIpcTransport;
use minix_pm::exit::KernelGateway;
use minix_pm::mproc::{Lifecycle, ProcTable};
use minix_pm::mproc::{SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK, SigAction};
use minix_pm::signal::do_kill;
use minix_pm::signal_handlers::{
    SigActionReq, handle_sigaction, handle_sigpending, handle_sigprocmask,
};
use minix_pm::timer::{SIGALRM, TimerCtl, cause_sigalrm, set_alarm};
use minix_types::{Endpoint, VirBytes};

// ---------------------------------------------------------------------------
// 夹具
// ---------------------------------------------------------------------------

const SIG_IGN: usize = 1; // C signal.h:SIG_IGN(处理映射 mproc/signal.rs:125-127)
const SIGKILL: i32 = 9;
const SIGTERM: i32 = 15;
const SIGUSR1: i32 = 16;

/// C `__sigmask`(Rust 侧 `init::sig_bit` 为 crate 私有,此处按同式计算)。
fn sig_bit(sig: i32) -> u64 {
    1u64 << (sig - 1)
}

fn ep_of(slot: usize) -> Endpoint {
    Endpoint::from_generation_slot(1, slot as i32)
}

/// 播种一个 Running 进程并给全同凭证(uid = 参数值)。
fn seed(table: &mut ProcTable, slot: usize, pid: i32, uid: u32) {
    let p = &mut table.procs[slot];
    p.identity.endpoint = ep_of(slot);
    p.identity.id.pid = pid;
    p.state.lifecycle = Lifecycle::Running;
    let cred = p.resources.privilege.credentials_mut();
    cred.user.real = uid;
    cred.user.effective = uid;
}

/// 槽 0 = PM 自身(uid 0 超级用户):alarm 递送与 kill 权限判定都以它为基准。
fn pm_table() -> ProcTable {
    let mut table = ProcTable::new();
    seed(&mut table, 0, 0, 0);
    table
}

/// 无操作内核网关(与 pm_vm_fork.rs 同型:夹具链不触内核面)。
struct NoopKernelGateway;

impl KernelGateway for NoopKernelGateway {
    fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> {
        Ok(())
    }
    fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> {
        Ok(())
    }
    fn sys_abort(&mut self, _how: i32) -> Result<(), i32> {
        Ok(())
    }
    fn proc_times(
        &mut self,
        _ep: Endpoint,
    ) -> Result<(minix_types::Clock, minix_types::Clock), i32> {
        Ok((0, 0))
    }
    fn copy_to_user(
        &mut self,
        _bytes: &[u8],
        _dst_ep: Endpoint,
        _dst_addr: u64,
    ) -> Result<(), i32> {
        Ok(())
    }
    fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> {
        Ok(())
    }
    fn sys_delay_stop(&mut self, _ep: Endpoint) -> Result<(), i32> {
        Ok(())
    }
    fn sys_trace(
        &mut self,
        _req: i32,
        _ep: Endpoint,
        _addr: u64,
        _data: &mut i64,
    ) -> Result<(), i32> {
        Ok(())
    }
    fn sys_vircopy(
        &mut self,
        _src_ep: Endpoint,
        _src: u64,
        _dst_ep: Endpoint,
        _dst: u64,
        _len: u64,
    ) -> Result<(), i32> {
        Ok(())
    }
    fn copy_from_user(
        &mut self,
        _src_ep: Endpoint,
        _src: u64,
        _bytes: &mut [u8],
    ) -> Result<(), i32> {
        Ok(())
    }
    fn get_ksig(&mut self) -> Result<Option<(Endpoint, u64)>, i32> {
        Ok(None)
    }
    fn end_ksig(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> {
        Ok(())
    }
    fn sys_sigsend(
        &mut self,
        _ep: Endpoint,
        _sigmsg: &minix_sys::syscall::SigMsgWire,
    ) -> Result<(), i32> {
        Ok(())
    }
    fn sys_sigreturn(&mut self, _ep: Endpoint, _ctx: VirBytes) -> Result<(), i32> {
        Ok(())
    }
    fn diag_write(&mut self, _text: &str) -> Result<(), i32> {
        Ok(())
    }
    fn sys_diagctl_stacktrace(&mut self, _ep: Endpoint) -> Result<(), i32> {
        Ok(())
    }
}

/// 脚本化内核定时器(TimerCtl,C alarm.c 的 set_timer/cancel_timer 缝):
/// 记录 set/cancel 调用,时钟手动推进。
struct ScriptedTimerCtl {
    clock: minix_types::Clock,
    sets: Vec<(Endpoint, minix_types::Clock)>,
    cancels: Vec<Endpoint>,
}

impl ScriptedTimerCtl {
    fn new(clock: minix_types::Clock) -> Self {
        Self {
            clock,
            sets: Vec::new(),
            cancels: Vec::new(),
        }
    }
}

impl TimerCtl for ScriptedTimerCtl {
    fn set(&mut self, ep: Endpoint, ticks: minix_types::Clock) {
        self.sets.push((ep, ticks));
    }
    fn cancel(&mut self, ep: Endpoint) {
        self.cancels.push(ep);
    }
    fn exptime(&self, _ep: Endpoint) -> Option<minix_types::Clock> {
        None
    }
    fn now(&self) -> minix_types::Clock {
        self.clock
    }
}

// ---------------------------------------------------------------------------
// test37 —— sigaction 装置、读回与拒绝
// ---------------------------------------------------------------------------

/// C 语义(`minix3/minix/tests/test37.c` 的 sigaction 用法;处置表见
/// `servers/pm/signal.c`):装新处置并读回旧处置;SIGKILL 不可装置(早退,
/// 不动 oact);无效信号号返回 EINVAL;act 为空时纯读回。
#[test]
#[ignore = "点亮前提:PM 信号处置面随载体用户态点亮后复核"]
fn sigaction_installs_reads_back_and_rejects_invalid() {
    let mut table = pm_table();
    let caller = minix_types::UserSlot::new(1);
    seed(&mut table, 1, 100, 200);

    // 安装捕获处置:handler 0x3000,执行期屏蔽 SIGUSR1。
    let installed = SigAction {
        sa_handler: 0x3000,
        sa_mask: sig_bit(SIGUSR1),
        sa_flags: 0,
    };
    let old = handle_sigaction(
        &mut table,
        caller,
        SigActionReq {
            signo: SIGALRM,
            act: Some(installed),
            need_oact: true,
            sigreturn: VirBytes::new(0x9000),
        },
    )
    .expect("SIGALRM 可装置处置")
    .expect("oact 请求必回填当前处置(C sigaction oact 语义)");
    assert_eq!(old.sa_handler, 0, "初始处置为 SIG_DFL(0)");

    // 纯读回(act == 0):返回刚装上的处置。
    let readback = handle_sigaction(
        &mut table,
        caller,
        SigActionReq {
            signo: SIGALRM,
            act: None,
            need_oact: true,
            sigreturn: VirBytes::new(0),
        },
    )
    .expect("读回不改变处置")
    .expect("已装处置可读回");
    assert_eq!(readback.sa_handler, 0x3000, "handler 读回一致");
    assert_eq!(readback.sa_mask, sig_bit(SIGUSR1), "执行期掩码读回一致");

    // SIGKILL 不可装置(处置表 D1:早退返回 None,旧处置也不给)。
    let kill_req = handle_sigaction(
        &mut table,
        caller,
        SigActionReq {
            signo: SIGKILL,
            act: Some(installed),
            need_oact: true,
            sigreturn: VirBytes::new(0),
        },
    )
    .expect("SIGKILL 路径不报错(C 早退语义)");
    assert!(kill_req.is_none(), "SIGKILL 无 oact 产物");

    // 无效信号号:EINVAL(C sigaction 对 0 与越界号返回 EINVAL)。
    for bad in [0, 64] {
        let err = handle_sigaction(
            &mut table,
            caller,
            SigActionReq {
                signo: bad,
                act: Some(installed),
                need_oact: false,
                sigreturn: VirBytes::new(0),
            },
        )
        .expect_err("无效信号号必须拒绝");
        assert_eq!(err.to_errno(), minix_types::EINVAL, "signo {bad} → EINVAL");
    }
}

// ---------------------------------------------------------------------------
// test37/52 —— 掩码信号停于 pending,解除后可见
// ---------------------------------------------------------------------------

/// C 语义(signal.c:486-490 掩码臂):目标把 SIGUSR1 屏蔽后收到该信号,
/// 信号不投递,置入 pending 位图,进程不受扰;sigpending 快照可见;
/// 掩码解除时返回旧掩码(C sigprocmask 的 oact 语义)。
#[test]
#[ignore = "点亮前提:PM 信号面随载体用户态点亮后复核"]
fn masked_signal_parks_in_pending_until_unblocked() {
    let mut table = pm_table();
    let caller = minix_types::UserSlot::new(0); // PM 超级用户:kill 权限恒过
    let target = minix_types::UserSlot::new(1);
    seed(&mut table, 1, 100, 200);

    // 目标屏蔽 SIGUSR1。
    let (old_mask, _effect) = handle_sigprocmask(&mut table, target, SIG_BLOCK, sig_bit(SIGUSR1))
        .expect("SIG_BLOCK 合法");
    assert_eq!(old_mask, 0, "屏蔽前掩码为空");

    // kill 送达:掩码臂把信号停进 pending,目标不受扰。
    let sent = do_kill(
        &mut table,
        caller,
        100,
        SIGUSR1,
        &mut NoopKernelGateway,
        &mut TestIpcTransport::new(),
    )
    .expect("有权限且目标存在");
    assert_eq!(sent, 1, "命中一个目标");

    let parked = handle_sigpending(&table, target);
    assert_eq!(
        parked,
        sig_bit(SIGUSR1),
        "信号停于 pending(C signal.c:88-97 快照)"
    );

    let state = &table.procs[1];
    assert!(
        matches!(state.state.lifecycle, Lifecycle::Running),
        "掩码信号不改变目标生命周期"
    );
    assert_eq!(state.resources.signals.pending, sig_bit(SIGUSR1));

    // 解除屏蔽:返回旧掩码(C sigprocmask oact),SIG_SETMASK 清空。
    let (prev, _effect) =
        handle_sigprocmask(&mut table, target, SIG_SETMASK, 0).expect("SIG_SETMASK 合法");
    assert_eq!(prev, sig_bit(SIGUSR1), "旧掩码含被屏蔽位");
    assert_eq!(
        handle_sigprocmask(&mut table, target, SIG_UNBLOCK, 0)
            .err()
            .map(|e| e.to_errno()),
        None,
        "SIG_UNBLOCK 合法"
    );
}

// ---------------------------------------------------------------------------
// test5 —— kill 权限、存在性与存在探测
// ---------------------------------------------------------------------------

/// C 语义(signal.c:616-628 权限与探测):不同用户 kill → EPERM(C 622-628
/// 的 real/eff 匹配规则);同用户 → 命中;signo == 0 是存在探测(不投递);
/// 未知 pid → ESRCH;负信号号 → EINVAL。
#[test]
#[ignore = "点亮前提:PM 信号面随载体用户态点亮后复核"]
fn kill_permission_eperm_esrch_and_probe() {
    let mut table = pm_table();
    let caller = minix_types::UserSlot::new(1);
    seed(&mut table, 1, 100, 100); // uid 100
    seed(&mut table, 2, 200, 200); // uid 200:与 caller 不同用户
    seed(&mut table, 3, 300, 100); // uid 100:同用户
    let mut kern = NoopKernelGateway;
    let mut transport = TestIpcTransport::new();

    // 跨用户:EPERM(C signal.c:622-628 无一匹配)。
    let err = do_kill(&mut table, caller, 200, SIGTERM, &mut kern, &mut transport)
        .expect_err("不同用户必须拒绝");
    assert_eq!(err.to_errno(), minix_types::EPERM, "跨用户 kill → EPERM");

    // 同用户:命中一个目标。
    let hits =
        do_kill(&mut table, caller, 300, SIGTERM, &mut kern, &mut transport).expect("同用户可发");
    assert_eq!(hits, 1);

    // 存在探测(signo == 0):命中但不投递(C signal.c:630-631 探测分支),
    // 目标存活、无 VFS 告知。用未收到过终止信号的新目标与全新通道,
    // 免得前一段终止链的出站消息混入对账。
    seed(&mut table, 4, 400, 100);
    let mut probe_transport = TestIpcTransport::new();
    let probe =
        do_kill(&mut table, caller, 400, 0, &mut kern, &mut probe_transport).expect("探测合法");
    assert_eq!(probe, 1);
    assert!(
        matches!(table.procs[4].state.lifecycle, Lifecycle::Running),
        "探测不改变目标生命周期"
    );
    assert!(
        probe_transport.sent().is_empty(),
        "探测不产生出站消息(不投递语义)"
    );

    // 未知 pid:ESRCH(C check_sig 尾段 error_code 缺省 ESRCH)。
    let err = do_kill(&mut table, caller, 999, SIGTERM, &mut kern, &mut transport)
        .expect_err("未知 pid 必须拒绝");
    assert_eq!(err.to_errno(), minix_types::ESRCH);

    // 负信号号:EINVAL(check_sig 首门)。
    let err = do_kill(&mut table, caller, 300, -1, &mut kern, &mut transport)
        .expect_err("负信号号必须拒绝");
    assert_eq!(err.to_errno(), minix_types::EINVAL);
}

// ---------------------------------------------------------------------------
// test41 —— alarm 置位、撤销与到期
// ---------------------------------------------------------------------------

/// C 语义(`minix3/minix/tests/test41.c`;机制 `servers/pm/alarm.c`):
/// alarm(n) 挂内核定时器并标记 ALARM_ON(set_alarm,alarm.c:299-311);
/// alarm(0) 撤销;到期回调投递 SIGALRM(alarm.c:317-344)——已装 SIG_IGN
/// 则吞掉(signal.c:486-488 忽略臂),默认处置则终止(经 VFS_PM_EXIT 告知);
/// 无警报时的到期回调是空操作。
#[test]
#[ignore = "点亮前提:PM 定时器与信号终止链随载体点亮后复核"]
fn alarm_set_cancel_and_expiry_paths() {
    let mut table = pm_table();
    seed(&mut table, 1, 100, 200);
    let target = minix_types::UserSlot::new(1);
    let target_ep = ep_of(1);
    let mut kern = NoopKernelGateway;

    // 置位:内核定时器收到 (ep, 10),进程标记 ALARM_ON,到期时间 = now + 10。
    let mut tctl = ScriptedTimerCtl::new(1000);
    set_alarm(&mut table, target, 10, &mut tctl);
    assert_eq!(
        tctl.sets,
        vec![(target_ep, 10)],
        "alarm(10) 挂 10 tick 定时器"
    );
    let timer = table.procs[1].resources.timer.as_ref().expect("ALARM_ON");
    assert_eq!(timer.expire_time, 1010, "到期时间 = now + ticks");

    // 撤销:alarm(0) 取消内核定时器并清标记(alarm.c:305-308)。
    set_alarm(&mut table, target, 0, &mut tctl);
    assert_eq!(tctl.cancels, vec![target_ep], "alarm(0) 取消定时器");
    assert!(table.procs[1].resources.timer.is_none(), "标记清除");

    // 无警报时的到期回调:guard 拦截,不投递(alarm.c:324-330)。
    let mut transport = TestIpcTransport::new();
    let delivered = cause_sigalrm(&mut table, target_ep, &mut tctl, &mut kern, &mut transport);
    assert!(!delivered, "无 ALARM_ON 标记 → 不触发投递");

    // SIG_IGN:重挂警报后到期,信号被忽略臂吞掉,目标存活。
    set_alarm(&mut table, target, 10, &mut tctl);
    handle_sigaction(
        &mut table,
        target,
        SigActionReq {
            signo: SIGALRM,
            act: Some(SigAction {
                sa_handler: SIG_IGN,
                sa_mask: 0,
                sa_flags: 0,
            }),
            need_oact: false,
            sigreturn: VirBytes::new(0),
        },
    )
    .expect("SIGALRM 可设 SIG_IGN");
    let mut transport_ign = TestIpcTransport::new();
    let delivered = cause_sigalrm(
        &mut table,
        target_ep,
        &mut tctl,
        &mut kern,
        &mut transport_ign,
    );
    assert!(delivered, "ALARM_ON → 投递流程启动");
    assert!(
        matches!(table.procs[1].state.lifecycle, Lifecycle::Running),
        "SIG_IGN 吞掉 SIGALRM,目标存活"
    );

    // 默认处置:重挂警报后到期,SIGALRM 终止目标并告知 VFS(C
    // forkexit.c:350-358 的无条件 tell_vfs;终止链告知见 run_once_integration
    // ::kill_termination_tells_vfs_exit 判例)。
    let target2_slot = minix_types::UserSlot::new(2);
    seed(&mut table, 2, 101, 200);
    set_alarm(&mut table, target2_slot, 5, &mut tctl);
    let mut transport_term = TestIpcTransport::new();
    let delivered = cause_sigalrm(
        &mut table,
        ep_of(2),
        &mut tctl,
        &mut kern,
        &mut transport_term,
    );
    assert!(delivered, "ALARM_ON → 投递流程启动");
    let lifecycle = &table.procs[2].state.lifecycle;
    assert!(
        lifecycle.is_zombie() || lifecycle.is_exiting(),
        "默认处置的 SIGALRM 终止目标(僵尸化或退出中)"
    );
    assert!(
        transport_term
            .sent()
            .iter()
            .any(|(ep, m)| *ep == Endpoint::VFS && m.m_type == minix_types::VFS_PM_EXIT),
        "终止链告知 VFS_PM_EXIT"
    );
    let _ = target2_slot;
}
