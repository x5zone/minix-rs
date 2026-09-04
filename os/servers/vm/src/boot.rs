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

use minix_types::{BootImage, Endpoint, HandoffMemRegion, NR_BOOT_PROCS, PhysBytes};
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
    /// Physical address of VM's bootstrap page-table root (A1 adoption).
    ///
    /// The kernel builds and enables this root before scheduling VM and
    /// hands its physical address over via the boot handoff page
    /// (`minix_types::VmBootHandoff`, mapped user read-only at
    /// `VM_BOOT_HANDOFF_VA`). Consumed by `init_vm_self_pt`: VM adopts
    /// this root as its own page table instead of creating a fresh one,
    /// so kernel-built mappings (VM DM window, handoff page, ELF) remain
    /// visible in VM's address space.
    pub root_paddr: PhysBytes,
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
    /// Bytes the kernel allocated to load the VM image itself.
    ///
    /// C: `kernel_boot_info.vm_allocated_bytes` (minix/include/minix/param.h:44),
    /// consumed by `get_usage_info_vm` (region.c:1369) for the VM-self
    /// usage query.
    pub vm_allocated_bytes: u64,
    /// Whether this is a fresh boot.
    ///
    /// C: `is_first_time()` (main.c:79-88) — returns true when RS still
    /// holds `RTS_BOOTINHIBIT`, which gates `init_vm()` in `main()`.
    pub is_first_time: bool,
}

impl<'a> BootParams<'a> {
    /// Single-region boot params with no modules and no boot processes.
    ///
    /// Intended for unit tests that only exercise allocator/dispatch
    /// paths. Production code must use real boot parameters.
    pub fn simple(total_pages: usize, free_regions: &'a [BootMemRegion]) -> Self {
        Self {
            // Fake-but-valid root: these tests never exercise adoption.
            root_paddr: PhysBytes(0x900_000),
            total_pages,
            free_regions,
            boot_procs: &[],
            modules: &[],
            kernel_allocated: KernelAllocated::ZERO,
            vm_allocated_bytes: 0,
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
        // A1 handoff contract: the root must be a real, page-aligned
        // physical page (mirrors `VmBootHandoff::validate`).
        assert!(
            self.root_paddr.0 != 0 && self.root_paddr.0 & 0xFFF == 0,
            "BootParams: root_paddr 0x{:x} must be a non-zero page-aligned physical address",
            self.root_paddr.0
        );

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

/// Read the full boot parameter set from the kernel→VM boot handoff page.
///
/// The kernel writes a `minix_types::VmBootHandoff` page and maps it user
/// read-only at `minix_types::VM_BOOT_HANDOFF_VA` before scheduling VM:
/// `root_paddr` is the A1 address-space identity hand-off (VM's initial
/// page table IS the bootstrap root), `free_regions`/`deducted` are the
/// A2 post-bootstrap classification output (kernel cut `LiveBootstrap`
/// out of the full memmap; see `07-paging_init_design` §6.0-A2), and the
/// boot tables + kernel footprint fill in the C `kernel_boot_info` role.
///
/// This runs once before any heap-consuming server setup; the converted
/// region/module slices are leaked (one-shot boot data, C keeps the
/// equivalent tables in BSS for the process lifetime).
///
/// # Panics
///
/// Panics if the page fails header validation or if the A2 reconciliation
/// ([`reconcile`]) fails — boot contract violations, not recoverable
/// errors.
pub fn read_boot_params() -> BootParams<'static> {
    // SAFETY: the kernel guarantees the handoff page is mapped at
    // VM_BOOT_HANDOFF_VA in VM's initial address space before VM is
    // scheduled; the mapping is user read-only and its contents are fixed
    // for VM's lifetime (one-way, one-shot boot contract). VM runs
    // single-threaded and reads it before any other boot-contract use.
    let handoff =
        unsafe { &*(minix_types::VM_BOOT_HANDOFF_VA as *const minix_types::VmBootHandoff) };
    handoff.validate();

    // A2 free list: kernel-cut survivors, already clipped to VM's DM
    // window (VM PMM eligible = conventional ∩ DM-representable −
    // LiveBootstrap).
    let free_regions: &'static [BootMemRegion] = {
        let v: alloc::vec::Vec<BootMemRegion> =
            handoff.free_regions[..handoff.free_region_count as usize]
                .iter()
                .map(|r| BootMemRegion {
                    base: r.base as usize,
                    size: r.size as usize,
                })
                .collect();
        alloc::boxed::Box::leak(v.into_boxed_slice())
    };
    let modules: &'static [BootModule] = {
        let v: alloc::vec::Vec<BootModule> =
            handoff.modules[..handoff.module_count as usize]
                .iter()
                .map(|m| BootModule {
                    start_addr: m.start_addr,
                    len: m.len,
                })
                .collect();
        alloc::boxed::Box::leak(v.into_boxed_slice())
    };

