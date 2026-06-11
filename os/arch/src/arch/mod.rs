//! ISA mechanism abstractions (architecture-independent hardware mechanisms).
//!
//! These modules define traits and generic data structures that describe
//! "what" the CPU must do (paging, protection, exceptions, interrupts, etc.),
//! not "how" a specific SoC does it.

pub mod paging;
pub mod paging_ext;
pub mod pt_alloc;
pub mod direct_map;
pub mod protection;
pub mod trap_entry;
pub mod exception;
pub mod irq_manager;
pub mod exception_dispatcher;
pub mod clock;
pub mod arch_init;
pub mod proc_arch;
pub mod post_init;
