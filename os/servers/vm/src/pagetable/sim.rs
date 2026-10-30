//! Software-simulated page table for host unit tests.
//!
//! Full `Paging` trait implementation backed by a `BTreeMap` — the same
//! idea as Redox rmm's `EmulateArch` ("testing memory management with
//! software emulation", rmm README): real allocation/aggregation logic
//! runs, only the hardware translation layer is simulated. This lets VM
//! handlers exercise real map/unmap/query flows in `cargo test` instead
//! of skipping them.
//!
//! cfg(test)-only: production builds keep the arch `CurrentPaging`
//! (`pagetable/mod.rs` swaps the `PageTable` alias).

use core::cell::Cell;

use alloc::collections::BTreeMap;

use minix_arch::paging::{PageFlags, PageTableError};
use minix_types::{PhysBytes, VirBytes};

use super::Paging;

/// Software page table: `vaddr → (paddr, flags)` per 4 KiB page.
#[derive(Debug, Default)]
pub struct SimPaging {
    /// Fake root physical address (reported by `root_paddr`).
    root: u64,
    /// vaddr → (paddr, flags), keyed by page base.
    entries: BTreeMap<u64, (u64, PageFlags)>,
    /// TLB flush count — incremented by `flush_tlb()` and
    /// `flush_tlb_addr()` (it only records, it forces no hardware
    /// effect). `Cell` because the trait methods take `&self` (hardware
    /// MMU operations likewise need no `&mut`). Nothing reads this field
    /// yet: it stays because it is the stub's only observable counter of
    /// hardware actions, and a consumer (a test that swaps `SimPaging`
    /// in for the real page table) will consume it directly once it
    /// asserts on the flush count.
    pub flush_count: core::cell::Cell<u32>,
}

impl SimPaging {
    /// Fresh simulated table with a synthetic root.
    pub fn new() -> Self {
        Self {
            root: 0xCAFE_0000,
            entries: BTreeMap::new(),
            flush_count: core::cell::Cell::new(0),
        }
    }

    /// Number of live mappings.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Paging for SimPaging {
    const PAGE_SIZE: usize = 4096;

    fn phys_addr_limit() -> Result<u64, PageTableError> {
        Ok(1u64 << 52)
    }

    fn validate_virtual_range(base: u64, len: u64) -> Result<(), PageTableError> {
        let end = base
            .checked_add(len)
            .ok_or(PageTableError::InvalidAddress)?;
        let canonical = |va: u64| {
            let top = va >> 47;
            top == 0 || top == 0x1_FFFF
        };
        if !canonical(base) || (len > 0 && !canonical(end - 1)) {
            return Err(PageTableError::InvalidAddress);
        }
        Ok(())
    }

    fn new() -> Result<Self, PageTableError> {
        Ok(Self::new())
    }

    fn new_from_page(root_page: PhysBytes) -> Self {
        let mut sim = Self::new();
        sim.root = root_page.0;
        sim
    }

    fn from_active_root(root_phys: PhysBytes) -> Self {
        let mut sim = Self::new();
        sim.root = root_phys.0;
        sim
    }

    fn adopt_active_root(root_phys: PhysBytes) -> Self {
        let mut sim = Self::new();
        sim.root = root_phys.0;
        sim
    }

    unsafe fn enable(&self) -> PhysBytes {
        PhysBytes(self.root)
    }

    unsafe fn destroy(&mut self) {
        self.entries.clear();
    }

    fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<(), PageTableError>
    {
        let page = vaddr.0 & !4095;
        if self.entries.contains_key(&page) {
            return Err(PageTableError::AlreadyMapped);
        }
        self.entries.insert(page, (paddr.0, flags));
        Ok(())
    }

    fn remap(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
        -> Result<Option<(PhysBytes, PageFlags)>, PageTableError>
    {
        let page = vaddr.0 & !4095;
        let old = self.entries.insert(page, (paddr.0, flags));
        Ok(old.map(|(p, f)| (PhysBytes(p), f)))
    }

    fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError> {
        let page = vaddr.0 & !4095;
        let (paddr, _) = self.entries.remove(&page)
            .ok_or(PageTableError::NotMapped)?;
        Ok(PhysBytes(paddr))
    }

    fn update_flags(&mut self, vaddr: VirBytes, flags: PageFlags) -> Result<(), PageTableError> {
        let page = vaddr.0 & !4095;
        let entry = self.entries.get_mut(&page).ok_or(PageTableError::NotMapped)?;
        entry.1 = flags;
        Ok(())
    }

    fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)> {
        self.entries.get(&(vaddr.0 & !4095)).map(|(p, f)| (PhysBytes(*p), *f))
    }

    fn root_paddr(&self) -> PhysBytes {
        PhysBytes(self.root)
    }

    unsafe fn flush_tlb(&self) {
        self.flush_count.set(self.flush_count.get() + 1);
    }

    unsafe fn flush_tlb_addr(&self, _vaddr: VirBytes) {
        self.flush_count.set(self.flush_count.get() + 1);
    }
}
