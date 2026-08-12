//! UEFI-specific boot helpers.
//!
//! Implements the `BootShim` trait for UEFI firmware. Firmware-specific
//! concerns live here; the actual kernel/module loading code is generic
//! and lives in [`crate::loader`].
//!
//! Responsibilities of this module:
//! - Wrap UEFI BootServices (memory map, page allocation, exit).
//! - Provide a `FileLoader` impl backed by `SimpleFileSystem`.
//! - Wire the firmware-specific pieces into the shared loader.

use alloc::boxed::Box;
use alloc::vec::Vec;
use uefi::boot::{self, AllocateType};
use uefi::fs::FileSystem;
use uefi::mem::memory_map::{MemoryMap, MemoryType};
use uefi::system;
use uefi::table::cfg::{ACPI_GUID, ACPI2_GUID};
use uefi::Guid;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{
    BootPrepareResult, BootShim, DTB, KernelInfo, MemoryRegion, PlatformDescSource, RSDP,
};

use crate::loader::{
    self, FileLoader, KernelLoadResult,
};

/// UEFI Configuration Table GUID for the Device Tree (FDT).
///
/// Defined in the UEFI Specification as `EFI_DEVICE_TREE_GUID`.
/// Not provided by the `uefi` crate, so we define it here.
const DEVICE_TREE_GUID: Guid = uefi::guid!("b1b621d2-f19c-41c5-8310-daa6f018a8d3");

/// Scan the UEFI Configuration Table for platform descriptor sources
/// (ACPI RSDP on x86-64, DTB on ARM64).
///
/// Returns a `'static` slice of `PlatformDescSource` entries, ordered by
/// boot-shim's preference. The kernel takes the first source that parses
/// successfully. An empty slice means no source was found — the kernel
/// falls back to `QemuVirtDesc` (dev) or panics (release).
///
/// # Ordering (DTB + RSDP coexistence)
///
/// Real-world ARM64 servers (SBBR) may provide both DTB and ACPI tables.
/// On ARM64 we prefer DTB first (QEMU virt provides DTB), then ACPI as
/// fallback. On x86-64 only ACPI is used.
///
/// Must be called **before** `exit_boot_services()`, because the UEFI
/// System Table (which holds the configuration table pointer) is only
/// valid while boot services are available.
fn find_platform_sources() -> &'static [PlatformDescSource] {
    use core::cell::RefCell;
    let sources = RefCell::new(Vec::<PlatformDescSource>::new());

    system::with_config_table(|entries| {
        // On x86-64, look for ACPI RSDP (prefer ACPI 2.0+).
        #[cfg(target_arch = "x86_64")]
        {
            for e in entries {
                if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
                    sources.borrow_mut().push(PlatformDescSource::new(RSDP, PhysBytes(e.address as u64)));
                    break;
                }
            }
        }

        // On aarch64, prefer DTB first (QEMU virt provides DTB), then ACPI.
        #[cfg(target_arch = "aarch64")]
        {
            for e in entries {
                if e.guid == DEVICE_TREE_GUID {
                    sources.borrow_mut().push(PlatformDescSource::new(DTB, PhysBytes(e.address as u64)));
                    break;
                }
            }
            // Fall back to ACPI if DTB not present.
            for e in entries {
                if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
                    sources.borrow_mut().push(PlatformDescSource::new(RSDP, PhysBytes(e.address as u64)));
                    break;
                }
            }
        }

        // On other architectures (e.g., riscv64 with UEFI — currently
        // unsupported as a Rust target), return empty and let the kernel
        // use the QEMU fallback.
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            let _ = entries;
        }
    });

    // Leak the Vec so it has 'static lifetime — acceptable for boot-stage
    // code that runs exactly once (same pattern as build_memmap).
    Box::leak(sources.into_inner().into_boxed_slice())
}

/// UEFI implementation of `BootShim`.
///
/// Uses UEFI BootServices to discover memory, allocate pages, load kernel
/// and boot modules from the ESP, then exits boot services before handing
/// control to the kernel.
pub struct UefiBootShim;

