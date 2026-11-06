//! riscv64 U-Boot boot-shim — the D-73 loader-side adapter.
//!
//! # Roles (D-73 frozen decision)
//!
//! - **U-Boot** is the loader: it stages the DTB, the kernel ELF and the
//!   12 boot modules into RAM (fixed addresses from `uboot_layout` below —
//!   the single source shared with the gate's boot.scr), then hands over
//!   with `bootelf -p <shim> <dtb_addr>`.
//! - **OpenSBI** is the S-mode runtime service provider this leg inherits
//!   (U-Boot runs under it; the shim does not need SBI calls itself, but the
//!   kernel may use them after takeover).
//! - **This shim** consumes the table, prepares memory + `KernelInfo`,
//!   serializes the frozen `BootHandoffV1` arena and jumps into the
//!   `kernel-uboot` receiver image.
//!
//! # Entry ABI (probe-verified 2026-10-31)
//!
//! `bootelf -p <img> [args…]` passes the arguments after the image address
//! to the entry as `(extra_argc, extra_argv)` in `a0`/`a1` — `argv[0]` is
//! the first extra argument (not the command name). The U-Boot script
//! passes exactly one argument:
//!   - `argv[0]`: hex DTB physical address
//! `autostart=yes` must be set in the U-Boot environment or bootelf only
//! loads and returns (probe finding).
//!
//! The `MNXBOOT1` `BootFileTable` is **not** staged by U-Boot: the shim
//! builds its own table in .bss (`build_shim_table`) from the same layout
//! constants. A pre-built table fatloaded by U-Boot read garbage in its
//! `phys_addr` field on the first QEMU run (the `len` field was correct —
//! a staging-vs-table coherency issue), so the script only stages the
//! files and the shim fills the table itself; the DTB address goes through
//! argv for the same reason (no table entry to mis-read).
//!
//! # Physical execution
//!
//! `bootelf` jumps with **no MMU**: the shim is linked VMA == LMA
//! (`riscv64-uboot.ld`, 0x84500000 — inside the bdinfo/lmb free band, see
//! the script header) and installs its own bootstrap stack before any Rust
//! code runs. After the bootstrap root is enabled the shim keeps executing
//! through the root's identity window until `enter_kernel_image_v1` jumps
//! to the receiver's high-half `_start`.

#![no_std]
#![no_main]
#![cfg(target_arch = "riscv64")]

use core::arch::asm;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::alloc::{GlobalAlloc, Layout};
use boot_shim::{raw_serial, raw_serial_line, RiscvUbootBootShim};
use minix_boot::{BootShim, Serializer, ARENA_CAP};

extern crate alloc;

// ── Image-time bump allocator ────────────────────────────────────────
//
// Same shape as kernel-image's `ImageBump` (single-source pattern): the
// shim's Rust surface needs a heap only for `prepare_boot`'s `Box::leak`
// landings (memmap/reserved/platform/param); one-shot bump, never
// reclaimed. The real boot allocations (root page, PT region, module
// landings, arena) do NOT go through this — they use the physical bump
// pool in `opensbi_helpers`.

// 8 MiB: the kernel ELF (1.2 MB) + 12 module ELFs (~2.5 MB) are each
// `read` into a `Vec` and the bump allocator never reclaims — the reads
// accumulate across the `prepare_boot` lifetime. 8 MiB covers the total
// with headroom; the image .bss grows accordingly (NOBITS, no file cost).
const SHIM_HEAP_LEN: usize = 16 * 1024 * 1024;

static mut SHIM_HEAP: [u8; SHIM_HEAP_LEN] = [0; SHIM_HEAP_LEN];

struct ShimBump {
    cursor: AtomicUsize,
}

impl ShimBump {
    const fn new() -> Self {
        Self {
            cursor: AtomicUsize::new(0),
        }
    }
}

