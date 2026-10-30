//! DMA contiguous memory -- the VM-side implementation of the [`DmaMemory`]
//! contract (the E-DMABUF edge).
//!
//! C drivers ask the kernel for DMA memory directly (`alloc_contig`, the same
//! shape as virtio/ahdi/usb_storage,
//! `minix3/minix/drivers/lib/libvirtio/virtio.c:319`) and fill the bus address
//! into the device descriptor. A user-space driver server cannot do that: the
//! physical allocator lives in VM. Half of the contract sits in minix-types
//! (`DmaMemory`/`DmaRegion`, following the precedent of rcore
//! `virtio-drivers`' `Hal` and Redox `common/src/dma.rs`); this module is the
//! VM-side behavior implementation that consumes it, so the driver library
//! stays generic over the trait and never touches page tables or bus
//! addresses itself.
//!
//! The production consumers (the 16-stage driver service layer, the USB
//! storage service) are wired up in a later stage; before that, lib builds
//! see only test consumers, hence the dead_code gate marked after the
//! handle_signal precedent.

#![cfg_attr(not(test), allow(dead_code))]
//!
//! # Why the implementation is "bookkeeping + pure arithmetic"
//!
//! VM holds the Direct Map (`[ARCH: A-1]`): physical memory is linearly
//! visible, and `vm_phys_to_virt`/`virt_to_phys` are constant-offset
//! conversions -- a DMA-coherent address needs no mapping at all. So all of
//! [`VmDmaMemory`]'s work comes down to three things:
//!
//! 1. obtain a contiguous page run through [`PfnAllocator::alloc_contiguous`]
//!    (one multi-page request; the funnel semantics -- and the reclaim retry
//!    that comes with it -- apply unchanged);
//! 2. keep the bookkeeping of live regions (translation only holds for
//!    regions this producer handed out -- the contract requires `None` for
//!    anything outside the window or region, and a pure linear map does not
//!    distinguish ownership, so the bookkeeping supplies that layer);
//! 3. trim for alignment: when `align` exceeds the page size, request
//!    `align/PAGE_SIZE - 1` extra pages of slack, advance the run start to
//!    the alignment line, and return the whole run from the actual base
//!    address on free.
//!
//! Cache coherency: x86-64 target devices are DMA-coherent, so no explicit
//! sync is needed (the contract document reaches the same conclusion);
//! non-coherent ports grow sync operations on the contract side, not here.

use alloc::vec::Vec;
use minix_types::{DmaMemory, DmaRegion, Errno, PhysBytes, VirBytes};

use crate::direct_map::vm_phys_to_virt;
use crate::phys_mem::AlignedPhysBytes;
use crate::region::PfnAllocator;

/// Page size (matching `region::page_state::PAGE_SIZE`; the PFN granularity).
const PAGE_SIZE: u64 = 4096;

/// Bookkeeping for one live DMA region.
#[derive(Debug, Clone, Copy)]
struct DmaEntry {
    /// The region handed to the caller (phys/cpu_addr/len; len is the usable
    /// length the contract promises).
    region: DmaRegion,
    /// The base PFN of the run actually occupied (the start before alignment
    /// trimming; free returns the whole run from it).
    base_pfn: u32,
    /// The number of pages actually occupied (alignment slack included).
    pages: u32,
}

/// VM-side [`DmaMemory`] implementation -- generic over the page allocator so
/// tests can inject a recording mock; the server integration wraps
/// [`VmPageAllocator`](crate::alloc_page::VmPageAllocator).
///
/// The bookkeeping uses a plain `Vec`: DMA allocation is a low-frequency event
/// (once per virtqueue/buffer), so the constant cost of a linear scan is
/// negligible; the upper bound is however many concurrent DMA regions physical
/// memory can hold, so no extra limit is needed.
pub(crate) struct VmDmaMemory<A: PfnAllocator> {
    alloc: A,
    live: Vec<DmaEntry>,
}

impl<A: PfnAllocator> VmDmaMemory<A> {
    pub(crate) fn new(alloc: A) -> Self {
        Self {
            alloc,
            live: Vec::new(),
        }
    }

    /// Directly observe the bookkeeping (the number of live regions).
    #[cfg(test)]
    pub(crate) fn live_count(&self) -> usize {
        self.live.len()
    }
}

