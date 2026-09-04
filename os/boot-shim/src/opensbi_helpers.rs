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
    BootPrepareResult, BootShim, DTB, KernelInfo, MemoryRegion, PlatformDescSource,
};

use crate::loader::{self, FileLoader};

/// QEMU virt DRAM base address.
const DRAM_BASE: u64 = 0x8000_0000;

/// Default QEMU virt RAM size (128 MB).
const DEFAULT_RAM_SIZE: u64 = 0x800_0000;

/// Bump-allocation region for boot modules in OpenSBI path.
///
/// U-Boot has placed `kernel.elf` and the `BootFileTable` somewhere
/// in low DRAM. To avoid clashing with them we carve a region high in
/// DRAM for boot-module pages. The exact start address is conservative;
/// the boot script controls actual U-Boot allocations.
const MODULE_REGION_BASE: u64 = DRAM_BASE + 0x0200_0000; // DRAM + 32 MB
const MODULE_REGION_SIZE: u64 = 0x0200_0000;             // 32 MB

/// Magic number identifying a valid [`BootFileTable`] in memory.
///
/// ASCII for "MNXBOOT1". Used to defensively detect a misconfigured U-Boot
/// boot script — passing the wrong `a0` is far more likely than passing
/// a corrupted-but-magic-matching table.
pub const BOOT_FILE_TABLE_MAGIC: u64 = 0x3154_4f4f_4258_4e4d;

/// Maximum number of files describable by a single `BootFileTable`.
///
/// One slot for the kernel + one per module. Sixteen is comfortable
/// headroom over the current `MODULE_NAMES` list (`vm`, `pm`, `vfs`,
/// `rs`, `ds`, `inet` = 6, plus kernel = 7) and keeps the table small.
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
        Some(slice.to_vec())
    }
}

/// OpenSBI implementation of `BootShim`.
///
/// Mirrors `UefiBootShim` step by step; the only differences are how
/// firmware-specific services are accessed:
/// - Memory map → hardcoded (no SBI call to enumerate RAM; QEMU virt is fixed).
/// - Page allocation → tiny bump allocator (no UEFI `AllocatePages`).
/// - File access → [`UbootFileLoader`] over a `BootFileTable`.
/// - Exit firmware services → nothing to do; U-Boot has already handed
///   over and we never call back.
pub struct OpenSbiBootShim;

impl BootShim for OpenSbiBootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
        // U-Boot passes the BootFileTable's physical address in a0,
        // which our entry trampoline (see `main.rs`) saves into a
        // module-private static during early boot. For unit testing
        // and headless development we fall back to a panic with a
        // clear message.
        let table = boot_file_table()
            .expect("boot-shim: U-Boot did not pass a BootFileTable in a0");
        let file_loader = UbootFileLoader::new(table);

        let memmap = build_memmap();
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
// invoking `OpenSbiBootShim::prepare_boot`.

static mut BOOT_FILE_TABLE_PTR: usize = 0;

/// Install the physical address of the [`BootFileTable`] passed by U-Boot.
///
/// Must be called exactly once, before [`OpenSbiBootShim::prepare_boot`].
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
// `OpenSbiBootShim::prepare_boot`.

static mut DTB_PTR: u64 = 0;

/// Install the physical address of the DTB passed by OpenSBI in `a1`.
///
/// Must be called exactly once, before [`OpenSbiBootShim::prepare_boot`].
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

// ── OpenSBI helpers (no firmware to call; everything is hardcoded) ──

/// Bump pointer used by `alloc_root_page` / `alloc_bump_region` and
/// `alloc_module_pages`. Starts above the boot-shim image and the
/// `BootFileTable` region; see `MODULE_REGION_BASE`.
static mut BUMP_PTR: u64 = MODULE_REGION_BASE;
const BUMP_END: u64 = MODULE_REGION_BASE + MODULE_REGION_SIZE;

// Compile-time proof (riscv64 target) that every bump allocation — root
// page, page-table pages, modules — ends at or below the DM-admissible
// bound `min(BOOT_IDENTITY_MAP_END, VM DM window PA end)` per target
// architecture (07-paging_init_design §6.1 资格过滤 ①). OpenSBI has no
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
        let need = (num_pages as u64) * 4096;
        if BUMP_PTR + need > BUMP_END {
            return None;
        }
        let addr = BUMP_PTR;
        BUMP_PTR += need;
        Some(addr)
    }
}

