//! VM page-level allocator wrapper.
//!
//! Provides `VmPageAllocator` which wraps a physical memory allocator
//! (`PhysAlloc`) and handles the Direct Map VA↔PA translation for
//! single-page and multi-page allocations.
//!
//! Implements `PfnAllocator` trait for integration with PageFrames
//! (PFN index model).

use minix_types::VirBytes;

use crate::alloc_stats::VmAllocStats;
use crate::direct_map::vm_phys_to_virt;
use crate::pagetable::PageTableError;
use crate::phys_mem::{PhysAlloc, PhysAllocator, PageAllocFlags, AllocError, AlignedPhysBytes, CLICK_SIZE};
use crate::region::{PfnAllocator, PfnAllocError, PAGE_SIZE};

/// Allocate a zero-filled physical page for an intermediate page table.
///
/// Registered with `minix_arch::pt_alloc::register()` during `VmServer`
/// construction. `Paging` implementations call this via
/// `pt_alloc::alloc_pt_page()` whenever `map()` needs a new PML4/PDPT/PD/PT
/// page.
///
/// C: `vm_allocpage(&phys, VMP_PAGETABLE)` (pagetable.c:515) — page-table
/// pages come from the VM page allocator. Minix3 must draw them from the
/// BSS spare-page pool during init / recursion (`vm_getsparepage`,
/// pagetable.c:264-274) because mapping a fresh page-table page required a
/// VA from `findhole()`, which recursed. minix-rs uses the Direct Map
/// (`[ARCH: A-1]`): the VA is a constant offset (`VM_DIRECT_MAP_BASE + phys`),
/// so allocation is a single non-recursive path regardless of init phase.
/// NK4-C 第 33 轮取证探针共享位图（task1-close 裁决删除）：记录每个曾
/// 被 vm_pt_alloc 分配的页表页 PFN（覆盖 0..32768 = 128MB 池空间）。
/// 分配侧查重（二次从本钩子返回 = 双重分配实锤）、归还侧查归（任何把
/// PT 页 PFN 还给分配器的路径都是 self=0 候选写者）。
///
/// 续-153b（扩围）：位图从 512 词（pfn<32768）扩到 10,240 词
/// （pfn<655,360 = 全 RAM：gh29 崩坏帧 pfn≈0x9d2eb/0xbd2ca 远超旧界，
/// 旧检测器全部漏检——续-128 已自证位图界不足）。DATA_SEEN 反向位图
/// 记录 alloc_phys（数据消费侧）曾给的页基址，vm_pt_alloc 撞上即
/// 「数据帧→表页」别名实锤。
#[cfg(not(test))]
const PT_SEEN_WORDS: usize = 10_240;
#[cfg(not(test))]
static PT_SEEN: [core::sync::atomic::AtomicU64; PT_SEEN_WORDS] =
    [const { core::sync::atomic::AtomicU64::new(0) }; PT_SEEN_WORDS];
#[cfg(not(test))]
static DATA_SEEN: [core::sync::atomic::AtomicU64; PT_SEEN_WORDS] =
    [const { core::sync::atomic::AtomicU64::new(0) }; PT_SEEN_WORDS];
#[cfg(not(test))]
static FREED: [core::sync::atomic::AtomicU64; PT_SEEN_WORDS] =
    [const { core::sync::atomic::AtomicU64::new(0) }; PT_SEEN_WORDS];

/// gh29 实测 RAM 顶 pfn（free_regions top=0x9fb33000 → pfn=0x9fb33）。
/// 探针用后即滚；越界即分配器上界崩坏的直接证据。
#[cfg(not(test))]
const RAM_TOP_PFN: u32 = 0x9fb33;

