//! PM service IPC message types.
//!
//! Defines the messages exchanged between PM and other services (Kernel, VM, VFS).

use crate::{EAGAIN, EINVAL, EIO, ENOMEM, ENOSYS, EPERM, ESRCH, Endpoint};

/// PM request message types.
///
/// These are the requests that PM receives from other services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmRequest {
    /// Fork request from user process (via kernel).
    ///
    /// Kernel sends this when a process calls fork().
    Fork {
        /// Caller process endpoint.
        caller: Endpoint,
    },
}

/// PM response message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmResponse {
    /// Fork succeeded (returned to parent process).
    ForkParent {
        /// Child process PID.
        child_pid: i32,
    },
    /// Fork succeeded (returned to child process).
    ForkChild,
    /// Operation failed.
    Error(PmError),
}

/// PM error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmError {
    /// Process table is full.
    ProcTableFull,
    /// Out of memory.
    OutOfMemory,
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Slot is already in use.
    SlotInUse,
    /// Permission denied (EPERM, e.g., non-RS srv_fork).
    PermissionDenied,
    /// Internal error.
    InternalError,
    /// Operation not implemented.
    NotImplemented,
}

impl crate::types::ToErrno for PmError {
    fn to_errno(&self) -> crate::types::Errno {
        crate::types::Errno::from_i32(self.to_errno())
    }
}

impl PmError {
    /// Converts error to errno value.
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::ProcTableFull => EAGAIN,
            Self::OutOfMemory => ENOMEM,
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::PermissionDenied => EPERM,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pm_fork_request() {
        let req = PmRequest::Fork {
            caller: Endpoint::PM,
        };

        match req {
            PmRequest::Fork { caller } => {
                assert_eq!(caller, Endpoint::PM);
            }
        }
    }

    #[test]
    fn test_pm_response_fork_parent() {
        let resp = PmResponse::ForkParent { child_pid: 100 };

        match resp {
            PmResponse::ForkParent { child_pid } => {
                assert_eq!(child_pid, 100);
            }
            _ => panic!("expected ForkParent"),
        }
    }

    #[test]
    fn test_pm_response_fork_child() {
        let resp = PmResponse::ForkChild;
        assert!(matches!(resp, PmResponse::ForkChild));
    }

    #[test]
    fn test_pm_error_to_errno() {
        assert_eq!(PmError::ProcTableFull.to_errno(), EAGAIN);
        assert_eq!(PmError::OutOfMemory.to_errno(), ENOMEM);
        assert_eq!(PmError::InvalidEndpoint.to_errno(), ESRCH);
        assert_eq!(PmError::NotImplemented.to_errno(), ENOSYS);
    }
}

// ── E7 A 批:凭证调用号与 wire 结构(callnr.h:15-45 + ipc.h)──
//
// PM_BASE = 0(无偏移基),A 批调用号 = 4..=37 的绝对值。
// 布局:i386 C 头的 m_lc_pm_* 全为 4 字节字段 + padding 至 56,LP64
// 仅 vir_bytes 指针需要 8 字节对齐(groups.ptr),padding 相应缩短
// (rs_start LP64 判例)。

/// C: `PM_GETPID` — callnr.h:17。
pub const PM_GETPID: i32 = 4;
/// C: `PM_SETUID` — callnr.h:18。
pub const PM_SETUID: i32 = 5;
/// C: `PM_GETUID` — callnr.h:19。
pub const PM_GETUID: i32 = 6;
/// C: `PM_PTRACE` — callnr.h:21。
pub const PM_PTRACE: i32 = 8;
/// C: `PM_SETGROUPS` — callnr.h:22.
pub const PM_SETGROUPS: i32 = 9;
/// C: `PM_GETGROUPS` — callnr.h:23.
pub const PM_GETGROUPS: i32 = 10;
/// C: `PM_SETGID` — callnr.h:25.
pub const PM_SETGID: i32 = 12;
/// C: `PM_GETGID` — callnr.h:26.
pub const PM_GETGID: i32 = 13;
/// C: `PM_SETSID` — callnr.h:28.
pub const PM_SETSID: i32 = 15;
/// C: `PM_GETPRIORITY` — callnr.h:41.
pub const PM_GETPRIORITY: i32 = 26;
/// C: `PM_SETPRIORITY` — callnr.h:42.
pub const PM_SETPRIORITY: i32 = 27;
/// C: `PM_SETEUID` — callnr.h:44.
pub const PM_SETEUID: i32 = 29;
/// C: `PM_SETEGID` — callnr.h:45.
pub const PM_SETEGID: i32 = 30;
/// C: `PM_GETSID` — callnr.h:46.
pub const PM_GETSID: i32 = 32;
/// C: `PM_REBOOT` — callnr.h:52.
pub const PM_REBOOT: i32 = 37;

