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
//! | kernel image | `KernelInfo.kern_phys_base/kern_size` |
//! | VM self root page | `arch_boot` parameter (= handoff `root_paddr`) |
//! | boot bump region | `boot_alloc::boot_alloc_region()` (whole range —
//!   consumed pages and remainder are both kernel-reserved) |
//! | bootstrap-used frames | per-pool-region used suffixes of
//!   `VmBootAllocator` (walk order: descending from each region's end) |
//! | reserved module blobs (deduction record) | `KernelInfo.boot_modules`
//!   — **every** blob including VM's own (VM's segment frames come from this
//!   free list before VM starts, so reclaiming its blob corrupts VM before
//!   entry) |
//!
//! Note the two module lists are different on purpose: the deduction record
//! keeps every blob, while the handoff module list (`VmBootHandoff.modules`,
//! built by [`build_vm_handoff`]) excludes VM — C: `kinfo.module_list[]`.
//!
//! The free list is then clipped to VM's Direct Map window
//! (`VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap`).
//!
//! All storage is fixed-size — boot is zero-heap.

use crate::boot_alloc::{boot_alloc_region, boot_alloc_used_bytes};
use crate::globals::SyncUnsafeCell;

use crate::proc::{BOOT_MODULE_PROC_NRS, KERNEL_TASKS};
use core::sync::atomic::{AtomicUsize, Ordering};
use minix_arch::DirectMapArch;
use minix_arch::arch::frame::VmBootAllocator;
use minix_boot::{KernelInfo, MemoryRegion};
use minix_types::{
    BootImage, Endpoint, HandoffMemRegion, HandoffModule, NR_BOOT_PROCS, PhysBytes,
    VM_BOOT_HANDOFF_MAGIC, VM_BOOT_HANDOFF_MAX_DEDUCTED,
    VM_BOOT_HANDOFF_MAX_MODULES, VM_BOOT_HANDOFF_MAX_REGIONS, VM_BOOT_HANDOFF_VERSION, PhysRange,
    VmBootHandoff,
};

/// The kernel image's text-segment end (link VMA), provided by
/// `PROVIDE(__text_end = .;)` in all three `kernel-image/*.ld` scripts.
///
/// Kernel-text W^X boundary source: the executable region is the image's
/// contiguous prefix `[kern_virt_base, __text_end)` (`.text` is the first
/// output section and nothing after it is executable — see
/// c-board-rulings-brief.md §11.1), so one symbol expresses the whole
/// boundary. The symbol is a naked address; it is never dereferenced.
/// Compile-gated to the `kernel-text-rx` builds — only those link against
/// the kernel-image scripts that define the symbol.
#[cfg(feature = "kernel-text-rx")]
unsafe extern "C" {
    static __text_end: u8;
}

/// Text page count for the VM per-process kernel mapping: the image prefix
/// up to `__text_end`, rounded UP to 4 KiB pages (the tail of the last text
/// page is section-alignment padding; `.rodata` starts on its own 4 KiB
/// boundary so the RX region never covers real data bytes).
///
/// The symbol path is compiled only with the `kernel-text-rx` feature
/// (forwarded by kernel-image's `fw-none-image`, so production images
/// exclusively enable it). Consumers linked without the kernel-image
/// scripts (test carriers, boot-shim) fall back to the "whole span as
/// text" placeholder; a placeholder handoff reports `kern_data_pages == 0`
/// and `map_kernel` keeps the legacy RWX semantics — behavior unchanged.
#[cfg(feature = "kernel-text-rx")]
pub(crate) fn kern_text_pages(kernel_info: &KernelInfo) -> u64 {
    let text_end_vma = core::ptr::addr_of!(__text_end) as usize as u64;
    let text_bytes = text_end_vma
        .checked_sub(kernel_info.kern_virt_base.0)
        .expect("__text_end below kern_virt_base — link layout drift");
    text_bytes.div_ceil(4096)
}

/// Placeholder: the whole image span as "text" (data pages = 0) — the
/// pre-W^X semantics, kept for consumers whose link scripts carry no
/// `__text_end` symbol.
#[cfg(not(feature = "kernel-text-rx"))]
pub(crate) fn kern_text_pages(kernel_info: &KernelInfo) -> u64 {
    kernel_info.kern_size / 4096
}

/// Frame size of every `VmBootAllocator` page (C: I386_PAGE_SIZE).
pub(crate) const PAGE_SIZE: u64 = 0x1000;

/// The A2 free list (physical memory still free after the first
/// boot-image load), published by [`build_vm_handoff`] at the
/// classification point.
///
/// Consumers that allocate frames AFTER the first boot-image load —
/// multi-image boot loaders (a boot loader's second image) — must
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
/// `globals::KERNEL_INFO` (`HandoffMemRegion` is in the approved
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
    let mut out = [MemoryRegion {
        base: PhysBytes(0),
        len: 0,
    }; VM_BOOT_HANDOFF_MAX_REGIONS];
    for (i, region) in regions.iter().enumerate().take(count) {
        out[i] = MemoryRegion {
            base: PhysBytes(region.base),
            len: region.size as usize,
        };
    }
    (out, count)
}

