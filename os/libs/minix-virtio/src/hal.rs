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
            self.live.iter().find(|r| {
                virtual_address.0 >= r.cpu_addr
                    && virtual_address.0 < r.cpu_addr + r.len as u64
            }).map(|r| minix_types::PhysBytes(virtual_address.0 - r.cpu_addr + r.phys))
        }

        fn physical_to_virtual(
            &self,
            physical_address: minix_types::PhysBytes,
        ) -> Option<minix_types::VirBytes> {
            self.live.iter().find(|r| {
                physical_address.0 >= r.phys && physical_address.0 < r.phys + r.len as u64
            }).map(|r| minix_types::VirBytes(physical_address.0 - r.phys + r.cpu_addr))
        }
    }

    #[test]
    fn test_dma_alloc_aligns_and_tracks_regions() {
        let mut hal = MockHal::new();
        let queue = hal.dma_alloc(vring_size_for(8), 4096).unwrap();
        assert_eq!(queue.phys, 0x1000); // already aligned
        assert_eq!(queue.len as u32, 4096 + 70);
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
