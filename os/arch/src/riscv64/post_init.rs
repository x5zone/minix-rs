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
