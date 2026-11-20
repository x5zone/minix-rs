//! aarch64 self-boot face (the Rust side of QEMU `-kernel` direct-ELF kernel
//! boot; the aarch64 mirror of the riscv `bootface.rs`).
//!
//! `[ARCH: FW_PAYLOAD direct-kernel form (Minix3 has no counterpart)]`: this
//! face is the auxiliary aarch64 boot path, aimed mainly at coverage tests
//! (QEMU `-kernel`); real-board boot normally still goes through UEFI/U-Boot.
//! The mainline is an independent boot-shim plus an independent kernel image;
//! the placement matrix is in chapter 01 §3.7.
//!
//! # Boot model (every difference from the riscv half comes from firmware, not
//! from a design fork)
//!
//! For a non-Linux ELF passed to `-kernel`, QEMU mach-virt takes the
//! `!info->is_linux` path of `do_cpu_reset` (hw/arm/boot.c). **Secondary cores do
//! not enter the ELF entry point**: only first_cpu is sent to the entry by
//! `cpu_set_pc`; the rest stay held in reset and are delivered by the kernel
//! through QEMU's built-in PSCI CPU_ON (conduit per DTB `/psci/method`, hvc on
//! the direct-kernel chain) — the gate reading decides it:
//! `test-smp-aps-aarch64` reports `ap-arrived=1 / ap-timeout=0` with `AP_GO`
//! never written (`2026-10-18` baseline, the proof that the delivery channel is
//! PSCI). Therefore:
//!
//! - The BSP is recorded by `_start`'s early election cell (MPIDR+1 stored into
//!   [`BOOT_HART_ELECTED`], same cell and same name as the riscv half); losers
//!   park locally in wfi (the `AP_GO` parking mailbox was deleted: it presupposed
//!   "all CPUs enter the entry point", and the reading above disproves that
//!   premise).
//! - QEMU `arm_load_dtb` places the DTB at the RAM base (`info->dtb_start =
//!   loader_start`, anchored in hw/arm/boot.c's ELF arm) = the fixed value
//!   [`DTB_PA`].
//! - `x0` is 0 at the direct-kernel entry (QEMU passes no arguments); the
//!   boot-shim UEFI chain instead passes the physical address of the
//!   `BootHandoffV1` arena in `x0` — `rust_image_main` splits the two forms on
//!   `x0==0`.
//!
//! # Mirror relationship with the riscv bootface
//!
//! The flow is section-by-section isomorphic (DTB→memmap→BootFileTable→bump
//! pool→12 modules→KernelInfo→`minix_kernel::arch_boot`). This file is
//! self-contained (bootface.rs is gated behind riscv64 cfg and an aarch64 build
//! does not compile it); deduplication convergence after both gates are green is
//! left to a separate cleanup increment, reconciling line by line against the two
//! serial outputs.
//!
//! # String discipline (inherited from the riscv half)
//!
//! Under the MMU-off physical view only literals materialized directly at the
//! call site can be dereferenced; all output on this face goes through
//! fixed-size byte buffers plus `early_console::write_str`, leaving no
//! baked-pointer legs.

use core::slice;

use crate::bootface_common;
use minix_boot::{BootModule, DTB, KernelInfo, MemoryRegion, NR_BOOT_MODULES, PlatformDescSource};
use minix_types::{PhysBytes, VirBytes};

use boot_shim::build_kernel_info;
use boot_shim::opensbi_helpers::BootFileTable;

use minix_plat::arm64::early_console;

// ── Layout contract constants (same source as aarch64.ld:
//    KERNEL_VIRT_BASE=0xFFFF800000000000, KERNEL_PHYS_BASE=0x40200000;
//    QEMU virt DRAM base 0x40000000) ──
//
// This group is architectural truth originating separately from the riscv half;
// each half keeps its own copy with cross-references. The name pool and pure
// function gates shared by both live in `bootface_common`.

/// QEMU virt (aarch64) DRAM base address.
const DRAM_BASE: u64 = 0x4000_0000;

/// Image physical base (`aarch64.ld` `KERNEL_PHYS_BASE`: DRAM+2MiB,
/// 2MiB-aligned).
const KERN_PHYS_BASE: u64 = 0x4020_0000;

/// Image link-time high-half base (`aarch64.ld` `KERNEL_VIRT_BASE`).
const KERN_VIRT_BASE: u64 = 0xFFFF_8000_0000_0000;

/// Image span (LMA tail difference after the last segment is clamped; the riscv
/// half uses the same 4 MiB).
const KERN_SIZE: u64 = 0x40_0000;

/// Where QEMU `arm_load_dtb` places the DTB for an ELF `-kernel` = the RAM base
/// (`info->dtb_start = info->loader_start`, hw/arm/boot.c ELF arm).
const DTB_PA: u64 = DRAM_BASE;

