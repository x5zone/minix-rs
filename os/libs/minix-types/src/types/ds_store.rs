//! Data-store table snapshot — the wire authority for `SI_DATA_STORE`
//! (`[ARCH: A-4]` single authority).
//!
//! C ground truth: `minix3/minix/servers/ds/store.h:16-29` (`ds_store[NR_DS_KEYS]`).
//! The snapshot carries the **scalar face** only: in C's
//! `union dsi_u { u32 u32; struct dsi_mem mem; }` the wide arm's `data` is a
//! pointer into the DS address space (meaningless across the wire), so the
//! snapshot replaces the whole union with one scalar slot — U32/LABEL take
//! `u.u32`, STR/MEM take `u.mem.length` (C's dump prints `length` for MEM;
//! for STR it prints the **contents** at the `data` pointer, which is a bug
//! in Minix3 itself, see `08-stage-is/09-is-dump-ds.md` §3.4).
//!
//! The producer (`render_image` in `os/servers/ds/src/server.rs`) renders
//! the `SI_DATA_STORE` reply image slot by slot from this structure, and the
//! consumer (IS `dump_ds`) reads it by name; the size gate is an exact match
//! (`store.c:668`), and both sides derive it from `size_of::<DsEntrySnap>()`.

/// Number of store slots. C: `NR_DS_KEYS (2*NR_SYS_PROCS)` — store.h:12
/// (`NR_SYS_PROCS 64` — sys_config.h:9).
pub const NR_DS_KEYS: usize = 128;
/// Max key/owner length. C: `DS_MAX_KEYLEN 80` — ds.h:29.
pub const DS_MAX_KEYLEN: usize = 80;
/// In-use bit. C: `DSF_IN_USE 0x001` — ds.h:12 (`DsFlags::IN_USE` is the same
/// bit's name in the flag set; the consumer (IS dump) reads the scalar word,
/// so the bare value is given here).
pub const DSF_IN_USE: u32 = 0x001;
// The authority for `DSF_MASK_TYPE` / `DS_MAX_KEYLEN` is `types/com.rs`
// (one single place); this module does not redefine them (it only adds
// `DsEntrySnap` plus its own slot count and in-use bit).

/// Data-store entry snapshot (scalar face only).
///
/// C: `struct data_store` — `store.h:16-29` (a subset: the string bodies and
/// pointers do not enter the snapshot, see the module docs).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct DsEntrySnap {
    /// C: `flags` (store.h:17).
    pub flags: i32,
    /// C: `key[DS_MAX_KEYLEN]` (store.h:18).
    pub key: [u8; DS_MAX_KEYLEN],
    /// C: `owner[DS_MAX_KEYLEN]` (store.h:19).
    pub owner: [u8; DS_MAX_KEYLEN],
    /// Scalar payload: `u.u32` (U32/LABEL) or `u.mem.length` (STR/MEM).
    pub scalar: u32,
}

impl Default for DsEntrySnap {
    // Manual: `[u8; 80]` has no `Default` — all-zero bytes are the C BSS
    // initialiser (vacant seats render all-zero, `server.rs` render_image).
    fn default() -> Self {
        Self { flags: 0, key: [0u8; DS_MAX_KEYLEN], owner: [0u8; DS_MAX_KEYLEN], scalar: 0 }
    }
}

#[cfg(test)]
mod ds_entry_snap_tests {
    use super::*;
    use core::mem::offset_of;

    /// Layout witness: flags(4) + key(80) + owner(80) + scalar(4) = 168.
    #[test]
    fn test_ds_entry_snap_layout() {
        assert_eq!(size_of::<DsEntrySnap>(), 168);
        assert_eq!(offset_of!(DsEntrySnap, flags), 0);
        assert_eq!(offset_of!(DsEntrySnap, key), 4);
        assert_eq!(offset_of!(DsEntrySnap, owner), 84);
        assert_eq!(offset_of!(DsEntrySnap, scalar), 164);
    }

    /// Image width = slot count × row width (the same formula for producer
    /// and consumer).
    #[test]
    fn test_image_bytes_follows_row_width() {
        assert_eq!(NR_DS_KEYS, 128);
        assert_eq!(size_of::<DsEntrySnap>() * NR_DS_KEYS, 21_504);
    }

    /// In-use and type bits (the wire values from ds.h:12/17-20; the mask
    /// constant authority is `types/com.rs`, so only this module's own in-use
    /// bit is pinned here).
    #[test]
    fn test_dsf_constants() {
        assert_eq!(DSF_IN_USE, 0x001);
        // Type-bit positions within the 0xFF0 mask (U32 0x010 / LABEL 0x100).
        assert_eq!(0x010 & 0xFF0, 0x010);
        assert_eq!(0x100 & 0xFF0, 0x100);
    }
}
