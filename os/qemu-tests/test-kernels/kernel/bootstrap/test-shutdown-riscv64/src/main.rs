//! Test: riscv64 semantic shutdown through the QEMU test backend
//! (edge1 K11 — the riscv64 leg; S-11 §3.8 two-layer rule: this exercises
//! the QEMU test layer, the `sifive_test` finisher at 0x100000;
//! the real-hardware layer is SBI SRST).
//!
//! Boot: OpenSBI (`-bios default`) → this ELF (`-kernel`) → production
//! `arch_boot_impl` paging bring-up, then
//! `minix_kernel::minix_shutdown(0)` → `minix_plat::shutdown_qemu` →
//! `FINISHER_PASS` write.
//!
//! PASS = the run script sees the serial marker AND QEMU terminates on its
//! own with exit code 0.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use core::alloc::{GlobalAlloc, Layout};

use minix_arch::riscv64::paging::Riscv64Paging;
use minix_arch::pt_alloc;
use minix_plat::riscv64::early_console;
use minix_kernel::boot_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo, MemoryRegion};

#[unsafe(link_section = ".bss")]
static mut HEAP: [u8; 0x10000] = [0u8; 0x10000];

struct BootAllocator;

unsafe impl GlobalAlloc for BootAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        static mut HEAP_PTR: usize = 0;
        unsafe {
            let base = core::ptr::addr_of_mut!(HEAP) as usize;
            let heap_len = 0x10000;
            let current = core::ptr::read_volatile(&raw const HEAP_PTR);
            let align = layout.align();
            let size = layout.size();
            let aligned = (current + align - 1) & !(align - 1);
            let next = aligned + size;
            if next > heap_len || aligned < current {
                return core::ptr::null_mut();
            }
            core::ptr::write_volatile(&raw mut HEAP_PTR, next);
            (base + aligned) as *mut u8
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BootAllocator = BootAllocator;

const DRAM_BASE: u64 = 0x8000_0000;

static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
    base: PhysBytes(DRAM_BASE),
    len: 0x800_0000,
}];

core::arch::global_asm!(
    ".section .text.init",
    ".global _start",
    "_start:",
    "    la t0, __bss_start",
    "    la t1, __bss_end",
    "1:",
    "    bgeu t0, t1, 2f",
    "    sd zero, 0(t0)",
    "    addi t0, t0, 8",
    "    j 1b",
    "2:",
    "    la sp, __stack_top",
    "    call rust_main",
    "1:",
    "    wfi",
    "    j 1b",
);

static mut STACK: [u8; 0x4000] = [0u8; 0x4000];

core::arch::global_asm!(
    ".global __stack_top",
    ".set __stack_top, {stack_top} + 0x4000",
    stack_top = sym STACK,
);

#[unsafe(no_mangle)]
extern "C" fn rust_main(_boot_hart: u64, _dtb_phys: u64) -> ! {
    early_console::write_str("### test_shutdown (riscv64): sifive_test finisher backend\n");

    static mut BUMP_PTR: u64 = DRAM_BASE + 0x0200_0000;
    const BUMP_END: u64 = DRAM_BASE + 0x0400_0000;
    fn bump(num_pages: usize) -> Option<u64> {
        unsafe {
            let need = (num_pages as u64) * 4096;
            let current = core::ptr::read_volatile(&raw const BUMP_PTR);
            if current + need > BUMP_END {
                return None;
            }
            core::ptr::write_volatile(&raw mut BUMP_PTR, current + need);
            Some(current)
        }
    }

    let root_page = PhysBytes(bump(1).expect("root page"));
    let bump_base = bump(8).expect("bump region");
    let bump_end = bump_base + 8 * 4096;

    let kernel_info = KernelInfo {
        memmap: &MEMMAP,
        kern_virt_base: VirBytes(DRAM_BASE),
        kern_phys_base: PhysBytes(DRAM_BASE),
        kern_size: 0x200_000,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_003f_ffff_f000),
        kern_stack_top: VirBytes(DRAM_BASE + 0x200_000),
        syscall_entry: VirBytes(DRAM_BASE),
        boot_modules: &[],
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
        platform_sources: &[],
        param_buf: &[],
    };

    let result = BootPrepareResult {
        kernel_info,
        root_page,
        bump_base,
        bump_end,
    };

    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let _info = minix_kernel::arch_boot_impl::<Riscv64Paging>(&result.kernel_info, result.root_page);
    early_console::write_str("  paging enabled (post-MMU shutdown path)\n");

    early_console::write_str("  requesting shutdown via sifive_test FINISHER_PASS\n");
    early_console::write_str("### TEST_RESULT: PASS test-shutdown-riscv64 ###\n");
    minix_kernel::minix_shutdown(0);

    #[allow(unreachable_code)]
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-shutdown-riscv64 ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}
