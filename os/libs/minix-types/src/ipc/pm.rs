//! PM service IPC message types.
//!
//! Defines the messages exchanged between PM and other services (Kernel, VM, VFS).

use crate::{EAGAIN, EINVAL, EIO, ENOMEM, ENOSYS, EPERM, ESRCH};

/// PM 错误类型（fork 协调错误的用户可见形态；`PmRequest`/`PmResponse`
/// 两个零使用枚举已按 E7 预裁决删除——PM wire 面由类型化
/// `Mess*` 结构承载，见下方 G 批及 A-E 批）。
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

// ── G 批：杂项 9 调用（04-stage-pm/todo.md §11.1.1 批次表）──
//
// C 真值：callnr.h 18/19/25/37/38/39/47 + ipc.h 对应
// `mess_lc_pm_*` / `mess_lc_svrctl` 结构。45/46（getepinfo/getprocnr）
// 已由 E9 PmApi 切片落地；37 常量已由 A 批锚定，本批补齐结构。

/// C: `PM_GETMCONTEXT (PM_BASE + 18)` — callnr.h:31.
pub const PM_GETMCONTEXT: i32 = 18;
/// C: `PM_SETMCONTEXT (PM_BASE + 19)` — callnr.h:32.
pub const PM_SETMCONTEXT: i32 = 19;
/// C: `PM_SYSUNAME (PM_BASE + 25)` — callnr.h:38（C 注记 obsolete，行为保留）。
pub const PM_SYSUNAME: i32 = 25;
/// C: `PM_SVRCTL (PM_BASE + 38)` — callnr.h:51.
pub const PM_SVRCTL: i32 = 38;
/// C: `PM_SPROF (PM_BASE + 39)` — callnr.h:52.
pub const PM_SPROF: i32 = 39;
// PM_GETSYSINFO（PM_BASE + 47，callnr.h:60）已有权威定义于
// `ipc/sysinfo.rs`（PM 表请求，E-MIBPROD/E-ISPROD 域）——不在此重复。

/// `mcontext_t` 指针载荷（GETMCONTEXT/SETMCONTEXT 共用）。
///
/// C: `mess_lc_pm_mcontext` — ipc.h:477-481。LP64：指针 4→8，
/// padding 52→48，总长保持 56。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmMcontext {
    /// `mcontext_t *`——用户态上下文存储地址。
    pub ctx: u64,
    _pad: [u8; 48],
}

impl Default for MessLcPmMcontext {
    fn default() -> Self {
        Self { ctx: 0, _pad: [0; 48] }
    }
}
impl MessLcPmMcontext {
    pub const fn new() -> Self {
        Self { ctx: 0, _pad: [0; 48] }
    }
}

/// REBOOT 载荷。
///
/// C: `mess_lc_pm_reboot` — ipc.h:503-507（int how + pad52，
/// LP64 同形 56）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmReboot {
    /// 重启方式（RBT_HALT/RBT_REBOOT/RBT_PANIC）。
    pub how: i32,
    _pad: [u8; 52],
}
impl MessLcPmReboot {
    pub const fn new(how: i32) -> Self {
        Self { how, _pad: [0; 52] }
    }
}

/// SYSUNAME 载荷。
///
/// C: `mess_lc_pm_sysuname` — ipc.h:565-571（req/field/len/value）。
/// LP64：len 对齐 8 → @8，value @16，padding 40→28，总长 56。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSysuname {
    /// 操作码（uname 命名空间 request）。
    pub req: i32,
    /// 字段选择。
    pub field: i32,
    /// 缓冲长度。
    pub len: u64,
    /// 用户缓冲指针。
    pub value: u64,
    _pad: [u8; 28],
}

/// SVRCTL 载荷（PM 服务器控制；与内核/RS 的 `mess_lc_svrctl` 同形）。
///
/// C: `mess_lc_svrctl` — ipc.h:603-608（unsigned long request +
/// vir_bytes arg）。LP64：request@0、arg@8、padding 48，总长 56。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSvrctl {
    /// SVRCTL 请求码。
    pub request: u64,
    /// 请求参数（用户指针或数值）。
    pub arg: u64,
    _pad: [u8; 40],
}

/// SPROF 载荷（PM 侧统计剖面控制）。
///
/// C: `mess_lc_pm_sprof` — ipc.h:550-558。LP64：三个 int + 三个
/// 8 字节域，padding 32→16，总长 56。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmSprof {
    /// 动作码（start/stop）。
    pub action: i32,
    /// 采样频率。
    pub freq: i32,
    /// 中断类型。
    pub intr_type: i32,
    /// 控制块指针。
    pub ctl_ptr: u64,
    /// 内存区指针。
    pub mem_ptr: u64,
    /// 内存区大小。
    pub mem_size: u64,
    _pad: [u8; 16],
}

