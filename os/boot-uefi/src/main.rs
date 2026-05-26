//! UEFI boot shim — bridges UEFI firmware to the bare-metal kernel.
//!
//! This crate has a single responsibility: collect information from UEFI
//! (memory map, kernel image location, boot modules), build a `KernelInfo`
//! struct, call ExitBootServices, and hand control to the kernel.
//!
//! After ExitBootServices, UEFI runtime services may still be available
//! (via GetVariable/SetVariable), but boot services (AllocatePages, etc.)
//! are gone. The kernel itself is UEFI-free — it only receives KernelInfo.
//!
//! **Architecture**: This crate is architecture-agnostic. UEFI abstracts
//! the hardware; arch differences are handled by the kernel's Paging trait.
//!
//! Corresponding Minix3 C:
//! - pre_init() — pre_init.c:217 (multiboot → kinfo)
//! - get_parameters() — pre_init.c:94 (parse GRUB data)
//! - pg_alloc_page() — pg_utils.c:138 (allocate root page table)

#![no_std]
#![no_main]
#![feature(abi_efiapi)]

use uefi::prelude::*;
use minix_types::{KernelInfo, MemoryRegion, PhysBytes, VirBytes};

extern crate alloc;

/// Image handle of the UEFI application itself.
static mut IMAGE_HANDLE: Option<Handle> = None;

#[entry]
fn uefi_main(image: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi::helpers::init(&mut system_table).expect("UEFI helpers init failed");

    // SAFETY: single-threaded UEFI context — only one entry point.
    unsafe { IMAGE_HANDLE = Some(image); }

    // 1. Get UEFI memory map (before ExitBootServices, because this needs
    //    UEFI AllocatePool to grow the buffer dynamically).
    //    C: get_parameters(ebx, &kinfo) — pre_init.c:94
    let mmap_buf = &mut [0u8; 16384]; // 16KB should fit most memory maps
    let mmap = system_table
        .boot_services()
        .memory_map(mmap_buf)
        .expect("Failed to get UEFI memory map");

    // 2. Locate the kernel image.
    //    In a real boot setup, the kernel might be a separate EFI file loaded
    //    via SimpleFileSystem, or embedded as a UEFI raw section. For now we
    //    use a placeholder.
    //    C: module_list[] — pre_init.c:149 memcpy from GRUB
    let (kern_phys, kern_virt, kern_size) = (0u64, 0u64, 0usize); // TODO: locate kernel

    // 3. Allocate a physical page for the page table root.
    //    C: alloc_pagetable() — pg_utils.c:123 (static pagetables[6])
    let root_page = system_table
        .boot_services()
        .allocate_pages(
            uefi::table::boot::AllocateType::AnyPages,
            uefi::table::boot::MemoryType::LOADER_DATA,
            1,
        )
        .expect("Failed to allocate root page for page table");
    let root_phys = PhysBytes(root_page as u64);

    // 4. Build KernelInfo — the single struct the kernel sees.
    //    C: return &kinfo — pre_init.c:279
    let _kernel_info = KernelInfo {
        memmap: &[],
        kern_virt_base: VirBytes(kern_virt),
        kern_phys_base: PhysBytes(kern_phys),
        kern_size,
        free_upper_idx: 0,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        boot_modules: &[],
    };

    // 5. Exit boot services — UEFI boot services are no longer available
    //    after this call. UEFI runtime services (SetVariable, GetTime) may
    //    remain but are not needed during early kernel bootstrap.
    let (_rt, _mmap) = system_table.exit_boot_services(
        uefi::table::boot::MemoryType::LOADER_DATA);

    // 6. Hand control to the kernel's arch-specific boot.
    //    The kernel is a separate binary; in a real setup, we'd jump to
    //    its entry point. For development, boot-uefi and kernel can be
    //    linked together via a linker script.
    //    C: cstart → pre_init → return &kinfo → kmain()
    minix_kernel::arch_boot(&_kernel_info, root_phys);

    // 7. Never returns.
    unreachable!()
}
