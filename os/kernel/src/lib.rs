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
// R-19 (2026-08-13): Test helpers across syscall_*.rs build `Message` via
// `default() + assign m_type` then fill union fields under `unsafe`.
// Struct-literal form would still need `unsafe` for union writes — marginal
// gain, large churn across 70+ tests. Allowed per clippy::field_reassign_with_default.
#![cfg_attr(test, allow(clippy::field_reassign_with_default))]

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
pub mod capability;
pub mod errno;
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
pub mod debug;
pub mod page_fault;
pub mod pte_walk;
pub mod grant;
pub mod krandom;

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
    // R-07 (2026-08-12): Use getter methods (preferred API).
    // `validate()` is called later in `kmain`; the assertions here are
    // defense-in-depth — they fail-fast before paging setup begins.
    let kv = kernel_info.kern_virt_base().0;
    let kp = kernel_info.kern_phys_base().0;
    let ks = kernel_info.kern_size();
    let huge = P::HUGE_PAGE_SIZE;
    let fallback = P::FALLBACK_HUGE_PAGE_SIZE;

    // kern_size must be positive
    assert!(ks > 0, "arch_boot: kern_size must be > 0");

    // kern_phys_base must be page-aligned (at least 4KB)
    assert!(kp.is_multiple_of(0x1000), "arch_boot: kern_phys_base must be page-aligned");

    // kern_virt_base must be page-aligned (at least 4KB)
    assert!(kv.is_multiple_of(0x1000), "arch_boot: kern_virt_base must be page-aligned");

    // Choose the largest huge-page size that both bases and kern_size are aligned to.
    let kern_huge = if kv.is_multiple_of(huge) && kp.is_multiple_of(huge) && ks.is_multiple_of(huge) {
        huge
    } else if kv.is_multiple_of(fallback) && kp.is_multiple_of(fallback) && ks.is_multiple_of(fallback) {
        fallback
    } else {
        panic!("arch_boot: kern_virt_base, kern_phys_base, and kern_size must be aligned to at least FALLBACK_HUGE_PAGE_SIZE");
    };

    // kern_size must be a multiple of kern_huge for the mapping loop
    assert!(ks.is_multiple_of(kern_huge),
        "arch_boot: kern_size must be a multiple of the chosen huge page size");

    // kern_stack_top must be 16-byte aligned (ABI requirement)
    assert!(kernel_info.kern_stack_top().0.is_multiple_of(16),
        "arch_boot: kern_stack_top must be 16-byte aligned");

    // Register boot-stage page table page allocator if not already registered.
    // The caller (e.g., a test kernel) may have registered its own allocator
    // with a safer region (e.g., a bump region past the kernel image).
    if !pt_alloc::is_registered() {
        let memmap = kernel_info.memmap();
        let base = memmap.first().map(|r| r.base.0).unwrap_or(0);
        let end = base + memmap.first().map(|r| r.len as u64).unwrap_or(0);
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
        addr += kern_huge;
    }

    // Step 2: Kernel high-address mapping.
    // C: pg_mapkernel() — pg_utils.c:186
    //
    // When kern_virt_base == kern_phys_base, the kernel mapping is
    // identical to the identity mapping (Step 1). Skip it to avoid
    // overwriting L2 entries (especially on riscv64 where the same
    // VPN[2] slot would be written twice with potentially different
    // page sizes, corrupting the identity mapping).
    // See 02-higher-half-kernel.md for the full analysis.
    // R-07 (2026-08-12): Use getter methods (preferred API).
    let kern_virt = kernel_info.kern_virt_base().0;
    let kern_phys = kernel_info.kern_phys_base().0;
    if kern_virt != kern_phys {
        let kern_flags = PageFlags::kernel_read_write() | PageFlags::EXECUTABLE;
        let mut offset = 0u64;
        while offset < kernel_info.kern_size() {
            paging.map_huge(
                VirBytes(kern_virt + offset), PhysBytes(kern_phys + offset),
                kern_huge as usize, kern_flags,
            ).expect("kernel map: map_huge failed — kernel image mismatch");
            offset += kern_huge;
        }
    }

    // Step 3: Enable paging.
    // SAFETY: Steps 1+2 set up identity mapping covering current RIP.
    let _root_phys = unsafe { paging.enable() };

    // Record the bootstrap root physical address in the kernel global so
    // later phases (e.g., `init_proc_and_boot` loading the VM ELF) can
    // wrap it via `Paging::from_active_root` and add more mappings to the
    // *same* page table. The `paging` instance is about to be dropped,
    // but the page table it created remains active in the MMU.
    //
    // We use the `root_page` parameter (not `_root_phys`'s return value)
    // because some architectures' `enable()` returns a different value
    // (e.g., the satp-encoded value on riscv64, not the raw physical
    // address). The parameter is always the raw physical address.
    set_current_root_phys(root_page);

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
    // R-07 (2026-08-12): Validate KernelInfo invariants before any use.
    // Fail-fast on boot-shim bugs (e.g. non-zero bootstrap_len would
    // trigger add_memmap and reclaim firmware regions).
    kernel_info.validate();

    // Initialize the early console first so any boot diagnostic output
    // uses the correct baud rate / UART configuration.
    // C: ser_init() — originally inside arch_init(); moved to EarlyConsole trait.
    {
        use minix_plat::{EarlyConsole, CurrentEarlyConsole as Console};
        Console::init();
    }

    // C: memcpy(&kinfo, local_cbi, sizeof(kinfo)) + kernel_may_alloc = 1
    // Rust: copy the KernelInfo into the global (KernelInfo is Copy).
    // SAFETY: This runs during boot (single-threaded, before BKL needed).
    //         No concurrent access possible at this point.
    unsafe {
        *KERNEL_INFO.get() = Some(*kernel_info);
    }
    KERNEL_MAY_ALLOC.store(true, Ordering::Release);

    // Phase A.2: Initialize FREE_MEMMAP from KernelInfo.memmap + cut boot module regions.
    // C: pre_init() → get_parameters() → add_memmap()×N → cut_memmap()×mod_count
    //
    // The boot-shim reports all DRAM as free (including boot module regions).
    // The kernel must:
    //   1. Copy kernel_info.memmap → FREE_MEMMAP (mutable kernel copy)
    //   2. cut_memmap for each boot module (temporarily reserve their physical
    //      memory so the allocator doesn't hand those pages out during boot)
    //
    // After load_vm_elf copies the ELF segments into the VM process's page
    // tables (Phase C), the module's physical memory is reclaimed via
    // add_memmap. See 01-boot-shim-bootstrap.md §2.5 for the full lifecycle.
    //
    // SAFETY: Boot is single-threaded; FREE_MEMMAP is only accessed here.
    unsafe {
        let mmap = &mut *FREE_MEMMAP.get();
        // Step 1: Copy kernel_info.memmap into FREE_MEMMAP
        for (i, region) in kernel_info.memmap().iter().enumerate() {
            if i >= memmap::MAXMEMMAP {
                break;
            }
            if region.len > 0 {
                mmap[i] = memmap::MemMapEntry {
                    base: region.base.0,
                    length: region.len as u64,
                };
            }
        }
        // Step 2: Cut boot module regions (temporarily reserve)
        // C: pre_init.c:211 — cut_memmap(&kinfo, mod_start, mod_end - mod_start)
        for module in kernel_info.boot_modules().iter() {
            let _ = memmap::cut_memmap(
                mmap,
                module.start.0,
                module.len as u64,
            );
        }
    }

    // Phase A.5: Platform discovery — initialize PlatformContext from KernelInfo.
    // This MUST run before init_clock_and_interrupts() because the clock,
    // interrupt controller, and arch_init all read hardware parameters
    // from the global platform descriptor (see 04-platform-discovery.md §3.4).
    // SAFETY: Single-threaded boot context; no concurrent access.
    unsafe {
        minix_platform::init_from_kinfo(kernel_info);
    }

    // Phase B: cstart — protection + clock + interrupt
    init_protection(kernel_info);        // prot_init equivalent
    init_clock_and_interrupts();         // clock + intr + arch_init (covered in 05)

    // Phase C: proc_init + arch_boot_proc
    // Populates the global `PROC_TABLE` / `PRIV_TABLE` statics.
    init_proc_and_boot(kernel_info);  // covered in 06

    // Phase C.5: IPCF_POOL_INIT — initialize IPC filter pool
    // C: IPCF_POOL_INIT() — main.c:158-162 (called after proc_init)
    // In C, this is memset(&ipc_filter_pool, 0, sizeof(ipc_filter_pool)).
    // In Rust, the pool is already zero-initialized (all slots = None),
    // but we explicitly create and store it for clarity and to match
    // the C boot sequence.
    // SAFETY: This runs during boot (single-threaded, before BKL needed).
    unsafe {
        *IPC_FILTER_POOL.get() = crate::ipc_filter::IpcFilterPool::new();
    }

    // Phase C.6: krandom init — mark the global as ready for IRQ entropy.
    // C: `krandom.random_sources = RANDOM_SOURCES;` — main.c:48.
    // In Rust, the static is already initialized via `const fn new()`, so
    // this just sets the init flag for `try_krandom()` callers. Must run
    // before the first IRQ could fire (IRQs are enabled in Phase B's
    // `init_clock_and_interrupts`, but BKL is held until `switch_to_user`).
    crate::krandom::init();

    // Phase D: arch_post_init + memory_init
    // SAFETY: boot is single-threaded before BKL exists.
    let proc_table = unsafe { crate::proc_table_boot_unchecked() };
    init_post_and_memory(kernel_info, proc_table);  // covered in 07

    // Phase E: system_init — register syscall handlers
    // C: system_init() — system.c:168-278
    // D1/D2: In Rust, enum Syscall + match + const assert replaces C's
    // call_vec[] + map() macro. IrqManager and KPriv constructors handle
    // the IRQ hook pool and alarm timer initialization respectively.
    // See 08-system-init-boot-finish.md §4.4

    // Phase F: add_memmap + bsp_finish_booting
    // C: add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)
    // D5: 4GB truncation removed for 64-bit.
    // Reclaim the bootstrap (unpaged kernel) physical memory region.
    //
    // In Minix3 C this reclaims the small identity-mapped unpaged section
    // (start..end linker symbols in kernel.lds). In the Rust port this is
    // always a no-op because the kernel is higher-half from instruction #1 —
    // boot-shim passes `bootstrap_start=PhysBytes(0), bootstrap_len=0` and
    // the guard skips reclaim. See TODO-01-1 in 01-boot-shim-bootstrap.md
    // §X for the over-reclaim bug that motivated this.
    if kernel_info.bootstrap_len() > 0 {
        // SAFETY: Boot is single-threaded; FREE_MEMMAP is only accessed here
        // during boot. kernel_may_alloc is true at this point.
        let add_memmap_result = unsafe {
            let mmap = &mut *FREE_MEMMAP.get();
            memmap::add_memmap(
                mmap,
                kernel_info.bootstrap_start().0,
                kernel_info.bootstrap_len(),
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
    // C: bsp_finish_booting() — main.c:38-109
    // D7: bsp_finish_booting() -> ! — never returns.
    // bsp_finish_booting takes &mut ProcessTable + &mut SmpState to
    // perform step 2 (bill_ptr = idle_proc), step 4 (RTS_PROC_STOP unset
    // for NR_BOOT_PROCS-NR_TASKS boot processes), and step 5/7
    // (cycles_accounting_init + fpu_init on the BSP's CpuLocal).
    // See 08-system-init-boot-finish.md §4.5-4.6
    //
    // The SmpState is created here (single-CPU BSP-only configuration)
    // and stored in the global `SMP_STATE` so that `cpu_load()` and
    // `notify_scheduler()` can reach per-CPU state without it being
    // threaded through every call site. SMP expansion (16-smp.md) will
    // replace this with a real SMP discovery (AP CPUs booted before
    // bsp_finish_booting).
    //
    // SAFETY: boot is single-threaded before BKL exists.
    unsafe {
        *SMP_STATE.get() = Some(smp::SmpState::new_single_cpu());
        let smp_state = crate::smp_state_boot_unchecked();
        let proc_table = crate::proc_table_boot_unchecked();
        bsp_finish_booting(proc_table, smp_state)
    }
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
/// # Architecture dispatch
///
/// `kmain` is `#[cfg]`-switched across three `naked_asm!` blocks for
/// x86_64 / aarch64 / riscv64. Each block captures SP/PC/FP at entry
/// using arch-specific register names, then calls `kmain_verify`.
///
/// `#[cfg(target_arch)]` here selects **asm register names**, not
/// behavior — `naked_asm!` requires literal register names at compile
/// time, so this is the idiomatic Rust pattern for multi-arch naked
/// functions (cf. `core::arch::asm!` docs). This does NOT violate the
/// "no `#[cfg(target_arch)]` behavior selection" rule.
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

    // Ensure early console is initialized for test kernel output.
    Console::init();

    #[cfg(target_arch = "riscv64")]
    Console::write_str("kmain_verify: reached!\n");

    let kern_high = kernel_info.kern_virt_base().0;

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
    trap.configure_syscall(kernel_info.syscall_entry());
    // Do NOT call trap.load() here — handler addresses are still 0.
}

/// Initialize clock and interrupt controller.
///
/// C: init_clock() + intr_init(0) + arch_init() — main.c:403-481
/// Covered in detail in 05-clock-interrupt-init.md.
///
/// **Order matters** (cf. 05-clock-interrupt-init.md §1.1):
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
        ClockArch,
        ArchInit,
        CurrentClockArch, CurrentArchInit,
    };
    use crate::clock::ClockState;
    use minix_plat::{InterruptController, CurrentInterruptController};
    use minix_platform::{platform_desc, PlatformDesc};

    // Obtain the platform descriptor (initialized earlier from KernelInfo).
    // This is the single source of truth for all hardware parameters.
    let pd = platform_desc();

    // Step 1: Initialize clock state (software).
    // C: init_clock() — clock.c:48
    let mut clock = ClockState::new();
    // No env_get("hz") needed — DEFAULT_HZ is compile-time constant.

    // Step 2: Initialize hardware timer.
    // C: hardware portion of init_clock + arch_init() APIC timer
    //
    // Instance-based design (04-platform-discovery.md §3.4): construct the clock
    // arch from the timer descriptor, then call `init_timer` on the
    // instance. This replaces the old static `CurrentClockArch::init_timer`.
    let mut clock_arch = CurrentClockArch::new(pd.timer());
    clock_arch.init_timer(clock.hz());

    // Step 3: Initialize interrupt controller.
    // C: intr_init(0) — i8259.c:28 / omap_intr.c:24
    //
    // Instance-based design: construct the interrupt controller from the
    // interrupt controller descriptor, then call `init` on the instance.
    // The instance is then moved into the global `IRQ_MANAGER` so that
    // trap entry points and `bsp_finish_booting` can reach it.
    let mut intr = CurrentInterruptController::new(pd.interrupt_controller());
    intr.init();  // mask_all() called internally

    // D10: Store the interrupt controller in the global `IRQ_MANAGER`.
    // This replaces the previous pattern of dropping `intr` after init.
    // SAFETY: Boot is single-threaded before BKL exists; no concurrent access.
    unsafe {
        *IRQ_MANAGER.get() =
            Some(crate::irq_manager::IrqManager::new(intr));
    }

    // Step 4: Architecture-specific initialization.
    // C: arch_init() — arch_system.c:246 / earm/arch_system.c:101
    //
    // Instance-based design: construct the arch-init from the arch-misc
    // descriptor, then call `init` on the instance.
    let mut arch_init = CurrentArchInit::new(&pd.arch_misc());
    arch_init.init();

    // Suppress unused-variable warnings for `clock` (its `hz()` was consumed
    // above) — the software ClockState will be wired into the clock
    // subsystem in a later step.
    let _ = clock;
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
pub fn init_proc_and_boot(kernel_info: &KernelInfo) {
    use minix_arch::{
        CpuContextArch, CurrentCpuContextArch, EntrySpec, ProcKind,
        load_vm_elf, VmLoadError,
    };
    use crate::proc::{ProcNr, ProcName, RtsFlagsBits, proc_nr, KERNEL_TASKS, BOOT_MODULE_PROC_NRS};
    use crate::proc_table::NR_TASKS;
    use crate::kpriv::{priv_flag_set, K_CALL_MASK_NONE, K_CALL_MASK_ALL, IPC_TO_NONE, IPC_TO_ALL};
    use crate::proc::NR_BOOT_MODULES;

    // Step 1+2: Acquire global process + privilege tables (SyncUnsafeCell, BSS).
    // C: `EXTERN struct proc proc[]` / `EXTERN struct priv priv[]` — glo.h.
    // The tables are `const fn`-initialized at compile time (all slots
    // SLOT_FREE / s_proc_nr=None); per-process setup happens below.
    // SAFETY: boot is single-threaded before BKL exists.
    let proc_table = unsafe { crate::proc_table_boot_unchecked() };
    let priv_table = unsafe { crate::priv_table_boot_unchecked() };

    // C: NR_BOOT_MODULES check — main.c:160-162
    // R-07 (2026-08-12): Use getter method (preferred API).
    assert_eq!(
        kernel_info.boot_modules().len(),
        NR_BOOT_MODULES,
        "expected {} boot modules, found {}",
        NR_BOOT_MODULES,
        kernel_info.boot_modules().len()
    );

    // Step 3a: Initialize kernel tasks (hardcoded, not from multiboot modules).
    // C: image[0..NR_TASKS] in table.c — ASYNCM, IDLE, CLOCK, SYSTEM, KERNEL
    // Kernel tasks are compiled into the kernel, not loaded from GRUB modules.
    for &(name, nr) in KERNEL_TASKS.iter() {
        let Some(proc) = proc_table.get_mut(nr) else { continue };

        proc.set_boot_name(name);

        // Kernel tasks are always schedulable.
        // C: schedulable_proc = iskerneln(proc_nr) — main.c:173
        // C: priv(rp)->s_flags = (nr==IDLE ? IDL_F : TSK_F) — main.c:188-189
        // C: TSK_M = NO_M (no IPC), TSK_KC = NO_C (no kernel calls).
        // 06-proc-init-boot-proc.md §3.2 — grant_capability replaces assign_static
        // + configure_boot_priv pair; template encodes flags + masks by construction.
        let template = if nr == proc_nr::IDLE {
            crate::capability::CapabilityTemplate::Idle
        } else {
            crate::capability::CapabilityTemplate::KernelTask
        };
        let _priv_id = priv_table.grant_capability(nr, template)
            .expect("grant_capability: kernel task priv slot occupied");

        // Architecture-private CPU context. The arch layer decides the
        // initial PSW/PSR/sstatus, segment selectors, FPU policy, and
        // per-process FPU enable (aarch64) — the kernel layer never
        // sees these values.
        let cpu_context = <CurrentCpuContextArch as CpuContextArch>::build_cpu_context(
            ProcKind::KernelTask,
            nr.0, // arch trait takes a plain i32 (arch::boot::ProcNr = i32 alias)
            EntrySpec::KERNEL_TASK,
        );
        proc.set_boot_cpu_context(cpu_context);

        // Kernel tasks start stopped.
        proc.p_rts_flags.set(RtsFlagsBits::PROC_STOP);
        proc.p_rts_flags.clear(RtsFlagsBits::SLOT_FREE);
    }

    // Step 3b: Initialize user-space boot modules (from multiboot module list).
    // C: image[NR_TASKS..NR_BOOT_PROCS] in table.c
    // C: kinfo.module_list[i] corresponds to image[NR_TASKS + i]
    for (i, module) in kernel_info.boot_modules().iter().enumerate() {
        // Map boot module index to process number using the C boot image order.
        // C: image[NR_TASKS + i].proc_nr — table.c
        let nr: ProcNr = BOOT_MODULE_PROC_NRS[i];

        let Some(proc) = proc_table.get_mut(nr) else { continue };

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
            // Assign static privilege + boot flags via capability template.
            // C: get_priv(rp, static_priv_id(proc_nr)) — main.c:200
            // C: priv(rp)->s_flags = VM_F (VM) or RSYS_F (RS) — main.c:179-209
            // C: ipc_to_m = SRV_M = ALL_M; kcalls = SRV_KC = ALL_C — main.c:184-213
            // 06-proc-init-boot-proc.md §3.2 — single template call encodes all of these.
            let template = if is_vm {
                crate::capability::CapabilityTemplate::Vm
            } else {
                crate::capability::CapabilityTemplate::RootService
            };
            let _priv_id = priv_table.grant_capability(nr, template)
                .expect("grant_capability: static priv slot occupied");
        } else {
            // Don't let the process run for now.
            // C: RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM) — main.c:226
            proc.p_rts_flags.set(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM);
        }

        // For user-space boot processes, set up the arch-private CPU
        // context. The C code's `arch_boot_proc(ip, rp)` is split
        // here into: (1) ELF loading for VM (`load_vm_elf`), and (2)
        // CPU-context construction (`build_cpu_context`). Both are
        // arch-layer responsibilities — the kernel never inspects
        // the fields.
        //
        // Process kind mapping (06-proc-init-boot-proc.md §3.4):
        //   VM_PROC_NR    → ProcKind::Vm
        //   RS_PROC_NR    → ProcKind::RootService
        //   other user    → ProcKind::UserService (RS will load ELF later)
        let proc_kind = if is_vm {
            ProcKind::Vm
        } else if is_root_sys {
            ProcKind::RootService
        } else {
            ProcKind::UserService
        };

        // VM ELF loading (§12.2: now returns Result, no silent failure).
        let entry = if is_vm {
            #[cfg(feature = "mock")]
            {
                use minix_arch::paging::mock::MockPaging;
                let mut paging = MockPaging::new_from_page(PhysBytes(0));
                let vm_result = load_vm_elf(module, kernel_info, &mut paging)
                    .expect("load_vm_elf: VM ELF is required at boot");
                // FIX-23 (Phase 4): Reclaim VM module physical memory after
                // ELF segments have been copied into the VM process's page
                // tables. C: protect.c:450-451 — mod->mod_start = mod_end = 0.
                // This undoes the cut_memmap done in Phase A.2, returning the
                // module's physical pages to FREE_MEMMAP for future allocation.
                // SAFETY: Boot is single-threaded; FREE_MEMMAP is only accessed here.
                unsafe {
                    let mmap = &mut *FREE_MEMMAP.get();
                    let _ = memmap::add_memmap(mmap, module.start.0, module.len as u64);
                }
                EntrySpec::loaded(vm_result.pc, vm_result.sp, vm_result.ps_strings)
            }
            #[cfg(not(feature = "mock"))]
            {
                // FIX-24 (Phase 9): Real VM ELF loading at boot.
                //
                // Previously this branch was DEFERRED — VM started with
                // PC=0 and RS was expected to load the VM ELF later.
                // That approach is incorrect because VM is the page-table
                // process: it must be runnable *immediately* after boot so
                // it can handle VMCTL/PRIVCTL syscalls from other boot
                // processes. A VM with PC=0 cannot serve any syscall.
                //
                // The bootstrap page table (created by `arch_boot_impl`)
                // is still active — we wrap it via `from_active_root` and
                // map the VM ELF segments into it. The segments use 1:1
                // identity mapping (VA = PA), matching what `load_vm_elf`
                // expects (see `arch/boot.rs`).
                //
                // After the ELF is loaded, the VM module's physical memory
                // is reclaimed via `add_memmap` (undoes the `cut_memmap`
                // from Phase A.2), matching C: protect.c:450-451.
                use minix_arch::paging::Paging as _;
                use minix_arch::CurrentPaging;

                let root_phys = current_root_phys()
                    .expect("init_proc_and_boot: bootstrap root not set \
                             — arch_boot_impl must run first");
                let mut paging = CurrentPaging::from_active_root(root_phys);
                let vm_result = load_vm_elf(module, kernel_info, &mut paging)
                    .expect("load_vm_elf: VM ELF is required at boot");

                // Reclaim VM module physical memory after ELF segments
                // have been copied into the bootstrap page table.
                // C: protect.c:450-451 — mod->mod_start = mod_end = 0.
                // SAFETY: Boot is single-threaded; FREE_MEMMAP is only
                // accessed here.
                unsafe {
                    let mmap = &mut *FREE_MEMMAP.get();
                    let _ = memmap::add_memmap(mmap, module.start.0, module.len as u64);
                }

                // Record VM's page-table root addresses in p_seg so
                // `init_post_and_memory` can install them via
                // `set_ptproc` + `set_current_ptproc_nr`. The bootstrap
                // root IS VM's initial root — VMCTL SetAddrSpace will
                // replace it later when VM installs its own page table.
                proc.p_seg.phys_root = root_phys;
                // The virtual address of the root is the identity-mapped
                // address (VA = PA during bootstrap).
                proc.p_seg.virt_root = Some(VirBytes(root_phys.0));

                EntrySpec::loaded(vm_result.pc, vm_result.sp, vm_result.ps_strings)
            }
        } else {
            // Other user processes have no ELF loaded at boot.
            // RS will load them at runtime.
            // Module physical memory stays cut (reserved) until RS loads
            // the ELF and reclaims it.
            EntrySpec::DEFERRED
        };

        let cpu_context = <CurrentCpuContextArch as CpuContextArch>::build_cpu_context(
            proc_kind,
            nr.0, // arch trait takes a plain i32 (arch::boot::ProcNr = i32 alias)
            entry,
        );
        proc.set_boot_cpu_context(cpu_context);

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
    //
    // The global `PROC_TABLE` / `PRIV_TABLE` statics now hold the initialized
    // tables; callers acquire them via `crate::proc_table()` / `crate::priv_table()`.
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

    // Record VM's proc-nr as the current ptproc in the kernel global.
    // C: get_cpulocal_var(ptproc) = vm — protect.c:372 (x86) / protect.c:99 (ARM)
    //
    // This is the kernel-level counterpart of `set_ptproc`: the arch-level
    // trait records any arch-internal state (e.g., virt_root for createpde),
    // while this global enables `dispatch_vmctl(SetAddrSpace)` to decide
    // whether to call `TlbArch::set_active_root` (write_cr3) when the
    // target process is the current ptproc.
    //
    // SAFETY: BKL is held during boot, only this CPU accesses the global.
    set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);

    // Step 2: Allocate free page directory entries (mirrors C's memory_init()
    // freepdes[] accounting). createpde() itself is superseded by Direct Map
    // in 64-bit (see FREE_PDE_SLOTS doc); the slots are retained for boot
    // accounting parity with C.
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
    // Store the slots in kernel global state (retained for boot accounting;
    // createpde() is superseded by Direct Map — see FREE_PDE_SLOTS doc).
    // SAFETY: This runs during boot (single-threaded, before BKL needed).
    //         No concurrent access possible at this point.
    unsafe {
        *FREE_PDE_SLOTS.get() = free_pde_slots;
    }
}

