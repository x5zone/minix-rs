//! PM process table snapshot — the `SI_PROC_TAB` wire authority (`[ARCH: A-4]`
//! single authority).
//!
//! C ground truth: `minix3/minix/servers/pm/mproc.h` (`mproc[NR_PROCS]`,
//! `MP_MAGIC = 0xC0FFEE0`). The snapshot carries only the **used-field
//! subset** (C declaration order, `#[repr(C)]`) — the same ruling as
//! `FProcSnap` (VFS table) and `ProcInfoStruct` (kernel table): pointers and
//! embedded messages that mean nothing across the wire (`mp_reply[64]`, the
//! `mpsigact` table `mp_sigact` points at) and fields nobody reads stay out
//! of the snapshot; the signal bitmap carries only its first word
//! (`__bits[0]`), because the dump prints only that word.
//!
//! The producer (`os/servers/pm/src/mproc/wire.rs`) serializes slot by slot
//! according to this structure, and consumers (IS `dump_pm`, the table-pull
//! half of MIB `proc/tables`) read it by name — the layout is the contract.
//! C uses a per-row `MP_MAGIC` for runtime drift checking ("Perhaps recompile
//! IS?"); the Rust side achieves the same end structurally at compile time
//! through the shared type plus layout tests.

/// C `NGROUPS_MAX` — sys/sys/syslimits.h:59.
pub const NGROUPS_MAX: usize = 16;
/// C `PROC_NAME_LEN` — type.h:145.
pub const PROC_NAME_LEN: usize = 16;
/// C `MP_MAGIC` — mproc.h:0xC0FFEE0.
pub const MP_MAGIC: i32 = 0x0C0F_FEE0;

/// PM process-table row snapshot (used-field subset, C declaration order).
///
/// Consumers read fields by name: IS's `mproc_dmp`/`sigaction_dmp` print all
/// 16 entries, and the MIB table-pull half extends as needed (the extension
/// point is **this structure** — the single authority).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MProcSnap {
    /// C: `mp_pid` (mproc.h:28).
    pub mp_pid: i32,
    /// C: `mp_parent`, a table index (mproc.h:33).
    pub mp_parent: i32,
    /// C: `mp_tracer` (mproc.h:34).
    pub mp_tracer: i32,
    /// C: `mp_name[PROC_NAME_LEN]` (mproc.h:80).
    pub mp_name: [u8; PROC_NAME_LEN],
    /// C: `mp_procgrp` (mproc.h:30).
    pub mp_procgrp: i32,
    /// C: `mp_realuid` (mproc.h:41).
    pub mp_realuid: u32,
    /// C: `mp_effuid` (mproc.h:42).
    pub mp_effuid: u32,
    /// C: `mp_realgid` (mproc.h:44).
    pub mp_realgid: u32,
    /// C: `mp_effgid` (mproc.h:45).
    pub mp_effgid: u32,
    /// C: `mp_nice` (mproc.h:75).
    pub mp_nice: i32,
    /// C: `mp_flags` (mproc.h:66).
    pub mp_flags: u32,
    /// C: `mp_ignore.__bits[0]` (mproc.h:53; the dump prints only the first
    /// word).
    pub mp_ignore0: u32,
    /// C: `mp_catch.__bits[0]` (mproc.h:54).
    pub mp_catch0: u32,
    /// C: `mp_sigmask.__bits[0]` (mproc.h:55).
    pub mp_sigmask0: u32,
    /// C: `mp_sigpending.__bits[0]` (mproc.h:57).
    pub mp_sigpending0: u32,
    /// C: `mp_timer.tmr_exp_time` (mproc.h:62, timers.h:35).
    pub mp_timer_exp: u32,
    /// C: `mp_started` (mproc.h; set to the kernel uptime at fork).
    ///
    /// `ps`'s swtime column (MIB `fill_lwp_common`'s `uptime - mp_started`)
    /// and `dump_pm`'s running time both read it — the C snapshot already
    /// carries it, and the MIB table-pull half added it to the wire. Appended
    /// at the **tail**: the offsets of the existing 15 slots plus name are
    /// unchanged.
    pub mp_started: u64,
    /// C: `mp_svuid` (mproc.h; the saved slot for setuid semantics).
    pub mp_svuid: u32,
    /// C: `mp_svgid`.
    pub mp_svgid: u32,
    /// C: `mp_child_utime` (mproc.h; user time of reaped children).
    pub mp_child_utime: u64,
    /// C: `mp_child_stime`.
    pub mp_child_stime: u64,
    /// C: `mp_ngroups` (mproc.h; number of supplementary groups, at most
    /// NGROUPS_MAX).
    pub mp_ngroups: u32,
    /// C: `mp_sgroups[NGROUPS_MAX]` (the supplementary-group table — consumed
    /// by `ps`'s GROUPS column and KERN_PROC2's `p_groups`; the earlier
    /// "nobody reads it" ruling lapsed with the MIB table-pull half, which
    /// extended the wire).
    pub mp_sgroups: [u32; NGROUPS_MAX],
    /// C: `mp_endpoint` (mproc.h; KERN_PROC_ARGS uses it to read the target
    /// process's ps_strings and argument page, proc.c:963-965).
    pub mp_endpoint: i32,
    /// C: `mp_frame_addr` (mproc.h; the argument-frame base address set at
    /// exec).
    pub mp_frame_addr: u64,
    /// C: `mp_frame_len` (mproc.h; the frame length — one of the size-
    /// estimate bounds, proc.c:981).
    pub mp_frame_len: u64,
}