/// SYS_SETUID / SYS_SETGID 请求载荷(C: `mess_lc_pm_setuid` /
/// `mess_lc_pm_setgid` — ipc.h:525-529,字段 4 字节 + padding[52])。
///
/// SETGID 与 SETUID 同布局(gid_t 与 uid_t 同宽),共用此结构;
/// 语义由调用号区分(callnr.h:18/:25)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSetid {
    /// 新的 uid/gid。C: `uid_t uid`(setuid)或 `gid_t gid`(setgid)。
    pub id: u32,
    /// Padding to 56 bytes (C: union payload size).
    pub _padding: [u8; 52],
}

/// SYS_GETSID 请求载荷(C: `mess_lc_pm_getsid` — ipc.h:453-457,
/// `pid_t pid` + padding[52])。pid 为 0 时查调用进程自身的会话。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmGetsid {
    /// 目标进程 pid(0 = 调用进程)。C: `pid_t pid`。
    pub pid: i32,
    /// Padding to 56 bytes.
    pub _padding: [u8; 52],
}

/// SYS_SETGROUPS / SYS_GETGROUPS 请求载荷(C: `mess_lc_pm_groups` —
/// ipc.h:459-465:`int num; vir_bytes ptr`)。LP64:`ptr` 8 字节对齐到
/// offset 8,padding 缩短至 44(总 56 不变,rs_start LP64 判例)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmGroups {
    /// 组数(SET)或请求的组数(GET)。C: `int num`。
    pub num: i32,
    /// gid_t 数组指针。C: `vir_bytes ptr`。
    pub ptr: u64,
    /// Padding to 56 bytes (LP64: C i386 的 48 → 40)。
    pub _padding: [u8; 40],
}

#[cfg(test)]
mod credential_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C 绝对值 pin:A 批凭证调用号(callnr.h:17-52,PM_BASE=0)。
    #[test]
    fn test_pm_credential_call_numbers_match_c() {
        assert_eq!(PM_GETPID, 4); // callnr.h:17
        assert_eq!(PM_SETUID, 5); // callnr.h:18
        assert_eq!(PM_GETUID, 6); // callnr.h:19
        assert_eq!(PM_PTRACE, 8); // callnr.h:21
        assert_eq!(PM_SETGROUPS, 9); // callnr.h:22
        assert_eq!(PM_GETGROUPS, 10); // callnr.h:23
        assert_eq!(PM_SETGID, 12); // callnr.h:25
        assert_eq!(PM_GETGID, 13); // callnr.h:26
        assert_eq!(PM_SETSID, 15); // callnr.h:28
        assert_eq!(PM_GETPRIORITY, 26); // callnr.h:41
        assert_eq!(PM_SETPRIORITY, 27); // callnr.h:42
        assert_eq!(PM_SETEUID, 29); // callnr.h:44
        assert_eq!(PM_SETEGID, 30); // callnr.h:45
        assert_eq!(PM_GETSID, 32); // callnr.h:46
        assert_eq!(PM_REBOOT, 37); // callnr.h:52
    }

    /// 布局见证:三 wire 结构均 56 字节(union payload size);
    /// groups.ptr 在 LP64 落 offset 8(u64 对齐)。
    #[test]
    fn test_pm_credential_wire_layouts() {
        assert_eq!(size_of::<MessLcPmSetid>(), 56);
        assert_eq!(offset_of!(MessLcPmSetid, id), 0);
        assert_eq!(size_of::<MessLcPmGetsid>(), 56);
        assert_eq!(offset_of!(MessLcPmGetsid, pid), 0);
        assert_eq!(size_of::<MessLcPmGroups>(), 56);
        assert_eq!(offset_of!(MessLcPmGroups, num), 0);
        assert_eq!(offset_of!(MessLcPmGroups, ptr), 8);
    }
}

// ── E7 B 批:信号控制调用号与 wire 结构(callnr.h:20-24 + ipc.h)──