/// A2 classification output — the free list and the deduction record.
///
/// The record is what the kernel cut from the memmap; the free list is
/// what survived (clipped to VM's Direct Map window). The two are disjoint
/// by construction (`FreeRangeSet::cut` on the recorded ranges).
pub struct Classification {
    /// Surviving free ranges, page-aligned, DM-window-clipped.
    pub free_regions: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_REGIONS],
    pub free_region_count: usize,
    /// `LiveBootstrap(t_classify)` — the ranges cut from the memmap.
    pub deducted: [HandoffMemRegion; VM_BOOT_HANDOFF_MAX_DEDUCTED],
    pub deducted_count: usize,
}

const _: () = assert!(
    VM_BOOT_HANDOFF_MAX_DEDUCTED
        >= 3 + minix_arch::arch::frame::MAX_BOOT_REGIONS + crate::proc::NR_BOOT_MODULES,
    "deduction capacity must cover kernel+root+bump, every pool region and every module"
);

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
    /// is rounded up to the page the way [`FreeRangeSet::cut`] rounds its cuts,
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
/// `vm_module_idx` is the boot-module index of VM itself. Its blob is **not**
/// withheld from the deduction record: every module — VM's own included — is
/// kernel-reserved until VM starts (see the module loop in [`classify`]). The
/// VM-exclusion rule applies only to the handoff module list built by
/// [`build_vm_handoff`] (C: `kinfo.module_list[]`).
///
/// The UEFI boot path allocates the root page and bump region as
/// `LOADER_DATA`, which never appears in the conventional memmap — cutting
/// them is a no-op there. The no-shim fallback path registers its bump
/// region *from* the memmap, and the cut is what keeps it out of VM's free
/// list. Either way the whole registered range is deducted (kernel-reserved
/// includes the bump remainder, §6.0-A2 composition table).
/// Sorted, disjoint, page-aligned set of free physical ranges (D01-F08).
///
/// Replaces the C-shaped ledger (`pg_utils.c:86-125` → the deleted
/// `memmap.rs`: fixed array + zero-length holes + high watermark +
/// first-empty-slot insert). The only production consumer was
/// [`classify`]'s one-shot local deduction, so the container artifacts
/// (holes every consumer had to skip, refill-into-earliest-hole) carried
/// no semantic weight. Behavior contract kept from the old ledger and
/// locked by the tests below: cuts round the removed interval outward to
/// page boundaries (start down, end up), zero-length cuts are no-ops,
/// and capacity exhaustion is a fail-closed panic.
pub(crate) struct FreeRangeSet {
    /// Sorted by `base`, pairwise disjoint, page-aligned half-open ranges.
    ranges: [MemRange; FREE_SET_CAP],
    len: usize,
}

#[derive(Clone, Copy)]
struct MemRange {
    base: u64,
    end: u64,
}

/// C `MAXMEMMAP` (param.h:13, 40) raised to 128: UEFI firmware maps
/// routinely exceed 40 entries and truncating them would silently drop
/// RAM. Capacity divergence from C is input-shaped, not semantic.
pub(crate) const FREE_SET_CAP: usize = crate::globals::MEMMAP_REGION_STORE_LEN;

impl FreeRangeSet {
    /// Builds the set from a boot report's conventional list. Zero-length
    /// entries are skipped (old ledger's "hole" inputs); adjacent ranges
    /// merge — set-equal to the old array shape, fewer entries.
    pub(crate) fn from_memmap(regions: &[MemoryRegion]) -> Self {
        let mut set = Self {
            ranges: [MemRange { base: 0, end: 0 }; FREE_SET_CAP],
            len: 0,
        };
        for region in regions {
            if region.len == 0 {
                continue;
            }
            let end = region
                .base
                .0
                .checked_add(region.len as u64)
                .expect("vm_handoff: memory region span overflows u64");
            set.insert(region.base.0, end);
        }
        set
    }

    /// Inserts one page-aligned range, keeping the sort and merging with
    /// any overlapping or touching neighbor (append + sort + merge pass —
    /// n ≤ 128 makes the obvious form the right one).
    fn insert(&mut self, base: u64, end: u64) {
        assert!(base < end, "vm_handoff: empty range inserted");
        assert!(
            self.len < FREE_SET_CAP,
            "vm_handoff: free-range set over capacity ({FREE_SET_CAP})"
        );
        self.ranges[self.len] = MemRange { base, end };
        self.len += 1;
        self.sort_and_merge();
    }

    /// Sorts by `base` (the list is near-sorted) and merges overlapping or
    /// touching ranges in place.
    fn sort_and_merge(&mut self) {
        for i in 1..self.len {
            let key = self.ranges[i];
            let mut j = i;
            while j > 0 && self.ranges[j - 1].base > key.base {
                self.ranges[j] = self.ranges[j - 1];
                j -= 1;
            }
            self.ranges[j] = key;
        }
        let mut w = 0;
        for r in 1..self.len {
            if self.ranges[w].end >= self.ranges[r].base {
                self.ranges[w].end = self.ranges[w].end.max(self.ranges[r].end);
            } else {
                w += 1;
                self.ranges[w] = self.ranges[r];
            }
        }
        self.len = w + 1;
    }

