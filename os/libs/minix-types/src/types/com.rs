//! System-level constant definitions.
//!
//! Corresponds to Minix3's `<minix/com.h>`.
//!
//! # Notes
//!
//! These constants are system-level global configuration, shared by multiple modules:
//! - `MAX_NR_TASKS`: Maximum number of tasks (kernel tasks).
//! - `NR_PROCS`: Maximum number of processes.
//! - `LAST_FEW`: Slots reserved for root.
//!
//! These constants are placed in a separate `com` module because they:
//! 1. Are system-level configuration, not belonging to a specific type.
//! 2. Are shared by multiple modules (pid, endpoint, etc.).
//! 3. Correspond to Minix3's `com.h` header file.

/// Maximum number of tasks (kernel tasks).
///
/// Corresponds to Minix3's `MAX_NR_TASKS` (defined in `com.h`).
///
/// # Notes
///
/// This is the maximum number of tasks (kernel tasks) supported by the system, value is 1023.
/// User process count is defined by `NR_PROCS`.
pub const MAX_NR_TASKS: usize = 1023;

/// Maximum number of processes.
///
/// Corresponds to Minix3's `NR_PROCS` (defined in `config.h`).
///
/// # Notes
///
/// This is the maximum number of user processes supported by the system.
/// Note: Actual available process slots are also limited by `MAX_NR_PROCS`
/// (determined by endpoint generation mechanism).
pub const NR_PROCS: usize = 256;

/// Slots reserved for root.
///
/// Corresponds to Minix3's `LAST_FEW`.
///
/// # Notes
///
/// The last few process slots are reserved for the root user,
/// preventing regular users from exhausting all process slots.
pub const LAST_FEW: usize = 5;

/// Actual number of tasks (tasks initialized at boot).
///
/// Corresponds to Minix3's `NR_TASKS`.
pub const NR_TASKS: usize = 5;

/// Last special process number.
///
/// Corresponds to Minix3's `LAST_SPECIAL_PROC_NR` (init process).
pub const LAST_SPECIAL_PROC_NR: usize = 11;

/// Number of boot modules.
///
/// Corresponds to Minix3's `NR_BOOT_MODULES` = `INIT_PROC_NR + 1`.
pub const NR_BOOT_MODULES: usize = LAST_SPECIAL_PROC_NR + 1;

// ── Scheduling message types ──
//
// C: `<minix/com.h>`:800-806 — `SCHEDULING_BASE = 0xF00`.
// These are message `m_type` values used between the kernel and the
// user-space scheduler (the `sched` system process).

/// Base for scheduling message types. C: `SCHEDULING_BASE` — com.h:801.
pub const SCHEDULING_BASE: i32 = 0xF00;

/// Kernel → scheduler: a user-scheduled process exhausted its quantum.
/// C: `SCHEDULING_NO_QUANTUM` — com.h:803. Payload: `MessKrnLsysSchedule`.
pub const SCHEDULING_NO_QUANTUM: i32 = SCHEDULING_BASE + 1;

