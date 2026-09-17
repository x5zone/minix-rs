//! VFS process table snapshot — the `SI_FPTAB` wire contract.
//!
//! C ground truth: `minix3/minix/servers/vfs/fproc.h` (`fproc[NR_PROCS]`).
//! The snapshot carries the **used-field subset** per the A-4 deviation
//! ruling (E-MIBPROD/E-ISPROD): pointer arrays (`fp_filp[255]`, vnode
//! pointers) carry no meaning across the wire — the producer ships the
//! derived descriptor count instead (V1-P1-3).
//!
//! This is a memory-table snapshot, not a message payload: the 56-byte
//! message判例 does not apply (slot size ≈ 96 bytes, see `layout` test).

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

    /// 布局见证：11 个 i32/u32 域（44 字节）+ 保留 8 字节 = 52。
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
}
