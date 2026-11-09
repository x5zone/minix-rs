//! UEFI-specific boot helpers.
//!
//! Provides the production UEFI entry `prepare_and_enter_kernel_image` and
//! UEFI helper library. The boot-prep interface is the plain function
//! `opensbi_helpers::prepare_boot` on the U-Boot leg; there is no shared
//! firmware trait. Firmware-specific
//! concerns live here; the generic kernel/module loading code lives in
//! [`crate::loader`].
//!
//! Responsibilities of this module:
//! - Wrap UEFI BootServices (memory map, page allocation, exit).
//! - Provide a `FileLoader` impl backed by `SimpleFileSystem`.
//! - Wire the firmware-specific pieces into the shared loader.

use crate::ebs_map;
use alloc::boxed::Box;
use alloc::vec::Vec;
use minix_arch::boot_dm_admissible_end;
use minix_boot::{DTB, MemoryRegion, PlatformDescSource, RSDP};
use minix_types::PhysBytes;
use uefi::Guid;
use uefi::boot::{self, AllocateType};
use uefi::fs::FileSystem;
use uefi::mem::memory_map::{MemoryMap, MemoryType};
use uefi::system;
use uefi::table::cfg::{ACPI_GUID, ACPI2_GUID};

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::loader::{self, FileBytes, FileLoader};

/// Physical span of the loaded kernel image, recorded before any module page
/// is allocated (D01-R7 D2-02).
///
/// The kernel is copied to its ELF physical LMAs, which firmware never marks
/// as allocated. `AllocatePages` can therefore return overlapping pages under
/// memory pressure; recording the span here lets `alloc_module_pages` refuse
/// such a request **before** module bytes are copied (a post-copy check can
/// only report the damage). Both fields are zero until a kernel load lands.
static KERNEL_SPAN_LO: AtomicU64 = AtomicU64::new(0);
static KERNEL_SPAN_HI: AtomicU64 = AtomicU64::new(0);

fn record_kernel_span(kern: &loader::KernelLoadResult) {
    KERNEL_SPAN_HI.store(
        kern.kern_phys_base.0.saturating_add(kern.kern_size),
        Ordering::Relaxed,
    );
    KERNEL_SPAN_LO.store(kern.kern_phys_base.0, Ordering::Relaxed);
}

/// Reserve the kernel image's physical LMA span with the firmware (D01-R13 A).
///
/// The kernel segments are written straight to their ELF physical LMAs, which
/// firmware does not know are occupied: they stay `EfiConventionalMemory` and
/// `AllocatePages`/`AllocatePool` may hand them out later. Claiming the whole
/// span as `LOADER_DATA` via `AllocateType::Address` removes that hazard at
/// the source: no firmware allocation of any kind can return a page inside
/// the kernel. The claim must cover the exact `[kern_phys_base, +kern_size)`
/// span, be page-aligned at both ends, and happen **before** anything is
/// allocated or copied.
///
/// The reservation cannot be folded into the classification: a `LOADER_DATA`
/// descriptor is `ReservedMapped`, so the kernel image leaves the
/// conventional list instead of being deducted from it by `classify`. That
/// changes the memmap's shape (documented in §3.4/§4.2), and the kernel-side
/// deduction of `kern_phys_base..+kern_size` becomes a no-op on UEFI legs
/// while remaining correct on the self-boot legs.
fn reserve_kernel_span(kern: &loader::KernelLoadResult) {
    let base = kern.kern_phys_base.0;
    let size = kern.kern_size;
    assert!(size > 0, "boot-shim: kernel image span is empty");
    assert!(
        base % PAGE_SIZE == 0 && size % PAGE_SIZE == 0,
        "boot-shim: kernel image span [0x{base:x}, +0x{size:x}) is not page-aligned — refusing to reserve"
    );
    let pages = (size / PAGE_SIZE) as usize;
    boot::allocate_pages(
        AllocateType::Address(base),
        MemoryType::LOADER_DATA,
        pages,
    )
    .unwrap_or_else(|e| {
        panic!(
            "boot-shim: firmware refused to reserve the kernel image span [0x{base:x}, +0x{size:x}) ({pages} pages): {e:?} — halting (the span would stay allocatable otherwise)"
        )
    });
    uefi::println!(
        "boot-shim: kernel span reserved [0x{base:x}, 0x{:x}) as LOADER_DATA",
        base.saturating_add(size)
    );
}

/// Assert that a physical range does not overlap the loaded kernel image
/// (D01-R7 D2-02). `kern_lo`/`kern_hi` are the kernel's physical span.
fn assert_range_clear_of_kernel(label: &str, lo: u64, hi: u64, kern_lo: u64, kern_hi: u64) {
    assert!(
        hi <= kern_lo || lo >= kern_hi,
        "boot-shim: {label} [0x{lo:x}, 0x{hi:x}) overlaps the kernel image [0x{kern_lo:x}, 0x{kern_hi:x})"
    );
}

/// Post-load check over the module layout (D01-R7 D2-02). Spans are
/// page-rounded defensively. With page-aligned module starts and a
/// page-aligned kernel start, rounding cannot detect an overlap that the
/// byte length misses, so it is not a second detection dimension.
fn assert_modules_clear_of_kernel(modules: &[minix_boot::BootModule], kern_lo: u64, kern_hi: u64) {
    for module in modules {
        let lo = module.start.0;
        let hi = lo.saturating_add(((module.len as u64) + 4095) & !4095);
        assert!(
            hi <= kern_lo || lo >= kern_hi,
            "boot-shim: module {} at [0x{lo:x}, 0x{hi:x}) overlaps the kernel image [0x{kern_lo:x}, 0x{kern_hi:x})",
            module.name
        );
    }
}

#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
use crate::ebs_pair::{self, PairError, RawOps};
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
use core::ffi::c_void;
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
use minix_boot::{ARENA_CAP, Serializer, decode_descriptors};

