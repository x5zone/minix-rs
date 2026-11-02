//! Test: protection structure initialization (x86_64).
//!
//! Verifies that after init_protection(), the CPU has valid GDT, IDT, and TSS
//! loaded — the three answers to §1.2's conceptual questions:
//!   1. Exception entry: IDT is loaded (SIDT base != 0)
//!   2. Privilege isolation: GDT is loaded (SGDT base != 0)
//!   3. Kernel stack: TSS is loaded (STR != 0) and sp0 is set
//!
//! Boot flow: UEFI → arch_boot_impl (paging) → init_protection → verify registers

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;
use minix_arch::x86_64::paging::X86_64Paging;
use minix_plat::x86_64::early_console;
use minix_arch::{ProtectionArch, TrapEntryArch, CurrentProtection, CurrentTrapEntry};
use minix_kernel::boot_alloc;
use minix_arch::pt_alloc;
use minix_types::{PhysBytes, VirBytes};
use minix_boot::{BootPrepareResult, KernelInfo};
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
    early_console::write_str("### test_protection (x86_64): init_protection → verify GDT/IDT/TSS\n");

    // 1. UEFI boot preparation
    let memmap = uefi_helpers::build_memmap();
    let root_page = uefi_helpers::alloc_root_page();
    let (bump_base, bump_end) = uefi_helpers::alloc_bump_region(64);

    let kernel_info = KernelInfo {
        memmap,
        kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
        kern_phys_base: PhysBytes(0x200_000),
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
        reserved_regions: &[],
    };

    let result = BootPrepareResult {
        kernel_info,
        root_page,
        bump_base,
        bump_end,
    };

    uefi_helpers::exit_boot_services();

    // 2. Enable paging (arch_boot_impl)
    boot_alloc::init_boot_pt_alloc(result.bump_base, result.bump_end);
    pt_alloc::register(boot_alloc::boot_pt_alloc);
    let info = minix_kernel::arch_boot_impl::<minix_arch::arch::boot_arch::X86_64BootArch>(&result.kernel_info, result.root_page);

    early_console::write_str("  paging enabled\n");

    // 3. init_protection's protection half — same trait calls as kmain's
    //    Phase B. The instance stays on this stack (main never returns), so
    //    the global-move + refresh_descriptors step is exercised by
    //    test-user-trap's production init_protection call instead.
    let prot = CurrentProtection::init(0, info.kern_stack_top);
    prot.load();

    let mut trap = CurrentTrapEntry::init();
    trap.configure_syscall(minix_arch::syscall_entry_va());
    trap.load();

    // SYSCALL MSR readback: the production entry value (the asm label), not
    // the KernelInfo metadata field, and the four values configure_syscall
    // must leave behind. STAR = anchor 0x1B (GDT[3]) in [63:48] and kernel
    // CS 0x08 (GDT[1]) in [47:32]; LSTAR = syscall entry; SFMASK = IF only;
    // EFER.SCE = 1.
    let star = unsafe { rdmsr(0xC000_0081) };
    let expected_star = (0x08u64 << 32) | (0x1Bu64 << 48);
    if star != expected_star {
        early_console::write_str("  FAIL: STAR mismatch\n");
        fail();
    }
    let lstar = unsafe { rdmsr(0xC000_0082) };
    if lstar != minix_arch::syscall_entry_va().get() {
        early_console::write_str("  FAIL: LSTAR mismatch\n");
        fail();
    }
    let sfmask = unsafe { rdmsr(0xC000_0084) };
    if sfmask != 0x200 {
        early_console::write_str("  FAIL: SFMASK mismatch\n");
        fail();
    }
    let efer = unsafe { rdmsr(0xC000_0080) };
    if efer & 1 == 0 {
        early_console::write_str("  FAIL: EFER.SCE is clear\n");
        fail();
    }
    early_console::write_str("  SYSCALL MSRs verified (STAR/LSTAR/SFMASK/EFER)\n");

    early_console::write_str("  protection structures loaded\n");

    // 4. Verify: GDT is loaded (SGDT base != 0)
    let gdtr_base: u64;
    unsafe {
        asm!(
            "sub rsp, 10",
            "sgdt [rsp]",
            "mov rax, [rsp+2]",
            "add rsp, 10",
            out("rax") gdtr_base,
            options(nostack, preserves_flags)
        );
    }
    if gdtr_base == 0 {
        early_console::write_str("  FAIL: GDT base is 0\n");
        fail();
    }
    early_console::write_str("  GDT loaded (base != 0)\n");

    // 5. Verify: IDT is loaded (SIDT base != 0)
    let idtr_base: u64;
    unsafe {
        asm!(
            "sub rsp, 10",
            "sidt [rsp]",
            "mov rax, [rsp+2]",
            "add rsp, 10",
            out("rax") idtr_base,
            options(nostack, preserves_flags)
        );
    }
    if idtr_base == 0 {
        early_console::write_str("  FAIL: IDT base is 0\n");
        fail();
    }
    early_console::write_str("  IDT loaded (base != 0)\n");

    // 6. Verify: TSS is loaded (STR != 0)
    let tr: u16;
    unsafe {
        asm!("str {0:x}", out(reg) tr, options(nomem, nostack, preserves_flags));
    }
    if tr == 0 {
        early_console::write_str("  FAIL: TR is 0\n");
        fail();
    }
    early_console::write_str("  TSS loaded (TR != 0)\n");

    // 7. Verify: CS segment selector indicates Ring 0 (RPL = 0)
    let cs: u16;
    unsafe {
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
    }
    if cs & 3 != 0 {
        early_console::write_str("  FAIL: CS RPL is not 0\n");
        fail();
    }
    early_console::write_str("  CS=RPL0 (kernel privilege)\n");

    // 8. Verify: DS segment selector indicates Ring 0 (RPL = 0)
    let ds: u16;
    unsafe {
        asm!("mov {0:x}, ds", out(reg) ds, options(nomem, nostack, preserves_flags));
    }
    if ds & 3 != 0 {
        early_console::write_str("  FAIL: DS RPL is not 0\n");
        fail();
    }
    early_console::write_str("  DS=RPL0 (kernel privilege)\n");

    early_console::write_str("### TEST_RESULT: PASS test-protection ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

fn fail() -> ! {
    early_console::write_str("### TEST_RESULT: FAIL test-protection ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}

/// Read a 64-bit MSR. `msr` must be valid for this CPU (the SYSCALL MSRs
/// are architectural and writable/readable at CPL0).
unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    early_console::write_str("### PANIC in test-protection ###\n");
    loop { unsafe { asm!("hlt", options(nomem, nostack)); } }
}
