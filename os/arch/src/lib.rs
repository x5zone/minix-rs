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
pub use arch::pte_walk_arch;
pub use arch::protection;
pub use arch::trap_entry;
pub use arch::exception;
pub use arch::exception_dispatcher;
pub use arch::clock;
pub use arch::fpu_arch;
pub use arch::signal_context;
pub use arch::smp;
pub use arch::arch_init;
pub use arch::timer_irq_gate;
pub use arch::boot;
pub use arch::post_init;
pub use arch::stacktrace;
pub use arch::tlb_arch;

pub use paging_ext::{PagingWithId, HugePages};
pub use direct_map::DirectMapArch;
pub use pte_walk_arch::PteWalkArch;
#[cfg(feature = "mock")]
pub use pte_walk_arch::MockPteWalk;
pub use protection::{ProtectionArch, Privilege, InterruptVector};
#[cfg(feature = "mock")]
pub use protection::MockProtection;
pub use trap_entry::TrapEntryArch;
#[cfg(feature = "mock")]
pub use trap_entry::MockTrapEntry;
pub use exception::{ExceptionArch, FaultContext, RecoveryPoint};
pub use exception_dispatcher::{
    ExceptionDispatcher, ExceptionOutcome, ExceptionClass, ExceptionSignal, KernTrapStyle,
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
    KTS_NONE, KTS_INT_HARD, KTS_INT_ORIG, KTS_INT_UM, KTS_FULLCONTEXT, KTS_SYSENTER,
};
pub use smp::SmpArch;
pub use arch_init::ArchInit;
pub use boot::{
    CpuContextArch, EntrySpec, ProcKind, ProcNr,
    VmLoadResult, VmLoadError, load_vm_elf,
};
pub use stacktrace::StacktraceArch;
pub use post_init::{PostInitArch, MemoryInitArch, VmPageTableInfo, FreePdeSlots, MAX_FREE_PDE_SLOTS};

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

// ── CurrentClockArch type aliases ──
#[cfg(feature = "mock")]
pub type CurrentClockArch = MockClockArch;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentClockArch = crate::x86_64::clock::X86_64ClockArch;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentClockArch = crate::arm64::clock::AArch64ClockArch;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentClockArch = crate::riscv64::clock::Riscv64ClockArch;

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

// ── CurrentPostInitArch type aliases ──
#[cfg(feature = "mock")]
pub type CurrentPostInitArch = crate::post_init::MockPostInitArch;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentPostInitArch = crate::x86_64::post_init::X86_64PostInitArch;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentPostInitArch = crate::arm64::post_init::AArch64PostInitArch;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentPostInitArch = crate::riscv64::post_init::Riscv64PostInitArch;

// ── CurrentMemoryInitArch type aliases ──
#[cfg(feature = "mock")]
pub type CurrentMemoryInitArch = crate::post_init::MockMemoryInitArch;
#[cfg(all(not(feature = "mock"), target_arch = "x86_64"))]
pub type CurrentMemoryInitArch = crate::x86_64::post_init::X86_64MemoryInitArch;
#[cfg(all(not(feature = "mock"), target_arch = "aarch64"))]
pub type CurrentMemoryInitArch = crate::arm64::post_init::AArch64MemoryInitArch;
#[cfg(all(not(feature = "mock"), target_arch = "riscv64"))]
pub type CurrentMemoryInitArch = crate::riscv64::post_init::Riscv64MemoryInitArch;

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
