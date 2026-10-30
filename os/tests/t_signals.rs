//! Rust-leg translation of minix3/minix/tests — the signal domain (test5/37/41/68).
//!
//! # Translation map
//!
//! | C test | Semantic home |
//! |---|---|
//! | test37 (sigaction install and read-back) | [`sigaction_installs_reads_back_and_rejects_invalid`] in this file |
//! | test37b (sigprocmask/sigpending: a masked signal parks in pending; the delivery carrier for unblocking is test37i) | [`masked_signal_parks_in_pending_until_unblocked`] in this file |
//! | test5 (same-uid parent/child mutual kill, exit status, EINTR face) | [`kill_permission_eperm_esrch_and_probe`] in this file (cross-user EPERM/ESRCH/probe has no dedicated carrier in the numbered suite; semantics taken from the implementation at `signal.c:621-632`) |
//! | test41 (alarm and itimer interaction, expiry delivery; cancel and SIG_IGN are alarm.c implementation semantics) | [`alarm_set_cancel_and_expiry_paths`] in this file |
//! | test38/68 (signals interleaved with file operations and exec) | the continuation face of the suspend path, to be added once the VFS suspend-continue real chain is lit; the kill-termination chain is already covered by `servers/pm/tests/run_once_integration.rs::kill_termination_tells_vfs_exit` |
//!
//! Each C test's assertion anchor is given (`minix3/minix/servers/pm/signal.c` and the
//! function anchors of `alarm.c`); errno assertions always use the C values
//! (EINVAL/ESRCH/EPERM). Delivery gate = compilation; every test is `#[ignore]`, and the
//! lighting prerequisites are in each test's attribute.

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
// Fixtures
// ---------------------------------------------------------------------------

const SIG_IGN: usize = 1; // C signal.h:SIG_IGN (handler mapping mproc/signal.rs:125-127)
const SIGKILL: i32 = 9;
const SIGTERM: i32 = 15;
const SIGUSR1: i32 = 30; // C signal.h:82 (SIGUSR1 = 30; 16 is SIGURG in C numbering)

/// C `__sigmask` (the Rust side's `init::sig_bit` is crate-private, so the same formula is used here).
fn sig_bit(sig: i32) -> u64 {
    1u64 << (sig - 1)
}

fn ep_of(slot: usize) -> Endpoint {
    Endpoint::from_generation_slot(1, slot as i32)
}

/// Seed one Running process with uniform credentials (uid = the parameter value).
fn seed(table: &mut ProcTable, slot: usize, pid: i32, uid: u32) {
    let p = &mut table.procs[slot];
    p.identity.endpoint = ep_of(slot);
    p.identity.id.pid = pid;
    p.state.lifecycle = Lifecycle::Running;
    let cred = p.resources.privilege.credentials_mut();
    cred.user.real = uid;
    cred.user.effective = uid;
}

/// Slot 0 = PM itself (uid 0, superuser): alarm delivery and kill permission checks both use it as the baseline.
fn pm_table() -> ProcTable {
    let mut table = ProcTable::new();
    seed(&mut table, 0, 0, 0);
    table
}

