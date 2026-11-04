//! Hardware-abstraction seam: DMA-safe memory for queues and buffers.
//!
//! C correspondence: `alloc_contig` for the physically contiguous queue
//! memory and buffer allocation in `virtio.c` (the setup path around
//! `virtio.c:319`, and per-buffer allocation in the drivers).
//!
//! The contract itself lives in the shared types crate
//! ([`minix_types::types::dma`]) as the edge E-DMABUF artifact — one
//! definition serves every driver library (virtio here, ahci and
//! usb_storage when they grow service seams) and one implementation comes
//! from the VM server (02-stage ownership). This module keeps the crate's
//! public names stable: `Hal` is the shared [`DmaMemory`] trait under the
//! name rcore-os's `virtio-drivers` made conventional, `DmaRegion` the
//! shared region type.

pub use minix_types::types::dma::{DmaMemory as Hal, DmaRegion};

/// The production [`Hal`]: DMA memory through the memory server, the same
/// shape C's virtio uses (`alloc_contig(q->ring_size, AC_ALIGN4K,
/// &q->paddr)`, `minix3/minix/lib/libvirtio/virtio.c:322`).
///
/// Every `dma_alloc` is one `alloc_contig_via` round (`mmap` with the
/// contiguous/prealloc base plus the alignment constraint, then the umap
/// for the physical base); every `dma_free` is the matching `munmap`. The
/// region bookkeeping is what makes [`Hal::physical_to_virtual`] honest —
/// a linear direct map cannot distinguish ownership, so only regions this
/// producer handed out translate.
pub struct VmHal {
    /// This server's endpoint: the mmap beneficiary and `sys_umap`'s SELF.
    pub self_endpoint: minix_types::Endpoint,
    live: RegionTable,
}

/// The live-region bookkeeping, kept pure so the translation half is unit
/// testable without a transport.
#[derive(Default)]
struct RegionTable {
    live: alloc::vec::Vec<DmaRegion>,
}

impl RegionTable {
    fn insert(&mut self, region: DmaRegion) {
        self.live.push(region);
    }

    fn remove(&mut self, cpu_addr: u64) -> Option<DmaRegion> {
        let index = self.live.iter().position(|r| r.cpu_addr == cpu_addr)?;
        Some(self.live.remove(index))
    }

    fn physical_to_virtual(&self, phys: u64) -> Option<u64> {
        self.live
            .iter()
            .find(|r| phys >= r.phys && phys < r.phys + r.len as u64)
            .map(|r| r.cpu_addr + (phys - r.phys))
    }
}

impl VmHal {
    pub fn new(self_endpoint: minix_types::Endpoint) -> Self {
        VmHal {
            self_endpoint,
            live: RegionTable::default(),
        }
    }
}

impl Hal for VmHal {
    fn dma_alloc(&mut self, size: u32, align: u32) -> Result<DmaRegion, minix_types::Errno> {
        // C virtio asks for `AC_ALIGN4K` (the page allocator's default);
        // a 64 KiB constraint maps onto the mmap flag, anything else has
        // no counterpart in the mmap face and is refused rather than
        // silently under-aligned.
        let alloc_flags = match align {
            a if a <= 0x1000 => 0,
            0x1_0000 => minix_sys::vm::AC_ALIGN64K,
            _ => return Err(minix_types::Errno::from_i32(minix_sys::EINVAL)),
        };
        let (cpu_addr, phys) = minix_sys::vm::alloc_contig_via(
            &minix_sys::ipc::DirectTrapTransport,
            self.self_endpoint,
            size as u64,
            alloc_flags,
        )?;
        let region = DmaRegion {
            phys: phys.0,
            cpu_addr: cpu_addr.0,
            len: size,
        };
        self.live.insert(region);
        Ok(region)
    }

    fn dma_free(&mut self, region: DmaRegion) {
        if self.live.remove(region.cpu_addr).is_some() {
            //[SD19-gate] munmap of a region this producer owns; a failure
            // leaves the address space larger, which is benign for a DMA
            // producer that is tearing down
            #[allow(clippy::let_underscore_must_use)]
            let _ = minix_sys::vm::munmap_via(
                &minix_sys::ipc::DirectTrapTransport,
                minix_types::VirBytes(region.cpu_addr),
                minix_types::VirBytes(region.len as u64),
            );
        }
    }

