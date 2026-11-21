//! riscv64 install face (self-boot form): kernel-image acts as its own bootloader.
//!
//! `[ARCH: FW_PAYLOAD direct-kernel form (no Minix3 counterpart)]`: this face is
//! the riscv64 auxiliary boot path — shared by the minimal `FW_PAYLOAD` install
//! on real boards and by QEMU `-kernel` tests; the mainline remains a separate
//! boot-shim plus a separate kernel image. The three-architecture placement
//! matrix is in doc 01 §3.7.
//!
//! Decision: kernel-image on riscv64 takes the `a1` DTB → parses memmap +
//! module load sources + zeroes `.bss` → calls arch_boot (through the real
//! validate gate). This module is the Rust landing of that decision (`.bss`
//! zeroing happens in `main.rs` entry asm; the other two of the three missing
//! pieces are here):
//!
//! 1. **memmap**: the DTB physical address OpenSBI passes in `a1`; parse the
//!    `/memory` node's `reg` — sharing `minix_boot::DtbMemoryMap` with the
//!    opensbi leg (the DTB collector lives in `minix-boot`; the three
//!    boot-source chains converge there, and the boot-form layer is not
//!    merged).
//! 2. **module load source**: `BootFileTable` (gated by magic `MNXBOOT1`). Two
//!    channels: a spare `a2` argument (no in-repo producer — QEMU/OpenSBI
//!    only promise `a0`/`a1`; the six real-machine scripts all inject the
//!    table through the DTB; the U-Boot shim leg passes `(extra_argc,
//!    extra_argv)` in `a0`/`a1` and is a different entry), and the DTB
//!    `/chosen` `opensbi,boot-file-table` property (the exercised path; it
//!    also lets OpenSBI `-kernel` direct load plus `-device loader` injection
//!    self-test the whole chain, without waiting for the xtask install leg).
//!    Entries carry the physical address/length of the staged file bytes; this
//!    face copies module bytes into bump physical pages in contract order
//!    (`MODULE_NAMES` = the 12 names in C `table.c:44-64`; position is
//!    primary, checking the in-table name against the expected literal is
//!    secondary).
//! 3. **real arch_boot gate**: build `KernelInfo` (reusing the boot-shim
//!    library's `build_kernel_info` lane: Sv39 `user_sp`; the old bootstrap
//!    reclaim interval was deleted with D01-F03), then call
//!    `minix_kernel::arch_boot` — which
//!    runs through `validate()` and `jump_to_kmain`; that is the half none of
//!    the three in-repo carriers (hello-boot / test-rt-birth etc.) ever
//!    exercised.
//!
//! Execution-view invariant (pinned by real-hardware measurement): under
//! riscv64 medany all intra-image references are relaxed to PC-relative, and
//! OpenSBI jumps to the **lowest physical load address** (`Domain0 Next
//! Address` = lowest PT_LOAD `p_paddr`), so this module runs in a translation
//! view where "link-time VMA − runtime PA = 0xFFFFFFC000000000 − 0x80200000":
//! every address value Rust obtains is a physical address (accessible
//! identically with satp=0), and DTB, table and module bytes are read directly
//! through physical pointers; `kern_virt_base` / `kern_stack_top` in
//! `KernelInfo` use the `.ld` constants (high-half view values) and are
//! dereferenced only after `arch_boot` has built the tables and called
//! `enable()` — the ordering is guaranteed by `arch_boot` itself.
//!
//! Boundary with boot-shim `opensbi_helpers::prepare_boot`: that leg is
//! "standalone shim installs kernel.elf then jumps", which re-copies **this
//! image's own segments** and clears NOBITS — self-destructive for a self-boot
//! image; this face copies only **modules** and never re-copies itself. Module
//! copies go through physical bump scratch pages rather than a heap `Vec` (the
//! heap has a single `IMAGE_HEAP` block of 1 MiB, while the staged file bytes
//! are already in RAM — reading and copying directly saves a layer and is not
//! bound by heap width).

use crate::bootface_common;
use minix_boot::NR_BOOT_MODULES;

use boot_shim::opensbi_helpers::{bump_alloc_pages, bump_end_bound, bump_pool_base};

use crate::early_console;

// String discipline for this self-boot form (pinned by real-hardware forensics
// under satp=0): only literals materialized directly at the call site (`auipc`
// relaxation) are dereferenceable; any `&str` that goes through a data cell —
// static fat pointers, match jump tables (`.Lswitch.table` carries baked VMAs,
// confirmed by disassembly) — dereferences as a Load access fault → OpenSBI
// warm re-entry. The name list therefore exists as a fixed-size byte pool
// (pure data, no baked pointers) plus literal-check shapes; the backing of
// `BootModule.name` must be the physical name pool (the kernel's
// `store_kernel_info` reads it on the physical-address leg, os/kernel/src/lib.rs:fn store_kernel_info).
const _: () = assert!(NR_BOOT_MODULES == 12);

// The name pool, the pure-function gates and the shared boot flow live in
// `bootface_common`.

