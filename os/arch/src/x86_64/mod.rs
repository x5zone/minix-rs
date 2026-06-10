//! x86-64 architecture implementation

pub mod pte;
pub mod paging;
pub mod protection;
pub mod trap_entry;
pub mod interrupt;
pub mod exception;
pub mod early_console;
pub mod clock;
pub mod arch_init;
pub mod proc_arch;
pub mod post_init;

pub use protection::{X86_64Protection, X86PrivilegeLevel};
pub use trap_entry::X86_64TrapEntry;
pub use interrupt::X86_64InterruptController;
pub use exception::X86_64ExceptionFrame;
pub use clock::X86_64ClockArch;
pub use arch_init::X86_64ArchInit;
pub use proc_arch::X86_64ProcArch;
pub use post_init::{X86_64PostInitArch, X86_64MemoryInitArch};
