//! Page table management module.
//!
//! Provides architecture-agnostic page table types and helpers.
//! Hardware-specific page table operations are abstracted through
//! the `Paging` trait in `minix_arch`.
//!
//! # Design
//!
//! This module does NOT expose hardware-specific details (PDE/PTE bit fields,
//! page table indices, etc.). Those belong in `minix_arch` implementations.
//! VM code should use `Paging` trait methods and `PageFlags` for all
//! page table operations.

use minix_types::VirBytes;

pub(crate) type PageTable = minix_arch::CurrentPaging;

pub(crate) use minix_arch::paging::PageFlags;
pub(crate) use minix_arch::paging::PageTableError;
pub(crate) use minix_arch::paging::Paging;

pub(crate) fn page_align(addr: VirBytes) -> VirBytes {
    let ps = <PageTable as Paging>::PAGE_SIZE as u64;
    VirBytes((addr.0 + ps - 1) & !(ps - 1))
}

pub(crate) fn page_align_down(addr: VirBytes) -> VirBytes {
    let ps = <PageTable as Paging>::PAGE_SIZE as u64;
    VirBytes(addr.0 & !(ps - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_align() {
        assert_eq!(page_align(VirBytes(0x1234)), VirBytes(0x2000));
        assert_eq!(page_align(VirBytes(0x1000)), VirBytes(0x1000));
        assert_eq!(page_align_down(VirBytes(0x1234)), VirBytes(0x1000));
    }

    #[test]
    fn test_page_size_from_trait() {
        assert_eq!(<PageTable as Paging>::PAGE_SIZE, 4096);
    }
}
