//! Full boot-chain test kernel — exercises the real AArch64Paging trait via UEFI.
//!
//! Uses the same boot path as the real kernel:
//!   UefiBootShim (BootShim trait) → arch_boot_impl (paging setup) → test output

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::arm64::paging::AArch64Paging;
use minix_arch::arm64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::BootShim;
use boot_shim::UefiBootShim;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    early_console::write_str("### Booting Minix-RS hello-boot (aarch64, Paging trait)...\n");

    // 1. UEFI boot preparation via BootShim trait
    let result = UefiBootShim::prepare_boot(0, 0, 0, 8);

    // --- Below this point, UEFI boot services are gone ---

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. Run arch_boot_impl — identity map + enable MMU
    let _info = minix_kernel::arch_boot_impl::<AArch64Paging>(&result.kernel_info, result.root_page);

    // If we reach here, paging is enabled and the CPU can still execute.
    early_console::write_str("Hello, World!\n");
    early_console::write_str("  arch:           aarch64\n");
    early_console::write_str("  paging:         AArch64Paging (Paging trait)\n");
    early_console::write_str("  identity_map:   via arch_boot_impl\n");
    early_console::write_str("### TEST_RESULT: PASS hello-boot-aarch64 ###\n");

    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}