/// No-op kernel gateway (same shape as pm_vm_fork.rs: the fixture chain does not touch the kernel face).
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
    fn get_ksig(&mut self) -> Result<Option<(Endpoint, minix_types::SigSetBits)>, i32> {
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

/// Scripted kernel timer (TimerCtl, C alarm.c's set_timer/cancel_timer seam): records
/// set/cancel calls; the clock is advanced by hand.
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
// test37 — sigaction install, read-back and rejection
// ---------------------------------------------------------------------------

/// C semantics (the install/read-back section of test37b in `minix3/minix/tests/test37.c`;
/// the disposition table is in `servers/pm/signal.c`): install a new disposition and read
/// back the old one; SIGKILL cannot be installed (early exit, oact untouched); an invalid
/// signal number returns EINVAL; a null act is a pure read-back.
#[test]
#[ignore = "点亮前提:PM 信号处置面随载体用户态点亮后复核"]
fn sigaction_installs_reads_back_and_rejects_invalid() {
    let mut table = pm_table();
    let caller = minix_types::UserSlot::new(1);
    seed(&mut table, 1, 100, 200);

    // Install a catching disposition: handler 0x3000, mask SIGUSR1 during execution.
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

    // Pure read-back (act == 0): returns the disposition just installed.
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

    // SIGKILL cannot be installed (the disposition table's early-exit rule: returns None and gives no old disposition either).
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

    // Invalid signal number: EINVAL (C sigaction returns EINVAL for 0 and out-of-range numbers).
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
// test37b — a masked signal parks in pending and is visible after unblocking
// (the body of C test52 is a pipe plus SIGCHLD rotation and does not contribute this case)
// ---------------------------------------------------------------------------

/// C semantics (the mask arm of signal.c:491-497): after the target masks SIGUSR1 and
/// receives that signal, it is not delivered, its pending bit is set, and the process is
/// undisturbed; the sigpending snapshot shows it; unblocking returns the old mask (C
/// sigprocmask's oact semantics).
#[test]
#[ignore = "点亮前提:PM 信号面随载体用户态点亮后复核"]
fn masked_signal_parks_in_pending_until_unblocked() {
    let mut table = pm_table();
    let caller = minix_types::UserSlot::new(0); // PM superuser: kill permission always passes
    let target = minix_types::UserSlot::new(1);
    seed(&mut table, 1, 100, 200);

    // The target masks SIGUSR1.
    let (old_mask, _effect) = handle_sigprocmask(&mut table, target, SIG_BLOCK, sig_bit(SIGUSR1))
        .expect("SIG_BLOCK 合法");
    assert_eq!(old_mask, 0, "屏蔽前掩码为空");

    // kill delivery: the mask arm parks the signal in pending and the target is undisturbed.
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

    // Unblock: returns the old mask (C sigprocmask oact), and SIG_SETMASK clears it.
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
// test5 — kill permission, existence, and the existence probe
// ---------------------------------------------------------------------------

/// C semantics (signal.c permissions and probe, 616-632): a kill from a different user →
/// EPERM (the real/eff matching rule at C 622-626); same user → hit; signo == 0 is an
/// existence probe (no delivery); unknown pid → ESRCH; negative signal number → EINVAL.
#[test]
#[ignore = "点亮前提:PM 信号面随载体用户态点亮后复核"]
fn kill_permission_eperm_esrch_and_probe() {
    let mut table = pm_table();
    let caller = minix_types::UserSlot::new(1);
    seed(&mut table, 1, 100, 100); // uid 100
    seed(&mut table, 2, 200, 200); // uid 200: a different user from the caller
    seed(&mut table, 3, 300, 100); // uid 100: same user
    let mut kern = NoopKernelGateway;
    let mut transport = TestIpcTransport::new();

    // Cross-user: EPERM (no rule at C signal.c:622-626 matches; the numbered suite has no cross-user carrier, so implementation semantics are used).
    let err = do_kill(&mut table, caller, 200, SIGTERM, &mut kern, &mut transport)
        .expect_err("不同用户必须拒绝");
    assert_eq!(err.to_errno(), minix_types::EPERM, "跨用户 kill → EPERM");

    // Same user: one target hit.
    let hits =
        do_kill(&mut table, caller, 300, SIGTERM, &mut kern, &mut transport).expect("同用户可发");
    assert_eq!(hits, 1);

    // Existence probe (signo == 0): hit without delivery (the probe branch at C
    // signal.c:631-632), the target stays alive and VFS is not told. Use a fresh target
    // that never received a termination signal and a brand-new channel, so outbound
    // messages from the previous termination chain do not mix into the accounting.
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

    // Unknown pid: ESRCH (the default error_code of check_sig's tail is ESRCH).
    let err = do_kill(&mut table, caller, 999, SIGTERM, &mut kern, &mut transport)
        .expect_err("未知 pid 必须拒绝");
    assert_eq!(err.to_errno(), minix_types::ESRCH);

    // Negative signal number: EINVAL (the first gate of check_sig).
    let err = do_kill(&mut table, caller, 300, -1, &mut kern, &mut transport)
        .expect_err("负信号号必须拒绝");
    assert_eq!(err.to_errno(), minix_types::EINVAL);
}

// ---------------------------------------------------------------------------
// test41 — alarm set, cancel and expiry
// ---------------------------------------------------------------------------

/// C semantics (the test_alarm section of `minix3/minix/tests/test41.c`, 263-288; the
/// mechanism is in `servers/pm/alarm.c`): alarm(n) arms a kernel timer and marks ALARM_ON
/// (set_alarm, alarm.c:299-311); alarm(0) cancels (implementation semantics; test41's
/// cancel goes through setitimer with a zero value); the expiry callback delivers SIGALRM
/// (alarm.c:317-344, and test41 asserts exactly one SIGALRM on expiry) — an installed
/// SIG_IGN swallows it (the ignore arm of signal.c:487-490), while the default disposition
/// terminates (announced through VFS_PM_EXIT); an expiry callback with no alarm is a no-op.
#[test]
#[ignore = "点亮前提:PM 定时器与信号终止链随载体点亮后复核"]
fn alarm_set_cancel_and_expiry_paths() {
    let mut table = pm_table();
    seed(&mut table, 1, 100, 200);
    let target = minix_types::UserSlot::new(1);
    let target_ep = ep_of(1);
    let mut kern = NoopKernelGateway;

    // Set: the kernel timer receives (ep, 10), the process is marked ALARM_ON, and the expiry time = now + 10.
    let mut tctl = ScriptedTimerCtl::new(1000);
    set_alarm(&mut table, target, 10, &mut tctl);
    assert_eq!(
        tctl.sets,
        vec![(target_ep, 10)],
        "alarm(10) 挂 10 tick 定时器"
    );
    let timer = table.procs[1].resources.timer.as_ref().expect("ALARM_ON");
    assert_eq!(timer.expire_time, 1010, "到期时间 = now + ticks");

    // Cancel: alarm(0) cancels the kernel timer and clears the mark (alarm.c:305-308).
    set_alarm(&mut table, target, 0, &mut tctl);
    assert_eq!(tctl.cancels, vec![target_ep], "alarm(0) 取消定时器");
    assert!(table.procs[1].resources.timer.is_none(), "标记清除");

    // Expiry callback with no alarm: the guard intercepts it and nothing is delivered (alarm.c:330-331).
    let mut transport = TestIpcTransport::new();
    let delivered = cause_sigalrm(&mut table, target_ep, &mut tctl, &mut kern, &mut transport);
    assert!(!delivered, "无 ALARM_ON 标记 → 不触发投递");

    // SIG_IGN: arm the alarm again, it expires, the ignore arm swallows the signal, and the target survives.
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

    // Default disposition: arm the alarm again, it expires, SIGALRM terminates the target
    // and tells VFS (C forkexit.c:347-359's unconditional tell_vfs; the termination-chain
    // announcement is covered by the run_once_integration::kill_termination_tells_vfs_exit
    // case).
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