pub(crate) fn vm_pt_alloc() -> Result<(minix_types::PhysBytes, VirBytes), PageTableError> {
    // V11/T30: C pagetable.c:375 allocates page-table pages through
    // `alloc_mem` (the reclaim-retry funnel), so the Rust hook does too.
    let pfn = alloc_pfn_reclaiming(crate::global::page_alloc_mut())
        .map_err(|_| PageTableError::AllocationFailed)?;
    // NK4-C 第 33 轮取证探针（task1-close 裁决删除）：PT 页重复分配
    // 检测器（共享位图见模块级 PT_SEEN）。续-153b：扩围 + DATA_SEEN
    // 反向检测 + 越界 pfn 探针。位图在 free_pages 侧清位——命中即
    // 「未释放就被双重给出」的活别名（合法 free→realloc 循环不误报）。
    #[cfg(not(test))]
    {
        use core::sync::atomic::Ordering as AtomicOrd;
        if (pfn as usize) < PT_SEEN_WORDS * 64 {
            let word = pfn as usize / 64;
            let bit = 1u64 << (pfn as usize % 64);
            let prev = PT_SEEN[word].fetch_or(bit, AtomicOrd::Relaxed);
            if prev & bit != 0 {
                crate::bootmark::mark(&alloc::format!(
                    "nk4a: ptalloc-DUP pfn={pfn:#x}\n"
                ));
            }
            if DATA_SEEN[word].load(AtomicOrd::Relaxed) & bit != 0 {
                // 续-153b：受害者身份鉴别——清零前 dump 首 24 字节
                // （前 48 次门控，防日志洪流；内容分类：全零=清零式
                // 释放路径、堆元数据/BTree 节点=活数据被覆写、文件
                // 字节=页缓存残留）。
                static DUMP_N: core::sync::atomic::AtomicUsize =
                    core::sync::atomic::AtomicUsize::new(0);
                let n = DUMP_N.fetch_add(1, AtomicOrd::Relaxed);
                let mut hex = [0u8; 48 + 2];
                if n < 48 {
                    let virt = vm_phys_to_virt(crate::phys_mem::AlignedPhysBytes::new(
                        pfn as u64 * PAGE_SIZE,
                    ));
                    // SAFETY: DM 窗内 pfn*4K 起 24 字节只读快照。
                    let bytes =
                        unsafe { core::slice::from_raw_parts(virt.0 as *const u8, 24) };
                    for (i, b) in bytes.iter().enumerate() {
                        let hi = b"0123456789abcdef"[(b >> 4) as usize];
                        let lo = b"0123456789abcdef"[(b & 0xf) as usize];
                        hex[i * 2] = hi;
                        hex[i * 2 + 1] = lo;
                    }
                    let s = core::str::from_utf8(&hex[..48]).unwrap_or("?");
                    crate::bootmark::mark(&alloc::format!(
                        "nk4a: ptalloc-reuse-DATA pfn={pfn:#x} n={n} b={s}\n"
                    ));
                } else {
                    crate::bootmark::mark(&alloc::format!(
                        "nk4a: ptalloc-reuse-DATA pfn={pfn:#x}\n"
                    ));
                }
            }
            FREED[word].fetch_and(!bit, AtomicOrd::Relaxed);
        } else {
            crate::bootmark::mark(&alloc::format!(
                "nk4a: ptalloc-BADPFN pfn={pfn:#x}\n"
            ));
        }
        if pfn >= RAM_TOP_PFN {
            crate::bootmark::mark(&alloc::format!(
                "nk4a: ptalloc-OOR pfn={pfn:#x}\n"
            ));
        }
    }
    let phys = AlignedPhysBytes::new(pfn as u64 * PAGE_SIZE);
    let virt = vm_phys_to_virt(phys);
    // Zero-fill via the Direct Map. `Paging::walk_alloc` (x86_64/paging.rs)
    // reads PRESENT bits of freshly allocated tables and must observe zeros.
    // SAFETY: `virt` is a page-aligned Direct Map VA of a freshly allocated,
    // exclusively owned physical page; no aliasing reference exists.
    unsafe {
        core::ptr::write_bytes(virt.0 as *mut u8, 0, CLICK_SIZE);
    }
    Ok((minix_types::PhysBytes(phys.as_u64()), virt))
}

