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

use core::slice;

use crate::bootface_common;
use minix_boot::{
    BootModule, DTB, DtbMemoryMap, KernelInfo, MemoryRegion, NR_BOOT_MODULES, PlatformDescSource,
};
use minix_types::{PhysBytes, VirBytes};

use boot_shim::build_kernel_info;
use boot_shim::opensbi_helpers::{BootFileTable, bump_alloc_pages, bump_end_bound, bump_pool_base};

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

// The name pool/slot width and the pure-function gates (`be_cell`/
// `in_memmap*`/`table_magic_ok`/`fdt_table_pa`) are shared by both self-boot
// faces and live in `bootface_common`.
use crate::bootface_common::{
    NAME_POOL, NAME_SLOT, fdt_table_pa, in_memmap_range, table_magic_ok,
};

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

/// Page-table bump region size: 1 MiB (256 pages, same width as the boot-shim
/// library fallback). Under QEMU virt's single-region memmap the intermediate
/// tables for Sv39 root + identity low half + high half + two DM windows
/// measured below this; pool exhaustion takes an explicit halt arm (no
/// `.expect` — the panic render path carries baked-VMA format pieces, which
/// under the physical address view hits exactly the invariant pinned in this
/// module).
const PT_BUMP_PAGES: usize = 256;

// ── Entry ───────────────────────────────────────────────────────

