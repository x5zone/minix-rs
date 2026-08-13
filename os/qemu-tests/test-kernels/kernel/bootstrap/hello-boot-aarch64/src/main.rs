//! Full boot-chain test kernel — exercises the real AArch64Paging trait via UEFI.
//!
//! Uses the same boot path as the real kernel:
//!   UEFI boot helpers → arch_boot_impl (paging setup) → test output
//!
//! See hello-boot/main.rs for why we use individual helpers instead of
//! `UefiBootShim::prepare_boot`.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::arm64::paging::AArch64Paging;
use minix_plat::arm64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    early_console::write_str("### Booting Minix-RS hello-boot (aarch64, Paging trait)...\n");

    // 1. UEFI boot preparation — use individual helpers (no kernel.elf needed)
    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(8);

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(0x4020_0000), // QEMU virt RAM starts at 0x4000_0000
        kern_size: 0x200_000,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
        syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
        boot_modules: &[],
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
        platform_sources: &[],
        param_buf: &[],
    };

    let result = BootPrepareResult {
        kernel_info,
        root_page,
        bump_base,
        bump_end,
    };

    // --- Below this point, UEFI boot services are gone ---
    uefi_helpers::exit_boot_services();

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