impl<A: PfnAllocator> DmaMemory for VmDmaMemory<A> {
    fn dma_alloc(&mut self, size: u32, align: u32) -> Result<DmaRegion, Errno> {
        // The contract's alignment precondition is a power of two (the natural
        // shape of device descriptors and ring buffers); a non-power alignment
        // cannot be expressed by "advance the start", so it is refused as
        // unserviceable. Zero length has no usable semantics, hence EINVAL as
        // well.
        if size == 0 || align == 0 || !align.is_power_of_two() {
            return Err(Errno::EINVAL);
        }
        let align = (align as u64).max(PAGE_SIZE);
        let pages = (size as u64).div_ceil(PAGE_SIZE) as u32;
        // When alignment is coarser than a page, ask for align/PAGE - 1 extra
        // pages of slack; advancing the run start by at most that many pages
        // then hits the alignment line.
        let slack = ((align / PAGE_SIZE) - 1) as u32;
        let total = pages + slack;

        let base_pfn = self
            .alloc
            .alloc_contiguous(total)
            .map_err(|_| Errno::ENOMEM)?;
        let base_phys = base_pfn as u64 * PAGE_SIZE;
        let misalign = base_phys & (align - 1);
        let skip = if misalign == 0 {
            0
        } else {
            ((align - misalign) / PAGE_SIZE) as u32
        };
        let phys = base_phys + skip as u64 * PAGE_SIZE;

        // The Direct Map makes it linearly visible: CPU-side base = window base
        // + physical (a constant offset, no mapping built -- `[ARCH: A-1]`).
        let cpu_addr = vm_phys_to_virt(AlignedPhysBytes::new(phys));
        let region = DmaRegion {
            phys,
            cpu_addr: cpu_addr.0,
            len: size,
        };
        self.live.push(DmaEntry {
            region,
            base_pfn,
            pages: total,
        });
        Ok(region)
    }

    fn dma_free(&mut self, region: DmaRegion) {
        // Contract: free cannot fail; a double free or a foreign region is the
        // producer's own bug. The producer self-checks: when no bookkeeping
        // entry is found, defensively leave physical pages alone (releasing
        // them would let arbitrary pages be returned), and debug builds assert
        // outright to expose the caller's error.
        let pos = self.live.iter().position(|e| e.region.phys == region.phys);
        debug_assert!(
            pos.is_some(),
            "dma_free: region not allocated by this producer"
        );
        if let Some(pos) = pos {
            let entry = self.live.remove(pos);
            for pfn in entry.base_pfn..entry.base_pfn + entry.pages {
                self.alloc.free_pfn(pfn);
            }
        }
    }

    fn virtual_to_physical(&self, virtual_address: VirBytes) -> Option<PhysBytes> {
        let entry = self.live.iter().find(|e| {
            let r = &e.region;
            virtual_address.0 >= r.cpu_addr && virtual_address.0 < r.cpu_addr + r.len as u64
        })?;
        Some(PhysBytes(
            virtual_address.0 - entry.region.cpu_addr + entry.region.phys,
        ))
    }

