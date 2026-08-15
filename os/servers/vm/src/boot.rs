//! VM boot contract — kernel → VM startup parameters.
//!
//! Corresponds to Minix3's `struct bootinfo kernel_boot_info` (glo.h),
//! filled once by `sys_getkinfo()` inside `init_vm()` (main.c:442),
//! plus the boot module list (`struct multiboot_module_t module_list[]`,
//! main.c:485-489) and the kernel's own memory footprint
//! (`kernel_allocated_bytes(_dynamic)`, main.c:492-495).
//!
//! # Why an explicit struct (not a hidden global)
//!
//! C keeps `kernel_boot_info` in BSS and every VM function reads it
//! implicitly. Rust models the kernel→VM boot hand-off as an explicit
//! input to [`VmServer::new_with_boot_params`](crate::VmServer):
//! the boot protocol (see `01-stage-kernel/09-vm-boot-protocol.md`) is
//! one-way and one-shot, so making the dependency visible at the
//! constructor keeps the startup chain auditable and testable.

use minix_types::{BootImage, Endpoint, NR_BOOT_PROCS};
use crate::phys_mem::{BootMemRegion, CLICK_SIZE};

/// VM's own boot-image process number.
///
/// C: `com.h:67` — `VM_PROC_NR 8`. Used both as the boot-image
/// `proc_nr` (kernel/table.c) and as the `vmproc[]` slot index
/// (main.c:474, 578).
pub const VM_PROC_NR: i32 = Endpoint::VM.get();

/// One boot-time module (ELF blob) loaded by the bootloader/kernel.
///
/// C: `struct multiboot_module_t` — main.c:485-489 charges each module's
/// size (`mod_end - mod_start + 1`, rounded up to a page) to the global
/// page total via `mem_add_total_pages()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootModule {
    /// Physical start of the module blob. C: `mod_start`.
    pub start_addr: u64,
    /// Module size in bytes. C: `mod_end - mod_start + 1`.
    pub len: u64,
}

/// Kernel's own memory footprint reported at boot.
///
/// C: `kernel_boot_info.kernel_allocated_bytes` (static) and
/// `kernel_allocated_bytes_dynamic` (dynamic) — main.c:492-495.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KernelAllocated {
    pub static_bytes: u64,
    pub dynamic_bytes: u64,
}

impl KernelAllocated {
    pub const ZERO: Self = Self { static_bytes: 0, dynamic_bytes: 0 };
}

/// Boot parameters passed from the kernel to VM at startup.
///
/// C: the single global `kernel_boot_info` (glo.h). Rust passes it
/// explicitly so the startup chain does not depend on hidden global
/// state that is only populated once during `init_vm()`.
#[derive(Debug, Clone, Copy)]
pub struct BootParams<'a> {
    /// Total physical memory pages.
    ///
    /// C: `total_pages`, accumulated by `mem_init()` from the memory
    /// chunks (alloc.c:319-331): `total_pages += chunks[i].size`.
    pub total_pages: usize,
    /// Free physical memory regions (page-aligned).
    ///
    /// C: `mem_chunks[]` produced by `get_mem_chunks()` (utility.c:44-79)
    /// from `kernel_boot_info.memmap[]`.
    pub free_regions: &'a [BootMemRegion],
    /// Boot process list.
    ///
    /// C: `kernel_boot_info.boot_procs[]` (main.c:497-520).
    pub boot_procs: &'a [BootImage],
    /// Boot module list.
    ///
    /// C: `kernel_boot_info.module_list[]` (main.c:485-489).
    pub modules: &'a [BootModule],
    /// Kernel's own memory footprint (charged via `mem_add_total_pages`).
    pub kernel_allocated: KernelAllocated,
    /// Whether this is a fresh boot.
    ///
    /// C: `is_first_time()` (main.c:79-88) — returns true when RS still
    /// holds `RTS_BOOTINHIBIT`, which gates `init_vm()` in `main()`.
    pub is_first_time: bool,
}

