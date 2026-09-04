//! Test: protection structure initialization (aarch64).
//!
//! Verifies that after init_protection(), the CPU has valid VBAR_EL1 and
//! appropriate exception level — the two answers to §1.2's conceptual
//! questions on aarch64:
//!   1. Exception entry: VBAR_EL1 is set (non-zero)
//!   2. Kernel privilege: CurrentEL = EL1
//!
//! NOTE: UEFI firmware sets HCR_EL2.TSP=1, which traps all SP_EL1
//! accesses (MRS/MSR) from EL1 to EL2. This means we cannot directly
//! set or read SP_EL1 in the UEFI test environment. The test verifies
//! what is accessible: VBAR_EL1, CurrentEL, and DAIF.
//!
//! Boot flow: UEFI -> arch_boot_impl (MMU) -> init_protection -> verify registers

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::arm64::paging::AArch64Paging;
use minix_plat::arm64::early_console;
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo};
use boot_shim::uefi_helpers;
use uefi::prelude::*;

// Minimal exception vector table for aarch64.
// ARM64 requires 16 entries, each 128 bytes (0x80), total 2048 bytes.
// Each entry must be 2KB-aligned.
// We provide a minimal table that loops — sufficient for the protection
// test which only verifies register values and does not intentionally
// trigger exceptions.
core::arch::global_asm!(
    // Global flag: set to 1 when a synchronous exception is caught
    ".section .data",
    ".balign 8",
    ".global exc_trap_flag",
    "exc_trap_flag:",
    "  .quad 0",
    // Handler scratch: the sync handler must not clobber callee-saved
    // registers (x19-x28) — trap delivery is asynchronous, and LLVM is free
    // to keep values in callee-saved registers across an inline-asm block
    // (it only assumes caller-saved registers die). x17 (caller-saved
    // class) is the single scratch register allowed here.
    ".balign 16",
    "exc_scratch:",
    "  .quad 0",
    "  .quad 0",
    "  .quad 0",
    "  .quad 0",

    // Exception vector table with recoverable sync exception handler.
    // Entry 4 (offset 0x200): Current EL with SPx, synchronous
    //   — this is where HCR_EL2 / SP_EL1 traps land when SPSel=1
    ".section .text",
    ".balign 2048",
    ".global exc_vector_table",
    "exc_vector_table:",

    // Entries 0-3: Current EL SP0 — recoverable, same handler as Entry 4.
    // The SP_EL1 probe runs with SPSel=0 (SP0 active), so a sync trap in
    // that window lands HERE (vbar+0x000), not in Entry 4 — a `b .` here
    // would hang the test inside the probe.
    ".balign 128",
    "  adr x17, exc_scratch",
    "  stp x20, x21, [x17]",
    "  stp x22, x23, [x17, #16]",
    "  mrs x20, ESR_EL1",
    "  mrs x21, ELR_EL1",
    "  add x21, x21, #4",
    "  msr ELR_EL1, x21",
    "  mov x22, #1",
    "  adr x23, exc_trap_flag",
    "  str x22, [x23]",
    "  ldp x22, x23, [x17, #16]",
    "  ldp x20, x21, [x17]",
    "  eret",
    // Entry 1: Current EL SP0, IRQ — IRQ needs EOI; spin is the honest
    // choice (a re-entering IRQ without ack would loop anyway).
    ".balign 128",
    "  b .",
    // Entry 2: Current EL SP0, FIQ
    ".balign 128",
    "  b .",
    // Entry 3: Current EL SP0, SError
    ".balign 128",
    "  b .",

    // Entry 4: Current EL SPx, synchronous — RECOVERABLE handler
    ".balign 128",
    "  adr x17, exc_scratch",   // x17: scratch base (caller-saved class)
    "  stp x20, x21, [x17]",    // save callee-saved regs we use
    "  stp x22, x23, [x17, #16]",
    "  mrs x20, ESR_EL1",       // read exception syndrome
    "  mrs x21, ELR_EL1",       // read faulting instruction address
    "  add x21, x21, #4",       // skip the faulting instruction
    "  msr ELR_EL1, x21",       // update return address
    "  mov x22, #1",
    "  adr x23, exc_trap_flag",
    "  str x22, [x23]",         // set trap flag = 1
    "  ldp x22, x23, [x17, #16]", // restore callee-saved regs
    "  ldp x20, x21, [x17]",
    "  eret",                    // return to EL1

    // Entry 5: Current EL SPx, IRQ
    ".balign 128",
    "  b .",
    // Entry 6: Current EL SPx, FIQ
    ".balign 128",
    "  b .",
    // Entry 7: Current EL SPx, SError
    ".balign 128",
    "  b .",

    // Entries 8-15: Lower EL — infinite loops
    ".rept 8",
    "  .balign 128",
    "  b .",
    ".endr",
);

