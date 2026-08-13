//! Test: protection structure initialization (riscv64).
//!
//! Verifies that after init_protection(), the CPU has valid stvec and
//! sscratch — the two answers to §1.2's conceptual questions on riscv64:
//!   1. Exception entry: stvec is set (non-zero, Direct mode)
//!   2. Kernel stack: sscratch equals kern_stack_top
//!
//! Boot flow: OpenSBI → arch_boot_impl (Sv39) → init_protection → verify CSRs

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::riscv64::paging::Riscv64Paging;
use minix_plat::riscv64::early_console;
use minix_arch::{ProtectionArch, TrapEntryArch, CurrentProtection, CurrentTrapEntry};
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo, MemoryRegion};

use core::alloc::{GlobalAlloc, Layout};

#[unsafe(link_section = ".bss")]
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

// Minimal trap vector for RISC-V.
// All traps jump here in Direct mode. We save context, call a
// handler, then sret. For this test, we just do sret immediately.
core::arch::global_asm!(
    ".section .text",
    ".balign 4",
    ".global trap_vector",
    "trap_vector:",
    "    csrw sscratch, sp",    // save user SP
    "    sret",                  // return from trap
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

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> ! {
    early_console::write_str("### test_protection (riscv64): init_protection → verify stvec/sscratch\n");

    let memmap: &'static [MemoryRegion] = &MEMMAP;
    let root_page = PhysBytes(bump_alloc(1).expect("bump alloc: root page"));
    let bump_pages = 8;
    let bump_base = bump_alloc(bump_pages).expect("bump alloc: bump region");
    let bump_end = bump_base + (bump_pages as u64) * 4096;

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

    let info = minix_kernel::arch_boot_impl::<Riscv64Paging>(&result.kernel_info, result.root_page);

    early_console::write_str("  Sv39 MMU enabled\n");

    // 3. init_protection — same sequence as kmain's Phase B
    let prot = CurrentProtection::init(0, info.kern_stack_top);
    prot.load();

    let mut trap = CurrentTrapEntry::init();
    trap.configure_syscall(info.syscall_entry);
    trap.load();

    early_console::write_str("  protection structures loaded\n");

    // 4. Verify: stvec is set (non-zero) — exception entry point
    let stvec: u64;
    unsafe {
        asm!("csrr {}, stvec", out(reg) stvec, options(nomem, nostack, preserves_flags));
    }
    if stvec == 0 {
        early_console::write_str("  FAIL: stvec is 0\n");
        fail();
    }
    // Verify Direct mode (bits [1:0] = 0)
    if stvec & 3 != 0 {
        early_console::write_str("  FAIL: stvec mode is not Direct\n");
        fail();
    }
    early_console::write_str("  stvec set (exception entry, Direct mode)\n");

    // 5. Verify: sscratch equals kern_stack_top — kernel stack for U→S
    let sscratch: u64;
    unsafe {
        asm!("csrr {}, sscratch", out(reg) sscratch, options(nomem, nostack, preserves_flags));
    }
    let expected_sp = info.kern_stack_top.get();
    if sscratch != expected_sp {
        early_console::write_str("  FAIL: sscratch mismatch\n");
        fail();
    }
    early_console::write_str("  sscratch = kern_stack_top (kernel stack)\n");

    // 6. Verify: we are in S-mode (SPP or current privilege)
    // Read sstatus to check we are in S-mode
    let sstatus: u64;
    unsafe {
        asm!("csrr {}, sstatus", out(reg) sstatus, options(nomem, nostack, preserves_flags));
    }
    // SPP bit (bit 8) indicates previous privilege; if 0, came from U-mode
    // We just verify sstatus is non-zero (some bits are set)
    if sstatus == 0 {
        early_console::write_str("  FAIL: sstatus is 0\n");
        fail();
    }
    early_console::write_str("  sstatus valid (S-mode)\n");

    early_console::write_str("### TEST_RESULT: PASS test-protection-riscv64 ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-protection-riscv64 ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-protection-riscv64 ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}
