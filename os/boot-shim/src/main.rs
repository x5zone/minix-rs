//! UEFI boot shim — bridges UEFI firmware to the bare-metal kernel.
//!
//! This is the UEFI binary entry point. It calls `UefiBootShim::prepare_boot()`
//! (which implements the `BootShim` trait from `minix-types`) to collect
//! firmware information, then hands control to the kernel's `arch_boot`.
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
use minix_kernel::boot_alloc;
use minix_boot::BootShim;
use boot_shim::UefiBootShim;

extern crate alloc;

// Global allocator for the UEFI boot-shim binary.
// We don't use uefi's "global_allocator" Cargo feature because it would
// conflict with the test-only allocator in lib.rs. Instead, we set it up
// manually here — this is exactly what the feature does internally.
#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

#[entry]
fn main() -> Status {
    // 1. UEFI boot preparation via BootShim trait
    //    Internally: GetMemoryMap → AllocatePages → load kernel ELF from ESP
    //    → load boot modules → build KernelInfo → ExitBootServices
    // 64 pages (256 KiB): the identity mapping, the kernel high mapping and
    // DM coverage establishment (kernel + VM windows) all draw
    // lower-level page-table pages from this bump pool via boot_pt_alloc.
    // A fragmentary firmware memmap forces 4 KiB-granularity DM leaves —
    // one PT page per 2 MiB region per window — which 16 pages cannot
    // cover (the same bound as the kernel's fallback bump, FALLBACK_BUMP_LEN).
    let result = UefiBootShim::prepare_boot(64);

    // --- Below this point, UEFI boot services are gone ---

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    minix_arch::pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. Hand control to the kernel's arch-specific boot.
    //    Establishes page tables, enables paging, enters kmain.
    //    NEVER RETURNS.
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);

    // This line should never be reached.
    unreachable!()
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
