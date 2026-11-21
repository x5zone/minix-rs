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
//! - The direct-kernel and UEFI-receive forms are separate build products
//!   (D-061): the direct product's entry ignores `x0` and calls this face;
//!   the UEFI-receive product passes the physical address of the
//!   `BootHandoffV1` arena in `x0` and treats `x0 == 0` as an honest halt,
//!   not as a form switch.
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

use core::sync::atomic::{AtomicU64, Ordering};

use crate::bootface_common;
use minix_boot::BumpRegion;

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

// The name pool, the pure-function gates and the shared boot flow live in
// `bootface_common`.

// ── bump pool (aarch64's own cursor over its own fixed region) ──

/// Monotonic cursor over `[BUMP_BASE, BUMP_BASE + BUMP_SIZE)`. The region
/// base/end are aarch64 board constants; the arithmetic is the same
/// `minix_boot::BumpRegion` the riscv half and the kernel-side allocator use
/// (single source for the bounds check, `used_bytes` and the consistency
/// predicate). The atomic carries the cursor without a `static mut`
/// reference under edition 2024; boot is single-threaded.
static BUMP_NEXT: AtomicU64 = AtomicU64::new(BUMP_BASE);

const _: () = assert!(BUMP_BASE % minix_boot::BUMP_PAGE_SIZE == 0);
const _: () = assert!(BUMP_SIZE % minix_boot::BUMP_PAGE_SIZE == 0);
// The module pool must start after the 4 MiB kernel image; otherwise the
// first module take lands inside the running image. The runtime check in
// `place_modules` is the second gate.
const _: () = assert!(KERN_PHYS_BASE + KERN_SIZE <= BUMP_BASE);

/// Allocate `num_pages` pages; a zero-page request is refused by
/// `BumpRegion::alloc_pages` (it would hand out duplicate addresses and break
/// the non-overlap invariant).
fn bump_alloc_pages(num_pages: usize) -> Option<u64> {
    let mut region = BumpRegion::resumed(
        BUMP_BASE,
        BUMP_NEXT.load(Ordering::Relaxed),
        BUMP_BASE + BUMP_SIZE,
    );
    let addr = region.alloc_pages(num_pages)?;
    BUMP_NEXT.store(region.next(), Ordering::Relaxed);
    Some(addr)
}

// ── boot-time static slots (the boot face writes them exclusively, then read-only) ──

/// Unified landing storage (DESIGN-09): modules, platform source, parameter
/// buffer, memory tables and source records. One `static mut` plus one one-shot guard
/// replaces four separate landing areas; `new()` is not all-zero, so this
/// lands in `.data` rather than zero-fill `.bss`. The size bound is asserted
/// in `bootface_common`.

// ── Entry ──────────────────────────────────────────────────────

/// aarch64 self-boot face entry (the aarch64 `_start` in `main.rs` zeroes
/// .bss, establishes the stack, selects the form at build time (D-061),
/// then `bl`s into here).
pub extern "C" fn bootface_a64() -> ! {
    say("### minix-rs kernel image: aarch64 self-boot (NK4-C sec-406)\n");

    bootface_common::run_self_boot(
        bootface_common::SelfBootParams {
            dtb_pa: DTB_PA,
            dram_base: DRAM_BASE,
            fw_end: 0,
            kern_phys_base: KERN_PHYS_BASE,
            kern_virt_base: KERN_VIRT_BASE,
            kern_size: KERN_SIZE,
            bump_start: BUMP_BASE,
            bump_end: BUMP_BASE + BUMP_SIZE,
        },
        bump_alloc_pages,
        say,
        say_hex,
    )
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
