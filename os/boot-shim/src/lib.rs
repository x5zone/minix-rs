//! Boot shim — bridges firmware (UEFI/OpenSBI/...) to the bare-metal kernel.
//!
//! The boot-prep entry on the U-Boot leg is
//! `opensbi_helpers::prepare_boot`, a plain function (the single-impl
//! `BootShim` trait was removed, F-R17-01). The kernel does **not** go
//! through it at all — its real entry surface is
//! `arch_boot(&KernelInfo, root_page)` / `enter_kernel_image_v1` (see doc 01
//! §3.2 for the honest division of labor: only the U-Boot leg's production
//! path calls `prepare_boot`).
//!
//! Feature gates in `Cargo.toml` select which firmware helpers get compiled;
//! the shared loading logic in `loader.rs` is firmware-agnostic, while
//! `uefi_helpers` / `opensbi_helpers` carry the firmware-specific pieces
//! behind `#[cfg(feature = ...)]` gates (7 `cfg(feature` sites in
//! `lib.rs`, none in business logic).
//!
//! Availability:
//! - `uefi` feature (default): UEFI production entry
//!   `prepare_and_enter_kernel_image` + helpers
//! - `opensbi` feature: `opensbi_helpers::prepare_boot` — the wired U-Boot
//!   leg (called from `uboot_main.rs`); memory comes from the DTB
//!   `/memory` node (QEMU virt; no hardcoded map)

#![cfg_attr(not(test), no_std)]
#![cfg_attr(test, allow(internal_features))]

extern crate alloc;

#[cfg(test)]
extern crate std;

// Provide a global allocator for the test binary.
// The kernel and its dependencies use `alloc` but are `#![no_std]`;
// without this, the test linker would have no allocator for them.
//
// The wrapper counts `alloc` calls (the default `alloc_zeroed`/`realloc`
// implementations route through it) so timing tests can prove that a boot
// phase allocates nothing; it delegates to `std::alloc::System` otherwise.
#[cfg(test)]
struct CountingAllocator;