#[cfg(test)]
mod tests {
    use super::*;


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

/// SIGPROCMASK/SIGPENDING 回复载荷(C: `mess_pm_lc_sigset` — 旧掩码/待决
/// 集由回复消息带回,signal.c:117/97)。LP64:set 16 字节 + padding 至 56。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessPmLcSigset {
    /// 旧掩码(SIGPROCMASK)或待决集(SIGPENDING)。C: `sigset_t set`。
    pub set: [u32; 4],
    /// Padding to 56 bytes (C: union payload size)。
    pub _padding: [u8; 40],
}

impl MessPmLcSigset {
    /// Zeroed form (tests and raw overlays).
    pub const fn zeroed() -> Self {
        Self { set: [0; 4], _padding: [0; 40] }
    }
}

/// 信号管理器把“已收到的信号”转发给目标服务的载荷。
///
/// C: `mess_pm_lsys_sigs_signal` — `ipc.h:1815-1819`（`int num` +
/// 52 字节 padding = 56）。发送方是信号管理器（PM 为服务代理：
/// `pm/signal.c:470-473`；RS 同理：`rs/main.c:699-701`），消息类型是
/// `SIGS_SIGNAL_RECEIVED`，接收方的 SEF 把这一个号码交给它的信号回调
/// （`sef_signal.c:115-128`）。它与通知载荷里的位图是信号请求的两种形状：
/// 位图形来自内核 `SYSTEM`，本形来自信号管理器。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessPmLsysSigsSignal {
    /// 被转发的信号号。C: `int num`。
    pub num: i32,
    /// Padding to 56 bytes (C: union payload size)。
    pub _padding: [u8; 52],
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

    /// 布局见证:MessPmLsysSigsSignal 56 字节,`num` 落在偏移 0
    ///(C `int num` 是结构体首个字段 — ipc.h:1815-1819)。
    #[test]
    fn test_mess_pm_lsys_sigs_signal_layout() {
        assert_eq!(size_of::<MessPmLsysSigsSignal>(), 56);
        assert_eq!(offset_of!(MessPmLsysSigsSignal, num), 0); // ipc.h:1816
        assert_eq!(offset_of!(MessPmLsysSigsSignal, _padding), 4); // ipc.h:1818
    }
}

// ── E7 C 批:时间调用号与 wire 结构(callnr.h:19/40-43 + ipc.h:574-583)──
//
// 六个时间调用(STIME 7/GETTIMEOFDAY 28/CLOCK_GETRES 33/CLOCK_GETTIME 34/
// CLOCK_SETTIME 35/GETRUSAGE 36)在 C 共用 `mess_lc_pm_time` 一臂,按
// `clk_id` 与 `now` 分派(time.c:31/55/79)。

/// C: `PM_STIME` — callnr.h:19(PM_BASE + 7)。
pub const PM_STIME: i32 = 7;
/// C: `PM_GETTIMEOFDAY` — callnr.h:43(PM_BASE + 28)。
pub const PM_GETTIMEOFDAY: i32 = 28;
/// C: `PM_CLOCK_GETRES` — callnr.h:48(PM_BASE + 33)。
pub const PM_CLOCK_GETRES: i32 = 33;
/// C: `PM_CLOCK_GETTIME` — callnr.h:49(PM_BASE + 34)。
pub const PM_CLOCK_GETTIME: i32 = 34;
/// C: `PM_CLOCK_SETTIME` — callnr.h:50(PM_BASE + 35)。
pub const PM_CLOCK_SETTIME: i32 = 35;
/// C: `PM_GETRUSAGE` — callnr.h:51(PM_BASE + 36)。
pub const PM_GETRUSAGE: i32 = 36;

/// 时间调用共用载荷(C: `mess_lc_pm_time` — ipc.h:574-583)。
///
/// LP64 布局:`time_t`/`long` 均为 8 字节,`_padding` 由 C i386 的 36
/// 缩至 32 保持 56 字节 payload(A-3 判例,rs_start 先例)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmTime {
    /// 秒。C: `time_t sec`。
    pub sec: u64,
    /// POSIX 时钟 id(CLOCK_REALTIME 等)。C: `clockid_t clk_id`。
    pub clk_id: i32,
    /// 0 = 渐变模式(adjtime),非 0 = 绝对设置。C: `int now`。
    pub now: i32,
    /// 纳秒部分。C: `long nsec`。
    pub nsec: i64,
    /// Padding to 56 bytes (LP64: C i386 的 36 → 32)。
    pub _padding: [u8; 32],
}

