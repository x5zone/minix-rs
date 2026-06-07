//! Test: paging.enable() switches page table base and CPU continues executing.
//!
//! Uses the same boot path as the real kernel (boot_helpers → arch_boot_impl),
//! then prints PASS. Serial output after enable() proves the code path is
//! still reachable.

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
    early_console::write_str("### test_paging_enable: boot → enable paging → verify alive\n");

    // 1. UEFI boot preparation via BootShim trait
    let result = UefiBootShim::prepare_boot(0, 0, 0, 8);

    // --- Below this point, UEFI boot services are gone ---

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. Run arch_boot_impl — identity map + enable paging
    let _info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);

    // If we reach here, paging is enabled and the CPU can still execute.
    early_console::write_str("  paging enabled — CPU still executing\n");
    early_console::write_str("### TEST_RESULT: PASS test-paging-enable ###\n");

    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