    let params = BootParams {
        root_paddr: PhysBytes::new(handoff.root_paddr),
        // C: total_pages accumulated by mem_init() from the memory chunks
        // (alloc.c:319-331) — the handoff free list IS the chunk list.
        total_pages: free_regions.iter().map(|r| r.size / CLICK_SIZE).sum(),
        free_regions,
        boot_procs: &handoff.boot_procs[..],
        modules,
        kernel_allocated: KernelAllocated {
            static_bytes: handoff.kernel_allocated_static,
            dynamic_bytes: handoff.kernel_allocated_dynamic,
        },
        vm_allocated_bytes: handoff.vm_allocated_bytes,
        is_first_time: handoff.is_first_time != 0,
    };
    params.validate();
    reconcile(&params, &handoff.deducted[..handoff.deducted_count as usize]);
    params
}

/// Page size of every handoff range (C: I386_PAGE_SIZE).
const PAGE: u64 = 0x1000;

/// A2 consumer-boundary reconciliation (07-paging_init_design §6.0-A2,
/// Proof 2 — "deducted set == record" audit).
///
/// The kernel builds the free list by cutting `LiveBootstrap(t_classify)`
/// from the full memmap (post-bootstrap classification, by construction)
/// and hands the deduction record over in the same page. VM cannot see
/// the pre-cut memmap, so the record is verified against everything VM
/// can enumerate independently:
///
/// 1. the record is non-empty (kernel image + root page are always
///    deducted);
/// 2. the adopted root page is inside the record — A1 identity, VM knows
///    `root_paddr` from the handoff itself;
/// 3. every reserved boot-module blob (boot-image entries other than
///    VM's reclaimed one) is inside the record;
/// 4. `free ∩ deducted = ∅` — the Proof 2 invariant at the consumption
///    point.
///
/// Runs before VM PMM construction, so the allocator's first allocation
/// cannot precede the reconciliation (`adopt → reconcile → PMM
/// enabled`).
fn reconcile(params: &BootParams, deducted: &[HandoffMemRegion]) {
    assert!(
        !deducted.is_empty(),
        "reconcile: empty LiveBootstrap record — kernel did not hand over the deduction list"
    );
    let covered = |addr: u64, len: u64| {
        deducted
            .iter()
            .any(|r| addr >= r.base && addr + len <= r.base + r.size)
    };

    // (2) root page — A1 identity hand-off must be part of the record.
    assert!(
        covered(params.root_paddr.0, PAGE),
        "reconcile: root page 0x{:x} not covered by the deduction record",
        params.root_paddr.0
    );

    // (3) reserved module blobs. Kernel tasks carry no blob (len == 0);
    // VM's own blob was reclaimed after the ELF copy
    // (C: protect.c:450-451) and therefore stays out of the record.
    for ip in params.boot_procs {
        if ip.len == 0 || ip.proc_nr == VM_PROC_NR {
            continue;
        }
        assert!(
            covered(ip.start_addr, ip.len),
            "reconcile: boot module [{:#x},{:#x}) (proc {}) not covered by the deduction record",
            ip.start_addr,
            ip.start_addr + ip.len,
            ip.proc_nr
        );
    }

    // (4) LiveBootstrap ∩ VM-free = ∅ (Proof 2 at the consumption point).
    for fr in params.free_regions {
        let fb = fr.base as u64;
        let fe = fb + fr.size as u64;
        for dr in deducted {
            assert!(
                fe <= dr.base || dr.base + dr.size <= fb,
                "reconcile: free [{fb:#x},{fe:#x}) overlaps deducted [{:#x},{:#x})",
                dr.base,
                dr.base + dr.size
            );
        }
    }
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
    #[should_panic]
    fn test_boot_params_validate_misaligned_root() {
        // A1 handoff contract: root must be page-aligned.
        let regions = [region(0, 4)];
        let mut params = BootParams::simple(4, &regions);
        params.root_paddr = PhysBytes(0x900_123);
        params.validate();
    }

    #[test]
    #[should_panic]
    fn test_boot_params_validate_zero_root() {
        // A1 handoff contract: root must be non-zero (no adopted root is
        // ever at physical 0).
        let regions = [region(0, 4)];
        let mut params = BootParams::simple(4, &regions);
        params.root_paddr = PhysBytes(0);
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
            root_paddr: PhysBytes(0x900_000),
            total_pages: 0,
            free_regions: &[],
            boot_procs: &[],
            modules: &modules,
            kernel_allocated: KernelAllocated::ZERO,
            vm_allocated_bytes: 0,
            is_first_time: true,
        };
        assert_eq!(params.extra_pages(), 3);
    }

    #[test]
    fn test_boot_params_extra_pages_kernel() {
        // C: main.c:492-495 — static rounded up, dynamic added as-is.
        let params = BootParams {
            root_paddr: PhysBytes(0x900_000),
            total_pages: 0,
            free_regions: &[],
            boot_procs: &[],
            modules: &[],
            kernel_allocated: KernelAllocated {
                static_bytes: PAGE as u64 + 1, // → 2 pages
                dynamic_bytes: 2 * PAGE as u64,
            },
            vm_allocated_bytes: 0,
            is_first_time: true,
        };
        assert_eq!(params.extra_pages(), 4);
    }

    /// A valid record for reconcile tests: covers the simple() root page.
    fn root_record(root: PhysBytes) -> Vec<HandoffMemRegion> {
        vec![HandoffMemRegion {
            base: root.0,
            size: 0x1000,
        }]
    }

    #[test]
    fn test_reconcile_accepts_consistent_record() {
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        reconcile(&params, &root_record(params.root_paddr));
    }

    #[test]
    #[should_panic(expected = "empty LiveBootstrap record")]
    fn test_reconcile_rejects_empty_record() {
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        reconcile(&params, &[]);
    }

    #[test]
    #[should_panic(expected = "root page")]
    fn test_reconcile_rejects_root_not_in_record() {
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        // Record covers some other page, not the root.
        let record = vec![HandoffMemRegion {
            base: 0x500000,
            size: 0x1000,
        }];
        reconcile(&params, &record);
    }

    #[test]
    #[should_panic(expected = "overlaps deducted")]
    fn test_reconcile_rejects_free_deducted_overlap() {
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        // Record covers the root AND claims part of the free region.
        let record = vec![
            HandoffMemRegion {
                base: params.root_paddr.0,
                size: 0x1000,
            },
            HandoffMemRegion {
                base: 0x100000,
                size: 0x2000,
            },
        ];
        reconcile(&params, &record);
    }

    #[test]
    #[should_panic(expected = "not covered by the deduction record")]
    fn test_reconcile_rejects_module_not_in_record() {
        // A boot image with a blob (RS) whose physical range the record
        // does not cover.
        let regions = [region(0x100000, 4)];
        let params = BootParams::simple(4, &regions);
        let mut record = root_record(params.root_paddr);
        record.push(HandoffMemRegion {
            base: 0x300000,
            size: 0x1000,
        });
        // params.boot_procs is empty in simple(); exercise the module
        // check through a boot-image slice on a locally built params.
        let boot_procs = [BootImage {
            proc_nr: 2, // RS_PROC_NR — not VM
            proc_name: [0; 16],
            endpoint: Endpoint(2),
            start_addr: 0x700000,
            len: 0x1000,
        }];
        let params = BootParams {
            boot_procs: &boot_procs,
            ..params
        };
        reconcile(&params, &record);
    }
}
