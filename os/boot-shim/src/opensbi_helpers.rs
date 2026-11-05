//! OpenSBI + U-Boot boot helpers (riscv64).
//!
//! ## Why this stage looks different from UEFI
//!
//! On UEFI systems the firmware itself exposes a `SimpleFileSystem` protocol,
//! so the boot-shim can read `kernel.elf` and modules directly from the ESP.
//!
//! On RISC-V there is no Rust UEFI target (`riscv64-unknown-uefi` does not
//! exist — LLVM cannot emit PE/COFF for riscv64), so the boot-shim cannot
//! call UEFI protocols from inside Rust. The fact that U-Boot *implements*
//! a UEFI subset (`CONFIG_EFI_LOADER`) does not help us, because we cannot
//! build a PE binary to consume it.
//!
//! The standard RISC-V boot chain is therefore:
//!
//! ```text
//! OpenSBI (M-mode)            ← SBI runtime services (console, timer, IPI)
//!   ↓ jr a1
//! U-Boot (S-mode)             ← provides FAT/EXT drivers, `fatload`, `go`
//!   ↓ fatload + go
//! boot-shim (S-mode, ELF)     ← THIS module
//!   ↓ minix_elf parse + copy
//! kernel (S-mode, ELF)
//! ```
//!
//! U-Boot performs the file-IO work for us: a small `boot.cmd` script (a
//! `bootenv`/`uEnv.txt` fragment) issues a sequence of `fatload` commands
//! that copy `kernel.elf` and every module into known physical addresses,
//! and then jumps into the boot-shim with the address of a small
//! [`BootFileTable`] in register `a0`.
//!
//! Inside the boot-shim, [`UbootFileLoader`] looks files up in that table.
//! Once it has the bytes, the rest of the loading pipeline
//! (parse ELF, copy segments, place modules, build `KernelInfo`) is the
//! exact same code as the UEFI path — see [`crate::loader`].
//!
//! ## Why not embed everything in the boot-shim?
//!
//! A simpler design would `include_bytes!()` the kernel and modules into
//! the boot-shim binary at build time. We rejected that because:
//! - It breaks the symmetry with the UEFI path (which reads files from an
//!   ESP at run time).
//! - It conflates "what to boot" with "the boot-shim itself", which is
//!   exactly the coupling UEFI was designed to avoid.
//! - It bloats the boot-shim binary with every module image.
//!
//! Going through U-Boot's `fatload` preserves the UEFI mental model — a
//! firmware-provided file-system view — at the cost of a tiny in-RAM
//! directory table.

use core::slice;

use minix_types::{PhysBytes, VirBytes};
use minix_boot::{
    BootPrepareResult, BootShim, DTB, DtbMemoryMap, KernelInfo,
    MemoryRegion, PlatformDescSource,
};

use crate::build_kernel_info;
use crate::loader::{self, FileLoader};

/// Bump-allocation region for boot modules in OpenSBI path.
///
/// U-Boot has placed `kernel.elf` and the `BootFileTable` somewhere
/// in low DRAM. To avoid clashing with them we carve a region high in
/// DRAM for boot-module pages. The exact start address is conservative;
/// the boot script controls actual U-Boot allocations.
const MODULE_REGION_BASE: u64 = 0x8000_0000 + 0x0200_0000; // DRAM + 32 MB
const MODULE_REGION_SIZE: u64 = 0x0200_0000;             // 32 MB

/// Kernel image physical base (`riscv64.ld` `KERNEL_PHYS_BASE` — the same
/// value `kernel-image/src/bootface.rs` pins as `KERN_PHYS_BASE`). Passed to
/// `DtbMemoryMap::new` as `firmware_reserve_end`: everything below it is the
/// S-mode firmware's territory (OpenSBI) and must stay out of the memmap —
/// the same functional trim bootface applies on the direct leg. Missed here,
/// `[DRAM_BASE, KERN_PHYS_BASE)` leaked into VM's free list and VM picked it
/// as the PMM metadata donor (first region under the DM-window bound); the
/// metadata store into DM(0x80000000) then page-faulted, because the kernel's
/// DM plan correctly refuses to map the firmware range.
const KERN_PHYS_BASE: u64 = 0x8020_0000;