/// Rust-side entry of the self-boot form (`main.rs` riscv64 `_start` zeroes
/// .bss, sets up the stack and `call`s in; a0/a1 become the two parameters per
/// the RV64 ABI, and `call` does not touch them). The spare `a2` channel was
/// retired: no in-repo producer ever set it.
pub extern "C" fn bootface(boot_hart: u64, dtb_pa: u64) -> ! {
    // Touch the anchor: reference it once on the running path so the kernel
    // boot graph is pulled into the link at archive-member granularity (the
    // linker script KEEP is the second safeguard).
    core::hint::black_box(crate::KERNEL_ENTRY_ANCHOR as usize);
    say("### minix-rs kernel image: riscv64 self-boot (NK4-C jia-an)\n");
    say("  boot hart 0x");
    say_hex(boot_hart);
    say(" dtb 0x");
    say_hex(dtb_pa);
    say("\n");

    // 1. DTB → memmap (the OpenSBI spec guarantees a1 carries the DTB; zero
    //    means the firmware configuration is abnormal, so halt honestly).
    if dtb_pa == 0 {
        say("### minix-rs kernel image: OpenSBI passed no DTB in a1 — halting\n");
        crate::halt();
    }
    // The specification requires an 8-byte aligned blob; refuse the shape
    // before the first dereference rather than parsing it silently.
    if !minix_boot::dtb_blob_aligned(dtb_pa) {
        say("### minix-rs kernel image: DTB pointer not 8-byte aligned — halting\n");
        crate::halt();
    }
    // SAFETY: `dtb_pa` is the physical address of the FDT that OpenSBI places
    // in high DRAM (spec promise, confirmed on the firmware console as
    // `Domain0 Next Arg1`); it is readable in the satp=0 identity view. Do not
    // build a slice from an assumed 2MiB window (its tail would run past the
    // top of DRAM): read the fixed 40B header for totalsize first, and only
    // after `fdt_bounded` proves the whole blob lies inside memmap build a
    // slice of the real width.
    // SAFETY: 40B header — `dtb_pa` is non-zero and OpenSBI guarantees the
    // blob is in DRAM; reading the first 40B only takes the magic/length
    // fields; the real use range is pinned by the range gate below.
    let head = unsafe { slice::from_raw_parts(dtb_pa as *const u8, 40) };
    if u32::from_be_bytes(head[0..4].try_into().unwrap_or([0; 4])) != 0xd00d_feed {
        say("### minix-rs kernel image: DTB magic bad — halting\n");
        crate::halt();
    }
    let fdt_total = u32::from_be_bytes(head[4..8].try_into().unwrap_or([0; 4])) as usize;
    if fdt_total < 40
        || !bootface_common::dtb_bounded(dtb_pa, fdt_total, DRAM_BASE, KERN_PHYS_BASE, KERN_SIZE)
    {
        say("### minix-rs kernel image: DTB blob escapes DRAM bound — halting\n");
        crate::halt();
    }
    let fdt_bytes = unsafe { slice::from_raw_parts(dtb_pa as *const u8, fdt_total) };
    if minix_boot::fdt_layout::validate(dtb_pa, fdt_bytes).is_err() {
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

    // The memmap source goes through `DtbMemoryMap` (minix-boot); the firmware
    // reserved-region clipping semantics are unchanged (fw_end =
    // KERN_PHYS_BASE, a functional trim pinned on real hardware).
    // The source records are held, not discarded: `audit_source_coverage`
    // refuses to boot when a projected span has no record behind it.
    let (memmap, reserved, sources) = DtbMemoryMap::new(&fdt, KERN_PHYS_BASE)
        .collect_with_sources()
        .expect("bootface: DTB memmap rejected (capacity or reservation shape) — halting");
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

    // 2. BootFileTable: the DTB /chosen channel must pass the DRAM bound gate
    //    and then the magic gate before dereference (reading a hartid or a
    //    garbage pointer directly is a Load fault; the discipline established
    //    by real-hardware and U-Boot forensics).
    let chosen_pa = fdt_table_pa(&fdt);
    let table_pa = chosen_pa
        .into_iter()
        .find(|pa| in_memmap_range(memmap, *pa, core::mem::size_of::<BootFileTable>() as u64) && table_magic_ok(*pa));
    let Some(table_pa) = table_pa else {
        say(
            "### minix-rs kernel image: no BootFileTable (chosen) — module channel not wired, halting\n",
        );
        crate::halt();
    };
    say("  boot file table pa 0x");
    say_hex(table_pa);
    say("\n");
    // SAFETY: the preceding `table_magic_ok` already checked the magic inside
    // the `in_memmap_range(pa, size_of::<BootFileTable>())` bound; QEMU's
    // direct-load staging (or the host injection
    // tool) places the table in physical RAM exclusive to the boot phase, and
    // this face is its first and only reader (single hart, no concurrent
    // writers).
    let table = unsafe { &*(table_pa as *const BootFileTable) };

    // 3. Bump physical pool (boot-shim library `MODULE_REGION_BASE` lane: 32MiB
    //    starting at DRAM+32M, `BUMP_END <= boot_dm_admissible_end()` proven at
    //    compile time): root page → page-table bump region → name pool, in that
    //    order with no overlap; module page takes come after.
    //    Every path goes through Option plus an honest halt, leaving no
    //    `.expect`/panic leg (the panic render path carries baked-VMA fmt
    //    pieces, which under the physical-address view hits exactly that
    //    invariant).
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
    // Name pool: one physical-address bump page, 12 slots of 16B (see the view
    // discussion in `place_modules`).
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

    // 4. 12 modules: locate the table (basename check) → copy each source
    //    straight into bump pages (the source has passed the disjointness gate
    //    against the pool, so `copy_nonoverlapping` cannot overlap).
    let module_table = unsafe { &mut *core::ptr::addr_of_mut!(MODULE_TABLE) };
    if bootface_common::place_modules(
        table,
        memmap,
        names_pool,
        module_table,
        bump_pool_base(),
        bump_end_bound(),
        bump_alloc_pages,
        say,
        say_hex,
    )
    .is_none()
    {
        say("### minix-rs kernel image: module placement failed — halting\n");
        crate::halt();
    }
    // SAFETY: `place_modules` has just filled all 12 slots in contract order;
    // read-only from here on.
    let modules: &'static [BootModule] = unsafe { &*core::ptr::addr_of!(MODULE_TABLE) };
    say("  boot modules placed 12\n");

    // 5. Platform descriptor source: riscv has only the DTB (same lane as the
    //    boot-shim library; the slot lives in .bss, avoiding the heap leak
    //    surface).
    // SAFETY: a single hart on the boot face; written once, then read-only
    // (from the next step on).
    unsafe {
        *core::ptr::addr_of_mut!(PLATFORM_SOURCE) = PlatformDescSource::new(DTB, PhysBytes(dtb_pa));
    }
    // SAFETY: as above — a static reference after the write, with no concurrent
    // rewriting; the single array cell and the length-1 slice share the same
    // storage.
    let platform_sources: &'static [PlatformDescSource] =
        unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(PLATFORM_SOURCE), 1) };

    // T3: the direct-boot parameter source is the standard firmware channel,
    // DTB `/chosen/bootargs` (QEMU `-append` fills it).
    let param_buf: &'static [u8] = {
        let out = crate::bootface_common::param_landing();
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
        let ptr = out.as_ptr();
        // SAFETY: `out` is a static landing pad; the slice covers the
        // normalized prefix only.
        unsafe { core::slice::from_raw_parts(ptr, stats.bytes) }
    };

    // 6. KernelInfo (boot-shim `build_kernel_info` lane) + the real arch_boot
    //    gate (validate → build root → DM → jump_to_kmain, never returning to
    //    this face).
    //
    // The records the collector just produced back the map it produced; the
    // consumer-boundary audit inside `KernelInfo::validate` re-checks the
    // pairing, so the handover is not a promise but a verified fact.
    assert!(
        !sources.is_empty(),
        "bootface: DTB legs must carry source records, not an empty partition"
    );
    // Startup-object records: the canonical set (D01-F07) — kernel image,
    // every boot module, the root page, and the bump pool. Written before the
    // strict gate so the gate can check coverage.
    // SAFETY: single-hart boot face; the array is written once here and read
    // only through the returned slice afterwards.
    static mut STARTUP_OBJECTS: [minix_boot::SourceRecord; minix_boot::WIRE_STARTUP_CAP] =
        [minix_boot::startup_record(0, 0, 0); minix_boot::WIRE_STARTUP_CAP];
    // One-shot guard: a second entry into this boot face would overwrite the
    // `'static` slice returned by the first call (UB under Rust aliasing).
    static STARTUP_TAKEN: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
    assert!(
        !STARTUP_TAKEN.swap(true, core::sync::atomic::Ordering::AcqRel),
        "bootface: startup-object landing re-entered"
    );
    let startup_objects = unsafe {
        // Over capacity must fail closed: the static array is the whole
        // partition, so an unchecked write would run past it.
        assert!(
            3 + modules.len() <= minix_boot::WIRE_STARTUP_CAP,
            "bootface: startup-object partition over capacity"
        );
        let slot = core::ptr::addr_of_mut!(STARTUP_OBJECTS) as *mut minix_boot::SourceRecord;
        let mut n = 0usize;
        slot.add(n).write(minix_boot::startup_record(
            0,
            KERN_PHYS_BASE,
            KERN_PHYS_BASE + KERN_SIZE,
        ));
        n += 1;
        for module in modules {
            slot.add(n).write(minix_boot::startup_record(
                n as u16,
                module.start.0,
                module.start.0 + module.len as u64,
            ));
            n += 1;
        }
        // Root page and bump pool complete the canonical set (D01-F07):
        // these are the bootstrap allocations a provenance consumer must
        // see to know which conventional pages are already spent.
        slot.add(n).write(minix_boot::startup_record(
            n as u16,
            root_page.0,
            root_page.0 + 4096,
        ));
        n += 1;
        slot.add(n).write(minix_boot::startup_record(
            n as u16,
            bump_pool_base(),
            bump_end_bound(),
        ));
        n += 1;
        core::slice::from_raw_parts(slot, n)
    };

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
        startup_objects,
        minix_boot::InputObjectId::DtbBootInput,
    );
    kernel_info.validate_requiring_sources();
    say("  calling arch_boot (validate gate)\n");
    minix_kernel::arch_boot(&kernel_info, root_page)
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

/// Landing descriptor table for the 12 modules (`.bss`, read-only after
/// assembly).
static mut MODULE_TABLE: [BootModule; NR_BOOT_MODULES] = [const {
    BootModule {
        name: "",
        start: PhysBytes(0),
        len: 0,
    }
}; NR_BOOT_MODULES];
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

/// memmap entry storage cell. (The old `fill_memmap` has moved to minix-boot
/// `DtbMemoryMap`.)

/// Platform descriptor source cell (DTB is the only source; written once before
/// `arch_boot`).
static mut PLATFORM_SOURCE: PlatformDescSource = PlatformDescSource::new(DTB, PhysBytes(0));
