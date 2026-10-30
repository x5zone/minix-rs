//! VFS device map table snapshot — the wire authority for `SI_DMAP_TAB`
//! (`[ARCH: A-4]` single authority).
//!
//! C ground truth: `minix3/minix/servers/vfs/dmap.h:16-28` (`struct dmap
//! dmap[NR_DEVICES]`, `NR_DEVICES 135` — dmap.h:82). The snapshot carries
//! only the **subset of fields in use** (C declaration order, `#[repr(C)]`)
//! — the same ruling as `FProcSnap`/`MProcSnap`/`DsEntrySnap`/
//! `RprocpubSnap`: C's `dmap_sel_filp` (filp pointer), `dmap_lock` (mutex)
//! and `dmap_servicing` (thread id) are all VFS-process-internal state and
//! mean nothing across the wire; IS's `dmap_dmp` prints only the label,
//! major and driver endpoint columns.
//!
//! The producer (the DMAP arm of `do_getsysinfo` in
//! `os/servers/vfs/src/misc.rs`) serializes the whole table row by row into
//! this structure; the consumer (`dmap_dmp` in IS `dump_vfs`) reads it by
//! name. An empty slot's `dmap_driver` is `NONE` (`init_dmap` fills every
//! row with `NONE` — dmap.c:235-247; it is not 0), and the consumer filters
//! on that.

/// C `LABEL_MAX 16` — vfs/const.h:34 (label column width).
pub const DMAP_LABEL_LEN: usize = 16;
/// C `NR_DEVICES 135` — dmap.h:82 (number of table rows; the length
/// contract of `SI_DMAP_TAB`).
pub const NR_DEVICES: usize = 135;

/// Device map table row snapshot (the subset of `struct dmap` fields in use).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmapSnap {
    /// C: `dmap_driver` (dmap.h:17; `NONE` = empty slot, not 0).
    pub dmap_driver: i32,
    /// C: `dmap_label[LABEL_MAX]` (dmap.h:18).
    pub dmap_label: [u8; DMAP_LABEL_LEN],
}

impl Default for DmapSnap {
    // Manual: `[u8; 16]` has no `Default`; all-zero bytes are the C BSS shape.
    fn default() -> Self {
        Self { dmap_driver: 0, dmap_label: [0u8; DMAP_LABEL_LEN] }
    }
}

#[cfg(test)]
mod dmap_snap_layout_tests {
    use super::*;
    use core::mem::offset_of;

    /// Layout witness: 4-byte driver endpoint + 16-byte label = 20.
    #[test]
    fn test_dmap_snap_layout() {
        assert_eq!(size_of::<DmapSnap>(), 20);
        assert_eq!(offset_of!(DmapSnap, dmap_driver), 0);
        assert_eq!(offset_of!(DmapSnap, dmap_label), 4);
    }

    /// Whole-table width = row count x row width (the same formula on the
    /// producer and consumer sides).
    #[test]
    fn test_table_width() {
        assert_eq!(NR_DEVICES, 135);
        assert_eq!(size_of::<DmapSnap>() * NR_DEVICES, 2_700);
    }
}
