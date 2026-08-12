//! Page table entry (PTE) walk helpers.
//!
//! C: `vm_lookup()` — kernel/arch/i386/memory.c:325 / kernel/arch/earm/memory.c:302
//!
//! This module provides the *offline* PTE walk used by the kernel when it
//! needs to translate a virtual address to a physical address without
//! having a live `&Paging` instance (which would require the MMU to be
//! running the target process's page table).
//!
//! # Offline walk pattern
//!
//! ```text
//! kernel_root_paddr → PML4 (4KB page of 512 PTEs)
//!                        ↓ read PML4[indices[0]]
//!                    PDPT (4KB page of 512 PTEs)
//!                        ↓ read PDPT[indices[1]]
//!                    PD   (4KB page of 512 PTEs)
//!                        ↓ read PD[indices[2]]
//!                    PT   (4KB page of 512 PTEs)
//!                        ↓ read PT[indices[3]]
//!                    final_paddr = (PTE & ADDR_MASK) | (vaddr & OFFSET_MASK)
//! ```
//!
//! Each PTE lookup reads 8 bytes (64-bit entry) from physical memory.
//! We use the Direct Map (`kernel_phys_to_virt`) to convert the
//! physical address of the next-level table into a kernel-virtual
//! pointer, then dereference.
//!
//! # Architecture abstraction
//!
//! The PTE layout (bits 0-11 are flags, bits 12-51 are physical page
//! frame number on x86-64) is **architecture-specific**. This module
//! encodes the x86-64 layout as a default; aarch64/RISC-V callers
//! must provide their own walk functions (with their own PTE encoding).
//!
//! # PageFlags
//!
//! We translate the raw PTE bits into the `PageFlags` enum from
//! `minix_arch::paging::PageFlags`. The mapping is one-way: callers
//! see only OS-semantic flags, not hardware encoding details.

use minix_arch::direct_map::DirectMapArch;
use minix_arch::paging::PageFlags;
use minix_arch::CurrentDirectMap;
use minix_arch::{CurrentPteWalk, PteWalkArch};
use minix_types::{PhysBytes, VirBytes};

/// Size of a PTE entry in bytes (x86-64: 8 bytes, 64-bit PTE).
pub const PTE_SIZE: usize = 8;

/// Number of entries per page table page (x86-64: 512 entries × 8 bytes).
pub const PT_ENTRIES: usize = 512;

/// Mask for the 9-bit index within a page table (bits 12-20, 21-29, 30-38, 39-47).
pub const PT_INDEX_MASK: u64 = 0x1FF;

/// Bits for page offset within a 4KB page.
pub const PAGE_OFFSET_MASK: u64 = 0xFFF;

/// Mask for the physical address field of a PTE.
/// On x86-64 with 4-level paging: bits 12-51 (40-bit physical address).
pub const PTE_ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// Error type for user↔kernel copy operations.
///
/// `Fault` — page not present (lazy page not yet faulted in).
/// The caller should set `RTS_VMSUSPEND` and retry after VM handles the fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserCopyError {
    /// Page table walk hit a non-present entry.
    Fault,
}

/// Read a single 8-byte PTE from physical memory via Direct Map.
///
/// The PTE's physical address is converted to a kernel virtual address
/// using `kernel_phys_to_virt`, then dereferenced as a `u64`.
///
/// # Safety
///
/// - The Direct Map must be set up (DirectMapArch::KERNEL_DIRECT_MAP_BASE
///   maps to physical address 0).
/// - `paddr` must point to a valid 8-byte aligned PTE entry.
///
/// Returns the raw 64-bit PTE value.
pub unsafe fn read_pte(paddr: PhysBytes) -> u64 {
    let vaddr = CurrentDirectMap::kernel_phys_to_virt(paddr);
    let ptr = vaddr.0 as *const u64;
    // SAFETY: caller guarantees `paddr` is a valid 8-byte aligned PTE and the
    // Direct Map is active, so `ptr` dereferences to a valid u64 PTE slot.
    unsafe { core::ptr::read_volatile(ptr) }
}