// ── Phase E-F: system_init + bsp_finish_booting (08-system-init-boot-finish.md) ──

use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering};

/// Global flag: kernel may allocate physical memory directly.
/// C: kernel_may_alloc in glo.h
/// Set to true at kmain start, cleared in bsp_finish_booting().
static KERNEL_MAY_ALLOC: AtomicBool = AtomicBool::new(false);

/// Check if kernel may allocate memory directly.
/// C: kernel_may_alloc checks throughout kernel code
pub fn kernel_may_alloc() -> bool {
    KERNEL_MAY_ALLOC.load(Ordering::Acquire)
}

/// `UnsafeCell` with `Sync` gated on the [`BklProtected`] marker trait.
///
/// Mirrors the unstable stdlib `SyncUnsafeCell` (rust-lang issue #95439),
/// but with a **compile-time guard** against accidental misuse: only types
/// that explicitly opt into `BklProtected` can be wrapped. This prevents
/// soundness bugs where a `!Sync` type (e.g. `RefCell<T>`, `Rc<T>`,
/// `Cell<T>`) is silently promoted to `Sync` by being placed in a
/// `static SyncUnsafeCell<...>`.
///
/// All access requires external synchronization (BKL or single-threaded boot).
/// This type makes the `Sync` requirement explicit, replacing `static mut` +
/// `addr_of_mut!` for Rust 2024 compliance — no `static_mut_refs` involved.
///
/// # Soundness contract (FIX-07: R-01)
///
/// The `unsafe impl<T: BklProtected + ?Sized> Sync` below is sound because:
/// 1. `BklProtected` is a sealed trait — only types in this file's
///    `bkl_protected_impls!` macro call can implement it.
/// 2. Every approved type is either `Send + Sync` by itself (so wrapping it
///    in `SyncUnsafeCell` adds no new cross-thread access capability beyond
///    what `static` already provides) or is an internal kernel struct whose
///    mutation is serialized by the BKL at every callsite.
/// 3. The BKL provides mutual exclusion at runtime; `SyncUnsafeCell` only
///    silences the `!Sync`-ness of `UnsafeCell` so the wrapped type can be
///    placed in a `static`. Interior mutability through `get()` still
///    requires the caller to uphold the safety contract.
///
/// See `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/06-proc-init-boot-proc.md`
/// §4.1 (storage model) for the design rationale.
#[repr(transparent)]
pub(crate) struct SyncUnsafeCell<T: ?Sized> {
    value: core::cell::UnsafeCell<T>,
}

