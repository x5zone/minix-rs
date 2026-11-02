//! UEFI-specific boot helpers.
//!
//! Implements the `BootShim` trait for UEFI firmware. Firmware-specific
//! concerns live here; the actual kernel/module loading code is generic
//! and lives in [`crate::loader`].
//!
//! Responsibilities of this module:
//! - Wrap UEFI BootServices (memory map, page allocation, exit).
//! - Provide a `FileLoader` impl backed by `SimpleFileSystem`.
//! - Wire the firmware-specific pieces into the shared loader.

use alloc::boxed::Box;
use alloc::vec::Vec;
use uefi::boot::{self, AllocateType};
use uefi::fs::FileSystem;
use uefi::mem::memory_map::{MemoryMap, MemoryType};
use uefi::system;
use uefi::table::cfg::{ACPI_GUID, ACPI2_GUID};
use uefi::Guid;
use minix_types::PhysBytes;
use minix_arch::boot_dm_admissible_end;
use minix_boot::{
    BootPrepareResult, BootShim, DTB, MemoryRegion, PlatformDescSource, RSDP,
};

use crate::build_kernel_info;
use crate::loader::{self, FileLoader};

#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
use core::ffi::c_void;
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
use minix_boot::{
    decode_descriptors, Serializer, ARENA_CAP, SEMANTIC_MEMMAP_OFFSET, SEMANTIC_RESERVED_OFFSET,
};
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
use crate::ebs_pair::{self, PairError, RawOps};

/// UEFI Configuration Table GUID for the Device Tree (FDT).
///
/// Defined in the UEFI Specification (2.4+) as `EFI_DEVICE_TREE_GUID`.
/// Not provided by the `uefi` crate, so we define it here.
const DEVICE_TREE_GUID: Guid = uefi::guid!("b1b621d5-f19c-41a5-830b-d9152c69aae0");

/// Legacy ARM FDT configuration-table GUID used by pre-2.4 firmware.
///
/// Kept as a second accepted GUID so older UEFI builds that published this
/// value still reach the DTB parser. It is not the UEFI-spec value.
const LEGACY_DEVICE_TREE_GUID: Guid = uefi::guid!("b1b621d2-f19c-41c5-8310-daa6f018a8d3");

/// Global allocator for UEFI test kernels that define none of their own.
///
/// While boot services are alive every allocation delegates to the UEFI
/// pool allocator (`LOADER_DATA`); after `exit_boot_services()` it returns
/// null — test kernels allocate nothing past that point (their post-exit
/// bookkeeping is bump-allocator based, not heap based).
///
/// The `#[global_allocator]` registration deliberately stays in each test
/// kernel (2 lines) instead of living here: a global allocator compiled
/// into boot-shim would reach every boot-shim consumer through feature
/// unification and conflict with kernels that define their own allocator
/// (e.g. `test-proc-init`'s hybrid UEFI+bump allocator).
pub struct UefiPoolAllocator;

unsafe impl core::alloc::GlobalAlloc for UefiPoolAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        // UEFI guarantees 8-byte alignment minimum; for larger alignments,
        // over-allocate and align within the block.
        let alloc_size = if align > 8 { size + align } else { size };
        match boot::allocate_pool(MemoryType::LOADER_DATA, alloc_size) {
            Ok(ptr) => {
                let addr = ptr.as_ptr() as usize;
                if align > 8 {
                    ((addr + align - 1) & !(align - 1)) as *mut u8
                } else {
                    addr as *mut u8
                }
            }
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {
        // Pool pages are reclaimed wholesale by the firmware at
        // ExitBootServices; boot-stage code never frees individually.
    }
}

/// Scan the UEFI Configuration Table for platform descriptor sources
/// (ACPI RSDP on x86-64, DTB on ARM64).
///
/// Returns a `'static` slice of `PlatformDescSource` entries, ordered by
/// boot-shim's preference. The kernel takes the first source that parses
/// successfully. An empty slice means no source was found — the kernel
/// falls back to `QemuVirtDesc` (dev) or panics (release).
///
/// # Ordering (DTB + RSDP coexistence)
///
/// Real-world ARM64 servers (SBBR) may provide both DTB and ACPI tables.
/// On ARM64 we prefer DTB first (QEMU virt provides DTB), then ACPI as
/// fallback. On x86-64 only ACPI is used.
///
/// Must be called **before** `exit_boot_services()`, because the UEFI
/// System Table (which holds the configuration table pointer) is only
/// valid while boot services are available.
pub fn find_platform_sources() -> &'static [PlatformDescSource] {
    use core::cell::RefCell;
    let sources = RefCell::new(Vec::<PlatformDescSource>::new());

    system::with_config_table(|entries| {
        // On x86-64, look for ACPI RSDP (prefer ACPI 2.0+).
        #[cfg(target_arch = "x86_64")]
        {
            for e in entries {
                if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
                    sources.borrow_mut().push(PlatformDescSource::new(RSDP, PhysBytes(e.address as u64)));
                    break;
                }
            }
        }

        // On aarch64, prefer DTB first (QEMU virt provides DTB), then ACPI.
        #[cfg(target_arch = "aarch64")]
        {
            for e in entries {
                if e.guid == DEVICE_TREE_GUID || e.guid == LEGACY_DEVICE_TREE_GUID {
                    sources.borrow_mut().push(PlatformDescSource::new(DTB, PhysBytes(e.address as u64)));
                    break;
                }
            }
            // Fall back to ACPI if DTB not present.
            for e in entries {
                if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
                    sources.borrow_mut().push(PlatformDescSource::new(RSDP, PhysBytes(e.address as u64)));
                    break;
                }
            }
        }

        // On other architectures (e.g., riscv64 with UEFI — currently
        // unsupported as a Rust target), return empty and let the kernel
        // use the QEMU fallback.
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            let _ = entries;
        }
    });

    // Leak the Vec so it has 'static lifetime — acceptable for boot-stage
    // code that runs exactly once (same pattern as build_memmap).
    Box::leak(sources.into_inner().into_boxed_slice())
}

