//! VM boot handoff construction — A2 post-bootstrap classification.
//!
//! At the classification point (all bootstrap allocations complete: DM
//! coverage page-table pages, PT hierarchy, ELF segments, stacks, handoff
//! frame) the kernel finalizes VM's free list from the **full** memmap by
//! cutting `LiveBootstrap(t_classify)` — bootstrap-used (① PT hierarchy,
//! ② ELF backing, ③ stack/handoff frames) ∪ kernel-reserved (kernel image,
//! boot bump region, reserved module blobs). The deduction record itself
//! is handed over in the same page, so VM can verify
//! `LiveBootstrap ∩ VM-free = ∅` at the consumer boundary
//! (`07-paging_init_design` §6.0-A2, Proof 2) instead of trusting the
//! kernel's bookkeeping.
//!
//! # Deduction ranges are the record
//!
//! Every range is an enumerable choke point (§6.0-A2 provenance audit):
//!
//! | Range | Source |
//! |-------|--------|
//! | kernel image | `KernelInfo.kern_phys_base()/kern_size()` |
//! | VM self root page | `arch_boot` parameter (= handoff `root_paddr`) |
//! | boot bump region | `boot_alloc::boot_alloc_region()` (whole range —
//!   consumed pages and remainder are both kernel-reserved) |
//! | bootstrap-used frames | per-pool-region used suffixes of
//!   `VmBootAllocator` (walk order: descending from each region's end) |
//! | reserved module blobs | `KernelInfo.boot_modules()` minus the VM
//!   module (reclaimed after ELF copy, C: protect.c:450-451) |
//!
//! The free list is then clipped to VM's Direct Map window
//! (`VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap`).
//!
//! All storage is fixed-size — boot is zero-heap.

use crate::boot_alloc::{boot_alloc_region, boot_alloc_used_bytes};
use crate::globals::SyncUnsafeCell;
use crate::memmap::{cut_memmap, MemMapEntry, MAXMEMMAP};
use crate::proc::{BOOT_MODULE_PROC_NRS, KERNEL_TASKS};
use core::sync::atomic::{AtomicUsize, Ordering};
use minix_arch::arch::frame::VmBootAllocator;
use minix_arch::DirectMapArch;
use minix_boot::{KernelInfo, MemoryRegion};
use minix_types::{
    BootImage, Endpoint, HandoffMemRegion, HandoffModule, NR_BOOT_PROCS,
    PhysBytes, VM_BOOT_HANDOFF_MAGIC, VM_BOOT_HANDOFF_MAX_DEDUCTED,
    VM_BOOT_HANDOFF_MAX_IDENT, VM_BOOT_HANDOFF_MAX_MODULES,
    VM_BOOT_HANDOFF_MAX_REGIONS, VM_BOOT_HANDOFF_VERSION, VmBootHandoff,
};

/// Frame size of every `VmBootAllocator` page (C: I386_PAGE_SIZE).
const PAGE_SIZE: u64 = 0x1000;

/// The A2 free list (physical memory still free after the first
/// boot-image load), published by [`build_vm_handoff`] at the
/// classification point.
///
/// Consumers that allocate frames AFTER the first boot-image load —
/// multi-image boot loaders (the C-27 carrier's second image) — must
/// select from this list, not from the raw `KernelInfo::memmap()`: the
/// raw map still advertises the frames the first load consumed
/// (segments, stack, page-table pages, handoff page), and handing them
/// out a second time overwrites the first image's memory. The first
/// process then executes corrupted bytes with a perfectly intact page
/// table (observed on real machine: test-sysboot's VM slot took #UD at
/// its entry once the per-image-root load unblocked the second load).
/// C parity: every `pg_alloc_page` consumer walks the mutated `memmap`
/// (pg_utils.c:138-160), so a later loader never sees spent frames.
///
/// # Concurrency
///
/// Written exactly once, at the classification point (single-threaded
/// boot, no scheduler yet); read-only afterwards. `SyncUnsafeCell`
/// soundness follows the same write-once-then-read contract as
/// `globals::FREE_MEMMAP` (`HandoffMemRegion` is in the approved
/// write-once list).
static VM_PMM_FREE: SyncUnsafeCell<[HandoffMemRegion; VM_BOOT_HANDOFF_MAX_REGIONS]> =
    SyncUnsafeCell::new([HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_REGIONS]);

