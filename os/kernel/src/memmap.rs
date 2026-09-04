//! Bootstrap memory reclamation — add_memmap.
//!
//! # Minix3 C Source Mapping
//!
//! - `pg_utils.c:86-125` — add_memmap(): add physical memory region to kinfo.memmap[]
//! - `com.h` — MAXMEMMAP constant
//!
//! # Design Decisions (08-system-init-boot-finish.md §3)
//!
//! - **D5**: 4GB truncation removed for 64-bit. The C version truncates at
//!   LIMIT=0xFFFFF000 because 32-bit Minix3 cannot handle >4GB physical addresses.
//!   In 64-bit minix-rs, Direct Map can access all physical memory.

/// Maximum number of memory map entries.
/// C: MAXMEMMAP = 40 — minix/include/minix/param.h:13. Rust raises it to
/// 128 deliberately: UEFI firmware memory maps routinely exceed 40 entries,
/// and truncating the firmware map would silently drop RAM. Slot-scan
/// semantics unchanged; capacity divergence from C is input-shaped.
pub const MAXMEMMAP: usize = 128;

/// Memory map entry representing a contiguous physical memory region.
/// C: struct memory_info in minix/type.h
#[derive(Debug, Clone, Copy)]
#[derive(Default)]
pub struct MemMapEntry {
    /// Physical base address (page-aligned).
    pub base: u64,
    /// Length in bytes (page-aligned).
    pub length: u64,
}

/// Const zero entry for static initialization.
pub const MEM_MAP_ENTRY_ZERO: MemMapEntry = MemMapEntry { base: 0, length: 0 };


impl MemMapEntry {
    /// Whether this entry is empty (available for use).
    /// C: mm_length == 0 check in add_memmap()
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

/// Errors from add_memmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemMapError {
    /// After page alignment, the region has zero length.
    ZeroLength,
    /// No empty slots available in the memory map.
    NoSlots,
}

/// Add a physical memory region to the kernel's memory map.
///
/// C: add_memmap() in pg_utils.c:86-121
///
/// # Design Decision D5
///
/// The C version truncates at `LIMIT = 0xFFFFF000` (4GB - 4KB) because
/// 32-bit Minix3 cannot handle physical addresses above 4GB. In 64-bit
/// minix-rs, Direct Map can access all physical memory, so this truncation
/// is unnecessary and has been removed.
///
/// # Known Gaps (TODO)
///
/// The C version also updates two `kinfo` fields that this function does
/// not handle, because `KernelInfo` is immutable (`&KernelInfo`) in Rust:
/// - `cbi->mmap_size` (pg_utils.c:110-111) — tracks highest used memmap index
/// - `cbi->mem_high_phys` (pg_utils.c:112-115) — tracks highest physical address
///
/// These must be updated by the caller (kmain Phase F) once a mutable
/// kernel state struct is available. See 08-system-init-boot-finish.md §4.5.
///
/// # Arguments
///
/// * `mmap` - Memory map array to insert into
/// * `addr` - Physical base address of the region
/// * `len` - Length of the region in bytes
///
/// # Returns
///
/// The index of the new entry, or a `MemMapError` on failure.
///
/// # Safety Invariant
///
/// This function should only be called during boot (while `kernel_may_alloc`
/// is true). The caller is responsible for ensuring this invariant.
/// C: assert(kernel_may_alloc) in pg_utils.c:102
pub fn add_memmap(mmap: &mut [MemMapEntry; MAXMEMMAP], addr: u64, len: u64) -> Result<usize, MemMapError> {
    // C: page alignment — roundup(addr) and rounddown(addr + len)
    let page_size = 4096u64;
    let aligned_base = (addr + page_size - 1) & !(page_size - 1);
    let aligned_end = (addr + len) & !(page_size - 1);
    let aligned_len = aligned_end.saturating_sub(aligned_base);

    if aligned_len == 0 {
        return Err(MemMapError::ZeroLength);
    }

    // C: linear scan for empty slot (mm_length == 0)
    for (i, entry) in mmap.iter_mut().enumerate() {
        if entry.is_empty() {
            *entry = MemMapEntry {
                base: aligned_base,
                length: aligned_len,
            };
            return Ok(i);
        }
    }

    Err(MemMapError::NoSlots)
}

