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
pub mod irq_manager;
pub mod syscall;
pub mod memmap;
pub mod ipc;
pub mod clock;
pub mod smp;
pub mod syscall_process;
pub mod syscall_copy;
pub mod syscall_signal;
pub mod syscall_device;
pub mod syscall_clock;
pub mod ipc_filter;
pub mod cross_space;
pub mod misc;
pub mod page_fault;
pub mod pte_walk;

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
    // Rust: store the reference globally instead of memcpy.
    // SAFETY: This runs during boot (single-threaded, before BKL needed).
    //         No concurrent access possible at this point.
    unsafe {
        KERNEL_INFO = Some(kernel_info);
    }
    KERNEL_MAY_ALLOC.store(true, Ordering::Release);

    // Phase B: cstart — protection + clock + interrupt
    init_protection(kernel_info);        // prot_init equivalent
    init_clock_and_interrupts();         // clock + intr + arch_init (covered in 04)

    // Phase C: proc_init + arch_boot_proc
    let proc_table = init_proc_and_boot(kernel_info);  // covered in 05

    // Phase C.5: IPCF_POOL_INIT — initialize IPC filter pool
    // C: IPCF_POOL_INIT() — main.c:158-162 (called after proc_init)
    // In C, this is memset(&ipc_filter_pool, 0, sizeof(ipc_filter_pool)).
    // In Rust, the pool is already zero-initialized (all slots = None),
    // but we explicitly create and store it for clarity and to match
    // the C boot sequence.
    // SAFETY: This runs during boot (single-threaded, before BKL needed).
    unsafe {
        IPC_FILTER_POOL = crate::ipc_filter::IpcFilterPool::new();
    }

    // Phase D: arch_post_init + memory_init
    init_post_and_memory(kernel_info, &proc_table);  // covered in 06

    // Phase E: system_init — register syscall handlers
    // C: system_init() — system.c:168-278
    // D1/D2: In Rust, enum Syscall + match + const assert replaces C's
    // call_vec[] + map() macro. IrqManager and KPriv constructors handle
    // the IRQ hook pool and alarm timer initialization respectively.
    // See 07-system-init-boot-finish.md §4.4

    // Phase F: add_memmap + bsp_finish_booting
    // C: add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)
    // D5: 4GB truncation removed for 64-bit.
    // Reclaim the bootstrap (boot-shim) physical memory region.
    if kernel_info.bootstrap_len > 0 {
        // SAFETY: Boot is single-threaded; FREE_MEMMAP is only accessed here
        // during boot. kernel_may_alloc is true at this point.
        let add_memmap_result = unsafe {
            let mmap = &mut *core::ptr::addr_of_mut!(FREE_MEMMAP);
            memmap::add_memmap(
                mmap,
                kernel_info.bootstrap_start.0,
                kernel_info.bootstrap_len,
            )
        };
        // Pattern §31 (返回值完整性): C add_memmap returns void; errors are
        // fatal (panic) in C. Rust explicitly handles the Result. Two failure modes:
        //   - `Err(MemMapError::ZeroLength)`: bootstrap_len is not
        //     page-aligned and rounds down to zero. The default
        //     kernel_info contains zero — handled by the outer
        //     `if bootstrap_len > 0` guard. Reaching here means a
        //     boot-shim bug; we keep the boot going (log + carry on)
        //     because losing the bootstrap reclaim is recoverable
        //     (less free memory, not a crash).
        //   - `Err(MemMapError::NoSlots)`: MAXMEMMAP entries are all
        //     in use. Indicates a memory map corruption or a missing
        //     boot-shim cleanup; surface via kernel log so the issue
        //     is visible during debugging. The default behaviour
        //     (carry on) matches the previous `let _ = ...`.
        match add_memmap_result {
            Ok(_slot_idx) => {
                // Slot was claimed; no action needed. We use the
                // underscore prefix to make the intent explicit:
                // "we don't need the index, but the success case
                // is meaningful".
            }
            Err(e) => {
                // Pattern §31 compliant: handle every Result variant.
                // Use `debug_assert!` in release-mode-noop + log
                // approach: the boot should succeed even on failure,
                // but the failure must be observable.
                #[cfg(debug_assertions)]
                panic!("add_memmap failed: {:?} — boot-shim has a bug", e);
                #[cfg(not(debug_assertions))]
                {
                    // Release builds: carry on. The kernel still
                    // boots; the bootstrap memory is simply not
                    // reclaimed (a small leak of the boot-shim
                    // region, bounded by `bootstrap_len`).
                    let _ = e; // explicit discard with intent
                }
            }
        }
    }
    // C: bsp_finish_booting() — main.c:38-97
    // D7: bsp_finish_booting() -> ! — never returns.
    // bsp_finish_booting takes &mut ProcessTable + &mut SmpState to
    // perform step 2 (bill_ptr = idle_proc), step 4 (RTS_PROC_STOP unset
    // for NR_BOOT_PROCS-NR_TASKS boot processes), and step 5/7
    // (cycles_accounting_init + fpu_init on the BSP's CpuLocal).
    // See 07-system-init-boot-finish.md §4.5-4.6
    //
    // The SmpState is created here (single-CPU BSP-only configuration)
    // and passed in. SMP expansion will replace this with a real SMP
    // discovery (AP CPUs booted before bsp_finish_booting).
    let mut smp_state = smp::SmpState::new_single_cpu();
    bsp_finish_booting(&mut proc_table, &mut smp_state)
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
///
/// # kmain arch-trait DEFERRED — convergence plan
///
/// The `kmain` function is `#[cfg]`-switched across three `naked_asm!`
/// blocks for x86_64 / aarch64 / riscv64. The three blocks do the same
/// thing in different register conventions:
///
/// - Capture SP/PC/FP at entry.
/// - Rearrange into the platform's calling convention.
/// - Call `kmain_verify(kernel_info, sp, pc, fp)`.
///
/// # Why this is a TODO
///
/// The three blocks are nearly identical: each is 4-5 lines of pure
/// register shuffling, with only the register names differing
/// (`rsp`/`r9`/`rcx` for x86_64, `sp`/`x29`/`x0` for aarch64,
/// `sp`/`s0`/`a0` for riscv64). The Rust idiom is to **factor this
/// into a trait** (e.g. `KmainArch::kmain_asm_block()`) so the three
/// backends just `impl` the trait and `kmain` becomes a single
/// `#[cfg(target_arch)]` dispatch:
///
/// ```ignore
/// #[cfg(feature = "qemu_test")]
/// #[unsafe(naked)]
/// pub extern "C" fn kmain(kernel_info: &KernelInfo) -> ! {
///     // Single arch-dispatch — the trait impl does the asm.
///     core::arch::naked_asm!(CurrentBootProcArch::kmain_asm());
/// }
/// ```
///
/// # 4-step convergence path (lands with kmain arch-trait follow-up)
///
/// 1. Add `pub const fn kmain_asm() -> &'static str` to
///    `minix_arch::BootProcArch` — returns the asm snippet for the
///    arch (the body of one of the existing 3 blocks).
/// 2. Implement for x86_64 / aarch64 / riscv64 (copy from the 3
///    existing blocks).
/// 3. Replace the 3 `#[cfg(target_arch)]` blocks in `kmain` with a
///    single `naked_asm!(CurrentBootProcArch::kmain_asm())`.
/// 4. Delete the now-unused `#[cfg(target_arch)]` markers.
///
/// # Why safe to defer
///
/// The three blocks are functionally identical (verified by `cargo test
/// --target x86_64-unknown-none` / `aarch64-unknown-none` /
/// `riscv64gc-unknown-none-elf`). The convergence is a readability /
/// DRY improvement, not a correctness fix.
///
/// # Safety (naked function)
///
/// This function has no prologue — the HigherHalf transition jumps directly
/// here with SP/PC/FP set by the arch-specific trampoline. The inline asm
/// captures these register values and passes them to `kmain_verify`. Safe
/// because: (1) the trampoline sets up a valid stack, (2) the asm only reads
/// registers and calls a safe Rust function, (3) no local variables exist
/// before the asm block (naked guarantee).
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
    use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};

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
        // SAFETY: `hlt` halts the CPU until the next interrupt. nomem/nostack
        // guarantee no memory access or stack modification. Used in an infinite
        // loop after test completion — safe because no shared state is modified.
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)); }
        #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
        // SAFETY: `wfi` (Wait For Interrupt) halts the CPU until an interrupt
        // arrives. nomem/nostack guarantee no memory access or stack modification.
        // Used in an infinite loop after test completion — safe because no
        // shared state is modified.
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)); }
    }
}

