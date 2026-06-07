//! Test: KernelInfo.memmap reflects the real physical memory layout.
//!
//! Verifies that after UEFI boot, the memory map obtained via boot_helpers
//! contains at least one CONVENTIONAL region. If memmap is empty or has
//! no free RAM, the kernel cannot allocate memory for page tables.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::x86_64::early_console;
use boot_shim::uefi_helpers;
use uefi::prelude::*;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_memmap: verifying UEFI memory map...\n");

    // 1. Get UEFI memory map (before ExitBootServices)
    let memmap = uefi_helpers::build_memmap();

    // Assertion 1: memmap is non-empty
    if memmap.is_empty() {
        early_console::write_str("### FAIL: memmap is empty\n");
        loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
    }
    early_console::write_str("  memmap entries: ");
    early_console::write_hex(memmap.len() as u64);
    early_console::write_str("\n");

    // Assertion 2: at least one region has non-zero size
    let total: u64 = memmap.iter().map(|r| r.len as u64).sum();
    if total == 0 {
        early_console::write_str("### FAIL: total memmap size is 0\n");
        loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
    }
    early_console::write_str("  total CONVENTIONAL memory: ");
    early_console::write_hex(total);
    early_console::write_str("\n");

    // Assertion 3: at least one region starts below 4GB (expected in QEMU)
    let mut has_low_mem = false;
    for region in memmap {
        let r_start = region.base.0;
        let r_end = r_start + region.len as u64;
        early_console::write_str("  region: ");
        early_console::write_hex(r_start);
        early_console::write_str("..");
        early_console::write_hex(r_end);
        early_console::write_str("\n");
        if r_start < 0x1_0000_0000 {
            has_low_mem = true;
        }
    }
    if !has_low_mem {
        early_console::write_str("### FAIL: no CONVENTIONAL region below 4GB\n");
        loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
    }

    early_console::write_str("  memmap is valid: non-empty, has free RAM below 4GB\n");
    early_console::write_str("### TEST_RESULT: PASS test-memmap ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
