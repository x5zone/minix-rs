//! ISA mechanism abstractions (architecture-independent hardware mechanisms).
//!
//! These modules define traits and generic data structures that describe
//! "what" the CPU must do (paging, protection, exceptions, interrupts, etc.),
//! not "how" a specific SoC does it.

pub mod paging;
pub mod paging_ext;
pub mod pt_alloc;
pub mod direct_map;
pub mod pte_walk_arch;
pub mod protection;
pub mod trap_entry;
pub mod exception;
pub mod exception_dispatcher;
pub mod clock;
pub mod fpu_arch;
pub mod signal_context;
pub mod smp;
pub mod arch_init;
pub mod arch_boot;
pub mod boot;
pub mod post_init;
pub mod stacktrace;
pub mod tlb_arch;
// proc_arch was removed in 06-proc-init-boot-proc.md; its responsibilities
// are now split between `boot` (CpuContextArch + load_vm_elf) and
// the kernel layer (KProcess::cpu_context field).
