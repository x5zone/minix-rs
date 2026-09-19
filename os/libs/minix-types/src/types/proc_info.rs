//! `GET_PROCTAB` process-table snapshot — the single Rust authority for
//! the wire layout of kernel `GET_PROCTAB`/`T_GETUSER` payloads.
//!
//! E-ISPROD: this struct was lifted verbatim from `os/kernel/src/misc.rs`
//! (the producer). The kernel emits `PROC_TABLE_LEN` of these per
//! `GET_PROCTAB` (chunked per-entry copies, do_getinfo.c:102-130), and
//! the Information Server's `proctab`/`procstack` dumps interpret the
//! bytes — previously the IS side kept a separate `KProcSnap` whose
//! field order/set/widths diverged (a wire-level mismatch, E-ISPROD's
//! founding incident). Consumer: `servers/is` (dmp_kernel render half);
//! the MIB server's `proc_tab` face (E-MIBPROD) resolves to this same
//! layout.
//!
//! Not a mirror of C `struct proc` (kernel/proc.h:22-137): it is the
//! minix-rs wire contract — a subset of used fields with 64-bit time
//! fields (C i386 `clock_t` is 32-bit; the LP64 rewrite widens, the
//! rs_start precedent).

// p_rts_flags 的位值与 C kernel/proc.h:142-166 逐位一致（内核的
// RtsFlagsBits 同源）；此前 SENDING/RECEIVING 误记 0x100/0x200 —— 真值是
// 0x04/0x08，错误值当时无消费者（IS dump_kernel.rs 用自己的正确副本），
// C-21 随 MIB 取表半把权威钉对。
/// C: `p_rts_flags` bit — SLOT_FREE（kernel/proc.h:142）。
pub const RTS_SLOT_FREE: u32 = 0x001;
/// C: `p_rts_flags` bit — PROC_STOP（kernel/proc.h:143）。
pub const RTS_PROC_STOP: u32 = 0x002;
/// C: `p_rts_flags` bit — SENDING（kernel/proc.h:144）。
pub const RTS_SENDING: u32 = 0x004;
/// C: `p_rts_flags` bit — RECEIVING（kernel/proc.h:145）。
pub const RTS_RECEIVING: u32 = 0x008;
/// C: `p_rts_flags` bit — SIGNALED（kernel/proc.h:146）。
pub const RTS_SIGNALED: u32 = 0x010;
/// C: `p_rts_flags` bit — SIG_PENDING（kernel/proc.h:147）。
pub const RTS_SIG_PENDING: u32 = 0x020;
/// C: `p_rts_flags` bit — P_STOP（kernel/proc.h:148，被跟踪停止）。
pub const RTS_P_STOP: u32 = 0x040;
/// C: `p_rts_flags` bit — NO_PRIV（kernel/proc.h:149）。
pub const RTS_NO_PRIV: u32 = 0x080;
/// C: `p_rts_flags` bit — NO_ENDPOINT（kernel/proc.h:150）。
pub const RTS_NO_ENDPOINT: u32 = 0x100;
/// C: `p_rts_flags` bit — VMINHIBIT（kernel/proc.h:151）。
pub const RTS_VMINHIBIT: u32 = 0x200;
/// C: `p_rts_flags` bit — PAGEFAULT（kernel/proc.h:152）。
pub const RTS_PAGEFAULT: u32 = 0x400;
/// C: `p_rts_flags` bit — VMREQUEST（kernel/proc.h:153）。
pub const RTS_VMREQUEST: u32 = 0x800;
/// C: `p_rts_flags` bit — VMREQTARGET（kernel/proc.h:154）。
pub const RTS_VMREQTARGET: u32 = 0x1000;
/// C: `p_rts_flags` bit — PREEMPTED（kernel/proc.h:155）。
pub const RTS_PREEMPTED: u32 = 0x4000;
/// C: `p_rts_flags` bit — NO_QUANTUM（kernel/proc.h:162）。
pub const RTS_NO_QUANTUM: u32 = 0x8000;
/// C: `p_rts_flags` bit — BOOTINHIBIT（kernel/proc.h:166）。
pub const RTS_BOOTINHIBIT: u32 = 0x10000;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ProcInfoStruct {
    /// C: `p_nr` — process number (slot index).
    pub p_nr: i32,
    /// C: `p_endpoint` — endpoint identifier.
    pub p_endpoint: i32,
    /// C: `p_rts_flags` — runtime status flags.
    pub p_rts_flags: u32,
    /// C: `p_misc_flags` — miscellaneous flags.
    pub p_misc_flags: u32,
    /// C: `p_priority` — current scheduling priority.
    pub p_priority: i8,
    /// C: `p_cpu` — CPU the process is running on.
    pub p_cpu: u32,
    /// C: `p_quantum_size_ms` — time quantum in milliseconds.
    pub p_quantum_size_ms: u32,
    /// C: `p_cpu_time_left` — CPU time remaining (ticks).
    pub p_cpu_time_left: u64,
    /// C: `p_user_time` — user time in ticks.
    pub p_user_time: u64,
    /// C: `p_sys_time` — system time in ticks.
    pub p_sys_time: u64,
    /// C: `p_cycles` — cycles consumed.
    pub p_cycles: u64,
    /// C: `p_pending` — pending signal bitmap.
    pub p_pending: u64,
    /// C: `p_getfrom_e` — endpoint to receive from.
    pub p_getfrom_e: i32,
    /// C: `p_sendto_e` — endpoint to send to.
    pub p_sendto_e: i32,
    /// C: `p_name` — process name (16 bytes including NUL).
    pub p_name: [u8; 16],
    /// C: `p_priv` (index) — privilege table index (replaces pointer).
    /// -1 = no privilege assigned (the manual `Default` keeps this, not
    /// zero — matching the kernel's unset-privilege sentinel).
    pub p_priv_id: i32,
    /// Padding to align the struct to 8 bytes.
    pub _padding: [u8; 4],
}

