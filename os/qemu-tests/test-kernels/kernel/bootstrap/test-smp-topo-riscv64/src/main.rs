//! Test: topology discovery pin (SMP bring-up S-2, ladder L1) — riscv64/DTB.
//!
//! Pins the D-36 upper half under real QEMU `-smp 4`: OpenSBI hands the
//! supervisor the device-tree blob pointer in `a1` at entry, `parse_by_kind`
//! walks the `cpus` node, and the resulting `CpuTopology` must describe
//! exactly the harts QEMU exposed — four of them, unique hart IDs, BSP among
//! the discovered set.
//!
//! Unlike the x86_64/aarch64 variants this is not a UEFI binary: riscv64 has
//! no Rust UEFI target, so QEMU loads this ELF directly (`-bios default`,
//! `-kernel test-smp-topo-riscv64`).

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_plat::riscv64::early_console;
use minix_boot::PlatformDescSource;
use minix_platform::PlatformDesc;
use minix_platform::kind::{parse_by_kind, DTB};
use minix_types::PhysBytes;

// Minimal bump allocator for bare-metal riscv64 (no UEFI allocator; same
// pattern as test-memmap-riscv64 — only `alloc` for parse-time structures).
use core::alloc::{GlobalAlloc, Layout};

#[unsafe(link_section = ".bss")]
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

// ── Boot assembly ──
// OpenSBI enters with a0 = boot hart ID, a1 = device-tree blob pointer.
// The DTB pointer is parked in a static so `rust_main` can read it after
// the stack switch (a1 itself is gone by then).

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

/// Stack: 64KB, placed in BSS. Top is at __stack_top.
static mut STACK: [u8; 0x10000] = [0u8; 0x10000];

core::arch::global_asm!(
    ".global __stack_top",
    ".set __stack_top, {stack_top} + 0x10000",
    stack_top = sym STACK,
);

/// Boot hart ID, passed through from OpenSBI's a0 (unused; the topology test
/// identifies the BSP via `CpuTopology.bsp_id`, not via a0).

fn fail(msg: &str) -> ! {
    early_console::write_str("### FAIL: ");
    early_console::write_str(msg);
    early_console::write_str("\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}

/// Rust entry — called by `_start` after the stack is up. OpenSBI's a0 (boot
/// hart) and a1 (device-tree blob pointer) pass through untouched: `la sp`
/// only touches the stack pointer and the RISC-V ABI delivers a0/a1 as the
/// first two C arguments.
#[unsafe(no_mangle)]
extern "C" fn rust_main(_boot_hart: u64, dtb_phys: u64) -> ! {
    early_console::write_str("### test_smp_topo (riscv64): discovering topology (DTB via a1)...\n");

    // 1. Device-tree blob physical address handed over by OpenSBI in a1.
    if dtb_phys == 0 {
        fail("OpenSBI did not pass a DTB pointer in a1");
    }
    early_console::write_str("  DTB @ ");
    early_console::write_hex(dtb_phys);
    early_console::write_str("\n");

    // 2. Parse through the same handoff path the kernel uses: kind tag →
    //    parser dispatch (`parse_by_kind`).
    let source = PlatformDescSource::new(DTB, PhysBytes(dtb_phys));
    let desc = match unsafe { parse_by_kind(source) } {
        Ok(d) => d,
        Err(_) => fail("DTB parse failed"),
    };
    let topo = desc.cpu_topology();
    early_console::write_str("  nr_cpus = ");
    early_console::write_hex(topo.nr_cpus as u64);
    early_console::write_str(", bsp hw_id = ");
    early_console::write_hex(topo.bsp_id as u64);
    early_console::write_str("\n");

    // 3. Contract assertion: QEMU runs with -smp 4.
    if topo.nr_cpus != 4 {
        fail("nr_cpus != 4");
    }

    // 4. Contract assertion: hart IDs pairwise distinct.
    for i in 0..topo.nr_cpus as usize {
        for j in 0..i {
            if topo.cpus[i].hw_id == topo.cpus[j].hw_id {
                fail("duplicate hw_id among discovered CPUs");
            }
        }
    }

    // 5. Contract assertion: boot hart ID belongs to the discovered set
    //    (OpenSBI's a0 hart must be one of the DTB cpu nodes).
    let mut bsp_found = false;
    for i in 0..topo.nr_cpus as usize {
        if topo.cpus[i].hw_id == topo.bsp_id as u64 {
            bsp_found = true;
        }
    }
    if !bsp_found {
        fail("bsp hw_id not among discovered CPUs");
    }

    // 6. Machine-specific observation (printed, never asserted): QEMU virt
    //    hart numbering is {0,1,2,3} by default.
    for i in 0..topo.nr_cpus as usize {
        early_console::write_str("  cpu[");
        early_console::write_hex(i as u64);
        early_console::write_str("] hw_id = ");
        early_console::write_hex(topo.cpus[i].hw_id);
        if topo.cpus[i].hw_id == topo.bsp_id as u64 {
            early_console::write_str(" (BSP)");
        }
        early_console::write_str("\n");
    }

    early_console::write_str("### TEST_RESULT: PASS test-smp-topo-riscv64 ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC ###\n");
    loop { unsafe { asm!("wfi", options(nomem, nostack)); } }
}