/// Collect this image's `EFI_LOADED_IMAGE_PROTOCOL.LoadOptions` and normalize
/// it into the project's `key=value\0` protocol.
///
/// The raw byte buffer is what the firmware actually passed to this image.
/// Our contract is ASCII records separated by NUL or whitespace; arbitrary
/// payloads (UCS-2 command lines, binary blobs) are rejected record by record
/// by the normalizer instead of guessed. The returned slice lives in a leaked
/// `LOADER_DATA` allocation, so it survives until the matching builder call.
///
/// Must run before `ExitBootServices` (it opens a protocol).
fn collect_param_buf() -> (&'static [u8], minix_boot::ParamStats) {
    let landing: &'static mut [u8] =
        Box::leak(alloc::vec![0u8; minix_boot::PARAM_BUF_CAP].into_boxed_slice());
    let input: Vec<u8> = boot::open_protocol_exclusive::<
        uefi::proto::loaded_image::LoadedImage,
    >(boot::image_handle())
    .ok()
    .and_then(|image| image.load_options_as_bytes().map(|bytes| bytes.to_vec()))
    .unwrap_or_default();
    let stats = minix_boot::normalize_param_buf(&input, None, landing);
    let ptr = landing.as_ptr();
    // SAFETY: `landing` is a leaked `'static` allocation; the slice covers the
    // normalized prefix only.
    let out = unsafe { core::slice::from_raw_parts(ptr, stats.bytes) };
    (out, stats)
}

/// UEFI implementation of `BootShim`.
///
/// Uses UEFI BootServices to discover memory, allocate pages, load kernel
/// and boot modules from the ESP, then exits boot services before handing
/// control to the kernel.
pub struct UefiBootShim;