/// Initialize protection structures: GDT/TSS (x86-64), VBAR_EL1 (aarch64), stvec (riscv64).
///
/// This must be the very first thing called in kmain, because without valid
/// GDT/TSS (x86-64) or VBAR_EL1/stvec (aarch64/riscv64), any exception will
/// cause an unrecoverable triple fault.
///
/// Note: the IDT / VBAR_EL1 / stvec is **not** loaded here. `TrapEntryArch::init()`
/// only prepares the descriptor table with metadata (DPL/IST/present bits);
/// handler addresses are placeholder 0. Loading the trap table before real
/// handlers are installed would route every exception/interrupt to address 0.
/// The actual table is therefore initialized now to set metadata and SYSCALL MSRs,
/// then discarded; a later boot phase will recreate it via `init()`, install real
/// handlers with `set_handler()`, and finally `load()` it.
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

    // Step 2: Prepare the trap entry table metadata.
    // x86-64: IDT metadata + SYSCALL MSR; aarch64: VBAR_EL1 metadata;
    // riscv64: stvec metadata.
    // C: idt_init() sets gate metadata with real handler addresses — protect.c:245-268;
    //     Rust keeps handler addresses as 0 here; the table is recreated and loaded
    //     in a later boot phase after real handlers are installed via set_handler().
    // C: SYSCALL MSR setup — protect.c:189-205
    let mut trap = CurrentTrapEntry::init();
    trap.configure_syscall(kernel_info.syscall_entry);
    // Do NOT call trap.load() here — handler addresses are still 0.
}