/// How many leading entries of [`VM_PMM_FREE`] are valid. 0 until the
/// classification point; the Release store pairs with the Acquire load in
/// [`vm_pmm_free_regions`] to publish the array write.
static VM_PMM_FREE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Physical memory still free after the first boot-image load — the A2
/// free list (`VM PMM eligible = conventional ∩ DM-representable −
/// LiveBootstrap`), converted to `MemoryRegion` for
/// `VmBootRegion::select_multi`. Deduction already covers the kernel
/// image, the boot bump region, every bootstrap-consumed frame and the
/// reserved module blobs, so no additional exclusions are needed.
///
/// Returns the populated slice (entries with `len > 0`); empty until
/// `build_vm_handoff` ran (i.e., before the first boot-image load
/// completed — nothing may allocate from this before that point).
pub fn vm_pmm_free_regions() -> ([MemoryRegion; VM_BOOT_HANDOFF_MAX_REGIONS], usize) {
    let count = VM_PMM_FREE_COUNT.load(Ordering::Acquire);
    // SAFETY: write-once-then-read (see `VM_PMM_FREE`); the copy-out is a
    // plain value read of plain data.
    let regions = unsafe { *VM_PMM_FREE.get() };
    let mut out = [MemoryRegion { base: PhysBytes(0), len: 0 }; VM_BOOT_HANDOFF_MAX_REGIONS];
    for (i, region) in regions.iter().enumerate().take(count) {
        out[i] = MemoryRegion { base: PhysBytes(region.base), len: region.size as usize };
    }
    (out, count)
}

/// A2 classification output — the free list and the deduction record.
///
/// The record is what the kernel cut from the memmap; the free list is
/// what survived (clipped to VM's Direct Map window). The two are disjoint
/// by construction (`cut_memmap` on the recorded ranges).
pub struct Classification {
    /// Surviving free ranges, page-aligned, DM-window-clipped.
    pub free_regions: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_REGIONS],
    pub free_region_count: usize,
    /// `LiveBootstrap(t_classify)` — the ranges cut from the memmap.
    pub deducted: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_DEDUCTED],
    pub deducted_count: usize,
}

/// Fixed-capacity collector for deduction ranges (fail-fast on overflow).
struct DeductionCollector {
    ranges: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_DEDUCTED],
    count: usize,
}

impl DeductionCollector {
    const fn new() -> Self {
        Self {
            ranges: [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_DEDUCTED],
            count: 0,
        }
    }

    /// Records one page-rounded deduction range.
    ///
    /// `start` must be page-aligned (every choke-point source is); `len`
    /// is rounded up to the page the way [`cut_memmap`] rounds its cuts,
    /// so the record is exactly the range removed from the memmap.
    fn push(&mut self, start: u64, len: u64) {
        if len == 0 {
            return;
        }
        assert!(
            start & (PAGE_SIZE - 1) == 0,
            "vm_handoff: deduction start 0x{start:x} must be page-aligned"
        );
        let size = len.div_ceil(PAGE_SIZE) * PAGE_SIZE;
        assert!(
            self.count < VM_BOOT_HANDOFF_MAX_DEDUCTED,
            "vm_handoff: deduction record overflow — LiveBootstrap is not enumerable within capacity"
        );
        self.ranges[self.count] = HandoffMemRegion { base: start, size };
        self.count += 1;
    }
}