impl Default for ProcInfoStruct {
    /// Zeroed except `p_priv_id = -1` — the kernel's unset-privilege
    /// sentinel (kernel misc.rs `impl Default for ProcInfoStruct`).
    fn default() -> Self {
        Self {
            p_nr: 0,
            p_endpoint: 0,
            p_rts_flags: 0,
            p_misc_flags: 0,
            p_priority: 0,
            p_cpu: 0,
            p_quantum_size_ms: 0,
            p_cpu_time_left: 0,
            p_user_time: 0,
            p_sys_time: 0,
            p_cycles: 0,
            p_pending: 0,
            p_getfrom_e: 0,
            p_sendto_e: 0,
            p_name: [0; 16],
            p_priv_id: Self::PRIV_NONE,
            _padding: [0; 4],
        }
    }
}

impl ProcInfoStruct {
    /// Privilege sentinel (C: unset `p_priv`).
    pub const PRIV_NONE: i32 = -1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    /// E-ISPROD 布局见证:GET_PROCTAB 按元素大小做 chunked 拷贝,IS 侧按
    /// 同一布局解释字节——任一字段偏移/宽度漂移即跨进程数据错位。
    /// offsets: i32×2 → p_cpu 前 8 字节对齐 → u64×5 → 4 字节×3 → 16 字节名
    /// → i32 → 尾 4 字节,总 104。
    /// rts 位值与 C kernel/proc.h:142-166 逐位 pin（C-21：此前
    /// SENDING/RECEIVING 误记 0x100/0x200，与内核 RtsFlagsBits 分叉）。
    #[test]
    fn test_rts_bits_match_kernel() {
        assert_eq!(RTS_SLOT_FREE, 0x001);
        assert_eq!(RTS_PROC_STOP, 0x002);
        assert_eq!(RTS_SENDING, 0x004);
        assert_eq!(RTS_RECEIVING, 0x008);
        assert_eq!(RTS_SIGNALED, 0x010);
        assert_eq!(RTS_SIG_PENDING, 0x020);
        assert_eq!(RTS_P_STOP, 0x040);
        assert_eq!(RTS_NO_PRIV, 0x080);
        assert_eq!(RTS_NO_ENDPOINT, 0x100);
        assert_eq!(RTS_VMINHIBIT, 0x200);
        assert_eq!(RTS_PAGEFAULT, 0x400);
        assert_eq!(RTS_VMREQUEST, 0x800);
        assert_eq!(RTS_VMREQTARGET, 0x1000);
        assert_eq!(RTS_PREEMPTED, 0x4000);
        assert_eq!(RTS_NO_QUANTUM, 0x8000);
        assert_eq!(RTS_BOOTINHIBIT, 0x10000);
    }

    #[test]
    fn test_proc_info_layout() {
        assert_eq!(size_of::<ProcInfoStruct>(), 104);
        assert_eq!(offset_of!(ProcInfoStruct, p_nr), 0);
        assert_eq!(offset_of!(ProcInfoStruct, p_endpoint), 4);
        assert_eq!(offset_of!(ProcInfoStruct, p_rts_flags), 8);
        assert_eq!(offset_of!(ProcInfoStruct, p_misc_flags), 12);
        assert_eq!(offset_of!(ProcInfoStruct, p_priority), 16);
        assert_eq!(offset_of!(ProcInfoStruct, p_cpu), 20);
        assert_eq!(offset_of!(ProcInfoStruct, p_quantum_size_ms), 24);
        assert_eq!(offset_of!(ProcInfoStruct, p_cpu_time_left), 32);
        assert_eq!(offset_of!(ProcInfoStruct, p_user_time), 40);
        assert_eq!(offset_of!(ProcInfoStruct, p_sys_time), 48);
        assert_eq!(offset_of!(ProcInfoStruct, p_cycles), 56);
        assert_eq!(offset_of!(ProcInfoStruct, p_pending), 64);
        assert_eq!(offset_of!(ProcInfoStruct, p_getfrom_e), 72);
        assert_eq!(offset_of!(ProcInfoStruct, p_sendto_e), 76);
        assert_eq!(offset_of!(ProcInfoStruct, p_name), 80);
        assert_eq!(offset_of!(ProcInfoStruct, p_priv_id), 96);
    }

    /// Default 语义:p_priv_id = -1(无特权),其余零——kernel
    /// `from_kprocess`/`GET_PROC` 的 `unwrap_or_default()` 空槽依赖此形。
    #[test]
    fn test_proc_info_default_priv_none() {
        let d = ProcInfoStruct::default();
        assert_eq!(d.p_priv_id, ProcInfoStruct::PRIV_NONE);
        assert_eq!(d.p_rts_flags, 0);
        assert_eq!(d.p_nr, 0);
    }
}
