//! SCHED's view of the kernel boundary: what goes down, what guards it.
//!
//! The kernel implements the far side (`sched_proc`, `os/kernel/src/
//! sched.rs:321`; `do_schedule`, `minix3/minix/kernel/system/do_schedule.c`).
//! This module owns the near side and nothing else: which fields travel,
//! which stay (`-1` keeps), and what the wire order is. Kernel validation
//! is documented here but never duplicated — the kernel judges with its
//! own gates (12 owns the registration; the kernel crate owns the checks).
//!
//! Single-threaded event loop: pure functions, no shared state.

pub mod schedctl;
pub mod schedule;