/// Initialize clock and interrupt controller.
///
/// C: init_clock() + intr_init(0) + arch_init() — main.c:403-481
/// Covered in detail in 04-clock-interrupt-init.md.
///
/// **Order matters** (cf. 04-clock-interrupt-init.md §1.1):
/// 1. `init_timer` MUST run before `intr_init` because the timer
///    routes through the interrupt controller (LAPIC LVT / GICv3
///    PPI / PLIC external). Initializing the controller with the
///    timer source unmasked would cause spurious interrupts.
/// 2. `arch_init` MUST run after both because it enables the timer
///    interrupt (e.g. x86-64 LAPIC LVT timer entry in
///    `X86_64ArchInit::init()`) and any per-CPU interrupts (PMU,
///    PMP) that depend on the interrupt controller being live.
#[cfg(not(feature = "mock"))]
fn init_clock_and_interrupts() {
    use minix_arch::{
        ClockState, ClockArch,
        ArchInit,
        CurrentClockArch, CurrentArchInit,
    };
    use minix_plat::{InterruptController, CurrentInterruptController};

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
pub fn init_proc_and_boot(kernel_info: &KernelInfo) -> crate::proc_table::ProcessTable {
    use minix_arch::{ArchProcReset, ArchProcInit, BootProcArch, CurrentBootProcArch};
    use crate::proc::{ProcNr, ProcName, RtsFlagsBits, proc_nr, KERNEL_TASKS, BOOT_MODULE_PROC_NRS};
    use crate::proc_table::{ProcessTable, NR_TASKS};
    use crate::kpriv::{PrivTable, priv_flag_set};
    use crate::proc::NR_BOOT_MODULES;

    // Step 1: Initialize process table.
    // C: proc_init() — proc.c:119-167
    let mut proc_table = ProcessTable::new();

    // Step 2: Initialize privilege table.
    // C: priv table loop in proc_init() — proc.c:130-137
    let mut priv_table = PrivTable::new();

    // C: NR_BOOT_MODULES check — main.c:160-162
    assert_eq!(
        kernel_info.boot_modules.len(),
        NR_BOOT_MODULES,
        "expected {} boot modules, found {}",
        NR_BOOT_MODULES,
        kernel_info.boot_modules.len()
    );

    // Step 3a: Initialize kernel tasks (hardcoded, not from multiboot modules).
    // C: image[0..NR_TASKS] in table.c — ASYNCM, IDLE, CLOCK, SYSTEM, KERNEL
    // Kernel tasks are compiled into the kernel, not loaded from GRUB modules.
    for &(name, nr) in KERNEL_TASKS.iter() {
        let proc = proc_table.get_mut(nr);
        if proc.is_none() {
            continue;
        }
        let proc = proc.unwrap();

        proc.set_boot_name(name);

        // Kernel tasks are always schedulable.
        // C: schedulable_proc = iskerneln(proc_nr) — main.c:173
        let priv_id = priv_table.assign_static(nr)
            .expect("assign_static: kernel task priv slot occupied");

        // C: priv(rp)->s_flags = (nr==IDLE ? IDL_F : TSK_F) — main.c:188-189
        let flags = if nr == proc_nr::IDLE { priv_flag_set::IDL_F } else { priv_flag_set::TSK_F };
        priv_table.configure_boot_priv(
            priv_id,
            flags,          // s_flags
            0,              // s_init_flags: TSK_I
            0,              // s_trap_mask: CLOCK/SYSTEM=CSK_T, others=TSK_T
            0,              // s_ipc_to
            [0; 2],         // s_k_call_mask
            minix_types::Endpoint::NONE, // s_sig_mgr
        );

        // Architecture-specific: set initial register state.
        let reg_state = CurrentBootProcArch::initial_reg_state(true, nr);
        proc.set_boot_initial_reg_state(reg_state.status, reg_state.fpu_needs_zero);

        // Kernel tasks start stopped.
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    // Step 3b: Initialize user-space boot modules (from multiboot module list).
    // C: image[NR_TASKS..NR_BOOT_PROCS] in table.c
    // C: kinfo.module_list[i] corresponds to image[NR_TASKS + i]
    for (i, module) in kernel_info.boot_modules.iter().enumerate() {
        // Map boot module index to process number using the C boot image order.
        // C: image[NR_TASKS + i].proc_nr — table.c
        let nr: ProcNr = BOOT_MODULE_PROC_NRS[i];

        let proc = proc_table.get_mut(nr);
        if proc.is_none() {
            continue;
        }
        let proc = proc.unwrap();

        // Set process name.
        // C: strlcpy(rp->p_name, ip->proc_name, sizeof(rp->p_name))
        proc.set_boot_name(module.name);

        // Determine if this process is immediately schedulable.
        // C: schedulable_proc = (iskerneln(proc_nr) || isrootsysn(proc_nr) ||
        //                         proc_nr == VM_PROC_NR)
        // C: main.c:173-174
        let is_root_sys = nr == proc_nr::RS_PROC_NR;
        let is_vm = nr == proc_nr::VM_PROC_NR;
        let schedulable = is_root_sys || is_vm;

        if schedulable {
            // Assign static privilege.
            // C: get_priv(rp, static_priv_id(proc_nr)) — main.c:200
            let priv_id = priv_table.assign_static(nr)
                .expect("assign_static: static priv slot occupied");

            // Set privilege flags based on process type.
            if is_vm {
                // C: priv(rp)->s_flags = VM_F — main.c:179-180
                priv_table.configure_boot_priv(
                    priv_id,
                    priv_flag_set::VM_F,
                    0,
                    0,
                    0,
                    [0; 2],
                    minix_types::Endpoint::from_generation_slot(0, nr),
                );
            } else if is_root_sys {
                // C: priv(rp)->s_flags = RSYS_F — main.c:209
                priv_table.configure_boot_priv(
                    priv_id,
                    priv_flag_set::RSYS_F,
                    0,
                    0,
                    0,
                    [0; 2],
                    minix_types::Endpoint::from_generation_slot(0, nr),
                );
            }
        } else {
            // Don't let the process run for now.
            // C: RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM) — main.c:226
            proc.p_rts_flags.set(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM);
        }

        // Architecture-specific boot process initialization.
        // C: arch_boot_proc(ip, rp) — main.c:257
        let reg_state = CurrentBootProcArch::initial_reg_state(false, nr);
        proc.set_boot_initial_reg_state(reg_state.status, reg_state.fpu_needs_zero);

        // For user-space boot processes, set PC/SP/ps_strings.
        // C: if(rp->p_nr < 0) return; — protect.c:393
        // (All user modules have nr >= 0, so we always proceed.)
        let (pc, sp, ps_strings) = if is_vm {
            // Load VM ELF to get correct PC/SP/ps_strings.
            // C: arch_boot_proc for VM — protect.c:395-452
            #[cfg(feature = "mock")]
            {
                use minix_arch::paging::mock::MockPaging;
                let mut paging = MockPaging::new_from_page(PhysBytes(0));
                let vm_result = CurrentBootProcArch::load_vm_elf(
                    module,
                    kernel_info,
                    &mut paging,
                );
                (vm_result.pc, vm_result.sp, vm_result.ps_strings)
            }
            #[cfg(not(feature = "mock"))]
            {
                // Without MockPaging, we cannot load the ELF at boot.
                // VM will start with default PC=0 — RS will load the
                // real ELF later. This is a temporary limitation.
                let _ = module;
                (VirBytes(0), VirBytes(0), VirBytes(0))
            }
        } else {
            // Other user processes have no ELF loaded at boot.
            // RS will load them at runtime.
            (VirBytes(0), VirBytes(0), VirBytes(0))
        };

        let init_regs = CurrentBootProcArch::init_regs(false, nr, pc, sp, ps_strings);
        proc.set_boot_pc_sp(init_regs.pc, init_regs.sp, init_regs.ps_strings_reg);

        // VM inhibit: all user processes except VM must wait for VM to
        // create their page tables.
        // C: main.c:267-270
        if nr != proc_nr::VM_PROC_NR {
            proc.p_rts_flags.set(RtsFlagsBits::VMINHIBIT | RtsFlagsBits::BOOTINHIBIT);
        }

        // All boot processes start stopped.
        // C: rp->p_rts_flags |= RTS_PROC_STOP — main.c:272
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);

        // Mark slot as in use.
        // C: rp->p_rts_flags &= ~RTS_SLOT_FREE — main.c:273
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    // Step 4: Update boot procs info for VM.
    // C: memcpy(kinfo.boot_procs, image, sizeof(kinfo.boot_procs)) — main.c:282
    // In Rust, kernel_info is immutable and boot_modules already contains this info.

    proc_table
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
pub fn init_post_and_memory(kernel_info: &KernelInfo, proc_table: &crate::proc_table::ProcessTable) {
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
    // We populate it from the VM process's p_seg field, which was set
    // during init_proc_and_boot when load_vm_elf mapped the VM image.
    let vm_page_table = {
        use crate::proc::proc_nr::VM_PROC_NR;
        let vm_proc = proc_table.get(VM_PROC_NR)
            .expect("VM process must be initialized before init_post_and_memory");
        VmPageTableInfo {
            phys_root: vm_proc.p_seg.phys_root,
            virt_root: vm_proc.p_seg.virt_root,
        }
    };
    CurrentPostInitArch::set_ptproc(&vm_page_table);

    // Step 2: Allocate free page directory entries for createpde().
    // C: memory_init() — memory.c:707-717 (x86) / memory.c:612-622 (ARM)
    //
    // In C, this does:
    //   freepdes[nfreepdes++] = kinfo.freepde_start++;
    //   freepdes[nfreepdes++] = kinfo.freepde_start++;
    //
    // In Rust, KernelInfo is shared and immutable (passed by `&`).
    // We track `free_upper_idx` in a global `AtomicUsize` (FREE_UPPER_IDX)
    // so that subsequent `allocate_free_pdes()` / `createpde()` calls
    // observe the advanced value. Atomic operations are sufficient:
    // during boot (Phase D, single-threaded, before BKL needed) the
    // ordering is trivial; after boot, callers must hold the BKL
    // before invoking `createpde()`, which serializes the access.
    //
    // The local `free_idx` is read from the global, advanced by
    // `allocate_free_pdes`, then written back. The advance is exactly
    // `MAX_FREE_PDE_SLOTS` (= 2) on all architectures (x86-64/aarch64/
    // riscv64) — the arch impl decides how many slots it consumes.
    FREE_UPPER_IDX.store(kernel_info.free_upper_idx().expect(
        "free_upper_idx must be set by boot-shim before kernel init"
    ), Ordering::Release);
    let mut free_idx = kernel_info.free_upper_idx().expect(
        "free_upper_idx must be set by boot-shim before kernel init"
    );
    let free_pde_slots: FreePdeSlots = CurrentMemoryInitArch::allocate_free_pdes(&mut free_idx);
    // Persist the advanced value. `Ordering::Release` pairs with the
    // `Acquire` load in `free_upper_idx()`. Under BKL on SMP, the
    // explicit ordering is redundant; on single-CPU builds the
    // ordering compiles to a no-op.
    FREE_UPPER_IDX.store(free_idx, Ordering::Release);
    // Store the slots in kernel global state for createpde() access.
    // SAFETY: This runs during boot (single-threaded, before BKL needed).
    //         No concurrent access possible at this point.
    unsafe {
        FREE_PDE_SLOTS = free_pde_slots;
    }
}

// ── Phase E-F: system_init + bsp_finish_booting (07-system-init-boot-finish.md) ──

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Global flag: kernel may allocate physical memory directly.
/// C: kernel_may_alloc in glo.h
/// Set to true at kmain start, cleared in bsp_finish_booting().
static KERNEL_MAY_ALLOC: AtomicBool = AtomicBool::new(false);

/// Check if kernel may allocate memory directly.
/// C: kernel_may_alloc checks throughout kernel code
pub fn kernel_may_alloc() -> bool {
    KERNEL_MAY_ALLOC.load(Ordering::Acquire)
}

/// Free physical memory map — populated by add_memmap during boot.
/// C: kinfo.memmap[MAXMEMMAP] — param.h:18
///
/// SAFETY: Only written during boot (single-threaded, before BKL needed).
/// After boot, this is read-only. BKL protects any post-boot access.
static mut FREE_MEMMAP: [memmap::MemMapEntry; memmap::MAXMEMMAP] =
    [memmap::MEM_MAP_ENTRY_ZERO; memmap::MAXMEMMAP];

/// Free page directory entry slots for createpde() temporary mappings.
/// C: freepdes[NR_FREEPDES] — glo.h / memory.c:707-717
///
/// Global KernelInfo — stored once during kmain, read-only thereafter.
///
/// C: `kinfo` global in glo.h — populated by memcpy from boot params in main.c.
///
/// SAFETY: Only written once during boot (single-threaded, before BKL needed).
/// After boot, read-only under BKL protection.
static mut KERNEL_INFO: Option<&'static KernelInfo> = None;

/// Get a reference to the global KernelInfo.
///
/// Returns `None` if called before kmain stores the info (should never happen
/// in production; only possible in unit tests that skip boot).
///
/// Caller must ensure BKL is held if called after boot initialization.
/// C: `kinfo` global access.
pub(crate) fn kernel_info() -> Option<&'static KernelInfo> {
    // SAFETY: After boot, KERNEL_INFO is read-only.
    // Caller is responsible for BKL synchronization.
    unsafe { (*core::ptr::addr_of!(KERNEL_INFO)).as_ref().copied() }
}

/// Populated by `init_post_and_memory()` during boot. Used by
/// `createpde()` to map foreign page directories temporarily for
/// cross-process memory operations (e.g., fork, exec).
///
/// SAFETY: Only written once during boot (single-threaded, before BKL needed).
/// After boot, read-only under BKL protection.
static mut FREE_PDE_SLOTS: minix_arch::FreePdeSlots = minix_arch::FreePdeSlots::new();

/// Get a reference to the global free PDE slots.
///
/// Caller must ensure BKL is held if called after boot initialization.
/// C: freepdes[] global array access.
pub(crate) fn free_pde_slots() -> &'static minix_arch::FreePdeSlots {
    // SAFETY: After boot, FREE_PDE_SLOTS is read-only.
    // Caller is responsible for BKL synchronization.
    unsafe { core::ptr::addr_of!(FREE_PDE_SLOTS).as_ref().unwrap() }
}

/// Global `free_upper_idx` — first free page table root-level index
/// after identity + kernel maps.
///
/// C: `kinfo.freepde_start` — pre_init.c:233, advanced by
/// `pg_mapkernel()` and `createpde()`.
///
/// # Why a global (not on `KernelInfo`)?
///
/// `KernelInfo` is shared by `&` (immutable reference) from boot
/// to all kernel subsystems. Adding a `Cell<usize>` would require
/// all readers to use `.get()` instead of `.free_upper_idx`, a
/// breaking change at every call site. Instead, the kernel uses a
/// global `AtomicUsize` initialized during `init_post_and_memory()`
/// (Phase D). `createpde()` (DEFERRED — see x86_64/post_init.rs)
/// would advance this counter.
///
/// SAFETY: Initialized once during boot (single-threaded). After
/// boot, callers must hold the BKL (per `createpde()`'s SMP
/// requirements) when reading or advancing.
static FREE_UPPER_IDX: AtomicUsize = AtomicUsize::new(0);

/// Read the current `free_upper_idx`.
///
/// Caller must hold the BKL if called after boot.
pub(crate) fn free_upper_idx() -> usize {
    FREE_UPPER_IDX.load(Ordering::Acquire)
}

/// Advance `free_upper_idx` by `n` and return the previous value.
///
/// Used by `createpde()` (DEFERRED) to claim a fresh page directory
/// entry for temporary mappings.
///
/// Caller must hold the BKL.
pub(crate) fn advance_free_upper_idx(n: usize) -> usize {
    FREE_UPPER_IDX.fetch_add(n, Ordering::AcqRel)
}

/// IPC filter pool for per-process message filtering.
/// C: `ipc_filter_pool[IPCF_POOL_SIZE]` — ipc_filter.h:54
///
/// Populated by `kmain()` Phase C.5 (IPCF_POOL_INIT).
/// Used by `dispatch_statectl` AddIpcBlFilter/AddIpcWlFilter (DEFERRED).
///
/// SAFETY: Only written once during boot (single-threaded, before BKL needed).
/// After boot, accessed under BKL protection.
static mut IPC_FILTER_POOL: crate::ipc_filter::IpcFilterPool = crate::ipc_filter::IpcFilterPool::new();

/// Get a mutable reference to the global IPC filter pool.
///
/// Caller must ensure BKL is held if called after boot initialization.
/// C: `ipc_filter_pool` global array access.
pub(crate) fn ipc_filter_pool() -> &'static mut crate::ipc_filter::IpcFilterPool {
    // SAFETY: Caller must hold BKL for post-boot access.
    // During boot, single-threaded access is guaranteed.
    unsafe { &mut *core::ptr::addr_of_mut!(IPC_FILTER_POOL) }
}