impl BootShim for UefiBootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
        // Stage prints (first-light diagnostics): the shim's panic handler is a
        // silent infinite loop, so without these lines any stuck step makes the
        // whole machine appear silent. ConOut is confirmed to land on the QEMU
        // serial port (the firmware countdown text is visible).
        uefi::println!("boot-shim: prepare_boot enter");
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);
        uefi::println!("boot-shim: root+bump allocated");
        // Ordering correction: the snapshot must come after **all** LOADER_DATA
        // allocations — besides the root page and the bump region, that includes
        // the allocate_pages for the kernel segments and the 12 boot modules
        // (second-layer lesson: the original correction only covered the first
        // two allocations, so module pages were still listed as conventional in
        // the snapshot and the kernel's VM bootstrap side needed 13 more
        // exclusions to cut those holes, producing fragments that directly
        // exceeded the VmBootRegions capacity). UEFI satisfies allocations from
        // conventional free memory and re-labels the allocated pages as
        // LOADER_DATA in the live map — snapshotting after that means the live
        // map is already re-labeled, so a conventional-only snapshot naturally
        // excludes every occupied range, and the PMM map handed to the kernel is
        // the final pre-EBS free state (matching C: kinfo.memmap is the cut
        // version).
        // The file accesses between the uefi::println calls depend on
        // SimpleFileSystem and must complete before ExitBootServices; the
        // snapshot only depends on the live map, so it goes last.
        // Both file accesses must happen before ExitBootServices because
        // they depend on the SimpleFileSystem protocol.
        let file_loader = UefiFileLoader;
        uefi::println!("boot-shim: loading kernel.elf from ESP…");
        let kern = loader::load_kernel_with_loader(&file_loader).expect(
            "Failed to load kernel ELF from ESP — check that the ESP contains \
             /EFI/minix/kernel.elf (loader::KERNEL_PATH); firmware may have \
             not mounted the FAT partition, or the build did not embed the \
             kernel binary into the ESP image",
        );
        uefi::println!("boot-shim: kernel loaded (entry staged)");
        let boot_modules =
            loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages);
        uefi::println!("boot-shim: 12 boot modules loaded");

        // Locate platform descriptor sources (ACPI RSDP and/or DTB) from the
        // UEFI configuration table. Must happen before ExitBootServices because
        // the System Table is only valid while boot services are available.
        uefi::println!("boot-shim: locating platform sources…");
        let platform_sources = find_platform_sources();
        let (param_buf, param_stats) = collect_param_buf();
        uefi::println!(
            "boot-shim: param_buf bytes={} rejected={} dropped={}",
            param_stats.bytes,
            param_stats.rejected,
            param_stats.dropped
        );

        // One snapshot, two lists — conventional (free) plus the
        // non-conventional remainder that hosts everything the kernel
        // executes and touches through low addresses at runtime.
        let (memmap, reserved_regions) = UefiMemoryMap.collect();
        uefi::println!("boot-shim: memmap ok (final pre-EBS snapshot)");
        // Wire the bootstrap-outside-memmap defense (it existed as
        // an uncalled helper). The two LOADER_DATA allocations above must
        // never sit in a page the memmap snapshot reports as conventional
        // — otherwise the kernel's A2 handoff classification would hand
        // live page-table pages to the VM PMM (07-paging_init_design
        // §6.0-A2). Fail fast while the boot-shim can still print.
        assert_bootstrap_outside_memmap(memmap, root_page.0, PAGE_SIZE);
        assert_bootstrap_outside_memmap(memmap, bump_base, bump_end - bump_base);
        // ── aarch64 UEFI chain DTB descriptor source (blob leg) ──
        // When ESP /dumpdtb.dtb exists: parse /memory through `DtbMemoryMap` to
        // get the full DRAM picture, and bound-gate every conventional range in
        // the GetMemoryMap snapshot (`minix_boot::dtb_covers_conventional`) —
        // out of bounds means the firmware snapshot and the platform truth
        // disagree, so fail fast while the shim can still print. v1 semantics =
        // gate + source print, without replacing the conventional source (the
        // DTB full picture includes the LOADER_DATA pages the shim occupies
        // itself; swapping directly would hand that self-occupied area to the
        // kernel PMM — self-allocation accounting is pending a decision); with
        // no blob the behavior is unchanged (per-form opt-in rollout).
        #[cfg(target_arch = "aarch64")]
        match file_loader.read(loader::DUMPDTB_PATH) {
            Some(blob) => {
                let fdt = fdt::Fdt::new(&blob)
                    .unwrap_or_else(|e| panic!("boot-shim: dumpdtb.dtb 不可解析: {e:?}"));
                let (dtb_free, _) =
                    <minix_boot::DtbMemoryMap as minix_boot::MemoryMapSource>::collect(
                        &minix_boot::DtbMemoryMap::new(&fdt, 0),
                    );
                if let Err((base, len)) =
                    minix_boot::dtb_covers_conventional(dtb_free, memmap)
                {
                    panic!(
                        "boot-shim: conventional [{base:#x},+{len:#x}) 在 DTB /memory 全景之外（SD-3 D1 出界闸）"
                    );
                }
                uefi::println!(
                    "boot-shim: memmap source: uefi-getmemorymap gated by dtb /memory (conv={} dtb_regions={})",
                    memmap.len(),
                    dtb_free.len()
                );
            }
            None => {
                uefi::println!(
                    "boot-shim: memmap source: uefi-getmemorymap (no dumpdtb.dtb on ESP)"
                );
            }
        }
        // Real-hardware correction (per-range assertions were once added on a
        // wrong assumption): the kernel segments and boot module segments are
        // **not** in scope for this assertion — this load model is "the shim
        // writes conventional RAM directly (without UEFI allocation), and the
        // kernel's A2 classification deducts boot occupancy range by range" (the
        // `deducted` count in vm_handoff). On real hardware it blew up on the
        // first run (conventional [0x100000,0x800000) containing kernel
        // [0x200000,0x400000) is the expected shape). root/bump are the real
        // UEFI LOADER_DATA allocations; the snapshot ordering guarantees they
        // are excluded, so the assertion stays.

        uefi::println!("boot-shim: building KernelInfo…");

        let kernel_info = build_kernel_info(
            memmap,
            reserved_regions,
            kern.kern_virt_base,
            kern.kern_phys_base,
            kern.kern_size,
            boot_modules,
            // Bootstrap (unpaged kernel) region.
            //
            // C semantics (pre_init.c:114-116):
            //     kinfo.bootstrap_start = &_kern_unpaged_start;
            //     kinfo.bootstrap_len   = &_kern_unpaged_end - &_kern_unpaged_start;
            //
            // In Minix3 C the kernel has a small "unpaged" section that runs
            // before paging is enabled and must remain identity-mapped; its
            // physical range is reclaimed via add_memmap() after boot completes.
            //
            // Rust port design choice: the kernel is higher-half from the
            // very first instruction — there is no separate unpaged section
            // because we never run with paging disabled. The startup trampoline
            // lives in boot-shim (a separate ELF loaded by firmware), not in
            // the kernel proper. Therefore no kernel-side memory needs to be
            // reclaimed at this point; the previous `PhysBytes(0) + len =
            // kern_phys_base.0` was a dangerous over-reclaim that swept up
            // OpenSBI/DTB/U-Boot/boot-shim itself. See 01-boot-shim-bootstrap.md
            // §3.5.1 for the rationale.
            PhysBytes(0),
            0,
            platform_sources,
            param_buf,
        );

        uefi::println!("boot-shim: exiting boot services…");
        exit_boot_services();
        // After EBS, uefi::println! is no longer used (the crate's logging path
        // may depend on boot-services state that no longer exists) — write COM1
        // directly instead (0x3F8, OVMF has already initialized the 16550).
        crate::raw_serial("boot-shim: [raw] boot services exited\r\n");

        BootPrepareResult {
            kernel_info,
            root_page,
            bump_base,
            bump_end,
        }
    }
}

// ── UEFI-specific helper functions ──

/// Convert UEFI memory map to the two physical-region lists the kernel
/// needs: conventional (free) RAM and the occupied (non-conventional)
/// remainder. Single iteration over one snapshot — both lists must come
/// from the SAME `GetMemoryMap` call, or entries could drift between two
/// snapshots (allocations made by the map-buffer allocation itself).
///
/// `build_memmap()` semantics for the conventional list are unchanged
/// (free RAM only, kernel/module occupancy NOT cut — the kernel's A2
/// classification does that). The reserved list is everything
/// else EXCEPT the MMIO descriptor types (`MMIO` / `MMIO_PORT_SPACE`):
/// the running kernel's code,
/// static data, GDT/IDT/TSS, the `KernelInfo` heap object, ACPI tables,
/// LOADER_DATA allocations (root page, bump region, module blobs). MMIO
/// windows are excluded because they are huge device ranges nobody may
/// identity-map cacheably; the two local-APIC/IO-APIC pages the kernel
/// touches on every tick are added kernel-side as arch constants
/// (`kernel_identity_mmio_regions`).
///
/// Leaks the allocations so they have `'static` lifetime — acceptable
/// for boot-stage code that runs exactly once.
///
/// # Ordering invariant (snapshot staleness)
///
/// The two output slices are built from the same pool AFTER this snapshot,
/// so the pages backing them are still reported as conventional/free in
/// `regions`. The handoff is safe only because the kernel's first action in
/// `arch_boot` is `store_kernel_info`, which deep-copies every slice into
/// its own `.bss` BEFORE anything can allocate from the handed-over free
/// list; after that copy these pages are genuinely dead (the shim never
/// returns and never frees them). The map buffer itself is allocated
/// BEFORE its `GetMemoryMap` call, so it is LOADER_DATA-classified in the
/// snapshot and conservatively absent from the free list. Keep both
/// orderings if this function is ever restructured.
/// UEFI-leg `MemoryMapSource` implementation — delegates to [`build_memmaps`]
/// (GetMemoryMap snapshot → conventional + reserved lists). The type lives in
/// boot-shim (the `uefi` crate dependency must not point back into minix-boot,
/// and the orphan rule makes this crate implement the foreign trait with a
/// local type).
use minix_boot::MemoryMapSource as _;

