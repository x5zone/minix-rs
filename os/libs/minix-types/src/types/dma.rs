//! DMA memory contract: contiguous, device-visible memory as a trait.
//!
//! C drivers allocate DMA memory straight from the kernel
//! (`alloc_contig` for virtio queue memory, `minix3/minix/drivers/lib/libvirtio/virtio.c:319`
//! and the same shape in ahci/usb_storage) and hand out bus addresses to
//! devices. A user-space driver server cannot do that: the physical
//! allocator lives in the VM server. This trait is the boundary — driver
//! libraries stay generic over it, the VM server (edge E-DMABUF, 02-stage
//! ownership) implements it over its contiguous-page allocator.
//!
//! The shape follows the community-converged predecessors: rcore-os
//! `virtio-drivers`' `Hal` trait (dma_alloc/dma_free plus address
//! translation) and Redox's `common/src/dma.rs`. Differences are policy
//! decisions, not drift: allocation failure carries `Errno::ENOMEM` (the
//! crate-wide errno rule) instead of `None`, and freeing is infallible —
//! C's `free` equally reports nothing.

use super::address::{PhysBytes, VirBytes};
use crate::Errno;

/// One DMA-safe memory region: physically contiguous, CPU-accessible.
///
/// `phys` is the bus/physical address that goes into device descriptor
/// fields; `cpu_addr` is the CPU-side view of the same first byte (the
/// mapping handle the producing side understands). `len` is the region's
/// usable length in bytes — an implementation may round the underlying
/// allocation up to page boundaries, but must report here exactly what the
/// caller may use. Bytes are NOT validated by consumers — the producer
/// owns the mapping's lifetime and cache discipline (x86-64 devices this
/// tree targets are DMA-coherent; a non-coherent port grows explicit sync
/// operations here before any driver consumes them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaRegion {
    /// Bus/physical address of the first byte.
    pub phys: u64,
    /// CPU-side address of the first byte.
    pub cpu_addr: u64,
    /// Usable length in bytes.
    pub len: u32,
}

/// Platform services for DMA-capable queue and buffer memory.
///
/// Implemented by the driver server over the VM's physical allocator;
/// driver libraries take an implementation as a parameter and never touch
/// page tables or bus addresses on their own. Every method is address
/// arithmetic from the consumer's point of view — no PTE bits, no CR3.
pub trait DmaMemory {
    /// Allocates `size` bytes aligned to at least `align` of physically
    /// contiguous, DMA-coherent memory.
    ///
    /// Exhaustion or an unservable alignment is `Err(Errno::ENOMEM)`, the
    /// same verdict C's allocator reaches by returning NULL.
    fn dma_alloc(&mut self, size: u32, align: u32) -> Result<DmaRegion, Errno>;

    /// Returns a region previously produced by [`DmaMemory::dma_alloc`].
    ///
    /// Infallible by contract: double-free or foreign regions are producer
    /// bugs, and the producer (which owns the allocator) checks for them.
    fn dma_free(&mut self, region: DmaRegion);

    /// Translates a CPU virtual address inside a mapped region to the
    /// device-visible physical address. `None` when the address is not
    /// backed by anything the producer manages.
    fn virtual_to_physical(&self, virtual_address: VirBytes) -> Option<PhysBytes>;

    /// The inverse translation for addresses this producer handed out.
    /// `None` when the physical address is not one of this producer's
    /// mappings (physical memory in general is not CPU-addressable here).
    fn physical_to_virtual(&self, physical_address: PhysBytes) -> Option<VirBytes>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic bump allocator over a fictitious window: exercises
    /// the alignment promise, the region bookkeeping, and the exhaustion
    /// verdict. Fixed-capacity bookkeeping — the crate is no_std without
    /// an allocator.
    struct BumpDma {
        next: u64,
        budget: u64,
        live: [Option<DmaRegion>; 4],
    }

    impl BumpDma {
        fn new(budget: u64) -> Self {
            BumpDma { next: 0x1000, budget, live: [None; 4] }
        }

        fn live_count(&self) -> usize {
            self.live.iter().filter(|r| r.is_some()).count()
        }
    }

