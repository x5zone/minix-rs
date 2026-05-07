use alloc::boxed::Box;

use minix_types::VirBytes;
use minix_arch::paging::PageFlags;

use crate::phys_mem::{PhysAllocator, PageAllocFlags, AllocError, PhysBytes};

const PAGE_SIZE: usize = 4096;

pub(crate) trait PtOps {
    fn write_pte(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn write_pde(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn write_pdpte(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn read_pte(&self, table_virt: VirBytes, index: usize) -> u64;
    fn read_pde(&self, table_virt: VirBytes, index: usize) -> u64;
    fn zero_table(&mut self, table_virt: VirBytes);
}

pub(crate) struct RealPtOps;

impl PtOps for RealPtOps {
    fn write_pte(&mut self, table_virt: VirBytes, index: usize, entry: u64) {
        unsafe {
            let ptr = (table_virt.0 as *mut u64).add(index);
            ptr.write_volatile(entry);
        }
    }

    fn write_pde(&mut self, table_virt: VirBytes, index: usize, entry: u64) {
        unsafe {
            let ptr = (table_virt.0 as *mut u64).add(index);
            ptr.write_volatile(entry);
        }
    }

    fn write_pdpte(&mut self, table_virt: VirBytes, index: usize, entry: u64) {
        unsafe {
            let ptr = (table_virt.0 as *mut u64).add(index);
            ptr.write_volatile(entry);
        }
    }

    fn read_pte(&self, table_virt: VirBytes, index: usize) -> u64 {
        unsafe {
            let ptr = (table_virt.0 as *const u64).add(index);
            ptr.read_volatile()
        }
    }

    fn read_pde(&self, table_virt: VirBytes, index: usize) -> u64 {
        unsafe {
            let ptr = (table_virt.0 as *const u64).add(index);
            ptr.read_volatile()
        }
    }

    fn zero_table(&mut self, table_virt: VirBytes) {
        unsafe {
            core::ptr::write_bytes(table_virt.0 as *mut u8, 0, PAGE_SIZE);
        }
    }
}

pub(crate) struct PtRegion<O: PtOps> {
    start: VirBytes,
    mapped_end: VirBytes,
    next: VirBytes,
    pd_page: VirBytes,
    current_pt: VirBytes,
    current_pt_base: VirBytes,
    phys_alloc: Box<dyn PhysAllocator>,
    pt_ops: O,
}

impl PtRegion<RealPtOps> {
    pub(crate) fn from_reserved(
        reserved: &ReservedRegion,
        phys_alloc: Box<dyn PhysAllocator>,
    ) -> Self {
        Self::from_reserved_with_ops(reserved, phys_alloc, RealPtOps)
    }
}

impl<O: PtOps> PtRegion<O> {
    pub(crate) fn from_reserved_with_ops(
        reserved: &ReservedRegion,
        phys_alloc: Box<dyn PhysAllocator>,
        pt_ops: O,
    ) -> Self {
        let start = reserved.alloc_contig_virt(3);
        let pd_page = VirBytes(start.0 + PAGE_SIZE as u64);
        let current_pt = VirBytes(start.0 + 2 * PAGE_SIZE as u64);

        let pdpt_phys = reserved.virt_to_phys(start);
        let pd_phys = reserved.virt_to_phys(pd_page);
        let pt0_phys = reserved.virt_to_phys(current_pt);

        let mut region = PtRegion {
            start,
            mapped_end: VirBytes(start.0 + 3 * PAGE_SIZE as u64),
            next: VirBytes(start.0 + 3 * PAGE_SIZE as u64),
            pd_page,
            current_pt,
            current_pt_base: start,
            phys_alloc,
            pt_ops,
        };

        region.pt_ops.write_pdpte(
            start,
            0,
            pdpt_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64,
        );

        region.pt_ops.write_pde(
            pd_page,
            0,
            pd_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64,
        );

        region.pt_ops.zero_table(current_pt);

        region
    }

    pub(crate) fn alloc_pt_page(&mut self) -> Option<VirBytes> {
        if self.remaining() < 8 {
            self.expand()?;
        }
        let virt = self.next;
        self.next = VirBytes(self.next.0 + PAGE_SIZE as u64);
        Some(virt)
    }

    pub(crate) fn alloc_phys(
        &mut self, clicks: usize, flags: PageAllocFlags,
    ) -> Result<PhysBytes, AllocError> {
        self.phys_alloc.alloc_mem(clicks, flags)
    }

    pub(crate) fn free_phys(&mut self, base: PhysBytes, clicks: usize) {
        self.phys_alloc.free_mem(base, clicks);
    }

    fn remaining(&self) -> usize {
        let slots_per_pt: usize = 512;
        let used = (self.next.0 - self.current_pt_base.0) as usize / PAGE_SIZE;
        slots_per_pt.saturating_sub(used)
    }

    fn expand(&mut self) -> Option<()> {
        let pt_phys = self.phys_alloc.alloc_mem(1, PageAllocFlags::empty()).ok()?;

        let pt_virt = self.next;
        self.next = VirBytes(self.next.0 + PAGE_SIZE as u64);

        let pd_idx = (pt_virt.0 - self.start.0) as usize / (512 * PAGE_SIZE);
        let current_pd_idx = (self.current_pt.0 - self.start.0) as usize / (512 * PAGE_SIZE);

        if pd_idx == current_pd_idx {
            let pte_idx = (pt_virt.0 - self.current_pt.0) as usize / PAGE_SIZE;
            self.pt_ops.write_pte(
                self.current_pt,
                pte_idx,
                pt_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64,
            );
        }

        self.pt_ops.write_pde(
            self.pd_page,
            pd_idx,
            pt_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64,
        );

        self.pt_ops.zero_table(pt_virt);

        self.current_pt = pt_virt;
        self.current_pt_base = VirBytes(self.start.0 + (pd_idx * 512 * PAGE_SIZE) as u64);

        self.pt_ops.write_pte(
            pt_virt,
            0,
            pt_phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64,
        );

        self.mapped_end = VirBytes(self.mapped_end.0 + 512 * PAGE_SIZE as u64);

        Some(())
    }

    fn write_data_pte(&mut self, virt: VirBytes, phys: PhysBytes) {
        let pte_idx = (virt.0 - self.current_pt_base.0) as usize / PAGE_SIZE;
        self.pt_ops.write_pte(
            self.current_pt,
            pte_idx,
            phys.as_u64() | PageFlags::PRESENT.bits() as u64 | PageFlags::WRITABLE.bits() as u64,
        );
    }

    pub(crate) fn relocate_phys_allocator(&mut self) {
        let count = self.phys_alloc.reloc_array_count();
        if count == 0 {
            return;
        }

        let mut new_virts: [Option<VirBytes>; 4] = [None; 4];
        let mut total_bytes: [usize; 4] = [0; 4];

        for i in 0..count {
            let (_old_ptr, elem_count, elem_size) = self.phys_alloc.reloc_array_info(i);
            let bytes = elem_count * elem_size;
            total_bytes[i] = bytes;
            let pages = (bytes + PAGE_SIZE - 1) / PAGE_SIZE;

            for _ in 0..pages {
                let phys = self.phys_alloc.alloc_mem(1, PageAllocFlags::empty())
                    .expect("relocation: failed to allocate physical page");
                let virt = self.alloc_pt_page()
                    .expect("relocation: failed to allocate virtual address");
                if new_virts[i].is_none() {
                    new_virts[i] = Some(virt);
                }
                self.write_data_pte(virt, phys);
            }
        }

        for i in 0..count {
            let (old_ptr, _elem_count, _elem_size) = self.phys_alloc.reloc_array_info(i);
            let new_virt = new_virts[i].unwrap();
            unsafe {
                core::ptr::copy_nonoverlapping(old_ptr, new_virt.0 as *mut u8, total_bytes[i]);
            }
        }

        let new_ptrs: [*mut u8; 4] = [
            new_virts[0].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[1].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[2].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[3].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
        ];
        self.phys_alloc.update_relocated_arrays(&new_ptrs[..count]);
    }
}

pub(crate) struct ReservedRegion {
    phys_start: PhysBytes,
    virt_start: VirBytes,
    total_pages: usize,
    allocated_pages: usize,
    bitmap: u64,
}

impl ReservedRegion {
    pub(crate) fn new(phys_start: PhysBytes, virt_start: VirBytes, total_pages: usize) -> Self {
        assert!(total_pages <= 64, "reserved region too large for u64 bitmap");
        Self {
            phys_start,
            virt_start,
            total_pages,
            allocated_pages: 0,
            bitmap: 0,
        }
    }

    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        let free_bit = (!self.bitmap).trailing_zeros() as usize;
        if free_bit >= self.total_pages {
            return None;
        }

        self.bitmap |= 1 << free_bit;
        self.allocated_pages += 1;

        let offset = free_bit * PAGE_SIZE;
        let virt = VirBytes(self.virt_start.0 + offset as u64);
        let phys = self.phys_start.add(offset);

        Some((virt, phys))
    }

    pub(crate) fn alloc_contig_virt(&self, pages: usize) -> VirBytes {
        assert!(pages <= self.total_pages - self.allocated_pages);
        let offset = self.allocated_pages * PAGE_SIZE;
        VirBytes(self.virt_start.0 + offset as u64)
    }

    pub(crate) fn virt_to_phys(&self, virt: VirBytes) -> PhysBytes {
        let offset = (virt.0 - self.virt_start.0) as usize;
        self.phys_start.add(offset)
    }

    pub(crate) fn contains(&self, virt: VirBytes) -> bool {
        virt.0 >= self.virt_start.0
            && virt.0 < self.virt_start.0 + (self.total_pages * PAGE_SIZE) as u64
    }

    pub(crate) fn allocated_pages(&self) -> usize {
        self.allocated_pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phys_mem::{PhysAllocator, PageAllocFlags, AllocError, PhysBytes};
    use alloc::collections::BTreeMap;

    struct MockPtOps {
        tables: BTreeMap<u64, [u64; 512]>,
    }

    impl MockPtOps {
        fn new() -> Self {
            Self { tables: BTreeMap::new() }
        }

        fn ensure_table(&mut self, virt: VirBytes) -> &mut [u64; 512] {
            self.tables.entry(virt.0).or_insert([0u64; 512])
        }
    }

    impl PtOps for MockPtOps {
        fn write_pte(&mut self, table_virt: VirBytes, index: usize, entry: u64) {
            let table = self.ensure_table(table_virt);
            table[index] = entry;
        }

        fn write_pde(&mut self, table_virt: VirBytes, index: usize, entry: u64) {
            let table = self.ensure_table(table_virt);
            table[index] = entry;
        }

        fn write_pdpte(&mut self, table_virt: VirBytes, index: usize, entry: u64) {
            let table = self.ensure_table(table_virt);
            table[index] = entry;
        }

        fn read_pte(&self, table_virt: VirBytes, index: usize) -> u64 {
            self.tables.get(&table_virt.0).map(|t| t[index]).unwrap_or(0)
        }

        fn read_pde(&self, table_virt: VirBytes, index: usize) -> u64 {
            self.tables.get(&table_virt.0).map(|t| t[index]).unwrap_or(0)
        }

        fn zero_table(&mut self, table_virt: VirBytes) {
            self.tables.insert(table_virt.0, [0u64; 512]);
        }
    }

    struct MockPhysAllocator {
        next: u64,
        total: usize,
    }

    impl MockPhysAllocator {
        fn new(total: usize) -> Self {
            Self { next: 0x1000, total }
        }
    }

    impl PhysAllocator for MockPhysAllocator {
        fn alloc_mem(&mut self, _clicks: usize, _flags: PageAllocFlags) -> Result<PhysBytes, AllocError> {
            if self.total == 0 {
                return Err(AllocError::OutOfMemory);
            }
            let addr = PhysBytes::new(self.next);
            self.next += 0x1000;
            self.total -= 1;
            Ok(addr)
        }

        fn free_mem(&mut self, _base: PhysBytes, _clicks: usize) {
            self.total += 1;
        }

        fn total_count(&self) -> usize {
            self.total
        }
    }

    fn mock_reserved() -> ReservedRegion {
        ReservedRegion::new(
            PhysBytes::new(0x1000),
            VirBytes(0x7000_0000),
            64,
        )
    }

    fn mock_phys_alloc() -> Box<dyn PhysAllocator> {
        Box::new(MockPhysAllocator::new(256))
    }

    #[test]
    fn test_pt_region_alloc_and_expand() {
        let reserved = mock_reserved();
        let phys_alloc = mock_phys_alloc();
        let pt_ops = MockPtOps::new();
        let mut region = PtRegion::from_reserved_with_ops(&reserved, phys_alloc, pt_ops);

        let initial = region.remaining();
        assert!(initial > 0);

        for _ in 0..initial + 10 {
            assert!(region.alloc_pt_page().is_some());
        }
    }

    #[test]
    fn test_reserved_alloc_and_exhaustion() {
        let mut reserved = ReservedRegion::new(
            PhysBytes::new(0x1000),
            VirBytes(0x7000_0000),
            2,
        );

        let a = reserved.alloc_page();
        assert!(a.is_some());

        let b = reserved.alloc_page();
        assert!(b.is_some());

        let c = reserved.alloc_page();
        assert!(c.is_none());
    }

    #[test]
    fn test_reserved_contains() {
        let reserved = ReservedRegion::new(
            PhysBytes::new(0x1000),
            VirBytes(0x7000_0000),
            4,
        );

        assert!(reserved.contains(VirBytes(0x7000_0000)));
        assert!(reserved.contains(VirBytes(0x7000_3FFF)));
        assert!(!reserved.contains(VirBytes(0x7000_4000)));
        assert!(!reserved.contains(VirBytes(0x6FFF_F000)));
    }
}