unsafe impl GlobalAlloc for ShimBump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: image-time single-threaded phase (boot CPU exclusive); the
        // atomic cursor only satisfies GlobalAlloc's &self contract; all
        // bounds checked explicitly. `fetch_update` returns `Ok(old_cursor)`;
        // the aligned start is recomputed from it (deterministic: same layout
        // → same rounding).
        let old = self
            .cursor
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                let aligned = cur.checked_add(layout.align() - 1)? & !(layout.align() - 1);
                let end = aligned.checked_add(layout.size())?;
                (end <= SHIM_HEAP_LEN).then_some(end)
            });
        match old {
            // SAFETY: `&raw mut` address arithmetic (no mutable reference to
            // the static is created — the 2024 static_mut_refs rule); the
            // returned pointer's lifetime is the allocation's, and this heap
            // is never reclaimed.
            Ok(old_cursor) => unsafe {
                let aligned = (old_cursor + layout.align() - 1) & !(layout.align() - 1);
                let base = core::ptr::addr_of_mut!(SHIM_HEAP) as *mut u8;
                base.add(aligned)
            },
            Err(_) => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOC: ShimBump = ShimBump::new();

// ── Entry: install the bootstrap stack, then the Rust surface ────────
//
// `bootelf -p` jumps to `e_entry` with `a0/a1` = (extra_argc, extra_argv)
// and **no stack contract** (the register file still points into U-Boot's
// own stack). `_start` swaps to the shim's own stack first; `a0`/`a1`
// survive the `la sp` (they are not touched again before `call`).
//
// Interrupts: U-Boot hands over with `sstatus.SIE` in whatever state its
// trap handling left it; the T2 entry-state contract (no maskable
// interrupt during root build) is honored by clearing SIE here, on the
// shim side, before any Rust code runs.

core::arch::global_asm!(
    r#"
    .attribute arch, "rv64gc"
    .section .text.boot, "ax"
    .globl _start
    _start:
        # 中断静默（T2 入口态契约）：SIE = sstatus bit 1。
        li t2, 2
        csrc sstatus, t2
        # .bss 清零：bootelf -p 的 NOBITS 清零不可依赖（U-Boot 版本行为
        # 差异），而 SHIM_HEAP/游标与表指针都住在 .bss —— 与 kernel-image
        # 续-88 甲案同形，先清再立栈。
        la t0, __bss_start
        la t1, __bss_end
    1:  bgeu t0, t1, 2f
        sd zero, 0(t0)
        addi t0, t0, 8
        j 1b
    2:
        la sp, UBOOT_SHIM_STACK_TOP
        li ra, 0
        li fp, 0
        call {uboot_main}
    2:  wfi
        j 2b
    "#,
    uboot_main = sym uboot_main,
);

/// D-73 U-Boot leg layout: the same addresses the boot.scr uses to
/// `fatload` the files. All inside the bdinfo/lmb free band
/// (0x80060000-0x8dea9fff for 256 MiB QEMU virt; the gate uses 512 MiB so
/// the band is even wider). These are the single source for both the
/// boot.scr generation and the shim's file reads — no MNXBOOT1 table file
/// is needed for the first gate (a fatload-coherency issue with the
/// table's `phys_addr` field made the table path unreliable; the shim now
/// builds its own table in .bss with correct values, see below).
pub mod uboot_layout {
    /// kernel-uboot ELF file staging (fatload target).
    pub const KERNEL_FILE_STAGE: u64 = 0x8600_0000;
    /// Per-module file staging base (fatload target for module i).
    pub const MODULE_FILE_STAGE_BASE: u64 = 0x8700_0000;
    /// Stride between module file staging slots (each < 1 MiB).
    pub const MODULE_FILE_STAGE_STRIDE: u64 = 0x10_0000;
    /// DTB staging (fatload target).
    pub const DTB_STAGE: u64 = 0x8c00_0000;
    /// Boot module names in `MODULE_NAMES` order (must match
    /// `boot_shim::loader::MODULE_NAMES` — single-source via the
    /// compile-time assert in opensbi_helpers).
    pub const MODULE_NAMES: &[&str] = &[
        "ds", "rs", "pm", "sched", "vfs", "memory", "tty", "mib", "vm",
        "pfs", "mfs", "init",
    ];

}

fn halt() -> ! {
    loop {
        // SAFETY: wfi in S-mode parks the hart; no memory effects.
        unsafe { asm!("wfi", options(nomem, nostack)) }
    }
}

/// Parse one hex argument from `argv[idx]`. Returns `None` on absence,
/// null pointer, or non-hex content. The U-Boot script writes plain hex
/// (no `0x` prefix); 16-digit cap covers any u64.
fn parse_hex_arg(argc: usize, argv: *const *const u8, idx: usize) -> Option<u64> {
    if idx >= argc || argv.is_null() {
        return None;
    }
    // SAFETY: U-Boot's argv array lives in its heap and stays valid for the
    // handover window; idx < argc was checked above.
    let arg = unsafe { *argv.add(idx) } as *const u8;
    if arg.is_null() {
        return None;
    }
    let mut value: u64 = 0;
    let mut digits = 0;
    // SAFETY: NUL-terminated by the U-Boot boot script contract; the scan
    // stops at the terminator and caps at 16 hex digits.
    unsafe {
        let mut i = 0usize;
        while i < 16 {
            let c = *arg.add(i);
            if c == 0 {
                break;
            }
            let d = match c {
                b'0'..=b'9' => (c - b'0') as u64,
                b'a'..=b'f' => (c - b'a' + 10) as u64,
                b'A'..=b'F' => (c - b'A' + 10) as u64,
                _ => return None,
            };
            value = value.checked_mul(16)?.checked_add(d)?;
            digits += 1;
            i += 1;
        }
    }
    (digits > 0).then_some(value)
}

/// Boot module names (matches `loader::MODULE_NAMES`).
const MODULE_NAMES: &[&str] = uboot_layout::MODULE_NAMES;

/// Module file staging address for the given index.
fn module_file_stage(idx: usize) -> u64 {
    uboot_layout::MODULE_FILE_STAGE_BASE + (idx as u64) * uboot_layout::MODULE_FILE_STAGE_STRIDE
}

/// The shim-built MNXBOOT1 table lives here (written at boot, before
/// `prepare_boot` reads it). 16 slots × 80 B + 16 B header = 1296 B.
static mut SHIM_TABLE: [u8; 16 + 16 * 80] = [0; 16 + 16 * 80];

/// Read a staged ELF file's used length from its program headers.
///
/// The U-Boot script only stages files; the table stores the slot capacity.
/// Parsing the staged ELF header gives the real file end (`max(p_offset +
/// p_filesz)` over PT_LOAD), so `BootModule.len` is not the 1 MiB/2 MiB slot.
/// Returns `None` for a non-ELF payload or a header that does not fit the
/// slot; the caller then falls back to the slot capacity.
///
/// # Safety
/// `pa` must point to `slot` readable bytes of the staged file (identity
/// mapped during boot, single-hart).
unsafe fn staged_elf_used_len(pa: u64, slot: u64) -> Option<u64> {
    // SAFETY: caller guarantees the readable range; reads are volatile and
    // bounds-checked against `slot`.
    unsafe {
        let read_u16 = |off: u64| -> Option<u16> {
            if off.checked_add(2)? > slot { return None; }
            Some(core::ptr::read_unaligned((pa + off) as *const u16))
        };
        let read_u32 = |off: u64| -> Option<u32> {
            if off.checked_add(4)? > slot { return None; }
            Some(core::ptr::read_unaligned((pa + off) as *const u32))
        };
        let read_u64 = |off: u64| -> Option<u64> {
            if off.checked_add(8)? > slot { return None; }
            Some(core::ptr::read_unaligned((pa + off) as *const u64))
        };
        if read_u32(0)? != 0x464C_457F { return None; } // \x7fELF
        if read_u16(4)? != 2 || read_u16(5)? != 1 { return None; } // 64-bit LE
        let phoff = read_u64(32)?;
        let phentsize = read_u16(54)? as u64;
        let phnum = read_u16(56)? as u64;
        if phentsize < 56 { return None; }
        let table_end = phoff.checked_add(phnum.checked_mul(phentsize)?)?;
        if table_end > slot { return None; }
        let mut end = 0u64;
        let mut i = 0u64;
        while i < phnum {
            let ph = phoff + i * phentsize;
            if read_u32(ph)? == 1 { // PT_LOAD
                let off = read_u64(ph + 8)?;
                let filesz = read_u64(ph + 32)?;
                end = end.max(off.checked_add(filesz)?);
            }
            i += 1;
        }
        (end > 0).then_some(end)
    }
}

/// Build the MNXBOOT1 table in .bss with the correct file addresses.
///
/// The table's binary layout must match `opensbi_helpers::BootFileTable`:
/// magic u64 + entry_count u32 + pad u32 + 16 × (path[64] + addr u64 + len u64).
/// The shim writes it itself — the U-Boot script only `fatload`s the files
/// to the fixed staging addresses, and the shim fills in the table with
/// those same addresses. This avoids the staging-coherency issue observed
/// when U-Boot fatloads a pre-built table binary.
unsafe fn build_shim_table() -> u64 {
    let base = core::ptr::addr_of_mut!(SHIM_TABLE) as *mut u8;
    let magic = boot_shim::opensbi_helpers::boot_file_table_magic();
    // SAFETY: writes into the shim's own .bss; single-threaded boot.
    unsafe {
        // magic (u64 LE)
        let magic_bytes = magic.to_le_bytes();
        for i in 0..8 { base.add(i).write_volatile(magic_bytes[i]); }
        // entry_count (u32 LE) = 1 kernel + 12 modules = 13
        let count = (1 + MODULE_NAMES.len()) as u32;
        let count_bytes = count.to_le_bytes();
        for i in 0..4 { base.add(8 + i).write_volatile(count_bytes[i]); }
        // pad (u32) = 0 (already zero from .bss clear)
        // entry 0: kernel
        let e0 = base.add(16);
        let path = b"/EFI/minix/kernel.elf";
        for i in 0..path.len() { e0.add(i).write_volatile(path[i]); }
        e0.add(path.len()).write_volatile(0);
        let addr = uboot_layout::KERNEL_FILE_STAGE;
        let addr_bytes = addr.to_le_bytes();
        for i in 0..8 { e0.add(64 + i).write_volatile(addr_bytes[i]); }
        let slot = 2u64 * 1024 * 1024; // 2 MB kernel slot
        let used = staged_elf_used_len(addr, slot).unwrap_or(slot);
        if used > slot {
            raw_serial_line("boot-shim: kernel ELF exceeds the 2 MiB staging slot — truncating");
        }
        let len_bytes = used.min(slot).to_le_bytes();
        for i in 0..8 { e0.add(72 + i).write_volatile(len_bytes[i]); }
        // entries 1-12: modules
        for (mi, name) in MODULE_NAMES.iter().enumerate() {
            let e = base.add(16 + (mi + 1) * 80);
            let module_path = alloc::format!("/EFI/minix/modules/{}", name);
            for i in 0..module_path.len() { e.add(i).write_volatile(module_path.as_bytes()[i]); }
            e.add(module_path.len()).write_volatile(0);
            let a = module_file_stage(mi);
            let a_bytes = a.to_le_bytes();
            for i in 0..8 { e.add(64 + i).write_volatile(a_bytes[i]); }
            let slot = 1u64 * 1024 * 1024; // 1 MB module slot
            let used = staged_elf_used_len(a, slot).unwrap_or(slot);
            if used > slot {
                raw_serial_line("boot-shim: module ELF exceeds the 1 MiB staging slot — truncating: ");
                raw_serial_line(name);
                raw_serial_line("\n");
            }
            let l_bytes = used.min(slot).to_le_bytes();
            for i in 0..8 { e.add(72 + i).write_volatile(l_bytes[i]); }
        }
    }
    base as u64
}

/// Rust-surface entry: consume the table, prepare, serialize, jump.
///
/// `argv[0]` = DTB PA (from the U-Boot script). The kernel and module file
/// addresses come from the layout constants (matching the boot.scr's
/// fatload targets); the shim builds its own MNXBOOT1 table in .bss with
/// those addresses and hands it to `prepare_boot`.
fn uboot_main(argc: usize, argv: *const *const u8) -> ! {
    raw_serial_line("boot-shim: uboot leg enter (d73)");

    let dtb_pa = match parse_hex_arg(argc, argv, 0) {
        Some(pa) => pa,
        None => {
            raw_serial_line("boot-shim: argv[0] dtb address missing/garbage — halting");
            halt();
        }
    };
    // SAFETY: the DTB was fatloaded by the U-Boot script to this address;
    // install is one-shot and precedes every read.
    unsafe { boot_shim::opensbi_helpers::install_dtb_ptr(dtb_pa) };

    // Build the shim's own MNXBOOT1 table (kernel + 12 modules) with the
    // layout-constant staging addresses, then install it.
    let table_pa = unsafe { build_shim_table() };
    // SAFETY: the table was just written by build_shim_table; install is
    // one-shot and precedes every read.
    unsafe { boot_shim::opensbi_helpers::install_boot_file_table(table_pa) };
    {
        let table = boot_shim::opensbi_helpers::boot_file_table()
            .expect("boot-shim: MNXBOOT1 magic mismatch in shim-built table");
        let n = table.valid_entries().len();
        raw_serial("boot-shim: table built (MNXBOOT1, ");
        raw_serial(core::str::from_utf8(
            alloc::format!("{}", n).as_bytes(),
        ).unwrap_or("?"));
        raw_serial_line(" entries)");
    }

    // prepare_boot: root page + PT region + kernel copy + 12 module landings
    // + KernelInfo (memmap/reserved/param from the argv DTB). The bump pool
    // lives at 0x82000000-0x84000000 (existing constants; inside the
    // measured free band).
    let result = <RiscvUbootBootShim as BootShim>::prepare_boot(1024);
    raw_serial_line("boot-shim: kernel + 12 modules loaded (d73)");

    // The bootstrap root's PT pages come from the reserved region prepare_boot
    // carved out of the same pool; register it before `enter_kernel_image_v1`
    // asserts on it.
    minix_kernel::boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);

    // The v1 arena: the next bump pages after the module landings, inside
    // the same pool — identity-mapped by the bootstrap root once enabled
    // (the same coverage the x86/aarch64 senders rely on for their arenas).
    let arena_pa = boot_shim::opensbi_helpers::bump_alloc_pages(ARENA_CAP / 4096)
        .expect("boot-shim: out of bump memory (arena)");
    // SAFETY: fresh bump pages below the identity bound; nothing else writes
    // them, and the slice lives for the rest of this `!` function.
    let arena = unsafe { core::slice::from_raw_parts_mut(arena_pa as *mut u8, ARENA_CAP) };
    let mut serializer = Serializer::new(arena).expect("handoff arena too small");
    serializer
        .write_envelope(&result.kernel_info)
        .expect("handoff envelope over capacity");
    for region in result.kernel_info.memmap {
        serializer
            .push_memmap(*region)
            .expect("too many memmap regions for the handoff arena");
    }
    serializer.finish_memmap();
    serializer
        .write_reserved_regions(result.kernel_info.reserved_regions)
        .expect("too many reserved regions for the handoff arena");
    serializer
        .write_boot_modules(result.kernel_info.boot_modules)
        .expect("too many boot modules for the handoff arena");
    serializer
        .write_platform_sources(result.kernel_info.platform_sources)
        .expect("too many platform sources for the handoff arena");
    serializer
        .write_param_buf(result.kernel_info.param_buf)
        .expect("param_buf over capacity");
    serializer
        .write_sources(result.kernel_info.memmap_sources)
        .expect("too many memory-map source records for the handoff arena");
    serializer
        .write_startup(result.kernel_info.startup_objects)
        .expect("too many startup-object records for the handoff arena");

    raw_serial_line("boot-shim: [raw] entering kernel image v1");
    // SAFETY: the bootstrap root was enabled by `enter_kernel_image_v1`
    // itself; the arena is identity-mapped by it; the jump lands on the
    // receiver's high-half `_start` with `a0` = arena PA.
    minix_kernel::enter_kernel_image_v1(arena_pa, &result.kernel_info, result.root_page)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Same shape as kernel-image's panic surface: fixed stack buffer, no
    // heap, raw serial (the 16550 emitter paces itself), then park.
    raw_serial("boot-shim(uboot) panic: ");
    struct StackWriter<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }
    impl core::fmt::Write for StackWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.pos >= self.buf.len() {
                    break;
                }
                self.buf[self.pos] = b;
                self.pos += 1;
            }
            Ok(())
        }
    }
    let mut buf = [0u8; 200];
    let mut w = StackWriter { buf: &mut buf, pos: 0 };
    let _ = core::fmt::write(&mut w, format_args!("{info}"));
    let n = w.pos;
    let mut end = n;
    while end > 0 && core::str::from_utf8(&buf[..end]).is_err() {
        end -= 1;
    }
    let text = core::str::from_utf8(&buf[..end]).unwrap_or("");
    raw_serial_line(text);
    raw_serial_line("boot-shim(uboot) panic — halting");
    halt()
}