/// Cut a physical memory region `[start, start + len)` from the memory map.
///
/// Removes the specified range from all memmap entries it overlaps with.
/// If the cut range splits an existing entry, the prefix and/or suffix
/// sub-ranges are added back via [`add_memmap`]. This ensures the total
/// free memory is preserved — only the cut region is removed.
///
/// C: cut_memmap() in pg_utils.c:32-63
///
/// # Design Decision D5 (alignment)
///
/// Like `add_memmap`, the cut range is page-aligned:
/// - `start` is rounded **down** to a page boundary (expand the cut)
/// - `end` is rounded **up** to a page boundary (expand the cut)
///
/// This matches C's `start -= o; end += PAGE_SIZE - o;` logic and ensures
/// the cut boundaries always fall on page boundaries, consistent with the
/// allocator's page-granularity allocations.
///
/// # Arguments
///
/// * `mmap` - Memory map array to modify
/// * `start` - Physical base address of the region to cut
/// * `len` - Length of the region to cut in bytes
///
/// # Returns
///
/// `Ok(())` on success. Returns `Err(MemMapError::NoSlots)` if `add_memmap`
/// fails to find an empty slot for a prefix/suffix sub-range (indicates
/// memory map fragmentation — fatal at boot).
///
/// # Safety Invariant
///
/// Same as `add_memmap`: should only be called during boot (while
/// `kernel_may_alloc` is true).
/// C: assert(kernel_may_alloc) in pg_utils.c:42
///
/// # Algorithm
///
/// 1. Page-align `start` down and `end = start + len` up.
/// 2. For each non-empty memmap entry `[memaddr, memend)`:
///    a. Clip `[start, end)` to `[memaddr, memend)` → `[substart, subend)`.
///    b. If no overlap (`substart >= subend`), skip.
///    c. Clear the entry (set base=0, length=0).
///    d. If prefix exists (`memaddr < substart`), add it back via `add_memmap`.
///    e. If suffix exists (`subend < memend`), add it back via `add_memmap`.
///
/// The `add_memmap` calls in steps d/e will reuse the just-cleared slot
/// (or another empty slot if the prefix/suffix doesn't fit the alignment
/// requirements of the cleared slot). This is safe because `add_memmap`
/// scans from index 0 for the first empty slot.
pub fn cut_memmap(mmap: &mut [MemMapEntry; MAXMEMMAP], start: u64, len: u64) -> Result<(), MemMapError> {
    // Zero-length cut is always a no-op (matches C behavior where the loop
    // body's `if(substart >= subend) continue;` skips zero-length overlaps).
    if len == 0 {
        return Ok(());
    }

    // C: page alignment — rounddown(start) and roundup(end)
    let page_size = 4096u64;
    let cut_start = start & !(page_size - 1); // round down
    let cut_end_raw = start.saturating_add(len);
    let mut cut_end = (cut_end_raw + page_size - 1) & !(page_size - 1); // round up
    if cut_end == 0 && cut_end_raw > 0 {
        // Overflow: cut to the end of addressable memory
        cut_end = u64::MAX;
    }
    if cut_start >= cut_end {
        // Zero-length cut after alignment — nothing to do
        return Ok(());
    }

    // Iterate all slots. We collect indices first to avoid borrow issues
    // (add_memmap needs &mut mmap while we're iterating).
    let mut indices_to_process: [bool; MAXMEMMAP] = [false; MAXMEMMAP];
    for i in 0..MAXMEMMAP {
        if !mmap[i].is_empty() {
            indices_to_process[i] = true;
        }
    }

    for i in 0..MAXMEMMAP {
        if !indices_to_process[i] {
            continue;
        }

        let memaddr = mmap[i].base;
        let memend = memaddr.saturating_add(mmap[i].length);

        // Clip cut range to this entry's bounds
        let substart = cut_start.max(memaddr);
        let subend = cut_end.min(memend);

        if substart >= subend {
            continue; // no overlap
        }

        // Clear the entry — we'll add back the non-overlapping parts
        mmap[i] = MemMapEntry::default();

        // Add back prefix: [memaddr, substart)
        if substart > memaddr {
            let prefix_len = substart - memaddr;
            // Ignore ZeroLength (shouldn't happen since substart > memaddr
            // and both are page-aligned, but defensive).
            let _ = add_memmap(mmap, memaddr, prefix_len);
        }

        // Add back suffix: [subend, memend)
        if subend < memend {
            let suffix_addr = subend;
            let suffix_len = memend - subend;
            add_memmap(mmap, suffix_addr, suffix_len)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn empty_mmap() -> [MemMapEntry; MAXMEMMAP] {
        [MemMapEntry::default(); MAXMEMMAP]
    }

    #[test]
    fn test_add_memmap_basic() {
        let mut mmap = empty_mmap();
        let idx = add_memmap(&mut mmap, 0x1000_0000, 0x1000_0000).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(mmap[0].base, 0x1000_0000);
        assert_eq!(mmap[0].length, 0x1000_0000);
    }

    #[test]
    fn test_add_memmap_alignment() {
        let mut mmap = empty_mmap();
        // Non-aligned address and length
        let idx = add_memmap(&mut mmap, 0x1000_0001, 0x1FFF).unwrap();
        assert_eq!(mmap[idx].base, 0x1000_1000); // rounded up
        assert_eq!(mmap[idx].length, 0x1000);    // rounded down
    }

    #[test]
    fn test_add_memmap_no_truncation() {
        // D5: >4GB addresses are NOT truncated
        let mut mmap = empty_mmap();
        let idx = add_memmap(&mut mmap, 0x1_0000_0000, 0x1000_0000).unwrap();
        assert_eq!(mmap[idx].base, 0x1_0000_0000); // >4GB, not truncated
        assert_eq!(mmap[idx].length, 0x1000_0000);
    }

    #[test]
    fn test_add_memmap_zero_length() {
        let mut mmap = empty_mmap();
        // Region too small to have any full page after alignment
        let result = add_memmap(&mut mmap, 0x1001, 0x1);
        assert_eq!(result, Err(MemMapError::ZeroLength));
    }

    #[test]
    fn test_add_memmap_no_slots() {
        let mut mmap = empty_mmap();
        // Fill all slots
        for (i, slot) in mmap.iter_mut().enumerate().take(MAXMEMMAP) {
            *slot = MemMapEntry { base: (i as u64) * 0x1000, length: 0x1000 };
        }
        let result = add_memmap(&mut mmap, 0x000F_F000_0000, 0x1000);
        assert_eq!(result, Err(MemMapError::NoSlots));
    }

    #[test]
    fn test_add_memmap_finds_first_empty() {
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x1000 };
        mmap[2] = MemMapEntry { base: 0x3000, length: 0x1000 };
        let idx = add_memmap(&mut mmap, 0x5000, 0x1000).unwrap();
        assert_eq!(idx, 1); // First empty slot
    }

    // ── cut_memmap tests ──

    #[test]
    fn test_cut_memmap_no_overlap() {
        // Cut range that doesn't overlap any entry → no change
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x1000 };
        cut_memmap(&mut mmap, 0x10000, 0x1000).unwrap();
        assert_eq!(mmap[0].base, 0x1000);
        assert_eq!(mmap[0].length, 0x1000);
    }

    #[test]
    fn test_cut_memmap_full_overlap() {
        // Cut range fully covers an entry → entry is removed entirely
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x1000 };
        cut_memmap(&mut mmap, 0x1000, 0x1000).unwrap();
        assert!(mmap[0].is_empty());
    }

    #[test]
    fn test_cut_memmap_prefix_split() {
        // Cut the beginning of an entry → prefix is removed, suffix is kept
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x4000 };
        cut_memmap(&mut mmap, 0x1000, 0x1000).unwrap();
        // The suffix [0x2000, 0x5000) should be added back
        let suffix = mmap.iter().find(|e| !e.is_empty()).unwrap();
        assert_eq!(suffix.base, 0x2000);
        assert_eq!(suffix.length, 0x3000);
    }

    #[test]
    fn test_cut_memmap_suffix_split() {
        // Cut the end of an entry → prefix is kept, suffix is removed
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x4000 };
        cut_memmap(&mut mmap, 0x4000, 0x1000).unwrap();
        // The prefix [0x1000, 0x4000) should be kept
        let prefix = mmap.iter().find(|e| !e.is_empty()).unwrap();
        assert_eq!(prefix.base, 0x1000);
        assert_eq!(prefix.length, 0x3000);
    }

    #[test]
    fn test_cut_memmap_middle_split() {
        // Cut the middle of an entry → both prefix and suffix are kept
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x8000 };
        // Cut [0x3000, 0x5000) from [0x1000, 0x9000)
        cut_memmap(&mut mmap, 0x3000, 0x2000).unwrap();
        let non_empty: Vec<_> = mmap.iter().filter(|e| !e.is_empty()).collect();
        assert_eq!(non_empty.len(), 2, "should have prefix + suffix");
        // Check that the two entries are [0x1000, 0x2000) and [0x5000, 0x9000)
        let has_prefix = non_empty.iter().any(|e| e.base == 0x1000 && e.length == 0x2000);
        let has_suffix = non_empty.iter().any(|e| e.base == 0x5000 && e.length == 0x4000);
        assert!(has_prefix, "prefix [0x1000, 0x3000) must exist");
        assert!(has_suffix, "suffix [0x5000, 0x9000) must exist");
    }

    #[test]
    fn test_cut_memmap_spans_multiple_entries() {
        // Cut range spans two entries → both are affected
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x2000 }; // [0x1000, 0x3000)
        mmap[1] = MemMapEntry { base: 0x4000, length: 0x2000 }; // [0x4000, 0x6000)
        // Cut [0x2000, 0x5000) — overlaps tail of entry 0 and head of entry 1
        cut_memmap(&mut mmap, 0x2000, 0x3000).unwrap();
        let non_empty: Vec<_> = mmap.iter().filter(|e| !e.is_empty()).collect();
        assert_eq!(non_empty.len(), 2, "should have prefix of 0 + suffix of 1");
        // Entry 0's prefix [0x1000, 0x2000) should survive
        let has_prefix0 = non_empty.iter().any(|e| e.base == 0x1000 && e.length == 0x1000);
        // Entry 1's suffix [0x5000, 0x6000) should survive
        let has_suffix1 = non_empty.iter().any(|e| e.base == 0x5000 && e.length == 0x1000);
        assert!(has_prefix0, "prefix of entry 0 must survive");
        assert!(has_suffix1, "suffix of entry 1 must survive");
    }

    #[test]
    fn test_cut_memmap_alignment_round_down_start() {
        // Non-page-aligned start is rounded DOWN (cut expands)
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x4000 }; // [0x1000, 0x5000)
        // Cut starting at 0x2001, len 0x1000 → [0x2001, 0x3001)
        // start rounds down to 0x2000, end rounds up to 0x4000
        // → cut [0x2000, 0x4000), prefix [0x1000, 0x2000), suffix [0x4000, 0x5000)
        cut_memmap(&mut mmap, 0x2001, 0x1000).unwrap();
        let non_empty: Vec<_> = mmap.iter().filter(|e| !e.is_empty()).collect();
        let has_prefix = non_empty.iter().any(|e| e.base == 0x1000 && e.length == 0x1000);
        let has_suffix = non_empty.iter().any(|e| e.base == 0x4000 && e.length == 0x1000);
        assert!(has_prefix, "prefix [0x1000, 0x2000) must exist");
        assert!(has_suffix, "suffix [0x4000, 0x5000) must exist (end rounds up)");
    }

    #[test]
    fn test_cut_memmap_alignment_round_up_end() {
        // Non-page-aligned end is rounded UP (cut expands)
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x4000 }; // [0x1000, 0x5000)
        // Cut ending at 0x2001 (start=0x1000, len=0x1001) → end rounds up to 0x3000
        cut_memmap(&mut mmap, 0x1000, 0x1001).unwrap();
        let non_empty: Vec<_> = mmap.iter().filter(|e| !e.is_empty()).collect();
        // Only suffix [0x3000, 0x5000) should exist
        let has_suffix = non_empty.iter().any(|e| e.base == 0x3000 && e.length == 0x2000);
        assert!(has_suffix, "end must be rounded up to page boundary");
    }

    #[test]
    fn test_cut_memmap_zero_length_no_op() {
        // Zero-length cut (len=0) is always a no-op regardless of alignment
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x1000 };
        cut_memmap(&mut mmap, 0x1001, 0).unwrap();
        assert_eq!(mmap[0].base, 0x1000, "zero-length cut should be no-op");
        assert_eq!(mmap[0].length, 0x1000);
    }

    #[test]
    fn test_cut_memmap_preserves_total_free_memory() {
        // After cutting, total free memory should decrease by exactly the
        // page-aligned cut size (no leaks, no over-reclaim)
        let mut mmap = empty_mmap();
        mmap[0] = MemMapEntry { base: 0x1000, length: 0x10000 }; // 64KB
        let total_before: u64 = mmap.iter().map(|e| e.length).sum();
        cut_memmap(&mut mmap, 0x4000, 0x2000).unwrap(); // cut 8KB
        let total_after: u64 = mmap.iter().map(|e| e.length).sum();
        assert_eq!(total_before - total_after, 0x2000, "total free memory must decrease by cut size");
    }
}
