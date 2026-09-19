//! Test: aarch64 semantic shutdown through the QEMU test backend
//! (edge1 K11 — the aarch64 leg of the three-arch shutdown verification;
//! S-11 §3.8 two-layer rule: this exercises the QEMU test layer,
//! semihosting `SYS_EXIT`; the real-hardware layer is PSCI SYSTEM_OFF).
//!
//! Boot: AAVMF → this .efi → production `arch_boot_impl` paging bring-up
//! (so the shutdown happens from a realistic post-MMU state), then
//! `minix_kernel::minix_shutdown(0)` → `minix_plat::shutdown_qemu` →
//! `hlt #0xF000` with the A64 semihosting SYS_EXIT argument block.
//!
//! PASS = the run script sees the serial marker AND QEMU terminates on its
//! own with exit code 0 (a guest that merely hangs cannot fake this — the
//! script's timeout would fire instead).

#![no_std]
#![no_main]

use minix_arch::arm64::paging::AArch64Paging;
use minix_arch::pt_alloc;
use minix_plat::arm64::early_console;
use minix_kernel::boot_alloc;
use core::panic::PanicInfo;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_shutdown (aarch64): semihosting SYS_EXIT backend\n");

    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(0x4020_0000),
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

    uefi_helpers::exit_boot_services();

    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let _info = minix_kernel::arch_boot_impl::<AArch64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled (post-MMU shutdown path)\n");

    early_console::write_str("  requesting shutdown via semihosting SYS_EXIT\n");
    early_console::write_str("### TEST_RESULT: PASS test-shutdown-aarch64 ###\n");
    minix_kernel::minix_shutdown(0);

    #[allow(unreachable_code)]
    loop { unsafe { core::arch::asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-shutdown-aarch64 ###\n");
    loop { unsafe { core::arch::asm!("wfe", options(nomem, nostack)); } }
}
