//! Post-initialization and memory init architecture abstraction
//!
//! Defines trait interfaces for the two Phase D operations:
//! - `PostInitArch`: Architecture-specific post-initialization (arch_post_init)
//! - `MemoryInitArch`: Architecture-specific memory initialization (memory_init)
//!
//! # Design decisions (see 06-arch-post-init.md §3)
//!
//! - **Two-trait split** (§3.1-3.2): `PostInitArch` handles the ptproc/VM
//!   page table registration, `MemoryInitArch` handles free page directory
//!   entry allocation for createpde.
//! - **OS-semantic types**: `VmPageTableInfo` wraps the architecture-specific
//!   page table root addresses, avoiding raw register types in the trait API.
//! - **FreePdeSlots replaces freepdes[]** (§3.2): C uses a static array with
//!   assertions; Rust uses a fixed-size array with compile-time capacity const.

use minix_types::{PhysBytes, VirBytes};

/// Information about a process's page table, as needed by the kernel
/// for cross-address-space operations (createpde, lin_lin_copy, etc.).
///
/// This is the Rust equivalent of what C stores in `proc.p_seg.p_cr3` (x86)
/// or `proc.p_seg.p_ttbr` (ARM), plus the kernel-virtual pointer to the
/// page directory (`p_cr3_v` / `p_ttbr_v`).
///
/// # Architecture mapping
///
/// | Field        | x86-64                | ARM64              | RISC-V       |
/// |-------------|-----------------------|--------------------|------------- |
/// | phys_root   | p_cr3 (CR3 value)     | p_ttbr (TTBR0)     | satp value   |
/// | virt_root   | p_cr3_v (virt ptr)    | p_ttbr_v (virt ptr)| N/A*         |
///
/// *RISC-V: the Sv39 page table root is a physical address stored in satp;
/// the kernel maintains its own direct-map pointer for manipulation.
pub struct VmPageTableInfo {
    /// Physical address of the page table root.
    /// C: vm->p_seg.p_cr3 (x86) / vm->p_seg.p_ttbr (ARM)
    pub phys_root: PhysBytes,
    /// Kernel-virtual address of the page table root (for direct modification).
    /// C: vm->p_seg.p_cr3_v (x86) / vm->p_seg.p_ttbr_v (ARM)
    pub virt_root: Option<VirBytes>,
}

/// Architecture abstraction for post-initialization.
///
/// Called after `init_proc_and_boot()` (Phase C) to register the VM process
/// as the "page table process" (ptproc) and record its page table location.
///
/// This is necessary because `createpde()` — the kernel's mechanism for
/// temporarily mapping another process's memory — needs to know which process
/// owns the currently active page table, and where that page table lives in
/// both physical and virtual memory.
///
/// # Why ptproc matters
///
/// When the kernel needs to access a user process's memory (e.g., to copy
/// data during IPC), it uses `createpde()` to temporarily insert that
/// process's page directory entry into the **currently active page table**.
/// The "currently active page table" belongs to `ptproc` (which is VM during
/// normal operation). Knowing `ptproc` allows `createpde()` to:
/// 1. Directly access memory of `ptproc` or kernel (no mapping needed)
/// 2. Map other processes' memory by inserting their PDEs into ptproc's table
///
/// C: arch_post_init() — protect.c:370 (x86) / protect.c:97 (ARM)
pub trait PostInitArch {
    /// Register VM as the page table process and record its page table info.
    ///
    /// This performs two things:
    /// 1. Sets the per-CPU `ptproc` variable to point to VM's proc struct
    /// 2. Calls `pg_info()` to record VM's page table physical and virtual addresses
    ///
    /// After this call, `createpde()` and `switch_address_space()` can
    /// correctly locate and modify the active page table.
    ///
    /// # Arguments
    ///
    /// * `vm_page_table` - VM's page table physical root and virtual root addresses
    fn set_ptproc(vm_page_table: &VmPageTableInfo);
}

/// Maximum number of free page directory entries (PDEs) that the kernel
/// reserves for `createpde()` temporary mappings.
///
/// C: `#define MAXFREEPDES 2` — memory.c:30 (x86) / memory.c:27 (ARM)
///
/// The value 2 is sufficient because `createpde()` uses at most 2 temporary
/// PDEs per operation: one for the source process and one for the destination.
/// This is an architectural constant, not configurable.
pub const MAX_FREE_PDE_SLOTS: usize = 2;

