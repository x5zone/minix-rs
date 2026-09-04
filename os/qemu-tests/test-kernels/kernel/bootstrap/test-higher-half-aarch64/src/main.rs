//! Test: higher-half kernel transition (aarch64).
//!
//! Verifies that after MMU is enabled and the HigherHalf trait performs
//! the stack/PC switch, execution reaches kmain at a high virtual address
//! with correct stack alignment and frame pointer.
//!
//! Boot flow: UEFI → identity mapping → kernel mapping → enable MMU →
//!   HigherHalf::jump_to_kmain → kmain (qemu_test) → verify SP/PC/FP

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_plat::arm64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

// UEFI test kernels allocate only while boot services are alive (build_memmap
// etc.); boot-shim's pool allocator covers exactly that window. See
// uefi_helpers::UefiPoolAllocator for why the registration lives here and not
// in boot-shim itself.
#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_higher_half (aarch64): verifying higher-half transition...\n");

    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);

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

    uefi_helpers::exit_boot_services();

    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    minix_kernel::arch_boot(&result.kernel_info, result.root_page);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-higher-half-aarch64 ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}