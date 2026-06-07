//! Full boot-chain test kernel — exercises the real Paging trait via UEFI.
//!
//! This test follows the same code path as the real kernel:
//!   boot-shim (UEFI entry) → arch_boot_impl (paging setup) → test output
//!
//! It reuses the same UEFI boot helpers and kernel boot code as the real
//! boot-shim crate, then prints PASS instead of entering kmain().
//!
//! Compile:  cargo build -p hello-boot --target x86_64-unknown-uefi --release
//! Run:      ./run_qemu.sh x86_64 target/x86_64-unknown-uefi/release/hello-boot.efi

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::x86_64::paging::X86_64Paging;
use minix_arch::x86_64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::BootShim;
use boot_shim::UefiBootShim;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    early_console::write_str("### Booting Minix-RS hello-boot (Paging trait)...\n");

    // 1. UEFI boot preparation via BootShim trait
    let result = UefiBootShim::prepare_boot(0, 0, 0, 8);

    // --- Below this point, UEFI boot services are gone ---

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. Run arch_boot_impl — identity map + enable paging
    //    This is the same code path as the real kernel boot.
    let _info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);

    // If we reach here, paging is enabled and the CPU can still execute.
    early_console::write_str("Hello, World!\n");
    early_console::write_str("  arch:           x86_64\n");
    early_console::write_str("  paging:         X86_64Paging (Paging trait)\n");
    early_console::write_str("  identity_map:   via arch_boot_impl\n");
    early_console::write_str("### TEST_RESULT: PASS hello-boot ###\n");

    loop {
        unsafe { asm!("hlt", options(nomem, nostack)); }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