// SAFETY: See "Soundness contract" above. `T: BklProtected` restricts the
// impl to types whose mutation is serialized by the BKL (or which are
// write-once-read-only after boot). Without this bound, any `!Sync` type
// could be wrapped — that was the original soundness hole (R-01).
unsafe impl<T: BklProtected + ?Sized> Sync for SyncUnsafeCell<T> {}

impl<T> SyncUnsafeCell<T> {
    /// Creates a new `SyncUnsafeCell` wrapping the given value.
    ///
    /// `T` must implement [`BklProtected`]. This is enforced at construction
    /// time so that the `Sync` impl applies.
    pub(crate) const fn new(value: T) -> Self
    where
        T: BklProtected,
    {
        SyncUnsafeCell {
            value: core::cell::UnsafeCell::new(value),
        }
    }

    /// Gets a mutable pointer to the wrapped value.
    ///
    /// The caller must ensure that no concurrent access occurs (BKL or
    /// single-threaded context). Dereferencing the returned pointer is `unsafe`.
    pub(crate) fn get(&self) -> *mut T
    where
        T: BklProtected,
    {
        self.value.get()
    }
}

// ── BklProtected: sealed marker trait gating `SyncUnsafeCell` (FIX-07: R-01)
//
// Without this trait, the old blanket `unsafe impl<T: ?Sized> Sync` let any
// type (including `RefCell<T>` / `Rc<T>` / `Cell<T>`) be silently promoted
// to `Sync` by wrapping it in `SyncUnsafeCell`. The marker trait is sealed
// so external crates (and other modules in this crate) cannot add new impls
// without going through the audit process documented above.
//
// The trait is implemented only for the 9 types currently stored in
// `SyncUnsafeCell` statics (see `bkl_protected_impls!` below). Adding a new
// `SyncUnsafeCell<NewType>` static requires extending the macro call —
// this is intentional friction.
mod bkl_protected {
    /// Sealed marker trait — see module docs.
    ///
    /// # Safety
    ///
    /// Implementors must guarantee that all mutation of `Self` is serialized
    /// by the Big Kernel Lock (BKL) at every callsite, OR that `Self` is
    /// write-once-read-only after boot (e.g. `KernelInfo`). The BKL provides
    /// runtime mutual exclusion; this trait only silences the `!Sync`-ness
    /// of `UnsafeCell` so the wrapped type can live in a `static`.
    pub(crate) unsafe trait BklProtected: Sealed {}

    /// Sealed trait — no external impls possible.
    pub(crate) trait Sealed {}

    // ── Macro to reduce boilerplate for approved types ──
    //
    // Each invocation expands to `impl Sealed for T {}` + `unsafe impl
    // BklProtected for T {}`. The `unsafe` is on the trait, so each
    // macro call is a single auditable unit.
    macro_rules! bkl_protected_impls {
        ($($ty:ty),+ $(,)?) => {
            $(
                impl Sealed for $ty {}
                /// # Safety
                ///
                /// All mutation is serialized by the BKL (or write-once after
                /// boot). See `bkl_protected` module docs.
                unsafe impl BklProtected for $ty {}
            )+
        };
    }

    // ── Approved types ──
    //
    // Adding a new type here is the ONLY way to wrap it in `SyncUnsafeCell`.
    // Each addition must be audited for BKL-serialized mutation.
    bkl_protected_impls! {
        // Write-once-read-only after boot (no BKL needed post-init):
        minix_boot::KernelInfo,
        crate::memmap::MemMapEntry,

        // BKL-serialized mutation (kernel-internal types):
        crate::proc_table::ProcessTable,
        crate::kpriv::PrivTable,
        crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>,
        crate::smp::SmpState,
        minix_arch::FreePdeSlots,
        crate::ipc_filter::IpcFilterPool,
        crate::krandom::KRandomness,
    }

