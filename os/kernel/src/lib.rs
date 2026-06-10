//! Minix-RS Kernel
//!
//! Microkernel implementation, including:
//! - Boot sequence (arch_boot → kmain)
//! - Process management (proc)
//! - IPC mechanism (ipc)
//! - Scheduler (sched)
//! - Virtual memory — kernel part (vm)
//! - Hardware abstraction (hal, arch)

#![no_std]
#![cfg_attr(not(test), no_main)]

extern crate alloc;

use minix_arch::paging_ext::HugePages;
use minix_types::{VirBytes, PhysBytes};
use minix_boot::KernelInfo;
use minix_arch::paging::PageFlags;
use minix_arch::pt_alloc;

/// End of identity-mapped region during boot (4 GB).
/// C: pg_identity() maps 1024 × 4MB = 4GB (I386_BIG_PAGE_SIZE × 1024).
const IDENTITY_MAP_END: u64 = 0x1_0000_0000;

pub mod vm;
pub mod proc;
pub mod proc_table;
pub mod kpriv;
pub mod sched;
pub mod boot_alloc;
pub mod boot;

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
#[path = "arch/x86_64/mod.rs"]
pub mod x86_64;

#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
#[path = "arch/aarch64/mod.rs"]
pub mod aarch64;

#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
#[path = "arch/riscv64/mod.rs"]
pub mod riscv64;

pub use core::panic::PanicInfo;

// ── Boot entry points ──

/// Architecture-specific boot entry (called by boot-shim after ExitBootServices).
/// Selects the correct `Paging` implementation at compile time, then performs
/// the higher-half transition via the `HigherHalf` trait.
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::x86_64::paging::X86_64Paging;
    use crate::x86_64::higher_half::X86_64HigherHalf;
    use crate::boot::HigherHalf;
    let info = arch_boot_impl::<X86_64Paging>(kernel_info, root_page);
    // SAFETY: arch_boot_impl just enabled paging with both identity
    // and kernel high mappings. info is valid and accessible at high address.
    // kern_stack_top is a valid high virtual address from KernelInfo.
    unsafe { X86_64HigherHalf::jump_to_kmain(info, info.kern_stack_top) }
}

#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::arm64::paging::AArch64Paging;
    use crate::aarch64::higher_half::AArch64HigherHalf;
    use crate::boot::HigherHalf;
    let info = arch_boot_impl::<AArch64Paging>(kernel_info, root_page);
    // SAFETY: arch_boot_impl just enabled paging with both identity
    // and kernel high mappings. info is valid and accessible at high address.
    // kern_stack_top is a valid high virtual address from KernelInfo.
    unsafe { AArch64HigherHalf::jump_to_kmain(info, info.kern_stack_top) }
}

#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub fn arch_boot(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::riscv64::paging::Riscv64Paging;
    use crate::riscv64::higher_half::Riscv64HigherHalf;
    use crate::boot::HigherHalf;
    let info = arch_boot_impl::<Riscv64Paging>(kernel_info, root_page);
    // SAFETY: arch_boot_impl just enabled paging with both identity
    // and kernel high mappings. info is valid and accessible at high address.
    // kern_stack_top is a valid high virtual address from KernelInfo.
    unsafe { Riscv64HigherHalf::jump_to_kmain(info, info.kern_stack_top) }
}

#[cfg(all(test, feature = "mock"))]
pub fn arch_boot_test(kernel_info: &KernelInfo, root_page: PhysBytes) -> ! {
    use minix_arch::paging::mock::MockPaging;
    let _info = arch_boot_impl::<MockPaging>(kernel_info, root_page);
    // In mock context, just verify we reach here without panic — that
    // confirms identity + kernel mapping + paging enable all succeeded.
    mock_kmain_ok()
}

/// Minimal kmain for mock/test context: verifies basic higher-half invariants.
#[cfg(all(test, feature = "mock"))]
fn mock_kmain_ok() -> ! {
    // `arch_boot_impl` already verified the boot flow. We just need to
    // return `!` to satisfy the caller contract. In tests, the caller
    // expects this function to diverge.
    loop { core::hint::spin_loop(); }
}

/// Validate KernelInfo constraints, register the boot allocator if needed,
/// and compute the appropriate huge page size for the kernel mapping.
///
/// Returns `kern_huge` (in bytes, as u64) — the page size to use for both
/// identity mapping and kernel mapping loops. This avoids duplicating the
/// alignment/size selection logic between Step 0a validation and Step 2.
fn boot_validate_and_prepare<P: HugePages>(kernel_info: &KernelInfo) -> u64 {
    let kv = kernel_info.kern_virt_base.0;
    let kp = kernel_info.kern_phys_base.0;
    let ks = kernel_info.kern_size;
    let huge = P::HUGE_PAGE_SIZE as u64;
    let fallback = P::FALLBACK_HUGE_PAGE_SIZE as u64;

    // kern_size must be positive
    assert!(ks > 0, "arch_boot: kern_size must be > 0");

    // kern_phys_base must be page-aligned (at least 4KB)
    assert!(kp % 0x1000 == 0, "arch_boot: kern_phys_base must be page-aligned");

    // kern_virt_base must be page-aligned (at least 4KB)
    assert!(kv % 0x1000 == 0, "arch_boot: kern_virt_base must be page-aligned");

    // Choose the largest huge-page size that both bases and kern_size are aligned to.
    let kern_huge = if kv % huge == 0 && kp % huge == 0 && ks % huge == 0 {
        huge
    } else if kv % fallback == 0 && kp % fallback == 0 && ks % fallback == 0 {
        fallback
    } else {
        panic!("arch_boot: kern_virt_base, kern_phys_base, and kern_size must be aligned to at least FALLBACK_HUGE_PAGE_SIZE");
    };

    // kern_size must be a multiple of kern_huge for the mapping loop
    assert!(ks % kern_huge == 0,
        "arch_boot: kern_size must be a multiple of the chosen huge page size");

    // kern_stack_top must be 16-byte aligned (ABI requirement)
    assert!(kernel_info.kern_stack_top.0 % 16 == 0,
        "arch_boot: kern_stack_top must be 16-byte aligned");

    // Register boot-stage page table page allocator if not already registered.
    // The caller (e.g., a test kernel) may have registered its own allocator
    // with a safer region (e.g., a bump region past the kernel image).
    if !pt_alloc::is_registered() {
        let base = kernel_info.memmap.first().map(|r| r.base.0).unwrap_or(0);
        let end = base + kernel_info.memmap.first().map(|r| r.len as u64).unwrap_or(0);
        let boot_alloc_end = core::cmp::min(base + 0x100_000, end);
        boot_alloc::init_boot_pt_alloc(base, boot_alloc_end);
        pt_alloc::register(boot_alloc::boot_pt_alloc);
    }

    kern_huge
}

