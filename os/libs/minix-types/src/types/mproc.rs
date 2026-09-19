//! PM 进程表快照 —— `SI_PROC_TAB` 的 wire 权威（`[ARCH: A-4]` 单一权威）。
//!
//! C ground truth: `minix3/minix/servers/pm/mproc.h`（`mproc[NR_PROCS]`，
//! `MP_MAGIC = 0xC0FFEE0`）。快照只带**使用字段子集**（C 声明序，
//! `#[repr(C)]`）——与 `FProcSnap`（VFS 表）、`ProcInfoStruct`（内核表）
//! 同一裁定：跨 wire 无意义的指针/内嵌消息（`mp_reply[64]`、
//! `mp_sigact` 指向的 `mpsigact` 表）与无人读的字段不进快照；信号位图
//! 只带第一字（`__bits[0]`），因为 dump 只打这一字。
//!
//! 生产者（`os/servers/pm/src/mproc/wire.rs`）按本结构逐槽序列化，消费方
//! （IS `dump_pm`、MIB `proc/tables` 的取表半）按名读取——布局即契约。
//! C 用逐行 `MP_MAGIC` 做运行时漂移校验（"Perhaps recompile IS?"），
//! Rust 侧同一目的由共享类型 + 布局测试在编译期结构性拦住。

/// C `NGROUPS_MAX` — sys/sys/syslimits.h:59。
pub const NGROUPS_MAX: usize = 16;
/// C `PROC_NAME_LEN` — type.h:145。
pub const PROC_NAME_LEN: usize = 16;
/// C `MP_MAGIC` — mproc.h:0xC0FFEE0。
pub const MP_MAGIC: i32 = 0x0C0F_FEE0;

/// PM 进程表行快照（使用字段子集，C 声明序）。
///
/// 消费者按名读字段：IS 的 `mproc_dmp`/`sigaction_dmp` 打全部 16 项，
/// MIB 的取表半按需扩展（扩展点在**本结构**——单一权威）。
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MProcSnap {
    /// C: `mp_pid` (mproc.h:28)。
    pub mp_pid: i32,
    /// C: `mp_parent`，表索引 (mproc.h:33)。
    pub mp_parent: i32,
    /// C: `mp_tracer` (mproc.h:34)。
    pub mp_tracer: i32,
    /// C: `mp_name[PROC_NAME_LEN]` (mproc.h:80)。
    pub mp_name: [u8; PROC_NAME_LEN],
    /// C: `mp_procgrp` (mproc.h:30)。
    pub mp_procgrp: i32,
    /// C: `mp_realuid` (mproc.h:41)。
    pub mp_realuid: u32,
    /// C: `mp_effuid` (mproc.h:42)。
    pub mp_effuid: u32,
    /// C: `mp_realgid` (mproc.h:44)。
    pub mp_realgid: u32,
    /// C: `mp_effgid` (mproc.h:45)。
    pub mp_effgid: u32,
    /// C: `mp_nice` (mproc.h:75)。
    pub mp_nice: i32,
    /// C: `mp_flags` (mproc.h:66)。
    pub mp_flags: u32,
    /// C: `mp_ignore.__bits[0]` (mproc.h:53；dump 只打第一字)。
    pub mp_ignore0: u32,
    /// C: `mp_catch.__bits[0]` (mproc.h:54)。
    pub mp_catch0: u32,
    /// C: `mp_sigmask.__bits[0]` (mproc.h:55)。
    pub mp_sigmask0: u32,
    /// C: `mp_sigpending.__bits[0]` (mproc.h:57)。
    pub mp_sigpending0: u32,
    /// C: `mp_timer.tmr_exp_time` (mproc.h:62, timers.h:35)。
    pub mp_timer_exp: u32,
    /// C: `mp_started`（mproc.h；fork 时置内核 uptime）。
    ///
    /// `ps` 的 swtime 列（MIB `fill_lwp_common` 的 `uptime - mp_started`）
    /// 与 `dump_pm` 的运行时长都读它——C 快照本就携带，C-21 随 MIB 取表半
    /// 补进 wire。追加在**尾部**：既有 15 槽 + name 的偏移不变。
    pub mp_started: u64,
    /// C: `mp_svuid`（mproc.h；setuid 语义的保存位）。
    pub mp_svuid: u32,
    /// C: `mp_svgid`。
    pub mp_svgid: u32,
    /// C: `mp_child_utime`（mproc.h；已收养子进程的用户态时钟）。
    pub mp_child_utime: u64,
    /// C: `mp_child_stime`。
    pub mp_child_stime: u64,
    /// C: `mp_ngroups`（mproc.h；补充组个数，≤ NGROUPS_MAX）。
    pub mp_ngroups: u32,
    /// C: `mp_sgroups[NGROUPS_MAX]`（补充组表——`ps` 的 GROUPS 列与
    /// KERN_PROC2 的 `p_groups` 消费；此前"无人读"的裁定随 MIB 取表半
    /// 失效，C-22 扩进 wire）。
    pub mp_sgroups: [u32; NGROUPS_MAX],
    /// C: `mp_endpoint`（mproc.h；KERN_PROC_ARGS 按它读目标进程的
    /// ps_strings 与参数页，proc.c:963-965）。
    pub mp_endpoint: i32,
    /// C: `mp_frame_addr`（mproc.h；exec 时置的参数帧基址）。
    pub mp_frame_addr: u64,
    /// C: `mp_frame_len`（mproc.h；帧长度——大小估算的上限之一，
    /// proc.c:981）。
    pub mp_frame_len: u64,
}

