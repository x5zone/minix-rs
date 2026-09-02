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

use crate::ipc::ReplyIntent;
use crate::mproc::ProcTable;
use minix_types::{ENOSYS, Endpoint};

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
/// handler 签名与 C `int (*)(void)` 的差异见 04 文档 §3.3（D3/D5）：
/// C handler 读全局 `m_in`/`mp`；Rust 显式传 `table` + caller endpoint。
///
/// 各 handler 的完整语义在对应文档落地前返回 `Reply(ENOSYS)` 占位——
/// 这是 C 中"已注册调用"与 Rust"已实现调用"的过渡差异，文档化于
/// 04 文档 §3.6（D6）。
pub fn dispatch_pm_call(call: PmCall, table: &mut ProcTable, caller: Endpoint) -> ReplyIntent {
    match call {
        // C: do_fork（forkexit.c:139 `return SUSPEND`）——fork 的回复不是
        // 同步的：do_fork 发出 VFS_PM_FORK 后返回 SUSPEND，父/子回复由
        // 05 的 VFS_PM_FORK_REPLY（handle_vfs_reply，main.c:369-394）
        // 异步完成。04 的分发层只建模"本次不回复"。
        //
        // DEFERRED: do_fork 本体（vm_fork/mproc 复制/VFS_PM_FORK/tracer
        // SIGSTOP）归 07-pm-fork.md；现有 fork.rs 协调占位（同步回复）
        // 与 C 语义不符，07 落地时替换。
        PmCall::Fork => ReplyIntent::ReplyLater,
        // 其余 46 个调用：handler 归属 07~20（ENOSYS 占位）。
        _ => ReplyIntent::Reply(ENOSYS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_dispatch_fork_is_reply_later() {
        // C: do_fork 返回 SUSPEND（forkexit.c:139）——fork 同步不回复。
        let mut table = ProcTable::new();
        let intent = dispatch_pm_call(
            PmCall::Fork,
            &mut table,
            Endpoint::from_generation_slot(1, 0),
        );
        assert_eq!(intent, ReplyIntent::ReplyLater);
    }

    #[test]
    fn test_dispatch_unimplemented_call_is_enosys() {
        // C: call_vec[call_index]() 已注册但 handler 未实现 → ENOSYS 占位
        // （DEFERRED 07~20）。
        let mut table = ProcTable::new();
        assert_eq!(
            dispatch_pm_call(
                PmCall::Kill,
                &mut table,
                Endpoint::from_generation_slot(1, 0)
            ),
            ReplyIntent::Reply(ENOSYS)
        );
        assert_eq!(
            dispatch_pm_call(
                PmCall::Exit,
                &mut table,
                Endpoint::from_generation_slot(1, 0)
            ),
            ReplyIntent::Reply(ENOSYS)
        );
    }
}