/// Runs the A2 classification.
///
/// `vm_module_idx` is the boot-module index of VM itself — its blob was
/// reclaimed after the ELF copy (C: protect.c:450-451 `add_memmap` +
/// `mod_start = mod_end = 0`), so it stays free and is not deducted.
///
/// The UEFI boot path allocates the root page and bump region as
/// `LOADER_DATA`, which never appears in the conventional memmap — cutting
/// them is a no-op there. The no-shim fallback path registers its bump
/// region *from* the memmap, and the cut is what keeps it out of VM's free
/// list. Either way the whole registered range is deducted (kernel-reserved
/// includes the bump remainder, §6.0-A2 构成表).
pub fn classify(
    kernel_info: &KernelInfo,
    root_paddr: PhysBytes,
    vm_alloc: &VmBootAllocator,
    vm_module_idx: usize,
) -> Classification {
    // Snapshot the full memmap into a local working copy (C: pre_init
    // operates on kinfo's own memmap copy). MAXMEMMAP bounds it; the
    // boot-shim cannot report more.
    let mut mmap = [MemMapEntry::default(); MAXMEMMAP];
    for (i, region) in kernel_info.memmap().iter().enumerate() {
        assert!(
            i < MAXMEMMAP,
            "vm_handoff: boot-shim memmap exceeds MAXMEMMAP"
        );
        if region.len > 0 {
            mmap[i] = MemMapEntry {
                base: region.base.0,
                length: region.len as u64,
            };
        }
    }

    // ── Build the LiveBootstrap(t_classify) record ──
    let mut record = DeductionCollector::new();

    // Kernel-reserved: kernel image (C: pre_init.c:196-199 treats the
    // kernel as an extra module and cuts it the same way).
    record.push(kernel_info.kern_phys_base().0, kernel_info.kern_size());

    // Kernel-reserved: VM's self root page (A1 identity — the handoff value
    // itself, §6.0-A2 构成表 "root page（handoff 值本身）").
    record.push(root_paddr.0, PAGE_SIZE);

    // Kernel-reserved: the boot bump region, whole range.
    if let Some((base, end)) = boot_alloc_region() {
        record.push(base, end.saturating_sub(base));
    }

    // Bootstrap-used ②③: frames handed out by the VM bootstrap allocator.
    // The allocator walks each pool region from `end` down to `start`, so
    // the consumed part of every touched region is a contiguous suffix:
    // regions before `idx` are fully consumed, region `idx` contributes
    // `[cursor, end)`, later regions nothing.
    let pool = vm_alloc.regions();
    let idx = vm_alloc.current_region_idx();
    let cursor = vm_alloc.cursor().0;
    for i in 0..pool.len().min(idx + 1) {
        let region = pool[i];
        let used_start = if i < idx {
            region.start.0
        } else {
            cursor.max(region.start.0)
        };
        if used_start < region.end.0 {
            record.push(used_start, region.end.0 - used_start);
        }
    }

    // Kernel-reserved: every boot module blob INCLUDING VM's own.
    // 续-187 成修：旧逻辑对 VM 模块做 reclaim（为 exec copy-out 腾帧），
    // 但 kernel 的 boot-exec 在 VM 启动前逐台装载服务器 0..7——它们的段
    // 帧从本 free list 取，VM blob 被取走即 VM 镜像启动前被覆写（gh53/
    // 58：VM text 页非法指令→SIGSEGV-for-itself，p_fault_addr=None 非
    // PF）。625KB 的让利不值得换启动前腐坏：全量保留。
    assert!(
        vm_module_idx < kernel_info.boot_modules().len(),
        "vm_handoff: VM module index {} out of bounds",
        vm_module_idx
    );
    for module in kernel_info.boot_modules().iter() {
        record.push(module.start.0, module.len as u64);
    }

    // ── Cut the record out of the memmap (same-family exclusion as
    //    `VmBootRegion::select_multi` — frame.rs invariant 1) ──
    for range in &record.ranges[..record.count] {
        // Every recorded range is page-aligned by construction.
        cut_memmap(&mut mmap, range.base, range.size)
            .expect("vm_handoff: cut_memmap failed — memmap slot exhaustion");
    }

    // ── Clip survivors to VM's Direct Map window ──
    // VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap
    // (§6.0-A2): a region the VM DM window cannot represent cannot be
    // accessed through vm_phys_to_virt, so it must not enter the free list.
    let window_end = minix_arch::CurrentDirectMap::VM_DIRECT_MAP_SIZE;
    let mut free_regions = [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_REGIONS];
    let mut free_count = 0usize;
    for entry in mmap.iter().take(MAXMEMMAP) {
        if entry.is_empty() {
            continue;
        }
        let base = entry.base;
        let end = entry.base.saturating_add(entry.length);
        let clipped_end = end.min(window_end);
        if base >= clipped_end {
            continue; // entirely outside the window
        }
        assert!(
            free_count < VM_BOOT_HANDOFF_MAX_REGIONS,
            "vm_handoff: free-region list overflow — raise VM_BOOT_HANDOFF_MAX_REGIONS"
        );
        free_regions[free_count] = HandoffMemRegion {
            base,
            size: clipped_end - base,
        };
        free_count += 1;
    }

    // NK4-A 取证（fix18 临时路标，task1-close 裁决去留）：分类幸存区可见
    // 性——VM PMM 只有 ~133 页可用（grow(229) PhysicalAllocFailed，
    // 2026-09-21 栈取证），需要知道是 memmap 快照本来就小，还是扣减切多
    // 了。只打前 8 条，避免刷屏。
    {
        use minix_plat::{CurrentEarlyConsole as Console, EarlyConsole as _};
        Console::write_str("kernel: vm_handoff free n=");
        Console::write_hex(free_count as u64);
        Console::write_str(" deducted=");
        Console::write_hex(record.count as u64);
        Console::write_str("\n");
        for r in free_regions.iter().take(free_count).take(8) {
            Console::write_str("  base=");
            Console::write_hex(r.base);
            Console::write_str(" size=");
            Console::write_hex(r.size);
            Console::write_str("\n");
        }
    }

    Classification {
        free_regions,
        free_region_count: free_count,
        deducted: record.ranges,
        deducted_count: record.count,
    }
}