    fn virtual_to_physical(
        &self,
        virtual_address: minix_types::VirBytes,
    ) -> Option<minix_types::PhysBytes> {
        minix_sys::vm::physical_address_via(
            &minix_sys::ipc::DirectTrapTransport,
            self.self_endpoint,
            virtual_address,
        )
        .ok()
    }

    fn physical_to_virtual(
        &self,
        physical_address: minix_types::PhysBytes,
    ) -> Option<minix_types::VirBytes> {
        self.live
            .physical_to_virtual(physical_address.0)
            .map(minix_types::VirBytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bump allocator over one fixed buffer: deterministic and enough to
    /// exercise allocation order and exhaustion.
    struct MockHal {
        next: u64,
        live: alloc::vec::Vec<DmaRegion>,
    }

    impl MockHal {
        fn new() -> Self {
            MockHal {
                next: 0x1000,
                live: alloc::vec::Vec::new(),
            }
        }
    }

    impl Hal for MockHal {
        fn dma_alloc(&mut self, size: u32, align: u32) -> Result<DmaRegion, minix_types::Errno> {
            let aligned = (self.next + align as u64 - 1) & !(align as u64 - 1);
            let region = DmaRegion {
                phys: aligned,
                cpu_addr: aligned,
                len: size,
            };
            self.next = aligned + size as u64;
            self.live.push(region);
            Ok(region)
        }

        fn dma_free(&mut self, region: DmaRegion) {
            self.live.retain(|r| *r != region);
        }

        fn virtual_to_physical(
            &self,
            virtual_address: minix_types::VirBytes,
        ) -> Option<minix_types::PhysBytes> {
            self.live
                .iter()
                .find(|r| {
                    virtual_address.0 >= r.cpu_addr && virtual_address.0 < r.cpu_addr + r.len as u64
                })
                .map(|r| minix_types::PhysBytes(virtual_address.0 - r.cpu_addr + r.phys))
        }

        fn physical_to_virtual(
            &self,
            physical_address: minix_types::PhysBytes,
        ) -> Option<minix_types::VirBytes> {
            self.live
                .iter()
                .find(|r| {
                    physical_address.0 >= r.phys && physical_address.0 < r.phys + r.len as u64
                })
                .map(|r| minix_types::VirBytes(physical_address.0 - r.phys + r.cpu_addr))
        }
    }

    #[test]
    fn region_table_translates_only_its_own_regions() {
        let mut table = RegionTable::default();
        table.insert(DmaRegion {
            phys: 0x1_0000,
            cpu_addr: 0x5000,
            len: 0x2000,
        });
        table.insert(DmaRegion {
            phys: 0x3_0000,
            cpu_addr: 0x9000,
            len: 0x1000,
        });
        // Inside a region: offset preserved.
        assert_eq!(table.physical_to_virtual(0x1_0800), Some(0x5800));
        assert_eq!(table.physical_to_virtual(0x3_0000), Some(0x9000));
        // Outside every region (a gap and past the end): None — a linear
        // map cannot answer this, the bookkeeping can.
        assert_eq!(table.physical_to_virtual(0x2_0000), None);
        assert_eq!(table.physical_to_virtual(0x1_0000 + 0x2000), None);
        // Removal clears the translation.
        assert_eq!(table.remove(0x5000).unwrap().len, 0x2000);
        assert_eq!(table.physical_to_virtual(0x1_0800), None);
    }

    #[test]
    fn test_dma_alloc_aligns_and_tracks_regions() {
        let mut hal = MockHal::new();
        let queue = hal.dma_alloc(vring_size_for(8), 4096).unwrap();
        assert_eq!(queue.phys, 0x1000); // already aligned
        assert_eq!(queue.len, 4096 + 70);
        let buffer = hal.dma_alloc(1514, 16).unwrap();
        // 0x1000 + 4166 rounded up to 16.
        assert_eq!(buffer.phys & 0xF, 0);
        assert_eq!(hal.live.len(), 2);
        hal.dma_free(buffer);
        assert_eq!(hal.live.len(), 1);
    }

    fn vring_size_for(num: u16) -> u32 {
        // Mirrors ring::vring_size for the mock's bookkeeping.
        let avail = 2 * (3 + num as u32);
        let avail_total = (16 * num as u32 + avail + 4095) & !4095;
        avail_total + 6 + 8 * num as u32
    }
}