#[cfg(test)]
mod time_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C 绝对值 pin:C 批时间调用号(callnr.h:19/43/48-51)。
    #[test]
    fn test_pm_time_call_numbers_match_c() {
        assert_eq!(PM_STIME, 7); // callnr.h:19
        assert_eq!(PM_GETTIMEOFDAY, 28); // callnr.h:43
        assert_eq!(PM_CLOCK_GETRES, 33); // callnr.h:48
        assert_eq!(PM_CLOCK_GETTIME, 34); // callnr.h:49
        assert_eq!(PM_CLOCK_SETTIME, 35); // callnr.h:50
        assert_eq!(PM_GETRUSAGE, 36); // callnr.h:51
    }

    /// 布局见证:MessLcPmTime 56 字节;sec u64@0、clk_id@8、now@12、
    /// nsec@16(LP64 time_t/long 均 8 字节,padding 36→32 保持 payload)。
    #[test]
    fn test_mess_lc_pm_time_layout() {
        assert_eq!(size_of::<MessLcPmTime>(), 56);
        assert_eq!(offset_of!(MessLcPmTime, sec), 0);
        assert_eq!(offset_of!(MessLcPmTime, clk_id), 8);
        assert_eq!(offset_of!(MessLcPmTime, now), 12);
        assert_eq!(offset_of!(MessLcPmTime, nsec), 16);
    }
}

// ── E7 D/E 批:itimer 与 exec 调用号与 wire 结构 ──

/// C: `PM_ITIMER` — callnr.h:30(PM_BASE + 17)。
pub const PM_ITIMER: i32 = 17;
/// C: `PM_EXEC` — callnr.h:27(PM_BASE + 14)。
pub const PM_EXEC: i32 = 14;
/// C: `PM_EXEC_NEW` — callnr.h:56(PM_BASE + 43)。
pub const PM_EXEC_NEW: i32 = 43;
/// C: `PM_EXEC_RESTART` — callnr.h:57(PM_BASE + 44)。
pub const PM_EXEC_RESTART: i32 = 44;

// ── E9 PmApi 分域:RS 服务进程管理调用号与 wire ──

/// `PM_SRV_FORK` — 为系统服务派生子进程(callnr.h:54,PM_BASE + 41)。
/// RS 自升级链的入口(C manager.c:576 `srv_fork`)。
pub const PM_SRV_FORK: i32 = 41;
/// `PM_GETEPINFO` — 按端点取进程凭证(callnr.h:58,PM_BASE + 45)。
pub const PM_GETEPINFO: i32 = 45;
/// `PM_GETPROCNR` — 按 pid 反查端点(callnr.h:59,PM_BASE + 46)。
pub const PM_GETPROCNR: i32 = 46;

/// getepinfo 请求载荷(C: `mess_lsys_pm_getepinfo` — ipc.h:1398-1406)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MessLsysPmGetepinfo {
    /// 目标进程端点。C: `endpoint_t endpt`。
    pub endpt: i32,
    /// LP64:vir_bytes 拓宽后的对齐垫。
    pub _pad: u32,
    /// groups 缓冲指针(0 = 不取组表)。C: `vir_bytes groups`。
    pub groups: u64,
    /// 缓冲容量(组数)。C: `int ngroups`。
    pub ngroups: i32,
    /// LP64 对齐垫。
    pub _pad2: u32,
    /// LP64:域合计 24,padding 由 44 缩至 32,总 56。
    pub _padding: [u8; 32],
}

/// getepinfo 应答载荷(PM → 调用方。C: `mess_pm_lsys_getepinfo` —
/// ipc.h:1788-1798,全 4 字节域,padding 36,总 56)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessPmLsysGetepinfo {
    /// 真实 uid。C: `uid_t uid`。
    pub uid: i32,
    /// 有效 uid。C: `uid_t euid`。
    pub euid: i32,
    /// 真实 gid。C: `gid_t gid`。
    pub gid: i32,
    /// 有效 gid。C: `gid_t egid`。
    pub egid: i32,
    /// 组数(groups 缓冲非空时有效)。C: `int ngroups`。
    pub ngroups: i32,
    /// Padding to 56 bytes。
    pub _padding: [u8; 36],
}

/// getprocnr 请求载荷(C: `mess_lsys_pm_getprocnr` — ipc.h:1407-1412)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLsysPmGetprocnr {
    /// 进程 id。C: `pid_t pid`。
    pub pid: i32,
    /// Padding to 56 bytes。
    pub _padding: [u8; 52],
}

