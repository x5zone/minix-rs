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

use minix_arch::paging::Paging;
use minix_arch::paging_ext::HugePages;
use minix_types::{KernelInfo, VirBytes, PhysBytes};
use minix_arch::paging::PageFlags;

pub mod vm;
pub mod proc;

pub use core::panic::PanicInfo;

// ── Boot entry points ──

/// Architecture-specific boot entry (called by boot-uefi after ExitBootServices).
/// Selects the correct `Paging` implementation at compile time.
#[cfg(target_arch = "x86_64")]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::x86_64::paging::X86_64Paging;
    arch_boot_impl::<X86_64Paging>(kernel_info, root_page)
}

#[cfg(all(test, feature = "mock"))]
pub fn arch_boot_test(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::paging::mock::MockPaging;
    arch_boot_impl::<MockPaging>(kernel_info, root_page)
}

/// Generic boot implementation — works for any `HugePages` impl.
///
/// `P: HugePages` implies `P: Paging`, so we get `new_empty`, `enable`, `map`
/// from `Paging` plus `HUGE_PAGE_SIZE` and `map_huge` from `HugePages`.
///
/// C: pg_clear() + pg_identity() + pg_mapkernel() + pg_load() + vm_enable_paging()
///    pre_init.c:268-271, pg_utils.c:162/186/204/247
pub fn arch_boot_impl<P: HugePages>(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    let mut paging = P::new_empty(root_page);

    let huge_size = P::HUGE_PAGE_SIZE as usize;

    // Step 1: Identity mapping — VA = PA for all free physical memory.
    // C: pg_identity(&kinfo) — pg_utils.c:162
    for region in kernel_info.memmap {
        let mut addr = region.base.0;
        let end = addr + region.len as u64;
        while addr < end {
            paging.map_huge(
                VirBytes(addr), PhysBytes(addr),
                huge_size, PageFlags::read_write(),
            ).expect("identity map: map_huge failed — region is valid");
            addr += huge_size as u64;
        }
    }

    // Step 2: Kernel high-address mapping.
    // C: pg_mapkernel() — pg_utils.c:186
    let kern_virt = kernel_info.kern_virt_base.0;
    let kern_phys = kernel_info.kern_phys_base.0;
    let mut offset = 0u64;
    while offset < kernel_info.kern_size as u64 {
        paging.map_huge(
            VirBytes(kern_virt + offset), PhysBytes(kern_phys + offset),
            huge_size, PageFlags::kernel_read_write(),
        ).expect("kernel map: map_huge failed — kernel image mismatch");
        offset += huge_size as u64;
    }

    // Step 3: Enable paging.
    // SAFETY: Steps 1+2 set up identity mapping covering current RIP.
    let _root_phys = unsafe { paging.enable() };

    // Step 4: Enter kmain.
    kmain(kernel_info)
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
        let mut paging = MockPaging::new_empty(root_page);

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
        let mut paging = MockPaging::new_empty(PhysBytes(0x1000));

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