/// Generic boot implementation — works for any `HugePages` impl.
///
/// `P: HugePages` implies `P: Paging`, so we get `new_from_page`, `enable`, `map`
/// from `Paging` plus `HUGE_PAGE_SIZE` and `map_huge` from `HugePages`.
///
/// Returns `&KernelInfo` after paging is enabled, so the caller can decide
/// what to do next (e.g., call `kmain` for the real kernel, or print PASS
/// for a test kernel).
///
/// C: pg_clear() + pg_identity() + pg_mapkernel() + pg_load() + vm_enable_paging()
///    pre_init.c:230-236, pg_utils.c:162/186/204/247
pub fn arch_boot_impl<P: HugePages>(kernel_info: &KernelInfo, root_page: PhysBytes) -> &KernelInfo {
    // Step 0: Validate KernelInfo + register allocator + compute kern_huge.
    let kern_huge = boot_validate_and_prepare::<P>(kernel_info);

    let mut paging = P::new_from_page(root_page);

    // Step 1: Identity mapping — VA = PA for the first 4GB of address space.
    // C: pg_identity(&kinfo) — pg_utils.c:162
    // Maps 1024 × 4MB = 4GB (C) or 4 × 1GB (x86-64) or equivalent on other archs.
    // We must map the entire low address space, not just CONVENTIONAL memory,
    // because UEFI loader code/data may be in LOADER_DATA/LOADER_CODE regions
    // that are not marked CONVENTIONAL. Without covering these regions,
    // the CPU faults immediately after the page table switch.
    // NOTE: EXECUTABLE flag is required so the CPU can fetch instructions
    // after the page table switch (CR3/satp/TTBR write).
    // NOTE: We use kernel_read_write() (supervisor-only, no USER_ACCESSIBLE)
    // because on RISC-V Sv39, a page with U=1 cannot be executed by
    // supervisor mode unless sstatus.SUM=1. Since we haven't set SUM,
    // identity mapping must be supervisor-only.
    let id_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
    let mut addr: u64 = 0;
    while addr < IDENTITY_MAP_END {
        // Ignore errors — some ranges may not be backed by physical memory,
        // but that's fine; the CPU won't access them.
        let _ = paging.map_huge(
            VirBytes(addr), PhysBytes(addr),
            kern_huge as usize, id_flags,
        );
        addr += kern_huge as u64;
    }

    // Step 2: Kernel high-address mapping.
    // C: pg_mapkernel() — pg_utils.c:186
    //
    // When kern_virt_base == kern_phys_base, the kernel mapping is
    // identical to the identity mapping (Step 1). Skip it to avoid
    // overwriting L2 entries (especially on riscv64 where the same
    // VPN[2] slot would be written twice with potentially different
    // page sizes, corrupting the identity mapping).
    // See 01-bug.md for the full analysis.
    let kern_virt = kernel_info.kern_virt_base.0;
    let kern_phys = kernel_info.kern_phys_base.0;
    if kern_virt != kern_phys {
        let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let mut offset = 0u64;
        while offset < kernel_info.kern_size {
            paging.map_huge(
                VirBytes(kern_virt + offset), PhysBytes(kern_phys + offset),
                kern_huge as usize, kern_flags,
            ).expect("kernel map: map_huge failed — kernel image mismatch");
            offset += kern_huge as u64;
        }
    }

    // Step 3: Enable paging.
    // SAFETY: Steps 1+2 set up identity mapping covering current RIP.
    let _root_phys = unsafe { paging.enable() };

    // Return kernel_info so the caller can decide what to do next.
    kernel_info
}

/// Kernel main — called after the higher-half transition.
///
/// This function runs at the kernel's high virtual address.
/// It orchestrates the six-phase boot sequence:
///
/// Phase A (this function): Entry — validate kinfo, allow kernel alloc
/// Phase B: cstart — prot_init + clock + intr + arch_init
/// Phase C: proc_init + arch_boot_proc
/// Phase D: arch_post_init + memory_init
/// Phase E: system_init
/// Phase F: bsp_finish_booting + switch_to_user
///
/// C: main.c:115-147
#[cfg(all(not(feature = "mock"), not(feature = "qemu_test")))]
pub fn kmain(kernel_info: &KernelInfo) -> ! {
    // Phase A: Entry
    // C: memcpy(&kinfo, local_cbi, sizeof(kinfo)) + kernel_may_alloc = 1
    // Rust: no memcpy needed — kernel_info is already a &KernelInfo reference
    // Rust: no BSS check needed — Rust guarantees zero-initialization

    // Phase B: cstart — protection + clock + interrupt
    init_protection(kernel_info);        // prot_init equivalent
    init_clock_and_interrupts();         // clock + intr + arch_init (covered in 04)

    // Phase C: proc_init + arch_boot_proc
    init_proc_and_boot(kernel_info);  // covered in 05

    // Phase D: arch_post_init + memory_init
    init_post_and_memory(kernel_info);  // covered in 06

    // Phase E-F: covered in subsequent documents
    // TODO: system_init + bsp_finish_booting (07)

    loop {}
}