/// C `mp_flags` 的位值（mproc.h:86-104）——wire 权威（C-21）。
///
/// 生产者（PM `mproc/wire.rs` 的 `flags_for`）与消费方（MIB 取表半的
/// `get_lwp_stat` 状态机、IS `dump_pm`）都从这里取值；此前 PM crate 本地
/// 持有一份同值表，上收后由布局/取值测试钉住同一来源。
pub mod mp_flags {
    /// mproc.h:86 —— 槽在用（判据位）。
    pub const IN_USE: u32 = 0x00001;
    /// mproc.h:87 —— wait4 挂起。
    pub const WAITING: u32 = 0x00002;
    /// mproc.h:88 —— 等父 wait4。
    pub const ZOMBIE: u32 = 0x00004;
    /// mproc.h:89 —— 被停止。
    pub const PROC_STOPPED: u32 = 0x00008;
    /// mproc.h:90 —— 闹钟在走。
    pub const ALARM_ON: u32 = 0x00010;
    /// mproc.h:91 —— exit 走完。
    pub const EXITING: u32 = 0x00020;
    /// mproc.h:92 —— 已通知父进程。
    pub const TOLD_PARENT: u32 = 0x00040;
    /// mproc.h:93 —— 跟踪停止。
    pub const TRACE_STOPPED: u32 = 0x00080;
    /// mproc.h:94 —— sigsuspend 挂起。
    pub const SIGSUSPENDED: u32 = 0x00100;
    /// mproc.h:95 —— VFS 调用挂起。
    pub const VFS_CALL: u32 = 0x00400;
    /// mproc.h:96 —— 新父进程。
    pub const NEW_PARENT: u32 = 0x00800;
    /// mproc.h:97 —— pause 被信号打断。
    pub const UNPAUSED: u32 = 0x01000;
    /// mproc.h:98 —— 内核权限进程。
    pub const PRIV_PROC: u32 = 0x02000;
    /// mproc.h:99 —— exec 走了一半。
    pub const PARTIAL_EXEC: u32 = 0x04000;
    /// mproc.h:100 —— 跟踪退出挂起。
    pub const TRACE_EXIT: u32 = 0x08000;
    /// mproc.h:101 —— 等跟踪者 wait4。
    pub const TRACE_ZOMBIE: u32 = 0x10000;
    /// mproc.h:102 —— 延迟调用。
    pub const DELAY_CALL: u32 = 0x20000;
    /// mproc.h:103 —— 被 taint。
    pub const TAINTED: u32 = 0x40000;
    /// mproc.h:104 —— 事件调用挂起。
    pub const EVENT_CALL: u32 = 0x80000;
}

#[cfg(test)]
mod mproc_snap_layout_tests {
    use super::*;
    use core::mem::offset_of;

    /// 布局见证：88 字节（C-21 的 mp_started）+ C-22 尾段
    /// （svuid/svgid/child 双时刻/ngroups/sgroups[16]/endpoint/frame
    /// 三域）= 200 字节（endpoint 尾随的 4 字节恰好被 frame_addr 的
    /// 8 对齐吸收，无结构尾垫）。
    #[test]
    fn test_mproc_snap_size() {
        assert_eq!(size_of::<MProcSnap>(), 200);
    }