/// C: `PM_SIGACTION` — callnr.h:33.
pub const PM_SIGACTION: i32 = 20;
/// C: `PM_SIGSUSPEND` — callnr.h:34.
pub const PM_SIGSUSPEND: i32 = 21;
/// C: `PM_SIGPENDING` — callnr.h:35.
pub const PM_SIGPENDING: i32 = 22;
/// C: `PM_SIGPROCMASK` — callnr.h:36.
pub const PM_SIGPROCMASK: i32 = 23;
/// C: `PM_SIGRETURN` — callnr.h:37.
pub const PM_SIGRETURN: i32 = 24;
/// C: `PM_KILL` — callnr.h:24(KILL 与 SIGACTION 共用 `mess_lc_pm_sig`)。
pub const PM_KILL: i32 = 11;

/// SIGACTION/KILL 请求载荷(C: `mess_lc_pm_sig` — ipc.h:528-540)。
///
/// 字段族:`pid`(目标进程,0=自身)、`nr`(信号号)、`act`/`oact`
/// (`struct sigaction` 的用户态指针,signal.c:48-84)、`ret`
/// (sigreturn 恢复桩,signal.c:84)。LP64 与 i386 同布局(全 4 字节
/// 字段在前,指针 8 字节对齐自动落在 8/16/24)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSig {
    /// 目标进程 pid。C: `pid_t pid`。
    pub pid: i32,
    /// 信号号。C: `int nr`。
    pub nr: i32,
    /// `struct sigaction *` 用户态指针。C: `vir_bytes act`。
    pub act: u64,
    /// 旧 action 出参指针。C: `vir_bytes oact`。
    pub oact: u64,
    /// sigreturn 恢复桩。C: `vir_bytes ret`。
    pub ret: u64,
    /// Padding to 56 bytes (C: union payload size)。
    pub _padding: [u8; 24],
}

/// SIGPROCMASK/SIGSUSPEND 请求载荷(C: `mess_lc_pm_sigset` — ipc.h:543-549)。
///
/// `set` 是 C `sigset_t`(sigtypes.h:59-62:`u32 bits[4]`,16 字节)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSigset {
    /// SIG_BLOCK/UNBLOCK/SETMASK/INQUIRE。C: `int how`。
    pub how: i32,
    /// Padding to offset 8 (LP64 `vir_bytes` 对齐;i386 无此 pad)。
    pub _pad: [u8; 4],
    /// 恢复桩指针。C: `vir_bytes ctx`。
    pub ctx: u64,
    /// 信号掩码。C: `sigset_t set`(16 字节)。
    pub set: [u32; 4],
    /// Padding to 56 bytes (LP64: C i386 的 32 → 24)。
    pub _padding: [u8; 24],
}

#[cfg(test)]
mod sig_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C 绝对值 pin:B 批信号控制调用号(callnr.h:33-37 + :24)。
    #[test]
    fn test_pm_signal_call_numbers_match_c() {
        assert_eq!(PM_SIGACTION, 20); // callnr.h:33
        assert_eq!(PM_SIGSUSPEND, 21); // callnr.h:34
        assert_eq!(PM_SIGPENDING, 22); // callnr.h:35
        assert_eq!(PM_SIGPROCMASK, 23); // callnr.h:36
        assert_eq!(PM_SIGRETURN, 24); // callnr.h:37
        assert_eq!(PM_KILL, 11); // callnr.h:24
    }

    /// 布局见证:MessLcPmSig 56 字节(mess_lc_pm_sig — ipc.h:528-540),
    /// 指针域 LP64 落 8/16/24。
    #[test]
    fn test_mess_lc_pm_sig_layout() {
        assert_eq!(size_of::<MessLcPmSig>(), 56);
        assert_eq!(offset_of!(MessLcPmSig, pid), 0);
        assert_eq!(offset_of!(MessLcPmSig, nr), 4);
        assert_eq!(offset_of!(MessLcPmSig, act), 8);
        assert_eq!(offset_of!(MessLcPmSig, oact), 16);
        assert_eq!(offset_of!(MessLcPmSig, ret), 24);
        assert_eq!(offset_of!(MessLcPmSig, _padding), 32);
    }

    /// 布局见证:MessLcPmSigset 56 字节(how/ctx/set/padding)。
    #[test]
    fn test_mess_lc_pm_sigset_layout() {
        assert_eq!(size_of::<MessLcPmSigset>(), 56);
        assert_eq!(offset_of!(MessLcPmSigset, how), 0);
        assert_eq!(offset_of!(MessLcPmSigset, ctx), 8);
        assert_eq!(offset_of!(MessLcPmSigset, set), 16);
        assert_eq!(offset_of!(MessLcPmSigset, _padding), 32);
    }
}