/// Magic number identifying a valid [`BootFileTable`] in memory.
///
/// ASCII for "MNXBOOT1". Used to defensively detect a misconfigured U-Boot
/// boot script — passing the wrong `a0` is far more likely than passing
/// a corrupted-but-magic-matching table.
pub const BOOT_FILE_TABLE_MAGIC: u64 = 0x3154_4f4f_4258_4e4d;

/// Maximum number of files describable by a single `BootFileTable`.
///
/// One slot for the kernel + one per module. Sixteen covers the 12-entry
/// `MODULE_NAMES` boot contract plus the kernel image (13 total) with
/// headroom, and keeps the table small.
pub const BOOT_FILE_TABLE_MAX_ENTRIES: usize = 16;

/// Maximum length of a path stored in a `BootFileEntry`.
pub const BOOT_FILE_PATH_MAX: usize = 64;

/// One entry in a [`BootFileTable`].
///
/// Layout is `#[repr(C)]` so the U-Boot boot script can populate the
/// table from a hand-written binary blob without depending on Rust's
/// layout decisions.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootFileEntry {
    /// NUL-terminated UTF-8 path, e.g. `b"/EFI/minix/kernel.elf\0"`.
    ///
    /// **U-Boot contract**: this field MUST be NUL-terminated within the
    /// first `BOOT_FILE_PATH_MAX - 1` bytes. Unterminated paths are
    /// rejected by [`entry_path_eq`] (lookup returns `None`) and the
    /// kernel silently fails to find the file. The U-Boot boot script
    /// in `os/boot-scripts/` is responsible for zero-padding after the
    /// path bytes.
    pub path: [u8; BOOT_FILE_PATH_MAX],
    /// Physical address of the file bytes (set by U-Boot `fatload`).
    pub phys_addr: u64,
    /// File length in bytes.
    pub len: u64,
}

/// Directory-style description of all files U-Boot pre-loaded into RAM.
///
/// The boot script builds this in memory and passes its physical address
/// to the boot-shim via register `a0`. The boot-shim reads `magic` first
/// to validate; if it doesn't match, boot is aborted with a clear panic
/// message.
#[repr(C)]
pub struct BootFileTable {
    /// Must equal [`BOOT_FILE_TABLE_MAGIC`].
    pub magic: u64,
    /// Number of valid entries in `entries`.
    pub entry_count: u32,
    /// Padding for 8-byte alignment of `entries`.
    pub _pad: u32,
    /// File descriptors. Only the first `entry_count` are valid.
    pub entries: [BootFileEntry; BOOT_FILE_TABLE_MAX_ENTRIES],
}

impl BootFileTable {
    /// Iterate over the valid entries.
    pub fn valid_entries(&self) -> &[BootFileEntry] {
        let n = (self.entry_count as usize).min(BOOT_FILE_TABLE_MAX_ENTRIES);
        &self.entries[..n]
    }

    /// Look up an entry by path (linear scan; the table is tiny).
    pub fn find(&self, path: &str) -> Option<&BootFileEntry> {
        self.valid_entries().iter().find(|e| entry_path_eq(e, path))
    }
}

fn entry_path_eq(entry: &BootFileEntry, path: &str) -> bool {
    let bytes = path.as_bytes();
    if bytes.len() >= BOOT_FILE_PATH_MAX {
        return false;
    }
    entry.path[..bytes.len()] == *bytes && entry.path[bytes.len()] == 0
}

/// `FileLoader` backed by a [`BootFileTable`] that U-Boot has placed
/// somewhere in RAM.
///
/// `'a` is the lifetime of the table itself, which lives in U-Boot-managed
/// RAM and remains valid for the entire boot-shim lifetime (U-Boot never
/// reclaims that region).
pub struct UbootFileLoader<'a> {
    table: &'a BootFileTable,
}

impl<'a> UbootFileLoader<'a> {
    /// Wrap an existing in-RAM `BootFileTable` reference.
    pub fn new(table: &'a BootFileTable) -> Self {
        Self { table }
    }
}

