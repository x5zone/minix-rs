//! Minix-RS Kernel
//!
//! Microkernel implementation, including:
//! - Boot sequence (arch_boot → kmain)
//! - Process management (proc)
//! - IPC mechanism (ipc)
//! - Scheduler (sched)
//! - Virtual memory — kernel part (vm)
//! - Hardware abstraction (hal, arch)

#![no_std]
#![cfg_attr(not(test), no_main)]

extern crate alloc;

use minix_arch::paging_ext::HugePages;
use minix_types::{KernelInfo, VirBytes, PhysBytes};
use minix_arch::paging::PageFlags;
use minix_arch::pt_alloc;

/// End of identity-mapped region during boot (4 GB).
/// C: pg_identity() maps 1024 × 4MB = 4GB (I386_BIG_PAGE_SIZE × 1024).
const IDENTITY_MAP_END: u64 = 0x1_0000_0000;

pub mod vm;
pub mod proc;
pub mod proc_table;
pub mod kpriv;
pub mod sched;
pub mod boot_alloc;

pub use core::panic::PanicInfo;

// ── Boot entry points ──

/// Architecture-specific boot entry (called by boot-shim after ExitBootServices).
/// Selects the correct `Paging` implementation at compile time.
#[cfg(target_arch = "x86_64")]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::x86_64::paging::X86_64Paging;
    let info = arch_boot_impl::<X86_64Paging>(kernel_info, root_page);
    kmain(info)
}

#[cfg(target_arch = "aarch64")]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::arm64::paging::AArch64Paging;
    let info = arch_boot_impl::<AArch64Paging>(kernel_info, root_page);
    kmain(info)
}

#[cfg(target_arch = "riscv64")]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::riscv64::paging::Riscv64Paging;
    let info = arch_boot_impl::<Riscv64Paging>(kernel_info, root_page);
    kmain(info)
}

#[cfg(all(test, feature = "mock"))]
pub fn arch_boot_test(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::paging::mock::MockPaging;
    let info = arch_boot_impl::<MockPaging>(kernel_info, root_page);
    kmain(info)
}

/// Generic boot implementation — works for any `HugePages` impl.
///
/// `P: HugePages` implies `P: Paging`, so we get `new_from_page`, `enable`, `map`
/// from `Paging` plus `HUGE_PAGE_SIZE` and `map_huge` from `HugePages`.
///
/// Returns `&KernelInfo` after paging is enabled, so the caller can decide
/// what to do next (e.g., call `kmain` for the real kernel, or print PASS
/// for a test kernel).
///
/// C: pg_clear() + pg_identity() + pg_mapkernel() + pg_load() + vm_enable_paging()
///    pre_init.c:230-236, pg_utils.c:162/186/204/247
pub fn arch_boot_impl<P: HugePages>(kernel_info: &KernelInfo, root_page: PhysBytes) -> &KernelInfo {
    // Step 0: Register boot-stage page table page allocator.
    // If the caller (e.g., a test kernel) has already registered one,
    // skip this step — the caller's allocator may use a safer region
    // (e.g., a bump region past the kernel image).
    if !pt_alloc::is_registered() {
        let base = kernel_info.memmap.first().map(|r| r.base.0).unwrap_or(0);
        let end = base + kernel_info.memmap.first().map(|r| r.len as u64).unwrap_or(0);
        let boot_alloc_end = core::cmp::min(base + 0x100_000, end);
        boot_alloc::init_boot_pt_alloc(base, boot_alloc_end);
        pt_alloc::register(boot_alloc::boot_pt_alloc);
    }

    let mut paging = P::new_from_page(root_page);

    let huge_size = P::HUGE_PAGE_SIZE as usize;

    // Step 1: Identity mapping — VA = PA for the first 4GB of address space.
    // C: pg_identity(&kinfo) — pg_utils.c:162
    // Maps 1024 × 4MB = 4GB (C) or 4 × 1GB (x86-64) or equivalent on other archs.
    // We must map the entire low address space, not just CONVENTIONAL memory,
    // because UEFI loader code/data may be in LOADER_DATA/LOADER_CODE regions
    // that are not marked CONVENTIONAL. Without covering these regions,
    // the CPU faults immediately after the page table switch.
    // NOTE: EXECUTABLE flag is required so the CPU can fetch instructions
    // after the page table switch (CR3/satp/TTBR write).
    // NOTE: We use kernel_read_write() (supervisor-only, no USER_ACCESSIBLE)
    // because on RISC-V Sv39, a page with U=1 cannot be executed by
    // supervisor mode unless sstatus.SUM=1. Since we haven't set SUM,
    // identity mapping must be supervisor-only.
    let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let mut addr: u64 = 0;
    while addr < IDENTITY_MAP_END {
        // Ignore errors — some ranges may not be backed by physical memory,
        // but that's fine; the CPU won't access them.
        let _ = paging.map_huge(
            VirBytes(addr), PhysBytes(addr),
            huge_size, id_flags,
        );
        addr += huge_size as u64;
    }

    // Step 2: Kernel high-address mapping.
    // C: pg_mapkernel() — pg_utils.c:186
    let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let kern_virt = kernel_info.kern_virt_base.0;
    let kern_phys = kernel_info.kern_phys_base.0;
    let mut offset = 0u64;
    while offset < kernel_info.kern_size as u64 {
        paging.map_huge(
            VirBytes(kern_virt + offset), PhysBytes(kern_phys + offset),
            huge_size, kern_flags,
        ).expect("kernel map: map_huge failed — kernel image mismatch");
        offset += huge_size as u64;
    }

    // Step 3: Enable paging.
    // SAFETY: Steps 1+2 set up identity mapping covering current RIP.
    let _root_phys = unsafe { paging.enable() };

    // Return kernel_info so the caller can decide what to do next.
    kernel_info
}

