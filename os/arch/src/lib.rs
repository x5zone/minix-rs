//! Hardware Abstraction Layer
//!
//! Provides cross-architecture hardware mechanism abstractions and trait interfaces.
//! Concrete implementations are provided by each architecture module (mock, x86_64, arm64, riscv64).
//!
//! # Crate structure
//!
//! - **CPU ISA traits** (this crate): paging, protection, trap entry, exception, clock, etc.
//! - **Board-level platform traits** (`minix-plat`): early console, interrupt controller
//!
//! # Design principles
//!
//! 1. **Distributed definition**: Each feature module defines its own traits (e.g. paging, interrupts, timers)
//! 2. **Centralized implementation**: All traits are implemented within the arch crate
//! 3. **Architecture-independent**: OS code depends only on traits, not on specific hardware

#![cfg_attr(not(feature = "mock"), no_std)]

extern crate alloc;

pub mod arch;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "aarch64")]
pub mod arm64;
#[cfg(target_arch = "riscv64")]
pub mod riscv64;

// ── Re-export board-level platform abstractions from minix-plat ──
pub use minix_plat::{
    EarlyConsole, InterruptController, IrqVector, IrqId, IrqNotifyId, IrqPolicy, IrqAction,
    NR_IRQ_VECTORS, NR_IRQ_HOOKS, CurrentInterruptController, CurrentEarlyConsole,
};

#[cfg(feature = "mock")]
pub use minix_plat::{MockInterruptController, MockEarlyConsole};

// ── Backward-compatible re-exports of CPU ISA modules ──
pub use arch::paging;
pub use arch::paging_ext;
pub use arch::pt_alloc;
pub use arch::direct_map;
pub use arch::frame;
pub use arch::pte_walk_arch;
pub use arch::protection;
pub use arch::trap_entry;
pub use arch::trap_return;
pub use arch::exception;
pub use arch::exception_dispatcher;
pub use arch::clock;
pub use arch::fpu_arch;
pub use arch::signal_context;
pub use arch::smp;
pub use arch::cpu_identity;
pub use arch::arch_init;
pub use arch::timer_irq_gate;
pub use arch::boot;
pub use arch::stacktrace;
pub use arch::tlb_arch;

pub use paging_ext::{PagingWithId, HugePages};
pub use direct_map::DirectMapArch;
pub use arch::dm_coverage::{DmCoverageArch, DmRange, establish_dm_range};
pub use pte_walk_arch::PteWalkArch;
#[cfg(feature = "mock")]
pub use pte_walk_arch::MockPteWalk;
pub use protection::{ProtectionArch, Privilege, InterruptVector};
#[cfg(feature = "mock")]
pub use protection::MockProtection;
pub use trap_entry::TrapEntryArch;
#[cfg(feature = "mock")]
pub use trap_entry::MockTrapEntry;
pub use trap_return::TrapReturnArch;
#[cfg(feature = "mock")]
pub use trap_return::MockTrapReturn;
pub use exception::{ExceptionArch, FaultContext, RecoveryPoint};
pub use exception_dispatcher::{
    ExceptionDispatcher, ExceptionOutcome, ExceptionClass, ExceptionSignal,
};
pub use clock::{ClockArch, DEFAULT_HZ};
pub use timer_irq_gate::TimerIrqGate;
#[cfg(feature = "mock")]
pub use clock::MockClockArch;
pub use fpu_arch::{FpuArch, MockFpuArch, MockFpuState};
pub use signal_context::{
    SignalContext, SignalInfo, MockSignalContext,
    MockSigContext, MockSigFrame, MockSigCpuContext,
    SC_MAGIC, MF_FPU_INITIALIZED, MF_CONTEXT_SET, X86_FLAGS_USER,
    KTS_NONE, KTS_INT_HARD, KTS_INT_ORIG, KTS_FULLCONTEXT, KTS_SYSCALL,
};
pub use arch::trap_style::{ReturnSequence, TrapStyle};
pub use smp::SmpArch;
pub use arch_init::ArchInit;
pub use boot::{
    CpuContextArch, EntrySpec, ProcKind, ProcNr,
    VmLoadResult, VmLoadError, load_vm_elf,
};
pub use stacktrace::StacktraceArch;
pub use arch::current::{Arch, CurrentArch};