/// Builds the full handoff page contents (A1 root + A2 classification +
/// boot tables + kernel footprint).
///
/// Called once at the classification point, after the handoff frame itself
/// was allocated (so `used_frames` already counts it).
pub fn build_vm_handoff(
    kernel_info: &KernelInfo,
    root_paddr: PhysBytes,
    vm_alloc: &VmBootAllocator,
    vm_module_idx: usize,
) -> VmBootHandoff {
    let classification = classify(kernel_info, root_paddr, vm_alloc, vm_module_idx);

    // Publish the A2 free list for post-first-load frame consumers (see
    // [`VM_PMM_FREE`]) — write-once at the classification point, before
    // any multi-image loader runs.
    {
        // SAFETY: write-once-then-read contract (see `VM_PMM_FREE`).
        let slot = unsafe { &mut *VM_PMM_FREE.get() };
        for (i, region) in classification.free_regions.iter().enumerate() {
            slot[i] = *region;
        }
        VM_PMM_FREE_COUNT.store(classification.free_region_count, Ordering::Release);
    }

    // Reserved module blobs (every module except VM's) — C: kinfo
    // `.module_list[]`, whose still-reserved entries VM charges to the
    // global page total (main.c:485-489).
    let mut modules = [HandoffModule::ZERO; VM_BOOT_HANDOFF_MAX_MODULES];
    let mut module_count = 0usize;
    for (i, module) in kernel_info.boot_modules().iter().enumerate() {
        if i == vm_module_idx {
            continue;
        }
        assert!(
            module_count < VM_BOOT_HANDOFF_MAX_MODULES,
            "vm_handoff: module list overflow"
        );
        modules[module_count] = HandoffModule {
            start_addr: module.start.0,
            len: module.len as u64,
        };
        module_count += 1;
    }

    // C: kinfo_t.boot_procs[NR_BOOT_PROCS] — the full boot image table:
    // kernel tasks first (negative proc_nr, C: table.c image[0..NR_TASKS]),
    // then user-space servers in boot-module order. `endpoint == proc_nr`
    // at generation 0 (C: com.h endpoints).
    let mut boot_procs = [BootImage::empty(); NR_BOOT_PROCS];
    let mut n = 0usize;
    for &(name, nr) in KERNEL_TASKS.iter() {
        boot_procs[n] = BootImage {
            proc_nr: nr.0,
            proc_name: copy_name(name),
            endpoint: Endpoint(nr.0),
            start_addr: 0,
            len: 0,
        };
        n += 1;
    }
    for (i, module) in kernel_info.boot_modules().iter().enumerate() {
        boot_procs[n] = BootImage {
            proc_nr: BOOT_MODULE_PROC_NRS[i].0,
            proc_name: copy_name(module.name),
            endpoint: Endpoint(BOOT_MODULE_PROC_NRS[i].0),
            start_addr: module.start.0,
            len: module.len as u64,
        };
        n += 1;
    }
    debug_assert_eq!(n, NR_BOOT_PROCS, "kernel tasks + boot modules must fill the boot image table");

    // fix27 (handoff v6): kernel runtime identity windows — the VM's
    // `map_kernel` replays them into every process page table so the
    // link-in execution model's low VA=PA code/data/MMIO accesses
    // survive the first CR3 switch (see version history v6).
    let (kern_ident, kern_ident_count) = build_identity_windows(kernel_info);

    VmBootHandoff {
        magic: VM_BOOT_HANDOFF_MAGIC,
        version: VM_BOOT_HANDOFF_VERSION,
        root_paddr: root_paddr.0,
        vm_allocated_bytes: vm_alloc.used_frames() * PAGE_SIZE,
        kernel_allocated_static: kernel_info.kern_size(),
        kernel_allocated_dynamic: boot_alloc_used_bytes(),
        // V11/E3: kernel layout for VM's per-process kernel mappings —
        // C kinfo carries the same role (kernel text/data base + pages).
        // minix-rs maps the whole contiguous image span as "text"
        // (data_pages = 0); the VM's map_kernel walks text_pages then
        // data_pages, so the sum is what matters.
        kern_virt_base: kernel_info.kern_virt_base().0,
        kern_phys_base: kernel_info.kern_phys_base().0,
        kern_text_pages: (kernel_info.kern_size() / 4096) as u32,
        kern_data_pages: 0,
        // fix26 (handoff v5): the real kernel Direct Map window. VM's
        // `map_kernel` replays this window into every process page
        // table; the first CR3 switch to a VM-built table makes it the
        // kernel's own DM access path, so base AND coverage must equal
        // what `establish_boot_dm` installed on the bootstrap root
        // (same candidate union — `kernel_dm_pa_end`).
        kern_dm_vbase: minix_arch::CurrentDirectMap::KERNEL_DIRECT_MAP_BASE,
        kern_dm_pages: crate::dm_coverage::kernel_dm_pa_end(kernel_info, root_paddr) / PAGE_SIZE,
        // E-BOOTFRAME: VM builds boot-proc initial stacks downward from
        // this value. C: execi->stack_high = kernel_boot_info.user_sp
        // (main.c:372); kinfo.user_sp = USR_STACKTOP (pre_init.c:156).
        user_sp: kernel_info.user_sp.0,
        is_first_time: 1,
        free_region_count: classification.free_region_count as u32,
        deducted_count: classification.deducted_count as u32,
        module_count: module_count as u32,
        free_regions: classification.free_regions,
        deducted: classification.deducted,
        modules,
        kern_ident_count: kern_ident_count as u32,
        kern_ident,
        boot_procs,
    }
}

