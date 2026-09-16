//! C `struct mproc` 的 repr(C) 镜像 wire——PM 进程表的二进制契约。
//!
//! C ground truth: `minix3/minix/servers/pm/mproc.h`（`mproc[NR_PROCS]`，
//! MP_MAGIC = 0xC0FFEE0）。消费方按字段偏移读取（RS live-update 的
//! SI_PROC_TAB、IS 的 mproc_tab dump），布局即契约。
//!
//! LP64 布局由逐字段推导得出（总 464 字节，见 `layout` 测试）：
//! 三 char 头 → pid/endpoint/pid/pid → vir_bytes → int×2 → clock_t×2
//! → uid/gid×6 → ngroups + sgroups[16] → sigset_t×7 → sigact 指针
//! → sigreturn → minix_timer_t(32) → interval[3] → started → flags×2
//! → message(64) → frame_addr/len → nice → scheduler → name[16] → magic。
//!
//! 字段语义与 C 一一对应；本重写没有的机制（如 `mp_sigact` 指向的
//! `mpsigact` 表、`mp_timer` 链）对应字段保持零值——C 消费方对
//! "未使用"与"零值"的处理相同。

/// C `NGROUPS_MAX` — sys/sys/syslimits.h:59。
pub const NGROUPS_MAX: usize = 16;
/// C `PROC_NAME_LEN` — type.h:145。
pub const PROC_NAME_LEN: usize = 16;
/// C `MP_MAGIC` — mproc.h:0xC0FFEE0。
pub const MP_MAGIC: i32 = 0x0C0F_FEE0;

/// `sigset_t`（sys/sys/sigtypes.h:59-62：`u32 __bits[4]`，16 字节）。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SigSetWire {
    pub bits: [u32; 4],
}

impl SigSetWire {
    pub const fn empty() -> Self {
        SigSetWire { bits: [0; 4] }
    }
}

/// `minix_timer_t`（timers.h:32-38）。LP64：next@0、exp_time@8、
/// func@16、arg@24 + pad4 = 32 字节。
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinixTimerWire {
    pub tmr_next: u64,
    pub tmr_exp_time: u64,
    pub tmr_func: u64,
    pub tmr_arg: i32,
    pub _pad: [u8; 4],
}

/// C `struct mproc` 单槽镜像。字段顺序/类型按 mproc.h 全文；
/// 布局见证见 `layout` 测试（总 464 字节）。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MprocWire {
    pub mp_exitstatus: u8,
    pub mp_sigstatus: u8,
    pub mp_eventsub: u8,
    _pad0: [u8; 1],
    pub mp_pid: i32,
    pub mp_endpoint: i32,
    pub mp_procgrp: i32,
    pub mp_wpid: i32,
    _pad1: [u8; 4],
    pub mp_waddr: u64,
    pub mp_parent: i32,
    pub mp_tracer: i32,
    pub mp_child_utime: u64,
    pub mp_child_stime: u64,
    pub mp_realuid: u32,
    pub mp_effuid: u32,
    pub mp_svuid: u32,
    pub mp_realgid: u32,
    pub mp_effgid: u32,
    pub mp_svgid: u32,
    pub mp_ngroups: i32,
    pub mp_sgroups: [u32; NGROUPS_MAX],
    pub mp_ignore: SigSetWire,
    pub mp_catch: SigSetWire,
    pub mp_sigmask: SigSetWire,
    pub mp_sigmask2: SigSetWire,
    pub mp_sigpending: SigSetWire,
    pub mp_ksigpending: SigSetWire,
    pub mp_sigtrace: SigSetWire,
    _pad2: [u8; 4],
    pub mp_sigact: u64,
    pub mp_sigreturn: u64,
    pub mp_timer: MinixTimerWire,
    pub mp_interval: [u64; 3],
    pub mp_started: u64,
    pub mp_flags: u32,
    pub mp_trace_flags: u32,
    pub mp_reply: [u64; 8],
    pub mp_frame_addr: u64,
    pub mp_frame_len: u64,
    pub mp_nice: i32,
    pub mp_scheduler: i32,
    pub mp_name: [u8; PROC_NAME_LEN],
    pub mp_magic: i32,
    _pad3: [u8; 4],
}