impl<'a> FileLoader for UbootFileLoader<'a> {
    fn read(&self, path: &str) -> Option<alloc::vec::Vec<u8>> {
        let entry = self.table.find(path)?;
        // SAFETY: U-Boot's `fatload` placed `entry.len` bytes at
        // `entry.phys_addr`. The region is identity-mapped during boot
        // and not concurrently modified.
        let slice = unsafe {
            slice::from_raw_parts(entry.phys_addr as *const u8, entry.len as usize)
        };
        // The table stores the slot capacity, not the file length (the shim
        // does not know the size before `fatload`). Trim to the end of the
        // last PT_LOAD segment so `BootModule.len` and the module-page
        // allocation reflect the actual file, not the 1 MiB/2 MiB slot.
        // A non-ELF payload (tests, future formats) keeps the full slot.
        let used = minix_elf::segment_iter(slice)
            .ok()
            .and_then(|iter| {
                iter.map(|seg| (seg.offset + seg.filesz) as usize)
                    .max()
            })
            .filter(|&end| end > 0 && end <= slice.len())
            .unwrap_or(slice.len());
        Some(slice[..used].to_vec())
    }
}

/// OpenSBI implementation of `BootShim`.
///
/// Mirrors `UefiBootShim` step by step; the only differences are how
/// firmware-specific services are accessed:
/// - Memory map → `DtbMemoryMap::collect(dtb_ptr())` (DTB `/memory`;
///   missing DTB halts).
/// - Page allocation → tiny bump allocator (no UEFI `AllocatePages`).
/// - File access → [`UbootFileLoader`] over a `BootFileTable`.
/// - Exit firmware services → nothing to do; U-Boot has already handed
///   over and we never call back.
///
/// # Naming (D-73)
///
/// The three roles are kept apart: **U-Boot** is the loader (it stages the
/// files and hands over via `bootelf`), **OpenSBI** is the S-mode runtime
/// service provider this leg inherits, and *this* struct is the Minix-RS
/// boot shim that consumes U-Boot's `BootFileTable`. The old
/// `RiscvUbootBootShim` name fused "loader" and "runtime provider" into one
/// word; the type describes the loader-side contract, so it is named after
/// U-Boot.
pub struct RiscvUbootBootShim;

