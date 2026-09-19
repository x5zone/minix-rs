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
}

#[cfg(test)]
mod mproc_snap_layout_tests {
    use super::*;
    use core::mem::offset_of;

    /// 布局见证：15 个 4 字节槽 + name[16] = 76 字节，全 4 字节对齐。
    #[test]
    fn test_mproc_snap_size() {
        assert_eq!(size_of::<MProcSnap>(), 76);
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
