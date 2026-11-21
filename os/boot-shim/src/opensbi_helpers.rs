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
//! U-Boot stages the files for us: its boot script `fatload`s the kernel ELF
//! and every module into the fixed addresses of `uboot_layout`, then
//! `bootelf -p <shim> <dtb_addr>` loads the shim and jumps into it with
//! `(extra_argc, extra_argv)` in `a0`/`a1` (the DTB address arrives as
//! `argv[0]`). The `MNXBOOT1` [`BootFileTable`] is **built by the shim
//! itself** (`uboot_main::build_shim_table`) — not staged by U-Boot (see
//! `uboot_main.rs` for the coherency finding that froze this).
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
use core::sync::atomic::{AtomicU64, Ordering};

use minix_boot::{BumpRegion, DTB, PlatformDescSource};
use minix_types::PhysBytes;

use crate::loader::{self, FileLoader};

/// Bump-allocation region for boot modules in the U-Boot shim path.
///
/// Actual topology (D01-R5 D2-19): DRAM starts at 0x8000_0000; this pool
/// occupies `0x8200_0000..0x8400_0000` (DRAM+32MB, 32MB) — *below* the
/// shim itself (linked at 0x8450_0000, `riscv64-uboot.ld`) and below the
/// kernel staging address (0x8600_0000, `uboot_layout::KERNEL_FILE_STAGE`).
/// "High in DRAM" here means "above the low region U-Boot loads into",
/// not "at the top of DRAM"; the three ranges are disjoint by construction.
const MODULE_REGION_BASE: u64 = 0x8000_0000 + 0x0200_0000; // DRAM + 32 MB
const MODULE_REGION_SIZE: u64 = 0x0200_0000; // 32 MB

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

/// Upper bound of the kernel image's physical LMA span.
///
/// The link-side authority is `os/kernel-image/riscv64.ld`'s
/// `ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000)`: a kernel image is at
/// most 4 MiB wide. Pinning the same bound here lets the shim prove, at
/// compile time, that the whole possible LMA span stays below its bump pool —
/// a relation between two separately linked artifacts that nothing else
/// enforces.
const KERNEL_MAX_LMA_END: u64 = KERN_PHYS_BASE + 0x40_0000;

const _: () = assert!(
    KERNEL_MAX_LMA_END <= MODULE_REGION_BASE,
    "kernel LMA span must end below the shim's bump pool (MODULE_REGION_BASE)"
);

/// Magic number identifying a valid [`BootFileTable`] in memory.
///
/// ASCII for "MNXBOOT1". The table is built by the shim itself
/// (`uboot_main::build_shim_table`), so the magic gate catches a table whose
/// contents were mis-written when the installed address does point at
/// readable memory. A wild address faults at the first read of `magic`; no
/// gate can validate a pointer before dereferencing it.
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
/// Layout is `#[repr(C)]` and mirrored by the shim's own builder
/// (`uboot_main::build_shim_table`); the U-Boot script never populates
/// this table (it only `fatload`s the files to fixed addresses).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootFileEntry {
    /// NUL-terminated UTF-8 path, e.g. `b"/EFI/minix/kernel.elf\0"`.
    ///
    /// The shim's builder (`uboot_main::build_shim_table`) writes the path
    /// and one terminating NUL into a zero-initialized `.bss` table; it also
    /// asserts the path fits within `BOOT_FILE_PATH_MAX - 1` bytes. The rest
    /// of the fixed field stays zero from `.bss` initialization. Unterminated
    /// paths are rejected by [`entry_path_eq`] (lookup returns `None`).
    pub path: [u8; BOOT_FILE_PATH_MAX],
    /// Physical address where U-Boot's `fatload` placed the file bytes.
    ///
    /// The shim's builder fills this from its own staging layout
    /// (`uboot_main::uboot_layout`), the same constants the U-Boot script
    /// uses.
    pub phys_addr: u64,
    /// File length in bytes.
    pub len: u64,
}

/// Directory-style description of the files the boot-shim loads.
///
/// The shim builds this in its own `.bss` (`uboot_main::build_shim_table`)
/// and installs its address via [`install_boot_file_table`]; U-Boot only
/// `fatload`s the files to the fixed staging addresses and passes
/// `(extra_argc, extra_argv)` in `a0`/`a1`. This type reads `magic` first to
/// validate; if it doesn't match, boot is aborted with a clear panic message.
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

