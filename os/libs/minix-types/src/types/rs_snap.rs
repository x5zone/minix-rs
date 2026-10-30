//! RS service-table snapshot — the wire authority for `SI_PROCPUB_TAB` /
//! `SI_PROC_TAB` (single authority per `[ARCH: A-4]`).
//!
//! C ground truth: `minix3/minix/servers/rs/type.h:56-108` (`rproc[NR_SYS_PROCS]` and
//! its `r_pub` public half `struct rprocpub`, rs.h:165-184). The snapshot carries only
//! the **used field subset** (C declaration order, `#[repr(C)]`) — the same ruling as
//! `FProcSnap`/`MProcSnap`/`DsEntrySnap`: pointers meaningless across the wire
//! (`r_exec`/`r_argv`), PCI ACL details (used only by devman, and through RS's own
//! interface), and `struct priv` (kernel runtime state) all stay out of the snapshot.
//!
//! The two consumers each need one half, so the snapshot takes the union (the
//! multi-reader precedent of `ProcInfoStruct`):
//! - IS `dmp_rs`: `label`/`endpoint`/`dev_nr`/`sys_flags` (PUB table),
//!   `r_pid`/`r_restarts`/`r_flags`/`r_period`/`r_alive_tm`/`r_args` (PRIV table);
//! - VM's `RS_INIT` handshake: `in_use`/`endpoint`/`vm_call_mask` (the public table is
//!   read out with `rproctab_gid` authorization, `main.c:244-255`).
//!
//! The producer (the three copy-outs in `os/servers/rs/src/shell_request.rs`) serializes
//! slot by slot per this structure; C's per-line `struct` byte image (`rprocpub_off`/
//! `rproc_off` offset tables) retires with it.

/// C `RS_MAX_LABEL_LEN 16` — rs/const.h.
pub const RS_MAX_LABEL_LEN: usize = 16;
/// C `MAX_COMMAND_LEN 512` — rs/const.h:18 (the width of `r_args`).
pub const RS_MAX_COMMAND: usize = 512;
/// C `NR_SYS_PROCS 64` (sys_config.h:9) — the number of `rproc[NR_SYS_PROCS]` slots.
pub const RS_TABLE_LEN: usize = 64;

/// Snapshot of one public table row (the used field subset of `struct rprocpub`).
///
/// The same shape serves two readers: IS prints the identity columns
/// (label/endpoint/dev_nr/sys_flags), and VM reads `in_use`/`endpoint`/`vm_call_mask`
/// during the `RS_INIT` handshake to build the service ACL.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RprocpubSnap {
    /// C: `in_use` (rs.h:166).
    pub in_use: i32,
    /// C: `sys_flags` (rs.h:167).
    pub sys_flags: u32,
    /// C: `endpoint` (rs.h:168).
    pub endpoint: i32,
    /// C: `dev_nr` (rs.h:172; the major device number or `NO_DEV`).
    pub dev_nr: i32,
    /// C: `label[RS_MAX_LABEL_LEN]` (rs.h:176).
    pub label: [u8; RS_MAX_LABEL_LEN],
    /// C: `vm_call_mask[VM_CALL_MASK_SIZE]` — two 32-bit blocks combined into one bit
    /// mask (rs.h:179; bit *i* is call number *i*, the same position as C's
    /// little-endian block layout).
    pub vm_call_mask: u64,
}

/// Snapshot of one private table row (the used field subset of `struct rproc`; `r_args` is the trailing `%s` column).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RprocSnap {
    /// C: `r_pid` (type.h:63).
    pub r_pid: i32,
    /// C: `r_restarts` (type.h:66).
    pub r_restarts: i32,
    /// C: `r_flags` (type.h:68; `RS_IN_USE 0x001` is the in-use criterion).
    pub r_flags: u32,
    /// C: `r_period` (type.h:71, the `int` heartbeat period).
    pub r_period: i32,
    /// C: `r_alive_tm` (type.h:73, `clock_t` — in-tree `Clock = i64`).
    pub r_alive_tm: i64,
    /// C: `r_args[MAX_COMMAND_LEN]` (type.h:79) — the NUL-separated raw command line;
    /// `rproc_dmp` prints it byte by byte as the trailing column (dmp_rs.c:64).
    pub r_args: [u8; RS_MAX_COMMAND],
}

impl Default for RprocSnap {
    // Manual: `[u8; 512]` has no `Default` — all-zero bytes are the C BSS
    // initialiser (vacant rows render all-zero).
    fn default() -> Self {
        Self {
            r_pid: 0,
            r_restarts: 0,
            r_flags: 0,
            r_period: 0,
            r_alive_tm: 0,
            r_args: [0; RS_MAX_COMMAND],
        }
    }
}

#[cfg(test)]
mod rs_snap_layout_tests {
    use super::*;
    use core::mem::offset_of;

    /// Layout witness: a PUB row is five 4-byte fields + label(16) + an 8-byte mask = 40;
    /// a PRIV row is four 4-byte fields + `Clock` (i64, 8-byte aligned) + args(512) = 536.
    #[test]
    fn test_row_layouts() {
        assert_eq!(size_of::<RprocpubSnap>(), 40);
        assert_eq!(offset_of!(RprocpubSnap, in_use), 0);
        assert_eq!(offset_of!(RprocpubSnap, sys_flags), 4);
        assert_eq!(offset_of!(RprocpubSnap, endpoint), 8);
        assert_eq!(offset_of!(RprocpubSnap, dev_nr), 12);
        assert_eq!(offset_of!(RprocpubSnap, label), 16);
        assert_eq!(offset_of!(RprocpubSnap, vm_call_mask), 32);

        assert_eq!(size_of::<RprocSnap>(), 536);
        assert_eq!(offset_of!(RprocSnap, r_pid), 0);
        assert_eq!(offset_of!(RprocSnap, r_alive_tm), 16);
        assert_eq!(offset_of!(RprocSnap, r_args), 24);
    }

    /// Table width = slots × row width (the same formula on producer and consumer: the IS
    /// `SI_PROCPUB_TAB` and `SI_PROC_TAB` request sizes, and the two-part sum of
    /// `SI_PROCALL_TAB`).
    #[test]
    fn test_table_widths() {
        assert_eq!(RS_TABLE_LEN, 64);
        assert_eq!(size_of::<RprocpubSnap>() * RS_TABLE_LEN, 2_560);
        assert_eq!(size_of::<RprocSnap>() * RS_TABLE_LEN, 34_304);
        assert_eq!(
            size_of::<RprocSnap>() * RS_TABLE_LEN + size_of::<RprocpubSnap>() * RS_TABLE_LEN,
            36_864
        );
    }
}
