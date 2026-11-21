//! Test: paging.enable() switches page table base and CPU continues executing.
//!
//! Uses the same boot path as the real kernel (UEFI boot helpers → arch_boot_impl),
//! then prints PASS. Serial output after enable() proves the code path is
//! still reachable.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::x86_64::paging::X86_64Paging;
use minix_plat::x86_64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{KernelInfo};
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
    // I-43 二层修复：UEFI 固件把 PE 加载到运行期地址——内核的恒等窗口
    // 必须覆盖真实镜像而非硬编码基址（BOOT-SCOPE 2a 之后窗口是最小的）。
    let (image_lo, image_hi) = uefi_helpers::shim_image_window();
    let image_base = image_lo & !0x001F_FFFF;
    let image_end = (image_hi + 0x001F_FFFF) & !0x001F_FFFF;

    early_console::write_str("### test_paging_enable: boot → enable paging → verify alive\n");

    // 1. UEFI boot preparation — use individual helpers (no kernel.elf needed)
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);



    // --- Below this point, UEFI boot services are gone ---
    // I-29-impl 门④（路径 A）：保留退出函数返回的最终内存图并立即分类——
    // 分类所依据的图 = 退出所用的图（设计文档 §2.7）。
    let (memmap, _reserved_regions) = uefi_helpers::exit_boot_services_and_classify();
    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(image_base),
        kern_size: image_end - image_base,
        boot_modules: &[],
        platform_sources: &[],
        param_buf: &[],
        reserved_regions: &[],
        memmap_sources: &[],
        authority: minix_boot::RamAuthority::UEFI,
    };

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(bump_base, bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. Run arch_boot_impl — identity map + enable paging
    let _info = minix_kernel::arch_boot_impl::<minix_arch::arch::boot_arch::X86_64BootArch>(&kernel_info, root_page);

    // If we reach here, paging is enabled and the CPU can still execute.
    early_console::write_str("  paging enabled — CPU still executing\n");
    early_console::write_str("### TEST_RESULT: PASS test-paging-enable ###\n");

    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