// ── Layout contract constants (same source as riscv64.ld; check-layout.sh
//    L0/L9/A1 reconcile them with the real board's `Domain0 Next Address`
//    to prevent drift) ──
//
// This group holds architecture truth (DRAM base / image physical base / pool
// base) and does not share a source with the aarch64 half, so each keeps its
// own copy and they point at each other instead of sharing; the name pool and
// pure-function gates shared by both live in `bootface_common`.

/// QEMU virt DRAM base (lower bound of the DTB/table bound gates; same value
/// as the `riscv64.ld` header note, and the base `opensbi_helpers`'s
/// `MODULE_REGION_BASE` (0x8000_0000 + 32 MB) is derived from; L0/A1
/// already protect the base from drifting).
const DRAM_BASE: u64 = 0x8000_0000;

/// Image physical base (`riscv64.ld` `KERNEL_PHYS_BASE`: the loader places by
/// PT_LOAD LMA; the span is rounded to 2MiB by the same script).
const KERN_PHYS_BASE: u64 = 0x8020_0000;

/// Image link-time high-half base (`riscv64.ld` `KERNEL_VIRT_BASE`).
const KERN_VIRT_BASE: u64 = 0xFFFF_FFC0_0000_0000;

/// Image span (= tail LMA delta after the 2MiB rounding of the last segment;
/// currently 4 MiB). Hard-coding the 2MiB copy once let an `IMAGE_HEAP` growth
/// push the real span to 4MiB — the high half then mapped only half the image
/// and `kern_stack_top` landed mid-.bss (silent corruption). This literal is
/// the single source of truth; `check-layout.sh` L9 reconciles the measured ELF
/// span against it (heap width change → span change → L9 goes red on the spot);
/// no separate symbol section in `.ld` (tried before: one extra PT_LOAD pushed
/// the span off a 2MiB multiple, breaking L5/L7 instead).
const KERN_SIZE: u64 = 0x40_0000;

/// The boot scratch page budget is gone: the 1 MiB scratch and the module size
/// cap of the early two-stage copy were replaced by "copy directly from source
/// + full-width `in_memmap_range` gate"; see `place_modules`.

// ── Entry ───────────────────────────────────────────────────────

/// Rust-side entry of the self-boot form (`main.rs` riscv64 `_start` zeroes
/// .bss, sets up the stack and `call`s in; a0/a1 become the two parameters per
/// the RV64 ABI, and `call` does not touch them). The spare `a2` channel was
/// retired: no in-repo producer ever set it.
pub extern "C" fn bootface(boot_hart: u64, dtb_pa: u64) -> ! {
    say("### minix-rs kernel image: riscv64 self-boot (NK4-C jia-an)\n");
    say("  boot hart 0x");
    say_hex(boot_hart);
    say(" dtb 0x");
    say_hex(dtb_pa);
    say("\n");

    if dtb_pa == 0 {
        say("### minix-rs kernel image: OpenSBI passed no DTB in a1 — halting\n");
        crate::halt();
    }

    bootface_common::run_self_boot(
        bootface_common::SelfBootParams {
            dtb_pa,
            dram_base: DRAM_BASE,
            fw_end: KERN_PHYS_BASE,
            kern_phys_base: KERN_PHYS_BASE,
            kern_virt_base: KERN_VIRT_BASE,
            kern_size: KERN_SIZE,
            bump_start: bump_pool_base(),
            bump_end: bump_end_bound(),
        },
        bump_alloc_pages,
        say,
        say_hex,
    )
}

// ── DTB face ────────────────────────────────────────────────────

// (fill_memmap has moved to minix-boot `DtbMemoryMap`; the bound-clipping pure
// functions moved in the same batch as the parameterized `clip_reserved`, with
// their compile-time guards.)

// (clip_firmware has moved to minix-boot's parameterized `clip_reserved`, and
// its three compile-time guard assertions moved along (_FW_PURE/_FW_CROSS/
// pass-through unchanged).)
// ── BootFileTable consumption (kernel-image shape: sources copied straight
//    into physical bump pages, zero heap) ──

/// Unified landing storage for the direct-boot face (DESIGN-09): modules,
/// platform source, parameter buffer, memory tables and source records live here.
/// One `static mut` plus one one-shot guard replaces four separate landing
/// areas; `new()` is not all-zero, so this lands in `.data` rather than
/// zero-fill `.bss`. The size bound is asserted in `bootface_common`.
// ── Console face (no heap: fixed-length strings + hand-written hex) ─────────
//
// `say` / `say_hex` are character-for-character identical to the aarch64 half,
// but the console implementation differs per architecture (riscv uses
// `early_console` at the top of this file, aarch64 uses `minix_plat::arm64`),
// so they are not pushed down into `bootface_common`; the two point at each
// other, so keep them in sync when changing semantics.

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
    // Hex literals are always ASCII.
    early_console::write_str(core::str::from_utf8(&buf).unwrap_or(""));
}

// ── .bss assembly cells (exclusive writes on the boot face, read-only
//    afterwards) ──