/// Bump physical pool base (DRAM+64MiB; clear of the 4MiB image and the DTB
/// page; the 32MiB extent matches the riscv half — the root page, the page-table
/// bump area, the name pool and the 12 module pages all come from here).
const BUMP_BASE: u64 = DRAM_BASE + 0x0400_0000;

/// Bump pool width 32MiB (same width as the riscv half).
const BUMP_SIZE: u64 = 0x0200_0000;

/// Number of pages in the page-table bump area (1 MiB, 256 pages; same width as
/// the riscv half — VMSv9 four-level and Sv39 mid-table usage is the same order
/// of magnitude, and pool exhaustion takes an explicit halt arm).
const PT_BUMP_PAGES: usize = 256;

// The name pool/slot width and the pure function gates (`be_cell`/
// `in_memmap*`/`table_magic_ok`/`fdt_table_pa`) are shared by both self-boot
// faces and live in `bootface_common`.
use crate::bootface_common::{fdt_table_pa, in_memmap_range, table_magic_ok};

// ── bump pool (aarch64's own cursor; opensbi_helpers' pool base is riscv DRAM) ──

/// Monotonic cursor (.bss; same discipline as opensbi_helpers: pool base, pool
/// end and cursor exist exactly once).
static mut BUMP_PTR: u64 = BUMP_BASE;

/// Allocate `num_pages` pages; a zero-page request is refused (it would hand out
/// duplicate addresses and break the non-overlap invariant).
fn bump_alloc_pages(num_pages: usize) -> Option<u64> {
    assert!(num_pages > 0, "bump_alloc: num_pages must be > 0");
    // SAFETY: single-threaded boot face (secondary cores stop at the kernel
    // rendezvous after PSCI CPU_ON delivery and never enter this face; even a
    // stray loser parks locally in wfi).
    unsafe {
        let need = (num_pages as u64) * 4096;
        let ptr = core::ptr::addr_of_mut!(BUMP_PTR);
        if *ptr + need > BUMP_BASE + BUMP_SIZE {
            return None;
        }
        let addr = *ptr;
        *ptr += need;
        Some(addr)
    }
}

// ── boot-time static slots (the boot face writes them exclusively, then read-only) ──

/// Unified landing storage (DESIGN-09): modules, platform source, parameter
/// buffer and startup records. One `static mut` plus one one-shot guard
/// replaces four separate landing areas; `new()` is not all-zero, so this
/// lands in `.data` rather than zero-fill `.bss`. The size bound is asserted
/// in `bootface_common`.

// ── Entry ──────────────────────────────────────────────────────

