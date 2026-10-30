//! VFS process table snapshot — the `SI_FPTAB` wire contract.
//!
//! C ground truth: `minix3/minix/servers/vfs/fproc.h` (`fproc[NR_PROCS]`).
//! The snapshot carries the **used-field subset** per the A-4 deviation
//! ruling (E-MIBPROD/E-ISPROD): pointer arrays (`fp_filp[255]`, vnode
//! pointers) carry no meaning across the wire — the producer ships the
//! derived descriptor count instead.
//!
//! This is a memory-table snapshot, not a message payload: the 56-byte
//! message precedent does not apply (slot size ≈ 96 bytes, see `layout` test).

/// Light per-process row — `SI_PROCLIGHT_TAB` wire contract.
///
/// C: `struct fproc_light` (vfs/fproc.h:111-115: `dev_t fpl_tty` +
/// `int fpl_blocked_on` + `endpoint_t fpl_task` = 16 bytes per slot). The
/// C fill side is the `SI_PROCLIGHT_TAB` arm of `do_getsysinfo`
/// (misc.c:81-95); the consumer is MIB's `get_lwp_stat` (the cdev/sdev
/// lane of wchan/wmesg). `fpl_blocked_on` takes the `FP_BLOCKED_ON_*`
/// raw value from vfs/const.h:19-25.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FprocLightSnap {
    /// C: `fpl_tty` — copy of `fproc.fp_tty` (controlling terminal device
    /// number).
    pub fpl_tty: u64,
    /// C: `fpl_blocked_on` — copy of `fproc.fp_blocked_on`.
    pub fpl_blocked_on: u32,
    /// C: `fpl_task` — the blocking driver endpoint (taken directly for
    /// cdev, looked up through smap for sdev; `NONE` otherwise).
    pub fpl_task: i32,
}

/// VFS process snapshot — `SI_FPTAB` table row.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FProcSnap {
    /// C: `fp_pid` (fproc.h:18).
    pub fp_pid: i32,
    /// C: `fp_tty` device number (fproc.h:27).
    pub fp_tty: i32,
    /// C: `fp_umask` (fproc.h:69).
    pub fp_umask: u32,
    /// C: `fp_realuid` (fproc.h:63).
    pub fp_realuid: u32,
    /// C: `fp_effuid` (fproc.h:64).
    pub fp_effuid: u32,
    /// C: `fp_realgid` (fproc.h:65).
    pub fp_realgid: u32,
    /// C: `fp_effgid` (fproc.h:66).
    pub fp_effgid: u32,
    /// C: `fp_flags` (fproc.h:16).
    pub fp_flags: u32,
    /// C: `fp_blocked_on` (fproc.h:29).
    pub fp_blocked_on: i32,
    /// Open-descriptor count (producer-derived; A-4 deviation).
    pub nfds: u32,
    /// C: `fp_cdev.endpt` — blocked-on-CDEV endpoint column (dmp_fs.c:64).
    pub fp_cdev_endpt: i32,
    /// Reserved for a future timestamp (keeps the struct 8-aligned past
    /// 88 bytes); always zero.
    pub _reserved: [u32; 2],
}

#[cfg(test)]
mod fproc_snap_tests {
    use super::*;

    /// Layout witness: 11 i32/u32 fields (44 bytes) + 8 reserved bytes = 52.
    #[test]
    fn test_fproc_snap_layout() {
        assert_eq!(core::mem::size_of::<FProcSnap>(), 52);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_pid), 0);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_tty), 4);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_umask), 8);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_realuid), 12);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_effuid), 16);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_realgid), 20);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_effgid), 24);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_flags), 28);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_blocked_on), 32);
        assert_eq!(core::mem::offset_of!(FProcSnap, nfds), 36);
        assert_eq!(core::mem::offset_of!(FProcSnap, fp_cdev_endpt), 40);
    }

    /// Light row layout witness: 16 bytes (tty@0/blocked_on@8/task@12).
    #[test]
    fn test_fproc_light_snap_layout() {
        use core::mem::{offset_of, size_of};
        assert_eq!(size_of::<FprocLightSnap>(), 16);
        assert_eq!(offset_of!(FprocLightSnap, fpl_tty), 0);
        assert_eq!(offset_of!(FprocLightSnap, fpl_blocked_on), 8);
        assert_eq!(offset_of!(FprocLightSnap, fpl_task), 12);
    }
}
