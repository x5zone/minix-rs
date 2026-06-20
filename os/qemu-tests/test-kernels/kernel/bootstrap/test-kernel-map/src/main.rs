//! Test: verify high-half kernel mapping after arch_boot_impl.
//!
//! After arch_boot_impl, the kernel's high-half virtual address range
//! [kern_virt_base, kern_virt_base + kern_size) should be mapped to
//! [kern_phys_base, kern_phys_base + kern_size). We write a sentinel
//! at a known low address, read it back via the high-half alias, and
//! compare.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::x86_64::paging::X86_64Paging;
use minix_plat::x86_64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

/// Sentinel value written to low memory, read back via high-half alias.
const SENTINEL: u64 = 0xdeadbeef_cafe0001;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_kernel_map: verify high-half mapping\n");

    // 1. UEFI boot preparation — use individual helpers (no kernel.elf needed)
    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(8);

    let kern_virt_base: u64 = 0xFFFF_8000_0000_0000;
    let kern_phys_base: u64 = 0x200_000; // 2MB-aligned for 2MB huge pages
    let kern_size: u64 = 0x200_000;

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(kern_virt_base),
        kern_phys_base: PhysBytes(kern_phys_base),
        kern_size,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(kern_virt_base + kern_size as u64),
        syscall_entry: VirBytes(kern_virt_base),
        boot_modules: &[],
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
        platform_descriptor: None,
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

    // 3. Run arch_boot_impl — identity map + high-half map + enable paging
    let _info = minix_kernel::arch_boot_impl::<X86_64Paging>(&result.kernel_info, result.root_page);

    // 4. Verify high-half mapping: write sentinel at a low physical address,
    //    read it back via the high-half virtual alias.
    //    High-half alias: kern_virt_base + (phys_addr - kern_phys_base)
    let test_phys: u64 = kern_phys_base + 0x1000; // offset 4KB into kernel region
    let test_virt: u64 = kern_virt_base + 0x1000; // same offset from kern_virt_base
    let low_addr = test_phys as *mut u64;
    let high_addr = test_virt as *const u64;

    unsafe {
        core::ptr::write_volatile(low_addr, SENTINEL);
    }

    early_console::write_str("  sentinel (low addr): ");
    early_console::write_hex(SENTINEL);
    early_console::write_str("\n");

    let read_val = unsafe { core::ptr::read_volatile(high_addr) };
    early_console::write_str("  read (high alias): ");
    early_console::write_hex(read_val);
    early_console::write_str("\n");

    if read_val == SENTINEL {
        early_console::write_str("  high-half mapping: OK\n");
        early_console::write_str("### TEST_RESULT: PASS test-kernel-map ###\n");
    } else {
        early_console::write_str("  high-half mapping: MISMATCH\n");
        early_console::write_str("### TEST_RESULT: FAIL test-kernel-map ###\n");
    }

    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
