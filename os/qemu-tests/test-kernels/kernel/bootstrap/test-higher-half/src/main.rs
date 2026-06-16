//! Test: higher-half kernel transition (x86_64).
//!
//! Verifies that after paging is enabled and the HigherHalf trait performs
//! the stack/PC switch, execution reaches kmain at a high virtual address
//! with correct stack alignment and frame pointer.
//!
//! Boot flow: UEFI → identity mapping → kernel mapping → enable paging →
//!   HigherHalf::jump_to_kmain → kmain (qemu_test) → verify SP/PC/FP

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_plat::x86_64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_higher_half (x86_64): verifying higher-half transition...\n");

    // 1. UEFI boot preparation — use individual helpers (no kernel.elf needed)
    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(8);

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(0x200_000),
        kern_size: 0x200_000,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
        syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
        boot_modules: &[],
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
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

    // 3. arch_boot performs identity map + kernel map + enable paging,
    //    then calls HigherHalf::jump_to_kmain which switches to high
    //    address and calls kmain. kmain (qemu_test) verifies SP/PC/FP
    //    invariants and prints TEST_RESULT.
    //    This function never returns.
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-higher-half ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}