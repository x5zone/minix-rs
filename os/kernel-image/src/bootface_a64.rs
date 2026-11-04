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

use minix_boot::{
    BootModule, DTB, KernelInfo, MemoryRegion, NR_BOOT_MODULES,
    PlatformDescSource,
};
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
use crate::bootface_common::{
    NAME_POOL, NAME_SLOT, fdt_table_pa, in_memmap, in_memmap_range, table_magic_ok,
};

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

// ── .bss assembly slots (the boot face writes them exclusively, then read-only) ──

/// Platform descriptor source slot (DTB is the sole source).
static mut PLATFORM_SOURCE: PlatformDescSource = PlatformDescSource::new(DTB, PhysBytes(0));

/// Landing descriptor table for the 12 modules.
static mut MODULE_TABLE: [BootModule; NR_BOOT_MODULES] = [const {
    BootModule { name: "", start: PhysBytes(0), len: 0 }
}; NR_BOOT_MODULES];

// ── Entry ──────────────────────────────────────────────────────

/// aarch64 self-boot face entry (the aarch64 `_start` in `main.rs` zeroes
/// .bss, establishes the stack, branches on `x0==0`, then `bl`s into here).
pub extern "C" fn bootface_a64() -> ! {
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
    let head = unsafe { slice::from_raw_parts(DTB_PA as *const u8, 40) };
    if u32::from_be_bytes(head[0..4].try_into().unwrap_or([0; 4])) != 0xd00d_feed {
        say("### minix-rs kernel image: DTB magic bad — halting\n");
        crate::halt();
    }
    let fdt_total = u32::from_be_bytes(head[4..8].try_into().unwrap_or([0; 4])) as usize;
    if fdt_total < 40 || !dtb_bounded(DTB_PA, fdt_total) {
        say("### minix-rs kernel image: DTB blob escapes DRAM bound — halting\n");
        crate::halt();
    }
    // SAFETY: `dtb_bounded` has proven the whole block lies inside the DRAM
    // shape; the MMU-off identity view can read it.
    let fdt_bytes = unsafe { slice::from_raw_parts(DTB_PA as *const u8, fdt_total) };
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
    // permanently subtracted by kernel phase A.2 using the real totalsize —
    // the two mechanisms do not substitute for each other: this site only
    // produces the semantic list, the subtraction is kernel accounting.
    let (memmap, reserved) = minix_boot::DtbMemoryMap::new(&fdt, 0)
        .collect()
        .expect("bootface: DTB memmap rejected (capacity or reservation shape) — halting");
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
    let table_pa = fdt_table_pa(&fdt).filter(|pa| in_memmap(memmap, *pa) && table_magic_ok(*pa));
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
    let Some(cur_root) = bump_alloc_pages(1) else { say(pool_exhaust); crate::halt() };
    let root_page = PhysBytes(cur_root);
    let Some(pt_base) = bump_alloc_pages(PT_BUMP_PAGES) else { say(pool_exhaust); crate::halt() };
    let pt_end = pt_base + (PT_BUMP_PAGES as u64) * 4096;
    let Some(names_pool) = bump_alloc_pages(1) else { say(pool_exhaust); crate::halt() };
    minix_kernel::boot_alloc::init_boot_pt_alloc(pt_base, pt_end);
    minix_arch::pt_alloc::register(minix_kernel::boot_alloc::boot_pt_alloc);
    say("  root 0x");
    say_hex(root_page.0);
    say(" pt bump 0x");
    say_hex(pt_base);
    say("..0x");
    say_hex(pt_end);
    say("\n");

    // 4. The 12 modules: locate the table (name alignment check) → copy the
    //    source straight into bump pages.
    if place_modules(table, memmap, names_pool).is_none() {
        say("### minix-rs kernel image: module placement failed — halting\n");
        crate::halt();
    }
    // SAFETY: `place_modules` has just filled all 12 slots in contract order;
    // read-only from here on.
    let modules: &'static [BootModule] = unsafe { &*core::ptr::addr_of!(MODULE_TABLE) };
    say("  boot modules placed 12\n");

    // 5. Platform descriptor source: DTB as the sole source (the slot lives
    //    in .bss).
    // SAFETY: single hart on the boot face; written once, read-only afterwards.
    unsafe {
        *core::ptr::addr_of_mut!(PLATFORM_SOURCE) =
            PlatformDescSource::new(DTB, PhysBytes(DTB_PA));
    }
    // SAFETY: as above — the static reference is taken after the write; the
    // single array cell and the length-1 slice have the same origin.
    let platform_sources: &'static [PlatformDescSource] = unsafe {
        core::slice::from_raw_parts(core::ptr::addr_of!(PLATFORM_SOURCE), 1)
    };

    // T3: DTB `/chosen/bootargs` is the direct-boot parameter source
    // (QEMU `-append` fills it).
    let param_buf: &'static [u8] = {
        let out = crate::bootface_common::param_landing();
        let input = crate::bootface_common::fdt_bootargs(&fdt).unwrap_or("");
        let stats = minix_boot::normalize_param_buf(input.as_bytes(), None, out);
        say("  param_buf bytes=");
        say_hex(stats.bytes as u64);
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
        let ptr = out.as_ptr();
        // SAFETY: static landing pad; the slice covers the normalized prefix.
        unsafe { core::slice::from_raw_parts(ptr, stats.bytes) }
    };

    // 6. KernelInfo plus the real arch_boot gate (validate → build the root
    //    → DM → jump_to_kmain; it never returns to this face).
    let kernel_info: KernelInfo = build_kernel_info(
        memmap,
        reserved,
        VirBytes(KERN_VIRT_BASE),
        PhysBytes(KERN_PHYS_BASE),
        KERN_SIZE,
        modules,
        PhysBytes(0),
        0,
        platform_sources,
        param_buf,
    );
    say("  calling arch_boot (validate gate)\n");
    minix_kernel::arch_boot(&kernel_info, root_page)
}

