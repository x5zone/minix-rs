//! Test: aarch64 semantic shutdown through the QEMU test backend — the
//! aarch64 leg of the three-arch shutdown verification. §3.8 two-layer
//! rule: this exercises the QEMU test layer, semihosting `SYS_EXIT`; the
//! real-hardware layer is PSCI SYSTEM_OFF.
//!
//! Boot: AAVMF → this .efi → production `arch_boot_impl` paging bring-up
//! (so the shutdown happens from a realistic post-MMU state), then
//! `minix_kernel::minix_shutdown(0)` → `minix_plat::shutdown_qemu` →
//! `hlt #0xF000` with the A64 semihosting SYS_EXIT argument block.
//!
//! PASS = the run script sees the serial marker AND QEMU terminates on its
//! own with exit code 0 (a guest that merely hangs cannot fake this — the
//! script's timeout would fire instead).

#![no_std]
#![no_main]

use minix_arch::arm64::paging::AArch64Paging;
use minix_arch::pt_alloc;
use minix_plat::arm64::early_console;
use minix_kernel::boot_alloc;
use core::panic::PanicInfo;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{KernelInfo};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

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

    early_console::write_str("### test_shutdown (aarch64): semihosting SYS_EXIT backend\n");

    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);



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
        source_records: &[],
        authority: minix_boot::RamAuthority::UEFI,
    };

    boot_alloc::init_boot_pt_alloc(bump_base, bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let _info = minix_kernel::arch_boot_impl::<minix_arch::arch::boot_arch::AArch64BootArch>(&kernel_info, root_page);
    early_console::write_str("  paging enabled (post-MMU shutdown path)\n");

    early_console::write_str("  requesting shutdown via semihosting SYS_EXIT\n");
    early_console::write_str("### TEST_RESULT: PASS test-shutdown-aarch64 ###\n");
    minix_kernel::minix_shutdown(0);

    #[allow(unreachable_code)]
    loop { unsafe { core::arch::asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-shutdown-aarch64 ###\n");
    loop { unsafe { core::arch::asm!("wfe", options(nomem, nostack)); } }
}