/// `FileLoader` backed by a [`BootFileTable`] that the shim built in `.bss`.
///
/// `'a` is the lifetime of the table itself, which lives in the shim's own
/// `.bss` and remains valid for the entire boot-shim lifetime.
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
    fn read<'b>(&'b self, path: &str) -> Option<crate::loader::FileBytes<'b>> {
        let entry = self.table.find(path)?;
        // SAFETY: U-Boot's `fatload` placed `entry.len` bytes at
        // `entry.phys_addr`. The region is identity-mapped during boot
        // and not concurrently modified.
        let slice =
            unsafe { slice::from_raw_parts(entry.phys_addr as *const u8, entry.len as usize) };
        // D-082（I-36）：`read` 只处理字节来源，不做任何 ELF 语义。表里的
        // `entry.len` 已是 `build_shim_table` 用 `staged_elf_used_len` 算出
        // 的修剪值（「表存的是槽容量」是登记时的误判，已核实推翻）——这里
        // 再跑 `segment_iter` 是对已修剪数据的幂等无效功。模块装载层的唯一
        // 修剪点在 `loader::load_boot_modules_into`（`elf_file_end`）。
        // U-Boot staged the file in RAM; hand out a borrow instead of a copy
        // (R18 D-4). The table is written once before the shim runs and is
        // not modified while the bytes are in use.
        Some(crate::loader::FileBytes::Borrowed(slice))
    }
}

