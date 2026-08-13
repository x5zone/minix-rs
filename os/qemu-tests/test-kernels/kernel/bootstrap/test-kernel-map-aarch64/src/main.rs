//! Test: verify high-half kernel mapping after arch_boot_impl (aarch64).
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
use minix_arch::arm64::paging::AArch64Paging;
use minix_plat::arm64::early_console;
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
    early_console::write_str("### test_kernel_map (aarch64): verify high-half mapping\n");

    // 1. UEFI boot preparation — use individual helpers (no kernel.elf needed)
    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(8);

    let kern_virt_base: u64 = 0xFFFF_8000_0000_0000;
    // QEMU virt RAM starts at 0x4000_0000. Use 0x4020_0000 (2MB-aligned, within RAM).
    let kern_phys_base: u64 = 0x4020_0000;
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

    // 3. Run arch_boot_impl — identity map + high-half map + enable MMU
    let _info = minix_kernel::arch_boot_impl::<AArch64Paging>(&result.kernel_info, result.root_page);

    // 4. Verify high-half mapping: write sentinel at a low physical address,
    //    read it back via the high-half virtual alias.
    let test_phys: u64 = kern_phys_base + 0x1000;
    let test_virt: u64 = kern_virt_base + 0x1000;
    let low_addr = test_phys as *mut u64;
    let high_addr = test_virt as *const u64;

    // First, verify identity map works: write and read back at low address
    unsafe {
        core::ptr::write_volatile(low_addr, SENTINEL);
    }
    let low_read = unsafe { core::ptr::read_volatile(low_addr) };
    early_console::write_str("  write+read (low addr): ");
    early_console::write_hex(low_read);
    early_console::write_str("\n");

    early_console::write_str("  sentinel (low addr): ");
    early_console::write_hex(SENTINEL);
    early_console::write_str("\n");

    let read_val = unsafe { core::ptr::read_volatile(high_addr) };
    early_console::write_str("  read (high alias): ");
    early_console::write_hex(read_val);
    early_console::write_str("\n");

    // Also try reading from high address directly (without prior low write)
    let test_phys2: u64 = kern_phys_base + 0x2000;
    let test_virt2: u64 = kern_virt_base + 0x2000;
    let low_addr2 = test_phys2 as *mut u64;
    let high_addr2 = test_virt2 as *mut u64;
    unsafe {
        core::ptr::write_volatile(high_addr2, 0xAAAA_BBBB_CCCC_DDDD);
    }
    let high_read2 = unsafe { core::ptr::read_volatile(high_addr2) };
    let low_read2 = unsafe { core::ptr::read_volatile(low_addr2) };
    early_console::write_str("  write high, read high: ");
    early_console::write_hex(high_read2);
    early_console::write_str("\n");
    early_console::write_str("  write high, read low:  ");
    early_console::write_hex(low_read2);
    early_console::write_str("\n");

    if read_val == SENTINEL {
        early_console::write_str("  high-half mapping: OK\n");
        early_console::write_str("### TEST_RESULT: PASS test-kernel-map-aarch64 ###\n");
    } else {
        early_console::write_str("  high-half mapping: MISMATCH\n");
        early_console::write_str("### TEST_RESULT: FAIL test-kernel-map-aarch64 ###\n");
    }

    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}