/// Translate a PTE's flag bits into `PageFlags`.
///
/// This is the OS-level semantic interface; the hardware encoding
/// (e.g. NX bit inversion on x86-64) is encapsulated here.
pub fn pte_to_page_flags(pte: u64) -> PageFlags {
    let mut flags = PageFlags::empty();
    // Bit 0: PRESENT
    if pte & 0x1 != 0 {
        flags |= PageFlags::PRESENT;
    }
    // Bit 1: WRITABLE (R/W)
    if pte & 0x2 != 0 {
        flags |= PageFlags::WRITABLE;
    }
    // Bit 2: USER_ACCESSIBLE (U/S)
    if pte & 0x4 != 0 {
        flags |= PageFlags::USER_ACCESSIBLE;
    }
    // Bit 5: ACCESSED (A)
    if pte & 0x20 != 0 {
        flags |= PageFlags::ACCESSED;
    }
    // Bit 6: DIRTY (D)
    if pte & 0x40 != 0 {
        flags |= PageFlags::DIRTY;
    }
    // Bit 7: HUGE_PAGE (PS at PD level for 2MB pages)
    //         We treat PS as HUGE_PAGE only when combined with PRESENT
    //         and not at the final level (the kernel decides level).
    //         For simplicity, propagate PS bit to HUGE_PAGE flag.
    if pte & 0x80 != 0 {
        flags |= PageFlags::HUGE_PAGE;
    }
    // Bit 8: GLOBAL (G)
    if pte & 0x100 != 0 {
        flags |= PageFlags::GLOBAL;
    }
    flags
}

/// Extract the physical address from a PTE.
///
/// Returns `Some(paddr)` if the PTE is PRESENT, `None` otherwise.
pub fn pte_to_phys(pte: u64) -> Option<PhysBytes> {
    if pte & 0x1 == 0 {
        return None; // Not present
    }
    Some(PhysBytes(pte & PTE_ADDR_MASK))
}

/// Compute the 9-bit indices for each level of the page table walk.
///
/// For x86-64 4-level paging:
/// - indices[0] = bits 39-47 (PML4 index)
/// - indices[1] = bits 30-38 (PDPT index)
/// - indices[2] = bits 21-29 (PD index)
/// - indices[3] = bits 12-20 (PT index)
pub fn vaddr_indices(vaddr: VirBytes) -> [usize; 4] {
    let v = vaddr.0;
    [
        ((v >> 39) & PT_INDEX_MASK) as usize,
        ((v >> 30) & PT_INDEX_MASK) as usize,
        ((v >> 21) & PT_INDEX_MASK) as usize,
        ((v >> 12) & PT_INDEX_MASK) as usize,
    ]
}

/// Walk a 4-level page table (x86-64) to translate a virtual address
/// to a physical address.
///
/// **Note**: This function is retained for backward compatibility with
/// existing call sites and tests. It delegates to
/// `minix_arch::CurrentPteWalk::walk`, which selects the architecture-
/// specific `PteWalkArch` implementor at compile time. On x86-64, this
/// calls `X86_64PteWalk::walk` — the same logic as the former inline
/// implementation, now living in the arch crate (`os/arch/src/x86_64/paging.rs`).
///
/// New code should call `minix_arch::CurrentPteWalk::walk` directly
/// instead of this wrapper, to avoid the x86_64-specific name.
///
/// # Arguments
///
/// * `root_paddr` — physical address of the PML4 (top-level) page.
/// * `vaddr` — the virtual address to translate.
///
/// # Returns
///
/// `Some((paddr, flags))` if the address is mapped (PRESENT), `None`
/// if any level of the walk encounters a non-present entry.
pub fn walk_x86_64(root_paddr: PhysBytes, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
    CurrentPteWalk::walk(root_paddr, vaddr)
}

/// Copy bytes from a user process's virtual address space into a kernel
/// buffer.
///
/// This is the Rust equivalent of C's `data_copy(caller_ep, user_addr,
/// KERNEL, kernel_buf, bytes)` — used by `do_vdevio`, `do_readbios`,
/// and similar system calls that need to read user-space data into a
/// kernel stack/static buffer.
///
/// Walks the caller's page table page-by-page via `walk_x86_64`, then
/// uses the Direct Map to access each physical page.
///
/// # Arguments
///
/// * `root_paddr` — physical address of the process's PML4 root
///   (from `proc.p_seg.phys_root`)
/// * `user_addr` — starting virtual address in the user process
/// * `dst` — destination kernel buffer (stack or static)
///
/// # Returns
///
/// `Ok(())` on success, `Err(CopyError::Fault)` if any page is not
/// present (lazy page not yet faulted in).
///
/// # Anti-translate
///
/// C uses `data_copy(..., KERNEL, ...)` with a special KERNEL endpoint.
/// Rust uses an explicit kernel buffer slice — the type system ensures
/// the destination is kernel-accessible without a magic endpoint.
pub fn copy_from_user(
    root_paddr: PhysBytes,
    user_addr: VirBytes,
    dst: &mut [u8],
) -> Result<(), UserCopyError> {
    let mut remaining = dst.len();
    let mut src_offset = user_addr.0;
    let mut dst_offset = 0usize;

    while remaining > 0 {
        let (phys, _flags) = walk_x86_64(root_paddr, VirBytes(src_offset))
            .ok_or(UserCopyError::Fault)?;
        let page_offset = (src_offset & PAGE_OFFSET_MASK) as usize;
        let chunk = core::cmp::min(remaining, 0x1000 - page_offset);

        // phys already includes the page offset (from walk_x86_64), so
        // kernel_phys_to_virt(phys) points directly to the exact byte.
        let kv = CurrentDirectMap::kernel_phys_to_virt(phys);
        // SAFETY: Direct Map is active; kv.0 is a valid kernel-virtual
        // pointer to the user's physical page. The chunk does not cross
        // a page boundary (ensured by the min calculation above).
        unsafe {
            core::ptr::copy_nonoverlapping(
                kv.0 as *const u8,
                dst[dst_offset..].as_mut_ptr(),
                chunk,
            );
        }
        src_offset += chunk as u64;
        dst_offset += chunk;
        remaining -= chunk;
    }
    Ok(())
}

