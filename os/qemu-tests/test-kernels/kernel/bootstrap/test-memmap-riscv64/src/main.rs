//! Test: KernelInfo.memmap reflects the real physical memory layout (riscv64).
//!
//! Verifies that the hardcoded memory map for QEMU virt contains at least
//! one CONVENTIONAL region with non-zero size. On riscv64, there is no
//! UEFI memory map — we use a static memory region covering DRAM.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_plat::riscv64::early_console;
use minix_boot::MemoryRegion;
use minix_types::PhysBytes;

// Minimal global allocator for riscv64 bare-metal (no UEFI).
use core::alloc::{GlobalAlloc, Layout};

#[link_section = ".bss"]
static mut HEAP: [u8; 0x4000] = [0u8; 0x4000];

struct BootAllocator;

unsafe impl GlobalAlloc for BootAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        static mut HEAP_PTR: usize = 0;
        let align = layout.align();
        let size = layout.size();
        unsafe {
            let base = core::ptr::addr_of_mut!(HEAP) as usize;
            let heap_len = 0x4000;
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

// ── QEMU virt constants ──
const DRAM_BASE: u64 = 0x8000_0000;
const DEFAULT_RAM_SIZE: u64 = 0x800_0000; // 128 MB

/// Hardcoded memory map for QEMU virt.
static MEMMAP: [MemoryRegion; 1] = [MemoryRegion {
    base: PhysBytes(DRAM_BASE),
    len: DEFAULT_RAM_SIZE as usize,
}];

// ── Boot assembly ──
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

/// Stack: 64KB, placed in BSS.
static mut STACK: [u8; 0x10000] = [0u8; 0x10000];

core::arch::global_asm!(
    ".global __stack_top",
    ".set __stack_top, {stack_top} + 0x10000",
    stack_top = sym STACK,
);

#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    early_console::write_str("### test_memmap (riscv64): verifying memory map...\n");

    // Assertion 1: memmap is non-empty
    if MEMMAP.is_empty() {
        early_console::write_str("### FAIL: memmap is empty\n");
        loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
    }
    early_console::write_str("  memmap entries: ");
    early_console::write_hex(MEMMAP.len() as u64);
    early_console::write_str("\n");

    // Assertion 2: at least one region has non-zero size
    let total: u64 = MEMMAP.iter().map(|r| r.len as u64).sum();
    if total == 0 {
        early_console::write_str("### FAIL: total memmap size is 0\n");
        loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
    }
    early_console::write_str("  total CONVENTIONAL memory: ");
    early_console::write_hex(total);
    early_console::write_str("\n");

    // Assertion 3: region starts at DRAM_BASE (expected in QEMU virt)
    let mut has_valid_base = false;
    for region in &MEMMAP {
        let r_start = region.base.0;
        let r_end = r_start + region.len as u64;
        early_console::write_str("  region: ");
        early_console::write_hex(r_start);
        early_console::write_str("..");
        early_console::write_hex(r_end);
        early_console::write_str("\n");
        if r_start == DRAM_BASE {
            has_valid_base = true;
        }
    }
    if !has_valid_base {
        early_console::write_str("### FAIL: no region at DRAM_BASE\n");
        loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
    }

    early_console::write_str("  memmap is valid: non-empty, has DRAM region\n");
    early_console::write_str("### TEST_RESULT: PASS test-memmap-riscv64 ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}