pub struct UefiMemoryMap;

impl minix_boot::MemoryMapSource for UefiMemoryMap {
    fn collect(&self) -> (&'static [MemoryRegion], &'static [MemoryRegion]) {
        build_memmaps()
    }
}

pub fn build_memmaps() -> (&'static [MemoryRegion], &'static [MemoryRegion]) {
    let mmap = boot::memory_map(MemoryType::LOADER_DATA)
        .expect("Failed to get UEFI memory map");

    let mut regions: Vec<MemoryRegion> = Vec::new();
    let mut reserved: Vec<MemoryRegion> = Vec::new();
    // RAM ceiling from the conventional list — descriptors entirely above
    // it (forensics: a 192 MiB `RESERVED` hole at 0xfd0000_0000,
    // the QEMU i440fx high chipset hole) are not memory the kernel can
    // ever run in or touch; identity-mapping device space at 4 KiB
    // granularity is waste at best. Collected in pass 1, applied in
    // pass 2 (GetMemoryMap descriptors are unsorted across types).
    let mut ram_top = 0u64;
    for desc in mmap.entries() {
        if desc.ty == MemoryType::CONVENTIONAL {
            let end = desc.phys_start + desc.page_count * 4096;
            if end > ram_top {
                ram_top = end;
            }
        }
    }
    for desc in mmap.entries() {
        if desc.ty == MemoryType::CONVENTIONAL {
            regions.push(MemoryRegion {
                base: PhysBytes(desc.phys_start),
                len: desc.page_count as usize * 4096,
            });
        } else if desc.ty != MemoryType::MMIO && desc.ty != MemoryType::MMIO_PORT_SPACE {
            if desc.phys_start >= ram_top {
                continue;
            }
            // Forensic note (2026-09-21): the payload landed and
            // the kernel's 512 MiB sanity cap fired — something in this
            // list is enormous despite the MMIO exclusions. Dump every
            // ≥ 4 MiB descriptor (there are only a handful) with its raw
            // type so the next fix can exclude by fact, not guess.
            if desc.page_count >= 1024 {
                uefi::println!(
                    "boot-shim: reserved-big ty={:?} base={:x} pages={:x}",
                    desc.ty, desc.phys_start, desc.page_count
                );
            }
            reserved.push(MemoryRegion {
                base: PhysBytes(desc.phys_start),
                len: desc.page_count as usize * 4096,
            });
        }
    }
    // Forensic note (2026-09-21): real machine showed ident-windows n=2
    // (= arch MMIO only) → reserved list arrives empty at the kernel.
    // Count both lists at the source to split "collection loop finds
    // nothing" from "forwarding loses it".
    uefi::println!(
        "boot-shim: memmaps conv={} reserved={}",
        regions.len(),
        reserved.len()
    );
    (
        Box::leak(regions.into_boxed_slice()),
        Box::leak(reserved.into_boxed_slice()),
    )
}

/// Convert UEFI memory map to a static slice of MemoryRegion.
///
/// Only includes CONVENTIONAL memory (free RAM). See `build_memmaps()`
/// for the reserved-list companion.
pub fn build_memmap() -> &'static [MemoryRegion] {
    build_memmaps().0
}

/// Size of one page-table page (UEFI allocates in 4 KiB pages).
const PAGE_SIZE: u64 = 4096;

/// Physical address ceiling assumed by the map classification. aarch64 writes
/// `TCR_EL1.IPS = 5` (48-bit PA); x86-64 4-level paging tops out at 52-bit.
/// The planned `Paging::phys_addr_limit` accessor replaces this; until then the
/// constant keeps the classifier from rejecting high-DRAM descriptors.
#[cfg(target_arch = "aarch64")]
const PA_LIMIT: u64 = 1 << 48;
#[cfg(target_arch = "x86_64")]
const PA_LIMIT: u64 = 1 << 52;

/// Refuse to boot if a conventional memmap range overlaps the bootstrap
/// allocation `[base, base + len)`.
///
/// The root page and the bump region are LOADER_DATA allocations made
/// before the memmap snapshot (see `prepare_boot` — the snapshot is taken
/// after both allocations, so the conventional-only list excludes them);
/// if firmware ever reports those pages as conventional, the A2
/// classification would hand live page-table pages to the VM PMM
/// (07-paging_init_design §6.0-A2).
fn assert_bootstrap_outside_memmap(memmap: &[MemoryRegion], base: u64, len: u64) {
    for r in memmap {
        let rbase = r.base.0;
        let rend = rbase + r.len as u64;
        assert!(
            base + len <= rbase || rend <= base,
            "conventional memmap [{rbase:#x}, {rend:#x}) overlaps bootstrap allocation [{base:#x}, {:#x})",
            base + len
        );
    }
}

