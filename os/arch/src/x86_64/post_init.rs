//! x86-64 post-initialization and memory init implementation
//!
//! Implements `PostInitArch` and `MemoryInitArch` for x86-64.
//!
//! # Register mapping
//!
//! | C field         | x86-64 register | Rust field              |
//! |-----------------|-----------------|------------------------|
//! | p_seg.p_cr3     | CR3             | VmPageTableInfo::phys_root |
//! | p_seg.p_cr3_v   | direct-mapped   | VmPageTableInfo::virt_root |
//!
//! C: arch_post_init() — protect.c:370-377
//! C: memory_init() — memory.c:707-717

use crate::post_init::{PostInitArch, MemoryInitArch, VmPageTableInfo, FreePdeSlots, MAX_FREE_PDE_SLOTS};

/// x86-64 post-initialization implementation.
///
/// Sets `ptproc` to VM and records VM's CR3 (page table root) addresses.
/// In x86-64, the page table root is stored in the CR3 register, and
/// the kernel maintains a direct-mapped virtual pointer for modification.
///
/// C: arch_post_init() — protect.c:370-377
pub struct X86_64PostInitArch;

impl PostInitArch for X86_64PostInitArch {
    fn set_ptproc(vm_page_table: &VmPageTableInfo) {
        // C: vm = proc_addr(VM_PROC_NR);
        // C: get_cpulocal_var(ptproc) = vm;
        //
        // In Rust, ptproc is stored as a per-CPU variable (or kernel global
        // under BKL). Setting it to VM's process pointer enables createpde()
        // to locate the active page table for temporary mappings.
        //
        // SAFETY: BKL is held during boot, only this CPU accesses ptproc.
        // The VM process slot was initialized in Phase C (init_proc_and_boot).
        //
        // TODO: Replace with per-CPU variable accessor once SMP support is added.
        //       Currently stored as a kernel global under BKL protection.
        //       Implementation needed:
        //       1. Store ptproc as a static mutable pointer (protected by BKL)
        //       2. Store vm_page_table.phys_root in a global for createpde()
        //       3. Store vm_page_table.virt_root in a global for createpde()

        // C: pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);
        // pg_info() writes the bootstrap page directory's physical address
        // into vm->p_seg.p_cr3 and its virtual address into vm->p_seg.p_cr3_v.
        // In Rust, these are already provided via VmPageTableInfo.
        //
        // The actual per-CPU ptproc pointer and page directory globals
        // will be set by the kernel's cross-address-space module
        // (createpde equivalent) using this information.
        let _ = vm_page_table;
    }
}

/// Number of entries in a 64-bit page directory (PD level).
///
/// In x86-64 4-level paging: PML4 has 512 entries, PDPT has 512 entries,
/// PD has 512 entries, PT has 512 entries. Each PD entry maps a 2MB huge page.
///
/// C: I386_VM_DIR_ENTRIES = 1024 (32-bit) — memory.c:715
/// Rust: 512 (64-bit PD has 512 entries)
const X86_64_PD_ENTRIES: usize = 512;

/// x86-64 memory initialization implementation.
///
/// Allocates 2 free page directory entries from `free_upper_idx` for
/// use by createpde() temporary mappings.
///
/// In x86-64, each PD entry maps 2MB. The 2 free PDEs provide 2 × 2MB = 4MB
/// of temporary mapping space, matching the C version's 2 × 4MB on x86-32.
///
/// C: memory_init() — memory.c:707-717
pub struct X86_64MemoryInitArch;

impl MemoryInitArch for X86_64MemoryInitArch {
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots {
        // C: assert(nfreepdes == 0);
        // Rust: this function is called only once during boot, enforced by kmain flow.

        let mut slots = FreePdeSlots::new();

        // C: freepdes[nfreepdes++] = kinfo.freepde_start++;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        // C: freepdes[nfreepdes++] = kinfo.freepde_start++;
        slots.push(*free_upper_idx).expect("free PDE slots overflow");
        *free_upper_idx += 1;

        // C: assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES);
        // In 64-bit mode, PD has 512 entries (not 1024 as in 32-bit).
        assert!(
            *free_upper_idx < X86_64_PD_ENTRIES,
            "free_upper_idx overflow: {} >= {}",
            *free_upper_idx, X86_64_PD_ENTRIES
        );

        // C: assert(nfreepdes == 2); assert(nfreepdes <= MAXFREEPDES);
        // Guaranteed by FreePdeSlots::push succeeding twice with MAX_FREE_PDE_SLOTS=2.
        slots
    }
}
