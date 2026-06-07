//! OpenSBI-specific boot helpers (riscv64).
//!
//! Implements the `BootShim` trait for OpenSBI firmware.
//! On riscv64 QEMU virt machines, OpenSBI provides the firmware layer
//! instead of UEFI. There is no `riscv64-unknown-uefi` Rust target,
//! so the boot path is:
//!   OpenSBI (-bios default) → kernel binary (-kernel) → rust_main()
//!
//! This module uses a hardcoded memory map for QEMU virt and a simple
//! bump allocator instead of UEFI AllocatePages/ExitBootServices.

use minix_types::{BootPrepareResult, BootShim, KernelInfo, MemoryRegion, PhysBytes, VirBytes};

/// QEMU virt DRAM base address.
const DRAM_BASE: u64 = 0x8000_0000;

/// Default QEMU virt RAM size (128 MB).
const DEFAULT_RAM_SIZE: u64 = 0x800_0000;

/// OpenSBI implementation of `BootShim`.
///
/// Uses hardcoded QEMU virt memory map and a simple bump allocator.
/// No firmware services to exit — the kernel already runs in S-mode.
pub struct OpenSbiBootShim;

impl BootShim for OpenSbiBootShim {
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

        // No ExitBootServices needed for OpenSBI — kernel already in S-mode.

        BootPrepareResult {
            kernel_info,
            root_page,
            bump_base,
            bump_end,
        }
    }
}

// ── OpenSBI-specific helper functions ──

/// Bump allocator state for OpenSBI boot.
///
/// SAFETY: BUMP_PTR is only accessed during boot, which is single-threaded.
/// BKL is not yet initialized, but only one CPU (hart 0) is active at this
/// point. After boot, this allocator is never used again. This satisfies
/// the safety requirements for `static mut` access in a single-threaded
/// boot context.
static mut BUMP_PTR: u64 = 0;

/// Simple bump allocation — allocates `num_pages` contiguous 4KB pages.
///
/// Returns the physical address of the first page.
///
/// # Safety
///
/// The bump pointer starts after the kernel image and stack. The offset
/// must be large enough to not overlap with the loaded ELF segments.
/// QEMU loads the kernel at DRAM_BASE (0x8000_0000), and the kernel
/// image + BSS + stack typically occupy < 4MB, so we skip 4MB.
fn bump_alloc(num_pages: usize) -> u64 {
    unsafe {
        if BUMP_PTR == 0 {
            // Skip first 4MB: OpenSBI (1MB at 0x8000_0000) + kernel image + BSS + stack.
            // The kernel is loaded at 0x8020_0000 and may be up to ~2MB.
            BUMP_PTR = DRAM_BASE + 0x40_0000; // 4MB offset
        }
        let addr = BUMP_PTR;
        BUMP_PTR += (num_pages as u64) * 4096;
        addr
    }
}

/// Build a hardcoded memory map for QEMU virt.
///
/// Returns a static slice with one CONVENTIONAL region covering DRAM.
/// No heap allocation needed — uses a const static array.
pub fn build_memmap() -> &'static [MemoryRegion] {
    const REGION: MemoryRegion = MemoryRegion {
        base: PhysBytes(DRAM_BASE),
        len: DEFAULT_RAM_SIZE as usize,
    };
    &[REGION]
}

/// Allocate a single physical page for the root page table.
pub fn alloc_root_page() -> PhysBytes {
    PhysBytes(bump_alloc(1))
}

/// Allocate a bump region for boot-stage page table page allocation.
///
/// Returns `(base, end)` physical addresses.
pub fn alloc_bump_region(num_pages: usize) -> (u64, u64) {
    let base = bump_alloc(num_pages);
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
        user_sp: VirBytes(0x0000_3fff_ffff_f000), // riscv64 Sv39 user address space top
        boot_modules: &[],
    }
}