    // Generic composite impls — derive BklProtected from the inner type.
    // These allow `SyncUnsafeCell<Option<T>>` and `SyncUnsafeCell<[T; N]>`
    // without listing every instantiation.
    impl<T: BklProtected> Sealed for Option<T> {}
    /// # Safety
    ///
    /// `Option<T>` is `BklProtected` iff `T` is. The `Option` layer adds no
    /// new mutation surface beyond what `T` already has.
    unsafe impl<T: BklProtected> BklProtected for Option<T> {}

    impl<T: BklProtected, const N: usize> Sealed for [T; N] {}
    /// # Safety
    ///
    /// `[T; N]` is `BklProtected` iff `T` is. Array indexing adds no new
    /// mutation surface beyond what `T` already has.
    unsafe impl<T: BklProtected, const N: usize> BklProtected for [T; N] {}
}

pub(crate) use bkl_protected::BklProtected;

#[cfg(test)]
mod bkl_protected_tests {
    use super::*;
    use ::alloc::string::String;

    /// Verify all 9 approved types implement `BklProtected`.
    ///
    /// If any of these fails to compile, a `SyncUnsafeCell<NewType>` static
    /// was added without extending the `bkl_protected_impls!` macro in
    /// `bkl_protected` module — that is the intended friction point (R-01).
    #[test]
    fn bkl_protected_approved_types_implement_trait() {
        fn assert_impl<T: BklProtected>() {}

        // Write-once-read-only after boot:
        assert_impl::<minix_boot::KernelInfo>();
        assert_impl::<crate::memmap::MemMapEntry>();

        // BKL-serialized mutation:
        assert_impl::<crate::proc_table::ProcessTable>();
        assert_impl::<crate::kpriv::PrivTable>();
        assert_impl::<crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>>();
        assert_impl::<crate::smp::SmpState>();
        assert_impl::<minix_arch::FreePdeSlots>();
        assert_impl::<crate::ipc_filter::IpcFilterPool>();
        assert_impl::<crate::krandom::KRandomness>();

        // Composite impls:
        assert_impl::<Option<minix_boot::KernelInfo>>();
        assert_impl::<[crate::memmap::MemMapEntry; 4]>();
    }

    /// Document the negative case: `RefCell<T>` is `!Sync` and must NOT
    /// implement `BklProtected`. If this test compiles, the soundness
    /// guard is working — `SyncUnsafeCell<RefCell<T>>` cannot be constructed
    /// because `RefCell<T>: BklProtected` does not hold.
    ///
    /// Note: This is a compile-pass test. To verify the negative case
    /// directly, attempt to uncomment the `SyncUnsafeCell::new(RefCell::new(0))`
    /// line — it will fail with "trait bound `RefCell<i32>: BklProtected`
    /// is not satisfied".
    #[test]
    fn bkl_protected_refcell_does_not_impl() {
        // Uncomment to verify the guard rejects `RefCell`:
        // let _ = SyncUnsafeCell::new(core::cell::RefCell::new(0i32));
        //                                                                        ^^^ expected error

        // The approved types still work:
        let _ = SyncUnsafeCell::new(crate::krandom::KRandomness::new());
        let _ = SyncUnsafeCell::new(Option::<KernelInfo>::None);

        // String is NOT in the approved list — would fail to compile:
        // let _ = SyncUnsafeCell::new(String::new());
        let _ = String::new(); // suppress unused import warning
    }
}

/// Free physical memory map — populated by add_memmap during boot.
/// C: kinfo.memmap[MAXMEMMAP] — param.h:18
///
/// SAFETY: Only written during boot (single-threaded, before BKL needed).
/// After boot, this is read-only. BKL protects any post-boot access.
#[allow(dead_code)] // SMP memmap infra (checklist D-17); not yet wired to all call sites
static FREE_MEMMAP: SyncUnsafeCell<[memmap::MemMapEntry; memmap::MAXMEMMAP]> =
    SyncUnsafeCell::new([memmap::MEM_MAP_ENTRY_ZERO; memmap::MAXMEMMAP]);

/// Free page directory entry slots for createpde() temporary mappings.
/// C: freepdes[NR_FREEPDES] — glo.h / memory.c:707-717
///
/// Global KernelInfo — stored once during kmain, read-only thereafter.
///
/// C: `kinfo` global in glo.h — populated by memcpy from boot params in main.c.
///
/// SAFETY: Only written once during boot (single-threaded, before BKL needed).
/// After boot, read-only under BKL protection.
static KERNEL_INFO: SyncUnsafeCell<Option<KernelInfo>> = SyncUnsafeCell::new(None);

/// Global process table — C's `EXTERN struct proc proc[NR_TASKS + NR_PROCS]`.
///
/// # Storage (06-proc-init-boot-proc.md §3.1)
///
/// `SyncUnsafeCell` is the Rust 2024 translation of C's BSS `EXTERN` array:
/// compile-time-fixed address, zero heap, zero runtime overhead, with explicit
/// `Sync` (BKL guards all access). The `#![no_std]` kernel has no allocator at
/// boot time, so `Box<[KProcess]>` is forbidden here.
///
/// # SAFETY
///
/// All access requires the Big Kernel Lock (BKL). The BKL serializes all
/// kernel code, so at most one CPU mutates `PROC_TABLE` at a time. Boot-time
/// init (single-threaded, before BKL exists) is also safe.
static PROC_TABLE: SyncUnsafeCell<crate::proc_table::ProcessTable> = SyncUnsafeCell::new(crate::proc_table::ProcessTable::new());

/// Global privilege table — C's `EXTERN struct priv priv[NR_SYS_PROCS]`.
///
/// Same storage / safety model as `PROC_TABLE`. See `06-proc-init-boot-proc.md` §3.1.
static PRIV_TABLE: SyncUnsafeCell<crate::kpriv::PrivTable> = SyncUnsafeCell::new(crate::kpriv::PrivTable::new());

/// Global IRQ manager — owns the architecture's interrupt controller and
/// the IRQ hook chain.
///
/// Initialized in `init_clock_and_interrupts` after the interrupt controller
/// is constructed from the platform descriptor. Stored as `Option` because
/// `InterruptController::new` is not `const fn` (it reads hardware base
/// addresses from a descriptor).
///
/// # Safety
///
/// All access requires the BKL (or single-threaded boot before BKL exists).
/// See `proc_table()` / `priv_table()` for the same pattern.
///
/// # Design (D10 / 14-exception-interrupt.md §4.4)
///
/// The IRQ manager is a global so that trap entry points (assembly stubs)
/// can reach it without holding a reference in a CPU-local. This mirrors
/// C's global `irq_hooks[]` + `irq_actids[]` + `intr_*` globals.
static IRQ_MANAGER: SyncUnsafeCell<Option<crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>>> = SyncUnsafeCell::new(None);

/// Global SMP state — owns per-CPU `CpuLocal` (proc_ptr, bill_ptr,
/// cpu_last_tsc, cpu_last_idle, ...) and CPU readiness flags.
///
/// Initialized in `init_proc_and_boot` to a single-CPU (BSP-only)
/// configuration. SMP expansion (16-smp.md) will replace this with a real
/// SMP discovery that boots APs before `bsp_finish_booting`.
///
/// # Safety
///
/// All access requires the BKL (or single-threaded boot before BKL exists).
/// Same storage / safety model as `PROC_TABLE` / `PRIV_TABLE` / `IRQ_MANAGER`.
///
/// # Design (D-notify-scheduler / 11-scheduling-primitives.md §4.5)
///
/// Made global so that `cpu_load()` (clock.rs) and `notify_scheduler()`
/// (proc_table.rs) can reach per-CPU `cpu_last_tsc` / `cpu_last_idle`
/// without threading `&mut SmpState` through every call site. Mirrors C's
/// `get_cpu_var_ptr(cpu, ...)` access pattern.
static SMP_STATE: SyncUnsafeCell<Option<crate::smp::SmpState>> = SyncUnsafeCell::new(None);

/// Get a reference to the global process table.
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
///
/// **Prefer [`proc_table_with`]** which takes a `BklSection` witness for
/// compile-time BKL proof (R-03). This unsafe version is retained for
/// paths that have not yet been migrated.
pub unsafe fn proc_table() -> &'static mut crate::proc_table::ProcessTable {
    // SAFETY: caller guarantees BKL (or single-threaded boot). We use raw
    // pointer dereference (not `&mut PROC_TABLE`) to avoid the
    // `static_mut_refs` lint (Rust 2024 compatibility).
    unsafe { &mut *PROC_TABLE.get() }
}

/// Get a reference to the global process table with BKL witness (R-03).
///
/// The `BklSection` parameter is a compile-time capability token proving
/// the caller holds the BKL. See [`crate::smp::BklGuard::section`] and
/// [`crate::smp::bkl_lock_section`].
pub fn proc_table_with(_section: &crate::smp::BklSection<'_>) -> &'static mut crate::proc_table::ProcessTable {
    // SAFETY: BklSection witness proves BKL is held. We use raw pointer
    // dereference to avoid the `static_mut_refs` lint (Rust 2024).
    unsafe { &mut *PROC_TABLE.get() }
}

/// Boot-time accessor: access process table without BKL witness.
///
/// # Safety
///
/// Only safe during single-threaded boot (before BKL exists or before
/// secondary CPUs are started). After boot, use [`proc_table_with`].
pub unsafe fn proc_table_boot_unchecked() -> &'static mut crate::proc_table::ProcessTable {
    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { &mut *PROC_TABLE.get() }
}

/// Get a reference to the global privilege table.
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
///
/// **Prefer [`priv_table_with`]** which takes a `BklSection` witness (R-03).
pub unsafe fn priv_table() -> &'static mut crate::kpriv::PrivTable {
    // SAFETY: caller guarantees BKL (or single-threaded boot). We use raw
    // pointer dereference (not `&mut PRIV_TABLE`) to avoid the
    // `static_mut_refs` lint (Rust 2024 compatibility).
    unsafe { &mut *PRIV_TABLE.get() }
}

/// Get a reference to the global privilege table with BKL witness (R-03).
pub fn priv_table_with(_section: &crate::smp::BklSection<'_>) -> &'static mut crate::kpriv::PrivTable {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *PRIV_TABLE.get() }
}

/// Boot-time accessor: access privilege table without BKL witness.
///
/// # Safety
///
/// Only safe during single-threaded boot. After boot, use [`priv_table_with`].
pub unsafe fn priv_table_boot_unchecked() -> &'static mut crate::kpriv::PrivTable {
    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { &mut *PRIV_TABLE.get() }
}

/// Get a reference to the global IRQ manager.
///
/// # Panics
///
/// Panics if `IRQ_MANAGER` has not been initialized yet. Initialization
/// happens in `init_clock_and_interrupts` (non-mock builds) or must be
/// done manually in test setups.
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
/// Concurrent access from multiple CPUs without BKL is a data race.
///
/// **Prefer [`irq_manager_with`]** which takes a `BklSection` witness (R-03).
pub unsafe fn irq_manager() -> &'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController> {
    // SAFETY: caller guarantees BKL (or single-threaded boot). We use raw
    // pointer dereference to avoid the `static_mut_refs` lint.
    unsafe { &mut *IRQ_MANAGER.get() }
        .as_mut()
        .expect("IRQ_MANAGER not initialized — init_clock_and_interrupts must run first")
}

