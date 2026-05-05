//! Paging extension traits
//!
//! Optional page table feature extensions, not supported by all architectures:
//! - `PagingWithId`: TLB process identification (PCID/ASID)
//! - `HugePages`: huge page support
//! - `VmPagingExt`: VM process management operations
//!
//! # Future extensions
//!
//! The following features will be added to this module when actual demand arises:
//!
//! - **`clear_dirty()`**: Page replacement algorithms (Clock/LRU) need to periodically
//!   clear the dirty bit to detect pages that have been re-written. Currently this can
//!   be done via `update_flags()`, but a dedicated method could use atomic RMW
//!   operations on x86-64 (e.g., `LOCK CMPXCHG16B`), avoiding loss of hardware
//!   updates during the read-modify-write cycle.

use minix_types::{Endpoint, PhysBytes, VirBytes};
use crate::paging::{PageFlags, PageTableError, Paging};

/// VM process management paging operations (optional trait)
///
/// These operations correspond to Minix3's `pt_bind()` and `pt_mapkernel()`.
/// They are VM policy operations, not pure paging hardware mechanisms:
///
/// - `bind_to_process()`: involves kernel IPC (`sys_vmctl_set_addrspace`),
///   not a pure page table operation
/// - `map_kernel()`: writes kernel mappings into a user process page table,
///   a VM strategy decision rather than hardware mechanism
///
/// Separated from `Paging` trait because they are VM-layer policy,
/// not hardware abstraction.
pub trait VmPagingExt: Paging {
    /// Bind this page table to a process in the kernel.
    ///
    /// Corresponds to Minix3's `pt_bind()` which calls
    /// `sys_vmctl_set_addrspace()` to register the page table
    /// with the kernel for the given process endpoint.
    fn bind_to_process(&self, endpoint: Endpoint) -> Result<(), PageTableError>;

    /// Map kernel address space into this page table.
    ///
    /// Corresponds to Minix3's `pt_mapkernel()`. Must be called
    /// after `Paging::new()` before the process runs, so that
    /// kernel entry points are accessible from user space.
    fn map_kernel(&mut self) -> Result<(), PageTableError>;
}

/// TLB process identification support (optional trait)
///
/// Tags TLB entries with a process identifier, avoiding a full TLB flush
/// on context switch and significantly reducing TLB miss overhead.
///
/// **Note**: This trait does not define ASID/PCID lifecycle semantics or
/// reuse strategies. ASID allocation/recycling policy, TLB shootdown
/// consistency maintenance, and generation counter mechanisms are the
/// responsibility of the upper VM manager. This trait only provides
/// low-level hardware operation primitives.
pub trait PagingWithId: Paging {
    type AddressSpaceId: Copy + Eq + core::fmt::Debug + Send;

    /// Allocate an Address Space ID (ASID/PCID).
    ///
    /// The `&self` receiver allows the implementation to decide the allocation
    /// strategy: it may use a global pool, a per-CPU pool, or a per-page-table
    /// pool. Minix3 does not implement PCID/ASID management, so there is no
    /// direct C counterpart. The allocation strategy is an implementation detail
    /// left to each architecture.
    fn alloc_asid(&self) -> Result<Self::AddressSpaceId, PageTableError>;

    /// Free a previously allocated ASID/PCID.
    ///
    /// The `&self` receiver mirrors `alloc_asid` so that the same allocation
    /// context is used for both operations.
    fn free_asid(&self, id: Self::AddressSpaceId);

    /// Activate this page table with the given ASID on the current CPU.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - The page table is fully initialized (kernel mappings present)
    /// - The ASID was allocated via `alloc_asid()` and has not been freed
    /// - On SMP systems, proper TLB shootdown is performed if needed
    unsafe fn switch_with_asid(&self, id: Self::AddressSpaceId);

    /// Flush TLB entries for the given ASID.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - This is called in a valid MMU context (a page table is active)
    /// - The ASID is valid and currently in use
    unsafe fn flush_tlb_asid(&self, id: Self::AddressSpaceId);

    /// Flush the TLB entry for a single virtual address within the given ASID.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// - `vaddr` falls within the currently active page table's valid range
    /// - The ASID is valid and currently in use
    unsafe fn flush_tlb_addr_asid(&self, vaddr: VirBytes, id: Self::AddressSpaceId);
}

/// Huge page support (optional trait)
pub trait HugePages: Paging {
    const HUGE_PAGE_SIZES: &'static [usize];

    fn map_huge(
        &mut self,
        vaddr: VirBytes,
        paddr: PhysBytes,
        size: usize,
        flags: PageFlags,
    ) -> Result<(), PageTableError>;

    fn supports_huge_page(size: usize) -> bool {
        Self::HUGE_PAGE_SIZES.contains(&size)
    }
}