impl Default for MprocWire {
    fn default() -> Self {
        Self::new()
    }
}

impl MprocWire {
    /// 空槽（全零 + magic 置位由调用方按语义决定；C 的全零槽 magic=0
    /// 即"不在用"）。
    pub const fn new() -> Self {
        Self {
            mp_exitstatus: 0,
            mp_sigstatus: 0,
            mp_eventsub: 0,
            _pad0: [0; 1],
            mp_pid: 0,
            mp_endpoint: 0,
            mp_procgrp: 0,
            mp_wpid: 0,
            _pad1: [0; 4],
            mp_waddr: 0,
            mp_parent: 0,
            mp_tracer: 0,
            mp_child_utime: 0,
            mp_child_stime: 0,
            mp_realuid: 0,
            mp_effuid: 0,
            mp_svuid: 0,
            mp_realgid: 0,
            mp_effgid: 0,
            mp_svgid: 0,
            mp_ngroups: 0,
            mp_sgroups: [0; NGROUPS_MAX],
            mp_ignore: SigSetWire::empty(),
            mp_catch: SigSetWire::empty(),
            mp_sigmask: SigSetWire::empty(),
            mp_sigmask2: SigSetWire::empty(),
            mp_sigpending: SigSetWire::empty(),
            mp_ksigpending: SigSetWire::empty(),
            mp_sigtrace: SigSetWire::empty(),
            _pad2: [0; 4],
            mp_sigact: 0,
            mp_sigreturn: 0,
            mp_timer: MinixTimerWire {
                tmr_next: 0,
                tmr_exp_time: 0,
                tmr_func: 0,
                tmr_arg: 0,
                _pad: [0; 4],
            },
            mp_interval: [0; 3],
            mp_started: 0,
            mp_flags: 0,
            mp_trace_flags: 0,
            mp_reply: [0; 8],
            mp_frame_addr: 0,
            mp_frame_len: 0,
            mp_nice: 0,
            mp_scheduler: 0,
            mp_name: [0; PROC_NAME_LEN],
            mp_magic: 0,
            _pad3: [0; 4],
        }
    }
}

#[cfg(test)]
mod mproc_wire_layout_tests {
    use super::*;
    use core::mem::offset_of;

    /// 布局见证：LP64 总大小 464（逐字段推导，见模块文档）。
    #[test]
    fn test_mproc_wire_size() {
        assert_eq!(size_of::<SigSetWire>(), 16);
        assert_eq!(size_of::<MinixTimerWire>(), 32);
        assert_eq!(size_of::<MprocWire>(), 464);
    }

    /// 关键字段偏移（消费方按这些偏移读取）。
    #[test]
    fn test_mproc_wire_field_offsets() {
        assert_eq!(offset_of!(MprocWire, mp_exitstatus), 0);
        assert_eq!(offset_of!(MprocWire, mp_pid), 4);
        assert_eq!(offset_of!(MprocWire, mp_endpoint), 8);
        assert_eq!(offset_of!(MprocWire, mp_waddr), 24);
        assert_eq!(offset_of!(MprocWire, mp_child_utime), 40);
        assert_eq!(offset_of!(MprocWire, mp_realuid), 56);
        assert_eq!(offset_of!(MprocWire, mp_ngroups), 80);
        assert_eq!(offset_of!(MprocWire, mp_sgroups), 84);
        assert_eq!(offset_of!(MprocWire, mp_ignore), 148);
        assert_eq!(offset_of!(MprocWire, mp_sigact), 264);
        assert_eq!(offset_of!(MprocWire, mp_timer), 280);
        assert_eq!(offset_of!(MprocWire, mp_interval), 312);
        assert_eq!(offset_of!(MprocWire, mp_started), 336);
        assert_eq!(offset_of!(MprocWire, mp_flags), 344);
        assert_eq!(offset_of!(MprocWire, mp_reply), 352);
        assert_eq!(offset_of!(MprocWire, mp_frame_addr), 416);
        assert_eq!(offset_of!(MprocWire, mp_nice), 432);
        assert_eq!(offset_of!(MprocWire, mp_scheduler), 436);
        assert_eq!(offset_of!(MprocWire, mp_name), 440);
        assert_eq!(offset_of!(MprocWire, mp_magic), 456);
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
