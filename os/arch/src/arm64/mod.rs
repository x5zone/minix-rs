//! ARM64 (aarch64) architecture implementation

pub mod paging;
pub mod early_console;
pub mod protection;
pub mod trap_entry;
pub mod interrupt;
pub mod clock;
pub mod arch_init;
pub mod proc_arch;
pub mod post_init;

pub use paging::AArch64Paging;
pub use protection::{AArch64Protection, AArch64PrivilegeLevel};
pub use trap_entry::AArch64TrapEntry;
pub use interrupt::AArch64InterruptController;
pub use clock::AArch64ClockArch;
pub use arch_init::AArch64ArchInit;
pub use proc_arch::AArch64ProcArch;
pub use post_init::{AArch64PostInitArch, AArch64MemoryInitArch};