#[cfg(feature = "mock")]
pub use paging::mock::MockPaging;

#[cfg(feature = "mock")]
pub use paging::mock::MockAsid;

// ── CurrentPaging type alias ──
//
// When `mock` feature is enabled (tests), always use `MockPaging` regardless
// of host `target_arch`. This prevents tests running on x86_64 host from
// selecting the real `X86_64Paging` (which touches Direct Map memory and
// would SIGSEGV outside QEMU). For the real kernel build (no `mock`
// feature), select by `target_arch`.
#[cfg(feature = "mock")]
pub type CurrentPaging = MockPaging;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentPaging = crate::x86_64::paging::X86_64Paging;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentPaging = crate::arm64::paging::AArch64Paging;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentPaging = crate::riscv64::paging::Riscv64Paging;

#[cfg(feature = "mock")]
pub use direct_map::MockDirectMap;

#[cfg(target_arch = "x86_64")]
pub use direct_map::X86_64DirectMap;
#[cfg(target_arch = "aarch64")]
pub use direct_map::AArch64DirectMap;
#[cfg(target_arch = "riscv64")]
pub use direct_map::Riscv64DirectMap;

#[cfg(feature = "mock")]
pub type CurrentDirectMap = MockDirectMap;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentDirectMap = X86_64DirectMap;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentDirectMap = AArch64DirectMap;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentDirectMap = Riscv64DirectMap;

// ── CurrentDmCoverage type alias ──
//
// Selects the architecture-specific `DmCoverageArch` implementor (DM
// coverage establishment on the bootstrap root). Like `CurrentPteWalk`, the
// implementor is a ZST: the operation is static boot-time table surgery, not
// a per-handle page-table service.
#[cfg(feature = "mock")]
pub use arch::dm_coverage::mock::MockDmCoverage;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub use x86_64::paging::X86_64DmCoverage;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub use arm64::paging::AArch64DmCoverage;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub use riscv64::paging::Riscv64DmCoverage;

#[cfg(feature = "mock")]
pub type CurrentDmCoverage = MockDmCoverage;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentDmCoverage = X86_64DmCoverage;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentDmCoverage = AArch64DmCoverage;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentDmCoverage = Riscv64DmCoverage;

// ── CurrentPteWalk type aliases ──
//
// Selects the architecture-specific `PteWalkArch` implementor at compile
// time. The kernel's cross-space copy code (`vm.rs::lookup_in_table`)
// uses `CurrentPteWalk` as the generic type parameter, so no
// `#[cfg(target_arch)]` leaks into kernel code (Phase 2 of the
// TODO/DEFERRED completion plan).
#[cfg(feature = "mock")]
pub type CurrentPteWalk = MockPteWalk;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentPteWalk = crate::x86_64::paging::X86_64PteWalk;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentPteWalk = crate::arm64::paging::AArch64PteWalk;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentPteWalk = crate::riscv64::paging::Riscv64PteWalk;

// ── CurrentProtection type aliases ──
//
// Mock branch prevents tests on x86_64 host from touching real GDT/TSS
// registers (FIX-06: R-11). Matches the pattern used by CurrentPaging,
// CurrentDirectMap, CurrentClockArch, etc.
#[cfg(feature = "mock")]
pub type CurrentProtection = MockProtection;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentProtection = crate::x86_64::protection::X86_64Protection;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentProtection = crate::arm64::protection::AArch64Protection;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentProtection = crate::riscv64::protection::Riscv64Protection;

// ── Bootstrap allocation bounds ──
//
// End of the boot identity-mapped window: `arch_boot_impl` maps PA
// `[0, END)` identity on the bootstrap root (C: pg_identity() maps
// 1024 × 4MiB = 4 GiB — pg_utils.c:162). Bootstrap allocations that must
// stay identity-write-reachable (the page-table root, the boot bump
// region) have to sit below this end.
pub const BOOT_IDENTITY_MAP_END: u64 = 0x1_0000_0000;