impl BootShim for UefiBootShim {
    fn prepare_boot(bump_pages: usize) -> BootPrepareResult {
        let memmap = build_memmap();
        let root_page = alloc_root_page();
        let (bump_base, bump_end) = alloc_bump_region(bump_pages);

        // Both file accesses must happen before ExitBootServices because
        // they depend on the SimpleFileSystem protocol.
        let file_loader = UefiFileLoader;
        let kern = loader::load_kernel_with_loader(&file_loader).expect(
            "Failed to load kernel ELF from ESP — check that the ESP contains \
             /EFI/minix/kernel.elf (loader::KERNEL_PATH); firmware may have \
             not mounted the FAT partition, or the build did not embed the \
             kernel binary into the ESP image",
        );
        let boot_modules =
            loader::load_boot_modules_with_loader(&file_loader, alloc_module_pages);

        // Locate platform descriptor sources (ACPI RSDP and/or DTB) from the
        // UEFI configuration table. Must happen before ExitBootServices because
        // the System Table is only valid while boot services are available.
        let platform_sources = find_platform_sources();

        let kernel_info = build_kernel_info(
            memmap,
            kern.kern_virt_base,
            kern.kern_phys_base,
            kern.kern_size,
            boot_modules,
            // Bootstrap (unpaged kernel) region.
            //
            // C semantics (pre_init.c:114-116):
            //     kinfo.bootstrap_start = &_kern_unpaged_start;
            //     kinfo.bootstrap_len   = &_kern_unpaged_end - &_kern_unpaged_start;
            //
            // In Minix3 C the kernel has a small "unpaged" section that runs
            // before paging is enabled and must remain identity-mapped; its
            // physical range is reclaimed via add_memmap() after boot completes.
            //
            // Rust port design choice: the kernel is higher-half from the
            // very first instruction — there is no separate unpaged section
            // because we never run with paging disabled. The startup trampoline
            // lives in boot-shim (a separate ELF loaded by firmware), not in
            // the kernel proper. Therefore no kernel-side memory needs to be
            // reclaimed at this point; the previous `PhysBytes(0) + len =
            // kern_phys_base.0` was a dangerous over-reclaim that swept up
            // OpenSBI/DTB/U-Boot/boot-shim itself. See 01-boot-shim-bootstrap.md
            // §3.5.1 for the rationale.
            PhysBytes(0),
            0,
            platform_sources,
        );

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

/// Convert UEFI memory map to a static slice of MemoryRegion.
///
/// Only includes CONVENTIONAL memory (free RAM). Leaks the allocation so
/// it has `'static` lifetime — acceptable for boot-stage code that runs
/// exactly once.
pub fn build_memmap() -> &'static [MemoryRegion] {
    let mmap = boot::memory_map(MemoryType::LOADER_DATA)
        .expect("Failed to get UEFI memory map");

    let mut regions: Vec<MemoryRegion> = Vec::new();
    for desc in mmap.entries() {
        if desc.ty == MemoryType::CONVENTIONAL {
            regions.push(MemoryRegion {
                base: PhysBytes(desc.phys_start),
                len: desc.page_count as usize * 4096,
            });
        }
    }
    Box::leak(regions.into_boxed_slice())
}

/// Allocate a single physical page for the root page table.
pub fn alloc_root_page() -> PhysBytes {
    let ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, 1)
        .expect("Failed to allocate root page for page table");
    PhysBytes(ptr.as_ptr() as u64)
}

/// Allocate a bump region for boot-stage page table page allocation.
pub fn alloc_bump_region(num_pages: usize) -> (u64, u64) {
    let ptr = boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, num_pages)
        .expect("Failed to allocate bump region for boot_pt_alloc");
    let base = ptr.as_ptr() as u64;
    let end = base + (num_pages as u64) * 4096;
    (base, end)
}

/// Build a KernelInfo struct.
///
/// `bootstrap_start`/`bootstrap_len` describe the boot-shim's physical
/// memory region that the kernel should reclaim after boot completes.
/// C: kinfo.bootstrap_start/len — pre_init.c:114-116
///
/// `platform_sources` is the ordered list of firmware-provided platform
/// descriptor sources (DTB, RSDP, or both). The kernel tries each in order,
/// using the first that parses successfully. Empty slice = no source found
/// (kernel falls back to QemuVirtDesc or panics).
pub fn build_kernel_info(
    memmap: &'static [MemoryRegion],
    kern_virt_base: VirBytes,
    kern_phys_base: PhysBytes,
    kern_size: u64,
    boot_modules: &'static [BootModule],
    bootstrap_start: PhysBytes,
    bootstrap_len: u64,
    platform_sources: &'static [PlatformDescSource],
) -> KernelInfo {
    KernelInfo {
        memmap,
        kern_virt_base,
        kern_phys_base,
        kern_size,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(kern_virt_base.0 as u64 + kern_size as u64),
        syscall_entry: VirBytes(kern_virt_base.0),
        boot_modules,
        bootstrap_start,
        bootstrap_len,
        platform_sources,
        // P9-1: UEFI load options not yet parsed into key=value pairs.
        // Pass empty slice — kernel's GET_MONPARAMS handler copies 0 bytes
        // to caller (matching C's behavior when param_buf[0] == '\0').
        param_buf: &[],
    }
}