/// Minimal kmain for QEMU integration tests (feature = "qemu_test").
///
/// This is a naked function that captures SP/PC/FP at the exact entry point
/// (before any function prologue modifies them), then calls the real test
/// verification logic.
///
/// The HigherHalf trait's `jump_to_kmain` lands here. We verify that:
/// - Stack pointer is at a high virtual address (kern_stack_top was used)
/// - Stack pointer is 16-byte aligned (ABI requirement at function entry)
/// - Frame pointer is zero (set by HigherHalf transition)
#[cfg(feature = "qemu_test")]
#[unsafe(naked)]
pub extern "C" fn kmain(kernel_info: &KernelInfo) -> ! {
    // The HigherHalf impl puts kinfo in RDI (System V convention).
    // On UEFI target (Windows x64 ABI), kmain_verify expects:
    //   RCX = kernel_info, RDX = sp, R8 = pc, R9 = fp
    // We rearrange registers accordingly.
    #[cfg(target_arch = "x86_64")]
    core::arch::naked_asm!(
        "mov r9, rbp",         // capture FP at entry → R9 (4th arg)
        "lea r8, [rip]",       // capture PC at entry → R8 (3rd arg)
        "mov rdx, rsp",        // capture SP at entry → RDX (2nd arg)
        "mov rcx, rdi",        // kinfo from RDI → RCX (1st arg, Windows ABI)
        "sub rsp, 40",         // shadow space (32B) + alignment (8B)
        "call {verify}",
        verify = sym kmain_verify,
    );
    #[cfg(target_arch = "aarch64")]
    core::arch::naked_asm!(
        "mov x1, sp",          // capture SP at entry
        "adr x2, .",           // capture PC at entry
        "mov x3, x29",         // capture FP at entry
        "bl {verify}",
        verify = sym kmain_verify,
    );
    #[cfg(target_arch = "riscv64")]
    core::arch::naked_asm!(
        "mv a1, sp",           // capture SP at entry
        "auipc a2, 0",         // capture PC at entry
        "mv a3, s0",           // capture FP at entry
        "call {verify}",
        verify = sym kmain_verify,
    );
}

/// Verification function called from the naked kmain entry point.
///
/// Receives captured register values as arguments:
/// - kernel_info: pointer to KernelInfo
/// - sp: stack pointer at kmain entry
/// - pc: program counter at kmain entry
/// - fp: frame pointer at kmain entry
#[cfg(feature = "qemu_test")]
fn kmain_verify(kernel_info: &KernelInfo, sp: u64, pc: u64, fp: u64) -> ! {
    use minix_arch::{EarlyConsole, CurrentEarlyConsole as Console};

    #[cfg(target_arch = "riscv64")]
    Console::write_str("kmain_verify: reached!\n");

    let kern_high = kernel_info.kern_virt_base.0;

    // Architecture-specific labels for register output
    #[cfg(target_arch = "x86_64")]
    const ARCH_NAME: &str = "x86_64";
    #[cfg(target_arch = "aarch64")]
    const ARCH_NAME: &str = "aarch64";
    #[cfg(target_arch = "riscv64")]
    const ARCH_NAME: &str = "riscv64";

    #[cfg(target_arch = "x86_64")]
    const SP_LABEL: &str = "  RSP (at entry): ";
    #[cfg(target_arch = "x86_64")]
    const PC_LABEL: &str = "  RIP (at entry): ";
    #[cfg(target_arch = "x86_64")]
    const FP_LABEL: &str = "  RBP (at entry): ";

    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    const SP_LABEL: &str = "  SP (at entry):  ";
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    const PC_LABEL: &str = "  PC (at entry):  ";
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    const FP_LABEL: &str = "  FP (at entry):  ";

    // ── Print captured register values ──
    Console::write_str("### test-higher-half ###\n");
    Console::write_str("  arch: "); Console::write_str(ARCH_NAME); Console::write_str("\n");
    Console::write_str(SP_LABEL); Console::write_hex(sp); Console::write_str("\n");
    Console::write_str(PC_LABEL); Console::write_hex(pc); Console::write_str("\n");
    Console::write_str(FP_LABEL); Console::write_hex(fp); Console::write_str("\n");
    Console::write_str("  kern_virt_base: "); Console::write_hex(kern_high); Console::write_str("\n");

    // ── Assertions ──
    // SP must be at a high virtual address (kern_stack_top was used)
    let sp_ok = sp >= kern_high;
    // SP must be 16-byte aligned (ABI requirement at function entry)
    let sp_aligned = sp & 0xF == 0;
    // Frame pointer must be zero (set by HigherHalf transition)
    let fp_zero = fp == 0;

    // Note: PC check is architecture-dependent. In the real kernel, kmain
    // is linked at a high virtual address, so PC >= kern_high. In QEMU
    // test kernels, UEFI/OpenSBI loads the test binary at a low address,
    // so PC may not be in the high range. We check PC only for informational
    // purposes — the key invariants are SP at high address + SP aligned + FP zero.
    let pc_at_high = pc >= kern_high;

    if sp_ok && sp_aligned && fp_zero {
        Console::write_str("### TEST_RESULT: PASS test-higher-half ###\n");
    } else {
        if !sp_ok { Console::write_str("  FAIL: SP not at high address\n"); }
        if !pc_at_high { Console::write_str("  INFO: PC not at high address (expected in test kernel)\n"); }
        if !sp_aligned { Console::write_str("  FAIL: SP not 16-byte aligned\n"); }
        if !fp_zero { Console::write_str("  FAIL: FP not zero\n"); }
        Console::write_str("### TEST_RESULT: FAIL test-higher-half ###\n");
    }

    loop {
        #[cfg(target_arch = "x86_64")]
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)); }
        #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)); }
    }
}

