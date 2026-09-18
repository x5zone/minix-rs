//! PM 调用分发表（C: `table.c:call_vec` + `callnr.h` 47 个调用号）。
//!
//! 文档: `notes/rewrite/fork-syscall-rewrite/04-stage-pm/04-ipc-dispatch.md`。
//!
//! # 与 C 的对应
//!
//! - `callnr.h:14-60` 的 47 个 `PM_*` 调用号 → [`PmCall`] 枚举判别值
//!   （`#[repr(i32)]`，值域 1..=47，与 C `PM_BASE + N` 完全一致）。
//! - `table.c:23-59` 的 `call_vec[NR_PM_CALLS]` 函数指针表 →
//!   [`dispatch_pm_call`] 的 match 分发（ARCH A-5：C 表驱动指针 →
//!   Rust 编译期穷尽 match）。
//! - C 对 NULL/越界槽位返回 ENOSYS（main.c:94-101）→ Rust 对未注册
//!   调用号返回 ENOSYS；对已注册但 handler 未实现的调用同样返回
//!   ENOSYS 占位（DEFERRED，归属 07~20 各文档）。
//!
//! 调用号常量不单独散列（不重复 callnr.h 47 个 `pub const`）：枚举判别
//! 值即单一事实源；将来内核侧 libc 需要调用号时再上移 minix-types。

use crate::event::EventRegistry;
use crate::ipc::{IpcTransport, ReplyIntent};
use crate::mproc::ProcTable;
use crate::credentials::{
    do_get, do_set, CopyGroups, GetOp, GetResult, SetOp, SetError, VfsForwarder,
};
use minix_types::{ENOSYS, EINVAL, Endpoint, Gid, Message, PmError, ProcEventMask, UserSlot, VirBytes};

/// PM 系统调用枚举（C: `callnr.h:14-60`，`PM_BASE + 1` ~ `PM_BASE + 47`）。
///
/// 判别值 = 消息 `m_type` 中的调用号（`callnr.h:9`：`PM_BASE = 0x000`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(clippy::upper_case_acronyms)] // 与 C `PM_*` 宏名保持同形。
pub enum PmCall {
    /// PM_EXIT — _exit(2)
    Exit = 1,
    /// PM_FORK — fork(2)
    Fork = 2,
    /// PM_WAIT4 — wait4(2)
    Wait4 = 3,
    /// PM_GETPID — get[p]pid(2)
    GetPid = 4,
    /// PM_SETUID — setuid(2)
    SetUid = 5,
    /// PM_GETUID — get[e]uid(2)
    GetUid = 6,
    /// PM_STIME — stime(2)
    Stime = 7,
    /// PM_PTRACE — ptrace(2)
    Ptrace = 8,
    /// PM_SETGROUPS — setgroups(2)
    SetGroups = 9,
    /// PM_GETGROUPS — getgroups(2)
    GetGroups = 10,
    /// PM_KILL — kill(2)
    Kill = 11,
    /// PM_SETGID — setgid(2)
    SetGid = 12,
    /// PM_GETGID — get[e]gid(2)
    GetGid = 13,
    /// PM_EXEC — execve(2)
    Exec = 14,
    /// PM_SETSID — setsid(2)
    SetSid = 15,
    /// PM_GETPGRP — getpgrp(2)
    GetPgrp = 16,
    /// PM_ITIMER — [gs]etitimer(2)
    Itimer = 17,
    /// PM_GETMCONTEXT — getmcontext(2)
    GetMContext = 18,
    /// PM_SETMCONTEXT — setmcontext(2)
    SetMContext = 19,
    /// PM_SIGACTION — sigaction(2)
    SigAction = 20,
    /// PM_SIGSUSPEND — sigsuspend(2)
    SigSuspend = 21,
    /// PM_SIGPENDING — sigpending(2)
    SigPending = 22,
    /// PM_SIGPROCMASK — sigprocmask(2)
    SigProcMask = 23,
    /// PM_SIGRETURN — sigreturn(2)
    SigReturn = 24,
    /// PM_SYSUNAME — sysuname(2)（obsolete）
    SysUname = 25,
    /// PM_GETPRIORITY — getpriority(2)
    GetPriority = 26,
    /// PM_SETPRIORITY — setpriority(2)
    SetPriority = 27,
    /// PM_GETTIMEOFDAY — gettimeofday(2)
    GetTimeOfDay = 28,
    /// PM_SETEUID — seteuid(2)
    SetEUid = 29,
    /// PM_SETEGID — setegid(2)
    SetEGid = 30,
    /// PM_ISSETUGID — issetugid(2)
    IsSetUid = 31,
    /// PM_GETSID — getsid(2)
    GetSid = 32,
    /// PM_CLOCK_GETRES — clock_getres(2)
    ClockGetRes = 33,
    /// PM_CLOCK_GETTIME — clock_gettime(2)
    ClockGetTime = 34,
    /// PM_CLOCK_SETTIME — clock_settime(2)
    ClockSetTime = 35,
    /// PM_GETRUSAGE — getrusage(2)
    GetRUsage = 36,
    /// PM_REBOOT — reboot(2)
    Reboot = 37,
    /// PM_SVRCTL — svrctl(2)
    SvrCtl = 38,
    /// PM_SPROF — sprofile(2)
    SProf = 39,
    /// PM_PROCEVENTMASK — proceventmask(2)
    ProcEventMask = 40,
    /// PM_SRV_FORK — srv_fork(2)
    SrvFork = 41,
    /// PM_SRV_KILL — srv_kill(2)
    SrvKill = 42,
    /// PM_EXEC_NEW — exec 新路径（newexec）
    ExecNew = 43,
    /// PM_EXEC_RESTART — exec 重启路径
    ExecRestart = 44,
    /// PM_GETEPINFO — getepinfo(2)
    GetEpInfo = 45,
    /// PM_GETPROCNR — getprocnr(2)
    GetProcNr = 46,
    /// PM_GETSYSINFO — getsysinfo(2)
    GetSysInfo = 47,
}