/// Highest PA *end* any bootstrap-tree allocation (page-table root, boot
/// bump region) may use: `min(BOOT_IDENTITY_MAP_END, VM DM window PA end)`.
///
/// Below this bound the bootstrap root is simultaneously
/// - identity-write-reachable — the DM establishment write channel
///   (`VA = PA`) works, and
/// - representable in the VM DM window — VM self page-table walks can
///   read/write root and PT pages through the window
///
/// (07-paging_init_design §6.1 资格过滤 ②). The boot-shims enforce this at
/// allocation time; the kernel re-validates at DM establishment time.
pub const fn boot_dm_admissible_end() -> u64 {
    // `core::cmp::min` is not const-stable; compare explicitly.
    if BOOT_IDENTITY_MAP_END < CurrentDirectMap::VM_DIRECT_MAP_SIZE {
        BOOT_IDENTITY_MAP_END
    } else {
        CurrentDirectMap::VM_DIRECT_MAP_SIZE
    }
}

// ── CurrentTrapEntry type aliases ──
//
// Mock branch prevents tests on x86_64 host from touching real IDT
// registers (FIX-06: R-11).
#[cfg(feature = "mock")]
pub type CurrentTrapEntry = MockTrapEntry;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentTrapEntry = crate::x86_64::trap_entry::X86_64TrapEntry;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentTrapEntry = crate::arm64::trap_entry::AArch64TrapEntry;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentTrapEntry = crate::riscv64::trap_entry::Riscv64TrapEntry;

// ── S-8 trap-stub facade (smp_todo.md §3.7) ──
//
// `install_trap_stubs` writes the arch's real asm entry addresses into every
// gate of the table; `syscall_entry_va` reports the SYSCALL entry address for
// `TrapEntryArch::configure_syscall` (x86-64 LSTAR). Together they turn
// `TrapEntryArch::load()` from "deferred" into a safe call: after both run,
// no gate has an empty handler (the S-8 stage invariant).
//
// aarch64/riscv64 stub inventory (S-8 deliverable, 2026-09-14): both archs
// use a single fixed asm vector (VBAR_EL1 table / stvec direct mode), where
// `set_handler` is already a documented no-op — there is nothing to install
// per-vector. The inventory also found that their `load()` paths reference
// asm symbols that are declared but never defined
// (`exc_vector_table` / `trap_vector`; zero `global_asm!` in either module)
// — currently masked only by dead-code elimination because no caller
// invokes `load()`. Their stub bodies stay part of their own bring-up
// lanes (S-4 arm64/riscv64); recorded in smp_todo.md S-8.
#[cfg(feature = "mock")]
pub fn install_trap_stubs(_entry: &mut CurrentTrapEntry) {}
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn install_trap_stubs(entry: &mut CurrentTrapEntry) {
    crate::x86_64::trap_stub::install_idt_handlers(entry);
}
#[cfg(all(not(feature = "mock"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn install_trap_stubs(_entry: &mut CurrentTrapEntry) {}

/// SYSCALL entry address for `TrapEntryArch::configure_syscall`.
///
/// x86-64: the LSTAR asm entry (`x86_syscall_entry`). aarch64/riscv64: 0 —
/// their SYSCALL (SVC/ecall) shares the exception vector, so
/// `configure_syscall` is a no-op and the value is unused (C parity: the
/// kernel sources the entry from its own asm label, protect.c:189-205 —
/// not from the boot-provided `KernelInfo.syscall_entry`, which is
/// reference-only metadata).
#[cfg(feature = "mock")]
pub fn syscall_entry_va() -> minix_types::VirBytes {
    minix_types::VirBytes::new(0)
}
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn syscall_entry_va() -> minix_types::VirBytes {
    crate::x86_64::trap_stub::syscall_entry_va()
}
#[cfg(all(not(feature = "mock"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn syscall_entry_va() -> minix_types::VirBytes {
    minix_types::VirBytes::new(0)
}