#[cfg(test)]
std::thread_local! {
    // Per-thread counter: tests assert "this phase allocates nothing" while
    // cargo may run other tests concurrently. A process-wide counter would
    // count their allocations too (R11 D2-01).
    static TEST_ALLOCATIONS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

#[cfg(test)]
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        TEST_ALLOCATIONS.with(|count| count.set(count.get() + 1));
        // SAFETY: delegation with the caller's layout.
        unsafe {
            <std::alloc::System as std::alloc::GlobalAlloc>::alloc(&std::alloc::System, layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: the pointer came from this allocator.
        unsafe {
            <std::alloc::System as std::alloc::GlobalAlloc>::dealloc(
                &std::alloc::System,
                ptr,
                layout,
            )
        }
    }
}

/// Number of heap allocations made by the current test thread so far.
#[cfg(test)]
pub(crate) fn test_allocations() -> usize {
    TEST_ALLOCATIONS.with(|count| count.get())
}

#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: CountingAllocator = CountingAllocator;

// Re-export the result type from minix-boot.
pub use minix_boot::BootPrepareResult;

// Re-export the ELF parser from the shared minix-elf crate.
// Both boot-shim and kernel need ELF parsing, so it lives in os/libs/minix-elf.
pub use minix_elf;

/// Single construction surface for the `KernelInfo` a boot surface hands over
/// (the UEFI and OpenSBI helpers previously carried two drifting
/// 8/9-parameter copies of this function).
///
/// `reserved_regions` is the surface's non-conventional list (UEFI-classified
/// firmware memory, DTB-clipped firmware ranges); pass `&[]` when the surface
/// has none. `param_buf` is the already-normalized `key=value\0` byte string
/// ([`minix_boot::normalize_param_buf`]); pass `&[]` when the leg has no
/// source.
///
/// v10 (D-062) removed three parameters that were pure restatements of the
/// two image-span arguments (`user_sp = USER_STACK_TOP`, `kern_stack_top =
/// kern_virt_base + kern_size`, `syscall_entry = kern_virt_base`) plus the
/// startup-object slice. Consumers derive them from `KernelInfo` instead
/// (`KernelInfo::kern_stack_top()`, `minix_types::USER_STACK_TOP`,
/// `syscall_entry_va()`).
/// Build the receiver-side `KernelInfo` from the sender's named view.
///
/// Taking `&BootReport<'static>` instead of ten positional slices removes
/// the two same-typed `&[MemoryRegion]` arguments that could be swapped
/// silently, and makes the lifetime source explicit: the report's slices
/// are already `'static` (the one-shot boot storage), so no
/// `from_raw_parts` laundering is needed at the call sites (D2-04).
pub fn build_kernel_info(
    report: &minix_boot::BootReport<'static>,
    authority: minix_boot::RamAuthority,
) -> minix_boot::KernelInfo {
    minix_boot::KernelInfo {
        memmap: report.memmap,
        reserved_regions: report.reserved_regions,
        source_records: report.source_records,
        kern_virt_base: report.kern_virt_base,
        kern_phys_base: report.kern_phys_base,
        kern_size: report.kern_size,
        boot_modules: report.boot_modules,
        platform_sources: report.platform_sources,
        param_buf: report.param_buf,
        authority,
    }
}

/// Single-threaded boot-stage cell holding the caller-owned
/// [`minix_boot::BootStorage`].
///
/// The UEFI and U-Boot senders keep their semantic tables in their own
/// `.bss` (R18 D-1) instead of pointing `'static` slices into the handoff
/// arena. `UnsafeCell` + `Sync` is sound under the boot contract: exactly one
/// CPU runs, the storage is filled before the wire image is finalized, and it
/// is read by the same thread before the jump. It is not a general-purpose
/// shared cell.
pub struct BootStorageCell {
    storage: core::cell::UnsafeCell<minix_boot::BootStorage>,
    taken: core::sync::atomic::AtomicBool,
}

// SAFETY: see the type doc — single-threaded boot, one writer, one reader.
unsafe impl Sync for BootStorageCell {}

impl BootStorageCell {
    /// An empty cell; usable as a static initializer (`.data`, since the
    /// contained `BootStorage::new()` is not all-zero).
    pub const fn new() -> Self {
        Self {
            storage: core::cell::UnsafeCell::new(minix_boot::BootStorage::new()),
            taken: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Take the storage exactly once. A second call is a boot bug and fails
    /// closed instead of handing out a second `&mut` alias.
    #[allow(clippy::mut_from_ref)] // the one-shot flag makes this the sole borrow
    pub fn take(&self) -> &mut minix_boot::BootStorage {
        assert!(
            !self.taken.swap(true, core::sync::atomic::Ordering::AcqRel),
            "BootStorageCell::take called twice — the boot storage has one owner"
        );
        // SAFETY: the one-shot flag above proves no other `&mut` was handed
        // out; the boot contract keeps every access on one thread.
        unsafe { &mut *self.storage.get() }
    }
}

impl BootStorageCell {
    /// Publish the filled storage as a `'static` borrowed view.
    ///
    /// The cell is a boot-time static: all `fill_*` calls have completed and
    /// no mutation follows, so handing out a `'static` view is the same
    /// one-shot contract as [`Self::take`]. This is the only lifetime bridge;
    /// callers build `KernelInfo` from the named `BootReport` fields instead
    /// of re-creating `'static` slices with `from_raw_parts`.
    pub fn report_static(
        &'static self,
        kern_virt_base: minix_types::VirBytes,
        kern_phys_base: minix_types::PhysBytes,
        kern_size: u64,
    ) -> minix_boot::BootReport<'static> {
        // SAFETY: one-shot boot cell; fills are complete before this call,
        // and the boot contract forbids mutation after the report is taken.
        let storage: &'static minix_boot::BootStorage = unsafe { &*self.storage.get() };
        storage.report(kern_virt_base, kern_phys_base, kern_size)
    }
}

impl Default for BootStorageCell {
    fn default() -> Self {
        Self::new()
    }
}

// ── Shared, firmware-agnostic loading logic ──
//
// The kernel/module loading pipeline is identical for every firmware
// (parse ELF → copy segments → place modules → build KernelInfo). The
// only thing that differs is how raw file bytes are obtained, captured by// the `FileLoader` trait. Both `uefi_helpers` and `opensbi_helpers`
// implement this trait and reuse the same shared loader functions.
pub mod loader;

#[cfg(test)]
pub(crate) mod test_serial {
    //! Process-wide lock for tests that touch one-shot crate globals: the
    //! DTB preflight state (`dtb_copy`) and the `resv_claims` static muts.
    //! This workspace pins `RUST_TEST_THREADS=1`
    //! (`.cargo/config.toml`), so the default path is already serial; the
    //! lock keeps those globals coherent when a caller explicitly overrides
    //! the thread count.

    use std::sync::{Mutex, MutexGuard};

    static LOCK: Mutex<()> = Mutex::new(());

    pub(crate) fn lock() -> MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn allocation_counter_sees_this_thread() {
        let before = super::test_allocations();
        let v: std::vec::Vec<u8> = std::vec![0u8; 8];
        let after = super::test_allocations();
        assert!(
            after > before,
            "per-thread allocation counter must see this thread's allocation"
        );
        drop(v);
    }
}

// ── Firmware-specific modules ──

#[cfg(feature = "uefi")]
pub mod uefi_helpers;

// UEFI-leg descriptor walker/classifier for the v1 handoff pairing loop
// (T1). Host-testable half; the raw GetMemoryMap/ExitBootServices ABI calls
// stay in `uefi_helpers`.
#[cfg(feature = "uefi")]
pub mod ebs_map;

// Scheme-B pairing loop (GetMemoryMap -> classify -> ExitBootServices) with
// injected firmware calls; host-testable retry/buffer discipline.
#[cfg(feature = "uefi")]
pub mod dtb_copy;
#[cfg(feature = "uefi")]
pub mod ebs_pair;

// O-11 static-reservation coverage: consumed by the UEFI pairing loop only,
// and it names the ebs_map types, so it follows the same feature gate.
#[cfg(feature = "uefi")]
pub mod resv_claims;

#[cfg(feature = "opensbi")]
pub mod opensbi_helpers;

// ── Convenience re-export of the active implementation ──
// The UEFI production entry is `uefi_helpers::prepare_and_enter_kernel_image`;
// the U-Boot leg's entry is `opensbi_helpers::prepare_boot`.

#[cfg(feature = "uefi")]
pub use uefi_helpers::find_platform_sources_into;

/// Maximum page-table subrange the boot-shim hands to the kernel image (in
/// 4 KiB pages). Both shim call sites (`main.rs` on UEFI, `uboot_main.rs` on
/// U-Boot) pass this value, and the U-Boot pool's compile-time partition
/// assertion uses it, so a change is one edit rather than a literal drift.
/// The kernel-image self-boot form allocates a smaller subrange (256 pages)
/// from the same pool; that stays below this bound.
pub const BOOT_PT_BUMP_PAGES: usize = 1024;

/// Raw-write the QEMU serial port (COM1 data port 0x3F8; OVMF has already
/// initialized the 16550).
///
/// Purpose: after EBS the logging path of `uefi::println!` is no longer
/// reliable (see the comment in `uefi_helpers::exit_boot_services`); a raw
/// port write does not depend on any UEFI state — the dedicated post-EBS
/// channel for first-light diagnostics.
///
/// x86 semantics stay as they are: byte-by-byte direct writes, without
/// waiting for the transmitter to drain (for the waiting path see
/// [`raw_serial_line`]).
#[cfg(target_arch = "x86_64")]
pub fn raw_serial(text: &str) {
    for byte in text.bytes() {
        // SAFETY: the COM1 data port is a fixed I/O resource of the QEMU/PC
        // platform; boot-shim is its sole owner (no concurrent writer after
        // the firmware hands over), and the `out` instruction leaks no side
        // effects.
        unsafe {
            core::arch::asm!("out dx, al", in("dx") 0x3f8u16, in("al") byte, options(nomem, nostack));
        }
    }
}

/// aarch64: after EBS the only option is to touch PL011 directly. The
/// register layout and transmit throttling both live in `minix-plat`'s
/// early_console (single source of truth), so this function does not
/// re-implement them. On this architecture "wait for drain" and "don't wait"
/// are indistinguishable (both channels share the same throttled emitter), so
/// this goes byte-by-byte through [`emit_byte`].
#[cfg(target_arch = "aarch64")]
pub fn raw_serial(text: &str) {
    for byte in text.bytes() {
        emit_byte(byte);
    }
}

/// riscv64: the QEMU virt 16550 early console from `minix-plat` (single
/// source of truth, same emitter the direct-boot bootface uses). The U-Boot
/// leg needs a live serial channel before the kernel image takes over —
/// `bootelf` hands over with no console contract of its own.
#[cfg(target_arch = "riscv64")]
pub fn raw_serial(text: &str) {
    minix_plat::riscv64::early_console::write_str(text);
}

/// All other targets: no verified bare-metal serial channel; degrade to an
/// empty implementation.
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
pub fn raw_serial(_text: &str) {}

/// Print an unsigned decimal integer through [`raw_serial`].
///
/// This exists for post-EBS diagnostics: the UEFI formatter is no longer a
/// valid output path after `ExitBootServices`, but counts still need to be
/// reported. The conversion uses a fixed 20-byte stack buffer, so it neither
/// allocates nor calls back into firmware.
pub fn raw_serial_u64(mut value: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    raw_serial(core::str::from_utf8(&buf[i..]).unwrap_or("?"));
}

/// Raw-write the serial port byte by byte (bounded wait on the transmit side
/// + automatic `\r` after `\n`).
///
/// Difference from [`raw_serial`]: this function does not require the caller
/// to supply `\r\n`, waits for transmit readiness (the original implementation
/// writing back-to-back overflowed the 16550 transmit buffer and dropped
/// characters), and consumes only the byte stream of `&str` — no allocation
/// and no UTF-8 re-slicing risk on the panic path. The panic handler uses it.
///
/// It lives in the library rather than in the binary so the architecture
/// split has a single home; otherwise `raw_serial` and `raw_serial_line`
/// would each have to write their own cfg. The x86 leg's instructions and
/// polling bound are unchanged verbatim.
pub fn raw_serial_line(text: &str) {
    for byte in text.bytes() {
        let mut emit = [byte, 0];
        let n = if byte == b'\n' {
            emit[0] = b'\r';
            emit[1] = b'\n';
            2
        } else {
            1
        };
        for &b in &emit[..n] {
            emit_byte(b);
        }
    }
}

/// [`raw_serial_line`]'s single-byte emitter: first wait (bounded) for the
/// transmitter to drain, then write.
#[cfg(target_arch = "x86_64")]
fn emit_byte(b: u8) {
    // SAFETY: COM1 (base 0x3F8) is a fixed I/O resource of the QEMU/PC
    // platform; after the handover boot-shim is the sole owner. LSR (0x3FD =
    // base+5) bit5 = THRE (transmit holding register empty): write THR only
    // when set, which guarantees every byte reaches the transmit register.
    // Evidence: polling is bounded (~100k iterations then give up, the byte
    // is still written) — if LSR misbehaves individual bytes are dropped,
    // never the whole panic message.
    // Errata (2026-10-06): the old implementation read 0x3F9 = IER (interrupt
    // enable register, only bits 0-3 valid, bit5 always 0) — every byte spun
    // to the full limit and was then written anyway. After the port
    // correction THRE is really awaited (early serial speedup by an order of
    // magnitude).
    unsafe {
        let mut spins: u32 = 0;
        loop {
            let lsr: u8;
            core::arch::asm!("in al, dx", out("al") lsr, in("dx") 0x3fdu16, options(nomem, nostack));
            if lsr & 0x20 != 0 || spins >= 100_000 {
                break;
            }
            spins += 1;
        }
        core::arch::asm!("out dx, al", in("dx") 0x3f8u16, in("al") b, options(nomem, nostack));
    }
}

/// aarch64: emitter = `minix-plat`'s early-console write (it has its own
/// bounded "wait for TXFF"; the limit and the bit definitions live there, so
/// this file does not repeat a copy of the register semantics).
#[cfg(target_arch = "aarch64")]
fn emit_byte(b: u8) {
    use minix_plat::EarlyConsole;
    <minix_plat::CurrentEarlyConsole as EarlyConsole>::write_byte(b);
}

/// riscv64: emitter = `minix-plat`'s early-console write (QEMU virt 16550,
/// with its own pacing; the U-Boot leg shares the direct-boot bootface's
/// console contract).
#[cfg(target_arch = "riscv64")]
fn emit_byte(b: u8) {
    use minix_plat::EarlyConsole;
    <minix_plat::CurrentEarlyConsole as EarlyConsole>::write_byte(b);
}

/// Other targets: empty implementation (same tune as [`raw_serial`]).
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
fn emit_byte(_b: u8) {}