/// UEFI `AllocateMaxAddress` limit for a bootstrap allocation of `size`
/// bytes: UEFI guarantees the allocated base is at or below the limit, so
/// passing `bound - size` keeps the whole region under the bound.
///
/// The bound is `min(BOOT_IDENTITY_MAP_END, VM DM window PA end)` per
/// target architecture (07-paging_init_design §6.1 qualification filter ①): below it the
/// bootstrap tree is simultaneously identity-write-reachable and
/// representable in the VM DM window. The kernel re-validates at DM
/// establishment (`os/kernel/src/dm_coverage.rs`).
fn bootstrap_alloc_limit(size: u64) -> u64 {
    boot_dm_admissible_end()
        .checked_sub(size)
        .expect("bootstrap allocation request exceeds the DM-admissible bound")
}

/// Allocate a single physical page for the root page table.
///
/// MaxAddress keeps the page below the DM-admissible bound so the identity
/// write channel and the VM DM window both cover it (§6.1 qualification filter ①).
pub fn alloc_root_page() -> PhysBytes {
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit(PAGE_SIZE)),
        MemoryType::LOADER_DATA,
        1,
    )
    .expect("Failed to allocate root page below DM-admissible bound");
    PhysBytes(ptr.as_ptr() as u64)
}

/// Allocate a bump region for boot-stage page table page allocation.
///
/// The region backs the VM self page-table tree's lower-level pages, which
/// the identity write channel constructs and the VM DM window must cover —
/// it must sit below the DM-admissible bound (§6.1 qualification filter ①②).
pub fn alloc_bump_region(num_pages: usize) -> (u64, u64) {
    let size = (num_pages as u64) * PAGE_SIZE;
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit(size)),
        MemoryType::LOADER_DATA,
        num_pages,
    )
    .expect("Failed to allocate bump region below DM-admissible bound");
    let base = ptr.as_ptr() as u64;
    let end = base + size;
    (base, end)
}

// ── handoff-v1 producer ────────────────────────────────────────────────
//
// The UEFI legs no longer snapshot the map and hand over a `BootHandoff`
// blob. They allocate one handoff arena on the loader-data pages, serialize
// the T1 protocol into it, classify the final map inside the EBS pairing
// loop, build the bootstrap root, patch the runtime fields in place and jump
// into the standalone image with `x0 = arena_pa` (aarch64) or
// `rbx = arena_pa` (x86_64; the image trampoline moves it to `rdi`). The
// riscv64 U-Boot leg uses the same protocol through `uboot_main`. Direct-boot
// faces (`bootface`/`bootface_a64`) keep the `arch_boot` path.

/// Allocate the handoff arena: exactly [`ARENA_CAP`] bytes of loader data
/// below the identity/DM-admissible bound (the receiver reads it through the
/// bootstrap root's identity window).
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
fn alloc_arena() -> PhysBytes {    let pages = ARENA_CAP / PAGE_SIZE as usize;
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit(ARENA_CAP as u64)),
        MemoryType::LOADER_DATA,
        pages,
    )
    .expect("Failed to allocate the handoff arena below the identity bound");
    PhysBytes(ptr.as_ptr() as u64)
}

/// Allocate a pool buffer for `GetMemoryMap`. The buffer is never freed: the
/// pairing loop may need it until `ExitBootServices` succeeds.
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
fn alloc_map_buffer(bytes: usize) -> *mut u8 {
    boot::allocate_pool(MemoryType::LOADER_DATA, bytes)
        .expect("Failed to allocate the GetMemoryMap buffer")
        .as_ptr()
}

/// Ask firmware for the required map-buffer size with a null buffer.
///
/// `uefi` 0.33 keeps its own `memory_map_size` private, so the producer uses
/// the raw ABI directly. The expected answer is `BUFFER_TOO_SMALL`; a success
/// with size zero (some firmware) falls back to a conservative floor.
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
fn probe_map_buffer_size(boot_services: *mut c_void) -> usize {
    // SAFETY: the system table is alive (before `ExitBootServices`) and the
    // raw BootServices pointer was copied from it.
    let bs = unsafe { &*(boot_services as *const uefi_raw::table::boot::BootServices) };
    let mut size = 0usize;
    let mut key = 0usize;
    let mut desc_size = 0usize;
    let mut desc_version = 0u32;
    // SAFETY: a null map with size 0 is the documented size-probe form; the
    // firmware only writes the four out-parameters.
    let status = unsafe {
        (bs.get_memory_map)(
            &mut size,
            core::ptr::null_mut(),
            &mut key,
            &mut desc_size,
            &mut desc_version,
        )
    };
    if status != uefi_raw::Status::BUFFER_TOO_SMALL && status != uefi_raw::Status::SUCCESS {
        panic!("boot-shim: GetMemoryMap size probe failed: {status:?}");
    }
    let floor = 16 * 1024usize;
    let margin = desc_size.max(64).saturating_mul(8).saturating_add(PAGE_SIZE as usize);
    size.max(floor).saturating_add(margin)
}

/// Raw `GetMemoryMap` adapter for the pairing loop.
///
/// `ebs_pair` hands its `boot_services` parameter back to this thunk, so the
/// raw table pointer travels through the existing `*mut c_void` slot and no
/// global is needed. The firmware field is called with the native `efiapi`
/// ABI (C on aarch64, Win64 on x86_64).
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
unsafe extern "C" fn raw_get_memory_map(
    boot_services: *mut c_void,
    size: *mut usize,
    map: *mut u8,
    key: *mut usize,
    desc_size: *mut usize,
    desc_version: *mut u32,
) -> usize {
    // SAFETY: `boot_services` is the live table pointer the caller passed in;
    // the map pointer is the caller's writable buffer.
    let bs = unsafe { &*(boot_services as *const uefi_raw::table::boot::BootServices) };
    unsafe {
        (bs.get_memory_map)(
            size,
            map.cast::<uefi_raw::table::boot::MemoryDescriptor>(),
            key,
            desc_size,
            desc_version,
        )
    }
    .0
}