/// Register the kernel-side trap/syscall dispatch bodies with the x86-64
/// entry stubs (see `x86_64::trap_stub` for the gate rationale). Must run
/// before `TrapEntryArch::load()`; a no-op on the mock/other-arch branches
/// where the stubs do not exist.
#[cfg(feature = "mock")]
pub fn register_trap_dispatchers(
    _trap: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
    _syscall: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
) {
}
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn register_trap_dispatchers(
    trap: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
    syscall: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
) {
    crate::x86_64::trap_stub::register_dispatchers(trap, syscall);
}
#[cfg(all(not(feature = "mock"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn register_trap_dispatchers(
    _trap: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
    _syscall: unsafe extern "C" fn(&mut x86_64::trap_stub::TrapFrame),
) {
}

#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub use x86_64::trap_stub::TrapFrame;

/// S-4 per-CPU bring-up helpers (§3.3 per-CPU MSR contract). No-ops on the
/// mock/other-arch branches; on x86-64 these write the CURRENT CPU's MSRs —
/// the AP wiring calls them from the AP itself.
#[cfg(feature = "mock")]
pub fn ap_write_syscall_msrs(_entry: minix_types::VirBytes) {}
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn ap_write_syscall_msrs(entry: minix_types::VirBytes) {
    crate::x86_64::trap_stub::write_syscall_msrs(entry);
}
#[cfg(all(not(feature = "mock"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn ap_write_syscall_msrs(_entry: minix_types::VirBytes) {}

/// Enable THIS CPU's LAPIC (S-10: the AP-side per-CPU half of controller
/// init — the LAPIC is disabled after INIT and deaf to IPIs until enabled).
/// No-op on mock/other-arch branches.
#[cfg(feature = "mock")]
pub fn ap_enable_lapic() {}
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn ap_enable_lapic() {
    // SAFETY: the AP enables its own LAPIC during bring-up, before any IPI
    // can target it (see the plat function's safety contract).
    unsafe { minix_plat::x86_64::interrupt::X86_64InterruptController::enable_current_cpu_lapic(); }
}
#[cfg(all(not(feature = "mock"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn ap_enable_lapic() {}

/// Read this CPU's per-CPU identity (x86-64: `gs:0x10` written by
/// `program_gs`; other archs return 0 — their identity path is per-CPU
/// register based and lands with their own S-4 lanes).
#[cfg(feature = "mock")]
pub fn ap_cpu_id_readback() -> u64 {
    0
}
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub fn ap_cpu_id_readback() -> u64 {
    crate::x86_64::trap_stub::gs_cpu_id()
}
#[cfg(all(not(feature = "mock"), any(target_arch = "aarch64", target_arch = "riscv64")))]
pub fn ap_cpu_id_readback() -> u64 {
    0
}

// ── CurrentClockArch type aliases ──
#[cfg(feature = "mock")]
pub type CurrentClockArch = MockClockArch;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentClockArch = crate::x86_64::clock::X86_64ClockArch;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentClockArch = crate::arm64::clock::AArch64ClockArch;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentClockArch = crate::riscv64::clock::Riscv64ClockArch;

// ── CurrentCpuIdentity type aliases ──
//
// Selects the per-arch CPU identity probe at compile time (same mock-first
// pattern as CurrentPaging/CurrentClockArch: tests run on the x86_64 host
// and must not execute `cpuid`/`mrs`/`ecall`).
#[cfg(feature = "mock")]
pub type CurrentCpuIdentity = arch::cpu_identity::mock::MockCpuIdentity;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentCpuIdentity = crate::x86_64::cpu_identity::X86_64CpuIdentity;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentCpuIdentity = crate::arm64::cpu_identity::AArch64CpuIdentity;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentCpuIdentity = crate::riscv64::cpu_identity::Riscv64CpuIdentity;