/// aarch64 self-boot face entry (the aarch64 `_start` in `main.rs` zeroes
/// .bss, establishes the stack, branches on `x0==0`, then `bl`s into here).
pub extern "C" fn bootface_a64() -> ! {
    // One-shot landing storage (same contract as the riscv half): every
    // `'static` slice handed to `KernelInfo` points into this storage, and a
    // second entry would overwrite them (UB under Rust aliasing).
    static STORAGE: boot_shim::BootStorageCell = boot_shim::BootStorageCell::new();
    let storage = STORAGE.take();

    core::hint::black_box(crate::KERNEL_ENTRY_ANCHOR as usize);
    say("### minix-rs kernel image: aarch64 self-boot (NK4-C sec-406)\n");

    // 1. DTB → memmap. QEMU `arm_load_dtb` puts the DTB at the RAM base
    //    (anchored in the source, see the module header); the fixed-size
    //    40-byte header yields totalsize, and only after `dtb_bounded` proves
    //    the whole block lies inside the DRAM shape do we build the window
    //    (the same out-of-bounds guard as the riscv half).
    // SAFETY: DTB_PA is written by the QEMU loader and is stable before this
    // face runs; the 40-byte read takes only the magic/length, and the real
    // extent is pinned down by the `dtb_bounded` bound gate.
    if !minix_boot::dtb_blob_aligned(DTB_PA) {
        say("### minix-rs kernel image: DTB pointer not 8-byte aligned — halting\n");
        crate::halt();
    }
    let head = unsafe { slice::from_raw_parts(DTB_PA as *const u8, 40) };
    if u32::from_be_bytes(head[0..4].try_into().unwrap_or([0; 4])) != 0xd00d_feed {
        say("### minix-rs kernel image: DTB magic bad — halting\n");
        crate::halt();
    }
    let fdt_total = u32::from_be_bytes(head[4..8].try_into().unwrap_or([0; 4])) as usize;
    if fdt_total < 40
        || !bootface_common::dtb_bounded(DTB_PA, fdt_total, DRAM_BASE, KERN_PHYS_BASE, KERN_SIZE)
    {
        say("### minix-rs kernel image: DTB blob escapes DRAM bound — halting\n");
        crate::halt();
    }
    // SAFETY: `dtb_bounded` has proven the whole block lies inside the DRAM
    // shape; the MMU-off identity view can read it.
    let fdt_bytes = unsafe { slice::from_raw_parts(DTB_PA as *const u8, fdt_total) };
    if minix_boot::fdt_layout::validate(DTB_PA, fdt_bytes).is_err() {
        say("### minix-rs kernel image: DTB container invalid — halting\n");
        crate::halt();
    }
    let fdt = match fdt::Fdt::new(fdt_bytes) {
        Ok(f) => f,
        Err(_) => {
            say("### minix-rs kernel image: DTB parse failed — halting\n");
            crate::halt();
        }
    };

    // The memmap source is unified on minix-boot's `DtbMemoryMap` (the same
    // impl as the opensbi leg; this closes the fourth link of the acquisition
    // lane). `fw_end = 0` is a premise of this form: the DRAM of the aarch64
    // direct-kernel chain has no resident firmware area (QEMU places only the
    // DTB and the image), so nothing is trimmed here; the DTB itself is
    // No longer subtracted by the kernel: Phase A.2 disappeared together with
    // FREE_MEMMAP. The pages stay outside every boot-time exclusion by the
    // ordering contract — platform discovery consumes the blob in Phase A.5,
    // before any allocator can reach conventional memory. A future consumer
    // of the raw blob would have to copy it or register it as a live object.
    let (memmap, reserved, sources) = minix_boot::DtbMemoryMap::new(&fdt, 0)
        .collect_with_sources()
        .expect("bootface: DTB memmap rejected (capacity or reservation shape) — halting");
    // Source records are held, not discarded: a projected span with no record
    // behind it must refuse to boot (same discipline as the riscv bootface).
    minix_boot::audit_source_coverage(memmap, reserved, sources)
        .expect("bootface: a projected span has no source record — halting");
    if memmap.is_empty() {
        say("### minix-rs kernel image: no usable /memory reg in DTB — halting\n");
        crate::halt();
    }
    say("  memmap regions 0x");
    say_hex(memmap.len() as u64);
    say(" first 0x");
    say_hex(memmap[0].base.0);
    say("+0x");
    say_hex(memmap[0].len as u64);
    say("\n");

    // 2. BootFileTable: the DTB /chosen channel; pass the DRAM bound gate and
    //    then the magic gate before dereferencing (the same discipline as the
    //    riscv half).
    let table_pa =
        fdt_table_pa(&fdt).filter(|pa| in_memmap_range(memmap, *pa, core::mem::size_of::<BootFileTable>() as u64) && table_magic_ok(*pa));
    let Some(table_pa) = table_pa else {
        say("### minix-rs kernel image: no BootFileTable (chosen) — halting\n");
        crate::halt();
    };
    say("  boot file table pa 0x");
    say_hex(table_pa);
    say("\n");
    // SAFETY: `table_magic_ok` has checked the magic inside the memmap bound;
    // the gate assembly tool places the table in physical RAM that boot owns
    // exclusively, and this face is its first and only reader.
    let table = unsafe { &*(table_pa as *const BootFileTable) };

    // 3. Bump physical pool: root page → page-table bump area → name pool
    //    (sequential and non-overlapping; Option plus an honest halt, with no
    //    expect/panic leg — the same shape as the riscv half).
    let pool_exhaust = "### minix-rs kernel image: bump pool exhausted — halting\n";
    let Some(cur_root) = bump_alloc_pages(1) else {
        say(pool_exhaust);
        crate::halt()
    };
    let root_page = PhysBytes(cur_root);
    let Some(pt_base) = bump_alloc_pages(PT_BUMP_PAGES) else {
        say(pool_exhaust);
        crate::halt()
    };
    let pt_end = pt_base + (PT_BUMP_PAGES as u64) * 4096;
    let Some(names_pool) = bump_alloc_pages(1) else {
        say(pool_exhaust);
        crate::halt()
    };
    minix_kernel::boot_alloc::init_boot_pt_alloc(pt_base, pt_end);
    // Registration happens in `boot_validate_and_prepare` under its
    // `is_registered()` guard, before the first page-table allocation; a
    // second registration here would hide ordering errors (DESIGN-09).
    say("  root 0x");
    say_hex(root_page.0);
    say(" pt bump 0x");
    say_hex(pt_base);
    say("..0x");
    say_hex(pt_end);
    say("\n");

    // 4. The 12 modules: locate the table (name alignment check) → copy the
    //    source straight into bump pages.
    storage.fill_modules(|module_table| {
    if bootface_common::place_modules(
        table,
        memmap,
        bootface_common::PlacementPool {
            names: names_pool,
            bump_start: BUMP_BASE,
            bump_end: BUMP_BASE + BUMP_SIZE,
        },
        module_table,
        bump_alloc_pages,
        say,
        say_hex,
    )
    .is_none()
    {
        say("### minix-rs kernel image: module placement failed — halting\n");
        crate::halt();
    }
    NR_BOOT_MODULES
    });
    say("  boot modules placed 12\n");

    // 5. Platform descriptor source: DTB as the sole source (the slot lives
    //    in .data).
    // SAFETY: single hart on the boot face; written once, read-only afterwards.
    // SAFETY: the DTB is still in the firmware/identity mapping; this face
    // runs before paging is enabled.
    let dtb_len = unsafe { minix_boot::dtb_total_size(PhysBytes(DTB_PA)) };
    storage.fill_platform(|slots| {
        slots[0] = PlatformDescSource::with_optional_len(DTB, PhysBytes(DTB_PA), dtb_len);
        1
    });

    // T3: DTB `/chosen/bootargs` is the direct-boot parameter source
    // (QEMU `-append` fills it).
    // `fill_param` commits the byte count; the pointer for the later
    // `build_kernel_info` and the diagnostic text are captured inside.
    let mut param_ptr = core::ptr::null::<u8>();
    storage.fill_param(|out| {
        let input = crate::bootface_common::fdt_bootargs(&fdt).unwrap_or("");
        let stats = minix_boot::normalize_param_buf(input.as_bytes(), None, out);
        say("  param_buf bytes=");
        say_hex(stats.bytes as u64);
        say(" rejected=");
        say_hex(stats.rejected as u64);
        say(" dropped=");
        say_hex(stats.dropped as u64);
        if stats.bytes > 0 {
            let end = out[..stats.bytes]
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(stats.bytes);
            if let Ok(text) = core::str::from_utf8(&out[..end]) {
                say(" first=");
                say(text);
            }
        }
        say("\n");
        param_ptr = out.as_ptr();
        stats.bytes
    });

    // 6. KernelInfo plus the real arch_boot gate (validate → build the root
    //    → DM → jump_to_kmain; it never returns to this face).
    //
    // Same handover as the riscv half: the collector's records back the
    // projected map, and the consumer boundary re-audits the pairing.
    assert!(
        !sources.is_empty(),
        "bootface: DTB legs must carry source records, not an empty partition"
    );
    // v10 (D-062) removed the startup-object set that used to be pushed here:
    // its single reader (a strict-gate coverage check) is now answered by the
    // map tables, and the kernel span is already in `memmap` on this leg.

    // Capture the live prefixes, then end the only `&mut` to the one-shot
    // storage. Creating the slices after every write is what keeps the
    // `&'static` views from aliasing a later `&mut` (DESIGN-D2-13).
    let modules_ptr = storage.boot_modules().as_ptr();
    let modules_len = storage.boot_modules().len();
    let platform_ptr = storage.platform_sources().as_ptr();
    let platform_len = storage.platform_sources().len();
    let _ = storage;

    // SAFETY: `BOOT_STORAGE` is a one-shot static whose writer has finished;
    // no mutation follows, and the pointers cover the committed prefixes.
    let modules: &'static [BootModule] =
        unsafe { core::slice::from_raw_parts(modules_ptr, modules_len) };
    let platform_sources: &'static [PlatformDescSource] =
        unsafe { core::slice::from_raw_parts(platform_ptr, platform_len) };
    let param_buf: &'static [u8] =
        unsafe { core::slice::from_raw_parts(param_ptr, storage.param_buf().len()) };
    let kernel_info: KernelInfo = build_kernel_info(
        memmap,
        reserved,
        VirBytes(KERN_VIRT_BASE),
        PhysBytes(KERN_PHYS_BASE),
        KERN_SIZE,
        modules,
        platform_sources,
        param_buf,
        sources,
        minix_boot::InputObjectId::DtbBootInput,
    );
    kernel_info.validate_requiring_sources();
    say("  calling arch_boot (validate gate)\n");
    minix_kernel::arch_boot(&kernel_info, root_page)
}

// ── DTB face ───────────────────────────────────────────────────
// ── Serial face (no heap: fixed-size strings plus hand-rolled hex; PL011
// identity PA path) ──

fn say(msg: &str) {
    early_console::write_str(msg);
}

fn say_hex(v: u64) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        buf[i] = HEX[((v >> ((15 - i) * 4)) & 0xf) as usize];
        i += 1;
    }
    early_console::write_str(core::str::from_utf8(&buf).unwrap_or(""));
}