/// Copy bytes from a kernel buffer into a user process's virtual address
/// space.
///
/// This is the Rust equivalent of C's `data_copy(KERNEL, kernel_buf,
/// caller_ep, user_addr, bytes)` — used by `do_vdevio` (input results),
/// `do_readbios`, and similar system calls.
///
/// # Arguments
///
/// * `src` — source kernel buffer (stack or static)
/// * `root_paddr` — physical address of the process's PML4 root
/// * `user_addr` — starting virtual address in the user process
///
/// # Returns
///
/// `Ok(())` on success, `Err(CopyError::Fault)` if any page is not
/// present.
pub fn copy_to_user(
    src: &[u8],
    root_paddr: PhysBytes,
    user_addr: VirBytes,
) -> Result<(), UserCopyError> {
    let mut remaining = src.len();
    let mut src_offset = 0usize;
    let mut dst_offset = user_addr.0;

    while remaining > 0 {
        let (phys, _flags) = walk_x86_64(root_paddr, VirBytes(dst_offset))
            .ok_or(UserCopyError::Fault)?;
        let page_offset = (dst_offset & PAGE_OFFSET_MASK) as usize;
        let chunk = core::cmp::min(remaining, 0x1000 - page_offset);

        let kv = CurrentDirectMap::kernel_phys_to_virt(phys);
        // SAFETY: Direct Map is active; kv.0 is a valid kernel-virtual
        // pointer to the user's physical page (writable via Direct Map).
        unsafe {
            core::ptr::copy_nonoverlapping(
                src[src_offset..].as_ptr(),
                kv.0 as *mut u8,
                chunk,
            );
        }
        dst_offset += chunk as u64;
        src_offset += chunk;
        remaining -= chunk;
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pte_to_page_flags_present_writable_user() {
        // Bits: P=1, R/W=1, U/S=1, A=0, D=0, PS=0, G=0 → 0x7
        let pte: u64 = 0x7;
        let flags = pte_to_page_flags(pte);
        assert!(flags.contains(PageFlags::PRESENT));
        assert!(flags.contains(PageFlags::WRITABLE));
        assert!(flags.contains(PageFlags::USER_ACCESSIBLE));
        assert!(!flags.contains(PageFlags::ACCESSED));
        assert!(!flags.contains(PageFlags::DIRTY));
    }

    #[test]
    fn test_pte_to_page_flags_kernel_global() {
        // Bits: P=1, R/W=0, U/S=0, G=1 → 0x101
        let pte: u64 = 0x101;
        let flags = pte_to_page_flags(pte);
        assert!(flags.contains(PageFlags::PRESENT));
        assert!(!flags.contains(PageFlags::WRITABLE));
        assert!(!flags.contains(PageFlags::USER_ACCESSIBLE));
        assert!(flags.contains(PageFlags::GLOBAL));
    }

    #[test]
    fn test_pte_to_page_flags_2mb_huge() {
        // P=1, R/W=1, U/S=1, PS=1 → 0x87
        let pte: u64 = 0x87;
        let flags = pte_to_page_flags(pte);
        assert!(flags.contains(PageFlags::PRESENT));
        assert!(flags.contains(PageFlags::WRITABLE));
        assert!(flags.contains(PageFlags::USER_ACCESSIBLE));
        assert!(flags.contains(PageFlags::HUGE_PAGE));
    }

    #[test]
    fn test_pte_to_phys_present() {
        // P=1, addr = 0x1000 → 0x1001
        let pte: u64 = 0x1001;
        let paddr = pte_to_phys(pte);
        assert_eq!(paddr, Some(PhysBytes(0x1000)));
    }

    #[test]
    fn test_pte_to_phys_not_present() {
        let pte: u64 = 0x0;
        assert_eq!(pte_to_phys(pte), None);
    }

    #[test]
    fn test_pte_to_phys_addr_mask_clears_flags() {
        // High physical bits set + flags → addr mask extracts addr.
        let pte: u64 = 0x0000_FFFF_FFFF_F007; // P+R/W+U/S + max addr bits
        let paddr = pte_to_phys(pte);
        // PTE_ADDR_MASK clears bits 0-11 (flags) → addr = pte itself
        // (the test value has all addr bits set to 1).
        assert_eq!(paddr, Some(PhysBytes(0x0000_FFFF_FFFF_F000)));
    }

    #[test]
    fn test_vaddr_indices_layout() {
        // x86-64 canonical high address: 0xFFFF_8000_0000_0000
        // indices should be: [256, 0, 0, 0]
        let vaddr = VirBytes(0xFFFF_8000_0000_0000);
        let idx = vaddr_indices(vaddr);
        assert_eq!(idx, [256, 0, 0, 0]);
    }

    #[test]
    fn test_vaddr_indices_low_address() {
        // Low canonical address: 0x0000_0000_1000 (page 1, offset 0)
        let vaddr = VirBytes(0x0000_0000_0000_1000);
        let idx = vaddr_indices(vaddr);
        assert_eq!(idx, [0, 0, 0, 1]);
    }

    #[test]
    fn test_vaddr_indices_mid_address() {
        // 0x0000_0040_0000_1000 = 2^38 + 2^12. On x86-64:
        // - bits 39-47 (PML4): 0 (since vaddr < 2^39)
        // - bits 30-38 (PDPT): (2^38 >> 30) & 0x1ff = 256 & 0x1ff = 256
        // - bits 21-29 (PD):   (vaddr >> 21) & 0x1ff
        //   0x40_0000_0000 >> 21 = 0x200_0000 = 33,554,432 & 0x1ff = 0
        // - bits 12-20 (PT):   (0x40_0000_1000 >> 12) & 0x1ff
        //   = 0x40_0000_1 & 0x1ff = 1
        let vaddr = VirBytes(0x0000_0040_0000_1000);
        let idx = vaddr_indices(vaddr);
        assert_eq!(idx[0], 0, "PML4 index for 0x4000001000");
        assert_eq!(idx[1], 256, "PDPT index for 0x4000001000");
        assert_eq!(idx[2], 0, "PD index for 0x4000001000");
        assert_eq!(idx[3], 1, "PT index for 0x4000001000");
    }

    #[test]
    fn test_pte_constants_match_x86_64() {
        // Verify constants match x86-64 hardware specification.
        assert_eq!(PTE_SIZE, 8, "x86-64 PTE is 8 bytes");
        assert_eq!(PT_ENTRIES, 512, "4KB page / 8B PTE = 512 entries");
        assert_eq!(PAGE_OFFSET_MASK, 0xFFF, "12-bit page offset");
    }

    #[test]
    fn test_walk_x86_64_requires_unsafe_memory() {
        // walk_x86_64 reads from physical memory via Direct Map. In the
        // test environment, the mock Direct Map base is a constant but
        // the memory at the resulting virtual address is unmapped →
        // any actual deref would SIGSEGV. We therefore only verify that
        // the function compiles and the index computation is correct.
        //
        // Real (integration) tests require QEMU + boot-time page table
        // setup. Those tests are scheduled in 03-stage-kernel phase C.
        let vaddr = VirBytes(0x0000_0040_0000_1000);
        let idx = vaddr_indices(vaddr);
        // Just verify index math, not the walk itself.
        assert_eq!(idx[0], 0);
        assert_eq!(idx[1], 256);
        assert_eq!(idx[2], 0);
        assert_eq!(idx[3], 1);
    }

    #[test]
    fn test_pte_addr_mask_correctness() {
        // PTE_ADDR_MASK should extract bits 12-51 from a 64-bit PTE.
        // Bits 0-11 are flags, bits 52-63 are reserved (must be 0).
        let pte_with_only_addr: u64 = 0x0000_FFFF_FFFF_F000;
        assert_eq!(pte_with_only_addr & PTE_ADDR_MASK, 0x0000_FFFF_FFFF_F000);
        let pte_with_high_bits: u64 = 0xFFFF_FFFF_FFFF_F007; // bad PTE
        // Bits 52-63 are masked out (they are reserved).
        assert_eq!(pte_with_high_bits & PTE_ADDR_MASK, 0x000F_FFFF_FFFF_F000);
    }
}