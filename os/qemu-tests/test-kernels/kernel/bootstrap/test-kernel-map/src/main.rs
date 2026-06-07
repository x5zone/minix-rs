//! Test: kernel high-half mapping is semantically correct.
//!
//! After arch_boot_impl sets up identity + high-half mapping and enables paging,
//! reads a known static variable through both the low (identity) and high
//! (kernel) virtual addresses and asserts the values match.

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

// A known sentinel value placed in a static variable.
static SENTINEL: u64 = 0xDEAD_BEEF_CAFE_0001;

// High-half base address — matches x86-64 kernel convention.
const KERN_VIRT_BASE: u64 = 0xFFFF_8000_0000_0000;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_kernel_map: verify high-half mapping\n");

    // Read sentinel via identity-mapped (low) address before enabling paging.
    let sentinel_low = unsafe { core::ptr::read_volatile(&SENTINEL) };
    early_console::write_str("  sentinel (low addr): ");
    early_console::write_hex(sentinel_low);
    early_console::write_str("\n");

    // 1. UEFI boot preparation via BootShim trait — with high-half kernel mapping
    let kern_size: usize = 0x40_0000; // 4MB
    let result = UefiBootShim::prepare_boot(KERN_VIRT_BASE, 0, kern_size, 16);

    // --- Below this point, UEFI boot services are gone ---

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. Run arch_boot_impl — identity map + high-half map + enable paging
    let _info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);

    early_console::write_str("  paging enabled\n");

    // 4. Read sentinel via high-half address.
    let sentinel_phys_addr = &SENTINEL as *const u64 as u64;
    let sentinel_high_vaddr = KERN_VIRT_BASE + sentinel_phys_addr;
    let sentinel_high = unsafe { core::ptr::read_volatile(sentinel_high_vaddr as *const u64) };

    early_console::write_str("  sentinel (high addr ");
    early_console::write_hex(sentinel_high_vaddr);
    early_console::write_str("): ");
    early_console::write_hex(sentinel_high);
    early_console::write_str("\n");

    // Assertion: both reads must yield the same value.
    if sentinel_low != sentinel_high {
        early_console::write_str("### FAIL: high-half mapping mismatch!\n");
        early_console::write_str("  low=0x");
        early_console::write_hex(sentinel_low);
        early_console::write_str(" high=0x");
        early_console::write_hex(sentinel_high);
        early_console::write_str("\n");
        loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
    }

    early_console::write_str("  high-half mapping verified: low == high\n");
    early_console::write_str("### TEST_RESULT: PASS test-kernel-map ###\n");

    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