impl BootShim for RiscvUbootBootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
        // U-Boot passes the BootFileTable's physical address in a0,
        // which our entry trampoline (see `main.rs`) saves into a
        // module-private static during early boot. For unit testing
        // and headless development we fall back to a panic with a
        // clear message.
        let table = boot_file_table()
            .expect("boot-shim: U-Boot did not pass a BootFileTable in a0");
        let file_loader = UbootFileLoader::new(table);

        // memmap source = DTB (OpenSBI/U-Boot pass the DTB in a1 and
        // `install_dtb_ptr` is already in place; `fw_end = 0` is not
        // trimmed — on this leg the OpenSBI region is reclaimable and the
        // module region is cut by the kernel-side `cut_memmap`, which is
        // equivalent to the former single-region hardcoding while taking the
        // source from the real DTB). A missing DTB means an honest halt (no
        // fallback; the single-region hardcoding is exactly the leak surface
        // being closed here).
        let (memmap, reserved, param_buf) = {
            let dtb_pa = dtb_ptr().expect(
                "boot-shim: OpenSBI/U-Boot 未递 a1=DTB（I-22：memmap 源=DTB，无回退）",
            );
            // SAFETY: a1 is passed in by firmware; the first 8 bytes give
            // totalsize (same as bootface), `Fdt::new` checks the magic/size
            // gates, and a bad blob → expect halts.
            assert!(
                minix_boot::dtb_blob_aligned(dtb_pa),
                "boot-shim: DTB pointer not 8-byte aligned — halting"
            );
            let head = unsafe { core::slice::from_raw_parts(dtb_pa as *const u8, 8) };
            let total_be = u32::from_be_bytes(head[4..8].try_into().expect("dtb header totalsize"));
            assert!(
                minix_boot::dtb_len_ok(total_be),
                "boot-shim: DTB totalsize outside the accepted range — halting"
            );
            assert!(
                dtb_pa.checked_add(total_be as u64).is_some(),
                "boot-shim: DTB range overflows the address space — halting"
            );
            let total = total_be as usize;
            let bytes = unsafe { core::slice::from_raw_parts(dtb_pa as *const u8, total) };
            assert!(
                minix_boot::fdt_layout::validate(dtb_pa, bytes).is_ok(),
                "boot-shim: DTB container invalid — halting"
            );
            let fdt = fdt::Fdt::new(bytes)
                .expect("boot-shim: DTB parse failed (a1) — halting (I-22)");
            let (memmap, reserved, sources) =
                minix_boot::DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)
                    .collect_with_sources()
                    .expect("boot-shim: DTB memmap rejected (capacity or reservation shape) — halting");
            // Hold the source records and audit coverage before anything is
            // projected; a span without a record behind it must halt.
            minix_boot::audit_source_coverage(memmap, reserved, sources)
                .expect("boot-shim: a projected span has no source record — halting");
            // The SBI leg's parameter source is the standard firmware
            // channel — DTB `/chosen/bootargs` (QEMU `-append` fills it).
            let landing: &'static mut [u8] =
                alloc::boxed::Box::leak(
                    alloc::vec![0u8; minix_boot::PARAM_BUF_CAP].into_boxed_slice(),
                );
            let input = fdt.chosen().bootargs().unwrap_or("");
            let stats = minix_boot::normalize_param_buf(input.as_bytes(), None, landing);
            let ptr = landing.as_ptr();
            // SAFETY: leaked 'static landing buffer; the slice covers the
            // normalized prefix only.
            let param = unsafe { core::slice::from_raw_parts(ptr, stats.bytes) };
            (memmap, reserved, param)
        };
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);

        let kern = loader::load_kernel_with_loader(&file_loader)
            .expect("boot-shim: failed to parse kernel ELF");
        let boot_modules =
            loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages);

        // OpenSBI passes the DTB physical address in register a1.
        // The entry trampoline saves it via `install_dtb_ptr(a1)`.
        // If no DTB was passed (e.g., unit test), this returns None
        // and the kernel falls back to QemuVirtDesc.
        //
        // On RISC-V only DTB is supported (no ACPI). The sources list is
        // either `[DTB source]` or empty.
        let platform_sources: &'static [PlatformDescSource] = match dtb_ptr() {
            Some(pa) => {
                use alloc::boxed::Box;
                Box::leak(
                    alloc::vec![PlatformDescSource::new(DTB, PhysBytes(pa))]
                        .into_boxed_slice(),
                )
            }
            None => &[],
        };

        let kernel_info = build_kernel_info(
            memmap,
            reserved,
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
            // lives in boot-shim (a separate ELF loaded by U-Boot via OpenSBI),
            // not in the kernel proper. Therefore no kernel-side memory needs
            // to be reclaimed at this point; the previous `PhysBytes(0) + len
            // = kern_phys_base.0` was a dangerous over-reclaim that swept up
            // OpenSBI firmware, DTB, U-Boot image, and the boot-shim itself.
            // See 01-boot-shim-bootstrap.md §3.5.1 for the rationale.
            PhysBytes(0),
            0,
            platform_sources,
            param_buf,
        );

        // No ExitBootServices analogue on OpenSBI — U-Boot already handed
        // over control before the boot-shim started executing.

        BootPrepareResult {
            kernel_info,
            root_page,
            bump_base,
            bump_end,
        }
    }
}

// ── BootFileTable handoff ──
//
// The trampoline in `main.rs` (or an equivalent setup routine) is
// expected to call `install_boot_file_table(a0)` exactly once before
// invoking `RiscvUbootBootShim::prepare_boot`.

static mut BOOT_FILE_TABLE_PTR: usize = 0;

/// Install the physical address of the [`BootFileTable`] passed by U-Boot.
///
/// Must be called exactly once, before [`RiscvUbootBootShim::prepare_boot`].
///
/// # Safety
///
/// Caller must ensure `addr` points to a valid [`BootFileTable`] whose
/// `magic` field equals [`BOOT_FILE_TABLE_MAGIC`]. The table must remain
/// valid for the entire boot-shim lifetime.
pub unsafe fn install_boot_file_table(addr: u64) {
    // Catch double-install: if a caller (e.g., a buggy trampoline) calls
    // this twice without `boot_file_table()` clearing the pointer, we'd
    // silently overwrite and lose track. The `debug_assert!` fires only
    // in debug builds; release builds skip the check to avoid a load.
    // SAFETY: single-writer during early boot before secondary harts run.
    debug_assert!(BOOT_FILE_TABLE_PTR == 0, "install_boot_file_table called twice");
    unsafe {
        BOOT_FILE_TABLE_PTR = addr as usize;
    }
}