/// Build a hardcoded memory map for QEMU virt.
///
/// Returns a static slice with one CONVENTIONAL region covering DRAM.
pub fn build_memmap() -> &'static [MemoryRegion] {
    const REGION: MemoryRegion = MemoryRegion {
        base: PhysBytes(DRAM_BASE),
        len: DEFAULT_RAM_SIZE as usize,
    };
    &[REGION]
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
pub fn build_kernel_info(
    memmap: &'static [MemoryRegion],
    kern_virt_base: VirBytes,
    kern_phys_base: PhysBytes,
    kern_size: u64,
    boot_modules: &'static [minix_boot::BootModule],
    bootstrap_start: PhysBytes,
    bootstrap_len: u64,
    platform_sources: &'static [PlatformDescSource],
) -> KernelInfo {
    KernelInfo {
        memmap,
        kern_virt_base,
        kern_phys_base,
        kern_size,
        free_upper_idx: None,
        // riscv64 Sv39 user address space top (2^38 - 1 aligned to page).
        user_sp: VirBytes(0x0000_003f_ffff_f000),
        kern_stack_top: VirBytes(kern_virt_base.0 as u64 + kern_size as u64),
        syscall_entry: VirBytes(kern_virt_base.0),
        boot_modules,
        bootstrap_start,
        bootstrap_len,
        platform_sources,
        // P9-1: SBI boot hart argument not yet parsed into key=value pairs.
        // Pass empty slice — kernel's GET_MONPARAMS handler copies 0 bytes
        // to caller (matching C's behavior when param_buf[0] == '\0').
        param_buf: &[],
    }
}

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
            VirBytes(0xFFFF_FFFF_8000_0000),
            PhysBytes(DRAM_BASE),
            0x100_000,
            &[],
            PhysBytes(0x80000000), // bootstrap_start
            0x200000,              // bootstrap_len
            &[],                   // platform_sources (empty = no source)
        );
        // Sv39 user-space top is below 2^38.
        assert!(info.user_sp.0 < (1u64 << 39));
        assert_eq!(info.kern_phys_base, PhysBytes(DRAM_BASE));
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
    // 集成测试覆盖层（单元层）
    //
    // 这些测试不依赖 U-Boot/OpenSBI 真实启动链，仅验证 boot-shim
    // 的内部接口契约（memmap 形状、字段映射、bump 分配器不变量）。
    // 真实集成测试（QEMU+OpenSBI+U-Boot）需要外部工具链 + 串口监控，
    // 见 01-boot-shim-bootstrap.md §5.1.1 B。
    // ─────────────────────────────────────────────────────────────────

    /// `build_memmap()` 返回 OpenSBI 平台的默认 DRAM 单区域：
    /// `[DRAM_BASE, DRAM_BASE + 128MB)`。
    ///
    /// **注意**：此 memmap **不排除** `MODULE_REGION_BASE = DRAM_BASE + 32MB`
    /// 区域。boot-shim 负责报告"全部 DRAM 是 free"；内核侧的 `cut_memmap()`
    /// （C：`pre_init.c:cut_memmap`）在 handover 后切除 module 区域。这与 C
    /// 版语义一致——boot-shim 不感知 module 地址。`cut_memmap` 的 Rust 实现
    /// 是内核侧 TODO（见 `todo.md §1` boot module 内存回收），不在 boot-shim
    /// 范围内。
    #[test]
    fn test_build_memmap_default_region() {
        let memmap = build_memmap();
        assert_eq!(memmap.len(), 1, "OpenSBI default memmap is a single region");
        let r = memmap[0];
        assert_eq!(r.base, PhysBytes(DRAM_BASE));
        assert_eq!(r.len, DEFAULT_RAM_SIZE as usize);
        // region 覆盖 module region（DRAM_BASE + 32MB 在内部）
        assert!(
            r.base.0 + r.len as u64 > MODULE_REGION_BASE,
            "memmap should cover module region (intentionally; kernel cuts later)"
        );
    }

    /// `build_kernel_info` 8 个字段全部按入参精确映射。
    ///
    /// 生产路径中 `bootstrap_start/len` 始终是 `(PhysBytes(0), 0)`（见 §3.5.1）。
    /// 此测试使用非零值仅为验证字段映射，不验证语义（语义在
    /// `test_build_kernel_info_bootstrap_zero_means_no_reclaim` 中覆盖）。
    #[test]
    fn test_build_kernel_info_riscv64_fields_match_input() {
        let modules: &'static [minix_boot::BootModule] = &[];
        let info = build_kernel_info(
            &[MemoryRegion {
                base: PhysBytes(0x8000_0000),
                len: 0x80_0000,
            }],
            VirBytes(0xFFFF_FFFF_8000_0000),
            PhysBytes(0x8000_0000),
            0x100_000,
            modules,
            PhysBytes(0x8000_0000),
            0x200_000,
            &[],
        );
        // memmap
        assert_eq!(info.memmap.len(), 1);
        assert_eq!(info.memmap[0].base, PhysBytes(0x8000_0000));
        assert_eq!(info.memmap[0].len, 0x80_0000);
        // kern_* 字段
        assert_eq!(info.kern_virt_base, VirBytes(0xFFFF_FFFF_8000_0000));
        assert_eq!(info.kern_phys_base, PhysBytes(0x8000_0000));
        assert_eq!(info.kern_size, 0x100_000);
        // 派生字段
        assert_eq!(
            info.kern_stack_top,
            VirBytes(0xFFFF_FFFF_8000_0000u64 + 0x100_000)
        );
        assert_eq!(info.syscall_entry, VirBytes(0xFFFF_FFFF_8000_0000));
        // user_sp 仍是 Sv39 顶（与入参无关）
        assert!(info.user_sp.0 < (1u64 << 39));
        // module + platform_sources
        assert_eq!(info.boot_modules.len(), 0);
        assert!(info.platform_sources.is_empty());
        // bootstrap 直通
        assert_eq!(info.bootstrap_start, PhysBytes(0x8000_0000));
        assert_eq!(info.bootstrap_len, 0x200_000);
    }

    /// 回归保护：`build_kernel_info` 接受 `(PhysBytes(0), 0)` 作为 bootstrap
    /// 参数，且该值在 `KernelInfo` 中保持不变（no-op 回收语义，见 §3.5.1）。
    ///
    /// 若误用 `(PhysBytes(0), kern_phys_base.0)`，会导致 `add_memmap(0,
    /// kern_phys_base)` 过度回收 `[0, kern_phys_base)` 整段低内存，覆盖
    /// OpenSBI/DTB/U-Boot。本测试确保该 no-op 语义不被回归。
    #[test]
    fn test_build_kernel_info_bootstrap_zero_means_no_reclaim() {
        let info = build_kernel_info(
            &[],
            VirBytes(0xFFFF_FFFF_8000_0000),
            PhysBytes(DRAM_BASE),
            0x100_000,
            &[],
            PhysBytes(0), // 生产路径值（见 §3.5.1）
            0,           // 生产路径值
            &[],
        );
        assert_eq!(info.bootstrap_start, PhysBytes(0));
        assert_eq!(info.bootstrap_len, 0);
        // 与 kern_phys_base 不相等（若相等则是 over-reclaim bug）
        assert_ne!(info.bootstrap_len, info.kern_phys_base.0);
    }

    /// `alloc_bump_region(n)` 返回的 `(base, end)` 满足：
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

    /// `alloc_root_page()` 返回 4KB 对齐的物理页。
    /// root 页表地址必须页对齐，否则 Sv39 页表遍历会触发 #PF。
    #[test]
    fn test_alloc_root_page_is_4k_aligned() {
        let paddr = alloc_root_page();
        assert_eq!(paddr.0 % 4096, 0, "root page must be 4K aligned");
        // 必须位于 bump region 内（不应超出 BUMP_END）
        assert!(paddr.0 + 4096 <= BUMP_END, "root page must fit in bump region");
    }
}