    fn physical_to_virtual(&self, physical_address: PhysBytes) -> Option<VirBytes> {
        let entry = self.live.iter().find(|e| {
            let r = &e.region;
            physical_address.0 >= r.phys && physical_address.0 < r.phys + r.len as u64
        })?;
        Some(VirBytes(
            physical_address.0 - entry.region.phys + entry.region.cpu_addr,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region::PfnAllocError;

    /// Recording page-allocator mock: scripted contiguous runs, plus the
    /// release sequence recorded.
    struct MockPfn {
        /// The "contiguous-run base PFNs" dequeued in order (None = resources
        /// exhausted).
        script: alloc::collections::VecDeque<Result<u32, ()>>,
        freed: Vec<u32>,
    }

    impl MockPfn {
        fn scripted(runs: &[Result<u32, ()>]) -> Self {
            Self {
                script: runs.iter().copied().collect(),
                freed: Vec::new(),
            }
        }
    }

    impl PfnAllocator for MockPfn {
        fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
            unreachable!("DMA path uses alloc_contiguous only")
        }
        fn free_pfn(&mut self, pfn: u32) {
            self.freed.push(pfn);
        }
        fn alloc_contiguous(&mut self, _count: u32) -> Result<u32, PfnAllocError> {
            match self.script.pop_front() {
                Some(Ok(base)) => Ok(base),
                _ => Err(PfnAllocError::OutOfMemory),
            }
        }
    }

    /// Attach a direct-map window of 4 pages (the translation assertions need
    /// a real window).
    fn with_window<F: FnOnce() -> R, R>(f: F) -> R {
        crate::direct_map::with_test_window(16, f)
    }

    #[test]
    fn dma_alloc_reports_exact_len_and_page_alignment() {
        with_window(|| {
            let mut dma = VmDmaMemory::new(MockPfn::scripted(&[Ok(0x20)]));
            let r = dma.dma_alloc(100, 8).expect("fits");
            assert_eq!(r.len, 100, "契约:报告调用方可用的精确长度");
            assert_eq!(r.phys, 0x20 * PAGE_SIZE, "页对齐运行原样返回");
            assert_eq!(
                r.cpu_addr,
                crate::direct_map::test_vm_base() + r.phys,
                "Direct Map:cpu 侧 = 窗口基 + 物理"
            );
            assert_eq!(dma.live_count(), 1);
        });
    }

    #[test]
    fn dma_alloc_trims_alignment_within_slack() {
        with_window(|| {
            // The base lands on page 0x21 (not 2-page aligned); asking for
            // 2-page alignment gives 1 page of slack, advancing the start to
            // 0x22.
            let mut dma = VmDmaMemory::new(MockPfn::scripted(&[Ok(0x21)]));
            let r = dma
                .dma_alloc(PAGE_SIZE as u32, 2 * PAGE_SIZE as u32)
                .expect("fits");
            let want = 0x22u64 * PAGE_SIZE;
            assert_eq!(r.phys, want, "运行起点推进到对齐线");
            assert_eq!(r.cpu_addr, crate::direct_map::test_vm_base() + want);
            // Free returns the whole run from the actual base (0x21..0x23,
            // slack included).
            dma.dma_free(r);
            assert_eq!(dma.live_count(), 0);
        });
    }

    #[test]
    fn dma_alloc_rejects_zero_and_non_power_of_two_align() {
        let mut dma = VmDmaMemory::new(MockPfn::scripted(&[]));
        assert_eq!(dma.dma_alloc(0, 8), Err(Errno::EINVAL));
        assert_eq!(dma.dma_alloc(64, 48), Err(Errno::EINVAL), "非 2 的幂对齐");
        assert_eq!(dma.dma_alloc(64, 0), Err(Errno::EINVAL));
    }

    #[test]
    fn dma_alloc_exhaustion_is_enomem() {
        let mut dma = VmDmaMemory::new(MockPfn::scripted(&[Err(())]));
        assert_eq!(dma.dma_alloc(4096, 8), Err(Errno::ENOMEM));
        assert_eq!(dma.live_count(), 0);
    }

    #[test]
    fn dma_translation_round_trips_and_bounds() {
        with_window(|| {
            let mut dma = VmDmaMemory::new(MockPfn::scripted(&[Ok(0x30)]));
            let r = dma.dma_alloc(0x2000, PAGE_SIZE as u32).expect("fits");
            // In-region offsets shift in both directions.
            let inside = VirBytes(r.cpu_addr + 0x123);
            let phys = dma.virtual_to_physical(inside).expect("区内");
            assert_eq!(phys.0, r.phys + 0x123);
            assert_eq!(dma.physical_to_virtual(phys), Some(inside));
            // Out of region (both past the DMA length and fully unrelated
            // addresses) maps to None in both directions.
            assert_eq!(dma.virtual_to_physical(VirBytes(r.cpu_addr + 0x2000)), None);
            assert_eq!(dma.physical_to_virtual(PhysBytes(r.phys + 0x2000)), None);
            assert_eq!(dma.virtual_to_physical(VirBytes(0x7000_0000)), None);
        });
    }

    #[test]
    fn dma_free_releases_pages_and_translation() {
        with_window(|| {
            let mut dma = VmDmaMemory::new(MockPfn::scripted(&[Ok(0x40), Ok(0x50)]));
            let r = dma
                .dma_alloc(PAGE_SIZE as u32, PAGE_SIZE as u32)
                .expect("fits");
            dma.dma_free(r);
            assert_eq!(dma.live_count(), 0);
            assert_eq!(
                dma.virtual_to_physical(VirBytes(r.cpu_addr)),
                None,
                "释放后翻译即失效"
            );

            // The normal release path: the whole page run goes back to the
            // allocator.
            let r2 = dma
                .dma_alloc(PAGE_SIZE as u32, PAGE_SIZE as u32)
                .expect("second");
            dma.dma_free(r2);
            assert_eq!(dma.live_count(), 0);
        });
    }

    /// A foreign region is the producer's own bug (the contract's words) -- the
    /// debug-build assertion is where "producer self-check" lands; release
    /// builds defensively leave physical pages alone.
    #[test]
    #[should_panic(expected = "dma_free: region not allocated")]
    fn dma_free_of_foreign_region_is_a_producer_bug() {
        let mut dma = VmDmaMemory::new(MockPfn::scripted(&[]));
        let foreign = DmaRegion {
            phys: 0xDEAD_0000,
            cpu_addr: 0,
            len: 16,
        };
        dma.dma_free(foreign);
    }
}