/// Prepare this leg's boot: discover memory from the DTB, load the kernel
/// and boot modules, allocate pages, fill the caller's storage, and return
/// the result the kernel entry consumes.
///
/// Mirrors the UEFI production entry step by step; the only differences are how
/// firmware-specific services are accessed:
/// - Memory map → `DtbMemoryMap::new(&fdt, KERN_PHYS_BASE).collect_owned()` (DTB `/memory`;
///   missing DTB halts; I-33-甲/乙 unified every DTB consumer onto the owned
///   form).
/// - Page allocation → tiny bump allocator (no UEFI `AllocatePages`).
/// - File access → [`UbootFileLoader`] over a `BootFileTable`.
/// - Exit firmware services → nothing to do; U-Boot has already handed
///   over and we never call back.
///
/// # Naming (D-73)
///
/// The three roles are kept apart: **U-Boot** is the loader (it stages the
/// files and hands over via `bootelf`), **OpenSBI** is the S-mode runtime
/// service provider this leg inherits, and *this function* is the Minix-RS
/// boot shim that builds its own `BootFileTable` in `.bss`. The retired
/// `RiscvUbootBootShim` type fused "loader" and "runtime provider" into one
/// name (F-R17-01).
pub fn prepare_boot<'a>(
    storage: &'a mut minix_boot::BootStorage,
    bump_pages: usize,
) -> minix_boot::BootPrepareResult<'a> {
    // The shim builds the table itself in .bss
    // (`uboot_main::build_shim_table`) and installs its address into a
    // module-private static before calling this. `a0` carries
    // `(extra_argc, extra_argv)` — not a table address. For unit testing
    // and headless development we fall back to a panic with a clear
    // message.
    let table = boot_file_table().expect(
        "boot-shim: BootFileTable not installed (install_boot_file_table was not called)",
    );
    let file_loader = UbootFileLoader::new(table);

    // memmap source = DTB (OpenSBI/U-Boot pass the DTB in a1 and
    // `install_dtb_ptr` is already in place; the collector is built with
    // `firmware_reserve_end = KERN_PHYS_BASE` — the firmware prefix
    // [DRAM_BASE, KERN_PHYS_BASE) is carved out as reserved, not
    // trimmed at 0: on this leg the OpenSBI region is reclaimable and
    // the module regions are excluded by the kernel's bootstrap-pool
    // exclusions and covered by the A2 deduction record). A missing DTB
    // means an honest halt (no fallback; the single-region hardcoding is
    // exactly the leak surface being closed here).
    {
        let dtb_pa = dtb_ptr()
            .expect("boot-shim: OpenSBI/U-Boot 未递 a1=DTB（I-22：memmap 源=DTB，无回退）");
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
        let fdt =
            fdt::Fdt::new(bytes).expect("boot-shim: DTB parse failed (a1) — halting (I-22)");
        // Owned tables: the storage below is the only owner, so the leg
        // does not keep the collector's `'static` tables alive (R18 D-1
        // stage 2). The source records ride along.
        let owned = minix_boot::DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)
            .collect_owned()
            .expect("boot-shim: DTB memmap rejected (capacity or reservation shape) — halting");
        // Hold the source records and audit coverage before anything is
        // projected; a span without a record behind it must halt.
        minix_boot::audit_source_coverage(owned.free(), owned.reserved(), owned.sources())
            .expect("boot-shim: a projected span has no source record — halting");
        assert!(
            !owned.sources().is_empty(),
            "boot-shim: the DTB leg must carry source records, not an empty partition"
        );
        // Copy the map tables into the caller's storage (one step: fill and
        // commit together).
        storage.fill_map_tables(|memmap_slots, reserved_slots| {
            memmap_slots[..owned.free().len()].copy_from_slice(owned.free());
            reserved_slots[..owned.reserved().len()].copy_from_slice(owned.reserved());
            minix_boot::MapCounts {
                memmap: owned.free().len(),
                reserved: owned.reserved().len(),
            }
        });
        storage.fill_source_records(|source_slots| {
            source_slots[..owned.sources().len()].copy_from_slice(owned.sources());
            owned.sources().len()
        });
        // The SBI leg's parameter source is the standard firmware
        // channel — DTB `/chosen/bootargs` (QEMU `-append` fills it).
        let input = fdt.chosen().bootargs().unwrap_or("");
        let mut stats = minix_boot::ParamStats { bytes: 0, rejected: 0, dropped: 0 };
        storage.fill_param(|buf| {
            stats = minix_boot::normalize_param_buf(input.as_bytes(), None, buf);
            stats.bytes
        });
        crate::raw_serial_line(&alloc::format!(
            "boot-shim: param_buf bytes={} rejected={} dropped={}",
            stats.bytes,
            stats.rejected,
            stats.dropped
        ));
    }
    let root_page = alloc_root_page();
    let (bump_base, bump_end) = alloc_bump_region(bump_pages);
    // Runtime partition check (D-3): the pool's fixed partitions are
    // handed out in one order — root page first, page-table subrange
    // second — and the kernel image resumes exactly that subrange from
    // the wire. If the order ever changes, the root page would sit inside
    // the kernel's bump region and the resumed cursor would hand out live
    // page-table pages.
    assert_eq!(
        root_page.0,
        bump_pool_base(),
        "boot-shim: root page is not the bump pool's first page — halting"
    );
    assert_eq!(
        bump_base,
        root_page.0 + 4096,
        "boot-shim: page-table subrange does not start after the root page — halting"
    );
    assert!(
        bump_end <= bump_end_bound(),
        "boot-shim: page-table subrange runs past the bump pool — halting"
    );

    let kern = loader::load_kernel_with_loader_gated(&file_loader, |layout| {
        // Pre-copy gate (D-5 / F-R16-01): the ELF program headers give the
        // kernel's LMA span before any segment byte is written. Refusing
        // an overlap here is the only point where the pool can still be
        // protected — a post-copy assertion would report the damage after
        // the fact.
        let k0 = layout.kern_phys_base.0;
        let k1 = k0.saturating_add(layout.kern_size);
        assert!(
            k1 <= KERNEL_MAX_LMA_END,
            "boot-shim: kernel LMA span [0x{k0:x}, 0x{k1:x}) exceeds the linker bound 0x{KERNEL_MAX_LMA_END:x} — refusing before any segment write"
        );
        assert!(
            k1 <= MODULE_REGION_BASE || k0 >= BUMP_END,
            "boot-shim: kernel LMA span [0x{k0:x}, 0x{k1:x}) overlaps the bump pool [0x{MODULE_REGION_BASE:x}, 0x{BUMP_END:x}) — refusing before any segment write"
        );
    })
    .expect("boot-shim: failed to parse kernel ELF");
    // The U-Boot leg has no `AllocateAddress` reservation (the firmware
    // placed the image), so the kernel span is only known after the
    // copy. Reject the layout if the already-allocated bootstrap pages
    // land inside it: continuing would put page-table stores on top of
    // the image. The UEFI leg prevents this before allocation; this is
    // the U-Boot leg's post-layout equivalent.
    {
        let k0 = kern.kern_phys_base.0;
        let k1 = k0.saturating_add(kern.kern_size);
        for (what, start, end) in [
            ("root page", root_page.0, root_page.0.saturating_add(4096)),
            ("bump region", bump_base, bump_end),
        ] {
            assert!(
                end <= k0 || start >= k1,
                "boot-shim: {what} overlaps the kernel image span — halting"
            );
        }
    }
    // The loader writes straight into the storage's module slots; no
    // `Box::leak` and no `'static` slice (R18 D-1 stage 2).
    let module_len =
        storage.fill_modules(|slots| {
        loader::load_boot_modules_into(&file_loader, alloc_module_pages, slots)
    });

    // OpenSBI passes the DTB physical address in register a1; the entry
    // trampoline saved it via `install_dtb_ptr(a1)`. On RISC-V only DTB
    // is supported (no ACPI), so the list is `[DTB]` or empty.
    storage.fill_platform(|slots| {
        let Some(pa) = dtb_ptr() else {
            return 0;
        };
        // SAFETY: a1 points at the live DTB in the firmware identity map;
        // this runs before paging is enabled.
        let len = unsafe { minix_boot::dtb_total_size(PhysBytes(pa)) };
        slots[0] = PlatformDescSource::with_optional_len(DTB, PhysBytes(pa), len);
        1
    });

    // The handoff arena: the next bump pages after the module landings, inside
    // the same pool. (v10 / D-062 removed the startup-object set that used to
    // be written here; its one reader is served by the map tables.)
    let arena_pa = bump_alloc_pages(minix_boot::ARENA_CAP / 4096)
        .expect("boot-shim: out of bump memory (arena)");

    // No ExitBootServices analogue on OpenSBI — U-Boot already handed
    // over control before the boot-shim started executing.
    minix_boot::BootPrepareResult {
        report: storage.report(kern.kern_virt_base, kern.kern_phys_base, kern.kern_size),
        root_page,
        bump: minix_types::PhysRange::new(
            minix_types::PhysBytes(bump_base),
            minix_types::PhysBytes(bump_end),
        )
        .expect("prepare_boot: bump region end precedes its base"),
        arena_pa: minix_types::PhysBytes(arena_pa),
    }
}