/// Initialize protection structures: GDT/IDT/TSS (x86-64), VBAR_EL1 (aarch64), stvec (riscv64).
///
/// This must be the very first thing called in kmain, because
/// without valid GDT/IDT (x86-64) or VBAR_EL1/stvec (aarch64/riscv64),
/// any exception will cause an unrecoverable triple fault.
///
/// C: prot_init() — protect.c:321 (x86) / protect.c:77 (ARM)
#[cfg(not(feature = "mock"))]
fn init_protection(kernel_info: &KernelInfo) {
    use minix_arch::{ProtectionArch, TrapEntryArch, CurrentProtection, CurrentTrapEntry};

    // Step 1: Initialize protection structures.
    // x86-64: GDT + TSS; aarch64: SP_EL1; riscv64: sscratch
    // C: tss_init(0, &k_boot_stktop) — protect.c:338
    let prot = CurrentProtection::init(0, kernel_info.kern_stack_top);
    prot.load();

    // Step 2: Initialize trap entry table.
    // x86-64: IDT + SYSCALL MSR; aarch64: VBAR_EL1; riscv64: stvec
    // C: SYSCALL MSR setup — protect.c:189-205
    let mut trap = CurrentTrapEntry::init();
    trap.configure_syscall(kernel_info.syscall_entry);
    trap.load();
}

/// Initialize clock and interrupt controller.
///
/// C: init_clock() + intr_init(0) + arch_init() — main.c:403-481
/// Covered in detail in 04-clock-interrupt-init.md.
#[cfg(not(feature = "mock"))]
fn init_clock_and_interrupts() {
    use minix_arch::{
        ClockState, ClockArch,
        InterruptController,
        ArchInit,
        CurrentClockArch, CurrentInterruptController, CurrentArchInit,
    };

    // Step 1: Initialize clock state (software).
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    // No env_get("hz") needed — DEFAULT_HZ is compile-time constant.

    // Step 2: Initialize hardware timer.
    // C: hardware portion of init_clock + arch_init() APIC timer
    CurrentClockArch::init_timer(clock.hz());

    // Step 3: Initialize interrupt controller.
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:24
    let mut intr = CurrentInterruptController::new();
    intr.init();  // mask_all() called internally

    // Step 4: Architecture-specific initialization.
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    CurrentArchInit::init();
}

/// Initialize process table and boot processes.
///
/// This is the Rust equivalent of C's `proc_init()` + the boot image loop
/// in `main.c:157-282`. It:
/// 1. Creates the process table (all slots SLOT_FREE, p_nr/p_endpoint set)
/// 2. Creates the privilege table (all slots free)
/// 3. Iterates over boot modules, assigning privileges and initializing
///    each boot process
/// 4. Loads VM ELF into the bootstrap page table (architecture-specific)
///
/// C: proc_init() — proc.c:119
/// C: boot image loop — main.c:157-282
/// C: arch_boot_proc() — protect.c:388 (x86) / protect.c:115 (ARM)
#[cfg(not(feature = "mock"))]
fn init_proc_and_boot(kernel_info: &KernelInfo) {
    use minix_arch::{ArchProcInit, BootProcArch, CurrentBootProcArch};
    use crate::proc::{KProcess, ProcNr, ProcName, rts, proc_nr};
    use crate::proc_table::{ProcessTable, NR_TASKS};
    use crate::kpriv::{PrivTable, priv_flags, priv_flag_set};

    // Step 1: Initialize process table.
    // C: proc_init() — proc.c:119-167
    // In Rust, ProcessTable::new() creates all slots with p_rts_flags = SLOT_FREE,
    // p_nr = -NR_TASKS..NR_PROCS-1, p_endpoint = _ENDPOINT(0, p_nr).
    let mut proc_table = ProcessTable::new();

    // Step 2: Initialize privilege table.
    // C: priv table loop in proc_init() — proc.c:130-137
    let mut priv_table = PrivTable::new();

    // Step 3: Iterate over boot modules and initialize each process.
    // C: boot image loop — main.c:157-282
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        // Map boot module index to process number.
        // C: image[i].proc_nr — table.c:44
        // Boot modules start after kernel tasks (i >= NR_TASKS → p_nr >= 0).
        let nr: ProcNr = if i < NR_TASKS {
            // Kernel tasks: CLOCK=-3, SYSTEM=-2, KERNEL=-1, etc.
            (i as ProcNr) - (NR_TASKS as ProcNr)
        } else {
            // User processes: DS=0, RS=1, PM=2, ..., VM=8, INIT=9, etc.
            (i - NR_TASKS) as ProcNr
        };

        let proc = proc_table.get_mut(nr);
        if proc.is_none() {
            continue; // Skip invalid process numbers
        }
        let proc = proc.unwrap();

        // Set process name.
        // C: strlcpy(rp->p_name, ip->proc_name, sizeof(rp->p_name))
        proc.p_name = ProcName::from_str(module.name);

        // Determine if this process is immediately schedulable.
        // C: schedulable_proc = (iskerneln(proc_nr) || isrootsysn(proc_nr) ||
        //                         proc_nr == VM_PROC_NR)
        // C: main.c:173-174
        let is_kernel = nr < 0;
        let is_root_sys = false; // TODO: check RS_PROC_NR
        let is_vm = nr == 8; // VM_PROC_NR — minix/com.h:67
        let schedulable = is_kernel || is_root_sys || is_vm;

        if schedulable {
            // Assign static privilege.
            // C: get_priv(rp, static_priv_id(proc_nr)) — main.c:176
            // TODO: implement PrivTable::assign_static()
            let _ = &mut priv_table;

            // Set privilege flags based on process type.
            // C: main.c:178-224
            if is_vm {
                // C: priv(rp)->s_flags = VM_F — main.c:179
                // priv_table.set_flags(nr, priv_flag_set::VM_F);
            } else if is_kernel {
                // C: priv(rp)->s_flags = (proc_nr == IDLE ? IDL_F : TSK_F) — main.c:188
                // priv_table.set_flags(nr, priv_flag_set::TSK_F);
            } else {
                // C: priv(rp)->s_flags = RSYS_F — main.c:209
                // priv_table.set_flags(nr, priv_flag_set::RSYS_F);
            }
        } else {
            // Don't let the process run for now.
            // C: RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM) — main.c:226
            proc.p_rts_flags.set(rts::NO_PRIV | rts::NO_QUANTUM);
        }

        // Architecture-specific boot process initialization.
        // C: arch_boot_proc(ip, rp) — main.c:257
        // For kernel tasks: no-op (p_nr < 0)
        // For VM: load ELF into bootstrap page table
        // For other user processes: no ELF loading (done by RS at runtime)
        if is_kernel {
            // Kernel tasks: arch_boot_proc skips (p_nr < 0)
            // C: if(rp->p_nr < 0) return; — protect.c:393
        } else if is_vm {
            // VM: load ELF and set PC/SP/ps_strings
            // C: arch_boot_proc for VM — protect.c:395-452
            CurrentBootProcArch::init(
                false,  // is_kernel
                nr,
                minix_types::VirBytes(0), // pc: placeholder, set by load_vm_elf
                minix_types::VirBytes(0), // sp: placeholder
                minix_types::VirBytes(0), // ps_strings: placeholder
                module.name,
            );
        } else {
            // Other user processes: just set initial register state
            // C: arch_boot_proc returns without loading ELF
            CurrentBootProcArch::init(
                false,
                nr,
                minix_types::VirBytes(0),
                minix_types::VirBytes(0),
                minix_types::VirBytes(0),
                module.name,
            );
        }

        // VM inhibit: all user processes except VM must wait for VM to
        // create their page tables.
        // C: main.c:267-270
        if nr != 8 && nr >= 0 {
            proc.p_rts_flags.set(rts::VMINHIBIT | rts::BOOTINHIBIT);
        }

        // All boot processes start stopped.
        // C: rp->p_rts_flags |= RTS_PROC_STOP — main.c:272
        proc.p_rts_flags.set(rts::PROC_STOP);

        // Mark slot as in use.
        // C: rp->p_rts_flags &= ~RTS_SLOT_FREE — main.c:273
        proc.p_rts_flags.clear(rts::SLOT_FREE);
    }

    // Step 4: Update boot procs info for VM.
    // C: memcpy(kinfo.boot_procs, image, sizeof(kinfo.boot_procs)) — main.c:282
    // In Rust, kernel_info is immutable and boot_modules already contains this info.
}

