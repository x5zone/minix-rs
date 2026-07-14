//! ARM64 (aarch64) architecture implementation

pub mod paging;
pub mod protection;
pub mod trap_entry;
pub mod exception;
pub mod clock;
pub mod arch_init;
pub mod boot;
pub mod post_init;

pub use paging::AArch64Paging;
pub use protection::{AArch64Protection, AArch64PrivilegeLevel};
pub use trap_entry::AArch64TrapEntry;
pub use exception::AArch64ExceptionFrame;
pub use clock::AArch64ClockArch;
pub use arch_init::AArch64ArchInit;
pub use boot::{AArch64CpuContext, AArch64CpuContextArch};
pub use post_init::{AArch64PostInitArch, AArch64MemoryInitArch};