// ── DTB face ───────────────────────────────────────────────────

/// Read `/chosen`'s `opensbi,boot-file-table` (big-endian cells; both 4- and
/// 8-byte forms are accepted; the property name follows the same convention as
/// the riscv gate assembly tool, whose gate script writes it with the same
/// `fdtput` shape).
///
/// Lookup goes through `all_nodes` rather than `find_node` (a settled finding:
/// fdt 0.1.5's `find_node` returns None for the blob QEMU rebuilds for a
/// `-kernel` direct-kernel chain — `all_nodes` and a host-side python parse both
/// see /chosen, while `find_node` does not; names are matched with the `@`
/// suffix stripped, consistent with `find_node`'s lenient semantics).
/// DTB block bound gate: totalsize is self-described, but the whole block must
/// lie inside the DRAM shape.
fn dtb_bounded(dtb_pa: u64, total: usize) -> bool {
    dtb_pa >= DRAM_BASE && dtb_pa + total as u64 <= 0x1_0000_0000
}

/// Landing descriptor table for the 12 modules (`.bss`, read-only once
/// assembled).
///
/// Position is primary and the name is secondary / the source range must clear
/// three gates / names go into the pool — the semantics match the riscv half's
/// `place_modules` item by item (straight copy, full-width and alignment gates)
/// and are not repeated here; the only difference is the bump pool base (the
/// aarch64 form).
fn place_modules(
    table: &BootFileTable,
    memmap: &[MemoryRegion],
    names_pool: u64,
) -> Option<()> {
    let entries = table.valid_entries();
    if entries.len() < NR_BOOT_MODULES {
        say("  table holds 0x");
        say_hex(entries.len() as u64);
        say(" entries, kernel contract needs 0x");
        say_hex(NR_BOOT_MODULES as u64);
        say("\n");
        return None;
    }
    for i in 0..NR_BOOT_MODULES {
        let entry = &entries[i];
        let expect = &NAME_POOL[i];
        let want_n = expect.iter().position(|&b| b == 0).unwrap_or(NAME_SLOT);
        let base = entry
            .path
            .iter()
            .rposition(|&b| b == b'/')
            .map_or(0, |p| p + 1);
        let got = &entry.path[base..];
        let got_n = got.iter().position(|&b| b == 0).unwrap_or(got.len());
        if got_n != want_n || got[..want_n] != expect[..want_n] {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" path basename mismatches contract order\n");
            return None;
        }
        let len = entry.len as usize;
        if len == 0 {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" length 0\n");
            return None;
        }
        if !in_memmap_range(memmap, entry.phys_addr, entry.len) {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" source 0x");
            say_hex(entry.phys_addr);
            say("+0x");
            say_hex(entry.len);
            say(" escapes memmap\n");
            return None;
        }
        if entry.phys_addr < BUMP_BASE + BUMP_SIZE && entry.phys_addr + entry.len >= BUMP_BASE {
            say("  module slot 0x");
            say_hex(i as u64);
            say(" source overlaps bump pool\n");
            return None;
        }
        let pages = len.div_ceil(4096);
        let dest = bump_alloc_pages(pages)?;
        // SAFETY: the source just passed the full-width memmap gate and does
        // not overlap the bump pool; dest = exclusive bump pages (monotonic
        // cursor), and the copy length is ≤ the length of both intervals.
        unsafe {
            core::ptr::copy_nonoverlapping(entry.phys_addr as *const u8, dest as *mut u8, len);
            core::ptr::write_bytes((dest as *mut u8).add(len), 0, pages * 4096 - len);
        }
        let slot = names_pool + (i as u64) * NAME_SLOT as u64;
        // SAFETY: source = the NAME_POOL byte cells (materialized at the base
        // call site); destination = name-pool slot i (i < 12 ⇒ offset < one
        // page).
        unsafe {
            core::ptr::copy_nonoverlapping(NAME_POOL[i].as_ptr(), slot as *mut u8, NAME_SLOT);
        }
        let n = NAME_POOL[i].iter().position(|&b| b == 0).unwrap_or(NAME_SLOT);
        let landed: &'static str = unsafe {
            // SAFETY: the bytes come from the ASCII literal pool; the backing
            // is an exclusive physical page.
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(slot as *const u8, n))
        };
        // SAFETY: single hart on the boot face; each slot is written once and
        // then read-only.
        unsafe {
            MODULE_TABLE[i] = BootModule {
                name: landed,
                start: PhysBytes(dest),
                len,
            };
        }
    }
    Some(())
}

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