impl PmCall {
    /// 从消息 `m_type` 解码为已注册的 PM 调用。
    ///
    /// 返回 `None` 当 `nr` 不在 1..=47（含 `nr == 0` 保留值与未注册号）；
    /// 调用方（分发层）对 `None` 返回 ENOSYS，与 C `call_index >=
    /// NR_PM_CALLS || call_vec[call_index] == NULL → ENOSYS`
    /// （main.c:94-101）一致。
    pub fn from_call_nr(nr: i32) -> Option<PmCall> {
        // nr 在 1..=47 内按判别值匹配；判别值即调用号。
        match nr {
            1 => Some(PmCall::Exit),
            2 => Some(PmCall::Fork),
            3 => Some(PmCall::Wait4),
            4 => Some(PmCall::GetPid),
            5 => Some(PmCall::SetUid),
            6 => Some(PmCall::GetUid),
            7 => Some(PmCall::Stime),
            8 => Some(PmCall::Ptrace),
            9 => Some(PmCall::SetGroups),
            10 => Some(PmCall::GetGroups),
            11 => Some(PmCall::Kill),
            12 => Some(PmCall::SetGid),
            13 => Some(PmCall::GetGid),
            14 => Some(PmCall::Exec),
            15 => Some(PmCall::SetSid),
            16 => Some(PmCall::GetPgrp),
            17 => Some(PmCall::Itimer),
            18 => Some(PmCall::GetMContext),
            19 => Some(PmCall::SetMContext),
            20 => Some(PmCall::SigAction),
            21 => Some(PmCall::SigSuspend),
            22 => Some(PmCall::SigPending),
            23 => Some(PmCall::SigProcMask),
            24 => Some(PmCall::SigReturn),
            25 => Some(PmCall::SysUname),
            26 => Some(PmCall::GetPriority),
            27 => Some(PmCall::SetPriority),
            28 => Some(PmCall::GetTimeOfDay),
            29 => Some(PmCall::SetEUid),
            30 => Some(PmCall::SetEGid),
            31 => Some(PmCall::IsSetUid),
            32 => Some(PmCall::GetSid),
            33 => Some(PmCall::ClockGetRes),
            34 => Some(PmCall::ClockGetTime),
            35 => Some(PmCall::ClockSetTime),
            36 => Some(PmCall::GetRUsage),
            37 => Some(PmCall::Reboot),
            38 => Some(PmCall::SvrCtl),
            39 => Some(PmCall::SProf),
            40 => Some(PmCall::ProcEventMask),
            41 => Some(PmCall::SrvFork),
            42 => Some(PmCall::SrvKill),
            43 => Some(PmCall::ExecNew),
            44 => Some(PmCall::ExecRestart),
            45 => Some(PmCall::GetEpInfo),
            46 => Some(PmCall::GetProcNr),
            47 => Some(PmCall::GetSysInfo),
            _ => None,
        }
    }

    /// 返回调用号（消息 `m_type` 值，C: `PM_BASE + N`）。
    pub fn call_nr(self) -> i32 {
        self as i32
    }
}

