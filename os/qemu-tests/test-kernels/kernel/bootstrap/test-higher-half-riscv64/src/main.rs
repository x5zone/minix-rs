//! Test: higher-half kernel transition (riscv64).
//!
//! Verifies that after Sv39 MMU is enabled and the HigherHalf trait
//! performs the stack/PC switch, execution reaches kmain at a high
//! virtual address with correct stack alignment and frame pointer.
//!
//! Boot flow: OpenSBI → this kernel (loaded via -kernel) →
//!   identity mapping → kernel mapping → enable MMU →
//!   HigherHalf::jump_to_kmain → kmain (qemu_test) → verify SP/PC/FP
//!
//! See hello-boot-riscv64/main.rs for rationale on why individual helpers
//! are used instead of `OpenSbiBootShim::prepare_boot`.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_plat::riscv64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo, MemoryRegion};

// ── Global allocator (bump allocator on a static heap) ──
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

// ── QEMU virt constants ──
const DRAM_BASE: u64 = 0x8000_0000;
const DEFAULT_RAM_SIZE: u64 = 0x800_0000; // 128 MB

// ── Boot assembly: set up stack pointer and jump to Rust ──
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

/// Bump allocator state for root_page and bump_region.
static mut BUMP_PTR: u64 = DRAM_BASE + 0x0200_0000; // DRAM + 32 MB
const BUMP_END: u64 = DRAM_BASE + 0x0400_0000;       // DRAM + 64 MB

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

/// Rust entry point — called by _start after stack setup.
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    early_console::write_str("### test_higher_half (riscv64): verifying higher-half transition...\n");

    let memmap: &'static [MemoryRegion] = &MEMMAP;

    let root_page = PhysBytes(bump_alloc(1).expect("bump alloc: root page"));
    let bump_pages = 8;
    let bump_base = bump_alloc(bump_pages).expect("bump alloc: bump region");
    let bump_end = bump_base + (bump_pages as u64) * 4096;

    // Use the real higher-half virtual address (Sv39 canonical high).
    // KERN_VIRT_BASE = 0xFFFFFFC000000000 (VPN[2] = 256, 1GB-aligned).
    // arch_boot_impl will create the mapping:
    //   0xFFFF_FFC0_0000_0000 → 0x8000_0000 (2MB huge page, since kern_phys_base=0x80200000 not 1GB-aligned)
    // After HigherHalf::jump_to_kmain, SP will be at a high virtual address,
    // same as x86_64/aarch64. PC remains at a low address because QEMU -kernel
    // loads the test binary at 0x8020_0000 (identity mapping keeps it accessible).
    let kern_virt_base: u64 = 0xFFFF_FFC0_0000_0000; // Sv39 canonical high
    // kern_size must be a multiple of 2MB (FALLBACK_HUGE_PAGE_SIZE for riscv64)
    // since kern_phys_base=0x80200000 is not 1GB-aligned.
    // We need at least 2MB for the kernel image + stack space.
    // 4MB gives us 2MB for code + 2MB for stack/heap, all mapped.
    let kern_size: u64 = 0x400_000; // 4MB (2 × 2MB huge pages)

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(kern_virt_base),
        kern_phys_base: PhysBytes(DRAM_BASE),
        kern_size,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_003f_ffff_f000),
        // Stack top at end of mapped kernel region (same pattern as x86_64/aarch64).
        kern_stack_top: VirBytes(kern_virt_base + kern_size as u64),
        syscall_entry: VirBytes(kern_virt_base + 0x100_000),
        boot_modules: &[],
        bootstrap_start: PhysBytes(0),
        bootstrap_len: 0,
    };

    let result = BootPrepareResult {
        kernel_info,
        root_page,
        bump_base,
        bump_end,
    };

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. arch_boot performs identity map + kernel map + enable MMU,
    //    then calls HigherHalf::jump_to_kmain → kmain (qemu_test).
    minix_kernel::arch_boot(&result.kernel_info, result.root_page);

    // NOTREACHED — arch_boot calls HigherHalf::jump_to_kmain which never returns.
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-higher-half-riscv64 ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}