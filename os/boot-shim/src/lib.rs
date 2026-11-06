//! Boot shim — bridges firmware (UEFI/OpenSBI/...) to the bare-metal kernel.
//!
//! This crate implements the `BootShim` trait (defined in `minix-types`)
//! for different firmware types. The kernel only depends on the trait,
//! not on any concrete implementation.
//!
//! Feature gates in `Cargo.toml` select which implementation gets compiled;
//! code uses the trait uniformly without `#[cfg]` conditionals.
//!
//! Available implementations:
//! - `uefi` feature (default): `UefiBootShim` — uses `uefi` crate
//! - `opensbi` feature: `RiscvUbootBootShim` — hardcoded QEMU virt memory map

#![cfg_attr(not(test), no_std)]
#![cfg_attr(test, allow(internal_features))]

extern crate alloc;

#[cfg(test)]
extern crate std;

// Provide a global allocator for the test binary.
// The kernel and its dependencies use `alloc` but are `#![no_std]`;
// without this, the test linker would have no allocator for them.
#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: std::alloc::System = std::alloc::System;

// Re-export the trait and result type from minix-boot.
pub use minix_boot::{BootShim, BootPrepareResult};

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
/// source. The derived fields follow the boot layout contract:
/// `kern_stack_top = kern_virt_base + kern_size`, `syscall_entry =
/// kern_virt_base`, `free_upper_idx = None`, and the per-arch user stack top
/// from `minix_types::USER_STACK_TOP`.
pub fn build_kernel_info(
    memmap: &'static [minix_boot::MemoryRegion],
    reserved_regions: &'static [minix_boot::MemoryRegion],
    kern_virt_base: minix_types::VirBytes,
    kern_phys_base: minix_types::PhysBytes,
    kern_size: u64,
    boot_modules: &'static [minix_boot::BootModule],
    bootstrap_start: minix_types::PhysBytes,
    bootstrap_len: u64,
    platform_sources: &'static [minix_boot::PlatformDescSource],
    param_buf: &'static [u8],
) -> minix_boot::KernelInfo {
    minix_boot::KernelInfo {
        memmap,
        reserved_regions,
        memmap_sources: &[],
        kern_virt_base,
        kern_phys_base,
        kern_size,
        free_upper_idx: None,
        // Per-arch user stack top; the single authority is
        // `minix_types::USER_STACK_TOP`.
        user_sp: minix_types::VirBytes(minix_types::USER_STACK_TOP),
        kern_stack_top: minix_types::VirBytes(kern_virt_base.0 + kern_size),
        syscall_entry: minix_types::VirBytes(kern_virt_base.0),
        boot_modules,
        bootstrap_start,
        bootstrap_len,
        platform_sources,
        param_buf,
    }
}

// ── Shared, firmware-agnostic loading logic ──
//
// The kernel/module loading pipeline is identical for every firmware
// (parse ELF → copy segments → place modules → build KernelInfo). The
// only thing that differs is how raw file bytes are obtained, captured by
// the `FileLoader` trait. Both `uefi_helpers` and `opensbi_helpers`
// implement this trait and reuse the same shared loader functions.
pub mod loader;

// ── Firmware-specific modules ──
// Each module provides a struct that implements BootShim.

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
pub mod ebs_pair;

// O-11 static-reservation coverage: consumed by the UEFI pairing loop only,
// and it names the ebs_map types, so it follows the same feature gate.
#[cfg(feature = "uefi")]
pub mod resv_claims;

#[cfg(feature = "opensbi")]
pub mod opensbi_helpers;

// ── Convenience re-export of the active implementation ──
// Re-exports the *active firmware implementation's* type — callers still
// write the concrete type. The production UEFI entry calls
// `uefi_helpers::prepare_and_enter_kernel_image`; `UefiBootShim::prepare_boot`
// is the auxiliary `BootShim` form. The Cargo.toml feature selection
// determines which impl is compiled (no forwarding fn is provided).

#[cfg(feature = "uefi")]
pub use uefi_helpers::{find_platform_sources, UefiBootShim};

#[cfg(all(feature = "opensbi", not(feature = "uefi")))]
pub use opensbi_helpers::RiscvUbootBootShim;

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
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
pub fn raw_serial(_text: &str) {}

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
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "riscv64")))]
fn emit_byte(_b: u8) {}