    impl DmaMemory for BumpDma {
        fn dma_alloc(&mut self, size: u32, align: u32) -> Result<DmaRegion, Errno> {
            let aligned = (self.next + align as u64 - 1) & !(align as u64 - 1);
            if aligned + size as u64 > self.budget {
                return Err(Errno::ENOMEM);
            }
            let region =
                DmaRegion { phys: aligned, cpu_addr: aligned | 0xffff_8000_0000_0000, len: size };
            self.next = aligned + size as u64;
            if let Some(slot) = self.live.iter_mut().find(|s| s.is_none()) {
                *slot = Some(region);
            }
            Ok(region)
        }

        fn dma_free(&mut self, region: DmaRegion) {
            if let Some(slot) = self.live.iter_mut().find(|s| s.map(|r| r.phys) == Some(region.phys)) {
                *slot = None;
            }
        }

        fn virtual_to_physical(&self, virtual_address: VirBytes) -> Option<PhysBytes> {
            self.live.iter().flatten().find(|r| {
                virtual_address.0 >= r.cpu_addr && virtual_address.0 < r.cpu_addr + r.len as u64
            }).map(|r| PhysBytes(virtual_address.0 - r.cpu_addr + r.phys))
        }

        fn physical_to_virtual(&self, physical_address: PhysBytes) -> Option<VirBytes> {
            self.live.iter().flatten().find(|r| {
                physical_address.0 >= r.phys && physical_address.0 < r.phys + r.len as u64
            }).map(|r| VirBytes(physical_address.0 - r.phys + r.cpu_addr))
        }
    }

    /// A second, deliberately different implementation (always exhausted):
    /// the trait needs at least two behaviorally distinct impls to stay an
    /// honest abstraction.
    struct NoDma;

    impl DmaMemory for NoDma {
        fn dma_alloc(&mut self, _size: u32, _align: u32) -> Result<DmaRegion, Errno> {
            Err(Errno::ENOMEM)
        }
        fn dma_free(&mut self, _region: DmaRegion) {}
        fn virtual_to_physical(&self, _a: VirBytes) -> Option<PhysBytes> {
            None
        }
        fn physical_to_virtual(&self, _p: PhysBytes) -> Option<VirBytes> {
            None
        }
    }

    /// Allocation honors the alignment request and reports the usable
    /// length; exhaustion is ENOMEM, not a panic or a null region.
    #[test]
    fn test_dma_alloc_alignment_and_exhaustion() {
        let mut dma = BumpDma::new(0x10000);
        let region = dma.dma_alloc(4096, 4096).expect("first allocation fits");
        assert_eq!(region.phys, 0x1000);
        assert_eq!(region.len, 4096);
        let region2 = dma.dma_alloc(1, 4096).expect("second allocation fits");
        assert_eq!(region2.phys, 0x2000, "aligned up past the first region");
        let mut dma = BumpDma::new(0x800);
        assert_eq!(dma.dma_alloc(4096, 16), Err(Errno::ENOMEM));
        assert_eq!(NoDma.dma_alloc(16, 16), Err(Errno::ENOMEM));
    }

    /// Translation round-trips inside a live region and reports `None`
    /// outside it, in both directions.
    #[test]
    fn test_dma_translation_round_trip() {
        let mut dma = BumpDma::new(0x10000);
        let region = dma.dma_alloc(0x2000, 0x1000).expect("allocation fits");
        let inside = VirBytes(region.cpu_addr + 0x123);
        let phys = dma.virtual_to_physical(inside).expect("inside the region");
        assert_eq!(phys.0, region.phys + 0x123);
        let back = dma.physical_to_virtual(phys).expect("round trip");
        assert_eq!(back.0, inside.0);
        assert_eq!(dma.virtual_to_physical(VirBytes(region.cpu_addr + 0x10_0000)), None);
        assert_eq!(dma.physical_to_virtual(PhysBytes(region.phys + 0x10_0000)), None);
    }

    /// Freeing removes the region from translation; the trait's infallible
    /// free keeps the call site free of error plumbing.
    #[test]
    fn test_dma_free_releases_translation() {
        let mut dma = BumpDma::new(0x10000);
        let region = dma.dma_alloc(0x1000, 0x1000).expect("allocation fits");
        dma.dma_free(region);
        assert_eq!(dma.virtual_to_physical(VirBytes(region.cpu_addr)), None);
    }
}
