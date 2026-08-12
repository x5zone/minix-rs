//! Page table entry (PTE) walk architecture abstraction.
//!
//! Defines the `PteWalkArch` trait for performing a **read-only** page
//! table walk to translate a virtual address to a physical address
//! without a live `Paging` instance.
//!
//! # Why a separate trait?
//!
//! The kernel sometimes needs to translate a virtual address in a
//! **foreign** address space (e.g. during cross-space IPC copy). The
//! target process's page table root is known, but the MMU is running
//! the *current* process's page table — so the hardware cannot walk
//! the target's table directly.
//!
//! The solution is an **offline walk**: read the target's PTEs from
//! physical memory via the Direct Map, following the page table
//! hierarchy level by level until a leaf entry is found.
//!
//! # Architecture-specific knowledge
//!
//! Each architecture encodes PTEs differently:
//! - **x86-64**: bit 0 = Present, bit 7 = Page Size (huge), ADDR_MASK = bits 12-51
//! - **aarch64**: bit 0 = Valid, bit 1 = Table/Block, AP/XN inverted permission bits
//! - **riscv64**: bit 0 = V, R/W/X bits determine leaf vs. table pointer, PPN field
//!
//! This trait encapsulates that knowledge so the kernel can walk any
//! architecture's page table without `#[cfg(target_arch)]` dispatch.
//!
//! # Relationship to `Paging::query`
//!
//! `Paging::query(&self, vaddr)` performs the same translation, but
//! requires a `&self` reference to a constructed `Paging` instance.
//! `PteWalkArch::walk(root_paddr, vaddr)` is a standalone function
//! that takes only the root physical address — useful when the caller
//! does not own a `Paging` instance (e.g. walking a foreign process's
//! table from the kernel).
//!
//! Each architecture's `PteWalkArch` implementation reuses the same
//! `walk_read` helper that powers `Paging::query`, ensuring the
//! offline walk and the live walk produce identical results.

use minix_types::{PhysBytes, VirBytes};
use crate::paging::PageFlags;

/// Architecture abstraction for read-only page table walks.
///
/// Implementations translate a virtual address to a physical address
/// by walking the page table hierarchy starting from a given root
/// physical address, reading PTEs via the Direct Map.
///
/// # Safety preconditions (not enforced at compile time)
///
/// - The kernel Direct Map must be active (paging enabled with
///   `KERNEL_DIRECT_MAP_BASE` mapped to PA=0).
/// - `root_paddr` must be the physical address of a valid top-level
///   page table page (PML4 on x86-64, PGD/L0 on aarch64, L2 on Sv39).
///
/// These preconditions are satisfied after `Paging::enable()` returns
/// during boot. Callers must not invoke `walk` before paging is enabled.
pub trait PteWalkArch {
    /// Walk the page table rooted at `root_paddr` to translate `vaddr`.
    ///
    /// Returns `Some((paddr, flags))` if the address is mapped (the
    /// leaf PTE is present/valid), `None` if any level of the walk
    /// encounters a non-present entry.
    ///
    /// The returned `paddr` includes the page offset (i.e. it is the
    /// physical address of the exact byte `vaddr` refers to, not just
    /// the page base).
    ///
    /// # Huge page handling
    ///
    /// If a huge page (1GB or 2MB block descriptor) is encountered at
    /// an intermediate level, the offset within the block is folded
    /// into the returned physical address, and `PageFlags::HUGE_PAGE`
    /// is set in the returned flags.
    fn walk(root_paddr: PhysBytes, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)>;
}

/// Mock PTE walk for testing on architectures without a real implementation.
///
/// Always returns `None` (address not mapped). This is correct for test
/// environments where no real page tables exist — the kernel's
/// `copy_from_user` / `copy_to_user` will return `Fault`, which test
/// callers can handle or assert against.
///
/// On real architectures (`x86_64`, `aarch64`, `riscv64`), the
/// architecture-specific `PteWalkArch` implementation is used instead,
/// so this mock is only active when `feature = "mock"` is enabled AND
/// the target architecture is not one of the three supported ones.
#[cfg(feature = "mock")]
pub struct MockPteWalk;

#[cfg(feature = "mock")]
impl PteWalkArch for MockPteWalk {
    fn walk(_root_paddr: PhysBytes, _vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the trait is object-safe enough for generic usage.
    /// (We only use it as a type-level bound, not dyn-dispatched, but
    /// this test ensures the signature stays callable.)
    #[test]
    fn test_pte_walk_arch_signature() {
        // The trait method takes `PhysBytes, VirBytes` and returns
        // `Option<(PhysBytes, PageFlags)>`. This test is a compile-time
        // check that the trait can be referenced.
        fn _accepts<P: PteWalkArch>() {}
        // No runtime assertion needed — compilation is the test.
    }
}