/// Borrow the installed `BootFileTable`, validating its magic.
///
/// Returns `None` if no table was installed or its magic is wrong.
pub fn boot_file_table() -> Option<&'static BootFileTable> {
    // SAFETY: single-writer at install time; readers (this function) run
    // strictly later. The cast is to a valid `BootFileTable` if the magic
    // check below passes.
    let ptr = unsafe { BOOT_FILE_TABLE_PTR };
    if ptr == 0 {
        return None;
    }
    let table = unsafe { &*(ptr as *const BootFileTable) };
    if table.magic != BOOT_FILE_TABLE_MAGIC {
        return None;
    }
    Some(table)
}

// ── DTB pointer handoff ──
//
// OpenSBI passes the Flattened Device Tree (DTB) physical address in
// register `a1` to the S-mode payload. The entry trampoline is expected
// to call `install_dtb_ptr(a1)` exactly once before invoking
// `RiscvUbootBootShim::prepare_boot`.

static mut DTB_PTR: u64 = 0;

/// Install the physical address of the DTB passed by OpenSBI in `a1`.
///
/// Must be called exactly once, before [`RiscvUbootBootShim::prepare_boot`].
/// Passing 0 is equivalent to "no DTB available" (kernel uses QEMU fallback).
///
/// # Safety
///
/// Caller must ensure `addr` points to a valid FDT blob (magic `0xD00DFEED`)
/// if non-zero. The blob must remain valid for the entire boot-shim lifetime.
pub unsafe fn install_dtb_ptr(addr: u64) {
    // SAFETY: single-writer during early boot before secondary harts run.
    debug_assert!(DTB_PTR == 0, "install_dtb_ptr called twice");
    unsafe {
        DTB_PTR = addr;
    }
}

/// Return the DTB physical address passed by OpenSBI, or `None` if
/// no DTB was installed (or was explicitly set to 0).
pub fn dtb_ptr() -> Option<u64> {
    // SAFETY: single-writer at install time; readers run strictly later.
    let ptr = unsafe { DTB_PTR };
    if ptr == 0 {
        return None;
    }
    Some(ptr)
}

// ── OpenSBI helpers (no firmware to call; bump pool base is fixed by the boot script) ──

/// Bump pointer used by `alloc_root_page` / `alloc_bump_region` and
/// `alloc_module_pages`. Starts above the boot-shim image and the
/// `BootFileTable` region; see `MODULE_REGION_BASE`.
static mut BUMP_PTR: u64 = MODULE_REGION_BASE;
const BUMP_END: u64 = MODULE_REGION_BASE + MODULE_REGION_SIZE;

// Compile-time proof (riscv64 target) that every bump allocation — root
// page, page-table pages, modules — ends at or below the DM-admissible
// bound `min(BOOT_IDENTITY_MAP_END, VM DM window PA end)` per target
// architecture (07-paging_init_design §6.1 eligibility filter ①). OpenSBI has no
// AllocateMaxAddress; the fixed pool location must satisfy the bound by
// construction. The kernel re-validates at DM establishment
// (`os/kernel/src/dm_coverage.rs`).
#[cfg(target_arch = "riscv64")]
const _: () = assert!(BUMP_END <= minix_arch::boot_dm_admissible_end());

fn bump_alloc(num_pages: usize) -> Option<u64> {
    // Reject zero-page requests: a successful call with num_pages == 0
    // would return the current BUMP_PTR (a duplicate address) without
    // advancing the pointer, breaking the "no overlap" invariant.
    assert!(num_pages > 0, "bump_alloc: num_pages must be > 0");
    // SAFETY: single-threaded boot context; SMP not yet started.
    unsafe {
        let need = (num_pages as u64).checked_mul(4096)?;
        let end = BUMP_PTR.checked_add(need)?;
        if end > BUMP_END {
            return None;
        }
        let addr = BUMP_PTR;
        BUMP_PTR = end;
        Some(addr)
    }
}

/// Exported for the kernel-image self-boot side (the second legitimate
/// consumer of the same bump pool). The `prepare_boot` leg is "a standalone
/// shim loads kernel.elf", which a self-boot image cannot use (it would copy
/// itself again); yet the page-allocation discipline must stay single-source
/// — the pool base/end and the monotonic cursor exist only in this
/// implementation, and a copy would be a second source of drift.
pub fn bump_alloc_pages(num_pages: usize) -> Option<u64> {
    bump_alloc(num_pages)
}