/// The UEFI sender's caller-owned semantic tables (R18 D-1). One boot, one
/// writer; the wire image is encoded from this storage and the kernel entry
/// borrows it through a `BootReport`.
static UEFI_BOOT_STORAGE: crate::BootStorageCell = crate::BootStorageCell::new();

/// UEFI Configuration Table GUID for the Device Tree (FDT).
///
/// Defined in the UEFI Specification (2.4+) as `EFI_DEVICE_TREE_GUID`.
/// Not provided by the `uefi` crate, so we define it here.
#[cfg(target_arch = "aarch64")]
const DEVICE_TREE_GUID: Guid = uefi::guid!("b1b621d5-f19c-41a5-830b-d9152c69aae0");

/// Legacy ARM FDT configuration-table GUID used by pre-2.4 firmware.
///
/// Kept as a second accepted GUID so older UEFI builds that published this
/// value still reach the DTB parser. It is not the UEFI-spec value.
#[cfg(target_arch = "aarch64")]
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
#[cfg(feature = "qemu-test-kernels")]
pub struct UefiPoolAllocator;

#[cfg(feature = "qemu-test-kernels")]
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
        // Nothing is returned to the firmware: once boot services are gone the
        // pages belong to the operating system, and firmware has no pool to hand
        // them back to. Blocks freed here are therefore reclaimed only when the
        // whole image is replaced, which is why boot-stage code must not lean on
        // this to keep the pool small.
    }
}