/// Raw `ExitBootServices` adapter for the pairing loop.
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
unsafe extern "C" fn raw_exit_boot_services(
    boot_services: *mut c_void,
    image_handle: usize,
    map_key: usize,
) -> usize {
    // SAFETY: same table-pointer contract as `raw_get_memory_map`; the image
    // handle is the value from `uefi::boot::image_handle()`.
    let bs = unsafe { &*(boot_services as *const uefi_raw::table::boot::BootServices) };
    unsafe { (bs.exit_boot_services)(image_handle as uefi_raw::Handle, map_key) }.0
}

/// Write a decimal number through the post-EBS raw serial channel (no
/// allocation, no formatting machinery).
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
fn raw_serial_number(mut value: u64) {
    if value == 0 {
        crate::raw_serial("0");
        return;
    }
    let mut digits = [0u8; 20];
    let mut n = 0;
    while value > 0 && n < digits.len() {
        digits[n] = b'0' + (value % 10) as u8;
        value /= 10;
        n += 1;
    }
    let mut out = [0u8; 20];
    for i in 0..n {
        out[i] = digits[n - 1 - i];
    }
    if let Ok(text) = core::str::from_utf8(&out[..n]) {
        crate::raw_serial(text);
    }
}

/// Write a hexadecimal number through the post-EBS raw serial channel.
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
fn raw_serial_hex(value: u64) {
    crate::raw_serial("0x");
    if value == 0 {
        crate::raw_serial("0");
        return;
    }
    let mut buf = [0u8; 16];
    let mut n = 0;
    let mut started = false;
    for shift in (0..16).rev() {
        let digit = ((value >> (shift * 4)) & 0xf) as u8;
        if digit != 0 || started || shift == 0 {
            buf[n] = if digit < 10 { b'0' + digit } else { b'a' + digit - 10 };
            n += 1;
            started = true;
        }
    }
    if let Ok(text) = core::str::from_utf8(&buf[..n]) {
        crate::raw_serial(text);
    }
}

/// Print the memmap-source line (and enforce the DTB bound gate) after EBS.
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
fn report_memmap_source(dtb_free: Option<&[MemoryRegion]>, conventional: &[MemoryRegion]) {
    match dtb_free {
        Some(dtb) => {
            if let Err((base, len)) = minix_boot::dtb_covers_conventional(dtb, conventional) {
                crate::raw_serial(
                    "boot-shim: conventional range outside DTB /memory: base=",
                );
                raw_serial_hex(base);
                crate::raw_serial(" len=");
                raw_serial_hex(len as u64);
                crate::raw_serial("\r\n");
                panic!("boot-shim: SD-3 D1 gate failed (conventional outside DTB /memory)");
            }
            crate::raw_serial(
                "boot-shim: memmap source: uefi-getmemorymap gated by dtb /memory (conv=",
            );
            raw_serial_number(conventional.len() as u64);
            crate::raw_serial(" dtb_regions=");
            raw_serial_number(dtb.len() as u64);
            crate::raw_serial(")\r\n");
        }
        None => crate::raw_serial(
            "boot-shim: memmap source: uefi-getmemorymap (no dumpdtb.dtb on ESP)\r\n",
        ),
    }
}

/// Read and parse the optional ESP `/dumpdtb.dtb` into the DTB `/memory`
/// free list. Pure read-only gate; the conventional source stays the UEFI
/// map (C17's v1 semantics).
#[cfg(target_arch = "aarch64")]
fn load_dumpdtb_free(file_loader: &dyn FileLoader) -> Option<&'static [MemoryRegion]> {
    let blob = file_loader.read(loader::DUMPDTB_PATH)?;
    let fdt = fdt::Fdt::new(&blob)
        .unwrap_or_else(|e| panic!("boot-shim: dumpdtb.dtb 不可解析: {e:?}"));
    let (free, _) = <minix_boot::DtbMemoryMap as minix_boot::MemoryMapSource>::collect(
        &minix_boot::DtbMemoryMap::new(&fdt, 0),
    );
    Some(free)
}

