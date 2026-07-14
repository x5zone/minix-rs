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
pub use arch::protection;
pub use arch::trap_entry;
pub use arch::exception;
pub use arch::exception_dispatcher;
pub use arch::clock;
pub use arch::arch_init;
pub use arch::arch_boot;
pub use arch::boot;
pub use arch::post_init;

pub use paging_ext::{PagingWithId, HugePages};
pub use direct_map::DirectMapArch;
pub use protection::{ProtectionArch, Privilege, InterruptVector};
pub use trap_entry::TrapEntryArch;
pub use exception::{ExceptionArch, FaultContext, RecoveryPoint};
pub use exception_dispatcher::{
    ExceptionDispatcher, ExceptionOutcome, ExceptionClass, ExceptionSignal, KernTrapStyle,
};
pub use clock::{ClockArch, ClockState, DEFAULT_HZ};
pub use arch_init::ArchInit;
pub use boot::{
    CpuContextArch, EntrySpec, ProcKind, ProcNr,
    VmLoadResult, VmLoadError, load_vm_elf,
};
pub use post_init::{PostInitArch, MemoryInitArch, VmPageTableInfo, FreePdeSlots, MAX_FREE_PDE_SLOTS};

#[cfg(feature = "mock")]
pub use paging::mock::MockPaging;

#[cfg(feature = "mock")]
pub use paging::mock::MockAsid;

// ── CurrentPaging type alias ──
#[cfg(all(feature = "mock", not(target_arch = "x86_64"), not(target_arch = "aarch64"), not(target_arch = "riscv64")))]
pub type CurrentPaging = MockPaging;
#[cfg(target_arch = "x86_64")]
pub type CurrentPaging = crate::x86_64::paging::X86_64Paging;
#[cfg(target_arch = "aarch64")]
pub type CurrentPaging = crate::arm64::paging::AArch64Paging;
#[cfg(target_arch = "riscv64")]
pub type CurrentPaging = crate::riscv64::paging::Riscv64Paging;

#[cfg(feature = "mock")]
pub use direct_map::MockDirectMap;

#[cfg(target_arch = "x86_64")]
pub use direct_map::X86_64DirectMap;

#[cfg(all(feature = "mock", not(target_arch = "x86_64")))]
pub type CurrentDirectMap = MockDirectMap;
#[cfg(target_arch = "x86_64")]
pub type CurrentDirectMap = X86_64DirectMap;

// ── CurrentProtection type aliases ──
#[cfg(target_arch = "x86_64")]
pub type CurrentProtection = crate::x86_64::protection::X86_64Protection;
#[cfg(target_arch = "aarch64")]
pub type CurrentProtection = crate::arm64::protection::AArch64Protection;
#[cfg(target_arch = "riscv64")]
pub type CurrentProtection = crate::riscv64::protection::Riscv64Protection;

// ── CurrentTrapEntry type aliases ──
#[cfg(target_arch = "x86_64")]
pub type CurrentTrapEntry = crate::x86_64::trap_entry::X86_64TrapEntry;
#[cfg(target_arch = "aarch64")]
pub type CurrentTrapEntry = crate::arm64::trap_entry::AArch64TrapEntry;
#[cfg(target_arch = "riscv64")]
pub type CurrentTrapEntry = crate::riscv64::trap_entry::Riscv64TrapEntry;

// ── CurrentClockArch type aliases ──
#[cfg(target_arch = "x86_64")]
pub type CurrentClockArch = crate::x86_64::clock::X86_64ClockArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentClockArch = crate::arm64::clock::AArch64ClockArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentClockArch = crate::riscv64::clock::Riscv64ClockArch;

// ── CurrentArchInit type aliases ──
#[cfg(target_arch = "x86_64")]
pub type CurrentArchInit = crate::x86_64::arch_init::X86_64ArchInit;
#[cfg(target_arch = "aarch64")]
pub type CurrentArchInit = crate::arm64::arch_init::AArch64ArchInit;
#[cfg(target_arch = "riscv64")]
pub type CurrentArchInit = crate::riscv64::arch_init::Riscv64ArchInit;

// ── CurrentCpuContextArch / CpuContext / TrapFrame type aliases ──
//
// Replaces the old `CurrentBootProcArch` (see 06-design-final.md §3.2
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
#[cfg(all(feature = "mock", not(target_arch = "x86_64"), not(target_arch = "aarch64"), not(target_arch = "riscv64")))]
pub type CurrentPostInitArch = crate::post_init::MockPostInitArch;
#[cfg(target_arch = "x86_64")]
pub type CurrentPostInitArch = crate::x86_64::post_init::X86_64PostInitArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentPostInitArch = crate::arm64::post_init::AArch64PostInitArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentPostInitArch = crate::riscv64::post_init::Riscv64PostInitArch;

// ── CurrentMemoryInitArch type aliases ──
#[cfg(all(feature = "mock", not(target_arch = "x86_64"), not(target_arch = "aarch64"), not(target_arch = "riscv64")))]
pub type CurrentMemoryInitArch = crate::post_init::MockMemoryInitArch;
#[cfg(target_arch = "x86_64")]
pub type CurrentMemoryInitArch = crate::x86_64::post_init::X86_64MemoryInitArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentMemoryInitArch = crate::arm64::post_init::AArch64MemoryInitArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentMemoryInitArch = crate::riscv64::post_init::Riscv64MemoryInitArch;
