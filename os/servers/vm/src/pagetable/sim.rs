//! Software-simulated page table for host unit tests (V11/T21, V11-P2-1).
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
    /// Set by `enable()` / `switch()` (recorded, not enforced). `Cell`
    /// because the trait methods take `&self` (hardware MMU ops don't need
    /// `&mut` either).
    pub enabled_count: core::cell::Cell<u32>,
    pub switch_count: core::cell::Cell<u32>,
    pub flush_count: core::cell::Cell<u32>,
}

impl SimPaging {
    /// Fresh simulated table with a synthetic root.
    pub fn new() -> Self {
        Self {
            root: 0xCAFE_0000,
            entries: BTreeMap::new(),
            enabled_count: core::cell::Cell::new(0),
            switch_count: core::cell::Cell::new(0),
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

    unsafe fn switch(&self) {
        self.switch_count.set(self.switch_count.get() + 1);
    }

    unsafe fn flush_tlb(&self) {
        self.flush_count.set(self.flush_count.get() + 1);
    }

    unsafe fn flush_tlb_addr(&self, _vaddr: VirBytes) {
        self.flush_count.set(self.flush_count.get() + 1);
    }
}