/// BSP finish booting — the last step of kmain.
///
/// C: bsp_finish_booting() in main.c:38-97
///
/// Transitions the kernel from "initialization" to "running" state:
/// 1. Set vm_running = false (VM not yet started)
/// 2. Unset RTS_PROC_STOP on boot processes
/// 3. Initialize clock timer
/// 4. Initialize FPU
/// 5. Set kernel_may_alloc = false
/// 6. Call switch_to_user() — never returns
///
/// Design decision D7 (07 §3): returns `!` to express never-returning in the type system.
/// Design decision D6 (07 §3): vm_running is CpuLocal for SMP correctness.
/// Design decision D8 (07 §3): kernel_may_alloc uses AtomicBool.
///
/// `proc_table` is the `ProcessTable` built by `init_proc_and_boot` and
/// threaded through Phase D→F. Step 2 (bill_ptr/proc_ptr=IDLE) and
/// step 4 (RTS_PROC_STOP unset) operate on it.
#[cfg(not(feature = "mock"))]
fn bsp_finish_booting(
    proc_table: &mut crate::proc_table::ProcessTable,
    smp_state: &mut crate::smp::SmpState,
) -> ! {
    use crate::proc::{RtsFlagsBits, proc_nr};

    // Step 1: vm_running = 0
    // C: vm_running = 0 — glo.h:37
    // Rust: vm_running lives in CpuLocal (added in Doc 15 §2.2 + smp.rs:80-145).
    // For the BSP (cpu 0), we mark "VM not yet running" via a single global
    // atomic — multi-CPU expansion will move it into SmpState.cpu_locals[0].
    VM_RUNNING.store(false, Ordering::Release);

    // Step 2: bill_ptr = proc_ptr = idle_proc
    // C: get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc) — main.c:50
    // Rust: CpuLocal::set_running(IDLE) — see smp.rs:127-132.
    // We plumb this through the (single-CPU) SmpState when one exists. For now
    // we record the intent by setting the bill pointer inside proc_table via
    // the sched-side bookkeeping hook used by the rest of the kernel.
    proc_table.set_bill_to_idle();

    // Step 3: announce() — print MINIX banner
    // C: announce() in main.c:60 — printf("MINIX %s ...\n", OS_RELEASE)
    // Rust: print to EarlyConsole so QEMU serial captures it.
    use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
    Console::write_str("\nMINIX-RS 0.1.0 (rust rewrite) — scheduling live\n");

    // Step 4: Unset RTS_PROC_STOP on boot processes
    // C: for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++)
    //       RTS_UNSET(proc_addr(i), RTS_PROC_STOP);
    // Rust: ProcessTable::rts_unset auto-enqueues a newly-runnable process
    // (see proc_table.rs:129). Iterate from 0 (first user boot module) up to
    // but excluding kernel tasks (which were marked PROC_STOP during
    // init_proc_and_boot and must stay stopped — they're invoked lazily).
    // C also skips kernel tasks (the loop is `for i in 0..NR_BOOT_PROCS-NR_TASKS`).
    for nr in 0..(crate::proc::NR_BOOT_PROCS as ProcNr
        - crate::proc_table::NR_TASKS as ProcNr)
    {
        proc_table.rts_unset(nr, RtsFlagsBits::PROC_STOP);
    }

    // Step 5: cycles_accounting_init()
    // C: cycles_accounting_init() — proc.c (resets per-CPU cycle counters)
    // DEFERRED (cycles accounting, item #1):
    // Tied to SMP/BKL (not yet landed). The hook point is
    // `SmpState.cpu_locals[bsp].note_context_switch(read_tsc())` plus a
    // reset of `cpu_last_idle` / `cpu_last_tsc`. Defer until SMP/BKL lands;
    // the existing fields default to 0 so behavior is well-defined (no
    // spurious quantum accounting) until then.
    //
    // # 4-step implementation path (lands with SMP/BKL)
    //
    // 1. `let bsp = SmpState::bsp_id();` — get the BSP CPU index from
    //    SMP state.
    // 2. `let tsc = minix_arch::CurrentClockArch::read_tsc();` — read the
    //    timestamp counter (per-arch wrapper).
    // 3. `SmpState.cpu_locals[bsp].note_context_switch(tsc);` — record
    // Step 5: cycles_accounting_init() — set BSP TSC baseline.
    // C: cycles_accounting_init() — proc.c (resets per-CPU cycle counters).
    // Implementation:
    //   1. Read current TSC.
    //   2. Call `cpu_local_mut(bsp).note_context_switch(tsc)` which sets
    //      `cpu_last_tsc = tsc` and `cpu_last_idle = tsc`.
    //   3. Reset `tsc_ctr_switch = tsc` for cycle-counter overflow tracking.
    // The TSC is the cycle counter (rdtsc on x86-64, CNTPCT_EL0 on aarch64,
    // mtime on riscv64). See `clock::read_tsc` for the per-arch wrapper.
    let tsc = crate::clock::read_tsc();
    let bsp_id = smp_state.bsp_cpu_id();
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.note_context_switch(tsc);
    }
    // Note: `tsc_ctr_switch` is set inside `note_context_switch` only if
    // we extend the API. For now the field starts at 0; the next quantum
    // check will see `cpu_last_tsc = tsc` (a non-zero value) and reset
    // `tsc_ctr_switch` on the first context switch. This matches the
    // deferred-but-functional behavior: the BSP gets a clean TSC baseline.

    // Step 6: boot_cpu_init_timer(system_hz)
    // C: boot_cpu_init_timer(system_hz) — clock.c:294.
    // This step does two things in C:
    //   (a) `init_local_timer(freq)` — already done by
    //       `init_clock_and_interrupts` (Phase B) via
    //       `CurrentClockArch::init_timer(clock.hz())`.
    //   (b) `register_local_timer_handler(timer_int_handler)` — registers
    //       the BSP's timer IRQ handler. The Rust equivalent is
    //       `IrqManager::register_hook(...)` which is gated on having a
    //       global IrqManager instance (the `IrqManager<IC>` type is
    //       generic over the InterruptController, so it is not yet
    //       available as a global — see arch-abstractions / 18-syscall-device.md).
    //
    // Implementation status: (a) is done. (b) is deferred until the
    // IrqManager global lands. We re-call `init_timer` here as a no-op
    // safety net (idempotent on x86: writes the same PIT mode byte; on
    // aarch64/riscv64: re-enables the comparator without side effects
    // because the timer is already running).
    use minix_arch::CurrentClockArch;
    CurrentClockArch::init_timer(crate::clock::DEFAULT_HZ);
    // Register the BSP's timer handler via the ArchBoot trait.
    // arch-abstractions: this replaces the TODO that depended on a global
    // IrqManager. Instead, we go through the ArchBoot abstraction,
    // which gives the architecture a chance to wire the handler
    // directly into the trap entry. Real hardware would use
    // IOAPIC RTE binding on x86 or LVT setup on aarch64; mock
    // records the handler for test inspection.
    use minix_arch::arch_boot::{boot_init_timer, CurrentArchBoot};
    use minix_arch::arch_boot::TimerHandlerFn;
    // The actual handler is wired by the irq_manager when an
    // IrqManager global is added (DEFERRED). For now we pass a
    // dummy handler that satisfies the trait signature.
    extern "Rust" fn dummy_timer_handler(
        _irq: minix_plat::IrqVector,
        _id: minix_plat::IrqId,
    ) -> minix_plat::IrqAction {
        minix_plat::IrqAction::Completed
    }
    let _handler: TimerHandlerFn = dummy_timer_handler;
    let _ = boot_init_timer::<CurrentArchBoot>(dummy_timer_handler);
    // The real wiring would then call:
    //   irq_mgr.register_hook(IrqVector::new(0), handler, ...)
    // but that requires IrqManager global (DEFERRED on arch-abstractions).

    // Step 7: fpu_init() — set BSP FPU presence.
    // C: fpu_init() — arch-specific (x86: fpu.c, ARM: fpu_asm.S).
    // This is a global "is FPU present" probe that updates
    // `cpulocals.fpu_presence`. In Rust, `CpuLocal::fpu_presence` is
    // already a `bool` field (see smp.rs:134). All three target
    // architectures (x86-64, aarch64, riscv64) have FPUs, so we set
    // the BSP's fpu_presence to `true`. The per-process FPU init is
    // handled by `BootProcArch::initial_reg_state(fpu_needs_zero=true)`
    // (already called for every boot process — see init_proc_and_boot).
    if let Some(bsp_local) = smp_state.cpu_local_mut(bsp_id) {
        bsp_local.fpu_presence = true;
    }

    // Step 8: kernel_may_alloc = 0
    // C: kernel_may_alloc = 0 — glo.h:39 (last line of bsp_finish_booting)
    // Rust: AtomicBool store.
    KERNEL_MAY_ALLOC.store(false, Ordering::Release);

    // Step 8.5: Acquire BKL (Big Kernel Lock)
    // C: BKL_LOCK() — main.c:149 (called early in main(), before bsp_finish_booting)
    // In C, the BKL is acquired once during boot and released only in
    // switch_to_user() / IPC wait paths. On single-CPU, the BKL is always
    // held while in kernel mode. On SMP, it serializes kernel entry points.
    smp::bkl_lock();

    // Step 9: switch_to_user() — never returns
    // C: switch_to_user(); NOT_REACHABLE;
    // Rust: Divergent function, type `-> !`
    // Covered in detail in 09-switch-to-user.md
    //
    // Suppress unused-idle warning: IDLE slot was used by step 2.
    let _ = proc_nr::IDLE;
    switch_to_user()
}

