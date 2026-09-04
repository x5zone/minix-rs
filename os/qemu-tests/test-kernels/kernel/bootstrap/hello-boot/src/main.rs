//! Full boot-chain test kernel — exercises the real Paging trait via UEFI.
//!
//! This test follows the same code path as the real kernel:
//!   UEFI boot helpers → arch_boot_impl (paging setup) → test output
//!
//! ## Why not use `UefiBootShim::prepare_boot`?
//!
//! `UefiBootShim::prepare_boot` loads `kernel.elf` from the ESP via
//! `FileLoader`, which requires `/EFI/minix/kernel.elf` to exist on the
//! FAT partition. In QEMU tests, the ESP only contains the test kernel
//! itself as `BOOTX64.EFI` — there is no separate `kernel.elf`. We
//! therefore call `uefi_helpers` functions directly (build_memmap,
//! alloc_root_page, alloc_bump_region) and construct `BootPrepareResult`
//! manually. The `arch_boot_impl` path is still fully exercised.

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

// UEFI test kernels allocate only while boot services are alive (build_memmap
// etc.); boot-shim's pool allocator covers exactly that window. See
// uefi_helpers::UefiPoolAllocator for why the registration lives here and not
// in boot-shim itself.
#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

#[entry]
fn main() -> Status {
    early_console::write_str("### Booting Minix-RS hello-boot (Paging trait)...\n");

    // 1. UEFI boot preparation — use individual helpers (no kernel.elf needed)
    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);

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
fn panic(info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    if let Some(loc) = info.location() {
        early_console::write_str("  at ");
        early_console::write_str(loc.file());
        early_console::write_str(":");
        let mut buf = [0u8; 12];
        early_console::write_str(write_u32(loc.line(), &mut buf));
        early_console::write_str(":");
        early_console::write_str(write_u32(loc.column(), &mut buf));
        early_console::write_str("\n");
    }
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

/// Format a u32 as decimal into the caller's buffer (no_std, no itoa).
fn write_u32(mut v: u32, buf: &mut [u8; 12]) -> &str {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 { break; }
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}