/// Bit values of C's `mp_flags` (mproc.h:86-104) — the wire authority.
///
/// The producer (PM `mproc/wire.rs`'s `flags_for`) and the consumers (the
/// `get_lwp_stat` state machine in the MIB table-pull half, and IS `dump_pm`)
/// all take their values from here; the PM crate used to hold a local table of
/// the same values, and after the pull-up the layout/value tests pin a single
/// source.
pub mod mp_flags {
    /// mproc.h:86 — slot in use (the predicate bit).
    pub const IN_USE: u32 = 0x00001;
    /// mproc.h:87 — suspended in wait4.
    pub const WAITING: u32 = 0x00002;
    /// mproc.h:88 — waiting for the parent's wait4.
    pub const ZOMBIE: u32 = 0x00004;
    /// mproc.h:89 — stopped.
    pub const PROC_STOPPED: u32 = 0x00008;
    /// mproc.h:90 — the alarm is running.
    pub const ALARM_ON: u32 = 0x00010;
    /// mproc.h:91 — exit has completed.
    pub const EXITING: u32 = 0x00020;
    /// mproc.h:92 — the parent has been notified.
    pub const TOLD_PARENT: u32 = 0x00040;
    /// mproc.h:93 — trace stop.
    pub const TRACE_STOPPED: u32 = 0x00080;
    /// mproc.h:94 — suspended in sigsuspend.
    pub const SIGSUSPENDED: u32 = 0x00100;
    /// mproc.h:95 — a VFS call is pending.
    pub const VFS_CALL: u32 = 0x00400;
    /// mproc.h:96 — new parent.
    pub const NEW_PARENT: u32 = 0x00800;
    /// mproc.h:97 — pause interrupted by a signal.
    pub const UNPAUSED: u32 = 0x01000;
    /// mproc.h:98 — kernel-privileged process.
    pub const PRIV_PROC: u32 = 0x02000;
    /// mproc.h:99 — exec is half done.
    pub const PARTIAL_EXEC: u32 = 0x04000;
    /// mproc.h:100 — trace exit pending.
    pub const TRACE_EXIT: u32 = 0x08000;
    /// mproc.h:101 — waiting for the tracer's wait4.
    pub const TRACE_ZOMBIE: u32 = 0x10000;
    /// mproc.h:102 — delayed call.
    pub const DELAY_CALL: u32 = 0x20000;
    /// mproc.h:103 — tainted.
    pub const TAINTED: u32 = 0x40000;
    /// mproc.h:104 — event call pending.
    pub const EVENT_CALL: u32 = 0x80000;
}

#[cfg(test)]
mod mproc_snap_layout_tests {
    use super::*;
    use core::mem::offset_of;

    /// Layout witness: 88 bytes (through `mp_started`) + the tail group
    /// (svuid/svgid/child utime+stime/ngroups/sgroups[16]/endpoint/frame
    /// three fields) = 200 bytes (the 4 bytes trailing endpoint are absorbed
    /// exactly by frame_addr's 8-alignment, so there is no struct tail
    /// padding).
    #[test]
    fn test_mproc_snap_size() {
        assert_eq!(size_of::<MProcSnap>(), 200);
    }

    /// Field offsets (the one contract the producer writes slot by slot and
    /// consumers read by name).
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

    /// mp_flags bit values (the wire authority's pinned values; compared bit
    /// by bit with C mproc.h:86-104).
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

    /// MP_MAGIC and the flags bit values pinned (the tail macros of mproc.h).
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