/// Build the kernel runtime identity windows for the handoff (fix27,
/// version 6) — the physical regions `map_kernel` must replay identity
/// (VA = PA, supervisor-only) into every VM-built process page table.
///
/// Sources:
/// - `KernelInfo::reserved_regions()` — the boot-shim's non-conventional
///   memory-map snapshot (running PE image incl. the linked-in kernel,
///   UEFI heap objects, ACPI tables, LOADER_DATA allocations). Empty on
///   platforms without link-in execution (OpenSBI, host tests).
/// - [`minix_arch::kernel_identity_mmio_regions()`] — fixed low-VA MMIO
///   the kernel dereferences at runtime (x86-64: local APIC EOI page).
///
/// Transform: page-align out, drop everything below
/// `USER_IDENTITY_FLOOR`, sort by base, merge overlapping/adjacent
/// regions. Fail-fast on candidate, merged-entry, and total-page caps.
///
/// `USER_IDENTITY_FLOOR` (4 MiB): user-space boot images link their
/// text/data at 2-4 MiB, and NO live kernel object sits below 4 MiB in
/// the UEFI model (the standalone kernel.elf copy at 2-4 MiB is loaded
/// but never executed, and its pages stay marked conventional so they
/// never enter `reserved_regions` anyway). The floor is the defensive
/// half of the identity-window / user-VA non-overlap discipline
/// ([ARCH]: future VM mmap ranges must stay clear of the windows).
pub(crate) fn build_identity_windows(
    kernel_info: &KernelInfo,
) -> ([HandoffMemRegion; VM_BOOT_HANDOFF_MAX_IDENT], usize) {
    const USER_IDENTITY_FLOOR: u64 = 0x40_0000;
    /// Candidate slots: UEFI non-conventional descriptors are a few
    /// dozen on QEMU/OVMF; overflow is a firmware-shape surprise, not
    /// something to truncate silently. fix27d: real machine measured
    /// 128 reserved descriptors after the above-RAM filter (the 4 KiB
    /// fragmented OVMF map is far busier than the "few dozen" the
    /// first cut assumed) — 256 keeps the fail-fast meaningful instead
    /// of firing on the normal shape.
    /// F6：单一常量双引用——与 globals::RESERVED_REGION_STORE_LEN 同源，
    /// 编译期消除两处 256 手工对齐的漂移面。
    const MAX_CANDIDATES: usize = crate::globals::RESERVED_REGION_STORE_LEN;
    /// Sanity cap on 4 KiB identity leaves: 128 Ki pages = 512 MiB.
    /// Blowing this means the "occupied" snapshot swallowed something
    /// enormous (fragmented firmware map or an MMIO-window regression) —
    /// mapping it per-process would exhaust the boot bump pool.
    const MAX_TOTAL_PAGES: u64 = 128 * 1024;

    let mut cand = [(0u64, 0u64); MAX_CANDIDATES];
    let mut n = 0usize;
    // fix27 forensic (2026-09-21): real machine showed res=127 arriving
    // with cand=2 — every reserved entry rejected. Count WHY to split
    // "payload zeroed (dangling slice)" from "all below the floor".
    let mut zeroed = 0usize;
    let mut below_floor = 0usize;
    let mut max_end = 0u64;
    let mut push = |base: u64, size: u64, n: &mut usize| {
        assert!(*n < MAX_CANDIDATES, "vm_handoff: identity candidate overflow");
        if base + size > max_end {
            max_end = base + size;
        }
        if size == 0 {
            zeroed += 1;
            return;
        }
        // align out: floor base, ceil end.
        let b = base & !(PAGE_SIZE - 1);
        let e = (base + size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        // clip below the user-image floor.
        if e <= USER_IDENTITY_FLOOR {
            below_floor += 1;
            return;
        }
        let b = b.max(USER_IDENTITY_FLOOR);
        cand[*n] = (b, e - b);
        *n += 1;
    };
    for r in kernel_info.reserved_regions() {
        push(r.base.0, r.len as u64, &mut n);
    }
    for &(base, size) in minix_arch::kernel_identity_mmio_regions() {
        push(base, size, &mut n);
    }
    // 结束闭包对计数器的可变借用，之后才能再读它们。
    drop(push);
    let res_rejected = zeroed + below_floor;
    let _ = res_rejected;

    // insertion sort by base — n ≤ MAX_CANDIDATES, boot-path sizes only.
    for i in 1..n {
        let mut j = i;
        while j > 0 && cand[j - 1].0 > cand[j].0 {
            cand.swap(j - 1, j);
            j -= 1;
        }
    }

    // merge overlapping/adjacent runs; fail fast on a produced-entry or
    // page-total cap breach (never truncate — a cut window would put
    // the #PF→#DF→triple-fault landmine back).
    let mut ident = [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_IDENT];
    let mut count = 0usize;
    let mut total_pages = 0u64;
    let mut i = 0usize;
    while i < n {
        let (b0, s0) = cand[i];
        let mut end = b0 + s0;
        let mut j = i + 1;
        while j < n && cand[j].0 <= end {
            end = end.max(cand[j].0 + cand[j].1);
            j += 1;
        }
        assert!(count < VM_BOOT_HANDOFF_MAX_IDENT, "vm_handoff: identity window overflow");
        total_pages += (end - b0) / PAGE_SIZE;
        assert!(
            total_pages <= MAX_TOTAL_PAGES,
            "vm_handoff: identity windows exceed the 4KiB-leaf sanity cap"
        );
        ident[count] = HandoffMemRegion {
            base: b0,
            size: end - b0,
        };
        count += 1;
        i = j;
    }

    // NK4-A fix27 取证路标（task1-close 裁决删除）：身份窗口真值必须
    // 在真机上可见——数量决定 rs 切换后低段取指是否活着。mock（宿主
    // 测试）下 console 是真实端口写，必须编译掉（同 lib.rs 路标惯例）。
    #[cfg(not(feature = "mock"))]
    {
        use minix_plat::{CurrentEarlyConsole as C0, EarlyConsole as _};
        C0::write_str("nk4a: ident-windows res=");
        C0::write_hex(kernel_info.reserved_regions().len() as u64);
        C0::write_str(" zero=");
        C0::write_hex(zeroed as u64);
        C0::write_str(" low=");
        C0::write_hex(below_floor as u64);
        C0::write_str(" maxend=");
        C0::write_hex(max_end);
        C0::write_str(" n=");
        C0::write_hex(count as u64);
        C0::write_str("\n");
    }
    let _ = res_rejected;

    (ident, count)
}

/// Copies a boot image name into the fixed `proc_name` field (truncate to
/// 16 bytes, C: `strlcpy`).
fn copy_name(name: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    let bytes = name.as_bytes();
    let len = bytes.len().min(out.len() - 1);
    out[..len].copy_from_slice(&bytes[..len]);
    out
}

/// Classifies a raw memmap slice directly (test entry point — the real
/// boot path snapshots `KernelInfo.memmap()` first).
#[cfg(test)]
pub(crate) fn classify_regions(
    memmap: &[MemoryRegion],
    root_paddr: PhysBytes,
    boot_bump: Option<(u64, u64)>,
    used_suffixes: &[(u64, u64)],
    reserved_modules: &[(u64, u64)],
) -> Classification {
    let mut mmap = [MemMapEntry::default(); MAXMEMMAP];
    for (i, region) in memmap.iter().enumerate() {
        assert!(i < MAXMEMMAP);
        if region.len > 0 {
            mmap[i] = MemMapEntry {
                base: region.base.0,
                length: region.len as u64,
            };
        }
    }

    let mut record = DeductionCollector::new();
    record.push(root_paddr.0, PAGE_SIZE);
    if let Some((base, end)) = boot_bump {
        record.push(base, end.saturating_sub(base));
    }
    for &(start, end) in used_suffixes {
        if start < end {
            record.push(start, end - start);
        }
    }
    for &(start, len) in reserved_modules {
        record.push(start, len);
    }

    for range in &record.ranges[..record.count] {
        cut_memmap(&mut mmap, range.base, range.size)
            .expect("vm_handoff test: cut_memmap failed");
    }

    let window_end = minix_arch::CurrentDirectMap::VM_DIRECT_MAP_SIZE;
    let mut free_regions = [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_REGIONS];
    let mut free_count = 0usize;
    for entry in mmap.iter().take(MAXMEMMAP) {
        if entry.is_empty() {
            continue;
        }
        let base = entry.base;
        let end = entry.base.saturating_add(entry.length);
        let clipped_end = end.min(window_end);
        if base >= clipped_end {
            continue;
        }
        assert!(free_count < VM_BOOT_HANDOFF_MAX_REGIONS);
        free_regions[free_count] = HandoffMemRegion {
            base,
            size: clipped_end - base,
        };
        free_count += 1;
    }

    Classification {
        free_regions,
        free_region_count: free_count,
        deducted: record.ranges,
        deducted_count: record.count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};

    // Re-import the raw-slice classifier under a short alias.
    use super::classify_regions as classify_raw;

    const WIN: u64 = minix_arch::CurrentDirectMap::VM_DIRECT_MAP_SIZE;

    fn mem_region(base: u64, len: u64) -> MemoryRegion {
        MemoryRegion {
            base: PhysBytes(base),
            len: len as usize,
        }
    }

    fn free_ranges(c: &Classification) -> Vec<(u64, u64)> {
        (0..c.free_region_count)
            .map(|i| {
                let r = c.free_regions[i];
                (r.base, r.base + r.size)
            })
            .collect()
    }

    fn deducted_ranges(c: &Classification) -> Vec<(u64, u64)> {
        (0..c.deducted_count)
            .map(|i| {
                let r = c.deducted[i];
                (r.base, r.base + r.size)
            })
            .collect()
    }

    /// The free list's *set* of ranges — `cut_memmap` re-adds split
    /// prefix/suffix pieces in cut order, not address order, and region
    /// order is not part of the classification contract.
    fn sorted_free(c: &Classification) -> Vec<(u64, u64)> {
        let mut v = free_ranges(c);
        v.sort();
        v
    }

    /// Asserts the A2 invariant on the classification output:
    /// LiveBootstrap(record) ∩ VM-free = ∅.
    fn assert_disjoint(c: &Classification) {
        for &(fb, fe) in &free_ranges(c) {
            for &(db, de) in &deducted_ranges(c) {
                assert!(
                    fe <= db || de <= fb,
                    "free [{fb:#x},{fe:#x}) overlaps deducted [{db:#x},{de:#x})"
                );
            }
        }
    }

    /// Kernel image, root, bump, used frames and a reserved module are all
    /// cut out of one contiguous span; the record contains every source and
    /// the survivors are exactly the complement (interior cuts → 5 spans).
    #[test]
    fn test_classification_deducts_all_record_sources() {
        let memmap = [mem_region(0x10_0000, 0x3F0_0000)]; // [1M, 64M)
        let root = PhysBytes(0x20_0000);
        let bump = (0x40_0000u64, 0x50_0000u64);
        let used = [(0x60_0000u64, 0x80_0000u64)];
        // Kernel image + one reserved module blob (tuples are
        // `(start, len)` — same semantics as `BootModule`).
        let reserved = [(0x2_0000u64, 0xE_0000u64), (0x30_0000u64, 0x8_0000u64)];

        let c = classify_raw(&memmap, root, Some(bump), &used, &reserved);
        let ded = deducted_ranges(&c);

        // The record contains every source (page-rounded).
        assert!(ded.contains(&(root.0, root.0 + PAGE_SIZE)));
        assert!(ded.contains(&(bump.0, bump.1)));
        assert!(ded.contains(&used[0]));
        assert!(ded.contains(&(reserved[0].0, reserved[0].0 + reserved[0].1)));
        assert!(ded.contains(&(reserved[1].0, reserved[1].0 + reserved[1].1)));
        assert_eq!(c.deducted_count, 5);

        // Survivors: the span minus the record's cuts. The kernel image and
        // root sit before 1M/inside the low span, so the single span splits
        // into: [1M,2M) ∪ [2M+4K,3M) ∪ [3.5M,4M) ∪ [5M,6M) ∪ [8M,64M).
        assert_eq!(
            sorted_free(&c),
            vec![
                (0x10_0000, 0x20_0000),
                (0x20_1000, 0x30_0000),
                (0x38_0000, 0x40_0000),
                (0x50_0000, 0x60_0000),
                (0x80_0000, 0x400_0000),
            ]
        );
        assert_disjoint(&c);
    }

    /// Per-span layout where every source occupies its own memmap span:
    /// the free list is exactly the untouched spans.
    #[test]
    fn test_classification_free_list_is_exact_complement() {
        let memmap = [
            mem_region(0x10_0000, 0x10_0000), // [1M, 2M)  → free
            mem_region(0x20_0000, 0x10_0000), // [2M, 3M)  → kernel image
            mem_region(0x30_0000, 0x10_0000), // [3M, 4M)  → bump region
            mem_region(0x40_0000, 0x10_0000), // [4M, 5M)  → used frames
            mem_region(0x50_0000, 0x10_0000), // [5M, 6M)  → reserved module
            mem_region(0x60_0000, 0x10_0000), // [6M, 7M)  → free
        ];
        let root = PhysBytes(0x1_f0_0000); // outside memmap (UEFI LOADER_DATA case)
        let c = classify_raw(
            &memmap,
            root,
            Some((0x30_0000, 0x40_0000)),
            &[(0x40_0000, 0x50_0000)],
            &[(0x20_0000, 0x10_0000), (0x50_0000, 0x10_0000)],
        );

        assert_eq!(
            sorted_free(&c),
            vec![(0x10_0000, 0x20_0000), (0x60_0000, 0x70_0000)]
        );
        assert_eq!(c.deducted_count, 5, "root + bump + used + kernel + module");
        assert_disjoint(&c);
    }

    /// A region straddling the VM DM window end is clipped, not dropped.
    /// The root page at PA 0 cuts the first frame off the free list.
    #[test]
    fn test_classification_clips_to_dm_window() {
        let memmap = [mem_region(0x0, WIN + 0x20_0000)];
        let c = classify_raw(&memmap, PhysBytes(0), None, &[], &[]);
        assert_eq!(free_ranges(&c), vec![(PAGE_SIZE, WIN)]);
        assert_eq!(c.deducted_count, 1, "root page only");
    }

    /// A region entirely outside the window does not enter the free list.
    #[test]
    fn test_classification_drops_out_of_window_region() {
        let memmap = [mem_region(WIN + 0x10_0000, 0x10_0000)];
        let c = classify_raw(&memmap, PhysBytes(0), None, &[], &[]);
        assert_eq!(c.free_region_count, 0);
    }

    /// Boot-image table contract: 5 kernel tasks + 12 boot modules fill the
    /// C `boot_image` table exactly, and the name helper truncates like
    /// C `strlcpy`.
    #[test]
    fn test_boot_image_table_layout() {
        let name = copy_name("vm");
        assert_eq!(&name[..2], b"vm");
        assert!(name[2..].iter().all(|&b| b == 0));

        assert_eq!(KERNEL_TASKS.len() + BOOT_MODULE_PROC_NRS.len(), NR_BOOT_PROCS);
    }

    // ── fix27: identity-window construction ──

    fn info_with_reserved(reserved: &'static [MemoryRegion]) -> KernelInfo {
        // build_identity_windows reads only `reserved_regions`; the rest
        // is inert filler.
        KernelInfo {
            memmap: &[],
            reserved_regions: reserved,
            kern_virt_base: minix_types::VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200000),
            kern_size: 0x200000,
            free_upper_idx: None,
            user_sp: minix_types::VirBytes(0x7fff_ffff_f000),
            kern_stack_top: minix_types::VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: minix_types::VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
            platform_sources: &[],
            param_buf: &[],
        }
    }

    #[test]
    fn test_identity_windows_clip_sort_merge() {
        static RESERVED: [MemoryRegion; 4] = [
            // below the 4 MiB user-image floor: dropped entirely.
            MemoryRegion { base: PhysBytes(0x1000), len: 0x2000 },
            // deliberately unsorted; overlaps the next entry → merged.
            MemoryRegion { base: PhysBytes(0x1da0_0000), len: 0x3000 },
            MemoryRegion { base: PhysBytes(0x1da0_2000), len: 0x2000 },
            // disjoint, stays its own entry.
            MemoryRegion { base: PhysBytes(0x1db0_0000), len: 0x1000 },
        ];
        let (ident, count) = build_identity_windows(&info_with_reserved(&RESERVED));
        let got = &ident[..count];

        // The two MMIO pages the arch always adds (x86-64 host tests).
        let mmio_n = minix_arch::kernel_identity_mmio_regions().len();
        assert_eq!(count, 2 + mmio_n, "merged reserved entries + arch MMIO");

        // Sorted; below-floor entry gone; the two overlapping entries
        // merged into [0x1da00000, 0x1da04000).
        assert_eq!(got[0].base, 0x1da0_0000);
        assert_eq!(got[0].size, 0x4000);
        assert_eq!(got[1].base, 0x1db0_0000);
        assert_eq!(got[1].size, 0x1000);
        if mmio_n > 0 {
            assert_eq!(got[2].base, 0x0FEC0_0000);
            assert_eq!(got[3].base, 0x0FEE0_0000);
        }
        // Invariant map_kernel relies on: sorted and non-overlapping.
        for w in got {
            assert_eq!(w.base % PAGE_SIZE, 0);
            assert_eq!(w.size % PAGE_SIZE, 0);
            assert!(w.base >= 0x40_0000);
        }
        for pair in got.windows(2) {
            assert!(pair[0].base + pair[0].size <= pair[1].base);
        }
    }

    #[test]
    fn test_identity_windows_empty_reserved_still_has_mmio() {
        // OpenSBI/host shape: reserved = ∅ → only the arch MMIO pages.
        let (ident, count) = build_identity_windows(&info_with_reserved(&[]));
        assert_eq!(count, minix_arch::kernel_identity_mmio_regions().len());
        for (i, &(base, _)) in minix_arch::kernel_identity_mmio_regions().iter().enumerate() {
            assert_eq!(ident[i].base, base);
        }
    }
}
