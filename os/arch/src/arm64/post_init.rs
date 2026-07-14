//! AArch64 post-initialization and memory init implementation
//!
//! Implements `PostInitArch` and `MemoryInitArch` for AArch64.
//!
//! # Register mapping
//!
//! | C field         | AArch64 register | Rust field              |
//! |-----------------|------------------|------------------------|
//! | p_seg.p_ttbr    | TTBR0_EL1        | VmPageTableInfo::phys_root |
//! | p_seg.p_ttbr_v  | direct-mapped    | VmPageTableInfo::virt_root |
//!
//! C: arch_post_init() — earm/protect.c:97-104
//! C: memory_init() — earm/memory.c:612-622

use crate::post_init::{PostInitArch, MemoryInitArch, VmPageTableInfo, FreePdeSlots, MAX_FREE_PDE_SLOTS};

/// AArch64 post-initialization implementation.
///
/// Sets `ptproc` to VM and records VM's TTBR0 (translation table base) addresses.
/// In AArch64, the page table root is stored in TTBR0_EL1 (user space) or
/// TTBR1_EL1 (kernel space). The kernel maintains a direct-mapped virtual
/// pointer for modification.
///
/// C: arch_post_init() — earm/protect.c:97-104
pub struct AArch64PostInitArch;

impl PostInitArch for AArch64PostInitArch {
    fn set_ptproc(vm_page_table: &VmPageTableInfo) {
        // C: vm = proc_addr(VM_PROC_NR);
        // C: get_cpulocal_var(ptproc) = vm;
        // C: pg_info(&vm->p_seg.p_ttbr, &vm->p_seg.p_ttbr_v);
        //
        // AArch64 uses TTBR0_EL1 for user-space page tables.
        // pg_info() records the translation table base physical address
        // (TTBR0 value) and its kernel-virtual pointer.
        //
        // SAFETY: BKL is held during boot, only this CPU accesses ptproc.
        let _ = vm_page_table;
    }
}

/// Maximum number of entries in an AArch64 translation table at the L1 level.
///
/// AArch64 with 4KB pages and 4-level paging:
/// - L0 table: 512 entries (each covers 512GB)
/// - L1 table: 512 entries (each covers 1GB)
/// - L2 table: 512 entries (each covers 2MB)
/// - L3 table: 512 entries (each covers 4KB)
///
/// C: ARM_VM_DIR_ENTRIES — earm/memory.c:620
const AARCH64_TABLE_ENTRIES: usize = 512;

/// AArch64 memory initialization implementation.
///
/// Allocates 2 free page table entries from `free_upper_idx` for
/// use by the createpde() equivalent temporary mappings.
///
/// C: memory_init() — earm/memory.c:612-622
pub struct AArch64MemoryInitArch;

impl MemoryInitArch for AArch64MemoryInitArch {
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots {
        // C: assert(nfreepdes == 0);
        let mut slots = FreePdeSlots::new();

        // C: freepdes[nfreepdes++] = kinfo.freepde_start++;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        // C: freepdes[nfreepdes++] = kinfo.freepde_start++;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        // C: assert(kinfo.freepde_start < ARM_VM_DIR_ENTRIES);
        assert!(
            *free_upper_idx < AARCH64_TABLE_ENTRIES,
            "free_upper_idx overflow: {} >= {}",
            *free_upper_idx, AARCH64_TABLE_ENTRIES
        );

        slots
    }
}

// ── AArch64-specific unit tests ──
//
// These tests verify architecture-specific overflow behavior and post-init
// contract. The generic FreePdeSlots data structure tests live in
// arch/post_init.rs; these tests cover the AArch64 panic-on-overflow
// assertion (matching C's assert in earm/memory.c:620).
//
// NOTE: These tests compile only on aarch64 targets (not on x86_64 host).
#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{PhysBytes, VirBytes};

    /// Verifies that allocate_free_pdes panics when free_upper_idx is
    /// at the overflow boundary (510 for 512-entry table → final idx 512 >= 512).
    ///
    /// C: assert(kinfo.freepde_start < ARM_VM_DIR_ENTRIES) — earm/memory.c:620
    #[test]
    #[should_panic(expected = "free_upper_idx overflow")]
    fn test_allocate_free_pdes_panics_on_overflow() {
        let mut idx: usize = 510;
        let _ = AArch64MemoryInitArch::allocate_free_pdes(&mut idx);
    }

    /// Verifies that allocate_free_pdes succeeds at the highest valid
    /// starting index (509 → final idx 511 < 512).
    #[test]
    fn test_allocate_free_pdes_at_highest_valid_index() {
        let mut idx: usize = 509;
        let slots = AArch64MemoryInitArch::allocate_free_pdes(&mut idx);
        assert_eq!(slots.len(), 2);
        assert_eq!(idx, 511);
    }

    /// Verifies that set_ptproc accepts a valid VmPageTableInfo without
    /// panicking. The set_ptproc implementation is a no-op placeholder
    /// until per-CPU ptproc variable support is added (todo.md §12.2).
    #[test]
    fn test_set_ptproc_accepts_valid_vm_page_table_info() {
        let info = VmPageTableInfo {
            phys_root: PhysBytes(0x1000),
            virt_root: Some(VirBytes(0xFFFF_0000_0010_0000)),
        };
        AArch64PostInitArch::set_ptproc(&info);
    }
}
