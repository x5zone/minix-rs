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
use crate::memmap::{cut_memmap, MemMapEntry, MAXMEMMAP};
use crate::proc::{BOOT_MODULE_PROC_NRS, KERNEL_TASKS};
use minix_arch::arch::frame::VmBootAllocator;
use minix_arch::DirectMapArch;
use minix_boot::{KernelInfo, MemoryRegion};
use minix_types::{
    BootImage, Endpoint, HandoffMemRegion, HandoffModule, NR_BOOT_PROCS,
    PhysBytes, VM_BOOT_HANDOFF_MAGIC, VM_BOOT_HANDOFF_MAX_DEDUCTED,
    VM_BOOT_HANDOFF_MAX_MODULES, VM_BOOT_HANDOFF_MAX_REGIONS,
    VM_BOOT_HANDOFF_VERSION, VmBootHandoff,
};

/// Frame size of every `VmBootAllocator` page (C: I386_PAGE_SIZE).
const PAGE_SIZE: u64 = 0x1000;

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

    // Kernel-reserved: every boot module blob except VM's own (reclaimed).
    assert!(
        vm_module_idx < kernel_info.boot_modules().len(),
        "vm_handoff: VM module index {} out of bounds",
        vm_module_idx
    );
    for (i, module) in kernel_info.boot_modules().iter().enumerate() {
        if i != vm_module_idx {
            record.push(module.start.0, module.len as u64);
        }
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

    VmBootHandoff {
        magic: VM_BOOT_HANDOFF_MAGIC,
        version: VM_BOOT_HANDOFF_VERSION,
        root_paddr: root_paddr.0,
        vm_allocated_bytes: vm_alloc.used_frames() * PAGE_SIZE,
        kernel_allocated_static: kernel_info.kern_size(),
        kernel_allocated_dynamic: boot_alloc_used_bytes(),
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
}
