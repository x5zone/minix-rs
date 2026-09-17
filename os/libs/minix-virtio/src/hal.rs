//! Hardware-abstraction seam: DMA-safe memory for queues and buffers.
//!
//! C correspondence: `alloc_contig` for the physically contiguous queue
//! memory and buffer allocation in `virtio.c` (the setup path around
//! `virtio.c:319`, and per-buffer allocation in the drivers). C allocates
//! straight from the kernel; here the service implements this trait over
//! its memory source, keeping the policy library free of address math.
//!
//! The same three operations appear in rcore-os's `virtio-drivers` `Hal`
//! trait (dma_alloc/dma_free plus physical-to-virtual translation) and
//! Redox's `common/src/dma.rs` — the community-converged shape for "DMA
//! memory without knowing the platform".
//!
//! Cross-stage note: the VM-side allocator contract is registered as
//! edge E-DMABUF (`02-stage-vm` owns the physical allocation); this
//! trait is the consumer seam this stage commits to.

/// One DMA-safe memory region: physically contiguous, CPU-accessible.
///
/// `cpu_addr` is an opaque handle the producing side understands (a
/// mapped address); `phys` is the bus address that goes into descriptor
/// `addr` fields. Bytes are NOT validated by this crate — the service
/// owns the mapping's lifetime and cache discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaRegion {
    /// Bus/physical address written into descriptor `addr` fields.
    pub phys: u64,
    /// CPU-side handle of the mapping (opaque to the policy library).
    pub cpu_addr: u64,
    /// Region length in bytes.
    pub len: u32,
}

/// Platform services for queue and buffer memory.
///
/// Implemented by the service crate over the VM's physical allocator
/// (edge E-DMABUF); tests implement it over a byte vector.
pub trait Hal {
    /// Allocate `size` bytes aligned to `align` of physically contiguous,
    /// DMA-safe memory.
    fn dma_alloc(&mut self, size: u32, align: u32) -> Option<DmaRegion>;

    /// Return a region previously produced by [`Hal::dma_alloc`].
    fn dma_free(&mut self, region: DmaRegion);
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
        fn dma_alloc(&mut self, size: u32, align: u32) -> Option<DmaRegion> {
            let aligned = (self.next + align as u64 - 1) & !(align as u64 - 1);
            let region = DmaRegion {
                phys: aligned,
                cpu_addr: aligned,
                len: size,
            };
            self.next = aligned + size as u64;
            self.live.push(region);
            Some(region)
        }

        fn dma_free(&mut self, region: DmaRegion) {
            self.live.retain(|r| *r != region);
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