/// 分发一个已注册的 PM 调用（C: `call_vec[call_index]()`，main.c:99）。
///
/// **单一分发表**（ARCH A-5）：C 的 `call_vec` 是一张 47 项函数指针表，
/// 所有已注册调用走同一路径；Rust 对应为这一个穷尽 match——已落地的
/// handler（7 个）在此解码消息载荷并调用，未落地的（40 个）返回
/// `Reply(ENOSYS)` 占位（过渡差异，文档化于 04 文档 §3.6 D6）。
/// 主循环不得绕过本函数内联拦截任何调用（旧实现的 7 个内联块已收编）。
///
/// handler 签名与 C `int (*)(void)` 的差异见 04 文档 §3.3（D3/D5）：
/// C handler 读全局 `m_in`/`mp`；Rust 显式传 `table`/`events`/`transport`
/// + caller 槽位 + 消息引用（ARCH A-3：隐式全局 → 显式参数）。
// 参数面是 ARCH A-3 的刻意形状:每个参数对应一条独立的 seam(表/事件/
// 传输/内核网关/定时器束/调用方/消息),不应为了 lint 计数再聚合。
#[allow(clippy::too_many_arguments)]
pub fn dispatch_pm_call<T: IpcTransport>(
    call: PmCall,
    table: &mut ProcTable,
    events: &mut EventRegistry,
    transport: &mut T,
    kern: &mut dyn crate::exit::KernelGateway,
    timers: &mut crate::timer::TimerFaces,
    caller: UserSlot,
    msg: &Message,
) -> ReplyIntent {
    match call {
        // C: do_fork（forkexit.c:139 `return SUSPEND`）——fork 的回复不是
        // 同步的：do_fork 发出 VFS_PM_FORK 后返回 SUSPEND，父/子回复由
        // 05 的 VFS_PM_FORK_REPLY（handle_vfs_reply，main.c:369-394）
        // 异步完成。失败（表满/内存不足/VM 拒绝）同步回复 errno
        //（forkexit.c:60-79 的 `return EAGAIN/ENOMEM/s`）。
        PmCall::Fork => match crate::fork::do_fork(table, msg.m_source, transport, kern) {
            Ok(_child_pid) => ReplyIntent::ReplyLater,
            Err(e) => ReplyIntent::Reply(PmError::from(e).to_errno()),
        },
        // C: do_srv_fork（forkexit.c:237/239）——与 fork 相反：立即
        // reply(child, OK) 后同步返回 pid。
        PmCall::SrvFork => {
            let (uid, gid) = super::decode::srv_fork(msg);
            let params = crate::mproc::SrvForkParams { uid, gid };
            match crate::fork::do_srv_fork(table, msg.m_source, params, transport, kern) {
                Ok(child_pid) => ReplyIntent::Reply(child_pid),
                Err(e) => ReplyIntent::Reply(PmError::from(e).to_errno()),
            }
        }
        // C: do_exit（forkexit.c:246-266）——返回 SUSPEND 且**永不回复**
        //（进程已消亡，"beyond the grave"），plan.md §7.3 的 NoReply 子情形。
        PmCall::Exit => {
            let status = super::decode::exit(msg);
            // do_exit 当前恒返 NoReply——直接透传而非丢弃，未来语义变化
            // 时回复意图不会被静默吞掉。
            crate::exit::do_exit(table, caller, status, transport, kern)
        }
        // C: do_wait4（forkexit.c:471-542）——同步回复（W_STOPCODE/WNOHANG/
        // ECHILD）或 SUSPEND（wait_test 命中后挂 WAITING，由 tell_parent/
        // tell_tracer 稍后回复）。handler 直接返回回复意图。
        PmCall::Wait4 => {
            let (pidarg, options, addr) = super::decode::wait4(msg);
            crate::wait::do_wait4(
                table,
                caller,
                pidarg,
                options as u32,
                VirBytes(addr),
                transport,
                kern,
            )
        }
        // C: do_kill（signal.c:197-204）——成功回复 0；caller 自杀且已处
        // EXITING 时是 SUSPEND（check_sig 的 sig_proc_exit 链，signal.c:384）
        // → 本次不回复。
        PmCall::Kill => {
            let (pid, signo) = super::decode::kill(msg);
            match crate::signal::do_kill(table, caller, pid, signo, kern, transport) {
                Ok(_count) => {
                    if table.procs[caller.get()].state.lifecycle.is_exiting() {
                        ReplyIntent::ReplyLater
                    } else {
                        ReplyIntent::Reply(0)
                    }
                }
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_srv_kill（signal.c:204-211）——同 do_kill 的回复模式。
        PmCall::SrvKill => {
            let (pid, signo) = super::decode::srv_kill(msg);
            match crate::signal::do_srv_kill(table, caller, pid, signo, kern, transport) {
                Ok(_count) => {
                    if table.procs[caller.get()].state.lifecycle.is_exiting() {
                        ReplyIntent::ReplyLater
                    } else {
                        ReplyIntent::Reply(0)
                    }
                }
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_proceventmask（event.c:179-206）——掩码更新走 EventRegistry
        //（06-event-subscription.md），回复意图由 registry 决定。
        PmCall::ProcEventMask => {
            let mask_bits = super::decode::proceventmask(msg);
            let mask = ProcEventMask::from_bits_truncate(mask_bits);
            events.do_proceventmask_mut(caller, mask, table, transport, kern)
        }
        // C: do_trace（trace.c:42-250）——ptrace 全命令面（V3-P1-1 接线，
        // 批次 B 的 trace 半边）。守卫/权限错误同步回复 errno；OK 回复的
        // data 载荷经 ipc.reply 预填（m_pm_lc_ptrace.data，C trace.c:59 等），
        // 主循环 reply() 以 m_type 携带返回码整体发出。
        PmCall::Ptrace => {
            let (pid, preq, addr, data) = super::decode::ptrace(msg);
            let req = crate::trace::PtraceReq { req: preq, pid, addr, data };
            match crate::trace::do_trace(table, caller, req, kern, transport) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_setitimer——批次 D。value/ovalue 是 `struct itimerval`
        // 用户态指针(32 字节:interval@0..16 + value@16..32,每域
        // tv_sec@0..8 + tv_usec@8..16),字节搬运经网关 copy 缝;旧值只在
        // ovalue 非零时写回。
        PmCall::Itimer => {
            let (which, value_ptr, ovalue_ptr) = super::decode::itimer(msg);
            let mut raw = [0u8; 32];
            if let Err(e) = kern.copy_from_user(msg.m_source, value_ptr, &mut raw) {
                return ReplyIntent::Reply(positive_errno(e));
            }
            let le_i64 = |b: &[u8]| i64::from_le_bytes(b.try_into().unwrap());
            let op = crate::timer::ItimerOp {
                set: Some(crate::timer::Itimerval {
                    it_interval: crate::timer::Timeval {
                        tv_sec: le_i64(&raw[0..8]),
                        tv_usec: le_i64(&raw[8..16]),
                    },
                    it_value: crate::timer::Timeval {
                        tv_sec: le_i64(&raw[16..24]),
                        tv_usec: le_i64(&raw[24..32]),
                    },
                }),
                get: ovalue_ptr != 0,
            };
            let conv = crate::timer::TicksConv { hz: timers.system_hz };
            match crate::timer::do_itimer(
                table,
                caller,
                which,
                op,
                &conv,
                timers.vctl,
                timers.tctl,
            ) {
                Ok(old) => {
                    if let Some(old) = old
                        && ovalue_ptr != 0
                    {
                        let mut out = [0u8; 32];
                        out[0..8].copy_from_slice(&old.it_interval.tv_sec.to_le_bytes());
                        out[8..16].copy_from_slice(&old.it_interval.tv_usec.to_le_bytes());
                        out[16..24].copy_from_slice(&old.it_value.tv_sec.to_le_bytes());
                        out[24..32].copy_from_slice(&old.it_value.tv_usec.to_le_bytes());
                        if let Err(e) = kern.copy_to_user(&out, msg.m_source, ovalue_ptr) {
                            return ReplyIntent::Reply(positive_errno(e));
                        }
                    }
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(positive_errno(e.to_errno())),
            }
        }
        // C: do_sigaction（signal.c:40-86）——批次 B 余。act/oact 是
        // `struct sigaction` 用户态指针(LP64 32 字节:handler@0/mask@8/
        // flags@24),字节搬运走网关 copy 缝;旧值快照在 install 前
        // (handle_sigaction),时序与 C 的 oact-先拷差异仅在 fault 排序,
        // 注释已锚 C。
        PmCall::SigAction => {
            let (nr, act_ptr, oact_ptr, ret) = super::decode::sigaction(msg);
            let mut svec = [0u8; 32];
            let act = if act_ptr != 0 {
                if let Err(e) = kern.copy_from_user(msg.m_source, act_ptr, &mut svec) {
                    return ReplyIntent::Reply(positive_errno(e));
                }
                let le_u64 = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap());
                Some(crate::mproc::SigAction {
                    sa_handler: le_u64(&svec[0..8]) as usize,
                    sa_mask: le_u64(&svec[8..16]),
                    sa_flags: i32::from_le_bytes(svec[24..28].try_into().unwrap()),
                })
            } else {
                None
            };
            match handle_sigaction(
                table,
                caller,
                SigActionReq { signo: nr, act, need_oact: oact_ptr != 0, sigreturn: VirBytes(ret) },
            ) {
                Ok(old) => {
                    if let (Some(old), false) = (old, oact_ptr == 0) {
                        let mut out = [0u8; 32];
                        out[0..8].copy_from_slice(&(old.sa_handler as u64).to_le_bytes());
                        out[8..16].copy_from_slice(&old.sa_mask.to_le_bytes());
                        out[24..28].copy_from_slice(&old.sa_flags.to_le_bytes());
                        if let Err(e) = kern.copy_to_user(&out, msg.m_source, oact_ptr) {
                            return ReplyIntent::Reply(positive_errno(e));
                        }
                    }
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_sigsuspend(signal.c:157-171)——存 mask/装新掩码/挂起。
        PmCall::SigSuspend => {
            let (_how, _ctx, set) = super::decode::sigset(msg);
            handle_sigsuspend(table, caller, set)
        }
        // C: do_sigpending(signal.c:88-97)——reply 载荷 m_pm_lc_sigset.set。
        PmCall::SigPending => {
            let pending = handle_sigpending(table, caller);
            table.procs[caller.get()].ipc.reply = Some(Message {
                m_u: minix_types::MessageUnion {
                    m_pm_lc_sigset: minix_types::MessPmLcSigset {
                        set: [pending as u32, (pending >> 32) as u32, 0, 0],
                        _padding: [0; 40],
                    },
                },
                ..Message::default()
            });
            ReplyIntent::Reply(0)
        }
        // C: do_sigprocmask(signal.c:99-155)——旧掩码经 reply 载荷回;
        // needs_check 的内联重投(check_pending)归信号流装配(S3 余件)。
        PmCall::SigProcMask => {
            let (how, _ctx, set) = super::decode::sigset(msg);
            match handle_sigprocmask(table, caller, how, set) {
                Ok((old, effect)) => {
                    table.procs[caller.get()].ipc.reply = Some(Message {
                        m_u: minix_types::MessageUnion {
                            m_pm_lc_sigset: minix_types::MessPmLcSigset {
                                set: [old as u32, (old >> 32) as u32, 0, 0],
                                _padding: [0; 40],
                            },
                        },
                        ..Message::default()
                    });
                    let _ = effect;
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_sigreturn(signal.c:173-192)——恢复掩码 + sys_sigreturn,
        // 无条件 check_pending(190)归信号流装配。
        PmCall::SigReturn => {
            let (_how, ctx, set) = super::decode::sigset(msg);
            let mut sig_kern = SigReturnKern(kern);
            match handle_sigreturn(table, caller, set, VirBytes(ctx), &mut sig_kern) {
                Ok(()) => ReplyIntent::Reply(0),
                // fault 载荷是 sys_sigreturn 的原始负 errno。
                Err(crate::signal_handlers::SigReturnError::Fault(code)) => {
                    ReplyIntent::Reply(positive_errno(code))
                }
            }
        }
        // ===== S2 批次 A:凭证族 13 调用(getset.c)=====
        PmCall::GetPid => {
            match do_get(table, caller, GetOp::GetPid, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::SetUid => {
            let id = super::decode::setid(msg);
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetUid(id), &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::GetUid => {
            match do_get(table, caller, GetOp::GetUid, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::SetGroups => {
            // C getset.c:184-206 —— num<0 → EINVAL;gid 数组经 copy 缝
            // 拷入后整组安装并转发 VFS。
            let (num, ptr) = super::decode::groups(msg);
            if num < 0 {
                return ReplyIntent::Reply(EINVAL);
            }
            let mut copier = SysCopyGroups { kern, who: msg.m_source };
            match copier.copy_from_user(VirBytes(ptr), num as usize) {
                Ok(gids) => {
                    let mut fwd = SysVfsForward { transport };
                    match do_set(
                        table,
                        caller,
                        SetOp::SetGroups { gids },
                        &mut NoopGroups,
                        &mut fwd,
                    ) {
                        Ok(intent) => intent,
                        Err(e) => ReplyIntent::Reply(e.to_errno()),
                    }
                }
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::GetGroups => {
            let (num, ptr) = super::decode::groups(msg);
            let mut copier = SysCopyGroups { kern, who: msg.m_source };
            match do_get(table, caller, GetOp::GetGroups { count: num, ptr: VirBytes(ptr) }, &mut copier) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::SetGid => {
            let id = super::decode::setid(msg);
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetGid(id), &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::GetGid => {
            match do_get(table, caller, GetOp::GetGid, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::SetSid => {
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetSid, &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::GetPgrp => {
            match do_get(table, caller, GetOp::GetPgrp, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::SetEUid => {
            let id = super::decode::setid(msg);
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetEUid(id), &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::SetEGid => {
            let id = super::decode::setid(msg);
            let mut copier = NoopGroups;
            let mut fwd = SysVfsForward { transport };
            match do_set(table, caller, SetOp::SetEGid(id), &mut copier, &mut fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::IsSetUid => {
            match do_get(table, caller, GetOp::Issetugid, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        PmCall::GetSid => {
            let pid = super::decode::getsid(msg);
            match do_get(table, caller, GetOp::GetSid { pid }, &mut NoopGroups) {
                Ok(r) => get_result_intent(table, caller, r),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // ===== S6 批次 E:exec 族三调用(exec.c)=====
        // C: do_exec(pm/exec.c:38-56)——exec 由 VFS/RS 发起:转发
        // VFS_PM_EXEC 五域后 tell_vfs,返回 SUSPEND。
        PmCall::Exec => {
            let (path, path_len, frame, frame_len, ps_str) = super::decode::exec(msg);
            let req = crate::exec::ExecRequest {
                caller,
                endpoint: table.procs[caller.get()].endpoint(),
                path: VirBytes(path),
                path_len: path_len as usize,
                frame: VirBytes(frame),
                frame_len: frame_len as usize,
                ps_str: VirBytes(ps_str),
            };
            let mut vfs_fwd = SysVfsExec { transport };
            match crate::exec::do_exec(table, caller, req, &mut vfs_fwd) {
                Ok(intent) => intent,
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_newexec(pm/exec.c:64-123)——endpt@0/ptr@8,ptr 指向 VFS
        // 内存中的 exec_info(拷入 192 字节);reply 载荷 m_pm_lexec_exec_new
        // .suid@0(C :120-122)。
        PmCall::ExecNew => {
            let (endpoint_raw, ptr) = super::decode::exec_new(msg);
            let mut info_raw = [0u8; EXEC_INFO_COPY_SIZE];
            if let Err(e) = kern.copy_from_user(msg.m_source, ptr, &mut info_raw) {
                return ReplyIntent::Reply(positive_errno(e));
            }
            let le_u64 = |b: &[u8]| u64::from_le_bytes(b.try_into().unwrap());
            let progname = {
                let mut n = [0u8; 16];
                n.copy_from_slice(&info_raw[32..48]);
                n
            };
            let info = crate::exec::ExecInfo {
                allow_setuid: i32::from_le_bytes(info_raw[56..60].try_into().unwrap()) != 0,
                new_uid: u32::from_le_bytes(info_raw[48..52].try_into().unwrap()),
                new_gid: u32::from_le_bytes(info_raw[52..56].try_into().unwrap()),
                progname,
                stack_high: VirBytes(le_u64(&info_raw[184..192])),
                frame_len: le_u64(&info_raw[24..32]) as usize,
            };
            match crate::exec::do_newexec(table, msg.m_source, Endpoint(endpoint_raw), info) {
                Ok(allow) => {
                    // C :120-122 —— reply 载荷 suid@0。
                    let mut reply = Message::default();
                    let suid = i32::from(allow);
                    // SAFETY: m_pm_lexec_exec_new.suid@0 的 raw 字节写入;
                    // 索引切片构成对 union 字段的读借用,需 unsafe 块。
                    unsafe {
                        reply.m_u.raw[0..4].copy_from_slice(&suid.to_le_bytes());
                    }
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_execrestart(pm/exec.c:130-151)——仅 RS;RS 门在
        // do_execrestart 内。服务装配照 vfs.rs ExecServices 先例。
        PmCall::ExecRestart => {
            let (endpt, result, pc, ps_str) = super::decode::exec_restart(msg);
            let info = crate::exec::ExecRestartInfo {
                endpoint: Endpoint(endpt),
                result,
                pc: VirBytes(pc),
                ps_str: VirBytes(ps_str),
            };
            let mut svc = crate::ipc::vfs::ExecServices { transport, kern };
            match crate::exec::do_execrestart(table, msg.m_source, info, &mut svc) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // ===== S4 批次 C:时间族六调用(time.c)=====

        // C: do_stime(time.c:110-131)——SUPER_USER 门 + boottime 锚重置。
        PmCall::Stime => {
            let (_clk, _now, sec, _nsec) = super::decode::time(msg);
            let src = crate::time::SysClockSource;
            let mut boot = crate::time::SysBootTimeCtl;
            match crate::time::do_stime(
                table,
                caller,
                sec as i64,
                timers.system_hz,
                &src,
                &mut boot,
            ) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_gettime(time.c:22-47)——clk 分派 REALTIME/MONOTONIC,
        // decompose 后 reply 预填 m_lc_pm_time{sec,nsec}。
        PmCall::GetTimeOfDay | PmCall::ClockGetTime => {
            let (clk, _now, _sec, _nsec) = super::decode::time(msg);
            let clk = match crate::time::ClockId::try_from(clk) {
                Ok(c) => c,
                Err(e) => return ReplyIntent::Reply(e.to_errno()),
            };
            let src = crate::time::SysClockSource;
            match crate::time::do_gettime(&src, clk, timers.system_hz) {
                Ok(ts) => {
                    let mut reply = minix_types::Message::default();
                    // SAFETY: m_lc_pm_time 应答载荷(sec@0/nsec@16),与
                    // C do_gettime 的 mp_reply 同臂。
                    unsafe {
                        reply.m_u.raw[0..8].copy_from_slice(&(ts.sec as u64).to_ne_bytes());
                        reply.m_u.raw[16..24].copy_from_slice(&(ts.nsec as i64).to_ne_bytes());
                    }
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_clock_getres——1e9/hz(REALTIME/MONOTONIC 同值)。
        PmCall::ClockGetRes => {
            let (clk, _now, _sec, _nsec) = super::decode::time(msg);
            let clk = match crate::time::ClockId::try_from(clk) {
                Ok(c) => c,
                Err(e) => return ReplyIntent::Reply(e.to_errno()),
            };
            match crate::time::do_getres(clk, timers.system_hz) {
                Ok(ts) => {
                    let mut reply = minix_types::Message::default();
                    // SAFETY: 应答臂 sec@0/nsec@16。
                    unsafe {
                        reply.m_u.raw[0..8].copy_from_slice(&(ts.sec as u64).to_ne_bytes());
                        reply.m_u.raw[16..24].copy_from_slice(&(ts.nsec as i64).to_ne_bytes());
                    }
                    table.procs[caller.get()].ipc.reply = Some(reply);
                    ReplyIntent::Reply(0)
                }
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // C: do_clock_settime(time.c:71-88)——SUPER_USER 门 + MONOTONIC
        // 不可变。
        PmCall::ClockSetTime => {
            let (clk, now, sec, nsec) = super::decode::time(msg);
            let clk = match crate::time::ClockId::try_from(clk) {
                Ok(c) => c,
                Err(e) => return ReplyIntent::Reply(e.to_errno()),
            };
            let mut ctl = crate::time::SysSetTimeCtl;
            match crate::time::do_settime(
                table,
                caller,
                crate::time::TimeRequest {
                    clk,
                    now,
                    sec: sec as i64,
                    nsec,
                },
                &mut ctl,
            ) {
                Ok(()) => ReplyIntent::Reply(0),
                Err(e) => ReplyIntent::Reply(e.to_errno()),
            }
        }
        // 其余 40 个调用：handler 归属 07~20（ENOSYS 占位）。逐调用的
        // 接线台账（C handler / Rust 逻辑位置 / wire·wrapper 前置条件 /
        // 建议批次 A-G）见 04-stage-pm/todo.md §11.1——每接线一批同步
        // 划账该表（V2-P2-6：兜底臂使"未接线"无需标记即可编译，台账是
        // 义务的承接面）。
        _ => ReplyIntent::Reply(ENOSYS),
    }
}

use crate::signal_handlers::{
    handle_sigaction, handle_sigpending, handle_sigprocmask, handle_sigreturn,
    handle_sigsuspend, SigActionReq,
};
use crate::mproc::SigMsg;

/// 无拷贝组表出口:不搬组表的调用(GetPid/GetUid 等)仍需满足
/// do_get/do_set 的 trait 形状;copy 路径诚实失败(不可达)。
struct NoopGroups;
impl CopyGroups for NoopGroups {
    fn copy_to_user(&mut self, _gids: &[Gid], _ptr: VirBytes) -> Result<(), SetError> {
        Err(SetError::Fault)
    }
    fn copy_from_user(&mut self, _ptr: VirBytes, _ngroups: usize) -> Result<Vec<Gid>, SetError> {
        Err(SetError::Fault)
    }
}

/// 凭证族的组表拷贝适配(S2):`CopyGroups` 语义经网关 copy 缝落地,
/// gid 按 u32 LE 逐元素搬运(NGROUPS_MAX 槽,credentials.rs A-11)。
struct SysCopyGroups<'a> {
    kern: &'a mut dyn crate::exit::KernelGateway,
    who: Endpoint,
}

impl CopyGroups for SysCopyGroups<'_> {
    fn copy_to_user(&mut self, gids: &[Gid], ptr: VirBytes) -> Result<(), SetError> {
        let mut raw = alloc::vec![0u8; gids.len() * 4];
        for (i, g) in gids.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&g.to_le_bytes());
        }
        self.kern
            .copy_to_user(&raw, self.who, ptr.0)
            .map_err(|_| SetError::Fault)
    }

    fn copy_from_user(&mut self, ptr: VirBytes, ngroups: usize) -> Result<Vec<Gid>, SetError> {
        let mut raw = alloc::vec![0u8; ngroups * 4];
        self.kern
            .copy_from_user(self.who, ptr.0, &mut raw)
            .map_err(|_| SetError::Fault)?;
        Ok(raw
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect())
    }
}

/// exec 臂的 VFS 转发适配(S6):VFS_PM_EXEC 五域编码 + tell_vfs 三段
/// (exec.c:44-54)。
struct SysVfsExec<'a, T: IpcTransport + ?Sized> {
    transport: &'a mut T,
}

impl<T: IpcTransport + ?Sized> crate::exec::VfsExec for SysVfsExec<'_, T> {
    fn forward_exec(
        &mut self,
        table: &mut ProcTable,
        caller: UserSlot,
        req: crate::exec::ExecRequest,
    ) -> Result<ReplyIntent, crate::exec::ExecError> {
        let call = minix_types::VfsCall::Exec {
            endpoint: req.endpoint,
            path: req.path.0,
            path_len: req.path_len as i32,
            frame: req.frame.0,
            frame_len: req.frame_len as i32,
            ps_str: req.ps_str.0 as i32,
        };
        crate::ipc::vfs::tell_vfs(table, caller, call, self.transport);
        Ok(ReplyIntent::ReplyLater)
    }
}

/// `struct exec_info` 的 PM 消费域字节宽(libexec.h:21-58,LP64):
/// proc_e@0/hdr@8/hdr_len@16/frame_len@24/progname@32(16)/new_uid@48/
/// new_gid@52/allow_setuid@56/…/stack_high@184;拷入尺寸 192。
const EXEC_INFO_COPY_SIZE: usize = 192;

/// 凭证族的 VFS 转发适配(S2):编码 VfsCall 后走 tell_vfs 同型三段
/// (not-idle 断言 / send / VFS_CALL 置位——vfs.rs:166-175)。
struct SysVfsForward<'a, T: IpcTransport + ?Sized> {
    transport: &'a mut T,
}

impl<T: IpcTransport + ?Sized> VfsForwarder for SysVfsForward<'_, T> {
    fn forward_set(
        &mut self,
        table: &mut ProcTable,
        slot: UserSlot,
        ep: Endpoint,
        op: &SetOp,
    ) -> Result<ReplyIntent, SetError> {
        let call = match op {
            SetOp::SetUid(uid) | SetOp::SetEUid(uid) => minix_types::VfsCall::SetUid {
                endpoint: ep,
                eid: *uid as i32,
                rid: *uid as i32,
            },
            SetOp::SetGid(gid) | SetOp::SetEGid(gid) => minix_types::VfsCall::SetGid {
                endpoint: ep,
                eid: *gid as i32,
                rid: *gid as i32,
            },
            SetOp::SetGroups { gids } => minix_types::VfsCall::SetGroups {
                endpoint: ep,
                group_no: gids.len() as i32,
                group_addr: 0,
            },
            SetOp::SetSid => minix_types::VfsCall::SetSid { endpoint: ep },
        };
        crate::ipc::vfs::tell_vfs(table, slot, call, self.transport);
        Ok(ReplyIntent::ReplyLater)
    }
}

/// `do_get` 结果 → 回复意图(C getset.c:52-81 的双值载荷语义)。
/// Uid/Gid 的 real/eff 双值经 reply 预填槽(m1i1/m1i2)交付。
fn get_result_intent(table: &mut ProcTable, caller: UserSlot, r: GetResult) -> ReplyIntent {
    let mut prefill = |real: i32, eff: i32| {
        let mut msg = Message::default();
        msg.m_u.m_m1.m1i1 = real;
        msg.m_u.m_m1.m1i2 = eff;
        table.procs[caller.get()].ipc.reply = Some(msg);
    };
    match r {
        GetResult::Uid { real, eff } => {
            prefill(real as i32, eff as i32);
            ReplyIntent::Reply(0)
        }
        GetResult::Gid { real, eff } => {
            prefill(real as i32, eff as i32);
            ReplyIntent::Reply(0)
        }
        GetResult::Groups { count } => ReplyIntent::Reply(count as i32),
        GetResult::Pid { self_pid, parent } => {
            prefill(self_pid, parent);
            ReplyIntent::Reply(0)
        }
        GetResult::Pgrp(p) | GetResult::Sid(p) => ReplyIntent::Reply(p),
        GetResult::Issetugid(t) => ReplyIntent::Reply(t as i32),
        GetResult::Error(e) => ReplyIntent::Reply(e),
    }
}

/// sigreturn 臂的网关适配:`handle_sigreturn` 只消费 `KernelSig::sigreturn`
/// (C do_sigreturn 不调 sys_sigsend),sigsend 在此适配器内诚实失败。
struct SigReturnKern<'a>(&'a mut dyn crate::exit::KernelGateway);

impl crate::signal_handlers::KernelSig for SigReturnKern<'_> {
    fn sigsend(&mut self, _ep: Endpoint, _msg: &SigMsg) -> Result<(), i32> {
        Err(ENOSYS)
    }
    fn sigreturn(&mut self, ep: Endpoint, ctx: VirBytes) -> Result<(), i32> {
        self.0.sys_sigreturn(ep, ctx)
    }
}

/// 网关/wrapper 的负 errno 约定 → reply 的正 errno(错误保真,取绝对值)。
#[cfg(test)]
fn leak_timers() -> crate::timer::TimerFaces<'static> {
    let tctl: &'static mut dyn crate::timer::TimerCtl = Box::leak(Box::new(crate::timer::SysTimerCtl::new()));
    let vctl: &'static mut dyn crate::timer::VTimerCtl = Box::leak(Box::new(crate::timer::SysVTimerCtl));
    crate::timer::TimerFaces { tctl, vctl, system_hz: 100 }
}

fn positive_errno(e: i32) -> i32 {
    if e < 0 {
        -e
    } else {
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::TestIpcTransport;
    use minix_types::Endpoint;

    /// 测试用内核网关 mock（sys_kill/sys_clear 恒 OK）。
    #[derive(Default)]
    struct NoopKernel;
    impl crate::exit::KernelGateway for NoopKernel {
        fn sys_sigsend(&mut self, _ep: minix_types::Endpoint, _sigmsg: &minix_sys::syscall::SigMsgWire) -> Result<(), i32> { Ok(()) }
        fn get_ksig(&mut self) -> Result<Option<(minix_types::Endpoint, u64)>, i32> { Ok(None) }
    fn end_ksig(&mut self, _ep: minix_types::Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_trace(&mut self, _req: i32, _ep: minix_types::Endpoint, _addr: u64, _data: &mut i64) -> Result<(), i32> { Ok(()) }
    fn sys_vircopy(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _dst_ep: minix_types::Endpoint, _dst: u64, _len: u64) -> Result<(), i32> { Ok(()) }
    fn copy_from_user(&mut self, _src_ep: minix_types::Endpoint, _src: u64, _bytes: &mut [u8]) -> Result<(), i32> { Ok(()) }
        fn sys_delay_stop(&mut self, _ep: minix_types::Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_kill(&mut self, _ep: Endpoint, _sig: i32) -> Result<(), i32> { Ok(()) }
        fn sys_clear(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn sys_abort(&mut self, _how: i32) -> Result<(), i32> { Ok(()) }
        fn copy_to_user(&mut self, _bytes: &[u8], _dst_ep: Endpoint, _dst_addr: u64) -> Result<(), i32> {
            Ok(())
        }
        fn sys_resume(&mut self, _ep: Endpoint) -> Result<(), i32> { Ok(()) }
        fn proc_times(&mut self, _ep: Endpoint) -> Result<(minix_types::Clock, minix_types::Clock), i32> { Ok((0, 0)) }
    }
        fn setup_with_caller(slot: usize, ep: Endpoint) -> (ProcTable, EventRegistry, TestIpcTransport) {
        let mut table = ProcTable::new();
        table.procs[slot].identity.endpoint = ep;
        table.procs[slot].identity.id.pid = 100 + slot as i32;
        table.procs[slot].state.lifecycle = crate::mproc::Lifecycle::Running;
        (table, EventRegistry::new(), TestIpcTransport::new())
    }

    #[test]
    fn test_call_nr_roundtrip_all_registered() {
        // callnr.h:14-60 — 47 个调用号全部可解码且可回编码。
        for nr in 1..=47 {
            let call = PmCall::from_call_nr(nr).unwrap_or_else(|| panic!("nr {} unregistered", nr));
            assert_eq!(call.call_nr(), nr);
        }
    }

    #[test]
    fn test_call_nr_rejects_unregistered() {
        // nr == 0 保留（message type 0 traditionally reserved）；
        // 越界 48/0xff+ 未注册。
        assert!(PmCall::from_call_nr(0).is_none());
        assert!(PmCall::from_call_nr(48).is_none());
        assert!(PmCall::from_call_nr(0x100).is_none());
        assert!(PmCall::from_call_nr(-1).is_none());
    }

    #[test]
    fn test_dispatch_fork_success_is_reply_later() {
        // C: do_fork 返回 SUSPEND（forkexit.c:139）——fork 同步不回复；
        // 父进程已注册时 do_fork 走到 vm_fork（脚本化 OK 应答）→ ReplyLater。
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let _kern = NoopKernel;
        let mut vm_reply = Message { m_type: minix_types::OK, ..Message::default() };
        vm_reply.m_u.m_m1.m1i3 = Endpoint::from_generation_slot(2, 1).0;
        transport.queue_sendrec_reply(vm_reply);

        let mut msg = Message { m_type: 2, ..Message::default() };
        msg.m_source = ep;
        let intent = dispatch_pm_call(
            PmCall::Fork,
            &mut table,
            &mut events,
            &mut transport,
            &mut NoopKernel,
            &mut leak_timers(),
            UserSlot::new(3),
            &msg,
        );
        assert_eq!(intent, ReplyIntent::ReplyLater);
        // fork 链路真实发出：先 VM_FORK（sendrec 记录在 sent），后 VFS_PM_FORK。
        assert_eq!(transport.sent()[0].0, Endpoint::VM);
        assert!(transport.sent().len() >= 2);
    }

    #[test]
    fn test_dispatch_fork_parent_unknown_is_error_reply() {
        // C: forkexit.c:59-79 的同步可失败段——父进程不存在 → errno 回复
        //（pm_isokendpt 防御，ESRCH），不再有"静默 ReplyLater"的假路径。
        let (mut table, mut events, mut transport) =
            setup_with_caller(3, Endpoint::from_generation_slot(1, 3));
        let mut msg = Message { m_type: 2, ..Message::default() };
        msg.m_source = Endpoint::from_generation_slot(9, 9); // 未注册的父
        let intent = dispatch_pm_call(
            PmCall::Fork,
            &mut table,
            &mut events,
            &mut transport,
            &mut NoopKernel,
            &mut leak_timers(),
            UserSlot::new(3),
            &msg,
        );
        assert_eq!(
            intent,
            ReplyIntent::Reply(minix_types::PmError::InvalidEndpoint.to_errno())
        );
    }

    #[test]
    fn test_dispatch_exit_is_no_reply() {
        // C: do_exit 返回 SUSPEND 且永不回复（forkexit.c:246-266，
        // plan.md §7.3 的 NoReply 子情形）。
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let _kern = NoopKernel;
        let mut msg = Message { m_type: 1, ..Message::default() };
        msg.m_source = ep;
        let intent = dispatch_pm_call(
            PmCall::Exit,
            &mut table,
            &mut events,
            &mut transport,
            &mut NoopKernel,
            &mut leak_timers(),
            UserSlot::new(3),
            &msg,
        );
        assert_eq!(intent, ReplyIntent::NoReply);
    }

    #[test]
    fn test_dispatch_unimplemented_call_is_enosys() {
        // C: call_vec[call_index]() 已注册但 handler 未实现 → ENOSYS 占位
        //（批次 A 接线后未接线者已缩至批次 C-G:25/26/27/28/33-39 等）。
        // SysUname(25) 归批次 G(20-misc),离当前批次最远。
        let ep = Endpoint::from_generation_slot(1, 3);
        let (mut table, mut events, mut transport) = setup_with_caller(3, ep);
        let _kern = NoopKernel;
        let mut msg = Message { m_type: 25, ..Message::default() };
        msg.m_source = ep;
        assert_eq!(
            dispatch_pm_call(
                PmCall::SysUname,
                &mut table,
                &mut events,
                &mut transport,
                &mut NoopKernel,
                &mut leak_timers(),
                UserSlot::new(3),
                &msg
            ),
            ReplyIntent::Reply(ENOSYS)
        );
    }
}
