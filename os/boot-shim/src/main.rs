//! UEFI boot shim — bridges UEFI firmware to the bare-metal kernel.
//!
//! This is the UEFI binary entry point. It calls
//! `prepare_and_enter_kernel_image()`: allocate the v1 arena, classify the
//! final memory map in the ExitBootServices pairing loop, build the
//! bootstrap root, and jump into the standalone high-half `kernel.elf`.
//! `UefiBootShim::prepare_boot` is the `BootShim` trait's auxiliary form and
//! has no production caller.
//!
//! After ExitBootServices, UEFI runtime services may still be available
//! (via GetVariable/SetVariable), but boot services (AllocatePages, etc.)
//! are gone. The kernel itself is UEFI-free — it only receives KernelInfo.
//!
//! Corresponding Minix3 C:
//! - pre_init() — pre_init.c:217 (multiboot → kinfo)
//! - get_parameters() — pre_init.c:94 (parse GRUB data)
//! - pg_alloc_page() — pg_utils.c:138 (allocate root page table)

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use uefi::prelude::*;
use boot_shim::raw_serial_line;

extern crate alloc;

// Global allocator for the UEFI boot-shim binary.
// We don't use uefi's "global_allocator" Cargo feature because it would
// conflict with the test-only allocator in lib.rs. Instead, we set it up
// manually here — this is exactly what the feature does internally.
#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

#[entry]
fn main() -> Status {
    // 1. UEFI boot preparation via `prepare_and_enter_kernel_image`
    //    (not the BootShim trait — see the module docs)
    //    Internally: GetMemoryMap → AllocatePages → load kernel ELF from ESP
    //    → load boot modules → build KernelInfo → ExitBootServices
    // 1024 pages (4 MiB): the identity mapping, the kernel high mapping,
    // DM coverage establishment (kernel + VM windows), the
    // boot-identity eviction (ELF segment pages + the 64 MiB VM heap
    // window at 4 KiB granularity ≈ one PT page per 2 MiB slice ≈ 33
    // pages) and the handoff page's fresh chain all draw lower-level
    // page-table pages from this bump pool via boot_pt_alloc. A
    // fragmentary firmware memmap forces 4 KiB-granularity DM leaves —
    // one PT page per 2 MiB region per window — which the former 16/64
    // pages cannot cover (the kernel's fallback bump is 1 MiB = 256
    // pages, FALLBACK_BUMP_LEN — this pool stays in the same order).
    // 128 pages sufficed for boot alone. The pool also served the old
    // kerninfo-injection chain (1–3 pages per new-root bind), which the
    // real machine exhausted with `AllocationFailed` after ~40 binds.
    // D72VM moved that mapping to VM (`init_page_table`), so the per-bind
    // draw is gone; the pool's remaining pressure is boot itself and the
    // 4 KiB-granularity DM coverage. 1024 = boot needs (~100) + that
    // coverage headroom + safety. On this UEFI leg the bump pages are
    // LOADER_DATA: the classifier keeps them out of the conventional list, so
    // VM's free list never contains them and the 4 MiB is not "taken from
    // VM". The whole-region deduction in vm_handoff matters on the self-boot
    // legs, where the pool is carved from conventional RAM.
    // The shim builds the bootstrap root itself, serializes the v1 handoff
    // arena (address space, seven payload partitions, pairing loop) and
    // absolutely jumps into the standalone high-half `kernel.elf` `_start`. aarch64
    // passes the arena in `x0`; x86-64 passes it in `rbx` (the image
    // trampoline moves it to `rdi`). riscv64's independent shim leg
    // (riscv64-uboot-shim + fw-riscv64-uboot receiver) is wired separately;
    // this binary stays UEFI-only (its riscv64 UEFI-shim form remains
    // unwired — the UEFI targets do not include riscv64).
    boot_shim::uefi_helpers::prepare_and_enter_kernel_image(1024)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // First-light diagnostic: this binary's original implementation was
    // `loop {}` — a silent hang. The kernel is a separate high-half image
    // entered through the v1 handoff; this handler covers shim-side panics
    // (the kernel has its own panic path after `arch_boot`). Any shim panic
    // lands here and nothing appears on the serial line, making a
    // "real hang" indistinguishable from a "swallowed panic". Now: (1)
    // format the panic location + message into a stack buffer (no heap
    // dependency, usable before and after EBS) and print it through
    // raw_serial; (2) fire the kernel diagnostic hook (registered at the
    // arch_boot entry; includes the CPU number + backtrace); (3) stay
    // resident.
    struct StackWriter<'a> {
        buf: &'a mut [u8; 256],
        len: usize,
    }
    impl core::fmt::Write for StackWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            for &b in s.as_bytes() {
                if self.len < self.buf.len() {
                    self.buf[self.len] = b;
                    self.len += 1;
                } // Dropping the tail beyond 256 B is fine: truncated diagnostics beat silence.
            }
            Ok(())
        }
    }
    let mut buf = [0u8; 256];
    let mut writer = StackWriter { buf: &mut buf, len: 0 };
    // PanicInfo's Display output is "panicked at file:line:\nmessage".
    let _ = core::fmt::Write::write_fmt(&mut writer, format_args!("{}", info));
    // Take the length first to end the mutable borrow of buf, then slice by byte.
    let written = writer.len;
    let message = core::str::from_utf8(&buf[..written])
        .unwrap_or("boot-shim panic: (non-utf8 message)");
    raw_serial_line("boot-shim panic: ");
    raw_serial_line(message);
    // The hook lives in the same address space as the panic path: if the
    // arch_boot entry registered one, also print the kernel-formatted
    // message.
    if !minix_types::run_panic_diagnostic_hook(message) {
        raw_serial_line("boot-shim panic: diagnostic hook not registered");
    }
    loop {
        core::hint::spin_loop();
    }
}