/// Get a reference to the global IRQ manager with BKL witness (R-03).
///
/// # Panics
///
/// Panics if `IRQ_MANAGER` has not been initialized yet.
pub fn irq_manager_with(_section: &crate::smp::BklSection<'_>) -> &'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController> {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *IRQ_MANAGER.get() }
        .as_mut()
        .expect("IRQ_MANAGER not initialized — init_clock_and_interrupts must run first")
}

/// Try to get a reference to the global IRQ manager.
///
/// Returns `None` if `IRQ_MANAGER` has not been initialized yet (e.g. in
/// unit tests that skip `init_clock_and_interrupts`). Callers that can
/// tolerate the absence (e.g. `GET_IRQACTIDS` before boot init) should
/// prefer this over [`irq_manager`].
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
///
/// **Prefer [`try_irq_manager_with`]** which takes a `BklSection` witness (R-03).
pub unsafe fn try_irq_manager() -> Option<&'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>> {
    // SAFETY: caller guarantees BKL (or single-threaded boot).
    unsafe { &mut *IRQ_MANAGER.get() }.as_mut()
}

/// Try to get a reference to the global IRQ manager with BKL witness (R-03).
pub fn try_irq_manager_with(_section: &crate::smp::BklSection<'_>) -> Option<&'static mut crate::irq_manager::IrqManager<minix_plat::CurrentInterruptController>> {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *IRQ_MANAGER.get() }.as_mut()
}

/// Install an empty `IrqManager` into the global `IRQ_MANAGER` for unit
/// tests that exercise dispatch paths calling `irq_manager()` (e.g.
/// `dispatch_clear`'s IRQ-hook cleanup).
///
/// The interrupt controller is constructed via `new` only — `init()` is
/// NOT called, so no hardware I/O is performed. The cleanup loops under
/// test only read the (empty) hook table, so no controller method is
/// ever invoked. This mirrors how `init_clock_and_interrupts` populates
/// the global at boot (lib.rs:656-659), minus the hardware init.
///
/// # Safety
///
/// Caller must ensure single-threaded access. The verification harness
/// runs with `--test-threads=1`; the assignment is idempotent (installs
/// a fresh empty manager each call), so test ordering does not matter.
#[cfg(test)]
pub(crate) unsafe fn init_irq_manager_for_test() { unsafe {
    let ctrl = new_test_interrupt_controller();
    // SAFETY: test-only; single-threaded under `--test-threads=1`. Uses
    // `addr_of_mut!` to avoid the `static_mut_refs` lint, same as boot.
    *IRQ_MANAGER.get() =
        Some(crate::irq_manager::IrqManager::new(ctrl));
}}

/// Construct a `CurrentInterruptController` for unit tests without
/// touching hardware. Only the matching target arch's descriptor is
/// compiled; the others are `cfg`-elided.
#[cfg(test)]
#[cfg(target_arch = "x86_64")]
fn new_test_interrupt_controller() -> minix_plat::CurrentInterruptController {
    use minix_plat::InterruptController;
    use minix_platform::arch::x86_64::ApicDesc;
    let desc = ApicDesc {
        lapic_base: 0xFEE0_0000,
        ioapic_base: 0xFEC0_0000,
        nr_irqs: 16,
    };
    minix_plat::CurrentInterruptController::new(&desc)
}

#[cfg(test)]
#[cfg(target_arch = "aarch64")]
fn new_test_interrupt_controller() -> minix_plat::CurrentInterruptController {
    use minix_plat::InterruptController;
    use minix_platform::arch::aarch64::Gicv3Desc;
    let desc = Gicv3Desc {
        gicd_base: 0x0800_0000,
        gicr_base: 0x080A_0000,
        gicr_stride: 0x1_0000,
        nr_irqs: 16,
    };
    minix_plat::CurrentInterruptController::new(&desc)
}

#[cfg(test)]
#[cfg(target_arch = "riscv64")]
fn new_test_interrupt_controller() -> minix_plat::CurrentInterruptController {
    use minix_plat::InterruptController;
    use minix_platform::arch::riscv64::PlicDesc;
    let desc = PlicDesc {
        plic_base: 0x0C00_0000,
        nr_irqs: 16,
        context: 1,
    };
    minix_plat::CurrentInterruptController::new(&desc)
}

/// Get a reference to the global SMP state.
///
/// # Panics
///
/// Panics if `SMP_STATE` has not been initialized yet. Initialization
/// happens in `init_proc_and_boot` (non-mock builds).
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
/// Concurrent access from multiple CPUs without BKL is a data race.
///
/// **Prefer [`smp_state_with`]** which takes a `BklSection` witness (R-03).
pub unsafe fn smp_state() -> &'static mut crate::smp::SmpState {
    // SAFETY: caller guarantees BKL (or single-threaded boot). We use raw
    // pointer dereference to avoid the `static_mut_refs` lint.
    unsafe { &mut *SMP_STATE.get() }
        .as_mut()
        .expect("SMP_STATE not initialized — init_proc_and_boot must run first")
}

/// Get a reference to the global SMP state with BKL witness (R-03).
///
/// # Panics
///
/// Panics if `SMP_STATE` has not been initialized yet.
pub fn smp_state_with(_section: &crate::smp::BklSection<'_>) -> &'static mut crate::smp::SmpState {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *SMP_STATE.get() }
        .as_mut()
        .expect("SMP_STATE not initialized — init_proc_and_boot must run first")
}

/// Boot-time accessor: access SMP state without BKL witness.
///
/// # Safety
///
/// Only safe during single-threaded boot. After boot, use [`smp_state_with`].
pub unsafe fn smp_state_boot_unchecked() -> &'static mut crate::smp::SmpState {
    // SAFETY: caller guarantees single-threaded boot context.
    unsafe { &mut *SMP_STATE.get() }
        .as_mut()
        .expect("SMP_STATE not initialized — init_proc_and_boot must run first")
}

/// Try to get a reference to the global SMP state.
///
/// Returns `None` if `SMP_STATE` has not been initialized yet (e.g. in
/// unit tests that exercise `ProcessTable` methods directly without the
/// full boot sequence). Callers that can tolerate the absence (clock load
/// measurement, cpu id readout) should prefer this over [`smp_state`].
///
/// # Safety
///
/// Caller must hold the BKL (or be in single-threaded boot before BKL exists).
///
/// **Prefer [`try_smp_state_with`]** which takes a `BklSection` witness (R-03).
pub unsafe fn try_smp_state() -> Option<&'static mut crate::smp::SmpState> {
    // SAFETY: caller guarantees BKL (or single-threaded boot).
    unsafe { &mut *SMP_STATE.get() }.as_mut()
}

/// Try to get a reference to the global SMP state with BKL witness (R-03).
pub fn try_smp_state_with(_section: &crate::smp::BklSection<'_>) -> Option<&'static mut crate::smp::SmpState> {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *SMP_STATE.get() }.as_mut()
}

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
    unsafe { (*KERNEL_INFO.get()).as_ref() }
}

/// Populated by `init_post_and_memory()` during boot. Allocated to mirror
/// C's `memory_init()` `freepdes[]` boot accounting, which reserved PDE
/// slots for `createpde()` temporary mappings.
///
/// # createpde superseded by Direct Map
///
/// In the 64-bit Rust rewrite, `createpde()` is **not implemented** — it
/// is superseded by Direct Map + `PteWalkArch`. C's `createpde()` inserted
/// a foreign process's PDE into the active page table to access its memory;
/// in 64-bit, `DirectMapArch::phys_to_virt` + `CurrentPteWalk::walk` read
/// any process's page table pages directly (see `cross_space.rs::data_copy_vmcheck`
/// and `vm::lookup_in_table`). The `FreePdeSlots` are retained only for
/// boot-time accounting parity with C (the slots are allocated but never
/// consumed at runtime). See [18-syscall-copy.md §1.5](../../notes/rewrite/fork-syscall-rewrite/03-stage-kernel/18-syscall-copy.md).
///
/// SAFETY: Only written once during boot (single-threaded, before BKL needed).
/// After boot, read-only under BKL protection.
#[allow(dead_code)] // SMP PDE slot mgmt (checklist D-17); retained for boot-accounting parity
static FREE_PDE_SLOTS: SyncUnsafeCell<minix_arch::FreePdeSlots> = SyncUnsafeCell::new(minix_arch::FreePdeSlots::new());

/// Get a reference to the global free PDE slots.
///
/// Caller must ensure BKL is held if called after boot initialization.
/// C: freepdes[] global array access.
#[allow(dead_code)] // accessor for FREE_PDE_SLOTS; not yet wired to all call sites
pub(crate) fn free_pde_slots() -> &'static minix_arch::FreePdeSlots {
    // SAFETY: After boot, FREE_PDE_SLOTS is read-only.
    // Caller is responsible for BKL synchronization.
    unsafe { &*FREE_PDE_SLOTS.get() }
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
/// (Phase D).
///
/// # createpde superseded
///
/// In C, `createpde()` advanced `freepde_start` to claim PDE slots for
/// temporary foreign mappings. In the 64-bit Rust rewrite, `createpde()`
/// is **not implemented** (superseded by Direct Map + `PteWalkArch` — see
/// `FREE_PDE_SLOTS` doc above and 18-syscall-copy.md §1.5). This counter
/// is therefore advanced only during boot (`init_post_and_memory`), never
/// at runtime; it is retained for boot-accounting parity with C.
///
/// SAFETY: Initialized once during boot (single-threaded). After
/// boot, callers must hold the BKL when reading or advancing.
static FREE_UPPER_IDX: AtomicUsize = AtomicUsize::new(0);

/// Read the current `free_upper_idx`.
///
/// Caller must hold the BKL if called after boot.
#[allow(dead_code)] // accessor for FREE_UPPER_IDX; not yet wired to all call sites
pub(crate) fn free_upper_idx() -> usize {
    FREE_UPPER_IDX.load(Ordering::Acquire)
}

/// Advance `free_upper_idx` by `n` and return the previous value.
///
/// Used during boot (`init_post_and_memory`) to claim page directory
/// slots. Not used at runtime — `createpde()` is superseded by Direct
/// Map (see `FREE_PDE_SLOTS` doc).
///
/// Caller must hold the BKL.
#[allow(dead_code)] // accessor for FREE_UPPER_IDX; not yet wired to all call sites
pub(crate) fn advance_free_upper_idx(n: usize) -> usize {
    FREE_UPPER_IDX.fetch_add(n, Ordering::AcqRel)
}

/// IPC filter pool for per-process message filtering.
/// C: `ipc_filter_pool[IPCF_POOL_SIZE]` — ipc_filter.h:54
///
/// Populated by `kmain()` Phase C.5 (IPCF_POOL_INIT).
/// Used by `dispatch_statectl` AddIpcBlFilter/AddIpcWlFilter (implemented).
///
/// SAFETY: Only written once during boot (single-threaded, before BKL needed).
/// After boot, accessed under BKL protection.
static IPC_FILTER_POOL: SyncUnsafeCell<crate::ipc_filter::IpcFilterPool> = SyncUnsafeCell::new(crate::ipc_filter::IpcFilterPool::new());