/// getprocnr 应答载荷(PM → 调用方。C: `mess_pm_lsys_getprocnr` —
/// ipc.h:1800-1805)。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessPmLsysGetprocnr {
    /// 进程端点。C: `endpoint_t endpt`。
    pub endpt: i32,
    /// Padding to 56 bytes。
    pub _padding: [u8; 52],
}

/// itimer 请求载荷(C: `mess_lc_pm_itimer` — ipc.h:468-474)。
///
/// `which`:ITIMER_REAL/ITIMER_VIRTUAL/ITIMER_PROF;`value`/`ovalue`
/// 为 `struct itimerval` 的用户态指针。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmItimer {
    /// 定时器类型。C: `int which`。
    pub which: i32,
    /// 新值指针(`struct itimerval *`)。C: `vir_bytes value`。
    pub value: u64,
    /// 旧值出参指针。C: `vir_bytes ovalue`。
    pub ovalue: u64,
    /// LP64:value/ovalue 拓为 8 字节(域合计 24),padding 由 44 缩至 32,总 56。
    pub _padding: [u8; 32],
}

/// exec 请求载荷(C: `mess_lc_pm_exec` — ipc.h:435-443:name/namelen/
/// frame/framelen/ps_str 五域 + padding[36])。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessLcPmExec {
    /// 新映像名指针。C: `vir_bytes name`。
    pub name: u64,
    /// 名字长度。C: `size_t namelen`。
    pub namelen: u64,
    /// 初始栈帧指针。C: `vir_bytes frame`。
    pub frame: u64,
    /// 栈帧长度。C: `size_t framelen`。
    pub framelen: u64,
    /// ps_strings 指针。C: `vir_bytes ps_str`。
    pub ps_str: u64,
    /// LP64:五个 4 字节域拓为 8 字节(40),padding 由 36 缩至 16,总 56。
    pub _padding: [u8; 16],
}

/// exec-restart 应答载荷(RS→PM)。C: `mess_rs_pm_exec_restart` —
/// ipc.h:1869-1876:endpt/result/pc/ps_str + padding[40]。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessRsPmExecRestart {
    /// 目标进程端点。C: `endpoint_t endpt`。
    pub endpt: i32,
    /// exec 结果。C: `int result`。
    pub result: i32,
    /// 新入口 PC。C: `vir_bytes pc`。
    pub pc: u64,
    /// ps_strings 指针。C: `vir_bytes ps_str`。
    pub ps_str: u64,
    /// LP64:pc/ps_str 拓为 8 字节(域合计 24),padding 由 40 缩至 32,总 56。
    pub _padding: [u8; 32],
}

#[cfg(test)]
mod itimer_exec_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// C 绝对值 pin:D/E 批调用号。
    #[test]
    fn test_itimer_exec_call_numbers_match_c() {
        assert_eq!(PM_ITIMER, 17); // callnr.h:30
        assert_eq!(PM_EXEC, 14); // callnr.h:27
        assert_eq!(PM_EXEC_NEW, 43); // callnr.h:56
        assert_eq!(PM_EXEC_RESTART, 44); // callnr.h:57
    }

    /// 布局见证:MessLcPmItimer 56 字节(which@0/value@8/ovalue@16)。
    #[test]
    fn test_mess_lc_pm_itimer_layout() {
        assert_eq!(size_of::<MessLcPmItimer>(), 56);
        assert_eq!(offset_of!(MessLcPmItimer, which), 0);
        assert_eq!(offset_of!(MessLcPmItimer, value), 8);
        assert_eq!(offset_of!(MessLcPmItimer, ovalue), 16);
    }

    /// 布局见证:MessLcPmExec 56 字节(name/namelen/frame/framelen/ps_str)。
    #[test]
    fn test_mess_lc_pm_exec_layout() {
        assert_eq!(size_of::<MessLcPmExec>(), 56);
        assert_eq!(offset_of!(MessLcPmExec, name), 0);
        assert_eq!(offset_of!(MessLcPmExec, namelen), 8);
        assert_eq!(offset_of!(MessLcPmExec, frame), 16);
        assert_eq!(offset_of!(MessLcPmExec, framelen), 24);
        assert_eq!(offset_of!(MessLcPmExec, ps_str), 32);
    }

    /// 布局见证:MessRsPmExecRestart 56 字节(endpt/result/pc/ps_str)。
    #[test]
    fn test_mess_rs_pm_exec_restart_layout() {
        assert_eq!(size_of::<MessRsPmExecRestart>(), 56);
        assert_eq!(offset_of!(MessRsPmExecRestart, endpt), 0);
        assert_eq!(offset_of!(MessRsPmExecRestart, result), 4);
        assert_eq!(offset_of!(MessRsPmExecRestart, pc), 8);
        assert_eq!(offset_of!(MessRsPmExecRestart, ps_str), 16);
    }
}