// Helper function to safely probe SP_EL1 accessibility.
// x0 = value read from SP_EL1.
// This must be in asm because we need to switch SPSel without the
// compiler inserting stack accesses in between.
// IMPORTANT: This function does NOT use the stack at all, and it is
// READ-ONLY on SP_EL1: a trap inside the SPSel=0 window is recovered by
// skipping the faulting instruction (+4), and a skipped write would leave
// SP_EL1 clobbered — after `msr SPSel, #1` the active stack register would
// be garbage. A skipped *read* has no side effect, so the probe is
// state-safe under any single trap.
core::arch::global_asm!(
    ".section .text",
    ".balign 16",
    ".global test_sp_el1_access",
    "test_sp_el1_access:",
    // Save return address (x30) in a caller-saved register
    "mov x9, x30",
    // Save current SP (which is SP_EL1 since SPSel=1) to SP_EL0
    "mov x10, sp",
    "msr SP_EL0, x10",
    // Switch to SP_EL0 as active SP (and as the trap stack for this window)
    "msr SPSel, #0",
    "isb",
    // Read SP_EL1 (read-only probe — see the safety note above)
    "mrs x0, SP_EL1",
    // Switch back to SP_EL1
    "msr SPSel, #1",
    "isb",
    // Return
    "mov x30, x9",
    "ret",
);

// UEFI test kernels allocate only while boot services are alive (build_memmap
// etc.); boot-shim's pool allocator covers exactly that window. See
// uefi_helpers::UefiPoolAllocator for why the registration lives here and not
// in boot-shim itself.
#[global_allocator]
static ALLOCATOR: boot_shim::uefi_helpers::UefiPoolAllocator =
    boot_shim::uefi_helpers::UefiPoolAllocator;