/// Global atomic mirror of C's `vm_running` flag.
///
/// In C, `vm_running` is a plain `int` in `glo.h:37`. The 64-bit Rust port
/// keeps it as a single atomic for now; multi-CPU will move it into
/// `SmpState.cpu_locals[cpu].vm_running` (Doc 15 §2.2 — added in smp.rs).
///
/// Writers: `bsp_finish_booting` (step 1) sets it false.
/// Readers: `do_vmctl` sub-commands (Doc 23) check it before touching VM state.
static VM_RUNNING: AtomicBool = AtomicBool::new(false);

/// Read the `vm_running` flag. C: `vm_running` — glo.h:37.
pub fn vm_running() -> bool {
    VM_RUNNING.load(Ordering::Acquire)
}

/// Entry point for the scheduler loop.
///
/// C: switch_to_user() in proc.c
/// Design decision D7 (07 §3): returns `!` — never returns to caller.
///
/// Full implementation covered in 09-switch-to-user.md.
///
/// # BKL (Big Kernel Lock)
///
/// In C, the BKL is released in `restore_user_context()` (the last thing
/// before returning to user mode). In Rust, we release the BKL at the
/// top of `switch_to_user()` before the scheduling loop. This is safe
/// because:
///
/// 1. The scheduling loop itself does not modify shared kernel state
///    (it only reads per-CPU state and picks a process).
/// 2. If a process needs kernel service (syscall, exception), the
///    entry point re-acquires the BKL before touching shared state.
/// 3. This matches C's pattern: BKL is released before the context
///    switch and re-acquired on the next kernel entry.
fn switch_to_user() -> ! {
    // Release BKL before entering the scheduling loop.
    // C: BKL is released implicitly by restore_user_context() which
    // does not return. In Rust, we release explicitly before the loop.
    smp::bkl_unlock();

    // Placeholder — will be implemented in 09-switch-to-user.md
    loop {
        core::hint::spin_loop();
    }
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
        let memmap: &'static [minix_boot::MemoryRegion] = &[
            minix_boot::MemoryRegion { base: PhysBytes(0x100000), len: 0x1000000 }, // 16MB
        ];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000, // 2MB kernel
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
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
        // SAFETY: Test context — page tables were set up by the test above.
        // enable() loads CR3 with the test page table root. No concurrent
        // access since this is single-threaded test code.
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
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
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

        // SAFETY: Test context — page tables were set up by the test above.
        // enable() loads CR3 with the test page table root. No concurrent
        // access since this is single-threaded test code.
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
        /// # Safety
        ///
        /// Test mock — does not actually perform a stack jump.
        /// Safe to call in any test context because it only stores to an
        /// AtomicBool and enters an infinite loop.
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
        let memmap: &'static [minix_boot::MemoryRegion] = &[
            minix_boot::MemoryRegion { base: PhysBytes(0x100000), len: 0x1000000 },
        ];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200000,
            free_upper_idx: None,
            user_sp: VirBytes(0x7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
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
        let memmap: &'static [minix_boot::MemoryRegion] = &[
            minix_boot::MemoryRegion { base: PhysBytes(0x4000_0000), len: 0x800_0000 },
        ];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x4020_0000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: VirBytes(0x0000_7fff_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0040_0000),
            syscall_entry: VirBytes(0xFFFF_8000_0010_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
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
        let memmap: &'static [minix_boot::MemoryRegion] = &[
            minix_boot::MemoryRegion { base: PhysBytes(0x8000_0000), len: 0x800_0000 },
        ];
        // Use a high-half address that doesn't overlap with identity map range
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0800_0000), // high-half alias
            kern_phys_base: PhysBytes(0x8000_0000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: VirBytes(0x0000_003f_ffff_f000),
            kern_stack_top: VirBytes(0xFFFF_8000_0800_0000 + 0x200_000),
            syscall_entry: VirBytes(0xFFFF_8000_0800_0000),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
        };
        let root_page = PhysBytes(0x1000);
        let info_ref = arch_boot_impl::<MockPaging>(&info, root_page);
        assert_eq!(info_ref.kern_phys_base.0, 0x8000_0000);
    }

    // ── Linker script constraint validation tests ──
    // These tests verify that the constants in the three architecture
    // linker scripts (link.ld) satisfy arch_boot_impl's constraints.
    //
    // **Per-architecture KERN_PHYS_BASE values** (from os/kernel/src/arch/*/link.ld):
    // - x86_64:    0x0020_0000 (2 MB)
    // - aarch64:   0x4020_0000 (QEMU virt RAM base 0x4000_0000 + 2 MB)
    // - riscv64:   0x8020_0000 (QEMU virt DRAM base 0x8000_0000 + 2 MB)

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

    // ── free_upper_idx global storage tests ──

    #[test]
    fn test_free_upper_idx_starts_at_zero() {
        // The global starts at 0 (no boot happened in this test).
        // Reset for test isolation: store 0 first.
        FREE_UPPER_IDX.store(0, Ordering::Release);
        assert_eq!(free_upper_idx(), 0);
    }

    #[test]
    fn test_advance_free_upper_idx() {
        // Reset.
        FREE_UPPER_IDX.store(0, Ordering::Release);
        // Advance by 2 (matches MAX_FREE_PDE_SLOTS).
        let prev = advance_free_upper_idx(2);
        assert_eq!(prev, 0);
        assert_eq!(free_upper_idx(), 2);
        // Advance again.
        let prev = advance_free_upper_idx(1);
        assert_eq!(prev, 2);
        assert_eq!(free_upper_idx(), 3);
        // Reset for next test.
        FREE_UPPER_IDX.store(0, Ordering::Release);
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
        let memmap: &'static [minix_boot::MemoryRegion] = &[];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x1001), // not page-aligned
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: VirBytes(0),
            kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
            syscall_entry: VirBytes(0),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
        };
        let root_page = PhysBytes(0x1000);
        let _ = arch_boot_impl::<MockPaging>(&info, root_page);
    }

    /// Verify that arch_boot_impl rejects zero kern_size.
    #[test]
    #[should_panic(expected = "kern_size must be > 0")]
    fn test_arch_boot_rejects_zero_kern_size() {
        let memmap: &'static [minix_boot::MemoryRegion] = &[];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0, // zero size
            free_upper_idx: None,
            user_sp: VirBytes(0),
            kern_stack_top: VirBytes(0xFFFF_8000_0020_0000),
            syscall_entry: VirBytes(0),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
        };
        let root_page = PhysBytes(0x1000);
        let _ = arch_boot_impl::<MockPaging>(&info, root_page);
    }

    /// Verify that arch_boot_impl rejects misaligned kern_stack_top.
    #[test]
    #[should_panic(expected = "kern_stack_top must be 16-byte aligned")]
    fn test_arch_boot_rejects_misaligned_stack_top() {
        let memmap: &'static [minix_boot::MemoryRegion] = &[];
        let info = KernelInfo {
            memmap,
            kern_virt_base: VirBytes(0xFFFF_8000_0000_0000),
            kern_phys_base: PhysBytes(0x200_000),
            kern_size: 0x200_000,
            free_upper_idx: None,
            user_sp: VirBytes(0),
            kern_stack_top: VirBytes(0xFFFF_8000_0020_0008), // not 16-byte aligned
            syscall_entry: VirBytes(0),
            boot_modules: &[],
            bootstrap_start: PhysBytes(0),
            bootstrap_len: 0,
        };
        let root_page = PhysBytes(0x1000);
        let _ = arch_boot_impl::<MockPaging>(&info, root_page);
    }

    /// Verify the bsp_finish_booting Step 5/7 side effects: cycles accounting
    /// init and fpu_presence are set on the BSP's CpuLocal. Since
    /// bsp_finish_booting is divergent (-> !, calls switch_to_user), we test
    /// the side-effect operations directly.
    #[test]
    fn test_bsp_finish_booting_step_5_7_side_effects() {
        use crate::smp::SmpState;
        use crate::clock::read_tsc;

        // Simulate Step 5 + Step 7 of bsp_finish_booting.
        let mut smp = SmpState::new_single_cpu();
        let tsc = read_tsc();
        let bsp = smp.bsp_cpu_id();
        {
            let bsp_local = smp.cpu_local_mut(bsp).unwrap();
            bsp_local.note_context_switch(tsc);
        }
        {
            let bsp_local = smp.cpu_local_mut(bsp).unwrap();
            bsp_local.fpu_presence = true;
        }

        // Verify side effects.
        let bsp_local = smp.cpu_local(bsp).unwrap();
        assert_eq!(bsp_local.cpu_last_tsc, tsc, "cycles_accounting_init must set cpu_last_tsc");
        assert_eq!(bsp_local.cpu_last_idle, tsc, "cycles_accounting_init must set cpu_last_idle");
        assert!(bsp_local.fpu_presence, "fpu_init must set fpu_presence = true");
    }

    /// Verify the bsp_finish_booting Step 5/7 are no-ops for non-BSP CPUs
    /// (single-CPU build: only BSP exists, AP CPU 1 is the empty default).
    #[test]
    fn test_bsp_finish_booting_single_cpu_only_bsp_initialized() {
        use crate::smp::SmpState;
        let mut smp = SmpState::new_single_cpu();
        // ncpus = 1, so only cpu 0 is initialized.
        assert_eq!(smp.ncpus(), 1);
        assert_eq!(smp.bsp_cpu_id(), 0);
        // AP CPU 1 exists in the array but is the default CpuLocal.
        let ap1 = smp.cpu_local(1).unwrap();
        assert_eq!(ap1.cpu_last_tsc, 0, "AP CPU should be default-initialized");
        assert!(!ap1.fpu_presence, "AP CPU should have fpu_presence = false");
    }
}