/// Free page directory entry slots reserved for `createpde()`.
///
/// In C, this is the `freepdes[]` static array + `nfreepdes` counter.
/// In Rust, we use a fixed-size array with a length field, providing
/// the same semantics with bounds checking.
///
/// # Lifecycle
///
/// 1. Created empty by `MemoryInitArch::allocate_free_pdes()`
/// 2. `createpde()` acquires slots temporarily, maps foreign PDEs into them
/// 3. `mem_clear_mapcache()` releases all slots by clearing the PDEs
pub struct FreePdeSlots {
    /// The actual page directory indices.
    slots: [usize; MAX_FREE_PDE_SLOTS],
    /// Number of slots currently allocated.
    len: usize,
}

impl FreePdeSlots {
    /// Create an empty FreePdeSlots.
    pub const fn new() -> Self {
        Self {
            slots: [0; MAX_FREE_PDE_SLOTS],
            len: 0,
        }
    }

    /// Push a new free PDE index.
    ///
    /// Returns `Err(index)` if the slots are already full.
    pub fn push(&mut self, pde_index: usize) -> Result<(), usize> {
        if self.len >= MAX_FREE_PDE_SLOTS {
            return Err(pde_index);
        }
        self.slots[self.len] = pde_index;
        self.len += 1;
        Ok(())
    }

    /// Get the PDE index at the given slot position.
    pub fn get(&self, idx: usize) -> Option<usize> {
        if idx < self.len {
            Some(self.slots[idx])
        } else {
            None
        }
    }

    /// Number of allocated slots.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether no slots have been allocated yet.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Iterate over the allocated PDE indices.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.slots[..self.len].iter().copied()
    }
}

/// Architecture abstraction for memory initialization.
///
/// Allocates free page directory entries (PDEs) for `createpde()` temporary
/// mappings. These PDEs are "scratch space" in the currently active page
/// table that the kernel can use to temporarily map another process's page
/// table entries.
///
/// # What are free PDEs?
///
/// `createpde()` is the kernel's mechanism for accessing memory that belongs
/// to a process whose page table is not currently loaded. It works by:
/// 1. Taking the target process's page directory entry (PDE)
/// 2. Writing it into a "free" PDE slot in the **currently active** page table
/// 3. Now the kernel can access that memory through the linear address
/// 4. After the operation, the temporary PDE is cleared
///
/// This is necessary because x86-32 uses 2-level paging (PDE→PTE), and
/// the kernel can only access pages that are reachable from the CR3 register
/// currently loaded. In 64-bit mode with 4-level paging, the mechanism
/// changes but the concept remains: the kernel needs temporary slots in
/// the active page table structure.
///
/// C: memory_init() — memory.c:707 (x86) / memory.c:612 (ARM)
pub trait MemoryInitArch {
    /// Allocate free page directory entries from `kinfo.free_upper_idx`.
    ///
    /// Reserves `MAX_FREE_PDE_SLOTS` (2) consecutive page directory entries
    /// starting from `free_upper_idx`, and returns them as a `FreePdeSlots`.
    ///
    /// The `free_upper_idx` is advanced by `MAX_FREE_PDE_SLOTS` after allocation.
    ///
    /// # Arguments
    ///
    /// * `free_upper_idx` - The first available page directory index after
    ///   identity and kernel mappings. Comes from `KernelInfo::free_upper_idx`.
    ///
    /// # Returns
    ///
    /// A `FreePdeSlots` containing the allocated PDE indices.
    ///
    /// # Panics
    ///
    /// Panics if `free_upper_idx + MAX_FREE_PDE_SLOTS` exceeds the
    /// architecture's maximum page directory entries.
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots;
}

// ── Mock implementations ──

#[cfg(feature = "mock")]
pub struct MockPostInitArch;

#[cfg(feature = "mock")]
impl PostInitArch for MockPostInitArch {
    fn set_ptproc(vm_page_table: &VmPageTableInfo) {
        let _ = vm_page_table;
    }
}

#[cfg(feature = "mock")]
pub struct MockMemoryInitArch;

#[cfg(feature = "mock")]
impl MemoryInitArch for MockMemoryInitArch {
    fn allocate_free_pdes(free_upper_idx: &mut usize) -> FreePdeSlots {
        let mut slots = FreePdeSlots::new();
        slots.push(*free_upper_idx).unwrap();
        *free_upper_idx += 1;
        slots.push(*free_upper_idx).unwrap();
        *free_upper_idx += 1;
        slots
    }
}