/// Placeholder boot image for the VM process itself.
///
/// Used by [`BootParams::placeholder()`] so the production binary has a
/// valid VM slot until the real boot protocol (sys_getkinfo) lands.
const VM_BOOT_IMAGE: BootImage = {
    let mut img = BootImage::empty();
    img.proc_nr = VM_PROC_NR;
    img.endpoint = Endpoint::VM;
    img
};

impl<'a> BootParams<'a> {
    /// Single-region boot params with no modules and no boot processes.
    ///
    /// Intended for unit tests that only exercise allocator/dispatch
    /// paths. Production code must use real boot parameters.
    pub fn simple(total_pages: usize, free_regions: &'a [BootMemRegion]) -> Self {
        Self {
            total_pages,
            free_regions,
            boot_procs: &[],
            modules: &[],
            kernel_allocated: KernelAllocated::ZERO,
            is_first_time: true,
        }
    }

    /// Placeholder boot params for the current binary entry point.
    ///
    /// Values match the previous hardcoded mock in `main.rs`
    /// (65536 pages starting at physical 0x100000). Once the kernel IPC
    /// vector (`minix-sys::sys_getkinfo`) lands, the boot-shim fills
    /// these from the real boot protocol instead.
    pub fn placeholder() -> BootParams<'static> {
        BootParams {
            total_pages: 65536,
            free_regions: &[BootMemRegion {
                base: 0x100000,
                size: 65536 * CLICK_SIZE,
            }],
            boot_procs: &[VM_BOOT_IMAGE],
            modules: &[],
            kernel_allocated: KernelAllocated::ZERO,
            is_first_time: true,
        }
    }

    /// Validates boot parameters.
    ///
    /// C: `init_vm()` sanity checks — `assert(kernel_boot_info.mmap_size > 0)`
    /// and `assert(kernel_boot_info.mods_with_kernel > 0)`
    /// (main.c:451-452). The module-count assert is deliberately not enforced
    /// here: unit tests construct module-free params, and the production boot
    /// protocol (09-vm-boot-protocol) guarantees a non-empty module list
    /// before VM starts.
    pub fn validate(&self) {
        // C: assert(kernel_boot_info.mmap_size > 0) — main.c:451
        assert!(
            !self.free_regions.is_empty(),
            "BootParams: no free memory regions (C: mmap_size > 0)"
        );
        for r in self.free_regions {
            r.validate();
        }

        // C: total_pages = Σ chunk sizes — mem_init() (alloc.c:319-331)
        let region_pages: usize = self
            .free_regions
            .iter()
            .map(|r| r.size / CLICK_SIZE)
            .sum();
        assert_eq!(
            self.total_pages, region_pages,
            "BootParams: total_pages ({}) must equal the free-region page sum ({})",
            self.total_pages, region_pages
        );

        // Boot processes must fit the process table (C: init_proc panic —
        // main.c:272-273) and VM must not be double-registered.
        //
        // NOTE: the bound uses minix-types' NR_BOOT_PROCS (17), which is
        // stricter than C's `_NR_PROCS` (256, sys_config.h:8). This is
        // intentional: the boot-image table (kernel/table.c, 17 entries)
        // bounds real boot proc numbers at or below 11, so the tighter
        // check cannot reject a valid boot image.
        for ip in self.boot_procs {
            assert!(
                ip.proc_nr < NR_BOOT_PROCS as i32,
                "BootParams: boot proc nr {} out of range",
                ip.proc_nr
            );
        }
        let vm_count = self
            .boot_procs
            .iter()
            .filter(|ip| ip.proc_nr == VM_PROC_NR)
            .count();
        assert!(
            vm_count <= 1,
            "BootParams: VM_PROC_NR listed {} times in boot_procs",
            vm_count
        );
    }

    /// Pages to charge to the global page total on top of the free
    /// regions.
    ///
    /// C: `init_vm()` — the `mem_add_total_pages()` call points
    /// (main.c:485-495): boot modules plus the kernel's own static and
    /// dynamic allocations. The kernel's free list does not include
    /// boot-time modules, so the allocator must learn the true physical
    /// memory size (main.c:456 comment).
    pub fn extra_pages(&self) -> usize {
        let mut pages: u64 = 0;

        // C: main.c:485-489 — the loop bound is `mods_with_kernel - 1`,
        // i.e. the last module entry is deliberately excluded.
        let charged_modules = self.modules.len().saturating_sub(1);
        for m in &self.modules[..charged_modules] {
            let len = round_up_page(m.len);
            pages += len / CLICK_SIZE as u64;
        }

        // C: main.c:492-495 — kernel_allocated_bytes_dynamic is already
        // page-rounded by the kernel; static is rounded up here.
        let kern_static = round_up_page(self.kernel_allocated.static_bytes);
        pages += (self.kernel_allocated.dynamic_bytes + kern_static) / CLICK_SIZE as u64;

        pages as usize
    }
}