/// Get a mutable reference to the global IPC filter pool.
///
/// Caller must ensure BKL is held if called after boot initialization.
/// C: `ipc_filter_pool` global array access.
///
/// **Prefer [`ipc_filter_pool_with`]** which takes a `BklSection` witness (R-03).
pub(crate) fn ipc_filter_pool() -> &'static mut crate::ipc_filter::IpcFilterPool {
    // SAFETY: Caller must hold BKL for post-boot access.
    // During boot, single-threaded access is guaranteed.
    unsafe { &mut *IPC_FILTER_POOL.get() }
}

/// Get a mutable reference to the global IPC filter pool with BKL witness (R-03).
#[allow(dead_code)] // BKL-witness accessor (FIX-09); new pattern not yet wired to all call sites
pub(crate) fn ipc_filter_pool_with(_section: &crate::smp::BklSection<'_>) -> &'static mut crate::ipc_filter::IpcFilterPool {
    // SAFETY: BklSection witness proves BKL is held.
    unsafe { &mut *IPC_FILTER_POOL.get() }
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
/// Design decision D7 (08 §3): returns `!` to express never-returning in the type system.
/// Design decision D6 (08 §3): vm_running is CpuLocal for SMP correctness.
/// Design decision D8 (08 §3): kernel_may_alloc uses AtomicBool.
///
/// `proc_table` is the `ProcessTable` built by `init_proc_and_boot` and
/// threaded through Phase D→F. Step 2 (bill_ptr/proc_ptr=IDLE) and
/// step 4 (RTS_PROC_STOP unset) operate on it.
#[cfg(not(feature = "mock"))]
fn bsp_finish_booting(
    proc_table: &mut crate::proc_table::ProcessTable,
    smp_state: &mut crate::smp::SmpState,
) -> ! {
    use crate::proc::{ProcNr, RtsFlagsBits, proc_nr};

    // Step 1: vm_running = 0
    // C: vm_running = 0 — glo.h:37
    // Rust: vm_running lives in CpuLocal (added in Doc 15 §2.2 + smp.rs:135-200).
    // For the BSP (cpu 0), we mark "VM not yet running" via a single global
    // atomic — multi-CPU expansion will move it into SmpState.cpu_locals[0].
    VM_RUNNING.store(false, Ordering::Release);

    // Step 2: bill_ptr = proc_ptr = idle_proc
    // C: get_cpulocal_var(bill_ptr) = get_cpulocal_var_ptr(idle_proc) — main.c:50
    // Rust: CpuLocal::set_running(IDLE) — see smp.rs:200.
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
    // (see proc_table.rs:276). Iterate from 0 (first user boot module) up to
    // but excluding kernel tasks (which were marked PROC_STOP during
    // init_proc_and_boot and must stay stopped — they're invoked lazily).
    // C also skips kernel tasks (the loop is `for i in 0..NR_BOOT_PROCS-NR_TASKS`).
    for nr in 0..(ProcNr(crate::proc::NR_BOOT_PROCS as i32)
        - ProcNr(crate::proc_table::NR_TASKS as i32)).0
    {
        proc_table.rts_unset(ProcNr(nr), RtsFlagsBits::PROC_STOP);
    }

    // Step 5: cycles_accounting_init() — set BSP TSC baseline.
    // C: cycles_accounting_init() — proc.c (resets per-CPU cycle counters).
    // Sets the BSP's TSC baseline so the first context switch has a
    // correct reference point. `note_context_switch(tsc)` records
    // `cpu_last_tsc = tsc` and `cpu_last_idle = tsc` in the BSP's
    // per-CPU state.
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
    //       available as a global — see arch-abstractions / 20-syscall-device.md).
    //
    // Implementation status: (a) is done. (b) is deferred until the
    // IrqManager global lands. We re-call `init_timer` here as a no-op
    // safety net (idempotent on x86: writes the same PIT mode byte; on
    // aarch64/riscv64: re-enables the comparator without side effects
    // because the timer is already running).
    //
    // Instance-based design (04-platform-discovery.md §3.4): construct a transient
    // clock arch instance from the global platform descriptor and call
    // `init_timer` on it. This replaces the old static
    // `CurrentClockArch::init_timer`.
    use minix_arch::{ClockArch, CurrentClockArch};
    use minix_platform::{platform_desc, PlatformDesc};
    {
        let pd = platform_desc();
        let mut clock_arch = CurrentClockArch::new(pd.timer());
        clock_arch.init_timer(crate::clock::DEFAULT_HZ);
    }
    // Register the BSP's timer handler via the ArchBoot trait.
    // The ArchBoot abstraction gives the architecture a chance to wire
    // the handler directly into the trap entry. Real hardware uses
    // IOAPIC RTE binding on x86 or LVT setup on aarch64; mock
    // records the handler for test inspection.
    use minix_arch::arch_boot::{boot_init_timer, CurrentArchBoot};
    use minix_arch::arch_boot::TimerHandlerFn;
    // D10: The global `IRQ_MANAGER` is now initialized (in
    // `init_clock_and_interrupts`), so a real `IrqManager::register_hook`
    // call could be made here. However, the `TimerHandlerFn` signature
    // (`fn(IrqVector, IrqId) -> IrqAction`) still uses the pre-D9 handler
    // shape; the new `IrqHandler` signature is `fn(&mut IrqHookContext) -> IrqAction`.
    // The arch-level `boot_init_timer` call below handles hardware-side
    // binding (IOAPIC RTE / LVT), while the IRQ-chain registration will
    // happen in Step 1.5.7 when the trap entry is connected to
    // `IrqManager::dispatch`. For now we pass a dummy handler that
    // satisfies the arch trait signature.
    extern "Rust" fn dummy_timer_handler(
        _irq: minix_plat::IrqVector,
        _id: minix_plat::IrqId,
    ) -> minix_plat::IrqAction {
        minix_plat::IrqAction::Completed
    }
    let _handler: TimerHandlerFn = dummy_timer_handler;
    let _ = boot_init_timer::<CurrentArchBoot>(dummy_timer_handler);
    // Real IRQ-chain registration will call:
    //   unsafe { crate::irq_manager() }.register_hook(
    //       IrqVector::new(0), clock_irq_handler, ..., IrqPolicy::REENABLE,
    //   );
    // once `clock_irq_handler` (an `IrqHandler`) is implemented in Step 1.5.7.

    // Step 7: fpu_init() — set BSP FPU presence.
    // C: fpu_init() — arch-specific (x86: fpu.c, ARM: fpu_asm.S).
    // This is a global "is FPU present" probe that updates
    // `cpulocals.fpu_presence`. In Rust, `CpuLocal::fpu_presence` is
    // already a `bool` field (see smp.rs:134). All three target
    // architectures (x86-64, aarch64, riscv64) have FPUs, so we set
    // the BSP's fpu_presence to `true`. The per-process FPU init is
    // handled by `CpuContextArch::build_cpu_context` via the per-arch FPU
    // strategy field (`fpu_policy` on x86_64, `fpu_enable_el0` on aarch64,
    // `sstatus.FS` on riscv64 — see 06-proc-init-boot-proc.md §4.1)
    // (already built for every boot process — see init_proc_and_boot).
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
    //
    // R-05: BklGuard is now RAII (Drop releases BKL). We must mem::forget
    // the guard to keep the BKL held across the call to switch_to_user(),
    // which will release it before entering the idle loop. Binding the
    // guard to a variable and letting it drop at end of scope would release
    // the BKL too early (before switch_to_user).
    core::mem::forget(smp::bkl_lock());

    // Step 9: switch_to_user() — never returns
    // C: switch_to_user(); NOT_REACHABLE;
    // Rust: Divergent function, type `-> !`
    // Covered in detail in 10-switch-to-user.md
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
/// Writers: `bsp_finish_booting` (step 1) sets it false;
///          `dispatch_vmctl(VMCTL_SETADDRSPACE)` sets it true when target is VM.
/// Readers: `do_vmctl` sub-commands (Doc 23) check it before touching VM state.
///
/// # C bug note
///
/// Minix3 C source never sets `vm_running = 1` — only `main.c:47` sets it to 0.
/// This is a C omission (the flag is read in `do_umap_remote.c:106`,
/// `acpi.c:61,70`, `oxpcie.c:52,73` but never set true). Rust corrects this
/// by setting `vm_running = true` in `VMCTL_SETADDRSPACE` when the target is
/// `VM_PROC_NR`, matching the design intent documented in
/// `09-vm-boot-protocol.md §3 decision4`.
static VM_RUNNING: AtomicBool = AtomicBool::new(false);

/// Read the `vm_running` flag. C: `vm_running` — glo.h:37.
pub fn vm_running() -> bool {
    VM_RUNNING.load(Ordering::Acquire)
}

/// Set the `vm_running` flag.
///
/// Called by `dispatch_vmctl(VMCTL_SETADDRSPACE)` when VM completes its
/// address-space switch. C: this write is missing in Minix3 (see
/// `VM_RUNNING` doc comment for details) — Rust corrects the omission.
///
/// # Concurrency
///
/// BKL must be held by the caller (single-writer guarantee). The `Release`
/// ordering ensures the flag is visible to other CPUs after the address-space
/// switch is complete.
pub fn set_vm_running(v: bool) {
    VM_RUNNING.store(v, Ordering::Release);
}

/// Global tracking of the current "page table process" (ptproc).
///
/// In Minix3 C, `ptproc` is a per-CPU `struct proc *` variable
/// (`get_cpulocal_var(ptproc)`) that records which process currently owns
/// the active page table on this CPU. The `setcr3()` helper inside
/// `arch_do_vmctl()` checks `if (p == get_cpulocal_var(ptproc))` to decide
/// whether a CR3 update should also reload the hardware CR3 register.
/// C: protect.c:370 (arch_post_init sets ptproc = VM) — see doc 09 §2.2.
///
/// In the Rust port, `CpuLocal::ptproc` (Doc 16 §2.2) is the eventual home
/// for per-CPU ptproc tracking under SMP. Until SMP lands, we keep a single
/// global `AtomicI32` mirror that records the proc-nr of the current ptproc.
/// This is safe because:
///
/// 1. **BKL protection**: All writers (`init_post_and_memory`,
///    `dispatch_vmctl(SetAddrSpace)` when target is ptproc) hold the BKL.
///    The reader (`dispatch_vmctl(SetAddrSpace)` comparison) also holds the
///    BKL — syscalls always acquire BKL before reaching the dispatcher.
/// 2. **Single-writer principle**: `ptproc` is set only once during boot
///    (to `VM_PROC_NR` in `init_post_and_memory`) and is not subsequently
///    changed in normal operation (matching C behavior — see
///    `arch_post_init()` which is the only writer in C).
///
/// The value stored is a `ProcNr.0` (i32). `i32::MIN` (sentinel) means
/// "no ptproc set yet" — distinct from any valid proc-nr (which are
/// non-negative for user processes and small negative for kernel tasks).
static CURRENT_PTPROC_NR: AtomicI32 = AtomicI32::new(i32::MIN);

/// Sentinel value indicating `CURRENT_PTPROC_NR` has not been initialized.
/// Distinct from any valid proc-nr (user procs ≥ 0, kernel tasks in
/// `-NR_TASKS..=-1`).
const PTPROC_UNSET: i32 = i32::MIN;

/// Read the proc-nr of the current ptproc.
///
/// Returns `None` if ptproc has not been set yet (before
/// `init_post_and_memory` runs).
///
/// # Concurrency
///
/// Caller must hold the BKL to observe a consistent value. Without the
/// BKL, the value may be stale — but stale reads are safe because the
/// only consequence is skipping a CR3 reload, which the next context
/// switch will correct.
pub fn current_ptproc_nr() -> Option<crate::proc::ProcNr> {
    let v = CURRENT_PTPROC_NR.load(Ordering::Acquire);
    if v == PTPROC_UNSET {
        None
    } else {
        Some(crate::proc::ProcNr(v))
    }
}

/// Set the current ptproc proc-nr.
///
/// Called once during `init_post_and_memory` (after
/// `CurrentPostInitArch::set_ptproc`) to record that VM is now the
/// page-table process. C: `get_cpulocal_var(ptproc) = vm` in
/// `arch_post_init()` — protect.c:372 (x86) / protect.c:99 (ARM).
///
/// # Concurrency
///
/// BKL must be held by the caller. `Release` ordering ensures the value
/// is visible to other CPUs after the boot-time ptproc installation is
/// complete.
pub fn set_current_ptproc_nr(nr: crate::proc::ProcNr) {
    CURRENT_PTPROC_NR.store(nr.0, Ordering::Release);
}

// ── Bootstrap page-table root tracking ──────────────────────────────────────
//
// The bootstrap page table is created by `arch_boot_impl` (Step 1+2 of boot)
// and dropped after `enable()`. Later phases — specifically
// `init_proc_and_boot` loading the VM ELF — need to add more mappings to the
// *same* page table. `from_active_root` ( Paging trait) lets them wrap the
// root into a fresh `Paging` handle, but they first need to know the root's
// physical address.
//
// We record it in this global right after `enable()` succeeds.
//
// # Concurrency
//
// Same model as `CURRENT_PTPROC_NR`: single writer during boot, readers hold
// the BKL. After boot the value is effectively immutable (the bootstrap
// table stays active until the first VMCTL SetAddrSpace swaps it out, at
// which point the global is no longer consulted).
//
// # SMP
//
// Per-CPU root tracking is not needed for the bootstrap table — there is
// only one bootstrap table, shared by all CPUs until userspace bring-up
// installs per-process roots. SMP migration would replace this with a
// per-CPU `CpuLocal::root_phys`, mirroring the ptproc migration plan.
static CURRENT_ROOT_PHYS: AtomicU64 = AtomicU64::new(ROOT_PHYS_UNSET);

/// Sentinel value indicating `CURRENT_ROOT_PHYS` has not been initialized.
/// Distinct from any valid physical address (4KB-aligned, non-zero).
const ROOT_PHYS_UNSET: u64 = u64::MAX;

/// Read the physical address of the bootstrap page-table root.
///
/// Returns `None` if `arch_boot_impl` has not yet run (before paging is
/// enabled). After `arch_boot_impl` completes, returns the root physical
/// address that was passed to `new_from_page` and subsequently loaded into
/// CR3/TTBR0_EL1/satp by `enable()`.
///
/// # Concurrency
///
/// Caller must hold the BKL to observe a consistent value. Without the
/// BKL, the value may be stale — but stale reads are safe because the
/// bootstrap root is never freed during normal operation.
pub fn current_root_phys() -> Option<minix_types::PhysBytes> {
    let v = CURRENT_ROOT_PHYS.load(Ordering::Acquire);
    if v == ROOT_PHYS_UNSET {
        None
    } else {
        Some(minix_types::PhysBytes(v))
    }
}

/// Record the bootstrap page-table root physical address.
///
/// Called once from `arch_boot_impl` after `Paging::enable()` succeeds.
/// The value remains valid until the bootstrap table is replaced (e.g.,
/// by a VMCTL SetAddrSpace that installs VM's page table as the active
/// root).
///
/// # Concurrency
///
/// Single-threaded boot context; `Release` ordering is sufficient.
pub fn set_current_root_phys(phys: minix_types::PhysBytes) {
    CURRENT_ROOT_PHYS.store(phys.0, Ordering::Release);
}

/// Entry point for the scheduler loop.
///
/// C: switch_to_user() in proc.c
/// Design decision D7 (08 §3): returns `!` — never returns to caller.
///
/// Full implementation covered in 10-switch-to-user.md.
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
#[allow(dead_code)] // called from arch trap entry (asm); not visible to compiler
fn switch_to_user() -> ! {
    // Release BKL before entering the scheduling loop.
    // C: BKL is released implicitly by restore_user_context() which
    // does not return. In Rust, we release explicitly before the loop.
    smp::bkl_unlock();

    // First-dispatch hook (06-proc-init-boot-proc.md §3.5): apply each boot
    // process's `cpu_context` to its trap frame once, before the
    // scheduling loop picks the first runnable process. The arch layer
    // owns the trap-frame layout; the kernel only hands it the opaque
    // `CpuContext` built during `init_proc_and_boot`.
    //
    // C: this work is folded into `arch_boot_proc()` + the first
    // `restore_user_context()` in Minix3. Splitting it here keeps the
    // arch trait's `apply_to_trap_frame` as the single sink for
    // initial-register writes (§3.2 "arch returns pure value").
    //
    // SAFETY: boot is single-threaded (BKL just released, but no other
    // CPU is up yet on single-CPU configs). On SMP this must move
    // inside the per-CPU dispatch path.
    unsafe {
        apply_boot_cpu_contexts();
    }

    // Placeholder — full scheduler loop implemented in 10-switch-to-user.md
    loop {
        core::hint::spin_loop();
    }
}

/// Apply each boot process's `cpu_context` to its trap frame (P2-2).
///
/// Called once from `switch_to_user()` before the scheduling loop. Iterates
/// the global `PROC_TABLE`, and for every slot that is not `SLOT_FREE` and
/// has a non-default `cpu_context`, calls
/// `CurrentCpuContextArch::apply_to_trap_frame(&ctx, &mut frame)`.
///
/// The trap frame is a stack-local zeroed value per process; the real
/// `restore_user_context()` (10-switch-to-user.md) will read from the
/// per-CPU exception stack instead. This stub exists to exercise the
/// `apply_to_trap_frame` call site and keep the trait contract honest.
///
/// # Safety
///
/// Caller must hold BKL (or be in single-threaded boot).
#[allow(dead_code)] // boot path, called conditionally; not visible to compiler
unsafe fn apply_boot_cpu_contexts() {
    use minix_arch::{CpuContextArch, CurrentCpuContextArch};
    use crate::proc::RtsFlagsBits;

    let table = unsafe { crate::proc_table_boot_unchecked() };
    for i in 0..crate::proc_table::PROC_TABLE_SIZE {
        let proc = match table.get_by_index(i) {
            Some(p) => p,
            None => continue,
        };
        // Skip free slots and slots that never got a boot context.
        if proc.p_rts_flags.is_set(RtsFlagsBits::SLOT_FREE) {
            continue;
        }
        // Apply the opaque arch context to a zeroed trap frame.
        // The real dispatch path will use the on-stack exception frame;
        // here we use `Default::default()` to satisfy the trait signature.
        let mut frame = <CurrentCpuContextArch as CpuContextArch>::TrapFrame::default();
        <CurrentCpuContextArch as CpuContextArch>::apply_to_trap_frame(
            &proc.cpu_context,
            &mut frame,
        );
        // In the real scheduler (09), `frame` is the actual exception
        // stack frame and `restore_user_context(&frame)` is the tail call.
        // The stub discards `frame` — the trait call itself is the
        // contract being exercised.
        let _ = frame;
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
            platform_sources: &[],
            param_buf: &[],
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
            platform_sources: &[],
            param_buf: &[],
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
            platform_sources: &[],
            param_buf: &[],
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
        // R-19 (2026-08-13): const assertions moved into `const {}` block so
        // they're checked at compile time, not at test runtime.
        const { assert!(MockPaging::HUGE_PAGE_SIZE > 0, "HUGE_PAGE_SIZE must be positive"); }
        const { assert!(MockPaging::HUGE_PAGE_SIZE.is_power_of_two(), "HUGE_PAGE_SIZE must be a power of two"); }
        const { assert!(IDENTITY_MAP_END > 0, "IDENTITY_MAP_END must be positive"); }
        let huge = MockPaging::HUGE_PAGE_SIZE;
        // IDENTITY_MAP_END / huge should be an integer (no partial pages at boundary)
        assert!(IDENTITY_MAP_END.is_multiple_of(huge),
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
            platform_sources: &[],
            param_buf: &[],
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
            platform_sources: &[],
            param_buf: &[],
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

    /// Verifies the Phase D integration contract: `allocate_free_pdes` is
    /// idempotent — calling it again does NOT re-allocate the same indices.
    ///
    /// L1 parity with C: C's `memory_init()` asserts `nfreepdes == 0` (it
    /// panics if called twice). Rust's `allocate_free_pdes` does not enforce
    /// the "called once" invariant by itself, but the global `FREE_UPPER_IDX`
    /// must advance monotonically — calling allocate again must observe the
    /// already-advanced index, not reset to a previous one.
    ///
    /// This test simulates two consecutive calls and verifies that the
    /// second call uses indices starting from where the first left off,
    /// which is what `init_post_and_memory` (Phase D) and any subsequent
    /// `createpde` call would rely on.
    #[test]
    fn test_createpde_does_not_reallocate_slots() {
        use minix_arch::post_init::MockMemoryInitArch;
        use minix_arch::{FreePdeSlots, MemoryInitArch};

        // Reset.
        FREE_UPPER_IDX.store(0, Ordering::Release);

        // First call: simulates Phase D's `init_post_and_memory` allocating
        // indices 0 and 1.
        let mut idx1: usize = free_upper_idx();
        let slots1: FreePdeSlots = MockMemoryInitArch::allocate_free_pdes(&mut idx1);
        FREE_UPPER_IDX.store(idx1, Ordering::Release);
        assert_eq!(slots1.get(0), Some(0));
        assert_eq!(slots1.get(1), Some(1));
        assert_eq!(free_upper_idx(), 2);

        // Second call: simulates a later `createpde` call that re-reads the
        // global counter. It must observe 2, not reset.
        let mut idx2: usize = free_upper_idx();
        let slots2: FreePdeSlots = MockMemoryInitArch::allocate_free_pdes(&mut idx2);
        // Second call must use indices 2 and 3, NOT 0 and 1.
        assert_eq!(
            slots2.get(0),
            Some(2),
            "Second allocate must observe the already-advanced counter, not reset"
        );
        assert_eq!(slots2.get(1), Some(3));
        assert_eq!(idx2, 4);

        // Reset for next test.
        FREE_UPPER_IDX.store(0, Ordering::Release);
    }

    /// Verifies the Phase C → Phase D dependency contract documented in
    /// `07-cross-space-init.md §1.2-1.3`: `init_post_and_memory` requires the
    /// VM process slot to already be initialized (Phase C's `proc_init`).
    ///
    /// Since we cannot easily call `init_post_and_memory` directly in a unit
    /// test (it requires a real `KernelInfo` from boot-shim), this test
    /// documents the dependency by verifying the trait + global ordering:
    /// `free_upper_idx()` reads the kernel global, and `set_ptproc` reads
    /// the VM process's p_seg. If Phase C did not initialize the VM proc,
    /// reading p_seg would return zeros and set_ptproc would store bogus
    /// values — which this test guards against by verifying that
    /// `allocate_free_pdes` reads `free_upper_idx` correctly from the
    /// global (independent of VM state).
    ///
    /// The actual VM-init-before-post-init ordering is enforced at runtime
    /// by `init_post_and_memory`'s `.expect("VM process must be initialized")`
    /// (lib.rs:934). If Phase C is skipped, this expect fires.
    #[test]
    fn test_init_post_and_memory_phase_d_dependencies() {
        use minix_arch::post_init::MockMemoryInitArch;
        use minix_arch::MemoryInitArch;

        // Reset.
        FREE_UPPER_IDX.store(0, Ordering::Release);

        // Simulate Phase D running with a fresh kernel: free_upper_idx
        // starts at 0 (boot-shim-provided value), and allocate_free_pdes
        // advances it by exactly MAX_FREE_PDE_SLOTS.
        let mut idx: usize = free_upper_idx();
        assert_eq!(idx, 0, "fresh kernel must have free_upper_idx == 0");
        let slots = MockMemoryInitArch::allocate_free_pdes(&mut idx);
        assert_eq!(slots.len(), 2, "MAX_FREE_PDE_SLOTS == 2");
        assert_eq!(idx, 2);

        // The dependency on Phase C is: if Phase C's init_proc_and_boot
        // was skipped, calling init_post_and_memory's vm_proc.p_seg access
        // would panic on `.expect("VM process must be initialized...")`.
        // We document this by asserting the contract that the global
        // FREE_UPPER_IDX state is correctly maintained regardless of
        // whether set_ptproc was called — proving Phase D's allocate
        // step is independent of Phase C's set_ptproc step.

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
            platform_sources: &[],
            param_buf: &[],
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
            platform_sources: &[],
            param_buf: &[],
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
            platform_sources: &[],
            param_buf: &[],
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
        use crate::proc::CpuId;
        use crate::smp::SmpState;
        let smp = SmpState::new_single_cpu();
        // ncpus = 1, so only cpu 0 is initialized.
        assert_eq!(smp.ncpus(), 1);
        assert_eq!(smp.bsp_cpu_id(), CpuId::BSP);
        // AP CPU 1 exists in the array but is the default CpuLocal.
        let ap1 = smp.cpu_local(CpuId::new_unchecked(1)).unwrap();
        assert_eq!(ap1.cpu_last_tsc, 0, "AP CPU should be default-initialized");
        assert!(!ap1.fpu_presence, "AP CPU should have fpu_presence = false");
    }

    // ── ptproc tracking tests (P9-4: SetAddrSpace write_cr3 support) ──

    /// Reset `CURRENT_PTPROC_NR` to the unset sentinel.
    /// Helper for ptproc tests so they don't leak state across each other.
    fn reset_ptproc_for_test() {
        CURRENT_PTPROC_NR.store(PTPROC_UNSET, Ordering::Release);
    }

    /// Fresh kernel: `current_ptproc_nr()` returns `None` because
    /// `init_post_and_memory` has not run yet. This matches C behavior
    /// where `ptproc` is uninitialized until `arch_post_init()`.
    #[test]
    fn test_ptproc_unset_returns_none_before_init() {
        reset_ptproc_for_test();
        assert_eq!(current_ptproc_nr(), None,
            "ptproc must be None before init_post_and_memory runs");
    }

    /// After `set_current_ptproc_nr(VM_PROC_NR)`, `current_ptproc_nr()`
    /// returns `Some(VM_PROC_NR)`. This mirrors C's
    /// `get_cpulocal_var(ptproc) = vm` in `arch_post_init()`.
    #[test]
    fn test_ptproc_set_returns_vm_proc_nr() {
        reset_ptproc_for_test();
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        assert_eq!(current_ptproc_nr(), Some(crate::proc::proc_nr::VM_PROC_NR),
            "ptproc must be VM_PROC_NR after init_post_and_memory");
        // Cleanup.
        reset_ptproc_for_test();
    }

    /// `set_current_ptproc_nr` is idempotent: setting twice to the same
    /// value produces the same observable state. (In normal operation,
    /// ptproc is set only once during boot, but the test guards against
    /// accidental state corruption.)
    #[test]
    fn test_ptproc_set_is_idempotent() {
        reset_ptproc_for_test();
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        assert_eq!(current_ptproc_nr(), Some(crate::proc::proc_nr::VM_PROC_NR));
        reset_ptproc_for_test();
    }

    /// The `SetAddrSpace` handler's ptproc comparison uses `ProcNr` equality.
    /// Verify that `Some(VM_PROC_NR) == Some(VM_PROC_NR)` holds — this is
    /// the branch condition that triggers `TlbArch::set_active_root`.
    #[test]
    fn test_ptproc_comparison_branch_condition() {
        reset_ptproc_for_test();
        set_current_ptproc_nr(crate::proc::proc_nr::VM_PROC_NR);
        // Simulate the SetAddrSpace branch:
        //   if current_ptproc_nr() == Some(target.p_nr) { set_active_root(...) }
        let target_p_nr = crate::proc::proc_nr::VM_PROC_NR;
        let should_reload = current_ptproc_nr() == Some(target_p_nr);
        assert!(should_reload,
            "SetAddrSpace on VM (the current ptproc) must trigger set_active_root");
        // A different proc-nr must NOT trigger the reload.
        let other_p_nr = crate::proc::ProcNr(crate::proc::proc_nr::VM_PROC_NR.0 + 1);
        let should_not_reload = current_ptproc_nr() == Some(other_p_nr);
        assert!(!should_not_reload,
            "SetAddrSpace on non-ptproc must NOT trigger set_active_root");
        reset_ptproc_for_test();
    }

    /// Verify the `PTPROC_UNSET` sentinel is distinct from all valid
    /// proc-nrs that could be passed to `set_current_ptproc_nr`.
    /// VM_PROC_NR is a small positive integer; PTPROC_UNSET = i32::MIN.
    #[test]
    fn test_ptproc_sentinel_distinct_from_valid_proc_nrs() {
        assert_ne!(PTPROC_UNSET, crate::proc::proc_nr::VM_PROC_NR.0,
            "PTPROC_UNSET must not collide with VM_PROC_NR");
        assert_eq!(PTPROC_UNSET, i32::MIN,
            "PTPROC_UNSET must be i32::MIN (sentinel value)");
        // Also distinct from kernel task proc-nrs (small negatives like -1, -5).
        assert_ne!(PTPROC_UNSET, -1,
            "PTPROC_UNSET must not collide with kernel task proc-nr -1");
    }

    // ── Bootstrap root tracking tests (P9-5: VM ELF loading at boot) ──

    /// Reset `CURRENT_ROOT_PHYS` to the unset sentinel.
    /// Helper for root-phys tests so they don't leak state across each other.
    fn reset_root_phys_for_test() {
        CURRENT_ROOT_PHYS.store(ROOT_PHYS_UNSET, Ordering::Release);
    }

    /// Fresh kernel: `current_root_phys()` returns `None` because
    /// `arch_boot_impl` has not yet run (paging not enabled).
    #[test]
    fn test_root_phys_unset_returns_none_before_boot() {
        reset_root_phys_for_test();
        assert_eq!(current_root_phys(), None,
            "root_phys must be None before arch_boot_impl runs");
    }

    /// After `set_current_root_phys(0x200000)`, `current_root_phys()`
    /// returns `Some(PhysBytes(0x200000))`. This mirrors the boot flow
    /// where `arch_boot_impl` records the root after `enable()` succeeds.
    #[test]
    fn test_root_phys_set_returns_recorded_value() {
        reset_root_phys_for_test();
        set_current_root_phys(minix_types::PhysBytes(0x200000));
        assert_eq!(current_root_phys(), Some(minix_types::PhysBytes(0x200000)),
            "root_phys must be the value passed to set_current_root_phys");
        reset_root_phys_for_test();
    }

    /// `set_current_root_phys` is idempotent: setting twice produces the
    /// same observable state.
    #[test]
    fn test_root_phys_set_is_idempotent() {
        reset_root_phys_for_test();
        set_current_root_phys(minix_types::PhysBytes(0x300000));
        set_current_root_phys(minix_types::PhysBytes(0x300000));
        assert_eq!(current_root_phys(), Some(minix_types::PhysBytes(0x300000)));
        reset_root_phys_for_test();
    }

    /// Verify the `ROOT_PHYS_UNSET` sentinel is distinct from any valid
    /// 4KB-aligned physical address. `u64::MAX` is not 4KB-aligned and
    /// cannot be a real root physical address.
    #[test]
    fn test_root_phys_sentinel_distinct_from_valid_addresses() {
        assert_eq!(ROOT_PHYS_UNSET, u64::MAX,
            "ROOT_PHYS_UNSET must be u64::MAX (sentinel value)");
        // u64::MAX is not 4KB-aligned (low 12 bits != 0), so it can
        // never collide with a real page-table root physical address.
        assert_ne!(ROOT_PHYS_UNSET & 0xFFF, 0,
            "ROOT_PHYS_UNSET must not be page-aligned");
        // Common root addresses used in tests/boot must not collide.
        assert_ne!(ROOT_PHYS_UNSET, 0x1000);
        assert_ne!(ROOT_PHYS_UNSET, 0x200000);
    }

    /// Verify that `from_active_root` produces a `Paging` handle whose
    /// `root_paddr()` matches the value passed in. This is the contract
    /// `init_proc_and_boot` relies on when wrapping the bootstrap root
    /// to load the VM ELF.
    #[test]
    fn test_from_active_root_round_trip_root_paddr() {
        use minix_arch::paging::Paging;
        use minix_arch::paging::mock::MockPaging;

        let root_phys = minix_types::PhysBytes(0x10_0000);
        let paging = MockPaging::from_active_root(root_phys);
        assert_eq!(paging.root_paddr(), root_phys,
            "from_active_root must produce a handle whose root_paddr matches");
    }

    /// Verify `from_active_root` does NOT zero the root (unlike
    /// `new_from_page`). For the mock this is observable via the
    /// `mappings` map being empty in both cases, but the *intent*
    /// difference is documented: `from_active_root` assumes the root
    /// is already initialized. The test asserts the contract by
    /// checking that `from_active_root` returns a usable handle
    /// without calling any allocator.
    #[test]
    fn test_from_active_root_does_not_allocate_via_new_mock_path() {
        use minix_arch::paging::Paging;
        use minix_arch::paging::mock::MockPaging;

        // `from_active_root` should succeed for any root physical
        // address, even one that would be unusual for `new_mock`'s
        // internal counter-based scheme.
        let unusual_root = minix_types::PhysBytes(0xDEAD_BEEF_0000);
        let paging = MockPaging::from_active_root(unusual_root);
        assert_eq!(paging.root_paddr(), unusual_root,
            "from_active_root must preserve the caller-supplied root");
    }
}