// ── CurrentFpuArch type aliases ──
//
// Selects the architecture-specific `FpuArch` implementor at compile time.
// When `mock` feature is enabled (tests), uses `MockFpuArch` (all no-ops).
#[cfg(feature = "mock")]
pub type CurrentFpuArch = MockFpuArch;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentFpuArch = crate::x86_64::fpu::X86_64FpuArch;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentFpuArch = crate::arm64::fpu::AArch64FpuArch;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentFpuArch = crate::riscv64::fpu::Riscv64FpuArch;

// ── CurrentFpuState type aliases ──
//
// Selects the architecture-specific FPU state buffer type. This is the
// `State` associated type of `CurrentFpuArch`. Stored per-process in
// `KProcess.fpu_state` for save/restore during context switches and
// signal handling.
//
// # Sizes
//
// - mock (test):     0 bytes (ZST)
// - x86-64 (FXSAVE): 512 bytes
// - aarch64 (FPSIMD): 528 bytes
// - riscv64 (F/D):   264 bytes
//
// C: `p_seg.fpu_state[FPU_XFP_SIZE]` — kernel/proc.h
#[cfg(feature = "mock")]
pub type CurrentFpuState = MockFpuState;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentFpuState = crate::x86_64::fpu::X86_64FpuState;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentFpuState = crate::arm64::fpu::AArch64FpuState;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentFpuState = crate::riscv64::fpu::Riscv64FpuState;

// ── CurrentSignalContext type aliases ──
//
// Selects the architecture-specific `SignalContext` implementor at compile
// time. Unlike other arch traits, `SignalContext` has NO mock variant
// because its associated type `CpuContext` MUST match `CurrentCpuContext`
// (the type stored in `KProcess.cpu_context`). Since `CurrentCpuContext`
// has no mock variant, `CurrentSignalContext` must also use the real arch
// implementation. This is safe because `SignalContext` methods are pure
// data transformations (no hardware access).
//
// `MockSignalContext` remains available for arch-crate-internal tests that
// don't need to interoperate with `KProcess`.
#[cfg(target_arch = "x86_64")]
pub type CurrentSignalContext = crate::x86_64::signal::X86_64SignalContext;
#[cfg(target_arch = "aarch64")]
pub type CurrentSignalContext = crate::arm64::signal::AArch64SignalContext;
#[cfg(target_arch = "riscv64")]
pub type CurrentSignalContext = crate::riscv64::signal::Riscv64SignalContext;

// ── CurrentArchInit type aliases ──
#[cfg(target_arch = "x86_64")]
pub type CurrentArchInit = crate::x86_64::arch_init::X86_64ArchInit;
#[cfg(target_arch = "aarch64")]
pub type CurrentArchInit = crate::arm64::arch_init::AArch64ArchInit;
#[cfg(target_arch = "riscv64")]
pub type CurrentArchInit = crate::riscv64::arch_init::Riscv64ArchInit;

// ── CurrentTimerIrqGate type aliases ──
//
// Selects the architecture-specific `TimerIrqGate` implementor at compile
// time. `TimerIrqGate` has only static methods (no instance state), so the
// selection follows the `CurrentArchInit` pattern (no mock variant).
#[cfg(target_arch = "x86_64")]
pub type CurrentTimerIrqGate = crate::x86_64::timer_irq_gate::X86_64TimerIrqGate;
#[cfg(target_arch = "aarch64")]
pub type CurrentTimerIrqGate = crate::arm64::timer_irq_gate::AArch64TimerIrqGate;
#[cfg(target_arch = "riscv64")]
pub type CurrentTimerIrqGate = crate::riscv64::timer_irq_gate::Riscv64TimerIrqGate;

// ── CurrentCpuContextArch / CpuContext / TrapFrame type aliases ──
//
// Replaces the old `CurrentBootProcArch` (see 06-proc-init-boot-proc.md §3.5
// for the renaming rationale — the abstraction is "process's CPU
// context", not just the boot phase).
#[cfg(target_arch = "x86_64")]
pub type CurrentCpuContextArch = crate::x86_64::boot::X86_64CpuContextArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentCpuContextArch = crate::arm64::boot::AArch64CpuContextArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentCpuContextArch = crate::riscv64::boot::Riscv64CpuContextArch;

