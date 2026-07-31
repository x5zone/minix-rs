//! Test: verify kernel mapping after arch_boot_impl (riscv64).
//!
//! After arch_boot_impl, the kernel's virtual address range
//! [kern_virt_base, kern_virt_base + kern_size) should be mapped to
//! [kern_phys_base, kern_phys_base + kern_size).
//!
//! Note: On riscv64 QEMU virt, the kernel is loaded at DRAM_BASE (0x8000_0000)
//! and kern_virt_base == kern_phys_base (identity mapping). We verify that
//! the mapping is valid by writing a sentinel and reading it back.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::riscv64::paging::Riscv64Paging;
use minix_plat::riscv64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo, MemoryRegion};

use core::alloc::{GlobalAlloc, Layout};

#[link_section = ".bss"]
static mut HEAP: [u8; 0x10000] = [0u8; 0x10000];

struct BootAllocator;

unsafe impl GlobalAlloc for BootAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        static mut HEAP_PTR: usize = 0;
        let align = layout.align();
        let size = layout.size();
        unsafe {
            let base = core::ptr::addr_of_mut!(HEAP) as usize;
            let heap_len = 0x10000;
            let current = HEAP_PTR;
            let aligned = (current + align - 1) & !(align - 1);
            let next = aligned + size;
            if next > heap_len {
                return core::ptr::null_mut();
            }
            HEAP_PTR = next;
            (base + aligned) as *mut u8
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BootAllocator = BootAllocator;

const DRAM_BASE: u64 = 0x8000_0000;
const DEFAULT_RAM_SIZE: u64 = 0x800_0000;

core::arch::global_asm!(
    ".section .text.entry",
    ".global _start",
    "_start:",
    "    la sp, __stack_top",
    "    call rust_main",
    "1:",
    "    wfi",
    "    j 1b",
);

static mut STACK: [u8; 0x10000] = [0u8; 0x10000];

core::arch::global_asm!(
    ".global __stack_top",
    ".set __stack_top, {stack_top} + 0x10000",
    stack_top = sym STACK,
);

static mut BUMP_PTR: u64 = DRAM_BASE + 0x0200_0000;
const BUMP_END: u64 = DRAM_BASE + 0x0400_0000;

static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
    base: PhysBytes(DRAM_BASE),
    len: DEFAULT_RAM_SIZE as usize,
}];

fn bump_alloc(num_pages: usize) -> Option<u64> {
    unsafe {
        let need = (num_pages as u64) * 4096;
        if BUMP_PTR + need > BUMP_END {
            return None;
        }
        let addr = BUMP_PTR;
        BUMP_PTR += need;
        Some(addr)
    }
}

/// Sentinel value written to memory, read back after mapping.
const SENTINEL: u64 = 0xdeadbeef_cafe0002;

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    early_console::write_str("### test_kernel_map (riscv64): verify kernel mapping\n");

    let memmap: &'static [MemoryRegion] = &MEMMAP;
    let root_page = PhysBytes(bump_alloc(1).expect("bump alloc: root page"));
    let bump_pages = 8;
    let bump_base = bump_alloc(bump_pages).expect("bump alloc: bump region");
    let bump_end = bump_base + (bump_pages as u64) * 4096;

    // On riscv64, kern_virt_base == kern_phys_base (identity mapping in Sv39)
    let kernel_info = KernelInfo {
        memmap,
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

    // Verify mapping: write sentinel and read it back.
    // Use an address well past the OpenSBI firmware (~322KB from 0x8000_0000)
    // and past the kernel image. 0x8020_1000 is ~2MB+4KB into DRAM.
    let test_addr = (DRAM_BASE + 0x200_000 + 0x1000) as *mut u64;

    unsafe {
        core::ptr::write_volatile(test_addr, SENTINEL);
    }

    early_console::write_str("  sentinel written: ");
    early_console::write_hex(SENTINEL);
    early_console::write_str("\n");

    let read_val = unsafe { core::ptr::read_volatile(test_addr) };
    early_console::write_str("  read back: ");
    early_console::write_hex(read_val);
    early_console::write_str("\n");

    if read_val == SENTINEL {
        early_console::write_str("  kernel mapping: OK\n");
        early_console::write_str("### TEST_RESULT: PASS test-kernel-map-riscv64 ###\n");
    } else {
        early_console::write_str("  kernel mapping: MISMATCH\n");
        early_console::write_str("### TEST_RESULT: FAIL test-kernel-map-riscv64 ###\n");
    }

    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}