    /// 字段偏移（生产者逐槽写、消费方按名读的同一份契约）。
    #[test]
    fn test_mproc_snap_field_offsets() {
        assert_eq!(offset_of!(MProcSnap, mp_pid), 0);
        assert_eq!(offset_of!(MProcSnap, mp_parent), 4);
        assert_eq!(offset_of!(MProcSnap, mp_tracer), 8);
        assert_eq!(offset_of!(MProcSnap, mp_name), 12);
        assert_eq!(offset_of!(MProcSnap, mp_procgrp), 28);
        assert_eq!(offset_of!(MProcSnap, mp_realuid), 32);
        assert_eq!(offset_of!(MProcSnap, mp_effuid), 36);
        assert_eq!(offset_of!(MProcSnap, mp_realgid), 40);
        assert_eq!(offset_of!(MProcSnap, mp_effgid), 44);
        assert_eq!(offset_of!(MProcSnap, mp_nice), 48);
        assert_eq!(offset_of!(MProcSnap, mp_flags), 52);
        assert_eq!(offset_of!(MProcSnap, mp_ignore0), 56);
        assert_eq!(offset_of!(MProcSnap, mp_catch0), 60);
        assert_eq!(offset_of!(MProcSnap, mp_sigmask0), 64);
        assert_eq!(offset_of!(MProcSnap, mp_sigpending0), 68);
        assert_eq!(offset_of!(MProcSnap, mp_timer_exp), 72);
        assert_eq!(offset_of!(MProcSnap, mp_started), 80);
        assert_eq!(offset_of!(MProcSnap, mp_svuid), 88);
        assert_eq!(offset_of!(MProcSnap, mp_svgid), 92);
        assert_eq!(offset_of!(MProcSnap, mp_child_utime), 96);
        assert_eq!(offset_of!(MProcSnap, mp_child_stime), 104);
        assert_eq!(offset_of!(MProcSnap, mp_ngroups), 112);
        assert_eq!(offset_of!(MProcSnap, mp_sgroups), 116);
        assert_eq!(offset_of!(MProcSnap, mp_endpoint), 180);
        assert_eq!(offset_of!(MProcSnap, mp_frame_addr), 184);
        assert_eq!(offset_of!(MProcSnap, mp_frame_len), 192);
    }

    /// mp_flags 位值（wire 权威的钉值；与 C mproc.h:86-104 逐位对照）。
    #[test]
    fn test_mp_flags_authority() {
        use super::mp_flags::*;
        assert_eq!(IN_USE, 0x00001);
        assert_eq!(WAITING, 0x00002);
        assert_eq!(ZOMBIE, 0x00004);
        assert_eq!(PROC_STOPPED, 0x00008);
        assert_eq!(ALARM_ON, 0x00010);
        assert_eq!(EXITING, 0x00020);
        assert_eq!(TOLD_PARENT, 0x00040);
        assert_eq!(TRACE_STOPPED, 0x00080);
        assert_eq!(SIGSUSPENDED, 0x00100);
        assert_eq!(VFS_CALL, 0x00400);
        assert_eq!(NEW_PARENT, 0x00800);
        assert_eq!(UNPAUSED, 0x01000);
        assert_eq!(PRIV_PROC, 0x02000);
        assert_eq!(PARTIAL_EXEC, 0x04000);
        assert_eq!(TRACE_EXIT, 0x08000);
        assert_eq!(TRACE_ZOMBIE, 0x10000);
        assert_eq!(DELAY_CALL, 0x20000);
        assert_eq!(TAINTED, 0x40000);
        assert_eq!(EVENT_CALL, 0x80000);
    }

    /// MP_MAGIC 与 flags 位值 pin（mproc.h 尾部宏）。
    #[test]
    fn test_mp_magic_and_flag_bits() {
        assert_eq!(MP_MAGIC, 0x0C0F_FEE0);
        let (in_use, waiting, zombie) = (0x1, 0x2, 0x4);
        let (vfs_call, priv_proc) = (0x400, 0x2000);
        let (tainted, event_call) = (0x40000, 0x80000);
        assert_eq!(in_use | waiting | zombie, 0x7);
        assert_eq!(priv_proc, 0x2000);
        assert_eq!(tainted | event_call, 0xC0000);
    }
}