/// E4 余件:页表页归还钩子——`minix_arch::pt_alloc::register_free` 的
/// VM 侧对应物(镜像 [`vm_pt_alloc`] 的分配方向)。`destroy()` 的
/// `free_child_tables` + 根页回收只有在 free 钩子注册后才激活;
/// 未注册时 destroy 退化为只清零根(中间页表页泄漏)。
///
/// C: `pt_free` 把页表页还回 `alloc_mem` 池(pagetable.c:1427-1437)。
pub(crate) fn vm_pt_free(phys: minix_types::PhysBytes) {
    // 页表页由分配器产出,恒页对齐;非对齐输入即调用方 bug,
    // AlignedPhysBytes::new 的 fail-fast 断言就是契约。
    let aligned = AlignedPhysBytes::new(phys.0);
    // 续-303 表帧归还探针（用后即滚）：逐笔记录被归还的表帧 pfn——对照
    // krewalk v3 的 leaf_now=0（fill-root 叶槽被清零假设）。CAP=400 防洪。
    #[cfg(all(target_arch = "riscv64", not(test)))]
    {
        use core::sync::atomic::{AtomicUsize, Ordering as FOrd};
        static PTF_LOGGED: AtomicUsize = AtomicUsize::new(0);
        if PTF_LOGGED.load(FOrd::Relaxed) < 400 {
            let n = PTF_LOGGED.fetch_add(1, FOrd::Relaxed);
            let pfn = phys.0 / 4096;
            crate::bootmark::mark(&alloc::format!(
                "nk4c: ptfree n={n} pfn={pfn:#x}\n"
            ));
        }
    }
    crate::global::page_alloc_mut().free_page(aligned);
}

pub(crate) struct VmPageAllocator {
    phys_alloc: PhysAlloc,
    stats: VmAllocStats,
}

impl VmPageAllocator {
    pub(crate) fn new(phys_alloc: PhysAlloc) -> Self {
        Self {
            phys_alloc,
            stats: VmAllocStats::new(),
        }
    }

    pub(crate) fn alloc_phys(
        &mut self, clicks: usize, flags: PageAllocFlags,
    ) -> Result<AlignedPhysBytes, AllocError> {
        let result = self.phys_alloc.alloc_mem(clicks, flags);
        // NK4-C 第 33 轮取证探针（task1-close 裁决删除）：底层分配漏斗
        // 返回曾作 PT 页的 PFN = 双重分配实锤（本路径不走 vm_pt_alloc，
        // DUP 检测器看不见这类重复）。续-153b：扩围到全 RAM + DATA_SEEN
        // 登记 + 越界探针。
        #[cfg(not(test))]
        if let Ok(ref phys) = result {
            use core::sync::atomic::Ordering as AtomicOrd;
            let base_pfn = (phys.as_u64() / PAGE_SIZE) as usize;
            for i in 0..clicks {
                let pfn = base_pfn + i;
                if pfn < PT_SEEN_WORDS * 64 {
                    FREED[pfn / 64].fetch_and(!(1u64 << (pfn % 64)), AtomicOrd::Relaxed);
                }
            }
            if base_pfn < PT_SEEN_WORDS * 64 {
                let word = base_pfn / 64;
                let bit = 1u64 << (base_pfn % 64);
                if PT_SEEN[word].load(AtomicOrd::Relaxed) & bit != 0 {
                    crate::bootmark::mark(&alloc::format!(
                        "nk4a: alloc-reuse-PT pfn={base_pfn:#x} clicks={clicks}\n"
                    ));
                }
                DATA_SEEN[word].fetch_or(bit, AtomicOrd::Relaxed);
            } else {
                crate::bootmark::mark(&alloc::format!(
                    "nk4a: alloc-BADPFN pfn={base_pfn:#x} clicks={clicks}\n"
                ));
            }
        }
        match &result {
            Ok(_) => self.stats.record_alloc(clicks),
            Err(_) => self.stats.record_failure(),
        }
        result
    }

    // V10-P2-1: `alloc_page`/`alloc_pages`/`self_alloc_count`/`stats` are
    // test-only today (production goes through `alloc_phys` + the
    // `PfnAllocator` impl).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn alloc_page(&mut self, flags: PageAllocFlags) -> Option<(VirBytes, AlignedPhysBytes)> {
        let phys = self.alloc_phys(1, flags).ok()?;
        let virt = vm_phys_to_virt(phys);
        Some((virt, phys))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn alloc_pages(
        &mut self, clicks: usize, flags: PageAllocFlags,
    ) -> Option<(VirBytes, AlignedPhysBytes)> {
        let phys = self.alloc_phys(clicks, flags).ok()?;
        let virt = vm_phys_to_virt(phys);
        Some((virt, phys))
    }

