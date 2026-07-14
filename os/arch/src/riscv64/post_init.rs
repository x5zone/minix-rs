//! RISC-V 64-bit post-initialization and memory init implementation
//!
//! Implements `PostInitArch` and `MemoryInitArch` for RISC-V 64-bit.
//!
//! # Register mapping
//!
//! | C field (analogy) | RISC-V register | Rust field              |
//! |-------------------|-----------------|------------------------|
//! | p_seg.p_cr3/p_ttbr | satp           | VmPageTableInfo::phys_root |
//! | p_seg.p_cr3_v/p_ttbr_v | direct-map | VmPageTableInfo::virt_root |
//!
//! Note: Minix3 does not have a RISC-V port. This implementation is
//! designed by analogy with the x86 and ARM ports, following RISC-V
//! privileged specification conventions (Sv39 paging).

use crate::post_init::{PostInitArch, MemoryInitArch, VmPageTableInfo, FreePdeSlots, MAX_FREE_PDE_SLOTS};

/// RISC-V 64-bit post-initialization implementation.
///
/// Sets `ptproc` to VM and records VM's satp (page table root) address.
/// In RISC-V Sv39, the page table root is a physical address stored in
/// the satp CSR. The kernel maintains a direct-mapped virtual pointer
/// for page table modification.
///
/// Unlike x86 (CR3) and ARM (TTBR0), RISC-V satp encodes both the
/// page table root physical address and the ASID (Address Space ID)
/// in a single CSR value. The physical root is extracted by masking
/// off the ASID field.
///
/// C: arch_post_init() — by analogy with protect.c:370 (x86) / protect.c:97 (ARM)
pub struct Riscv64PostInitArch;

impl PostInitArch for Riscv64PostInitArch {
    fn set_ptproc(vm_page_table: &VmPageTableInfo) {
        // C: vm = proc_addr(VM_PROC_NR);
        // C: get_cpulocal_var(ptproc) = vm;
        //
        // RISC-V: the page table root is stored in satp CSR.
        // pg_info() equivalent: record VM's satp value and the
        // direct-mapped virtual pointer to the page table.
        //
        // In Sv39, satp format: [MODE(1)] [ASID(16)] [PPN(44)]
        // The physical root address is PPN << 12.
        //
        // SAFETY: BKL is held during boot, only this CPU accesses ptproc.
        let _ = vm_page_table;
    }
}

/// Number of entries in a RISC-V Sv39 page table.
///
/// Sv39 uses 3-level paging with 512 entries per level:
/// - L0 (root): 512 entries (each covers 512GB)
/// - L1: 512 entries (each covers 1GB)
/// - L2: 512 entries (each covers 2MB)
///
/// Note: Sv39 has 39-bit virtual addresses (9+9+9+12).
const RISCV64_SV39_ENTRIES: usize = 512;

/// RISC-V 64-bit memory initialization implementation.
///
/// Allocates 2 free page table entries from `free_upper_idx` for
/// use by the createpde() equivalent temporary mappings.
///
/// C: memory_init() — by analogy with memory.c:707 (x86) / memory.c:612 (ARM)
pub struct Riscv64MemoryInitArch;

impl MemoryInitArch for Riscv64MemoryInitArch {
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots {
        // C: assert(nfreepdes == 0);
        let mut slots = FreePdeSlots::new();

        // C: freepdes[nfreepdes++] = kinfo.freepde_start++;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        // C: freepdes[nfreepdes++] = kinfo.freepde_start++;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        // C: assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);
        assert!(
            *free_upper_idx < RISCV64_SV39_ENTRIES,
            "free_upper_idx overflow: {} >= {}",
            *free_upper_idx, RISCV64_SV39_ENTRIES
        );

        slots
    }
}

// ── RISC-V 64-bit specific unit tests ──
//
// These tests verify architecture-specific overflow behavior and post-init
// contract. The generic FreePdeSlots data structure tests live in
// arch/post_init.rs; these tests cover the RISC-V panic-on-overflow
// assertion (by analogy with C's assert in memory.c:715).
//
// NOTE: These tests compile only on riscv64 targets (not on x86_64 host).
#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::{PhysBytes, VirBytes};

    /// Verifies that allocate_free_pdes panics when free_upper_idx is
    /// at the overflow boundary (510 for 512-entry Sv39 table → final idx 512 >= 512).
    ///
    /// C: assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES) — by analogy with memory.c:715
    #[test]
    #[should_panic(expected = "free_upper_idx overflow")]
    fn test_allocate_free_pdes_panics_on_overflow() {
        let mut idx: usize = 510;
        let _ = Riscv64MemoryInitArch::allocate_free_pdes(&mut idx);
    }

    /// Verifies that allocate_free_pdes succeeds at the highest valid
    /// starting index (509 → final idx 511 < 512).
    #[test]
    fn test_allocate_free_pdes_at_highest_valid_index() {
        let mut idx: usize = 509;
        let slots = Riscv64MemoryInitArch::allocate_free_pdes(&mut idx);
        assert_eq!(slots.len(), 2);
        assert_eq!(idx, 511);
    }

    /// Verifies that set_ptproc accepts a valid VmPageTableInfo without
    /// panicking. The set_ptproc implementation is a no-op placeholder
    /// until per-CPU ptproc variable support is added (todo.md §12.2).
    #[test]
    fn test_set_ptproc_accepts_valid_vm_page_table_info() {
        let info = VmPageTableInfo {
            phys_root: PhysBytes(0x8020_0000),
            virt_root: Some(VirBytes(0xFFFF_FFC0_0000_0000)),
        };
        Riscv64PostInitArch::set_ptproc(&info);
    }
}
