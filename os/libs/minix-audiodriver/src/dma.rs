//! DMA buffer ownership and ring planning (`init_buffers`, `audio_fw.c:753-810`).
//!
//! C correspondence: the framework — not the card — owns the DMA buffer.
//! `init_buffers` allocates `DmaSize + 64 KiB` contiguously with
//! `AC_ALIGN64K | AC_LOWER16M` (`audio_fw.c:764`), translates it with
//! `sys_umap(SELF, VM_D, ...)` (`audio_fw.c:786-789`), skips past a 64 KiB
//! boundary when the first half would cross one (`dma_bytes_left`,
//! `audio_fw.h:78-79`, applied at `audio_fw.c:791-796`), and hands the
//! physical address, the size, and the channel to the card through
//! `drv_set_dma` (`audio_fw.c:798-800`). The card never allocates, maps,
//! or aligns anything: it only programs what it is given.
//!
//! # The Rust verdict
//!
//! The ownership stays where C puts it: **the framework's service layer**.
//! Two primitives it needs, and their state in the rewrite:
//!
//! - *virtual to physical*: available (`minix_sys::vm::physical_address_via`,
//!   the same call the vbox driver's request pages use);
//! - *a contiguous, 64 KiB-aligned, below-16 MiB allocation*: **not
//!   available yet**. `alloc_contig`'s flags have no Rust counterpart, so
//!   the allocation sits behind [`DmaAlloc`], and a service whose allocator
//!   cannot serve refuses to start rather than programming an address that
//!   is not what the card will DMA into. This is the registered
//!   `E-DMABUF` dependency (the VM side owns contiguous allocation).

/// How many bytes one address can DMA before hitting a 64 KiB boundary.
///
/// C: `dma_bytes_left(phys)` (`audio_fw.h:78-79`): `0x10000` minus the
/// low sixteen bits of the physical address.
pub const fn dma_bytes_left(physical: u64) -> u32 {
    0x1_0000 - (physical & 0xFFFF) as u32
}

/// A DMA buffer the framework owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaBuffer {
    /// The buffer's virtual base in this process (`DmaBuf`).
    pub address: u64,
    /// Its physical base (`DmaPhys`, before the boundary adjustment).
    pub physical: u64,
    /// The bytes allocated (`DmaSize + 64 KiB`, `audio_fw.c:763`).
    pub size: u64,
}

/// The allocation primitive the framework needs.
///
/// C: `alloc_contig(size, AC_ALIGN64K | AC_LOWER16M, &ph)`
/// (`audio_fw.c:764`): contiguous, 64 KiB-aligned, and addressable below
/// 16 MiB. Registered `E-DMABUF`: the rewrite has no such allocator, so
/// the production implementation reports `ENOSYS` and a service built on
/// it refuses to start.
pub trait DmaAlloc {
    /// Allocate `size` bytes under the DMA constraints.
    fn alloc_contig(&mut self, size: u64) -> Result<DmaBuffer, i32>;
}

/// Where a channel's ring starts inside the buffer
/// (`audio_fw.c:791-796`): if the first half of the requested size would
/// cross a 64 KiB boundary, start after it.
///
/// C's arithmetic is `if dma_bytes_left(DmaPhys) < DmaSize { DmaPtr +=
/// left; DmaPhys += left; }` — the card is handed `DmaPhys` and
/// `DmaSize`, so the ring never straddles the boundary.
pub const fn ring_start(buffer: DmaBuffer, dma_size: u64) -> (u64, u64) {
    let left = dma_bytes_left(buffer.physical) as u64;
    if left < dma_size {
        (buffer.address + left, buffer.physical + left)
    } else {
        (buffer.address, buffer.physical)
    }
}

/// What the framework hands the card (`drv_set_dma`,
/// `audio_fw.c:798-800`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingPlan {
    /// The ring's virtual base (the framework writes fragments here).
    pub address: u64,
    /// The ring's physical base (programmed into the card).
    pub physical: u64,
    /// The ring's size (`DmaSize`).
    pub size: u64,
}

/// Plan one channel's ring over an allocated buffer.
pub const fn plan_ring(buffer: DmaBuffer, dma_size: u64) -> RingPlan {
    let (address, physical) = ring_start(buffer, dma_size);
    RingPlan {
        address,
        physical,
        size: dma_size,
    }
}

/// The production allocator: registered `E-DMABUF`, not there yet.
///
/// Reporting `ENOSYS` keeps a driver from programming a DMA address it
/// does not own; C's failure path is the same shape (a failed
/// `alloc_contig` fails the channel's bring-up, `audio_fw.c:765-769`).
pub struct UnboundDma;

impl DmaAlloc for UnboundDma {
    fn alloc_contig(&mut self, _size: u64) -> Result<DmaBuffer, i32> {
        Err(-minix_types::ENOSYS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dma_bytes_left_matches_the_c_macro() {
        // C: `0x10000 - (phys & 0xFFFF)`.
        assert_eq!(dma_bytes_left(0), 0x1_0000);
        assert_eq!(dma_bytes_left(0x1000), 0xF000);
        assert_eq!(dma_bytes_left(0xFFFF), 1);
        assert_eq!(dma_bytes_left(0x1_0000), 0x1_0000, "the boundary wraps");
        assert_eq!(dma_bytes_left(0x1_2345), 0xDCBB);
    }

    #[test]
    fn test_a_ring_that_fits_keeps_the_buffer_base() {
        let buffer = DmaBuffer {
            address: 0x1000_0000,
            physical: 0x2000_1000,
            size: 0x2_0000,
        };
        // 0xF000 bytes are free before the boundary: a 32 KiB ring fits.
        let plan = plan_ring(buffer, 0x8000);
        assert_eq!(plan.physical, 0x2000_1000);
        assert_eq!(plan.address, 0x1000_0000);
        assert_eq!(plan.size, 0x8000);
    }

    #[test]
    fn test_a_ring_crossing_the_boundary_starts_after_it() {
        let buffer = DmaBuffer {
            address: 0x1000_0000,
            physical: 0x2000_1000,
            size: 0x2_0000,
        };
        // 0xF000 free, a 64 KiB ring does not fit: it starts at the
        // boundary, exactly as C's `DmaPtr += left; DmaPhys += left`.
        let plan = plan_ring(buffer, 0x1_0000);
        assert_eq!(plan.physical, 0x2001_0000);
        assert_eq!(plan.address, 0x1000_F000);
        assert_eq!(plan.size, 0x1_0000);
        // The plan stays inside the allocation (C's `DmaSize + 64 KiB`).
        assert!(plan.physical + plan.size <= buffer.physical + buffer.size);
    }

    #[test]
    fn test_the_production_allocator_refuses_instead_of_faking_a_buffer() {
        let mut alloc = UnboundDma;
        assert_eq!(alloc.alloc_contig(0x2_0000), Err(-minix_types::ENOSYS));
    }
}