/// Exit UEFI boot services.
///
/// After this call, UEFI boot services (AllocatePages, etc.) are no longer
/// available. The caller must have completed all UEFI allocations before
/// calling this.
pub fn exit_boot_services() {
    // SAFETY: caller has finished all uses of boot services; from this
    // point on only runtime services (if any) may be used. The returned
    // memory map is discarded because the kernel already received the
    // CONVENTIONAL regions via `build_memmap()`.
    unsafe {
        let _mmap = boot::exit_boot_services(MemoryType::LOADER_DATA);
    }
}

// ── UEFI implementation of FileLoader ──

/// `FileLoader` backed by UEFI's `SimpleFileSystem` protocol on the ESP.
pub struct UefiFileLoader;

impl FileLoader for UefiFileLoader {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        let fs_proto = boot::get_image_file_system(boot::image_handle()).ok()?;
        let mut fs = FileSystem::new(fs_proto);
        // UEFI uses backslashes as path separators; convert from the
        // canonical forward-slash format used by `loader::KERNEL_PATH`.
        let uefi_path = path.replace('/', "\\");
        let cs = uefi::CString16::try_from(uefi_path.as_str()).ok()?;
        fs.read(cs.as_ref()).ok()
    }
}

/// UEFI page allocator used by `load_boot_modules_with_loader`.
fn alloc_module_pages(num_pages: usize) -> Option<u64> {
    let ptr =
        boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, num_pages).ok()?;
    Some(ptr.as_ptr() as u64)
}

// Re-export for backwards compatibility with the prior public surface.
use minix_boot::BootModule;

/// Convenience wrapper around `loader::load_kernel_with_loader` for callers
/// that want to invoke the UEFI path directly (e.g. integration tests).
pub fn load_kernel_elf() -> Result<KernelLoadResult, minix_elf::ElfError> {
    loader::load_kernel_with_loader(&UefiFileLoader)
}

/// Convenience wrapper around `loader::load_boot_modules_with_loader`.
pub fn load_boot_modules() -> &'static [BootModule] {
    loader::load_boot_modules_with_loader(&UefiFileLoader, alloc_module_pages)
}

#[cfg(test)]
mod tests {
    //! UEFI-side tests focus on `build_kernel_info` plumbing — the heavy
    //! lifting (ELF parsing, segment copy, module loading) is exercised
    //! by `loader::tests` against a mock file loader, so all firmware
    //! impls share the same coverage.

    use super::*;
    use minix_boot::BootModule;

    #[test]
    fn test_build_kernel_info_fields() {
        static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
            base: PhysBytes(0x10_0000),
            len: 0x1000,
        }];
        static MODULES: [BootModule; 1] = [BootModule {
            name: "vm",
            start: PhysBytes(0x40_0000),
            len: 1024,
        }];

        let info = build_kernel_info(
            &MEMMAP,
            VirBytes(0xFFFFFFFF80000000),
            PhysBytes(0x200000),
            0x100_000,
            &MODULES,
            PhysBytes(0x100000), // bootstrap_start
            0x100000,            // bootstrap_len
            &[],                 // platform_sources (empty = no source)
        );

        assert_eq!(info.kern_virt_base, VirBytes(0xFFFFFFFF80000000));
        assert_eq!(info.kern_phys_base, PhysBytes(0x200000));
        assert_eq!(info.kern_size, 0x100_000);
        assert_eq!(info.memmap.len(), 1);
        assert_eq!(info.boot_modules.len(), 1);
        assert_eq!(info.boot_modules[0].name, "vm");
        assert!(info.platform_sources.is_empty());
    }
}
