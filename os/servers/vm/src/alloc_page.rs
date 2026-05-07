use alloc::boxed::Box;
use core::marker::PhantomData;

use minix_types::VirBytes;

use crate::phys_mem::{PhysAllocator, PageAllocFlags, AllocError, PhysBytes};
use crate::pt_region::{PtOps, PtRegion, RealPtOps, ReservedRegion};

pub(crate) struct Bootstrap;
pub(crate) struct Normal;

pub(crate) struct VmPageAllocator<S, O: PtOps = RealPtOps> {
    reserved: ReservedRegion,
    phys_alloc: Option<Box<dyn PhysAllocator>>,
    pt_region: Option<PtRegion<O>>,
    pt_ops: Option<O>,
    _stage: PhantomData<S>,
}

impl<O: PtOps> VmPageAllocator<Bootstrap, O> {
    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        self.reserved.alloc_page()
    }
}

impl VmPageAllocator<Bootstrap, RealPtOps> {
    pub(crate) fn new(
        reserved: ReservedRegion,
        phys_alloc: Box<dyn PhysAllocator>,
    ) -> Self {
        Self {
            reserved,
            phys_alloc: Some(phys_alloc),
            pt_region: None,
            pt_ops: Some(RealPtOps),
            _stage: PhantomData,
        }
    }

    pub(crate) fn into_normal(mut self) -> VmPageAllocator<Normal, RealPtOps> {
        let pt_region = PtRegion::from_reserved_with_ops(
            &mut self.reserved,
            self.phys_alloc.unwrap(),
            self.pt_ops.unwrap(),
        );
        VmPageAllocator {
            reserved: self.reserved,
            phys_alloc: None,
            pt_region: Some(pt_region),
            pt_ops: None,
            _stage: PhantomData,
        }
    }
}

impl<O: PtOps> VmPageAllocator<Normal, O> {
    pub(crate) fn alloc_phys(
        &mut self, clicks: usize, flags: PageAllocFlags,
    ) -> Result<PhysBytes, AllocError> {
        self.pt_region.as_mut().unwrap().alloc_phys(clicks, flags)
    }

    pub(crate) fn alloc_virt(
        &mut self, _phys: PhysBytes, _clicks: usize,
    ) -> Option<VirBytes> {
        self.pt_region.as_mut().unwrap().alloc_pt_page()
    }

    pub(crate) fn alloc_page(&mut self) -> Option<(VirBytes, PhysBytes)> {
        let phys = self.alloc_phys(1, PageAllocFlags::empty()).ok()?;
        let virt = self.alloc_virt(phys, 1)?;
        Some((virt, phys))
    }

    pub(crate) fn free_page(&mut self, phys: PhysBytes) {
        self.pt_region.as_mut().unwrap().free_phys(phys, 1);
    }

