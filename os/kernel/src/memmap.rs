//! Bootstrap memory reclamation — add_memmap.
//!
//! # Minix3 C Source Mapping
//!
//! - `pg_utils.c:86-125` — add_memmap(): add physical memory region to kinfo.memmap[]
//! - `com.h` — MAXMEMMAP constant
//!
//! # Design Decisions (07-system-init-boot-finish.md §3)
//!
//! - **D5**: 4GB truncation removed for 64-bit. The C version truncates at
//!   LIMIT=0xFFFFF000 because 32-bit Minix3 cannot handle >4GB physical addresses.
//!   In 64-bit minix-rs, Direct Map can access all physical memory.

/// Maximum number of memory map entries.
/// C: MAXMEMMAP in minix/com.h
pub const MAXMEMMAP: usize = 128;

/// Memory map entry representing a contiguous physical memory region.
/// C: struct memory_info in minix/type.h
#[derive(Debug, Clone, Copy)]
pub struct MemMapEntry {
    /// Physical base address (page-aligned).
    pub base: u64,
    /// Length in bytes (page-aligned).
    pub length: u64,
}

/// Const zero entry for static initialization.
pub const MEM_MAP_ENTRY_ZERO: MemMapEntry = MemMapEntry { base: 0, length: 0 };

impl Default for MemMapEntry {
    fn default() -> Self {
        Self { base: 0, length: 0 }
    }
}

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
/// C: add_memmap() in pg_utils.c:86-125
///
/// # Design Decision D5
///
/// The C version truncates at `LIMIT = 0xFFFFF000` (4GB - 4KB) because
/// 32-bit Minix3 cannot handle physical addresses above 4GB. In 64-bit
/// minix-rs, Direct Map can access all physical memory, so this truncation
/// is unnecessary and has been removed.
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
/// C: assert(kernel_may_alloc) in pg_utils.c:96
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
    for i in 0..MAXMEMMAP {
        if mmap[i].is_empty() {
            mmap[i] = MemMapEntry {
                base: aligned_base,
                length: aligned_len,
            };
            return Ok(i);
        }
    }

    Err(MemMapError::NoSlots)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for i in 0..MAXMEMMAP {
            mmap[i] = MemMapEntry { base: (i as u64) * 0x1000, length: 0x1000 };
        }
        let result = add_memmap(&mut mmap, 0xFF000_0000, 0x1000);
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
}