#[cfg(target_arch = "x86_64")]
pub type CurrentCpuContext = crate::x86_64::boot::X86_64CpuContext;
#[cfg(target_arch = "aarch64")]
pub type CurrentCpuContext = crate::arm64::boot::AArch64CpuContext;
#[cfg(target_arch = "riscv64")]
pub type CurrentCpuContext = crate::riscv64::boot::Riscv64CpuContext;

#[cfg(target_arch = "x86_64")]
pub type CurrentTrapFrame = crate::x86_64::exception::X86_64ExceptionFrame;
#[cfg(target_arch = "aarch64")]
pub type CurrentTrapFrame = crate::arm64::exception::AArch64ExceptionFrame;
#[cfg(target_arch = "riscv64")]
pub type CurrentTrapFrame = crate::riscv64::exception::Riscv64ExceptionFrame;

// ── CurrentSmpArch type aliases ──
//
// Selects the architecture-specific `SmpArch` implementor at compile time.
// The kernel's SMP module (`os/kernel/src/smp.rs`) uses `CurrentSmpArch`
// as the generic type parameter, so no `#[cfg(target_arch)]` leaks into
// kernel code (D7 in 16-smp.md).
#[cfg(feature = "mock")]
pub type CurrentSmpArch = crate::arch::smp::MockSmpArch;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentSmpArch = crate::x86_64::smp::X86_64SmpArch;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentSmpArch = crate::arm64::smp::AArch64SmpArch;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentSmpArch = crate::riscv64::smp::Riscv64SmpArch;

// ── TlbArch re-exports + CurrentTlbArch type alias (FIX-24, Phase 5) ──
//
// `TlbArch` provides CPU-wide TLB invalidation (flush_all / flush_addr)
// as static methods, used by `dispatch_vmctl` for SVMCTL_FLUSHTLB and
// SVMCTL_INVLPG. Unlike `Paging::flush_tlb` (instance method), `TlbArch`
// operates on the *current* CPU's TLB without a Paging instance.
pub use tlb_arch::{TlbArch, MockTlbArch};

#[cfg(feature = "mock")]
pub type CurrentTlbArch = MockTlbArch;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentTlbArch = crate::x86_64::tlb::X86_64TlbArch;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentTlbArch = crate::arm64::tlb::AArch64TlbArch;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentTlbArch = crate::riscv64::tlb::Riscv64TlbArch;

// ── CurrentTrapReturnArch type alias ──
//
// Selects the architecture-specific `TrapReturnArch` implementor (the
// return-path twin of `CurrentTrapEntry`). Mock build uses `MockTrapReturn`
// (panics on dispatch — see `arch/trap_return.rs`); tests exercise the
// scheduling stages, never the real mode switch.
#[cfg(feature = "mock")]
pub type CurrentTrapReturnArch = crate::arch::trap_return::MockTrapReturn;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentTrapReturnArch = crate::x86_64::trap_return::X86_64TrapReturn;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentTrapReturnArch = crate::arm64::trap_return::AArch64TrapReturn;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentTrapReturnArch = crate::riscv64::trap_return::Riscv64TrapReturn;

// ── CurrentStacktraceArch type aliases ──
//
// Selects the architecture-specific `StacktraceArch` implementor at compile
// time. Used by `proc_stacktrace` in the kernel's diagnostic path
// (SYS_DIAGCTL DIAGCTL_CODE_STACKTRACE). See `arch/stacktrace.rs`.
//
// Same pattern as `CurrentCpuContextArch` — no mock variant because
// `StacktraceArch: CpuContextArch` and tests run on the host arch.
#[cfg(target_arch = "x86_64")]
pub type CurrentStacktraceArch = crate::x86_64::boot::X86_64CpuContextArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentStacktraceArch = crate::arm64::boot::AArch64CpuContextArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentStacktraceArch = crate::riscv64::boot::Riscv64CpuContextArch;