    /// Removes `[start, start + size)` from the set, rounding the removed
    /// interval outward to page boundaries (old `cut_memmap` contract:
    /// start down, end up). Zero-length is a no-op.
    pub(crate) fn cut(&mut self, range: PhysRange) {
        if range.is_empty() {
            return;
        }
        let lo = range.start().0 & !(PAGE_SIZE - 1);
        let hi = range.end().0.div_ceil(PAGE_SIZE) * PAGE_SIZE;
        assert!(hi > lo, "vm_handoff: cut range rounds to empty");
        let mut i = 0;
        while i < self.len {
            let r = self.ranges[i];
            if r.end <= lo || r.base >= hi {
                i += 1;
                continue;
            }
            // Overlap: [max(r.base, lo), min(r.end, hi)) is removed.
            let keep_left = r.base < lo;
            let keep_right = r.end > hi;
            match (keep_left, keep_right) {
                (false, false) => {
                    // Whole range consumed.
                    for j in i..self.len - 1 {
                        self.ranges[j] = self.ranges[j + 1];
                    }
                    self.len -= 1;
                    // Do not advance: the shifted tail needs re-examining.
                }
                (true, false) => {
                    self.ranges[i].end = lo;
                    i += 1;
                }
                (false, true) => {
                    self.ranges[i].base = hi;
                    i += 1;
                }
                (true, true) => {
                    // Split: right half becomes a new range after i.
                    let right = MemRange {
                        base: hi,
                        end: r.end,
                    };
                    assert!(
                        self.len < FREE_SET_CAP,
                        "vm_handoff: free-range set over capacity ({FREE_SET_CAP})"
                    );
                    for j in (i + 1..self.len).rev() {
                        self.ranges[j + 1] = self.ranges[j];
                    }
                    self.ranges[i + 1] = right;
                    self.len += 1;
                    self.ranges[i].end = lo;
                    i += 2;
                }
            }
        }
    }

    /// Page-rounded total (forensics/metrics).
    pub(crate) fn total_pages(&self) -> u64 {
        self.ranges[..self.len]
            .iter()
            .map(|r| (r.end - r.base) / PAGE_SIZE)
            .sum()
    }
}