/// aarch64 UEFI production entry for the v1 handoff.
///
/// Order (frozen by the T1 design): load kernel/modules → platform sources →
/// root/bump → arena → map buffer → non-map payloads → pairing loop
/// (`GetMemoryMap` → classify → raw `ExitBootServices`) → bootstrap root →
/// patch runtime fields → jump with `x0 = arena_pa`.
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {
    uefi::println!("boot-shim: prepare_and_enter enter (handoff v1 arena)");
    let root_page = alloc_root_page();
    let (bump_base, bump_end) = alloc_bump_region(bump_pages);
    // The in-shim kernel copy uses this region for the bootstrap root pages;
    // the same triple travels in the envelope so the image resumes it.
    minix_kernel::boot_alloc::init_boot_pt_alloc(bump_base, bump_end);

    let file_loader = UefiFileLoader;
    #[cfg(target_arch = "aarch64")]
    let dumpdtb_free = load_dumpdtb_free(&file_loader);
    #[cfg(target_arch = "x86_64")]
    let dumpdtb_free: Option<&'static [MemoryRegion]> = None;
    let kern = loader::load_kernel_with_loader(&file_loader).expect(
        "Failed to load kernel ELF from ESP — check that the ESP contains \
         /EFI/minix/kernel.elf (loader::KERNEL_PATH); firmware may have \
         not mounted the FAT partition, or the build did not embed the \
         kernel binary into the ESP image",
    );
    uefi::println!("boot-shim: kernel loaded (entry staged)");
    let boot_modules = loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages);
    uefi::println!("boot-shim: 12 boot modules loaded");
    let platform_sources = find_platform_sources();
    let (param_buf, param_stats) = collect_param_buf();
    uefi::println!(
        "boot-shim: param_buf bytes={} rejected={} dropped={}",
        param_stats.bytes,
        param_stats.rejected,
        param_stats.dropped
    );

    let arena_pa = alloc_arena();
    // SAFETY: the arena is a fresh four-page LOADER_DATA allocation below the
    // identity bound; boot services own it until EBS and nothing else writes
    // those pages. The slice lives for the rest of this `!` function.
    let arena = unsafe {
        core::slice::from_raw_parts_mut(arena_pa.0 as *mut u8, ARENA_CAP)
    };
    let mut serializer = Serializer::new(arena).expect("handoff arena too small");
    // Scalars-only carrier: the real memmap arrives from the pairing loop, but
    // the envelope's scalar fields are already final here.
    let carrier = build_kernel_info(
        &[],
        &[],
        kern.kern_virt_base,
        kern.kern_phys_base,
        kern.kern_size,
        boot_modules,
        PhysBytes(0),
        0,
        platform_sources,
        param_buf,
    );
    serializer
        .write_envelope(&carrier)
        .expect("handoff envelope over capacity");
    serializer
        .write_boot_modules(boot_modules)
        .expect("too many boot modules for the handoff arena");
    serializer
        .write_platform_sources(platform_sources)
        .expect("too many platform sources for the handoff arena");
    serializer
        .write_param_buf(param_buf)
        .expect("param_buf over capacity");

    let system_table = uefi::table::system_table_raw()
        .expect("boot-shim: raw system table missing before EBS")
        .as_ptr();
    // SAFETY: called before `ExitBootServices`; the table pointer is live.
    let boot_services = unsafe { (*system_table).boot_services };
    let boot_services_raw = boot_services as *mut c_void;
    let ops = RawOps {
        get_memory_map: raw_get_memory_map,
        exit_boot_services: raw_exit_boot_services,
    };
    let image_handle = uefi::boot::image_handle();
    let mut buffer_bytes = probe_map_buffer_size(boot_services_raw);
    let classification = loop {
        let buffer_ptr = alloc_map_buffer(buffer_bytes);
        // SAFETY: fresh pool allocation of `buffer_bytes` bytes.
        let buffer = unsafe { core::slice::from_raw_parts_mut(buffer_ptr, buffer_bytes) };
        // SAFETY: the raw ops dereference the live boot-services table; the
        // serializer writes only into the arena. On success the call has
        // performed ExitBootServices and no further boot-service call is made.
        match unsafe {
            ebs_pair::pair_and_classify(
                &ops,
                boot_services_raw,
                image_handle.as_ptr() as usize,
                buffer,
                PA_LIMIT,
                &mut serializer,
            )
        } {
            Ok(classification) => break classification,
            Err(PairError::NeedBiggerBuffer(needed)) => {
                assert!(
                    needed > buffer_bytes,
                    "boot-shim: GetMemoryMap asked for a smaller buffer"
                );
                buffer_bytes = needed.saturating_add(PAGE_SIZE as usize);
            }
            Err(error) => {
                panic!("boot-shim: EBS pairing loop failed: {error:?}");
            }
        }
    };
    crate::raw_serial("boot-shim: [raw] boot services exited (paired)\r\n");

    // Build the real KernelInfo from the semantic partition. The arena stays
    // mapped as identity memory until the first process root switch, which is
    // why the receiver deep-copies it before then.
    let descriptors = decode_descriptors(serializer.as_bytes())
        .expect("boot-shim: handoff descriptors invalid after classification");
    let memmap_len = (descriptors.memmap_byte_len / 16) as usize;
    let reserved_len = (descriptors.reserved_byte_len / 16) as usize;
    // SAFETY: the semantic partitions live inside the arena, are 8-byte
    // aligned (compile-time gate) and hold exactly `len` entries written by
    // the classifier. The slices are not mutated after EBS.
    let memmap: &'static [MemoryRegion] = unsafe {
        core::slice::from_raw_parts(
            (arena_pa.0 + SEMANTIC_MEMMAP_OFFSET as u64) as *const MemoryRegion,
            memmap_len,
        )
    };
    let reserved: &'static [MemoryRegion] = unsafe {
        core::slice::from_raw_parts(
            (arena_pa.0 + SEMANTIC_RESERVED_OFFSET as u64) as *const MemoryRegion,
            reserved_len,
        )
    };
    // The classification must have marked every bootstrap allocation as
    // non-conventional; a containment here would hand live pages to the VM PMM.
    assert_bootstrap_outside_memmap(memmap, root_page.0, PAGE_SIZE);
    assert_bootstrap_outside_memmap(memmap, bump_base, bump_end - bump_base);
    assert_bootstrap_outside_memmap(memmap, arena_pa.0, ARENA_CAP as u64);
    report_memmap_source(dumpdtb_free, memmap);

    let kernel_info = build_kernel_info(
        memmap,
        reserved,
        kern.kern_virt_base,
        kern.kern_phys_base,
        kern.kern_size,
        boot_modules,
        PhysBytes(0),
        0,
        platform_sources,
        param_buf,
    );

    crate::raw_serial("boot-shim: [raw] arena=");
    raw_serial_hex(arena_pa.0);
    crate::raw_serial(" conv=");
    raw_serial_number(classification.conventional as u64);
    crate::raw_serial(" res=");
    raw_serial_number(classification.reserved as u64);
    crate::raw_serial("\r\nboot-shim: [raw] entering kernel image v1\r\n");
    minix_kernel::enter_kernel_image_v1(arena_pa.0, &kernel_info, root_page)
}