// ── BootFileTable handoff ──
//
// `uboot_main` builds the table in `.bss` (`build_shim_table`) and calls
// `install_boot_file_table(table_pa)` exactly once before invoking
// `prepare_boot`.

static mut BOOT_FILE_TABLE_PTR: usize = 0;

/// Install the physical address of the shim-built [`BootFileTable`].
///
/// Must be called exactly once, before [`prepare_boot`].
/// The table is built by `uboot_main::build_shim_table` in the shim's own
/// `.bss`; U-Boot does not pass a table.
///
/// # Safety
///
/// Caller must ensure `addr` points to a valid [`BootFileTable`] whose
/// `magic` field equals [`BOOT_FILE_TABLE_MAGIC`]. The table must remain
/// valid for the entire boot-shim lifetime.
pub unsafe fn install_boot_file_table(addr: u64) {
    // Catch double-install: if a caller (e.g., a buggy trampoline) calls
    // this twice without `boot_file_table()` clearing the pointer, we'd
    // silently overwrite and lose track. The check is an unconditional
    // `assert!` so it fails closed in release too — `FileBytes::Borrowed`
    // relies on this table's address stability, and the sibling installers
    // (`BootStorageCell::take`, `pt_alloc::register`) already check in
    // both builds (D-064).
    // SAFETY: single-writer during early boot before secondary harts run.
    assert!(
        BOOT_FILE_TABLE_PTR == 0,
        "install_boot_file_table called twice"
    );
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
// `prepare_boot`.

static mut DTB_PTR: u64 = 0;

/// Install the physical address of the DTB passed by OpenSBI in `a1`.
///
/// Must be called exactly once, before [`prepare_boot`].
/// Passing 0 is equivalent to "no DTB available" (kernel uses QEMU fallback).
///
/// # Safety
///
/// Caller must ensure `addr` points to a valid FDT blob (magic `0xD00DFEED`)
/// if non-zero. The blob must remain valid for the entire boot-shim lifetime.
pub unsafe fn install_dtb_ptr(addr: u64) {
    // SAFETY: single-writer during early boot before secondary harts run.
    // `assert!` for release parity with the sibling installers (D-064): a
    // second install would silently swap the blob every consumer reads.
    assert!(DTB_PTR == 0, "install_dtb_ptr called twice");
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

// ── OpenSBI helpers (no firmware to call; the bump pool base is a compile-time constant) ──

/// Cursor of the U-Boot shim's fixed bump pool. The pool is the compile-time
/// region starting at `MODULE_REGION_BASE` (below the shim image and the
/// kernel/module staging addresses — see the constant's doc).
///
/// The cursor is an `AtomicU64` for the same reason as the kernel-side
/// [`minix_kernel::boot_alloc::BootAlloc`]: boot is single-threaded, and the
/// atomic avoids the `static mut` reference rules of edition 2024. It does
/// not make concurrent allocation linearizable.
static BUMP_NEXT: AtomicU64 = AtomicU64::new(MODULE_REGION_BASE);
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

// Fixed partitions of the pool, asserted at compile time (D-3). The pool is
// handed out in allocation order: root page → page-table subrange → module
// pages → arena. Only the extents known at compile time can be checked here;
// module sizes are runtime, and `bump_alloc` returns `None` instead of
// overrunning. The kernel image resumes exactly the page-table subrange, so
// the root page must stay outside it.
const _: () = assert!(MODULE_REGION_BASE % 4096 == 0, "bump pool base alignment");
const _: () = assert!(MODULE_REGION_SIZE % 4096 == 0, "bump pool size alignment");
const _: () = assert!(
    MODULE_REGION_BASE % minix_boot::BUMP_PAGE_SIZE == 0,
    "bump pool base must be a whole number of boot pages"
);
const _: () = assert!(
    4096 + (crate::BOOT_PT_BUMP_PAGES as u64) * minix_boot::BUMP_PAGE_SIZE
        + minix_boot::ARENA_CAP as u64
        <= MODULE_REGION_SIZE,
    "root page + page-table subrange + arena must fit the bump pool"
);

fn bump_alloc(num_pages: usize) -> Option<u64> {
    // The zero-page rejection lives in `BumpRegion::alloc_pages` (shared with
    // the kernel-side allocator, so the two cannot drift).
    let mut region = BumpRegion::resumed(
        MODULE_REGION_BASE,
        BUMP_NEXT.load(Ordering::Relaxed),
        BUMP_END,
    );
    let addr = region.alloc_pages(num_pages)?;
    BUMP_NEXT.store(region.next(), Ordering::Relaxed);
    Some(addr)
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

/// Page allocator used by the shared `load_boot_modules_into`.
fn alloc_module_pages(num_pages: usize) -> Option<u64> {
    bump_alloc(num_pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_kernel_info;
    use minix_boot::MemoryRegion;
    use minix_types::VirBytes;
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
        let table = build_table(&[("/a", &[0x11, 0x22][..]), ("/b", &[0x33][..])]);
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
        assert_eq!(leaked.valid_entries().len(), BOOT_FILE_TABLE_MAX_ENTRIES);
    }

    #[test]
    fn test_uboot_file_loader_reads_bytes_from_table() {
        let table = build_table(&[("/EFI/minix/modules/vm", &[0xAA, 0xBB, 0xCC][..])]);
        let loader = UbootFileLoader::new(table);
        let data = loader.read("/EFI/minix/modules/vm").unwrap();
        assert_eq!(data.as_slice(), &[0xAA, 0xBB, 0xCC]);
        assert!(
            matches!(data, crate::loader::FileBytes::Borrowed(_)),
            "the U-Boot leg already holds the bytes; a borrow is the D-4 contract"
        );
        assert!(loader.read("/missing").is_none());
    }

    // D-064 / I-32 的两条安装型全局负控**不在本模块**：`DTB_PTR` /
    // `BOOT_FILE_TABLE_PTR` 是进程级静态，装过一次就无法在本进程内复位，
    // 而本模块的其它测试（以及 `prepare_boot` 的未来测试）会读它们。
    // 反例就在下面 —— `test_boot_file_table_validates_magic` 明写「测试不能
    // 安全地装全局」。所以负控放在 `tests/install_write_once.rs`：集成测试
    // 有独立进程，从「未安装」的干净状态起步，并用 `catch_unwind` 断言
    // 第二次安装被拒且没有覆盖已装值（与 `arch/tests/pt_alloc_write_once.rs`
    // 同一范式）。

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
        let report = minix_boot::BootReport {
            memmap: &[],
            reserved_regions: &[],
            boot_modules: &[],
            platform_sources: &[], // empty = no source
            param_buf: &[],        // no source in this unit test
            source_records: &[],
            kern_virt_base: VirBytes(0xFFFF_FFFF_8000_0000),
            kern_phys_base: PhysBytes(0x8000_0000),
            kern_size: 0x100_000,
        };
        let info = build_kernel_info(&report, minix_boot::RamAuthority::DTB);
        // Wiring assertion: the builder must publish the single authority
        // constant itself (host tests take the non-riscv branch = the 48-bit
        // value; the Sv39 value range is pinned on riscv64 targets by
        // minix_types' compile-time invariant).
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
        assert!(
            b > a,
            "bump_alloc must return strictly increasing addresses"
        );
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

    /// `build_kernel_info` maps the fields exactly from the inputs.
    #[test]
    fn test_build_kernel_info_riscv64_fields_match_input() {
        let report = minix_boot::BootReport {
            memmap: &[MemoryRegion {
                base: PhysBytes(0x8000_0000),
                len: 0x80_0000,
            }],
            reserved_regions: &[],
            boot_modules: &[],
            platform_sources: &[],
            param_buf: &[],
            source_records: &[],
            kern_virt_base: VirBytes(0xFFFF_FFFF_8000_0000),
            kern_phys_base: PhysBytes(0x8000_0000),
            kern_size: 0x100_000,
        };
        let info = build_kernel_info(&report, minix_boot::RamAuthority::DTB);
        // memmap
        assert_eq!(info.memmap.len(), 1);
        assert_eq!(info.memmap[0].base, PhysBytes(0x8000_0000));
        assert_eq!(info.memmap[0].len, 0x80_0000);
        // kern_* fields
        assert_eq!(info.kern_virt_base, VirBytes(0xFFFF_FFFF_8000_0000));
        assert_eq!(info.kern_phys_base, PhysBytes(0x8000_0000));
        assert_eq!(info.kern_size, 0x100_000);
        // Derived fields (v10 / D-062): `kern_stack_top` is an accessor,
        // `user_sp` is the compile-time constant, and the syscall entry comes
        // from `syscall_entry_va()` — none of them is carried on the wire.
        assert_eq!(
            info.kern_stack_top(),
            VirBytes(0xFFFF_FFFF_8000_0000u64 + 0x100_000)
        );
        // module + platform_sources
        assert_eq!(info.boot_modules.len(), 0);
        assert!(info.platform_sources.is_empty());
    }

    // The bootstrap over-reclaim regression (D01-F03): C's
    // `add_memmap(0, kern_phys_base)` hazard is structurally impossible
    // after the `bootstrap_start/len` fields left `KernelInfo` — there is
    // no caller-supplied reclaim range left to mistype.
    /// The `(base, end)` returned by `alloc_bump_region(n)` satisfies:
    /// 1. `end - base == n * 4096`
    /// 2. `base >= MODULE_REGION_BASE`
    /// 3. `end <= BUMP_END`
    #[test]
    fn test_alloc_bump_region_round_trip() {
        let (base, end) = alloc_bump_region(2);
        assert_eq!(end - base, 2 * 4096, "end - base must equal n * page_size");
        assert!(
            base >= MODULE_REGION_BASE,
            "base must be inside bump region"
        );
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
        assert!(
            paddr.0 + 4096 <= BUMP_END,
            "root page must fit in bump region"
        );
    }
}