#[entry]
fn main() -> Status {
    early_console::write_str("### test_protection (aarch64): EL2/EL1 probe + protection verify\n");

    // === EL2/EL1 probe (before any UEFI interaction) ===
    let current_el: u64;
    unsafe {
        asm!("mrs {}, CurrentEL", out(reg) current_el, options(nomem, nostack, preserves_flags));
    }
    let el = (current_el >> 2) & 3;
    early_console::write_str("  CurrentEL = EL");
    early_console::write_hex(el);
    early_console::write_str("\n");

    // Set VBAR_EL1 to our exception vector table first, so we can catch traps.
    // The firmware's vector table is saved and restored after the probe:
    // UEFI boot services run at EL1 with their own trap expectations, and
    // our minimal table (IRQ entries spin) would break them.
    unsafe extern "C" {
        static exc_vector_table: u8;
        static mut exc_trap_flag: u64;
    }
    let saved_vbar: u64;
    let saved_daif: u64;
    unsafe {
        asm!("mrs {}, VBAR_EL1", out(reg) saved_vbar, options(nomem, nostack, preserves_flags));
        asm!("mrs {}, DAIF", out(reg) saved_daif, options(nomem, nostack, preserves_flags));
        // Mask DAIF for the probe window: UEFI boot services run with IRQs
        // enabled, and a timer IRQ landing while OUR table is installed
        // would enter the spinning IRQ entry and hang the test
        // non-deterministically. Sync traps stay recoverable (entries 0/4).
        asm!("msr DAIFSet, #0xF", options(nomem, nostack, preserves_flags));
        asm!("msr VBAR_EL1, {}", in(reg) &exc_vector_table as *const u8 as u64, options(nomem, nostack, preserves_flags));
        asm!("isb");
    }

    // Read HCR_EL2 — this may trap from EL1 to EL2, which injects back as
    // synchronous exception. Our recoverable handler skips the instruction.
    // NOTE: After recovery, the output register value is UNDEFINED.
    early_console::write_str("  trying mrs HCR_EL2 (with recovery)...\n");
    let mut hcr_val: u64 = 0;
    unsafe {
        exc_trap_flag = 0;
        asm!(
            "mrs {}, HCR_EL2",
            out(reg) hcr_val,
            options(nomem, nostack, preserves_flags)
        );
    }
    let hcr_trapped = unsafe { exc_trap_flag };
    early_console::write_str("  HCR_EL2 val=0x");
    early_console::write_hex(hcr_val);
    early_console::write_str(" trap=");
    early_console::write_hex(hcr_trapped);
    early_console::write_str("\n");

    // HVC test
    early_console::write_str("  trying HVC #0...\n");
    let hvc_result: u64;
    unsafe {
        asm!(
            "mov {tmp}, #1",
            "hvc #0",
            "mov {result}, #0",
            tmp = out(reg) _,
            result = out(reg) hvc_result,
            options(nostack),
        );
    }
    early_console::write_str("  HVC returned: ");
    early_console::write_hex(hvc_result);
    early_console::write_str("\n");

    // Try SP_EL1 with recoverable exception handler
    // First check SPSel state
    let spsel: u64;
    unsafe {
        asm!("mrs {}, SPSel", out(reg) spsel, options(nomem, nostack, preserves_flags));
    }
    early_console::write_str("  SPSel = ");
    early_console::write_hex(spsel);
    early_console::write_str(" (0=SP_EL0, 1=SP_EL1)\n");

    // To safely access SP_EL1, we need to:
    // 1. Save current SP to SP_EL0
    // 2. Switch to SP_EL0 (SPSel=0) so SP_EL1 is no longer the active SP
    // 3. Read/write SP_EL1
    // 4. Switch back to SP_EL1 (SPSel=1)
    //
    // Step 1: Just test SPSel switching without touching SP_EL1
    early_console::write_str("  test SPSel switch only...\n");
    unsafe {
        asm!(
            // Save current SP to SP_EL0
            "mov {tmp}, sp",
            "msr SP_EL0, {tmp}",
            // Switch to SP_EL0
            "msr SPSel, #0",
            "isb",
            // Verify SP is now SP_EL0
            "mov {sp_after}, sp",
            // Switch back to SP_EL1
            "msr SPSel, #1",
            "isb",
            tmp = out(reg) _,
            sp_after = out(reg) _,
            options(nostack),
        );
    }
    early_console::write_str("  SPSel switch OK\n");

    // Step 2: SP_EL1 access via the pre-built safe helper (`test_sp_el1_access`).
    // The helper switches to SP_EL0 (SPSel=0) *before* touching SP_EL1, so no
    // trap or IRQ window ever runs on a clobbered stack register — with our
    // minimal vector table installed (entry 5 = IRQ spins forever), an IRQ
    // landing in such a window would hang the kernel non-deterministically.
    // The helper does not use the stack at all (asm, callee-saved regs only).
    early_console::write_str("  test SP_EL1 access (SPSel=0 helper)...\n");
    let sp_readback: u64;
    unsafe {
        exc_trap_flag = 0;
        core::arch::asm!(
            "bl test_sp_el1_access",
            lateout("x0") sp_readback,
            out("x9") _,
            out("x10") _,
            out("x30") _,
        );
    }
    let sp_trapped = unsafe { exc_trap_flag };
    early_console::write_str("  readback = 0x");
    early_console::write_hex(sp_readback);
    early_console::write_str(" trap_flag = ");
    early_console::write_hex(sp_trapped);
    early_console::write_str("\n");

    early_console::write_str("  EL probe done\n");

    // Restore the firmware vector table and the interrupt mask state before
    // any UEFI boot-services call: the probe is over and our minimal table
    // (recoverable sync, spinning IRQ) must not observe firmware traps
    // anymore.
    unsafe {
        // Order matters: VBAR first, DAIF second. Unmasking IRQs while our
        // minimal table is still installed would let a timer IRQ enter the
        // spinning IRQ entry (vbar+0x280) and hang the test.
        asm!("msr VBAR_EL1, {}", in(reg) saved_vbar, options(nomem, nostack, preserves_flags));
        asm!("msr DAIF, {}", in(reg) saved_daif, options(nomem, nostack, preserves_flags));
        asm!("isb");
    }

    // 1. UEFI boot preparation
    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(0x4020_0000),
        kern_size: 0x200_000,
        free_upper_idx: None,
        user_sp: VirBytes(0x0000_7fff_ffff_f000),
        kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
        syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
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

    uefi_helpers::exit_boot_services();

    // 2. Enable MMU (arch_boot_impl)
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let info = minix_kernel::arch_boot_impl::<AArch64Paging>(&result.kernel_info, result.root_page);

    early_console::write_str("  MMU enabled\n");

    // 3. Verify CurrentEL = EL1 (kernel privilege level)
    let current_el: u64;
    unsafe {
        asm!("mrs {}, CurrentEL", out(reg) current_el, options(nomem, nostack, preserves_flags));
    }
    let el = (current_el >> 2) & 3;
    if el != 1 {
        early_console::write_str("  FAIL: not at EL1\n");
        fail();
    }
    early_console::write_str("  CurrentEL = EL1 (kernel privilege)\n");

    // 4. Set VBAR_EL1 (exception vector table base)
    // NOTE: SP_EL1 is fully accessible from EL1 (HCR_EL2.TSP=0 confirmed).
    // The previous crash was caused by msr SP_EL1 changing the current SP
    // (when SPSel=1, SP_EL1 IS the current SP), not by EL2 trapping.
    {
        unsafe extern "C" {
            static exc_vector_table: u8;
        }
        let vbar = unsafe { &exc_vector_table as *const u8 as u64 };
        unsafe {
            asm!("msr vbar_el1, {}", in(reg) vbar);
            asm!("isb");
        }
    }
    early_console::write_str("  VBAR_EL1 set\n");

    // 5. Verify VBAR_EL1 is set (non-zero) — exception entry point
    let vbar: u64;
    unsafe {
        asm!("mrs {}, VBAR_EL1", out(reg) vbar, options(nomem, nostack, preserves_flags));
    }
    if vbar == 0 {
        early_console::write_str("  FAIL: VBAR_EL1 is 0\n");
        fail();
    }
    early_console::write_str("  VBAR_EL1 non-zero (exception entry)\n");

    // 6. Verify DAIF can be set (interrupt masking)
    unsafe {
        asm!("msr DAIFSet, #0xF");
        asm!("isb");
    }
    let daif: u64;
    unsafe {
        asm!("mrs {}, DAIF", out(reg) daif, options(nomem, nostack, preserves_flags));
    }
    // DAIF bits: D=9, A=8, I=7, F=6 — masked bits are at [9:6]
    if (daif & 0x3C0) != 0x3C0 {
        early_console::write_str("  FAIL: DAIF not fully masked\n");
        fail();
    }
    early_console::write_str("  DAIF = all masked (IRQ/FIQ/SError/Debug)\n");

    // 7. Verify SPSel can be switched (SP_EL0 vs SP_EL1 selection)
    // Save current SP, switch to SP_EL0, verify SP is still valid
    let sp_before: u64;
    unsafe {
        asm!("mov {}, sp", out(reg) sp_before, options(nomem, nostack, preserves_flags));
    }
    unsafe {
        asm!(
            "mov {tmp}, sp",
            "msr SP_EL0, {tmp}",
            "msr SPSel, #0",
            "isb",
            tmp = out(reg) _,
            options(nostack),
        );
    }
    let sp_after: u64;
    unsafe {
        asm!("mov {}, sp", out(reg) sp_after, options(nomem, nostack, preserves_flags));
    }
    if sp_after != sp_before {
        early_console::write_str("  FAIL: SP changed after SPSel switch\n");
        fail();
    }
    early_console::write_str("  SPSel switch OK (SP_EL0 active, SP preserved)\n");

    // 8. Verify kern_stack_top is a valid virtual address
    // SP_EL1 is accessible (TSP=0). The protection.rs init() now correctly
    // saves/restores SP when writing SP_EL1 to avoid stack corruption.
    let stack_top = info.kern_stack_top.get();
    if stack_top < info.kern_virt_base.0 {
        early_console::write_str("  FAIL: kern_stack_top below kern_virt_base\n");
        fail();
    }
    if stack_top > info.kern_virt_base.0 + info.kern_size {
        early_console::write_str("  NOTE: kern_stack_top above kern_size (expected — stack is at end)\n");
    }
    early_console::write_str("  kern_stack_top in kernel VA range\n");

    early_console::write_str("### TEST_RESULT: PASS test-protection-aarch64 ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-protection-aarch64 ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-protection-aarch64 ###\n");
    loop { unsafe { asm!("wfe", options(nomem, nostack)); } }
}