/// Exit UEFI boot services.
///
/// After this call, UEFI boot services (AllocatePages, etc.) are no longer
/// available. The caller must have completed all UEFI allocations before
/// calling this.
pub fn exit_boot_services() {
    // SAFETY: caller has finished all uses of boot services; from this
    // point on only runtime services (if any) may be used. The returned
    // memory map is discarded because the kernel already received the
    // CONVENTIONAL regions via `build_memmap()`.
    unsafe {
        let _mmap = boot::exit_boot_services(MemoryType::LOADER_DATA);
    }
}

// ── UEFI implementation of FileLoader ──

/// `FileLoader` backed by UEFI's `SimpleFileSystem` protocol on the ESP.
pub struct UefiFileLoader;

impl FileLoader for UefiFileLoader {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let fs_proto = boot::get_image_file_system(boot::image_handle()).ok()?;
        let mut fs = FileSystem::new(fs_proto);
        // UEFI uses backslashes as path separators; convert from the
        // canonical forward-slash format used by `loader::KERNEL_PATH`.
        let uefi_path = path.replace('/', "\\");
        let cs = uefi::CString16::try_from(uefi_path.as_str()).ok()?;
        fs.read(cs.as_ref()).ok()
    }
}

/// UEFI page allocator used by `load_boot_modules_with_loader`.
fn alloc_module_pages(num_pages: usize) -> Option<u64> {
    let size = (num_pages as u64).checked_mul(PAGE_SIZE)?;
    // T2: the kernel reads module blobs through the bootstrap identity window
    // while loading their ELF segments. Allocate below the same DM-admissible
    // bound as the root/bump pages, then verify the complete range end (not
    // just the start) stays under it.
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit(size)),
        MemoryType::LOADER_DATA,
        num_pages,
    )
    .ok()?;
    let base = ptr.as_ptr() as u64;
    let end = base.checked_add(size)?;
    (end <= boot_dm_admissible_end()).then_some(base)
}

#[cfg(test)]
mod tests {
    //! UEFI-side tests focus on `build_kernel_info` plumbing — the heavy
    //! lifting (ELF parsing, segment copy, module loading) is exercised
    //! by `loader::tests` against a mock file loader, so all firmware
    //! impls share the same coverage.

    use super::*;
    use minix_boot::BootModule;
    use minix_types::VirBytes;

    #[test]
    fn test_build_kernel_info_fields() {
        static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(0x10_0000),
            len: 0x1000,
        }];
        static MODULES: [BootModule; 1] = [BootModule {
            name: "vm",
            start: PhysBytes(0x40_0000),
            len: 1024,
        }];
        static RESERVED: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(0x1da0_0000),
            len: 0x1000,
        }];

        let info = build_kernel_info(
            &MEMMAP,
            &RESERVED,
            VirBytes(0xFFFFFFFF80000000),
            PhysBytes(0x200000),
            0x100_000,
            &MODULES,
            PhysBytes(0x100000), // bootstrap_start
            0x100000,            // bootstrap_len
            &[],                 // platform_sources (empty = no source)
            &[],                 // param_buf (no source in this unit test)
        );

        assert_eq!(info.reserved_regions().len(), 1);
        assert_eq!(info.reserved_regions()[0].base, PhysBytes(0x1da0_0000));

        assert_eq!(info.kern_virt_base, VirBytes(0xFFFFFFFF80000000));
        assert_eq!(info.kern_phys_base, PhysBytes(0x200000));
        assert_eq!(info.kern_size, 0x100_000);
        assert_eq!(info.memmap.len(), 1);
        assert_eq!(info.boot_modules.len(), 1);
        assert_eq!(info.boot_modules[0].name, "vm");
        assert!(info.platform_sources.is_empty());
    }

    /// Disjoint and exactly-abutting ranges are accepted — the predicate is
    /// half-open overlap (`end <= base || region_end <= start`), so touching
    /// edges must not trip it.
    #[test]
    fn test_bootstrap_outside_memmap_accepts_disjoint_and_abutting() {
        static MEMMAP: [MemoryRegion; 2] = [
            MemoryRegion { base: PhysBytes(0x10_0000), len: 0x10_0000 },
            MemoryRegion { base: PhysBytes(0x40_0000), len: 0x10_0000 },
        ];
        // Before the first region, after the last, and exactly abutting the
        // seam at 0x20_0000 from the right.
        assert_bootstrap_outside_memmap(&MEMMAP, 0xf_f000, 0x1000);
        assert_bootstrap_outside_memmap(&MEMMAP, 0x50_0000, 0x1000);
        assert_bootstrap_outside_memmap(&MEMMAP, 0x20_0000, 0x1000);
    }

    /// A bootstrap allocation fully inside a conventional range is the bug
    /// the check exists for (firmware reported live page-table pages as free).
    #[test]
    #[should_panic(expected = "overlaps bootstrap allocation")]
    fn test_bootstrap_outside_memmap_rejects_contained_range() {
        static MEMMAP: [MemoryRegion; 1] =
            [MemoryRegion { base: PhysBytes(0x10_0000), len: 0x10_0000 }];
        assert_bootstrap_outside_memmap(&MEMMAP, 0x14_0000, 0x1000);
    }

    /// Partial overlap across a region boundary must also fail: neither
    /// endpoint is inside the region, but the allocation still covers
    /// conventional pages.
    #[test]
    #[should_panic(expected = "overlaps bootstrap allocation")]
    fn test_bootstrap_outside_memmap_rejects_partial_overlap() {
        static MEMMAP: [MemoryRegion; 1] =
            [MemoryRegion { base: PhysBytes(0x10_0000), len: 0x10_0000 }];
        assert_bootstrap_outside_memmap(&MEMMAP, 0x1f_f000, 0x2000);
    }
}