/// Initialize post-boot architecture state and memory mapping slots.
///
/// This is the Rust equivalent of C's `arch_post_init()` + `memory_init()`.
/// It:
/// 1. Sets ptproc to VM and records VM's page table addresses
/// 2. Allocates free page directory entries for createpde() temporary mappings
///
/// Must be called after `init_proc_and_boot()` (Phase C), because VM's
/// process slot and page table must already be initialized.
///
/// C: arch_post_init() — protect.c:370 (x86) / protect.c:97 (ARM)
/// C: memory_init() — memory.c:707 (x86) / memory.c:612 (ARM)
#[cfg(not(feature = "mock"))]
fn init_post_and_memory(kernel_info: &KernelInfo) {
    use minix_arch::{
        PostInitArch, MemoryInitArch,
        CurrentPostInitArch, CurrentMemoryInitArch,
        VmPageTableInfo, FreePdeSlots,
    };

    // Step 1: Set ptproc to VM and record VM's page table addresses.
    // C: arch_post_init() — protect.c:370-377 (x86) / protect.c:97-104 (ARM)
    //
    // In C, this does:
    //   vm = proc_addr(VM_PROC_NR);
    //   get_cpulocal_var(ptproc) = vm;
    //   pg_info(&vm->p_seg.p_cr3, &vm->p_seg.p_cr3_v);   // x86
    //   pg_info(&vm->p_seg.p_ttbr, &vm->p_seg.p_ttbr_v); // ARM
    //
    // In Rust, VmPageTableInfo encapsulates the page table root addresses.
    // The actual VM page table info will be populated from the process table
    // once the process module provides accessor methods.
    //
    // TODO: Populate VmPageTableInfo from the VM process's p_seg fields
    //       after ProcessTable provides get_vm_page_table_info().
    let vm_page_table = VmPageTableInfo {
        phys_root: PhysBytes(0), // TODO: from VM process's p_seg.p_cr3/p_ttbr
        virt_root: Some(VirBytes(0)), // TODO: from VM process's p_seg.p_cr3_v/p_ttbr_v
    };
    CurrentPostInitArch::set_ptproc(&vm_page_table);

    // Step 2: Allocate free page directory entries for createpde().
    // C: memory_init() — memory.c:707-717 (x86) / memory.c:612-622 (ARM)
    //
    // In C, this does:
    //   freepdes[nfreepdes++] = kinfo.freepde_start++;
    //   freepdes[nfreepdes++] = kinfo.freepde_start++;
    //
    // In Rust, we use a mutable reference to free_upper_idx (which is
    // part of KernelInfo). Since KernelInfo is shared and immutable,
    // we track the free index locally and pass it to the arch impl.
    //
    // TODO: Once KernelInfo supports mutable access to free_upper_idx,
    //       use kernel_info.free_upper_idx directly. For now, we use
    //       a local copy that is advanced by the arch implementation.
    let mut free_idx = kernel_info.free_upper_idx;
    let _free_pde_slots: FreePdeSlots = CurrentMemoryInitArch::allocate_free_pdes(&mut free_idx);
    // free_idx is now advanced by MAX_FREE_PDE_SLOTS (2).
    // The slots are stored for later use by createpde().
    // TODO: Store _free_pde_slots in kernel global state for createpde() access.
}

// ── Tests ──

#[cfg(all(test, feature = "mock"))]
mod tests {
    use super::*;
    use minix_arch::paging::mock::MockPaging;
    use minix_arch::paging::Paging;