/// Public pool-end value (for assembly-time bound checks:
/// `MODULE_REGION_BASE + MODULE_REGION_SIZE`).
pub const fn bump_end_bound() -> u64 {
    BUMP_END
}

/// Public pool-base value (wired to the overlap guard asserting that a
/// module source range must not intersect the bump pool — overlap in
/// `copy_nonoverlapping` is UB and silently produces garbage).
pub const fn bump_pool_base() -> u64 {
    MODULE_REGION_BASE
}

/// Public accessor for the table magic (the kernel-image side checks the
/// magic; the literal is not hard-coded a second time).
pub const fn boot_file_table_magic() -> u64 {
    BOOT_FILE_TABLE_MAGIC
}

/// Allocate a single physical page for the root page table.
pub fn alloc_root_page() -> PhysBytes {
    PhysBytes(bump_alloc(1).expect("boot-shim: out of bump memory (root page)"))
}

/// Allocate a bump region for boot-stage page table page allocation.
///
/// Returns `(base, end)` physical addresses.
pub fn alloc_bump_region(num_pages: usize) -> (u64, u64) {
    let base = bump_alloc(num_pages).expect("boot-shim: out of bump memory (bump region)");
    let end = base + (num_pages as u64) * 4096;
    (base, end)
}

/// Page allocator used by the shared `load_boot_modules_with_loader`.
fn alloc_module_pages(num_pages: usize) -> Option<u64> {
    bump_alloc(num_pages)
}

