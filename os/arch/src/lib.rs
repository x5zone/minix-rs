//! Hardware Abstraction Layer
//!
//! Provides cross-architecture hardware mechanism abstractions and trait interfaces.
//! Concrete implementations are provided by each architecture module (mock, x86_64, arm64, riscv64).
//!
//! # Design principles
//!
//! 1. **Distributed definition**: Each feature module defines its own traits (e.g. paging, interrupts, timers)
//! 2. **Centralized implementation**: All traits are implemented within the arch crate
//! 3. **Architecture-independent**: OS code depends only on traits, not on specific hardware
//!
//! # Current support
//!
//! - `mock`: Mock hardware implementation for user-space testing
//! - `x86_64`: x86-64 architecture (not yet implemented)
//! - `arm64`: ARM64 architecture (not yet implemented)
//! - `riscv64`: RISC-V 64-bit architecture (not yet implemented)

#![cfg_attr(not(feature = "mock"), no_std)]

extern crate alloc;

pub mod paging;
pub mod paging_ext;
pub mod pt_alloc;
pub mod direct_map;
pub mod protection;
pub mod trap_entry;
pub mod interrupt;
pub mod exception;
pub mod irq_manager;
pub mod exception_dispatcher;
pub mod clock;
pub mod arch_init;
pub mod proc_arch;
pub mod post_init;
pub mod early_console;
#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "aarch64")]
pub mod arm64;
#[cfg(target_arch = "riscv64")]
pub mod riscv64;

pub use paging_ext::{PagingWithId, HugePages};
pub use direct_map::DirectMapArch;
pub use protection::{ProtectionArch, Privilege, InterruptVector};
pub use trap_entry::TrapEntryArch;
pub use interrupt::{
    InterruptController, IrqVector, IrqId, IrqNotifyId, IrqPolicy, IrqAction,
    NR_IRQ_VECTORS, NR_IRQ_HOOKS,
};
pub use exception::{ExceptionArch, FaultContext, RecoveryPoint};
pub use irq_manager::{IrqManager, IrqError};
pub use exception_dispatcher::{
    ExceptionDispatcher, ExceptionOutcome, ExceptionClass, ExceptionSignal, KernTrapStyle,
};
pub use clock::{ClockArch, ClockState, DEFAULT_HZ};
pub use arch_init::ArchInit;
pub use proc_arch::{ArchProcReset, ArchProcInit, BootProcArch, VmLoadResult};
pub use post_init::{PostInitArch, MemoryInitArch, VmPageTableInfo, FreePdeSlots, MAX_FREE_PDE_SLOTS};
pub use early_console::EarlyConsole;

#[cfg(feature = "mock")]
pub use proc_arch::MockProcArch;

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

// ── CurrentInterruptController type aliases ──
#[cfg(target_arch = "x86_64")]
pub type CurrentInterruptController = crate::x86_64::interrupt::X86_64InterruptController;
#[cfg(target_arch = "aarch64")]
pub type CurrentInterruptController = crate::arm64::interrupt::AArch64InterruptController;
#[cfg(target_arch = "riscv64")]
pub type CurrentInterruptController = crate::riscv64::interrupt::Riscv64InterruptController;

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

// ── CurrentBootProcArch type aliases ──
#[cfg(all(feature = "mock", not(target_arch = "x86_64"), not(target_arch = "aarch64"), not(target_arch = "riscv64")))]
pub type CurrentBootProcArch = MockProcArch;
#[cfg(target_arch = "x86_64")]
pub type CurrentBootProcArch = crate::x86_64::proc_arch::X86_64ProcArch;
#[cfg(target_arch = "aarch64")]
pub type CurrentBootProcArch = crate::arm64::proc_arch::AArch64ProcArch;
#[cfg(target_arch = "riscv64")]
pub type CurrentBootProcArch = crate::riscv64::proc_arch::Riscv64ProcArch;

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

// ── CurrentEarlyConsole type aliases ──
#[cfg(target_arch = "x86_64")]
pub type CurrentEarlyConsole = crate::x86_64::early_console::X86_64EarlyConsole;
#[cfg(target_arch = "aarch64")]
pub type CurrentEarlyConsole = crate::arm64::early_console::AArch64EarlyConsole;
#[cfg(target_arch = "riscv64")]
pub type CurrentEarlyConsole = crate::riscv64::early_console::Riscv64EarlyConsole;