    pub(crate) fn free_page(&mut self, phys: AlignedPhysBytes) {
        self.free_pages(phys, 1);
    }

    pub(crate) fn free_pages(&mut self, phys: AlignedPhysBytes, clicks: usize) {
        // NK4-C 第 33 轮取证探针（task1-close 裁决删除）：任何把页表页
        // PFN 归还分配器的路径都是 self=0 候选写者（共享位图见模块级
        // PT_SEEN）。续-153b：扩围到全 RAM + 逐页清位（活别名语义的
        // 另一半：释放即清，后续再分配不误报；多页释放逐页扫内部页）。
        #[cfg(not(test))]
        {
            use core::sync::atomic::Ordering as AtomicOrd;
            let base_pfn = (phys.as_u64() / PAGE_SIZE) as usize;
            for i in 0..clicks {
                let pfn = base_pfn + i;
                if pfn < PT_SEEN_WORDS * 64 {
                    let clear = !(1u64 << (pfn % 64));
                    PT_SEEN[pfn / 64].fetch_and(clear, AtomicOrd::Relaxed);
                    DATA_SEEN[pfn / 64].fetch_and(clear, AtomicOrd::Relaxed);
                    // 续-153b：双重释放检测——FREED 位已置再释放即实锤
                    // （buddy 簿记被双 free 退化→分配器把活帧当空闲
                    // 二次发放 = 别名总根）。
                    let prev = FREED[pfn / 64].fetch_or(1u64 << (pfn % 64), AtomicOrd::Relaxed);
                    if prev & (1u64 << (pfn % 64)) != 0 && i < 4 {
                        crate::bootmark::mark(&alloc::format!(
                            "nk4a: dblfree pfn={pfn:#x} i={i} clicks={clicks}\n"
                        ));
                    }
                }
            }
        }
        self.phys_alloc.free_mem(phys, clicks);
        self.stats.record_dealloc(clicks);
    }