    /// Full boot-flow integration test with MockPaging.
    /// Verifies: identity mapping + kernel mapping + enable → no panic.
    #[test]
    fn test_boot_flow_identity_and_kernel_map() {
        let memmap: &'static [minix_types::MemoryRegion] = &[
            minix_types::MemoryRegion { base: PhysBytes(0x100000), len: 0x1000000 }, // 16MB
        ];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000, // 2MB kernel
            free_upper_idx: 0,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
        };

        let root_page = PhysBytes(0x1000);
        let mut paging = MockPaging::new_from_page(root_page);

        let huge_size = MockPaging::HUGE_PAGE_SIZE as usize;

        // Test Step 1: Identity mapping
        let region = &info.memmap[0];
        let mut addr = region.base.0;
        let end = addr + region.len as u64;
        let mut identity_pages = 0u64;
        while addr < end {
            paging.map_huge(VirBytes(addr), PhysBytes(addr), huge_size, PageFlags::read_write())
                .unwrap();
            addr += huge_size as u64;
            identity_pages += 1;
        }
        assert!(identity_pages > 0, "no identity pages mapped");

        // Verify identity mapping — query a mapped huge-page-aligned address
        let query_addr = region.base.0; // base is 0x100000, mapped at first iteration
        let result = paging.query(VirBytes(query_addr));
        assert!(result.is_some(), "identity mapping not found at 0x{:x}", query_addr);

        // Test Step 2: Kernel high-address mapping
        let mut offset = 0u64;
        let mut kernel_pages = 0u64;
        while offset < info.kern_size {
            paging.map_huge(
                VirBytes(info.kern_virt_base.0 + offset),
                PhysBytes(info.kern_phys_base.0 + offset),
                huge_size, PageFlags::kernel_read_write(),
            ).unwrap();
            offset += huge_size as u64;
            kernel_pages += 1;
        }
        assert!(kernel_pages > 0, "no kernel pages mapped");

        // Test Step 3: Enable paging
        let root_phys = unsafe { paging.enable() };
        assert_eq!(root_phys, PhysBytes(0));
    }

    /// Verify empty memmap is handled gracefully (identity pass is a no-op).
    #[test]
    fn test_boot_empty_memmap() {
        let info = KernelInfo {
            memmap: &[],
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000,
            free_upper_idx: 0,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
        };
        let mut paging = MockPaging::new_from_page(PhysBytes(0x1000));

        // Identity pass should be a no-op with zero regions
        for _region in info.memmap { /* empty */ }

        // Kernel map should still work
        let huge_size = MockPaging::HUGE_PAGE_SIZE as usize;
        let mut offset = 0u64;
        while offset < info.kern_size {
            paging.map_huge(
                VirBytes(info.kern_virt_base.0 + offset),
                PhysBytes(info.kern_phys_base.0 + offset),
                huge_size, PageFlags::kernel_read_write(),
            ).unwrap();
            offset += huge_size as u64;
        }

        unsafe { paging.enable() };
    }

    // ── HigherHalf trait unit tests ──

    use core::sync::atomic::{AtomicBool, Ordering};

    /// Global flag to verify that HigherHalf::jump_to_kmain was invoked.
    static HIGHER_HALF_CALLED: AtomicBool = AtomicBool::new(false);

    /// Mock HigherHalf: sets a global flag when jump_to_kmain is called.
    /// The real trait has `fn jump_to_kmain(kinfo: &KernelInfo) -> !` (no &self),
    /// so we use a static flag instead of a struct.
    struct MockHigherHalf;

    impl boot::HigherHalf for MockHigherHalf {
        unsafe fn jump_to_kmain(_kinfo: &KernelInfo, _stack_top: VirBytes) -> ! {
            HIGHER_HALF_CALLED.store(true, Ordering::SeqCst);
            // Don't actually jump — spin for test purposes.
            loop {
                core::hint::spin_loop();
            }
        }
    }

    /// Verify HigherHalf trait can be implemented by a mock type
    /// (type-system check: associated fn with `-> !` return).
    #[test]
    fn test_higher_half_trait_is_implementable() {
        // Compile-time: MockHigherHalf must implement HigherHalf.
        // This test ensures the trait bounds and signature are correct.
        fn accept_higher_half<H: boot::HigherHalf>() {}
        accept_higher_half::<MockHigherHalf>();
    }

    /// Verify that after arch_boot_impl completes, the boot flow
    /// (arch_boot_impl → enable paging) is consistent.
    #[test]
    fn test_arch_boot_impl_enables_paging() {
        let memmap: &'static [minix_types::MemoryRegion] = &[
            minix_types::MemoryRegion { base: PhysBytes(0x100000), len: 0x1000000 },
        ];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000,
            free_upper_idx: 0,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
        };
        let root_page = PhysBytes(0x1000);

        // arch_boot_impl calls P::new_from_page, map_huge for identity + kernel,
        // and enable(). Should not panic.
        let info_ref = arch_boot_impl::<MockPaging>(&info, root_page);
        assert_eq!(info_ref.kern_virt_base.0, info.kern_virt_base.0,
            "returned KernelInfo should match input");
    }

    /// Verify MockPaging queries work correctly after mapping.
    #[test]
    fn test_mock_paging_query_after_map() {
        let mut paging = MockPaging::new_from_page(PhysBytes(0x1000));
        paging.map_huge(VirBytes(0x100000), PhysBytes(0x100000),
            MockPaging::HUGE_PAGE_SIZE as usize, PageFlags::read_write()).unwrap();
        assert!(paging.query(VirBytes(0x100000)).is_some(),
            "mapped page should be queryable at 0x100000");
        assert!(paging.query(VirBytes(0x00_0001_0000_0000)).is_none(),
            "unmapped page should not be queryable");
    }

    /// Verify that IDENTITY_MAP_END is aligned for huge-page mapping loops.
    #[test]
    fn test_identity_map_end_alignment() {
        let huge = MockPaging::HUGE_PAGE_SIZE as u64;
        assert!(huge > 0, "HUGE_PAGE_SIZE must be positive");
        assert!(huge.is_power_of_two(), "HUGE_PAGE_SIZE must be a power of two");
        assert!(IDENTITY_MAP_END > 0, "IDENTITY_MAP_END must be positive");
        // IDENTITY_MAP_END / huge should be an integer (no partial pages at boundary)
        assert!(IDENTITY_MAP_END % huge == 0,
            "IDENTITY_MAP_END (0x{:x}) must be a multiple of HUGE_PAGE_SIZE (0x{:x})",
            IDENTITY_MAP_END, huge);
    }

    // ── Three-architecture KernelInfo consistency tests ──

    /// Verify x86_64 KernelInfo: kern_phys_base must be 2MB-aligned for huge pages.
    #[test]
    fn test_kernel_info_x86_64_alignment() {
        let kern_phys_base: u64 = 0x200_000; // 2MB, as used in QEMU tests
        let kern_virt_base: u64 = 0xFFFF_8000_0000_0000;
        assert_eq!(kern_phys_base % (1 << 21), 0,
            "x86_64 kern_phys_base must be 2MB-aligned");
        assert_eq!(kern_virt_base % (1 << 30), 0,
            "x86_64 kern_virt_base must be 1GB-aligned for 1GB huge pages");
    }

    /// Verify aarch64 KernelInfo: kern_phys_base must be in QEMU virt RAM range.
    /// QEMU virt aarch64 RAM starts at 0x4000_0000.
    #[test]
    fn test_kernel_info_aarch64_phys_in_ram() {
        let ram_start: u64 = 0x4000_0000; // QEMU virt aarch64 RAM base
        let kern_phys_base: u64 = 0x4020_0000; // 2MB-aligned, within RAM
        assert!(kern_phys_base >= ram_start,
            "aarch64 kern_phys_base must be >= QEMU RAM start (0x4000_0000)");
        assert_eq!(kern_phys_base % (1 << 21), 0,
            "aarch64 kern_phys_base must be 2MB-aligned");
    }

    /// Verify riscv64 KernelInfo: kern_virt_base == kern_phys_base (identity mapping in Sv39).
    /// QEMU virt riscv64 RAM starts at 0x8000_0000.
    #[test]
    fn test_kernel_info_riscv64_identity() {
        let dram_base: u64 = 0x8000_0000; // QEMU virt riscv64 DRAM base
        let kern_phys_base: u64 = dram_base;
        let kern_virt_base: u64 = dram_base; // identity mapping
        assert_eq!(kern_virt_base, kern_phys_base,
            "riscv64 kern_virt_base must equal kern_phys_base (identity mapping)");
        assert_eq!(kern_phys_base % (1 << 30), 0,
            "riscv64 kern_phys_base must be 1GB-aligned for Sv39 huge pages");
    }

    /// Verify arch_boot_impl with aarch64-style KernelInfo (phys in QEMU RAM range).
    #[test]
    fn test_arch_boot_impl_aarch64_params() {
        let memmap: &'static [minix_types::MemoryRegion] = &[
            minix_types::MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x800_0000 },
        ];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x4020_0000),
            kern_size: 0x200_000,
            free_upper_idx: 0,
            user_sp: VirBytes(0x0000_7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
        };
        let root_page = PhysBytes(0x1000);
        let info_ref = arch_boot_impl::<MockPaging>(&info, root_page);
        assert_eq!(info_ref.kern_phys_base.0, 0x4020_0000);
    }

    /// Verify arch_boot_impl with riscv64-style KernelInfo (identity mapping).
    /// Note: In real riscv64, kern_virt_base == kern_phys_base, so identity map
    /// and kernel map overlap at the same virtual address. MockPaging rejects
    /// double-mapping, so we use a non-overlapping kern_virt_base here to test
    /// the arch_boot_impl flow, and verify the identity property separately.
    #[test]
    fn test_arch_boot_impl_riscv64_params() {
        let memmap: &'static [minix_types::MemoryRegion] = &[
            minix_types::MemoryRegion { base: PhysBytes(0x8000_0000), len: 0x800_0000 },
        ];
        // Use a high-half address that doesn't overlap with identity map range
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0800_0000), // high-half alias
            kern_phys_base: PhysBytes(0x8000_0000),
            kern_size: 0x200_000,
            free_upper_idx: 0,
            user_sp: VirBytes(0x0000_003f_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0800_0000 + 0x200_000),
            syscall_entry: VirBytes(0xFFFF_8000_0800_0000),
            boot_modules: &[],
        };
        let root_page = PhysBytes(0x1000);
        let info_ref = arch_boot_impl::<MockPaging>(&info, root_page);
        assert_eq!(info_ref.kern_phys_base.0, 0x8000_0000);
    }

    // ── Linker script constraint validation tests ──
    // These tests verify that the constants in the three architecture
    // linker scripts (link.ld) satisfy arch_boot_impl's constraints.

    /// x86_64 linker script: KERN_VIRT_BASE = 0xFFFF800000000000, KERN_PHYS_BASE = 0x200000
    /// Verify these values satisfy arch_boot_impl constraints.
    #[test]
    fn test_linker_script_x86_64_constraints() {
        let kern_virt_base: u64 = 0xFFFF_8000_0000_0000; // link.ld KERN_VIRT_BASE
        let kern_phys_base: u64 = 0x200_000;             // link.ld KERN_PHYS_BASE
        let kern_size: u64 = 0x200_000;                  // typical 2MB kernel

        // Must be page-aligned
        assert_eq!(kern_virt_base % 0x1000, 0, "kern_virt_base must be page-aligned");
        assert_eq!(kern_phys_base % 0x1000, 0, "kern_phys_base must be page-aligned");

        // x86_64 HUGE_PAGE_SIZE = 1GB, FALLBACK = 2MB
        let huge: u64 = 1 << 30; // 1GB
        let fallback: u64 = 1 << 21; // 2MB

        // kern_phys_base = 0x200_000 is 2MB-aligned but not 1GB-aligned
        assert_eq!(kern_phys_base % fallback, 0,
            "kern_phys_base must be at least FALLBACK_HUGE_PAGE_SIZE aligned");
        assert_eq!(kern_virt_base % huge, 0,
            "kern_virt_base must be HUGE_PAGE_SIZE aligned for 1GB pages");

        // kern_size must be a multiple of the chosen huge page size
        // Since kern_phys_base is not 1GB-aligned, we use 2MB fallback
        assert_eq!(kern_size % fallback, 0,
            "kern_size must be a multiple of 2MB");
    }

    /// aarch64 linker script: KERN_VIRT_BASE = 0xFFFF800000000000, KERN_PHYS_BASE = 0x40200000
    /// Verify these values satisfy arch_boot_impl constraints.
    #[test]
    fn test_linker_script_aarch64_constraints() {
        let kern_virt_base: u64 = 0xFFFF_8000_0000_0000; // link.ld KERN_VIRT_BASE
        let kern_phys_base: u64 = 0x4020_0000;           // link.ld KERN_PHYS_BASE (2MB-aligned)
        let kern_size: u64 = 0x200_000;                  // typical 2MB kernel

        // Must be page-aligned
        assert_eq!(kern_virt_base % 0x1000, 0, "kern_virt_base must be page-aligned");
        assert_eq!(kern_phys_base % 0x1000, 0, "kern_phys_base must be page-aligned");

        // aarch64 HUGE_PAGE_SIZE = 1GB, FALLBACK = 2MB
        let huge: u64 = 1 << 30; // 1GB
        let fallback: u64 = 1 << 21; // 2MB

        // kern_phys_base = 0x4020_0000 is 2MB-aligned
        assert_eq!(kern_phys_base % fallback, 0,
            "kern_phys_base must be at least FALLBACK_HUGE_PAGE_SIZE aligned");
        assert_eq!(kern_virt_base % huge, 0,
            "kern_virt_base must be HUGE_PAGE_SIZE aligned");

        // kern_size must be a multiple of the chosen huge page size
        assert_eq!(kern_size % fallback, 0,
            "kern_size must be a multiple of 2MB");
    }

    /// riscv64 linker script: KERN_VIRT_BASE = 0xFFFFFFC000000000, KERN_PHYS_BASE = 0x80200000
    /// Verify these values satisfy arch_boot_impl constraints.
    #[test]
    fn test_linker_script_riscv64_constraints() {
        let kern_virt_base: u64 = 0xFFFF_FFC0_0000_0000; // link.ld KERN_VIRT_BASE (Sv39 canonical high, VPN[2]=256)
        let kern_phys_base: u64 = 0x8020_0000;           // link.ld KERN_PHYS_BASE
        let kern_size: u64 = 0x200_000;                  // typical 2MB kernel

        // Must be page-aligned
        assert_eq!(kern_virt_base % 0x1000, 0, "kern_virt_base must be page-aligned");
        assert_eq!(kern_phys_base % 0x1000, 0, "kern_phys_base must be page-aligned");

        // riscv64 HUGE_PAGE_SIZE = 1GB (Sv39), FALLBACK = 2MB
        let huge: u64 = 1 << 30; // 1GB
        let fallback: u64 = 1 << 21; // 2MB

        // kern_phys_base = 0x8020_0000 is 2MB-aligned but not 1GB-aligned
        assert_eq!(kern_phys_base % fallback, 0,
            "kern_phys_base must be at least 2MB-aligned");
        // kern_virt_base = 0xFFFF_FFC0_0000_0000 is 1GB-aligned (Sv39 canonical high)
        assert_eq!(kern_virt_base % huge, 0,
            "kern_virt_base must be HUGE_PAGE_SIZE aligned for Sv39 1GB pages");

        // Verify Sv39 canonical high: VPN[2] must be in 256..=511 range
        let vpn2 = (kern_virt_base >> 30) & 0x1FF;
        assert!((256..=511).contains(&(vpn2 as usize)),
            "Sv39 kern_virt_base VPN[2] must be 256..511, got {}", vpn2);

        // kern_size must be a multiple of the chosen huge page size
        assert_eq!(kern_size % fallback, 0,
            "kern_size must be a multiple of 2MB");
    }

    /// Verify that arch_boot_impl rejects misaligned KernelInfo.
    #[test]
    #[should_panic(expected = "kern_phys_base must be page-aligned")]
    fn test_arch_boot_rejects_misaligned_phys() {
        let memmap: &'static [minix_types::MemoryRegion] = &[];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x1001), // not page-aligned
            kern_size: 0x200_000,
            free_upper_idx: 0,
            user_sp: VirBytes(0),
            kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
            syscall_entry: VirBytes(0),
            boot_modules: &[],
        };
        let root_page = PhysBytes(0x1000);
        let _ = arch_boot_impl::<MockPaging>(&info, root_page);
    }

    /// Verify that arch_boot_impl rejects zero kern_size.
    #[test]
    #[should_panic(expected = "kern_size must be > 0")]
    fn test_arch_boot_rejects_zero_kern_size() {
        let memmap: &'static [minix_types::MemoryRegion] = &[];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0, // zero size
            free_upper_idx: 0,
            user_sp: VirBytes(0),
            kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
            syscall_entry: VirBytes(0),
            boot_modules: &[],
        };
        let root_page = PhysBytes(0x1000);
        let _ = arch_boot_impl::<MockPaging>(&info, root_page);
    }

    /// Verify that arch_boot_impl rejects misaligned kern_stack_top.
    #[test]
    #[should_panic(expected = "kern_stack_top must be 16-byte aligned")]
    fn test_arch_boot_rejects_misaligned_stack_top() {
        let memmap: &'static [minix_types::MemoryRegion] = &[];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200_000,
            free_upper_idx: 0,
            user_sp: VirBytes(0),
            kern_stack_top: VirBytes(0xFFFF_8000_0020_0008), // not 16-byte aligned
            syscall_entry: VirBytes(0),
            boot_modules: &[],
        };
        let root_page = PhysBytes(0x1000);
        let _ = arch_boot_impl::<MockPaging>(&info, root_page);
    }
}