/// Scan the UEFI Configuration Table for platform descriptor sources
/// (ACPI RSDP on x86-64, DTB on ARM64).
///
/// Returns the number of entries written into `out` (the caller's
/// [`crate::BootStorage`] platform-slot array), ordered by boot-shim's
/// preference. The kernel takes the first source that parses successfully.
/// Zero means no source was found — the kernel falls back to `QemuVirtDesc`
/// (dev) or panics (release).
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
pub fn find_platform_sources_into(out: &mut [PlatformDescSource]) -> usize {
    use core::cell::RefCell;
    let sources = RefCell::new(Vec::<PlatformDescSource>::new());

    system::with_config_table(|entries| {
        // On x86-64, look for ACPI RSDP (prefer ACPI 2.0+).
        #[cfg(target_arch = "x86_64")]
        {
            for e in entries {
                if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
                    sources
                        .borrow_mut()
                        .push(PlatformDescSource::new(RSDP, PhysBytes(e.address as u64)));
                    break;
                }
            }
        }

        // On aarch64, prefer DTB first (QEMU virt provides DTB), then ACPI.
        #[cfg(target_arch = "aarch64")]
        {
            for e in entries {
                if e.guid == DEVICE_TREE_GUID || e.guid == LEGACY_DEVICE_TREE_GUID {
                    sources
                        .borrow_mut()
                        .push(PlatformDescSource::new(DTB, PhysBytes(e.address as u64)));
                    break;
                }
            }
            // Fall back to ACPI if DTB not present.
            for e in entries {
                if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
                    sources
                        .borrow_mut()
                        .push(PlatformDescSource::new(RSDP, PhysBytes(e.address as u64)));
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

    let sources = sources.into_inner();
    assert!(
        sources.len() <= out.len(),
        "boot-shim: platform-source slot array has {} slots, firmware supplied {}",
        out.len(),
        sources.len()
    );
    out[..sources.len()].copy_from_slice(&sources);
    // Record the DTB address only: reading its header needs a snapshot that
    // already accepted the header pages, and no memory map has been read yet
    // (brief O-11). The preflight stage reads it once the first map is in
    // hand; a missing DTB leaves the stage idle and
    // `cross_validate_config_dtb` owns the absence diagnostic.
    #[cfg(target_arch = "aarch64")]
    if let Some(source) = out[..sources.len()]
        .iter()
        .find(|s| s.kind() == minix_boot::DTB)
    {
        crate::dtb_copy::note(source.phys_addr().0);
    }
    sources.len()
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
///
/// Normalizes into the caller's landing buffer (`out`, a
/// [`crate::BootStorage`] field) and returns the statistics plus the live
/// prefix length via `out`'s committed length.
fn collect_param_buf_into(out: &mut [u8; minix_boot::PARAM_BUF_CAP]) -> minix_boot::ParamStats {
    let input: Vec<u8> = boot::open_protocol_exclusive::<uefi::proto::loaded_image::LoadedImage>(
        boot::image_handle(),
    )
    .ok()
    .and_then(|image| image.load_options_as_bytes().map(|bytes| bytes.to_vec()))
    .unwrap_or_default();
    minix_boot::normalize_param_buf(&input, None, out)
}

/// UEFI helper surface (the production entry, not a shared boot trait).
///
/// Uses UEFI BootServices to discover memory, allocate pages, load kernel
/// and boot modules from the ESP, then exits boot services before handing
/// control to the kernel.

// ── UEFI-specific helper functions ──

/// Convert UEFI memory map to the two physical-region lists the kernel
/// needs: conventional (free) RAM and the occupied (non-conventional)
/// remainder. Single iteration over one snapshot — both lists must come
/// from the SAME `GetMemoryMap` call, or entries could drift between two
/// snapshots (allocations made by the map-buffer allocation itself).
///
/// `build_memmap()` semantics for the conventional list are unchanged
/// (free RAM only, kernel/module occupancy NOT cut — the kernel's A2
/// classification does that). The reserved list is exactly the
/// `ReservedMapped` disposition set (D01-R5 D2-12): `LOADER_CODE`,
/// `LOADER_DATA`, `BOOT_SERVICES_CODE`, `BOOT_SERVICES_DATA`,
/// `ACPI_RECLAIM` — all `WRITE_BACK` only. Runtime-services types,
/// `ACPI_NVS`, `PERSISTENT`, and every non-write-back descriptor go to
/// the *side* list instead (no ordinary mapping); `RESERVED`/`UNUSABLE`
/// are excluded and `MMIO`/`MMIO_PORT_SPACE` go to the device path.
/// MMIO windows are huge device ranges nobody may identity-map cacheably;
/// the two local-APIC/IO-APIC pages the kernel touches on every tick are
/// added kernel-side as arch constants via `kernel_identity_mmio_regions`.
///
/// Leaks the allocations so they have `'static` lifetime — acceptable
/// for boot-stage code that runs exactly once.
///
/// Snapshot the UEFI memory map and split it into the two tables the kernel is
/// handed.
///
/// The partition rules are not here: this reads the same raw buffer the
/// boot-services pairing loop reads and hands it to [`ebs_map`], so one map
/// cannot be classified one way on the way into the kernel image and another way
/// on the way into the handoff arena. The earlier copy of those rules in this
/// function differed from the pairing loop in validation strictness and in the
/// diagnostics it printed, which is exactly the drift a second implementation
/// invites.
///
/// Membership of the two tables comes from each descriptor's type and access
/// attributes, never from an address heuristic. The previous version also dropped
/// every non-conventional descriptor starting above the highest conventional end,
/// which was not a statement about RAM — firmware reports usable and unusable
/// ranges interleaved, so the top of the free space says nothing about where a
/// mapped range should stop — and it silently discarded ranges that no policy had
/// excluded.
///
/// Classification allocates nothing: descriptors are parsed into [`MAP_STORAGE`]
/// and the ranges land in the fixed tables below, whose capacity is proved before
/// the first write. Ranges that policy keeps out of both tables are counted into
/// the shim's own side list and reported here; they do not reach the kernel, which
/// has no way to consume them yet.
///
/// # Ordering invariant (snapshot staleness)
///
/// The storage the outputs are built from is reserved before the map is read, so
/// classification cannot add a descriptor to the map it is classifying. Allocating
/// while boot services are alive does not always change the memory map — the pool
/// allocator only takes fresh pages from conventional memory when it grows a pool
/// — but when it does, the change lands in ranges already classified. Reserving
/// the storage up front removes the condition instead of relying on it not
/// happening.
///
/// The kernel's first action in `arch_boot` is `store_kernel_info`, which
/// deep-copies both tables into its own `.bss` before anything can allocate from
/// the free list; the shim never returns and never frees, so those pages are dead
/// to the firmware afterwards. Keep both orderings if this function is restructured.
const EMPTY_REGION: MemoryRegion = MemoryRegion {
    base: PhysBytes(0),
    len: 0,
};

/// Write-once storage for the QEMU test path. `initialized` enforces a single
/// writer; after the Release store every read is immutable, so no `static mut`
/// or post-initialization alias exists. The arrays are `'static` because they
/// really do live for the process (they are statics, not leaked heap).
#[cfg(feature = "qemu-test-kernels")]
struct TestMapTables {
    initialized: AtomicBool,
    storage: UnsafeCell<ebs_map::MapStorage>,
    free: UnsafeCell<[MemoryRegion; ebs_map::MAX_MAP_DESCRIPTORS]>,
    reserved: UnsafeCell<[MemoryRegion; ebs_map::MAX_MAP_DESCRIPTORS]>,
    side: UnsafeCell<[MemoryRegion; ebs_map::SIDE_CAP]>,
}

// SAFETY: single-threaded test-kernel initialization; the AtomicBool admits
// exactly one writer, and no reader observes the cells before that writer
// publishes `initialized = true` with Release.
#[cfg(feature = "qemu-test-kernels")]
unsafe impl Sync for TestMapTables {}

#[cfg(feature = "qemu-test-kernels")]
static TEST_MAP_TABLES: TestMapTables = TestMapTables {
    initialized: AtomicBool::new(false),
    storage: UnsafeCell::new(ebs_map::MapStorage::new()),
    free: UnsafeCell::new([EMPTY_REGION; ebs_map::MAX_MAP_DESCRIPTORS]),
    reserved: UnsafeCell::new([EMPTY_REGION; ebs_map::MAX_MAP_DESCRIPTORS]),
    side: UnsafeCell::new([EMPTY_REGION; ebs_map::SIDE_CAP]),
};

#[cfg(feature = "qemu-test-kernels")]
pub fn build_memmaps() -> (&'static [MemoryRegion], &'static [MemoryRegion]) {
    let mmap = boot::memory_map(MemoryType::LOADER_DATA).expect("Failed to get UEFI memory map");

    let entries = mmap.len();
    if entries == 0 {
        uefi::println!("boot-shim: memmaps conv=0 reserved=0 unmapped=0 (empty map)");
        return (&[], &[]);
    }
    // The stride and the start of the descriptor array are read from the crate's
    // own view of the map. Its raw buffer is sized by an allocation hint with
    // slack past the descriptors firmware actually wrote, so the buffer's length
    // is not a whole number of descriptors and cannot be divided down to a
    // stride; the distance between two consecutive typed descriptors is the
    // stride in use, and the crate only hands out descriptors of the layout it
    // validated, so a padded later version cannot be mistaken for it.
    let mut view = mmap.entries();
    let first = view
        .next()
        .expect("boot-shim: memory map counted entries it does not contain");
    let base = core::ptr::from_ref(first) as usize;
    let descriptor_size = match view.next() {
        Some(second) => (core::ptr::from_ref(second) as usize) - base,
        // A one-descriptor map offers no distance to measure; the layout the
        // crate used to produce that single reference is the only answer.
        None => core::mem::size_of_val(first),
    };
    if descriptor_size < core::mem::size_of_val(first) {
        panic!(
            "boot-shim: memory map stride {} cannot hold a version-1 descriptor of {} bytes",
            descriptor_size,
            core::mem::size_of_val(first)
        );
    }
    // SAFETY: the counted walk covers `entries` descriptors at this stride inside
    // the buffer firmware just filled; nothing else writes it on this boot face.
    let buffer =
        unsafe { core::slice::from_raw_parts(base as *const u8, descriptor_size * entries) };

    // SAFETY: boot is single-threaded; `initialized` admits one writer and
    // publishes only after the tables are filled.
    let tables = &TEST_MAP_TABLES;
    assert!(
        !tables.initialized.swap(true, Ordering::AcqRel),
        "boot-shim: build_memmaps called twice on one test kernel"
    );
    unsafe {
        let storage = &mut *tables.storage.get();
        let free = &mut *tables.free.get();
        let reserved = &mut *tables.reserved.get();
        let side = &mut *tables.side.get();
        storage
            .parse_raw_map(
                buffer,
                buffer.len(),
                descriptor_size,
                ebs_map::MEMORY_DESCRIPTOR_VERSION,
                0,
            )
            .unwrap_or_else(|err| panic!("boot-shim: memory map rejected: {err:?}"));
        let descs = storage.descriptors();
        let plan = ebs_map::classify(descs, pa_limit(), storage.side_capacity())
            .unwrap_or_else(|err| panic!("boot-shim: memory map rejected: {err:?}"));
        let (mut free_n, mut reserved_n, mut side_n) = (0usize, 0usize, 0usize);
        plan.write_to(
            descs,
            pa_limit(),
            |_index, disposition, region| match disposition {
                ebs_map::Disposition::Allocate => {
                    free[free_n] = *region;
                    free_n += 1;
                }
                ebs_map::Disposition::ReservedMapped => {
                    reserved[reserved_n] = *region;
                    reserved_n += 1;
                }
                ebs_map::Disposition::Side => {
                    if side_n < side.len() {
                        side[side_n] = *region;
                    }
                    side_n += 1;
                }
                ebs_map::Disposition::Excluded | ebs_map::Disposition::DevicePath => {}
            },
        );
        // Count both lists at the source, so "the walk found nothing" and
        // "forwarding lost it" stay distinguishable on real hardware, and name the
        // ranges that policy left out rather than leaving them implied.
        uefi::println!(
            "boot-shim: memmaps conv={} reserved={} unmapped={}",
            free_n,
            reserved_n,
            side_n
        );
        (
            core::slice::from_raw_parts(free.as_ptr(), free_n),
            core::slice::from_raw_parts(reserved.as_ptr(), reserved_n),
        )
    }
}

/// Convert UEFI memory map to a static slice of MemoryRegion.
///
/// Only includes CONVENTIONAL memory (free RAM). See `build_memmaps()`
/// for the reserved-list companion.
#[cfg(feature = "qemu-test-kernels")]
pub fn build_memmap() -> &'static [MemoryRegion] {
    build_memmaps().0
}

/// Size of one page-table page (UEFI allocates in 4 KiB pages).
const PAGE_SIZE: u64 = 4096;

/// Physical address ceiling assumed by the map classification. aarch64 writes
/// `TCR_EL1.IPS = 5` (48-bit PA); x86-64 4-level paging tops out at 52-bit.
/// The planned `Paging::phys_addr_limit` accessor replaces this; until then the
/// ceiling keeps the classifier from rejecting high-DRAM descriptors. It is the
/// address width this boot face's own page table can represent, so it is read
/// per architecture. A target with no UEFI boot face has no map to classify and
/// no width to report: reaching this there is a wiring mistake, not a state.
#[cfg(target_arch = "aarch64")]
fn pa_limit() -> u64 {
    1 << 48
}
#[cfg(target_arch = "x86_64")]
fn pa_limit() -> u64 {
    1 << 52
}
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn pa_limit() -> u64 {
    unreachable!("no UEFI memory map to classify on this target")
}

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
    // A wrapping end is a malformed input, not an overlap-free range: both
    // operands come from firmware and neither is trusted to stay in range.
    let Some(base_end) = base.checked_add(len) else {
        panic!("bootstrap allocation [{base:#x}, +{len:#x}) overflows the address space");
    };
    for r in memmap {
        let rbase = r.base.0;
        let Some(rend) = rbase.checked_add(r.len as u64) else {
            panic!("conventional memmap region starts at {rbase:#x} with a wrapping length");
        };
        assert!(
            base_end <= rbase || rend <= base,
            "conventional memmap [{rbase:#x}, {rend:#x}) overlaps bootstrap allocation [{base:#x}, {base_end:#x})"
        );
    }
}

/// UEFI `AllocateMaxAddress` limit for a bootstrap allocation.
///
/// The firmware constrains the *uppermost address* of the range it hands back,
/// not its start: `base + size - 1 <= limit`. Passing the last admissible byte
/// (`bound - 1`) is therefore exactly enough to force `base + size <= bound`
/// for any request size. Subtracting the size on top of that was stricter than
/// the goal and could refuse a request that fit — a free region ending right at
/// the bound is large enough, but `bound - size` would not admit it.
///
/// The bound is `min(BOOT_IDENTITY_MAP_END, VM DM window PA end)` per target
/// architecture (07-paging_init_design §6.1 qualification filter ①): below it the
/// bootstrap tree is simultaneously identity-write-reachable and representable
/// in the VM DM window. The kernel re-validates at DM establishment
/// (`os/kernel/src/dm_coverage.rs`).
fn bootstrap_alloc_limit() -> u64 {
    boot_dm_admissible_end()
        .checked_sub(1)
        .expect("DM-admissible bound must sit above address zero")
}

/// Allocate a single physical page for the root page table.
///
/// MaxAddress keeps the page below the DM-admissible bound so the identity
/// write channel and the VM DM window both cover it (§6.1 qualification filter ①).
pub fn alloc_root_page() -> PhysBytes {
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit()),
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
        AllocateType::MaxAddress(bootstrap_alloc_limit()),
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
fn alloc_arena() -> PhysBytes {
    let pages = ARENA_CAP / PAGE_SIZE as usize;
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit()),
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
    let margin = desc_size
        .max(64)
        .saturating_mul(8)
        .saturating_add(PAGE_SIZE as usize);
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
            buf[n] = if digit < 10 {
                b'0' + digit
            } else {
                b'a' + digit - 10
            };
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
fn report_memmap_source(dtb_blob: Option<&[u8]>, conventional: &[MemoryRegion]) {
    // The collector's `'static` storage is claimed once per boot face (the
    // claims capture inside the pairing loop); this post-EBS check is a later
    // consumer, so it takes the owned form and never writes those tables.
    let dtb_map = dtb_blob.map(|blob| {
        let fdt = fdt::Fdt::new(blob).expect("boot-shim: dumpdtb.dtb 不可解析");
        minix_boot::DtbMemoryMap::new(&fdt, 0)
            .collect_owned()
            .expect("boot-shim: DTB memmap rejected — halting")
    });
    match dtb_map {
        Some(map) => {
            let dtb = map.free();
            if let Err((base, len)) = minix_boot::dtb_covers_conventional(dtb, conventional) {
                crate::raw_serial("boot-shim: conventional range outside DTB /memory: base=");
                raw_serial_hex(base);
                crate::raw_serial(" len=");
                raw_serial_hex(len as u64);
                crate::raw_serial("\r\n");
                // Diagnostic only: the UEFI map is the RAM authority and the
                // DTB `/memory` node must not influence the boot decision.
                crate::raw_serial(
                    "boot-shim: WARN dtb /memory mismatch (diagnostic only; /memory is ignored under UEFI)\r\n",
                );
            }
            crate::raw_serial(
                "boot-shim: memmap source: uefi-getmemorymap (dtb /memory cross-checked, diagnostic only; conv=",
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

/// Cross-validate the UEFI snapshot against the configuration-table DTB.
///
/// The aarch64 UEFI leg has exactly three states, and no fourth:
///
/// 1. **No DTB in the configuration table** — the UEFI `GetMemoryMap`
///    snapshot is the only memory-map source (this is also the x86-64
///    case, where the lookup finds nothing and returns here).
/// 2. **DTB present and parseable** — its `/memory` panorama must contain
///    every conventional span the UEFI classifier produced. A span outside
///    the DTB's view means firmware and platform truth disagree; the boot
///    refuses rather than handing a page whose provenance nobody can
///    explain to the VM PMM.
/// 3. **DTB present but unusable** (misaligned, bad container, rejected
///    map) — the boot refuses. Ignoring a present DTB would turn a
///    firmware defect into a silent provenance gap.
///
/// The disk `/dumpdtb.dtb` diagnostic (`report_memmap_source`) is a
/// separate, non-authoritative input and stays diagnostic-only.
fn cross_validate_config_dtb(sources: &[PlatformDescSource], conventional: &[MemoryRegion]) {
    let Some(dtb_pa) = sources
        .iter()
        .find(|s| s.kind() == minix_boot::DTB)
        .map(|s| s.phys_addr().0)
    else {
        crate::raw_serial("boot-shim: platform DTB absent — UEFI map is the only source\r\n");
        return;
    };
    let _ = dtb_pa;
    let owned = crate::dtb_copy::owned().unwrap_or_else(|| {
        panic!("boot-shim: configuration-table DTB present but no owned copy — halting")
    });
    let bytes = owned.bytes();
    assert!(
        minix_boot::fdt_layout::validate(0, bytes).is_ok(),
        "boot-shim: configuration-table DTB container invalid — halting"
    );
    let fdt = fdt::Fdt::new(bytes).unwrap_or_else(|e| {
        panic!("boot-shim: configuration-table DTB unparseable ({e:?}) — halting")
    });
    // Owned form: the post-EBS cross-check is a second consumer on the same
    // boot face, so it must not write the one-shot `'static` tables.
    let map = minix_boot::DtbMemoryMap::new(&fdt, 0)
        .collect_owned()
        .expect("boot-shim: configuration-table DTB memmap rejected — halting");
    match minix_boot::dtb_covers_conventional(map.free(), conventional) {
        Ok(()) => {
            crate::raw_serial("boot-shim: platform DTB cross-validated against the UEFI map\r\n")
        }
        Err((base, len)) => panic!(
            "boot-shim: UEFI conventional [{base:#x},+{len:#x}) lies outside the configuration-table DTB /memory — halting"
        ),
    }
}

/// Read and validate the optional ESP `/dumpdtb.dtb`, returning the leaked
/// blob. The caller parses it at the point of use: the collector's storage is
/// process-wide, so a slice captured here would be overwritten by the next
/// collection (the configuration-table DTB or the claims capture).
#[cfg(target_arch = "aarch64")]
fn load_dumpdtb_blob(file_loader: &dyn FileLoader) -> Option<Vec<u8>> {
    let blob = file_loader.read(loader::DUMPDTB_PATH)?;
    if minix_boot::fdt_layout::validate(0, blob.as_slice()).is_err() {
        panic!("boot-shim: dumpdtb.dtb container invalid");
    }
    if fdt::Fdt::new(blob.as_slice()).is_err() {
        panic!("boot-shim: dumpdtb.dtb 不可解析");
    }
    Some(blob.into_owned())
}

/// x86_64 / aarch64 UEFI production entry for the v1 handoff.
///
/// Order (frozen by the T1 design): root/bump → load kernel/modules →
/// platform sources → arena → envelope + non-map payloads → pairing loop
/// (`GetMemoryMap` → classify → raw `ExitBootServices`) → bootstrap root →
/// patch runtime fields → jump (aarch64 passes the arena in `x0`, x86_64 in
/// `rbx`).
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
pub fn prepare_and_enter_kernel_image(bump_pages: usize) -> ! {
    uefi::println!("boot-shim: prepare_and_enter enter (handoff v1 arena)");
    // D (D01-R13): read the kernel ELF first, then claim its LMA span with
    // firmware, and only then allocate root/bump. Every later firmware
    // allocation (modules, arena, pool buffers) is thereby unable to return a
    // page inside the kernel image, and no bootstrap page can be placed under
    // the kernel's copy destination either.
    let file_loader = UefiFileLoader;
    #[cfg(target_arch = "aarch64")]
    let dumpdtb_blob = load_dumpdtb_blob(&file_loader);
    #[cfg(target_arch = "x86_64")]
    let dumpdtb_blob: Option<Vec<u8>> = None;
    let elf_data = file_loader.read_required(loader::KERNEL_PATH);
    let layout = loader::compute_kernel_layout(elf_data.as_slice()).expect(
        "Failed to parse kernel ELF from ESP — check that the ESP contains \
         /EFI/minix/kernel.elf (loader::KERNEL_PATH); firmware may have \
         not mounted the FAT partition, or the build did not embed the \
         kernel binary into the ESP image",
    );
    // A: claim the span before the root/bump and module allocations, so those
    // pages cannot land under the image. The ELF read itself is an earlier
    // pool allocation (its `Vec` comes from the UEFI allocator) and cannot be
    // ordered after the claim — the layout is only known once the bytes are
    // read. If that buffer happens to sit inside the span, `AllocateAddress`
    // refuses the overlapping claim and the boot halts here, so the failure
    // is fail-closed rather than a silent overlap. The copy itself happens
    // after root/bump so that the bootstrap pages are not overwritten by the
    // image.
    reserve_kernel_span(&layout);
    record_kernel_span(&layout);
    let root_page = alloc_root_page();
    let (bump_base, bump_end) = alloc_bump_region(bump_pages);
    // The in-shim kernel copy uses this region for the bootstrap root pages;
    // the same triple travels in the envelope so the image resumes it.
    minix_kernel::boot_alloc::init_boot_pt_alloc(bump_base, bump_end);
    // SAFETY: the span is reserved above and identity-mapped; the source and
    // destination overlap gate runs inside.
    unsafe { loader::load_segments_into_phys_memory(elf_data.as_slice()) }.expect(
        "Failed to copy kernel ELF segments into their LMAs — the span was reserved but the copy range is malformed",
    );
    let kern = layout;
    uefi::println!("boot-shim: kernel loaded (entry staged)");
    // Same-family check (D01-R7): root/bump were allocated before the kernel
    // copy, so they must not sit inside the kernel's LMA span.
    assert_range_clear_of_kernel(
        "root page",
        root_page.0,
        root_page.0.saturating_add(PAGE_SIZE),
        kern.kern_phys_base.0,
        kern.kern_phys_base.0.saturating_add(kern.kern_size),
    );
    assert_range_clear_of_kernel(
        "bump region",
        bump_base,
        bump_end,
        kern.kern_phys_base.0,
        kern.kern_phys_base.0.saturating_add(kern.kern_size),
    );
    let storage = UEFI_BOOT_STORAGE.get();
    let module_len = loader::load_boot_modules_into(
        &file_loader,
        alloc_module_pages,
        storage.module_slots_mut(),
    );
    storage.set_modules_len(module_len);
    uefi::println!("boot-shim: 12 boot modules loaded");
    // Same second line as `prepare_boot` (D01-R7 D2-02): the allocator refuses
    // kernel-overlapping pages before copying; this verifies the returned
    // layout using the page-rounded spans the loader allocated.
    assert_modules_clear_of_kernel(
        storage.boot_modules(),
        kern.kern_phys_base.0,
        kern.kern_phys_base.0.saturating_add(kern.kern_size),
    );
    let platform_len = find_platform_sources_into(storage.platform_slots_mut());
    storage.set_platform_len(platform_len);
    let param_stats = collect_param_buf_into(storage.param_buf_mut());
    storage.set_param_len(param_stats.bytes);
    uefi::println!(
        "boot-shim: param_buf bytes={} rejected={} dropped={}",
        param_stats.bytes,
        param_stats.rejected,
        param_stats.dropped
    );

    let arena_pa = alloc_arena();
    // Defensive check (D01-R6 D2-13): the arena must not overlap the kernel
    // image's physical LMAs (firmware does not know they are occupied).
    assert_range_clear_of_kernel(
        "arena",
        arena_pa.0,
        arena_pa.0.saturating_add(ARENA_CAP as u64),
        kern.kern_phys_base.0,
        kern.kern_phys_base.0.saturating_add(kern.kern_size),
    );
    // SAFETY: the arena is a fresh `ARENA_CAP` (eight-page) LOADER_DATA allocation below the
    // identity bound; boot services own it until EBS and nothing else writes
    // those pages. The slice lives for the rest of this `!` function.
    let arena = unsafe { core::slice::from_raw_parts_mut(arena_pa.0 as *mut u8, ARENA_CAP) };
    let mut serializer = Serializer::new(arena, minix_boot::InputObjectId::UefiMemoryMap)
        .expect("handoff arena too small");
    // Scalars are final here; the payload slices come from the storage, which
    // owns them (no `'static` slice into the arena).
    let report = storage.report(kern.kern_virt_base, kern.kern_phys_base, kern.kern_size);
    serializer
        .write_envelope_report(&report)
        .expect("handoff envelope over capacity");
    serializer
        .write_boot_modules(report.boot_modules)
        .expect("too many boot modules for the handoff arena");
    serializer
        .write_platform_sources(report.platform_sources)
        .expect("too many platform sources for the handoff arena");
    serializer
        .write_param_buf(report.param_buf)
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
        // O-11 preflight snapshot: it authorizes the DTB header read and the
        // copy allocation. Both happen before the pairing loop's own read, so
        // the copy's LOADER_DATA pages are already re-labeled in the final
        // snapshot. With no configuration-table DTB this is a no-op.
        // SAFETY: same contract as the pairing call below.
        match unsafe { ebs_pair::preflight_dtb(&ops, boot_services_raw, buffer) } {
            Ok(()) => {}
            Err(PairError::NeedBiggerBuffer(needed)) => {
                assert!(
                    needed > buffer_bytes,
                    "boot-shim: GetMemoryMap asked for a smaller buffer"
                );
                buffer_bytes = needed.saturating_add(PAGE_SIZE as usize);
                continue;
            }
            Err(error) => {
                panic!("boot-shim: DTB preflight failed: {error:?}");
            }
        }
        // SAFETY: the raw ops dereference the live boot-services table; the
        // serializer writes only into the arena. On success the call has
        // performed ExitBootServices and no further boot-service call is made.
        match unsafe {
            let (memmap_slots, reserved_slots) = storage.map_tables_mut();
            ebs_pair::pair_and_classify(
                &ops,
                boot_services_raw,
                image_handle.as_ptr() as usize,
                buffer,
                pa_limit(),
                &mut serializer,
                memmap_slots,
                reserved_slots,
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

    // Startup-object records: the second record class, in its own partition.
    // Producer-side capacity proof before any write: kernel image + every
    // module + the three bootstrap objects this leg owns (root page, bump
    // region, arena). The serializer still rejects an over-capacity write,
    // but that must not be the first time the count is noticed.
    assert!(
        1 + storage.boot_modules().len() + 3 <= minix_boot::WIRE_STARTUP_CAP,
        "boot-shim: startup-object list needs {} slots, partition holds {}",
        1 + storage.boot_modules().len() + 3,
        minix_boot::WIRE_STARTUP_CAP
    );
    let mut startup = [minix_boot::startup_record(0, 0, 0); minix_boot::WIRE_STARTUP_CAP];
    let mut startup_n = 0usize;
    let mut push_startup = |start: u64, len: u64| {
        if len == 0 {
            return;
        }
        // Over capacity must fail closed: silently dropping the last records
        // would hand the kernel an incomplete startup-object list.
        assert!(
            startup_n < startup.len(),
            "boot-shim: startup-object partition over capacity"
        );
        startup[startup_n] = minix_boot::startup_record(startup_n as u16, start, start + len);
        startup_n += 1;
    };
    push_startup(kern.kern_phys_base.0, kern.kern_size);
    for module in storage.boot_modules() {
        push_startup(module.start.0, module.len as u64);
    }
    push_startup(root_page.0, PAGE_SIZE);
    push_startup(bump_base, bump_end - bump_base);
    push_startup(arena_pa.0, ARENA_CAP as u64);
    serializer
        .write_startup(&startup[..startup_n])
        .expect("boot-shim: startup-object partition over capacity");

    // The classifier wrote the live prefixes into the storage tables; commit
    // the lengths the wire descriptors report. From here on the report is the
    // single semantic view the sender uses — the arena carries no second copy.
    let descriptors = decode_descriptors(serializer.as_bytes())
        .expect("boot-shim: handoff descriptors invalid after classification");
    let memmap_len = (descriptors.memmap_byte_len / 16) as usize;
    let reserved_len = (descriptors.reserved_byte_len / 16) as usize;
    storage.commit_map_tables(memmap_len, reserved_len);
    let report = storage.report(kern.kern_virt_base, kern.kern_phys_base, kern.kern_size);
    cross_validate_config_dtb(report.platform_sources, report.memmap);
    // The classification must have marked every bootstrap allocation as
    // non-conventional; a containment here would hand live pages to the VM PMM.
    assert_bootstrap_outside_memmap(report.memmap, root_page.0, PAGE_SIZE);
    assert_bootstrap_outside_memmap(report.memmap, bump_base, bump_end - bump_base);
    assert_bootstrap_outside_memmap(report.memmap, arena_pa.0, ARENA_CAP as u64);
    report_memmap_source(dumpdtb_blob.as_deref(), report.memmap);

    crate::raw_serial("boot-shim: [raw] arena=");
    raw_serial_hex(arena_pa.0);
    crate::raw_serial(" conv=");
    raw_serial_number(classification.conventional as u64);
    crate::raw_serial(" res=");
    raw_serial_number(classification.reserved as u64);
    crate::raw_serial("\r\nboot-shim: [raw] entering kernel image v1\r\n");
    minix_kernel::enter_kernel_image_v1(arena_pa, &report, root_page)
}

/// Exit UEFI boot services.
///
/// After this call, UEFI boot services (AllocatePages, etc.) are no longer
/// available. The caller must have completed all UEFI allocations before
/// calling this.
#[cfg(feature = "qemu-test-kernels")]
pub fn exit_boot_services() {
    // SAFETY: caller has finished all uses of boot services; from this
    // point on only runtime services (if any) may be used. The returned
    // memory map is discarded: this helper is the test-kernel path only.
    // Its snapshot is best-effort and not MapKey-paired with
    // `build_memmaps`; production uses `ebs_pair` so classification and EBS
    // share one key.
    unsafe {
        let _mmap = boot::exit_boot_services(MemoryType::LOADER_DATA);
    }
}

// ── UEFI implementation of FileLoader ──

/// `FileLoader` backed by UEFI's `SimpleFileSystem` protocol on the ESP.
pub struct UefiFileLoader;

impl FileLoader for UefiFileLoader {
    fn read<'a>(&'a self, path: &str) -> Option<FileBytes<'a>> {
        let fs_proto = boot::get_image_file_system(boot::image_handle()).ok()?;
        let mut fs = FileSystem::new(fs_proto);
        // UEFI uses backslashes as path separators; convert from the
        // canonical forward-slash format used by `loader::KERNEL_PATH`.
        let uefi_path = path.replace('/', "\\");
        let cs = uefi::CString16::try_from(uefi_path.as_str()).ok()?;
        // The firmware protocol returns a fresh buffer: this leg owns it.
        fs.read(cs.as_ref()).ok().map(FileBytes::Owned)
    }
}

/// UEFI page allocator used by `load_boot_modules_into`.
///
/// Refuses any page range that would overlap the loaded kernel image
/// (D01-R7 D2-02). The kernel is written to its ELF physical LMAs, which
/// firmware does not track as allocated, so `AllocatePages` may return such
/// pages under memory pressure; the refusal happens **before** the loader
/// copies module bytes, which is the only point where the overwrite can
/// still be prevented (a post-copy check would only report it).
fn alloc_module_pages(num_pages: usize) -> Option<u64> {
    let size = (num_pages as u64).checked_mul(PAGE_SIZE)?;
    // T2: the kernel reads module blobs through the bootstrap identity window
    // while loading their ELF segments. Allocate below the same DM-admissible
    // bound as the root/bump pages, then verify the complete range end (not
    // just the start) stays under it.
    let ptr = boot::allocate_pages(
        AllocateType::MaxAddress(bootstrap_alloc_limit()),
        MemoryType::LOADER_DATA,
        num_pages,
    )
    .ok()?;
    let base = ptr.as_ptr() as u64;
    let end = base.checked_add(size)?;
    if end > boot_dm_admissible_end() {
        return None;
    }
    let kern_lo = KERNEL_SPAN_LO.load(Ordering::Relaxed);
    let kern_hi = KERNEL_SPAN_HI.load(Ordering::Relaxed);
    if kern_hi > kern_lo && base < kern_hi && end > kern_lo {
        panic!(
            "boot-shim: module page allocation [0x{base:x}, 0x{end:x}) overlaps the kernel image [0x{kern_lo:x}, 0x{kern_hi:x}) — refusing before copy"
        );
    }
    Some(base)
}

#[cfg(test)]
mod tests {
    //! UEFI-side tests focus on `build_kernel_info` plumbing — the heavy
    //! lifting (ELF parsing, segment copy, module loading) is exercised
    //! by `loader::tests` against a mock file loader, so all firmware
    //! impls share the same coverage.

    use super::*;
    use crate::build_kernel_info;
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
            &[], // platform_sources (empty = no source)
            &[], // param_buf (no source in this unit test)
            &[], // memmap_sources
            &[], // startup_objects
            minix_boot::InputObjectId::UefiMemoryMap,
        );

        assert_eq!(info.reserved_regions.len(), 1);
        assert_eq!(info.reserved_regions[0].base, PhysBytes(0x1da0_0000));

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
            MemoryRegion {
                base: PhysBytes(0x10_0000),
                len: 0x10_0000,
            },
            MemoryRegion {
                base: PhysBytes(0x40_0000),
                len: 0x10_0000,
            },
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
        static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(0x10_0000),
            len: 0x10_0000,
        }];
        assert_bootstrap_outside_memmap(&MEMMAP, 0x14_0000, 0x1000);
    }

    /// Partial overlap across a region boundary must also fail: neither
    /// endpoint is inside the region, but the allocation still covers
    /// conventional pages.
    #[test]
    #[should_panic(expected = "overlaps bootstrap allocation")]
    fn test_bootstrap_outside_memmap_rejects_partial_overlap() {
        static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(0x10_0000),
            len: 0x10_0000,
        }];
        assert_bootstrap_outside_memmap(&MEMMAP, 0x1f_f000, 0x2000);
    }

    /// A wrapping region end is malformed firmware input. Under plain `+` it
    /// would compare as a tiny end and pass as "disjoint"; the guard must
    /// refuse it instead.
    #[test]
    #[should_panic(expected = "wrapping length")]
    fn test_bootstrap_outside_memmap_rejects_wrapping_region() {
        static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(0xffff_ffff_ffff_f000),
            len: 0x2000,
        }];
        assert_bootstrap_outside_memmap(&MEMMAP, 0x10_0000, 0x1000);
    }

    /// Likewise for the allocation itself: an end that wraps is not disjoint
    /// from anything.
    #[test]
    #[should_panic(expected = "overflows the address space")]
    fn test_bootstrap_outside_memmap_rejects_wrapping_allocation() {
        static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(0),
            len: 0x1000,
        }];
        assert_bootstrap_outside_memmap(&MEMMAP, 0xffff_ffff_ffff_f000, 0x2000);
    }

    // ── kernel-image overlap guards (D01-R7 D2-02) ──

    /// The reservation claim must be page-aligned at both ends. A non-page-
    /// aligned span is a linker-script bug that would make `AllocateAddress`
    /// reject (or over-claim) the reservation, so the guard has to fire.
    #[test]
    #[should_panic(expected = "is not page-aligned")]
    fn test_reserve_kernel_span_rejects_unaligned_span() {
        // Pure check: no firmware call is reached because the assert fires
        // first. Span [0x200001, +0x400000).
        let kern = loader::KernelLoadResult {
            kern_phys_base: PhysBytes(0x200001),
            kern_virt_base: minix_types::VirBytes(0xFFFF_8000_0000_0000),
            kern_size: 0x40_0000,
            entry_point: 0,
        };
        reserve_kernel_span(&kern);
    }

    #[test]
    fn test_assert_range_clear_of_kernel_accepts_disjoint_and_abutting() {
        // Abutting before, abutting after, and fully disjoint.
        assert_range_clear_of_kernel("range", 0x1000, 0x2000, 0x2000, 0x3000);
        assert_range_clear_of_kernel("range", 0x3000, 0x4000, 0x2000, 0x3000);
        assert_range_clear_of_kernel("range", 0x4000, 0x5000, 0x2000, 0x3000);
    }

    #[test]
    #[should_panic(expected = "overlaps the kernel image")]
    fn test_assert_range_clear_of_kernel_rejects_contained() {
        assert_range_clear_of_kernel("range", 0x2400, 0x2600, 0x2000, 0x3000);
    }

    #[test]
    #[should_panic(expected = "overlaps the kernel image")]
    fn test_assert_range_clear_of_kernel_rejects_partial_overlap() {
        assert_range_clear_of_kernel("range", 0x1000, 0x2800, 0x2000, 0x3000);
    }

    #[test]
    #[should_panic(expected = "overlaps the kernel image")]
    fn test_assert_range_clear_of_kernel_rejects_kernel_inside() {
        assert_range_clear_of_kernel("range", 0x1000, 0x4000, 0x2000, 0x3000);
    }

    /// A module whose bytes cross the kernel start must be refused. Note
    /// that page rounding cannot add detection power here: the module start
    /// is page-aligned, so a byte end before a page-aligned kernel start
    /// implies the zero-filled tail also stops before it.
    #[test]
    #[should_panic(expected = "overlaps the kernel image")]
    fn test_assert_modules_clear_of_kernel_rejects_byte_overlap() {
        static MODULES: [minix_boot::BootModule; 1] = [minix_boot::BootModule {
            name: "vm",
            start: PhysBytes(0x2000),
            len: 0x1001,
        }];
        assert_modules_clear_of_kernel(&MODULES, 0x3000, 0x4000);
    }
}