/// Build a KernelInfo struct.
///
/// `bootstrap_start`/`bootstrap_len` describe the boot-shim's physical
/// memory region that the kernel should reclaim after boot completes.
/// C: kinfo.bootstrap_start/len — pre_init.c:114-116
///
/// `platform_sources` is the ordered list of firmware-provided platform
/// descriptor sources. On RISC-V (OpenSBI) this is either `[DTB source]`
/// or empty. The kernel tries each in order, using the first that parses
/// successfully.

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// Build a `BootFileTable` in heap-allocated memory and hand back a
    /// `'static` reference + the backing storage (leaked via `Box::leak`).
    fn build_table(files: &[(&str, &[u8])]) -> &'static BootFileTable {
        let mut table = Box::new(BootFileTable {
            magic: BOOT_FILE_TABLE_MAGIC,
            entry_count: files.len() as u32,
            _pad: 0,
            entries: [BootFileEntry {
                path: [0u8; BOOT_FILE_PATH_MAX],
                phys_addr: 0,
                len: 0,
            }; BOOT_FILE_TABLE_MAX_ENTRIES],
        });

        for (i, (path, data)) in files.iter().enumerate() {
            let pb = path.as_bytes();
            assert!(pb.len() < BOOT_FILE_PATH_MAX);
            table.entries[i].path[..pb.len()].copy_from_slice(pb);
            // Leak the data so its address remains valid for the test.
            let leaked: &'static [u8] = Box::leak(data.to_vec().into_boxed_slice());
            table.entries[i].phys_addr = leaked.as_ptr() as u64;
            table.entries[i].len = leaked.len() as u64;
        }
        Box::leak(table)
    }

    use alloc::boxed::Box;

    #[test]
    fn test_entry_path_eq_matches_nul_terminated() {
        let mut entry = BootFileEntry {
            path: [0u8; BOOT_FILE_PATH_MAX],
            phys_addr: 0,
            len: 0,
        };
        let p = b"/foo";
        entry.path[..p.len()].copy_from_slice(p);
        assert!(entry_path_eq(&entry, "/foo"));
        assert!(!entry_path_eq(&entry, "/foo/bar"));
        assert!(!entry_path_eq(&entry, "/fo"));
    }

    #[test]
    fn test_boot_file_table_find_returns_matching_entry() {
        let table = build_table(&[
            ("/a", &[0x11, 0x22][..]),
            ("/b", &[0x33][..]),
        ]);
        let e = table.find("/b").unwrap();
        assert_eq!(e.len, 1);
        assert!(table.find("/c").is_none());
    }

    #[test]
    fn test_boot_file_table_caps_entry_count() {
        // entry_count larger than capacity is clamped by valid_entries.
        let table = Box::new(BootFileTable {
            magic: BOOT_FILE_TABLE_MAGIC,
            entry_count: 999,
            _pad: 0,
            entries: [BootFileEntry {
                path: [0u8; BOOT_FILE_PATH_MAX],
                phys_addr: 0,
                len: 0,
            }; BOOT_FILE_TABLE_MAX_ENTRIES],
        });
        let leaked: &'static BootFileTable = Box::leak(table);
        assert_eq!(
            leaked.valid_entries().len(),
            BOOT_FILE_TABLE_MAX_ENTRIES
        );
    }

    #[test]
    fn test_uboot_file_loader_reads_bytes_from_table() {
        let table = build_table(&[
            ("/EFI/minix/modules/vm", &[0xAA, 0xBB, 0xCC][..]),
        ]);
        let loader = UbootFileLoader::new(table);
        let data = loader.read("/EFI/minix/modules/vm").unwrap();
        assert_eq!(data, vec![0xAA, 0xBB, 0xCC]);
        assert!(loader.read("/missing").is_none());
    }

    #[test]
    fn test_boot_file_table_validates_magic() {
        let bad = Box::new(BootFileTable {
            magic: 0xDEADBEEF,
            entry_count: 0,
            _pad: 0,
            entries: [BootFileEntry {
                path: [0u8; BOOT_FILE_PATH_MAX],
                phys_addr: 0,
                len: 0,
            }; BOOT_FILE_TABLE_MAX_ENTRIES],
        });
        let leaked: &'static BootFileTable = Box::leak(bad);
        // Direct check (bypasses the global static which the test cannot
        // safely install; see install_boot_file_table for the production
        // path).
        assert_ne!(leaked.magic, BOOT_FILE_TABLE_MAGIC);
    }

    #[test]
    fn test_build_kernel_info_uses_riscv64_user_sp() {
        let info = build_kernel_info(
            &[],
            &[],
            VirBytes(0xFFFF_FFFF_8000_0000),
            PhysBytes(0x8000_0000),
            0x100_000,
            &[],
            PhysBytes(0x80000000), // bootstrap_start
            0x200000,              // bootstrap_len
            &[],                   // platform_sources (empty = no source)
            &[],                   // param_buf (no source in this unit test)
        );
        // Wiring assertion: the builder must publish the single authority
        // constant itself (host tests take the non-riscv branch = the 48-bit
        // value; the Sv39 value range is pinned on riscv64 targets by
        // minix_types' compile-time invariant).
        assert_eq!(info.user_sp.0, minix_types::USER_STACK_TOP);
        assert_eq!(info.kern_phys_base, PhysBytes(0x8000_0000));
        assert!(info.platform_sources.is_empty());
    }

    #[test]
    #[should_panic(expected = "num_pages must be > 0")]
    fn test_bump_alloc_rejects_zero_pages() {
        // num_pages == 0 would return the current BUMP_PTR without
        // advancing, violating the "no overlap" invariant.
        bump_alloc(0);
    }

    #[test]
    fn test_bump_alloc_advances_pointer() {
        // On a fresh state (assuming no prior allocations in this test
        // process), the first allocation returns the BUMP_PTR start.
        // We don't assert the exact value (it depends on test ordering
        // and may have been mutated), only that subsequent allocations
        // return strictly increasing addresses.
        let a = bump_alloc(1).expect("first alloc should succeed");
        let b = bump_alloc(1).expect("second alloc should succeed");
        assert!(b > a, "bump_alloc must return strictly increasing addresses");
    }

    // ─────────────────────────────────────────────────────────────────
    // Integration-test coverage layer (unit level)
    //
    // These tests do not depend on the real U-Boot/OpenSBI boot chain; they
    // only verify the boot-shim's internal interface contracts (memmap
    // shape, field mapping, bump-allocator invariants). The real integration
    // test (QEMU+OpenSBI+U-Boot) needs an external toolchain plus serial
    // monitoring; see 01-boot-shim-bootstrap.md §5.1.1 B.
    // ─────────────────────────────────────────────────────────────────

    /// `build_kernel_info` maps all 8 fields exactly from the inputs.
    ///
    /// On the production path `bootstrap_start/len` are always
    /// `(PhysBytes(0), 0)` (see §3.5.1). This test uses nonzero values only
    /// to verify field mapping, not semantics (semantics are covered by
    /// `test_build_kernel_info_bootstrap_zero_means_no_reclaim`).
    #[test]
    fn test_build_kernel_info_riscv64_fields_match_input() {
        let modules: &'static [minix_boot::BootModule] = &[];
        let info = build_kernel_info(
            &[MemoryRegion {
                base: PhysBytes(0x8000_0000),
                len: 0x80_0000,
            }],
            &[],
            VirBytes(0xFFFF_FFFF_8000_0000),
            PhysBytes(0x8000_0000),
            0x100_000,
            modules,
            PhysBytes(0x8000_0000),
            0x200_000,
            &[],
            &[],
        );
        // memmap
        assert_eq!(info.memmap.len(), 1);
        assert_eq!(info.memmap[0].base, PhysBytes(0x8000_0000));
        assert_eq!(info.memmap[0].len, 0x80_0000);
        // kern_* fields
        assert_eq!(info.kern_virt_base, VirBytes(0xFFFF_FFFF_8000_0000));
        assert_eq!(info.kern_phys_base, PhysBytes(0x8000_0000));
        assert_eq!(info.kern_size, 0x100_000);
        // Derived fields
        assert_eq!(
            info.kern_stack_top,
            VirBytes(0xFFFF_FFFF_8000_0000u64 + 0x100_000)
        );
        assert_eq!(info.syscall_entry, VirBytes(0xFFFF_FFFF_8000_0000));
        // user_sp is the single authority constant (independent of the
        // inputs); host tests take the non-riscv branch, and the Sv39 value
        // range is pinned on riscv64 by minix_types' compile-time invariant.
        assert_eq!(info.user_sp.0, minix_types::USER_STACK_TOP);
        // module + platform_sources
        assert_eq!(info.boot_modules.len(), 0);
        assert!(info.platform_sources.is_empty());
        // bootstrap passthrough
        assert_eq!(info.bootstrap_start, PhysBytes(0x8000_0000));
        assert_eq!(info.bootstrap_len, 0x200_000);
    }

    /// Regression guard: `build_kernel_info` accepts `(PhysBytes(0), 0)` as
    /// the bootstrap arguments and the value stays unchanged in `KernelInfo`
    /// (no-op reclaim semantics, see §3.5.1).
    ///
    /// Passing `(PhysBytes(0), kern_phys_base.0)` by mistake would make
    /// `add_memmap(0, kern_phys_base)` over-reclaim the entire low-memory
    /// range `[0, kern_phys_base)`, covering OpenSBI/DTB/U-Boot. This test
    /// keeps that no-op semantics from regressing.
    #[test]
    fn test_build_kernel_info_bootstrap_zero_means_no_reclaim() {
        let info = build_kernel_info(
            &[],
            &[],
            VirBytes(0xFFFF_FFFF_8000_0000),
            PhysBytes(0x8000_0000),
            0x100_000,
            &[],
            PhysBytes(0), // production-path value (see §3.5.1)
            0,           // production-path value
            &[],
            &[],
        );
        assert_eq!(info.bootstrap_start, PhysBytes(0));
        assert_eq!(info.bootstrap_len, 0);
        // Not equal to kern_phys_base (equality would be an over-reclaim bug)
        assert_ne!(info.bootstrap_len, info.kern_phys_base.0);
    }

    /// The `(base, end)` returned by `alloc_bump_region(n)` satisfies:
    /// 1. `end - base == n * 4096`
    /// 2. `base >= MODULE_REGION_BASE`
    /// 3. `end <= BUMP_END`
    #[test]
    fn test_alloc_bump_region_round_trip() {
        let (base, end) = alloc_bump_region(2);
        assert_eq!(end - base, 2 * 4096, "end - base must equal n * page_size");
        assert!(base >= MODULE_REGION_BASE, "base must be inside bump region");
        assert!(end <= BUMP_END, "end must be inside bump region");
    }

    /// `alloc_root_page()` returns a 4 KiB-aligned physical page.
    /// The root page-table address must be page-aligned; otherwise the Sv39
    /// page-table walk raises #PF.
    #[test]
    fn test_alloc_root_page_is_4k_aligned() {
        let paddr = alloc_root_page();
        assert_eq!(paddr.0 % 4096, 0, "root page must be 4K aligned");
        // Must lie inside the bump region (must not exceed BUMP_END)
        assert!(paddr.0 + 4096 <= BUMP_END, "root page must fit in bump region");
    }
}
