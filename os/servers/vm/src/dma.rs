//! DMA 连续内存——[`DmaMemory`] 契约的 VM 侧实现(edge E-DMABUF)。
//!
//! C 驱动直接向内核要 DMA 内存(`alloc_contig`,virtio/ahdi/usb_storage
//! 同型,`minix3/minix/drivers/lib/libvirtio/virtio.c:319`)并把总线地址
//! 填进设备描述符。用户态驱动服务器做不到:物理分配器在 VM 手里。契约
//! 半在 minix-types(`DmaMemory`/`DmaRegion`,edge2 L9 定稿,先例是
//! rcore `virtio-drivers` 的 `Hal` 与 Redox `common/src/dma.rs`);本模块
//! 是消费该契约的 VM 侧行为实现,让驱动库保持对 trait 泛型、永不自触
//! 页表或总线地址。
//!
//! 生产消费方(16-stage 驱动服务层、USB 存储服务)在后续阶段接线;
//! 接线前 lib 构建只见测试消费,按 handle_signal 先例标注 dead_code 门。

#![cfg_attr(not(test), allow(dead_code))]
//!
//! # 为什么实现是"簿记 + 纯算术"
//!
//! VM 持有 Direct Map(`[ARCH: A-1]`):物理内存线性可见,
//! `vm_phys_to_virt`/`virt_to_phys` 是常数偏移换算——DMA 一致性地址不
//! 需要建任何映射。因此 [`VmDmaMemory`] 的全部工作只有三件:
//!
//! 1. 经 [`PfnAllocator::alloc_contiguous`](单次多页请求,V12-P2-7 的
//!    漏斗语义,回收重试随之自带)拿连续页运行;
//! 2. 维护在役区域的簿记(翻译只对"本生产者发出的区域"成立——契约
//!    要求窗外/外区返回 `None`,纯线性映射不区分归属,簿记补上这层);
//! 3. 对齐裁剪:`align` 超过页大小时多要 `align/PAGE_SIZE - 1` 页松弛,
//!    把运行起点推进到对齐线,释放时按实际基址整段归还。
//!
//! 缓存一致性:x86-64 目标设备 DMA 一致,无需显式 sync(契约文档同
//! 结论);非一致移植在契约侧生长 sync 操作,不在这里。

use alloc::vec::Vec;
use minix_types::{DmaMemory, DmaRegion, Errno, PhysBytes, VirBytes};

use crate::direct_map::vm_phys_to_virt;
use crate::phys_mem::AlignedPhysBytes;
use crate::region::PfnAllocator;

/// 页大小(与 `region::page_state::PAGE_SIZE` 一致;PFN 的粒度)。
const PAGE_SIZE: u64 = 4096;

/// 一个在役 DMA 区域的簿记。
#[derive(Debug, Clone, Copy)]
struct DmaEntry {
    /// 发给调用方的区域(phys/cpu_addr/len;len 是契约承诺的可用长度)。
    region: DmaRegion,
    /// 实际占用的运行基 PFN(对齐裁剪前的起点,释放按它整段归还)。
    base_pfn: u32,
    /// 实际占用的页数(含对齐松弛)。
    pages: u32,
}

/// VM 侧 [`DmaMemory`] 实现——泛型在页分配器上,测试可注入记录型
/// mock;服务器集成时包住 [`VmPageAllocator`](crate::alloc_page::VmPageAllocator)。
///
/// 簿记用普通 `Vec`:DMA 分配是低频事件(每个 virtqueue/缓冲区一次),
/// 线性查找的常数成本可忽略;上限即物理内存能承载的并发 DMA 区域数,
/// 不需要额外设界。
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

    /// 直接测试簿记(在役区域数)。
    #[cfg(test)]
    pub(crate) fn live_count(&self) -> usize {
        self.live.len()
    }
}

impl<A: PfnAllocator> DmaMemory for VmDmaMemory<A> {
    fn dma_alloc(&mut self, size: u32, align: u32) -> Result<DmaRegion, Errno> {
        // 契约的对齐前提是 2 的幂(设备描述符与环形缓冲的天然形状);
        // 非幂对齐无法用"推进起点"表达,按不可服务拒绝。零长度没有
        // 可用语义,同理 EINVAL。
        if size == 0 || align == 0 || !align.is_power_of_two() {
            return Err(Errno::EINVAL);
        }
        let align = (align as u64).max(PAGE_SIZE);
        let pages = (size as u64).div_ceil(PAGE_SIZE) as u32;
        // 对齐超出页粒度时,多要 align/PAGE - 1 页松弛,运行起点最多
        // 推进这么多页就能命中对齐线。
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

        // Direct Map 线性可见:CPU 侧首地址 = 窗口基 + 物理(常数偏移,
        // 不建映射——[ARCH: A-1])。
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
        // 契约:free 不可能失败,双重释放/外来区域是生产者自身的 bug。
        // 生产者自检:找不到簿记条目时防御性不动物理页(放开会导致
        // 任意物理页被归还),debug 构建直接断言暴露调用方错误。
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

    /// 记录型页分配器 mock:脚本化连续运行,记录释放序列。
    struct MockPfn {
        /// 依次出队的"连续运行基 PFN"(None = 资源耗尽)。
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

    /// 挂一个 4 页窗口的 direct map(translation 断言需要真窗口)。
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
            // 基址落在 0x21 页(非 2 页对齐),要 2 页对齐 → 松弛 1 页,
            // 起点推进到 0x22。
            let mut dma = VmDmaMemory::new(MockPfn::scripted(&[Ok(0x21)]));
            let r = dma
                .dma_alloc(PAGE_SIZE as u32, 2 * PAGE_SIZE as u32)
                .expect("fits");
            let want = 0x22u64 * PAGE_SIZE;
            assert_eq!(r.phys, want, "运行起点推进到对齐线");
            assert_eq!(r.cpu_addr, crate::direct_map::test_vm_base() + want);
            // 释放按实际基址整段归还(0x21..0x23,含松弛)。
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
            // 区内偏移双向平移。
            let inside = VirBytes(r.cpu_addr + 0x123);
            let phys = dma.virtual_to_physical(inside).expect("区内");
            assert_eq!(phys.0, r.phys + 0x123);
            assert_eq!(dma.physical_to_virtual(phys), Some(inside));
            // 区外(含 DMA 长度之外与完全无关地址)双向 None。
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

            // 正常释放路径:整段页归还分配器。
            let r2 = dma
                .dma_alloc(PAGE_SIZE as u32, PAGE_SIZE as u32)
                .expect("second");
            dma.dma_free(r2);
            assert_eq!(dma.live_count(), 0);
        });
    }

    /// 外来区域是生产者自身的 bug(契约原文)——debug 构建的断言就是
    /// "生产者自检"的落地;release 构建防御性不动物理页。
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
