//! Full boot-chain test kernel — exercises the real Riscv64Paging trait via OpenSBI.
//!
//! On riscv64, the boot path is:
//!   OpenSBI firmware → this kernel (loaded via -kernel) →
//!   OpenSbiBootShim (BootShim trait) → arch_boot_impl → test output
//!
//! Unlike x86_64/aarch64 which use UEFI, riscv64 uses OpenSBI directly.
//! There is no `riscv64-unknown-uefi` Rust target, so this kernel is
//! a bare-metal binary (not a .efi file), loaded by QEMU's -kernel flag.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::riscv64::paging::Riscv64Paging;
use minix_arch::riscv64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::BootShim;
use boot_shim::OpenSbiBootShim;

// ── Global allocator (bump allocator on a static heap) ──
// UEFI targets get this from the `uefi` crate's `global_allocator` feature.
// riscv64 has no UEFI, so we provide a simple bump allocator here.
// This is only used during early boot for `alloc` operations in
// minix-kernel/minix-arch (e.g., Vec, Box). After the kernel's
// proper allocator is initialized, this is no longer used.

use core::alloc::{GlobalAlloc, Layout};

/// 64KB static heap region for early boot allocations.
/// This is enough for the few allocations that happen before
/// arch_boot_impl completes (e.g., building page table structures).
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
            let heap_len = 0x10000; // HEAP.len()
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

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator — never deallocates during boot
    }
}

#[global_allocator]
static ALLOCATOR: BootAllocator = BootAllocator;

// ── Boot assembly: set up stack pointer and jump to Rust ──

core::arch::global_asm!(
    ".section .text.init",
    ".global _start",
    "_start:",
    // Set up stack pointer — use top of DRAM (0x8000_0000 + 128MB = 0x8800_0000)
    // QEMU virt default RAM is 128MB. Stack grows downward.
    "    la sp, __stack_top",
    // Jump to Rust entry point
    "    call rust_main",
    // If rust_main returns, halt
    "1:",
    "    wfi",
    "    j 1b",
);

/// Stack: 64KB, placed in BSS. Top is at __stack_top.
static mut STACK: [u8; 0x10000] = [0u8; 0x10000];

/// Rust entry point — called by _start assembly after stack is set up.
#[no_mangle]
pub extern "C" fn rust_main() -> ! {
    early_console::write_str("### Booting Minix-RS hello-boot (riscv64, Paging trait)...\n");

    // 1. OpenSBI boot preparation via BootShim trait
    let result = OpenSbiBootShim::prepare_boot(0, 0, 0, 8);

    // 2. Register boot-stage page table allocator
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);

    // 3. Run arch_boot_impl — identity map + enable Sv39
    let _info = minix_kernel::arch_boot_impl::<Riscv64Paging>(&result.kernel_info, result.root_page);

    // If we reach here, paging is enabled and the CPU can still execute.
    early_console::write_str("Hello, World!\n");
    early_console::write_str("  arch:           riscv64\n");
    early_console::write_str("  paging:         Riscv64Paging (Paging trait)\n");
    early_console::write_str("  firmware:       OpenSBI\n");
    early_console::write_str("  identity_map:   via arch_boot_impl\n");
    early_console::write_str("### TEST_RESULT: PASS hello-boot-riscv64 ###\n");

    loop {
        unsafe { asm!("wfi", options(nomem, nostack)); }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop {
        unsafe { asm!("wfi", options(nomem, nostack)); }
    }
}

// Export stack top address for assembly
core::arch::global_asm!(
    ".global __stack_top",
    ".set __stack_top, {stack_top} + 0x10000",
    stack_top = sym STACK,
);
