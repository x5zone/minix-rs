//! UEFI-specific boot helpers.
//!
//! Implements the `BootShim` trait for UEFI firmware.
//! Uses the `uefi` crate for GetMemoryMap, AllocatePages, ExitBootServices.

use uefi::boot::{self, AllocateType};
use uefi::mem::memory_map::{MemoryMap, MemoryType};
use minix_types::{BootPrepareResult, BootShim, KernelInfo, MemoryRegion, PhysBytes, VirBytes};

/// UEFI implementation of `BootShim`.
///
/// Uses UEFI BootServices to discover memory, allocate pages,
/// and exit boot services before handing control to the kernel.
pub struct UefiBootShim;

impl BootShim for UefiBootShim {
    fn prepare_boot(
        kern_virt_base: u64,
        kern_phys_base: u64,
        kern_size: usize,
        bump_pages: usize,
    ) -> BootPrepareResult {
        let memmap = build_memmap();
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);
        let kernel_info = build_kernel_info(memmap, kern_virt_base, kern_phys_base, kern_size);

        exit_boot_services();

        BootPrepareResult {
            kernel_info,
            root_page,
            bump_base,
            bump_end,
        }
    }
}

// ── UEFI-specific helper functions ──
// These are pub so test kernels can call them individually if needed.

/// Convert UEFI memory map to a static slice of MemoryRegion.
///
/// Only includes CONVENTIONAL memory (free RAM).
/// Leaks the allocation so it has `'static` lifetime — acceptable for
/// boot-stage code that runs exactly once.
pub fn build_memmap() -> &'static [MemoryRegion] {
    use alloc::boxed::Box;

    let mmap = boot::memory_map(MemoryType::LOADER_DATA)
        .expect("Failed to get UEFI memory map");

    let mut regions: alloc::vec::Vec<MemoryRegion> = alloc::vec::Vec::new();
    for desc in mmap.entries() {
        if desc.ty == MemoryType::CONVENTIONAL {
            regions.push(MemoryRegion {
                base: PhysBytes(desc.phys_start),
                len: desc.page_count as usize * 4096,
            });
        }
    }

    let boxed = regions.into_boxed_slice();
    Box::leak(boxed)
}

/// Allocate a single physical page for the root page table.
pub fn alloc_root_page() -> PhysBytes {
    let ptr = boot::allocate_pages(
        AllocateType::AnyPages, MemoryType::LOADER_DATA, 1,
    ).expect("Failed to allocate root page for page table");
    PhysBytes(ptr.as_ptr() as u64)
}

/// Allocate a bump region for boot-stage page table page allocation.
pub fn alloc_bump_region(num_pages: usize) -> (u64, u64) {
    let ptr = boot::allocate_pages(
        AllocateType::AnyPages, MemoryType::LOADER_DATA, num_pages,
    ).expect("Failed to allocate bump region for boot_pt_alloc");
    let base = ptr.as_ptr() as u64;
    let end = base + (num_pages as u64) * 4096;
    (base, end)
}

/// Build a KernelInfo struct with the given parameters.
pub fn build_kernel_info(
    memmap: &'static [MemoryRegion],
    kern_virt_base: u64,
    kern_phys_base: u64,
    kern_size: usize,
) -> KernelInfo {
    KernelInfo {
        memmap,
        kern_virt_base: VirBytes(kern_virt_base),
        kern_phys_base: PhysBytes(kern_phys_base),
        kern_size,
        free_upper_idx: 0,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        boot_modules: &[],
    }
}

/// Exit UEFI boot services.
///
/// After this call, UEFI boot services (AllocatePages, etc.) are no longer
/// available. The caller must have completed all UEFI allocations before
/// calling this.
pub fn exit_boot_services() {
    unsafe {
        let _mmap = boot::exit_boot_services(MemoryType::LOADER_DATA);
    }
}
