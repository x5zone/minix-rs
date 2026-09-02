//! ARM64 (aarch64) architecture implementation

pub mod paging;
pub mod protection;
pub mod trap_entry;
pub mod exception;
pub mod clock;
pub mod timer_irq_gate;
pub mod fpu;
pub mod signal;
pub mod smp;
pub mod arch_init;
pub mod boot;
pub mod tlb;

pub use paging::AArch64Paging;
pub use protection::{AArch64Protection, AArch64PrivilegeLevel};
pub use trap_entry::AArch64TrapEntry;
pub use exception::AArch64ExceptionFrame;
pub use clock::AArch64ClockArch;
pub use timer_irq_gate::AArch64TimerIrqGate;
pub use fpu::{AArch64FpuArch, AArch64FpuState};
pub use signal::{AArch64SignalContext, AArch64SigContext, AArch64SigFrame};
pub use smp::AArch64SmpArch;
pub use arch_init::AArch64ArchInit;
pub use boot::{AArch64CpuContext, AArch64CpuContextArch};