    pub(crate) fn relocate_phys_allocator(&mut self) {
        self.pt_region.as_mut().unwrap().relocate_phys_allocator();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phys_mem::{PhysAllocator, PageAllocFlags, AllocError, PhysBytes};
    use crate::pt_region::{PtOps, ReservedRegion};
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

    fn mock_bootstrap() -> VmPageAllocator<Bootstrap, MockPtOps> {
        VmPageAllocator {
            reserved: mock_reserved(),
            phys_alloc: Some(mock_phys_alloc()),
            pt_region: None,
            pt_ops: Some(MockPtOps::new()),
            _stage: PhantomData,
        }
    }

    impl VmPageAllocator<Bootstrap, MockPtOps> {
        fn into_normal_for_test(mut self) -> VmPageAllocator<Normal, MockPtOps> {
            let pt_region = PtRegion::from_reserved_with_ops(
                &mut self.reserved,
                self.phys_alloc.unwrap(),
                self.pt_ops.unwrap(),
            );
            VmPageAllocator {
                reserved: self.reserved,
                phys_alloc: None,
                pt_region: Some(pt_region),
                pt_ops: None,
                _stage: PhantomData,
            }
        }
    }

    #[test]
    fn test_typestate_transition() {
        let mut bootstrap = mock_bootstrap();

        let (v1, _p1) = bootstrap.alloc_page().unwrap();
        assert!(bootstrap.reserved.contains(v1));

        let mut normal = bootstrap.into_normal_for_test();

        let (v2, p2) = normal.alloc_page().unwrap();
        assert_ne!(v2.0, 0);
        assert_ne!(p2.as_u64(), 0);
    }

    #[test]
    fn test_split_alloc() {
        let bootstrap = mock_bootstrap();
        let mut normal = bootstrap.into_normal_for_test();

        let phys = normal.alloc_phys(1, PageAllocFlags::empty()).unwrap();
        assert_ne!(phys.as_u64(), 0);

        let virt = normal.alloc_virt(phys, 1).unwrap();
        assert_ne!(virt.0, 0);

        let (v, p) = normal.alloc_page().unwrap();
        assert_ne!(v.0, 0);
        assert_ne!(p.as_u64(), 0);
    }

    #[test]
    fn test_reserved_exhaustion() {
        let reserved = ReservedRegion::new(
            PhysBytes::new(0x1000),
            VirBytes(0x7000_0000),
            2,
        );
        let phys_alloc = mock_phys_alloc();
        let mut bootstrap = VmPageAllocator::<Bootstrap, RealPtOps>::new(reserved, phys_alloc);

        let _a = bootstrap.alloc_page().unwrap();
        let _b = bootstrap.alloc_page().unwrap();

        assert!(bootstrap.alloc_page().is_none());
    }

    struct MockPhysAllocatorWithArrays {
        next: u64,
        total: usize,
        array1: [u64; 4],
        array2: [usize; 4],
    }

    impl MockPhysAllocatorWithArrays {
        fn new() -> Self {
            Self {
                next: 0x1000,
                total: 256,
                array1: [0xAA, 0xBB, 0xCC, 0xDD],
                array2: [100, 200, 300, 400],
            }
        }
    }

    impl PhysAllocator for MockPhysAllocatorWithArrays {
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

        fn reloc_array_count(&self) -> usize {
            2
        }

        fn reloc_array_info(&self, index: usize) -> (*const u8, usize, usize) {
            match index {
                0 => (self.array1.as_ptr() as *const u8, 4, core::mem::size_of::<u64>()),
                1 => (self.array2.as_ptr() as *const u8, 4, core::mem::size_of::<usize>()),
                _ => (core::ptr::null(), 0, 0),
            }
        }

        fn update_relocated_arrays(&mut self, _new_ptrs: &[*mut u8]) {
        }
    }

    #[test]
    fn test_relocate_noop() {
        let reserved = mock_reserved();
        let phys_alloc = mock_phys_alloc();
        let pt_ops = MockPtOps::new();

        let bootstrap = VmPageAllocator::<Bootstrap, MockPtOps> {
            reserved,
            phys_alloc: Some(phys_alloc),
            pt_region: None,
            pt_ops: Some(pt_ops),
            _stage: PhantomData,
        };

        let mut normal = bootstrap.into_normal_for_test();

        normal.relocate_phys_allocator();

        let (v, p) = normal.alloc_page().unwrap();
        assert_ne!(v.0, 0);
        assert_ne!(p.as_u64(), 0);
    }

    #[test]
    fn test_reloc_array_info() {
        let alloc = MockPhysAllocatorWithArrays::new();
        assert_eq!(alloc.reloc_array_count(), 2);

        let (ptr, count, size) = alloc.reloc_array_info(0);
        assert_eq!(count, 4);
        assert_eq!(size, core::mem::size_of::<u64>());
        assert!(!ptr.is_null());

        let (ptr, count, size) = alloc.reloc_array_info(1);
        assert_eq!(count, 4);
        assert_eq!(size, core::mem::size_of::<usize>());
        assert!(!ptr.is_null());

        let (ptr, _, _) = alloc.reloc_array_info(99);
        assert!(ptr.is_null());
    }
}