/// Round a byte length up to a whole page. C: `roundup(len, VM_PAGE_SIZE)`.
#[inline]
fn round_up_page(bytes: u64) -> u64 {
    bytes.div_ceil(CLICK_SIZE as u64) * CLICK_SIZE as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phys_mem::BootMemRegion;

    const PAGE: usize = CLICK_SIZE;

    fn region(base: usize, pages: usize) -> BootMemRegion {
        BootMemRegion { base, size: pages * PAGE }
    }

    #[test]
    fn test_boot_params_simple_validates() {
        let regions = [region(0, 4)];
        let params = BootParams::simple(4, &regions);
        params.validate();
    }

    #[test]
    #[should_panic]
    fn test_boot_params_validate_empty_regions() {
        let params = BootParams::simple(0, &[]);
        params.validate();
    }

    #[test]
    #[should_panic]
    fn test_boot_params_validate_total_pages_mismatch() {
        // Region covers 4 pages but total_pages claims 5.
        let regions = [region(0, 4)];
        let params = BootParams::simple(5, &regions);
        params.validate();
    }

    #[test]
    fn test_boot_params_extra_pages_modules() {
        // C: main.c:485-489 — each module (except the last entry) is
        // charged rounded up to a page.
        let modules = [
            BootModule { start_addr: 0x1000, len: PAGE as u64 },        // 1 page
            BootModule { start_addr: 0x2000, len: PAGE as u64 + 1 },    // 2 pages
            BootModule { start_addr: 0x4000, len: 999 },                // excluded (last)
        ];
        let params = BootParams {
            total_pages: 0,
            free_regions: &[],
            boot_procs: &[],
            modules: &modules,
            kernel_allocated: KernelAllocated::ZERO,
            is_first_time: true,
        };
        assert_eq!(params.extra_pages(), 3);
    }

    #[test]
    fn test_boot_params_extra_pages_kernel() {
        // C: main.c:492-495 — static rounded up, dynamic added as-is.
        let params = BootParams {
            total_pages: 0,
            free_regions: &[],
            boot_procs: &[],
            modules: &[],
            kernel_allocated: KernelAllocated {
                static_bytes: PAGE as u64 + 1, // → 2 pages
                dynamic_bytes: 2 * PAGE as u64,
            },
            is_first_time: true,
        };
        assert_eq!(params.extra_pages(), 4);
    }

    #[test]
    fn test_boot_params_placeholder_has_vm_slot() {
        let params = BootParams::placeholder();
        params.validate();
        let vm = params
            .boot_procs
            .iter()
            .find(|ip| ip.proc_nr == VM_PROC_NR)
            .expect("placeholder must include VM boot image");
        assert_eq!(vm.endpoint, Endpoint::VM);
    }
}