pub fn classify(
    kernel_info: &KernelInfo,
    root_paddr: PhysBytes,
    vm_alloc: &VmBootAllocator,
) -> Classification {
    // The free set starts as the boot report's conventional list (D01-F08:
    // sorted disjoint page-aligned ranges; zero-length entries skipped).
    let mut free = FreeRangeSet::from_memmap(kernel_info.memmap);

    // ── Build the LiveBootstrap(t_classify) record ──
    let mut record = DeductionCollector::new();

    // Bootstrap-used ②③: frames handed out by the VM bootstrap allocator.
    // The allocator walks each pool region from `end` down to `start`, so
    // the consumed part of every touched region is a contiguous suffix:
    // regions before `idx` are fully consumed, region `idx` contributes
    // `[cursor, end)`, later regions nothing.
    let pool = vm_alloc.regions();
    let idx = vm_alloc.current_region_idx();
    let cursor = vm_alloc.cursor().0;
    let mut suffixes = [(0u64, 0u64); minix_arch::arch::frame::MAX_BOOT_REGIONS];
    let mut suffix_n = 0usize;
    for i in 0..pool.len().min(idx + 1) {
        let region = pool[i];
        let used_start = if i < idx {
            region.start.0
        } else {
            cursor.max(region.start.0)
        };
        if used_start < region.end.0 {
            suffixes[suffix_n] = (used_start, region.end.0);
            suffix_n += 1;
        }
    }

    // The remaining members (kernel image, root page, bump region, module
    // blobs) are shared with the pre-allocation exclusion list; see
    // `for_each_live_bootstrap`. `classify` deliberately does not exclude the
    // VM's own module: the deduction record keeps it, and `build_vm_handoff`
    // filters the VM blob out of the module list afterwards (DESIGN-08).
    for_each_live_bootstrap(
        kernel_info.kern_phys_base,
        kernel_info.kern_size,
        kernel_info.boot_modules,
        Some(root_paddr),
        boot_alloc_region(),
        &suffixes[..suffix_n],
        |base, len| record.push(base, len),
    );

    // ── Cut the record out of the memmap (same-family exclusion as
    //    `VmBootRegion::select_multi` — frame.rs invariant 1) ──
    for range in &record.ranges[..record.count] {
        // Every recorded range is page-aligned by construction.
        free.cut(
            PhysRange::new(
                PhysBytes(range.base),
                PhysBytes(range.base + range.size),
            )
            .expect("vm_handoff: boot range end precedes its base"),
        );
    }

    // ── Clip survivors to VM's Direct Map window ──
    // VM PMM eligible = conventional ∩ DM-representable − LiveBootstrap
    // (§6.0-A2): a region the VM DM window cannot represent cannot be
    // accessed through vm_phys_to_virt, so it must not enter the free list.
    let window_end = minix_arch::CurrentDirectMap::VM_DIRECT_MAP_SIZE;
    let mut free_regions = [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_REGIONS];
    let mut free_count = 0usize;
    for r in free.ranges.iter().take(free.len) {
        let clipped_end = r.end.min(window_end);
        if r.base >= clipped_end {
            continue; // entirely outside the window
        }
        assert!(
            free_count < VM_BOOT_HANDOFF_MAX_REGIONS,
            "vm_handoff: free-region list overflow — raise VM_BOOT_HANDOFF_MAX_REGIONS"
        );
        free_regions[free_count] = HandoffMemRegion {
            base: r.base,
            size: clipped_end - r.base,
        };
        free_count += 1;
    }

    // Forensic visibility for the surviving classified regions: VM PMM had only
    // ~133 usable pages (grow(229) PhysicalAllocFailed, 2026-09-21 stack
    // forensics), so it must be known whether the memmap snapshot was already
    // small or the deduction cut too much. Print the first 8 only, to avoid
    // flooding the console. Gated like `boot_stage!`: the host-test form has no
    // console sink, and this is the only thing keeping `classify` itself
    // unreachable from a host test.
    #[cfg(not(feature = "mock"))]
    {
        use crate::kmess::{EarlyConsole as _, KernelConsole as Console};
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
    let classification = classify(kernel_info, root_paddr, vm_alloc);

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
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
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
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        boot_procs[n] = BootImage {
            proc_nr: BOOT_MODULE_PROC_NRS[i].0,
            proc_name: copy_name(module.name),
            endpoint: Endpoint(BOOT_MODULE_PROC_NRS[i].0),
            start_addr: module.start.0,
            len: module.len as u64,
        };
        n += 1;
    }
    debug_assert_eq!(
        n, NR_BOOT_PROCS,
        "kernel tasks + boot modules must fill the boot image table"
    );

    // Handoff v9 removed the always-empty v6 identity-window field. MMIO
    // consumers use the kernel device window; v6/v7/v8 senders are rejected by
    // the exact-version check.

    // Production reaches here after kmain Phase B.5 initialized the
    // kernel-information page. Staged bootstrap test kernels call
    // `arch_boot_impl`/`init_proc_and_boot` directly and skip that phase;
    // they report 0 and VM skips the per-root mapping, preserving the earlier
    // test shape. Production correctness is gated by the
    // rt-birth/user-trap QEMU legs, which read the page at CPL3: a
    // production 0 would fail those gates instead of silently passing.
    let kerninfo_pa = crate::kerninfo::kerninfo_page_phys().unwrap_or(0);

    VmBootHandoff {
        magic: VM_BOOT_HANDOFF_MAGIC,
        version: VM_BOOT_HANDOFF_VERSION,
        root_paddr: root_paddr.0,
        // The kernel owns the page; VM builds the per-root mapping at
        // `KERNINFO_USER_VA` and owns only the intermediate page-table pages.
        kerninfo_pa,
        vm_allocated_bytes: vm_alloc.used_frames() * PAGE_SIZE,
        kernel_allocated_static: kernel_info.kern_size,
        kernel_allocated_dynamic: boot_alloc_used_bytes(),
        // Kernel layout for VM's per-process kernel mappings —
        // C kinfo carries the same role (kernel text/data base + pages).
        //
        // [ARCH: kernel-text-rx] Real text/data boundary. The boundary
        // comes from this image's `__text_end` linker symbol (PROVIDEd at
        // the end of the `.text` output section in all three
        // kernel-image/*.ld scripts; the executable region is the image's
        // contiguous prefix — see
        // rewrite-notes/01-stage-kernel/c-board-rulings-brief.md §11.1).
        // `kern_text_pages` rounds UP to 4 KiB pages: the tail of the last
        // text page is section-alignment padding (mapped RX, no content),
        // and `.rodata` starts on its own 4 KiB boundary so it is never
        // covered. The previous placeholder reported the whole span as
        // text (data_pages = 0), leaving per-process roots fully RWX; the
        // real values let the VM's `map_kernel` map the text segment RX
        // and the data segment RW (W^X within the per-process-root scope).
        kern_virt_base: kernel_info.kern_virt_base.0,
        kern_phys_base: kernel_info.kern_phys_base.0,
        kern_text_pages: u32::try_from(kern_text_pages(kernel_info))
            .expect("vm_handoff: kern_text_pages exceeds u32"),
        kern_data_pages: {
            let total = kernel_info.kern_size / 4096;
            let text = kern_text_pages(kernel_info) as u64;
            let data = total
                .checked_sub(text)
                .expect("vm_handoff: __text_end exceeds kern_size — link layout drift");
            // 0 in `map_kernel` means "the whole image is text" (RWX); do not
            // let an invariant failure silently fall into that sentinel.
            u32::try_from(data).expect("vm_handoff: kern_data_pages exceeds u32")
        },
        // Handoff v5: the real kernel Direct Map window. VM's
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
        boot_procs,
    }
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

/// Enumerate the ranges occupied by the live bootstrap, in one place.
///
/// Two consumers need this set before the VM free list exists: the VM boot
/// exclusion list (first allocation) and `classify` (handoff deduction). They
/// differ only in whether bootstrap-allocator suffixes exist yet, so the
/// caller passes them explicitly; keeping the membership here stops the two
/// copies from drifting when a new bootstrap allocation is added.
pub(crate) fn for_each_live_bootstrap<F>(
    kern_base: PhysBytes,
    kern_len: u64,
    modules: &[minix_boot::BootModule],
    root_paddr: Option<PhysBytes>,
    boot_bump: Option<(u64, u64)>,
    used_suffixes: &[(u64, u64)],
    mut f: F,
) where
    F: FnMut(u64, u64),
{
    f(kern_base.0, kern_len);
    if let Some(root) = root_paddr {
        f(root.0, PAGE_SIZE);
    }
    if let Some((base, end)) = boot_bump {
        f(base, end.saturating_sub(base));
    }
    for &(start, end) in used_suffixes {
        if start < end {
            f(start, end - start);
        }
    }
    for module in modules.iter() {
        f(module.start.0, module.len as u64);
    }
}

/// Classifies a raw memmap slice directly (test entry point — the real
/// boot path snapshots `KernelInfo.memmap` first).
#[cfg(test)]


pub(crate) fn classify_regions(
    memmap: &[MemoryRegion],
    root_paddr: PhysBytes,
    boot_bump: Option<(u64, u64)>,
    used_suffixes: &[(u64, u64)],
    reserved_modules: &[(u64, u64)],
) -> Classification {
    let mut free = FreeRangeSet::from_memmap(memmap);

    let mut record = DeductionCollector::new();
    // Single source for the membership shape: the test entry reuses
    // `for_each_live_bootstrap` (kernel span empty here; the raw test memmap
    // has no kernel image), then adds the test-only reserved-module spans.
    for_each_live_bootstrap(
        PhysBytes(0),
        0,
        &[],
        Some(root_paddr),
        boot_bump,
        used_suffixes,
        |base, len| record.push(base, len),
    );
    for &(start, len) in reserved_modules {
        record.push(start, len);
    }

    for range in &record.ranges[..record.count] {
        free.cut(
            PhysRange::new(
                PhysBytes(range.base),
                PhysBytes(range.base + range.size),
            )
            .expect("vm_handoff: boot range end precedes its base"),
        );
    }

    let window_end = minix_arch::CurrentDirectMap::VM_DIRECT_MAP_SIZE;
    let mut free_regions = [HandoffMemRegion::ZERO; VM_BOOT_HANDOFF_MAX_REGIONS];
    let mut free_count = 0usize;
    for r in free.ranges.iter().take(free.len) {
        let clipped_end = r.end.min(window_end);
        if r.base >= clipped_end {
            continue;
        }
        assert!(free_count < VM_BOOT_HANDOFF_MAX_REGIONS);
        free_regions[free_count] = HandoffMemRegion {
            base: r.base,
            size: clipped_end - r.base,
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
    // Test-only constant: the production face of this file does not
    // reference it, so the import lives here instead of the top-level use
    // list (where `clippy --workspace`, which builds without `--tests`,
    // would flag it unused).
    use alloc::{vec, vec::Vec};
    use minix_types::USER_STACK_TOP;

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

    /// The capacity is derived (`3 + MAX_BOOT_REGIONS + NR_BOOT_MODULES`):
    /// a boot that reaches the bound must still be enumerable.
    #[test]
    fn test_deduction_capacity_accepts_full_bound() {
        let mut c = DeductionCollector::new();
        for i in 0..VM_BOOT_HANDOFF_MAX_DEDUCTED {
            c.push(i as u64 * PAGE_SIZE, PAGE_SIZE);
        }
        assert_eq!(c.count, VM_BOOT_HANDOFF_MAX_DEDUCTED);
    }

    /// One entry past the bound is a fail-stop, not a truncation: a silently
    /// dropped deduction would hand live bootstrap frames to the VM.
    #[should_panic(expected = "deduction record overflow")]
    #[test]
    fn test_deduction_capacity_overflow_is_fail_stop() {
        let mut c = DeductionCollector::new();
        for i in 0..=VM_BOOT_HANDOFF_MAX_DEDUCTED {
            c.push(i as u64 * PAGE_SIZE, PAGE_SIZE);
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

    /// Bootstrap-used frames are recorded **per touched region**, not per
    /// frame: VM's image frames, the page that carries the handoff itself
    /// and every other frame the bootstrap allocator handed out all fall
    /// inside the used suffix of the region they came from. This is what
    /// keeps the deduction record within `3 + MAX_BOOT_REGIONS +
    /// NR_BOOT_MODULES` regardless of how much memory the bootstrap used, so
    /// two allocations of different sizes must yield the same record count
    /// while every delivered frame is still covered by the record.
    #[test]
    #[cfg(feature = "mock")]
    fn test_bootstrap_frames_are_recorded_per_region_not_per_frame() {
        let _boot = crate::test_sync::lock_boot_globals();
        use crate::vm_boot_exclusions;
        use minix_arch::arch::frame::{MAX_BOOT_REGIONS, VmBootRegion};
        use minix_boot::BootModule;
        use minix_types::VirBytes;

        static MEMMAP: &[MemoryRegion] = &[MemoryRegion {
            base: PhysBytes(0x10_0000),
            len: 0x80_0000,
        }]; // [1 MiB, 9 MiB)
        static MODS: &[BootModule] = &[
            BootModule {
                name: "vm",
                start: PhysBytes(0x40_0000),
                len: 0x8_0000,
            },
            BootModule {
                name: "rs",
                start: PhysBytes(0x50_0000),
                len: 0x4_0000,
            },
        ];
        let info = KernelInfo {
            memmap: MEMMAP,
            reserved_regions: &[],
            memmap_sources: &[],
            startup_objects: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x20_0000),
            kern_size: 0x10_0000,
            user_sp: VirBytes(USER_STACK_TOP as u64),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: MODS,
            platform_sources: &[],
            param_buf: &[],
            authority: minix_boot::InputObjectId::DtbBootInput,
        };
        let root = PhysBytes(0x80_0000);

        let run = |frames: usize| -> (Vec<(u64, u64)>, Classification) {
            let (excl, n) = vm_boot_exclusions(
                info.kern_phys_base,
                info.kern_size as usize,
                info.boot_modules,
                Some(root),
                boot_alloc_region(),
            );
            let regions = VmBootRegion::select_multi::<MAX_BOOT_REGIONS>(info.memmap, &excl[..n])
                .expect("select_multi")
                .expect("a bootstrap region survives the exclusions");
            let mut alloc = VmBootAllocator::<MAX_BOOT_REGIONS>::new(regions);
            let taken = (0..frames)
                .map(|_| {
                    let f = alloc.alloc_page().expect("frame");
                    (f.start().0, f.start().0 + PAGE_SIZE)
                })
                .collect();
            (taken, classify(&info, root, &alloc))
        };

        let (few, c_few) = run(2);
        let (many, c_many) = run(8);

        // Same region touched, same record size: the count does not track
        // the number of frames used.
        assert_eq!(
            c_few.deducted_count, c_many.deducted_count,
            "deduction record grew with the number of bootstrap frames used"
        );

        // Every frame the allocator handed out is inside the record — the
        // handoff page included, since it is taken from this same allocator
        // before the classification point.
        let ded = deducted_ranges(&c_many);
        for &(start, end) in few.iter().chain(many.iter()) {
            assert!(
                ded.iter().any(|&(d, e)| d <= start && end <= e),
                "bootstrap frame [{start:#x},{end:#x}) is not covered by any deduction record"
            );
        }

        assert_disjoint(&c_few);
        assert_disjoint(&c_many);
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

    /// D-5 lifecycle proof: the receiver's last read of the v1 arena is the
    /// decode. The test serializes a real arena, decodes the map into
    /// caller-owned stores, poisons the arena, and then runs the same
    /// classification the VM handoff uses. The free list must still be the
    /// decoded map (nothing re-reads the arena), and the arena's own frames —
    /// which sit inside the conventional region — must be reissued as free,
    /// because the decode was the last read.
    #[test]
    fn decoded_map_survives_arena_poison_and_reissues_the_arena_frames() {
        use minix_boot::{
            ARENA_CAP, InputObjectId, RuntimeFields, Serializer, WIRE_MEMMAP_CAP,
            WIRE_RESERVED_CAP, decode_envelope, decode_memmap, decode_reserved_regions,
        };
        use minix_types::VirBytes;

        const ARENA_PA: u64 = 0x40_0000; // 4 MiB, inside the conventional region
        const KERNEL_PA: u64 = 0x20_0000;
        const KERNEL_LEN: u64 = 0x10_0000;
        const ROOT_PA: u64 = 0x80_0000;

        let mut arena = [0u8; ARENA_CAP];
        let mut s = Serializer::new(&mut arena, InputObjectId::UefiMemoryMap).unwrap();
        let carrier = KernelInfo {
            memmap: &[],
            reserved_regions: &[],
            memmap_sources: &[],
            startup_objects: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(KERNEL_PA),
            kern_size: KERNEL_LEN,
            user_sp: VirBytes(USER_STACK_TOP as u64),
            kern_stack_top: VirBytes(0xFFFF_8000_0010_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0000_0000),
            boot_modules: &[],
            platform_sources: &[],
            param_buf: &[],
            authority: minix_boot::InputObjectId::DtbBootInput,
        };
        s.write_envelope(&carrier).unwrap();
        s.push_memmap(mem_region(0x10_0000, 0x80_0000)).unwrap(); // [1 MiB, 9 MiB)
        s.finish_memmap();
        s.write_reserved_regions(&[mem_region(0x100_0000, 0x1000)])
            .unwrap();
        s.patch_runtime_fields(RuntimeFields {
            root_page: ROOT_PA,
            bump_base: 0x90_0000,
            bump_next: 0x90_0000,
            bump_end: 0x92_0000,
        });

        let scalars = decode_envelope(s.as_bytes()).unwrap();
        let mut free = [mem_region(0, 0); WIRE_MEMMAP_CAP];
        let n_free = decode_memmap(s.as_bytes(), &scalars, &mut free).unwrap();
        let mut reserved = [mem_region(0, 0); WIRE_RESERVED_CAP];
        let n_reserved = decode_reserved_regions(s.as_bytes(), &scalars, &mut reserved).unwrap();
        assert_eq!((n_free, n_reserved), (1, 1));

        // The receiver's `KernelInfo` slices point into its own `.bss` stores;
        // leaking the decoded arrays mirrors that ownership here.
        let memmap: &'static [MemoryRegion] =
            alloc::boxed::Box::leak(free[..n_free].to_vec().into_boxed_slice());
        let reserved_regions: &'static [MemoryRegion] =
            alloc::boxed::Box::leak(reserved[..n_reserved].to_vec().into_boxed_slice());
        let info = KernelInfo {
            memmap,
            reserved_regions,
            memmap_sources: &[],
            startup_objects: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(KERNEL_PA),
            kern_size: KERNEL_LEN,
            user_sp: VirBytes(USER_STACK_TOP as u64),
            kern_stack_top: VirBytes(0xFFFF_8000_0010_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0000_0000),
            boot_modules: &[],
            platform_sources: &[],
            param_buf: &[],
            authority: minix_boot::InputObjectId::DtbBootInput,
        };

        // Last read is done: poison the arena.
        arena.fill(0xA5);

        // VM-facing classification: kernel suffix + root deducted, bump
        // outside the memmap. The arena frames are *not* deducted — that is
        // exactly the D-5 lifecycle claim (decode was the last read).
        let c = classify_raw(
            info.memmap,
            PhysBytes(ROOT_PA),
            Some((0x90_0000, 0x92_0000)),
            &[(KERNEL_PA, KERNEL_PA + KERNEL_LEN)],
            &[],
        );
        assert_eq!(
            sorted_free(&c),
            vec![
                (0x10_0000, 0x20_0000),
                (0x30_0000, 0x80_0000),
                (0x80_1000, 0x90_0000),
            ]
        );
        assert!(
            free_ranges(&c)
                .iter()
                .any(|&(base, end)| base <= ARENA_PA && ARENA_PA + 0x2000 <= end),
            "the poisoned arena's frames must be reissued to VM"
        );
        assert!(
            !free_ranges(&c)
                .iter()
                .any(|&(base, end)| base <= KERNEL_PA && KERNEL_PA + KERNEL_LEN <= end),
            "the kernel image must stay deducted"
        );
        assert_eq!(
            (info.memmap[0].base.0, info.memmap[0].len),
            (0x10_0000, 0x80_0000),
            "the decoded map must not alias the poisoned arena"
        );
        assert_eq!(
            (
                info.reserved_regions[0].base.0,
                info.reserved_regions[0].len
            ),
            (0x100_0000, 0x1000)
        );
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

        assert_eq!(
            KERNEL_TASKS.len() + BOOT_MODULE_PROC_NRS.len(),
            NR_BOOT_PROCS
        );
    }

    // ── identity-window construction ──

    #[test]
    fn test_arch_mmio_identity_regions_are_page_aligned_and_sorted() {
        // The arch table is not replayed into process roots (the identity
        // replay is retired); this test only checks the table's own
        // invariants.
        let mut previous_end = 0u64;
        for &(base, size) in minix_arch::kernel_identity_mmio_regions() {
            assert_eq!(base % PAGE_SIZE, 0, "MMIO identity base not page-aligned");
            assert_eq!(size % PAGE_SIZE, 0, "MMIO identity size not page-aligned");
            assert!(
                base >= previous_end,
                "MMIO identity regions overlap or are unsorted"
            );
            previous_end = base + size;
        }
    }

    // ── FreeRangeSet behavior lock (D01-F08) ────────────────────────────
    // The old C-shaped ledger's observable contract, restated on the set:
    // cuts round outward to page boundaries, zero-length is a no-op, the
    // free output is a sorted disjoint page-aligned list.

    fn rng(start: u64, size: u64) -> PhysRange {
        PhysRange::new(PhysBytes(start), PhysBytes(start + size)).expect("test range")
    }

    fn set(regions: &[(u64, u64)]) -> FreeRangeSet {
        let owned: Vec<MemoryRegion> = regions
            .iter()
            .map(|&(b, l)| MemoryRegion {
                base: PhysBytes(b),
                len: l as usize,
            })
            .collect();
        FreeRangeSet::from_memmap(&owned)
    }

    fn ranges_of(s: &FreeRangeSet) -> Vec<(u64, u64)> {
        s.ranges[..s.len].iter().map(|r| (r.base, r.end)).collect()
    }

    /// Reference model: page-rounded interval subtraction on a plain list.
    /// The set must be page-equal to this on every input below.
    fn reference_subtract(regions: &[(u64, u64)], cuts: &[(u64, u64)]) -> Vec<(u64, u64)> {
        let page = PAGE_SIZE;
        let lo_cut = |(b, l): (u64, u64)| (b & !(page - 1), (b + l).div_ceil(page) * page);
        let mut pieces: Vec<(u64, u64)> = regions.iter().map(|&(b, l)| (b, b + l)).collect();
        for &c in cuts {
            let (lo, hi) = lo_cut(c);
            let mut next = Vec::new();
            for (b, e) in pieces {
                if e <= lo || b >= hi {
                    next.push((b, e));
                } else {
                    if b < lo {
                        next.push((b, lo));
                    }
                    if e > hi {
                        next.push((hi, e));
                    }
                }
            }
            pieces = next;
        }
        pieces.sort();
        pieces
    }

    fn assert_page_equal(regions: &[(u64, u64)], cuts: &[(u64, u64)]) {
        let mut s = set(regions);
        for &(b, l) in cuts {
            s.cut(rng(b, l));
        }
        let got = ranges_of(&s);
        let want = reference_subtract(regions, cuts);
        assert_eq!(
            got, want,
            "set diverged from reference: regions={regions:?} cuts={cuts:?}"
        );
    }

    #[test]
    fn free_set_zero_length_inputs_are_skipped() {
        let s = set(&[(0x1000, 0x2000), (0x0, 0x0), (0x8000, 0x1000)]);
        assert_eq!(ranges_of(&s), vec![(0x1000, 0x3000), (0x8000, 0x9000)]);
    }

    #[test]
    fn free_set_adjacent_inputs_merge() {
        let s = set(&[(0x1000, 0x1000), (0x2000, 0x1000)]);
        assert_eq!(ranges_of(&s), vec![(0x1000, 0x3000)]);
    }

    #[test]
    #[should_panic(expected = "memory region span overflows")]
    fn free_set_from_memmap_rejects_span_overflow() {
        let _ = FreeRangeSet::from_memmap(&[MemoryRegion {
            base: PhysBytes(u64::MAX - 1),
            len: 4,
        }]);
    }

    #[test]
    fn phys_range_rejects_inverted_or_wrapping_bounds() {
        assert!(PhysRange::new(PhysBytes(0x2000), PhysBytes(0x1000)).is_none());
        assert!(PhysRange::new(PhysBytes(u64::MAX - 1), PhysBytes(4)).is_none());
    }

    #[test]
    fn free_set_cut_rounds_outward() {
        // Old contract: cut start rounds down, end rounds up.
        let mut s = set(&[(0x1000, 0x4000)]);
        s.cut(rng(0x1800, 0x1000)); // removes [0x1000, 0x3000) after rounding
        assert_eq!(ranges_of(&s), vec![(0x3000, 0x5000)]);
    }

    #[test]
    fn free_set_cut_splits_middle() {
        let mut s = set(&[(0x1000, 0x8000)]);
        s.cut(rng(0x3000, 0x1000));
        assert_eq!(ranges_of(&s), vec![(0x1000, 0x3000), (0x4000, 0x9000)]);
    }

    #[test]
    fn free_set_zero_length_cut_is_noop() {
        let mut s = set(&[(0x1000, 0x2000)]);
        s.cut(rng(0x1000, 0));
        assert_eq!(ranges_of(&s), vec![(0x1000, 0x3000)]);
    }

    #[test]
    fn free_set_disjoint_cut_is_noop() {
        let mut s = set(&[(0x5000, 0x1000)]);
        s.cut(rng(0x1000, 0x1000));
        assert_eq!(ranges_of(&s), vec![(0x5000, 0x6000)]);
    }

    #[test]
    fn free_set_unaligned_input_region_keeps_shape_until_cut() {
        // The old ledger stored unaligned input as-is (consumers clipped);
        // the set keeps the same rule — rounding happens at cut time.
        let s = set(&[(0x1800, 0x800)]);
        assert_eq!(ranges_of(&s), vec![(0x1800, 0x2000)]);
    }

    /// Page-equivalence battery (D01-F08 acceptance): the set is page-equal
    /// to reference interval subtraction on mixed overlapping cuts, the
    /// shape that the old add/cut tests pinned.
    #[test]
    fn free_set_page_equivalent_to_reference_subtraction() {
        assert_page_equal(
            &[(0x100000, 0x1000000), (0x2000000, 0x800000)],
            &[
                (0x200000, 0x200000),  // kernel image
                (0x100000, 0x1000),    // root page
                (0x300000, 0x400000),  // bump
                (0x180000, 0x123000),  // unaligned mid cut
                (0x2500000, 0x40000),  // module over boundary
                (0x2800000, 0x100000), // full-removal cut
                (0x2900000, 0x40000),  // already-removed region
            ],
        );
        assert_page_equal(
            &[(0x1000, 0x3000), (0x8000, 0x1000)],
            &[(0x2000, 0x2000)], // cut spans two input regions
        );
        assert_page_equal(&[(0x1000, 0x1000)], &[(0x0, 0x10000)]); // swallow all
        assert_page_equal(&[(0x1000, 0x2000)], &[]); // no cuts
    }

    #[test]
    #[should_panic(expected = "free-range set over capacity")]
    fn free_set_capacity_fails_closed() {
        let mut s = set(&[]);
        // 128 disjoint inserts fill the cap; the 129th must panic.
        for i in 0..FREE_SET_CAP {
            s.insert(
                (i as u64 + 1) * PAGE_SIZE * 2,
                (i as u64 + 1) * PAGE_SIZE * 2 + PAGE_SIZE,
            );
        }
        s.insert(0, PAGE_SIZE);
    }
}