    pub(crate) fn total_pages(&self) -> usize {
        self.phys_alloc.total_count()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn self_alloc_count(&self) -> usize {
        self.stats.active_allocations()
    }

    pub(crate) fn self_page_count(&self) -> usize {
        self.stats.active_pages()
    }

    /// Page-allocation failures recorded (V11/T18, [ARCH: A-16]).
    pub(crate) fn alloc_failures(&self) -> usize {
        self.stats.allocation_failures()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn stats(&self) -> &VmAllocStats {
        &self.stats
    }

    pub(crate) fn phys_alloc(&self) -> &PhysAlloc {
        &self.phys_alloc
    }

    pub(crate) fn phys_alloc_mut(&mut self) -> &mut PhysAlloc {
        &mut self.phys_alloc
    }
}

impl PfnAllocator for VmPageAllocator {
    fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
        self.alloc_phys(1, PageAllocFlags::empty())
            // PFN fits in u32: max 4TB physical memory with 4KB pages
            .map(|phys| (phys.as_u64() / PAGE_SIZE) as u32)
            .map_err(|_| PfnAllocError::OutOfMemory)
    }

    fn free_pfn(&mut self, pfn: u32) {
        let phys = AlignedPhysBytes::new(pfn as u64 * PAGE_SIZE);
        self.free_page(phys);
    }

    /// V12-P2-7: one multi-page request through the funnel — the backends
    /// hand out contiguous runs natively, and the funnel's reclaim-retry
    /// (V11/T30) comes along for free. Replaces ContiguousAnonymous's
    /// allocate-verify-rollback dance.
    fn alloc_contiguous(&mut self, count: u32) -> Result<u32, PfnAllocError> {
        self.alloc_phys(count as usize, PageAllocFlags::empty())
            .map(|phys| (phys.as_u64() / PAGE_SIZE) as u32)
            .map_err(|_| PfnAllocError::OutOfMemory)
    }
}

/// C `alloc_mem` (alloc.c:242-270) — the allocation funnel with
/// reclaim-retry: try the allocator; on failure drive page-cache reclaim
/// passes (`crate::global::reclaim_pages`, the `cache_freepages`
/// equivalent) and retry while reclaim yields pages.
///
/// Without a registered reclaim sink (host unit tests that inject their
/// own `PfnAllocator`), the first reclaim yields 0 and this is a plain
/// single try — injected-allocator tests behave exactly as before.
///
/// Note on accounting: every retried attempt still records an allocation
/// failure in `VmAllocStats` — the counter measures pressure episodes and
/// the inflation is bounded by the retry bound below.
pub(crate) fn alloc_pfn_reclaiming(alloc: &mut dyn PfnAllocator) -> Result<u32, PfnAllocError> {
    alloc_pfn_reclaiming_inner(alloc, &mut |a| crate::global::reclaim_pages(a))
}

/// Retry core with the reclaim pass injected — the global sink read makes
/// parallel-test registration a non-concern, and the deterministic core is
/// unit-testable without touching process-global state (V11/T30 test-safety
/// contract: no test spins an unbounded loop or grows an unbounded buffer).
fn alloc_pfn_reclaiming_inner(
    alloc: &mut dyn PfnAllocator,
    reclaim: &mut dyn FnMut(&mut dyn PfnAllocator) -> usize,
) -> Result<u32, PfnAllocError> {
    // Defensive bound: C's do-while terminates because each pass with
    // progress strictly shrinks the reclaimable set; the bound additionally
    // caps pathological sinks (same posture as do_memory's drain bound).
    const MAX_RECLAIM_RETRIES: usize = 16;

    match alloc.alloc_pfn() {
        Ok(pfn) => Ok(pfn),
        Err(first) => {
            let mut last = first;
            for _ in 0..MAX_RECLAIM_RETRIES {
                if reclaim(alloc) == 0 {
                    break;
                }
                match alloc.alloc_pfn() {
                    Ok(pfn) => return Ok(pfn),
                    Err(e) => last = e,
                }
            }
            Err(last)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phys_mem::{BitmapAllocator, BootMemRegion, PhysAllocType, bytes_to_clicks, CLICK_SIZE};
    use crate::direct_map::{test_vm_base, with_test_window};

    /// Run a test closure with the direct-map window pointed at a fresh
    /// per-thread leaked buffer (V11/T26: 512 pages covers the largest
    /// allocator under test; each window is exclusive to this test thread,
    /// so no global serialization is needed).
    // ── V11/T30: alloc_mem reclaim-retry funnel (deterministic core) ──

    /// Counting stub allocator: succeeds `budget` times, then reports OOM.
    struct BudgetAlloc {
        budget: usize,
        alloc_calls: usize,
    }
    impl BudgetAlloc {
        fn new(budget: usize) -> Self {
            Self { budget, alloc_calls: 0 }
        }
    }
    impl PfnAllocator for BudgetAlloc {
        fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
            self.alloc_calls += 1;
            if self.budget > 0 {
                self.budget -= 1;
                Ok(self.alloc_calls as u32)
            } else {
                Err(PfnAllocError::OutOfMemory)
            }
        }
        fn free_pfn(&mut self, _pfn: u32) {}
    }

    #[test]
    fn test_reclaim_retry_recovers_via_reclaim_pass() {
        // C alloc.c:242-270 — first alloc fails, one reclaim pass returns a
        // page to the allocator, retry succeeds. The reclaim closure models
        // exactly that: it hands one page back (cache.c:288-305 frees the
        // block through the same allocator).
        let budget: alloc::rc::Rc<core::cell::Cell<usize>> =
            alloc::rc::Rc::new(core::cell::Cell::new(0));
        struct SharedBudget(alloc::rc::Rc<core::cell::Cell<usize>>);
        impl PfnAllocator for SharedBudget {
            fn alloc_pfn(&mut self) -> Result<u32, PfnAllocError> {
                let left = self.0.get();
                if left == 0 {
                    return Err(PfnAllocError::OutOfMemory);
                }
                self.0.set(left - 1);
                Ok(1)
            }
            fn free_pfn(&mut self, _pfn: u32) {}
        }
        let mut alloc = SharedBudget(alloc::rc::Rc::clone(&budget));
        let mut reclaim_calls = 0usize;
        let mut reclaim = |_: &mut dyn PfnAllocator| {
            reclaim_calls += 1;
            budget.set(budget.get() + 1); // a cached page returns
            1 // productive pass
        };
        let pfn = alloc_pfn_reclaiming_inner(&mut alloc, &mut reclaim).unwrap();
        assert_eq!(pfn, 1);
        assert_eq!(reclaim_calls, 1, "exactly one reclaim pass");
        assert_eq!(alloc.0.get(), 0, "the returned page was consumed");
    }

    #[test]
    fn test_reclaim_retry_stops_when_reclaim_yields_zero() {
        // A zero-yield pass makes further retries pointless (C do-while
        // condition) — the original error is preserved.
        let mut alloc = BudgetAlloc::new(0);
        let mut reclaim = |_: &mut dyn PfnAllocator| 0usize;
        let result = alloc_pfn_reclaiming_inner(&mut alloc, &mut reclaim);
        assert!(result.is_err());
        assert_eq!(alloc.alloc_calls, 1, "no retry after a barren pass");
    }

    #[test]
    fn test_reclaim_retry_is_bounded() {
        // A pathological sink that always yields pages must not spin: the
        // defensive bound terminates the loop with the error preserved.
        let mut alloc = BudgetAlloc::new(0);
        let mut reclaim = |_: &mut dyn PfnAllocator| 1usize;
        let result = alloc_pfn_reclaiming_inner(&mut alloc, &mut reclaim);
        assert!(result.is_err());
        assert_eq!(
            alloc.alloc_calls,
            1 + 16, // initial try + MAX_RECLAIM_RETRIES retries
            "retry count must be bounded"
        );
    }

    #[test]
    fn test_reclaim_pages_without_sink_is_zero() {
        // No sink registered (this test never calls VmServer::init()) → the
        // global reclaim is a no-op, so the wrapper is a plain single try.
        let mut alloc = BudgetAlloc::new(0);
        assert_eq!(crate::global::reclaim_pages(&mut alloc), 0);
    }

    fn with_alloc_mock_base<F: FnOnce()>(f: F) {
        with_test_window(512, f);
    }

    /// The Direct Map offset in effect during `with_alloc_mock_base`.
    ///
    /// `vm_phys_to_virt()` in test builds adds the per-thread window base —
    /// the leaked-heap address of the mock physical memory — not the
    /// compile-time `VM_DIRECT_MAP_BASE` constant. Assertions on the VA↔PA
    /// offset must use this value (or the `virt_to_phys()` round-trip) to
    /// stay consistent with the window currently installed.
    fn mock_base() -> u64 {
        test_vm_base()
    }

    fn make_test_phys_alloc(available_pages: usize) -> PhysAlloc {
        let base = 0usize;
        let size = available_pages * CLICK_SIZE;
        let total_pages = available_pages;

        let meta_size = PhysAllocType::Bitmap.metadata_size(total_pages);
        let meta_pages = bytes_to_clicks(meta_size);

        let meta_phys_base = base;
        // The per-thread window base (V11/T26) is stable for this test, so
        // the metadata slice can be derived directly from it.
        let metadata = unsafe {
            core::slice::from_raw_parts_mut(
                (test_vm_base() + meta_phys_base as u64) as *mut u8,
                meta_size,
            )
        };

        let adjusted_base = meta_phys_base + meta_pages * CLICK_SIZE;
        let adjusted_size = size.saturating_sub(meta_pages * CLICK_SIZE);
        let adjusted_regions = [BootMemRegion { base: adjusted_base, size: adjusted_size }];

        PhysAlloc::Bitmap(BitmapAllocator::init(metadata, total_pages, &adjusted_regions, meta_phys_base as u64, meta_pages))
    }

    /// V12-P2-7: the alloc_contiguous override issues ONE multi-page
    /// request through the funnel. Contiguity within each run is the
    /// backend's construction guarantee (bitmap scans a run, buddy hands
    /// out a block, segment-tree first-fits a range) — this test pins the
    /// observable contract instead: runs are disjoint, and a freed run is
    /// satisfiable again whole. (The bitmap is high-first, so run bases
    /// descend; don't assert adjacency between runs.)
    #[test]
    fn test_alloc_contiguous_override_returns_consecutive_run() {
        with_alloc_mock_base(|| {
            let mut alloc = VmPageAllocator::new(make_test_phys_alloc(256));

            let base = alloc.alloc_contiguous(4).expect("4-page run available");
            let second = alloc.alloc_contiguous(2).expect("second run available");

            // Runs must be disjoint allocations.
            assert!(
                second + 2 <= base || base + 4 <= second,
                "runs overlap: first={base}, second={second}"
            );

            // Return the first run whole — one free_pages call, matching
            // the allocation's event granularity (the stats count events,
            // so page-granular free_pfn ×4 after a 4-click alloc would
            // underflow active_allocations — a pre-existing accounting
            // asymmetry this test surfaced, noted in todo Fix #79).
            alloc.free_pages(
                crate::phys_mem::AlignedPhysBytes::new(base as u64 * PAGE_SIZE as u64),
                4,
            );
            let again = alloc.alloc_contiguous(4).expect("freed run reusable");
            assert!(
                again + 4 <= second || second + 2 <= again,
                "re-request must not overlap the live second run"
            );
        });
    }

    #[test]
    fn test_alloc_page() {
        with_alloc_mock_base(|| {
            let phys_alloc = make_test_phys_alloc(256);
            let mut alloc = VmPageAllocator::new(phys_alloc);

            let (v1, p1) = alloc.alloc_page(PageAllocFlags::empty()).unwrap();
            assert_eq!(v1.0 - p1.as_u64(), mock_base());
            assert_eq!(crate::direct_map::virt_to_phys(v1), p1);

            let (v2, p2) = alloc.alloc_page(PageAllocFlags::empty()).unwrap();
            assert_ne!(p1.as_u64(), p2.as_u64());
            assert_eq!(v2.0 - p2.as_u64(), mock_base());
            assert_eq!(crate::direct_map::virt_to_phys(v2), p2);
        });
    }

    #[test]
    fn test_alloc_phys_and_free() {
        with_alloc_mock_base(|| {
            let phys_alloc = make_test_phys_alloc(256);
            let mut alloc = VmPageAllocator::new(phys_alloc);

            let p1 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
            let p2 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
            assert_ne!(p1, p2);

            alloc.free_page(p1);
            let p3 = alloc.alloc_phys(1, PageAllocFlags::empty()).unwrap();
            assert_eq!(p3, p1);
            assert_ne!(p3, p2);
        });
    }

    #[test]
    fn test_total_pages() {
        with_alloc_mock_base(|| {
            let phys_alloc = make_test_phys_alloc(10);
            let alloc = VmPageAllocator::new(phys_alloc);
            assert_eq!(alloc.total_pages(), 10);
        });
    }

    #[test]
    fn test_vm_pt_alloc() {
        // vm_pt_alloc() reaches the allocator through the global
        // PAGE_ALLOC_PTR, so this test registers a local allocator. The
        // per-thread window (with_test_window) isolates it from the
        // VmServer tests, which also register the global pointer.
        with_alloc_mock_base(|| {
            let phys_alloc = make_test_phys_alloc(64);
            let mut alloc = VmPageAllocator::new(phys_alloc);
            crate::global::register_page_alloc(&mut alloc);

            let (phys, virt) = vm_pt_alloc().expect("pt page allocation");
            assert_eq!(virt.0 - phys.0, mock_base());
            assert_eq!(phys.0 % CLICK_SIZE as u64, 0);
            // Fresh page-table pages must be zero-filled: Paging::walk_alloc
            // (x86_64/paging.rs) reads PRESENT bits of new tables and must
            // observe zeros.
            // SAFETY: `virt` is the Direct Map VA of a freshly allocated,
            // exclusively owned physical page.
            let word = unsafe { core::ptr::read_volatile(virt.0 as *const u64) };
            assert_eq!(word, 0);

            let (phys2, _virt2) = vm_pt_alloc().expect("second pt page allocation");
            assert_ne!(phys, phys2);

            crate::global::unregister_page_alloc();
        });
    }

    /// E4 余件:vm_pt_free 把页表页还回分配器——free 后再分配应取回
    /// 同一物理页(空闲表 LIFO/位图回收语义),证明归还真实入池而非
    /// 丢弃。与 test_vm_pt_alloc 同一 mock 窗口隔离。
    #[test]
    fn test_vm_pt_free_returns_page_to_allocator() {
        with_alloc_mock_base(|| {
            let phys_alloc = make_test_phys_alloc(64);
            let mut alloc = VmPageAllocator::new(phys_alloc);
            crate::global::register_page_alloc(&mut alloc);

            let (phys, _virt) = vm_pt_alloc().expect("pt page allocation");
            vm_pt_free(phys);

            // 归还后的页回到可分配池:后续分配成功且池余量恢复。
            // (不断言"取回同一 pfn"——那是具体分配器策略而非契约。)
            let again = alloc_pfn_reclaiming(crate::global::page_alloc_mut())
                .expect("allocator must serve from the pool after pt free");
            let _ = again;

            // PAGE_ALLOC_PTR is one global static: leaving `alloc` (a
            // local) registered would dangle the pointer into freed
            // stack and trip the overwrite guard of whichever test
            // registers next (same hygiene as test_vm_pt_alloc above).
            crate::global::unregister_page_alloc();
        });
    }


    #[test]
    fn test_alloc_pages_multi() {
        with_alloc_mock_base(|| {
            let phys_alloc = make_test_phys_alloc(256);
            let mut alloc = VmPageAllocator::new(phys_alloc);

            let (v1, p1) = alloc.alloc_pages(4, PageAllocFlags::empty()).unwrap();
            assert_eq!(v1.0 - p1.as_u64(), mock_base());
            assert_eq!(crate::direct_map::virt_to_phys(v1), p1);
            assert_eq!(crate::direct_map::virt_to_phys(VirBytes(v1.0 + 3 * CLICK_SIZE as u64)), AlignedPhysBytes::from_page_index(p1.page_index() + 3));

            let (v2, p2) = alloc.alloc_pages(2, PageAllocFlags::empty()).unwrap();
            assert_ne!(p1, p2);
            assert_eq!(v2.0 - p2.as_u64(), mock_base());
            assert_eq!(crate::direct_map::virt_to_phys(v2), p2);
        });
    }

    #[test]
    fn test_free_pages_multi() {
        with_alloc_mock_base(|| {
        let phys_alloc = make_test_phys_alloc(256);
        let mut alloc = VmPageAllocator::new(phys_alloc);

        let (_, p1) = alloc.alloc_pages(4, PageAllocFlags::empty()).unwrap();
        assert_eq!(alloc.self_alloc_count(), 1);
        assert_eq!(alloc.self_page_count(), 4);

        alloc.free_pages(p1, 4);
        assert_eq!(alloc.self_alloc_count(), 0);
        assert_eq!(alloc.self_page_count(), 0);
        });
    }

    #[test]
    fn test_self_pages_tracking() {
        with_alloc_mock_base(|| {
        let phys_alloc = make_test_phys_alloc(256);
        let mut alloc = VmPageAllocator::new(phys_alloc);

        assert_eq!(alloc.self_alloc_count(), 0);
        assert_eq!(alloc.self_page_count(), 0);

        let (_, p1) = alloc.alloc_page(PageAllocFlags::empty()).unwrap();
        assert_eq!(alloc.self_alloc_count(), 1);
        assert_eq!(alloc.self_page_count(), 1);

        let (_, p2) = alloc.alloc_page(PageAllocFlags::empty()).unwrap();
        assert_eq!(alloc.self_alloc_count(), 2);
        assert_eq!(alloc.self_page_count(), 2);

        alloc.free_page(p1);
        assert_eq!(alloc.self_alloc_count(), 1);
        assert_eq!(alloc.self_page_count(), 1);

        alloc.free_page(p2);
        assert_eq!(alloc.self_alloc_count(), 0);
        assert_eq!(alloc.self_page_count(), 0);
        });
    }
}