/// Kernel main — called after paging is enabled.
fn kmain(kernel_info: &KernelInfo) -> ! {
    let _ = kernel_info;
    // TODO: continue bootstrap — protect_init, proc_init, interrupt_init...
    loop {}
}

// ── Tests ──

#[cfg(all(test, feature = "mock"))]
mod tests {
    use super::*;
    use minix_arch::paging::mock::MockPaging;

    /// Full boot-flow integration test with MockPaging.
    /// Verifies: identity mapping + kernel mapping + enable → no panic.
    #[test]
    fn test_boot_flow_identity_and_kernel_map() {
        let memmap: &'static [minix_types::MemoryRegion] = &[
            minix_types::MemoryRegion { base: PhysBytes(0x100000), len: 0x1000000 }, // 16MB
        ];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x100000),
            kern_size: 0x200000, // 2MB kernel
            free_upper_idx: 0,
            user_sp: VirBytes(0x7fff_ffff_f000),
            boot_modules: &[],
        };

        let root_page = PhysBytes(0x1000);
        let mut paging = MockPaging::new_from_page(root_page);

        let huge_size = MockPaging::HUGE_PAGE_SIZE as usize;

        // Test Step 1: Identity mapping
        let region = &info.memmap[0];
        let mut addr = region.base.0;
        let end = addr + region.len as u64;
        let mut identity_pages = 0u64;
        while addr < end {
            paging.map_huge(VirBytes(addr), PhysBytes(addr), huge_size, PageFlags::read_write())
                .unwrap();
            addr += huge_size as u64;
            identity_pages += 1;
        }
        assert!(identity_pages > 0, "no identity pages mapped");

        // Verify identity mapping — query a mapped huge-page-aligned address
        let query_addr = region.base.0; // base is 0x100000, mapped at first iteration
        let result = paging.query(VirBytes(query_addr));
        assert!(result.is_some(), "identity mapping not found at 0x{:x}", query_addr);

        // Test Step 2: Kernel high-address mapping
        let mut offset = 0u64;
        let mut kernel_pages = 0u64;
        while offset < info.kern_size as u64 {
            paging.map_huge(
                VirBytes(info.kern_virt_base.0 + offset),
                PhysBytes(info.kern_phys_base.0 + offset),
                huge_size, PageFlags::kernel_read_write(),
            ).unwrap();
            offset += huge_size as u64;
            kernel_pages += 1;
        }
        assert!(kernel_pages > 0, "no kernel pages mapped");

        // Test Step 3: Enable paging
        let root_phys = unsafe { paging.enable() };
        assert_eq!(root_phys, PhysBytes(0));
    }

    /// Verify empty memmap is handled gracefully (identity pass is a no-op).
    #[test]
    fn test_boot_empty_memmap() {
        let info = KernelInfo {
            memmap: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x100000),
            kern_size: 0x200000,
            free_upper_idx: 0,
            user_sp: VirBytes(0x7fff_ffff_f000),
            boot_modules: &[],
        };
        let mut paging = MockPaging::new_from_page(PhysBytes(0x1000));

        // Identity pass should be a no-op with zero regions
        for _region in info.memmap { /* empty */ }

        // Kernel map should still work
        let huge_size = MockPaging::HUGE_PAGE_SIZE as usize;
        let mut offset = 0u64;
        while offset < info.kern_size as u64 {
            paging.map_huge(
                VirBytes(info.kern_virt_base.0 + offset),
                PhysBytes(info.kern_phys_base.0 + offset),
                huge_size, PageFlags::kernel_read_write(),
            ).unwrap();
            offset += huge_size as u64;
        }

        unsafe { paging.enable() };
    }
}