#[cfg(test)]
mod pm_service_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// 绝对值 pin:E9 PmApi 三调用号(callnr.h:54/:58/:59)。
    #[test]
    fn test_pm_service_call_numbers() {
        assert_eq!(PM_SRV_FORK, 41);
        assert_eq!(PM_GETEPINFO, 45);
        assert_eq!(PM_GETPROCNR, 46);
    }

    /// 布局见证 ×4:getepinfo 请求/应答与 getprocnr 请求/应答
    /// (LP64:getepinfo 请求 groups@8 对齐垫,总 56)。
    #[test]
    fn test_pm_service_wire_layouts() {
        assert_eq!(size_of::<MessLsysPmGetepinfo>(), 56);
        assert_eq!(offset_of!(MessLsysPmGetepinfo, endpt), 0);
        assert_eq!(offset_of!(MessLsysPmGetepinfo, groups), 8);
        assert_eq!(offset_of!(MessLsysPmGetepinfo, ngroups), 16);

        assert_eq!(size_of::<MessPmLsysGetepinfo>(), 56);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, uid), 0);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, euid), 4);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, gid), 8);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, egid), 12);
        assert_eq!(offset_of!(MessPmLsysGetepinfo, ngroups), 16);

        assert_eq!(size_of::<MessLsysPmGetprocnr>(), 56);
        assert_eq!(offset_of!(MessLsysPmGetprocnr, pid), 0);
        assert_eq!(size_of::<MessPmLsysGetprocnr>(), 56);
        assert_eq!(offset_of!(MessPmLsysGetprocnr, endpt), 0);
    }
}

#[cfg(test)]
mod g_batch_wire_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// G 批调用号绝对值 pin（C callnr.h 31/32/38/51/52/60，
    /// PM_BASE = 0）。
    #[test]
    fn test_g_batch_call_numbers_match_c() {
        assert_eq!(PM_GETMCONTEXT, 18); // callnr.h:31
        assert_eq!(PM_SETMCONTEXT, 19); // callnr.h:32
        assert_eq!(PM_SYSUNAME, 25); // callnr.h:38
        assert_eq!(PM_SVRCTL, 38); // callnr.h:51
        assert_eq!(PM_SPROF, 39); // callnr.h:52
    }

    /// 布局见证：五个 G 批结构均为 56 字节（PM wire 总长契约）。
    #[test]
    fn test_g_batch_struct_sizes() {
        assert_eq!(size_of::<MessLcPmMcontext>(), 56);
        assert_eq!(size_of::<MessLcPmReboot>(), 56);
        assert_eq!(size_of::<MessLcPmSysuname>(), 56);
        assert_eq!(size_of::<MessLcPmSvrctl>(), 56);
        assert_eq!(size_of::<MessLcPmSprof>(), 56);
    }

    /// 字段偏移见证（LP64 判例：Mcontext/Reboot 单域；Sysuname 的
    /// len 对齐 8 → @8、value @16；Svrctl 双 8 字节域；Sprof 三个
    /// int + 对齐垫 + 三个 8 字节域）。
    #[test]
    fn test_g_batch_field_offsets() {
        assert_eq!(offset_of!(MessLcPmMcontext, ctx), 0);
        assert_eq!(offset_of!(MessLcPmReboot, how), 0);
        assert_eq!(offset_of!(MessLcPmSysuname, req), 0);
        assert_eq!(offset_of!(MessLcPmSysuname, field), 4);
        assert_eq!(offset_of!(MessLcPmSysuname, len), 8);
        assert_eq!(offset_of!(MessLcPmSysuname, value), 16);
        assert_eq!(offset_of!(MessLcPmSvrctl, request), 0);
        assert_eq!(offset_of!(MessLcPmSvrctl, arg), 8);
        assert_eq!(offset_of!(MessLcPmSprof, action), 0);
        assert_eq!(offset_of!(MessLcPmSprof, freq), 4);
        assert_eq!(offset_of!(MessLcPmSprof, intr_type), 8);
        assert_eq!(offset_of!(MessLcPmSprof, ctl_ptr), 16);
        assert_eq!(offset_of!(MessLcPmSprof, mem_ptr), 24);
        assert_eq!(offset_of!(MessLcPmSprof, mem_size), 32);
    }
}