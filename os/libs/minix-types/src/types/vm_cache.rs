//! Virtual-memory cache words: the flags and sentinels the file-system
//! buffer pool and the VM server agree on.
//!
//! C correspondence: `minix3/minix/include/minix/vm.h:84-93`. One header
//! serves both sides of the block-sharing protocol. The file-system buffer
//! pool carries one flags word per block
//! (`u32_t lmfs_flags`, `minix3/minix/include/minix/libminixfs.h:23`) and
//! passes its address in the `flags_ptr` lane of the cache-page requests
//! (`m_vmmcp.flags_ptr`, `ipc.h:2389`); the VM server stores the one-shot
//! bit in its own cache entry (`hb->flags = flags & VMSF_ONCE`,
//! `minix3/minix/servers/vm/cache.c:241`).
//!
//! The values live here, not in either side, because two crates that never
//! call each other directly must still agree byte for byte: `minix-fs`
//! builds the flags word, `minix-vm` interprets the setflags lane.
//!
//! Two families share the word width but never the field. `VMMC_*` travels
//! in the per-buffer flags word; `VMSF_ONCE` travels in the setflags lane of
//! `vm_set_cacheblock`. That `VMSF_ONCE` and `VMMC_FLAGS_LOCKED` are both
//! `0x01` is the C header's own choice and is kept verbatim — the families
//! are told apart by which lane carries them, never by the value alone.

/// Special inode number: the block has no associated file, it is a plain
/// disk block (`VMC_NO_INODE`, `vm.h:90`, value zero).
pub const VMC_NO_INODE: u64 = 0;

/// Discard the block after one-time use (`VMSF_ONCE`, `vm.h:93`). This is
/// the setflags lane of `vm_set_cacheblock`, not a buffer flag.
pub const VMSF_ONCE: u32 = 0x01;

/// Someone is updating the flags; readers must not trust the word
/// (`VMMC_FLAGS_LOCKED`, `vm.h:84`).
pub const VMMC_FLAGS_LOCKED: u32 = 0x01;

/// The buffer holds changes not yet on storage and may not be evicted
/// (`VMMC_DIRTY`, `vm.h:85`).
pub const VMMC_DIRTY: u32 = 0x02;

/// The virtual memory cache has evicted the buffer; its contents are
/// invalid (`VMMC_EVICTED`, `vm.h:86`).
pub const VMMC_EVICTED: u32 = 0x04;

/// The client is using the buffer and it may not be evicted
/// (`VMMC_BLOCK_LOCKED`, `vm.h:87`).
pub const VMMC_BLOCK_LOCKED: u32 = 0x08;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentinels_match_header() {
        assert_eq!(VMC_NO_INODE, 0);
        assert_eq!(VMSF_ONCE, 0x01);
    }

    #[test]
    fn test_flag_bits_match_header() {
        assert_eq!(VMMC_FLAGS_LOCKED, 0x01);
        assert_eq!(VMMC_DIRTY, 0x02);
        assert_eq!(VMMC_EVICTED, 0x04);
        assert_eq!(VMMC_BLOCK_LOCKED, 0x08);
    }

    #[test]
    fn test_buffer_flags_are_distinct_bits() {
        // The four VMMC bits share one word: a value carrying one must not
        // be readable as another.
        let all = [VMMC_FLAGS_LOCKED, VMMC_DIRTY, VMMC_EVICTED, VMMC_BLOCK_LOCKED];
        for (i, left) in all.iter().enumerate() {
            for right in &all[i + 1..] {
                assert_eq!(left & right, 0);
            }
        }
    }

    #[test]
    fn test_setflags_lane_collides_by_design() {
        // `VMSF_ONCE` and `VMMC_FLAGS_LOCKED` are numerically equal in C's
        // header; they never share a field. This test exists so a future
        // "de-duplication" cannot silently merge two different lanes.
        assert_eq!(VMSF_ONCE, VMMC_FLAGS_LOCKED);
    }
}
