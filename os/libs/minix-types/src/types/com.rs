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

/// Maximum number of system services (server/driver slots).
///
/// Corresponds to Minix3's `NR_SYS_PROCS` (defined in `config.h` as
/// `_NR_SYS_PROCS` — `sys_config.h:9`).
///
/// # Notes
///
/// This is the length of the RS `rproc`/`rprocpub` tables
/// (`minix3/minix/servers/rs/glo.h:33-34`). It is smaller than `NR_PROCS`
/// because only system services (servers and drivers) are registered with RS;
/// ordinary user processes are tracked by PM's `mproc` instead.
pub const NR_SYS_PROCS: usize = 64;

/// Slots reserved for root.
///
/// Corresponds to Minix3's `LAST_FEW` — `servers/pm/forkexit.c:32`
/// (`#define LAST_FEW 2`), a PM-local constant, not a global config header.
///
/// # Notes
///
/// The last few process slots are reserved for the root user,
/// preventing regular users from exhausting all process slots.
pub const LAST_FEW: usize = 2;

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

// ── SYS_STATE_* opcodes ──
//
// C: `<minix/com.h>`:442-446 — the `sys_statectl` request codes used by RS
// for IPC-filter state management (`minix3/minix/servers/rs/utility.c:266,
// 294`). Contract: 19-rs-external-interfaces.md §2.1; dictionary:
// 99-rs-global-concepts.md §2.4.

/// Clear IPC references. C: `SYS_STATE_CLEAR_IPC_REFS` — com.h:442.
pub const SYS_STATE_CLEAR_IPC_REFS: i32 = 1;
/// Set the state map. C: `SYS_STATE_SET_STATE_TABLE` — com.h:443.
pub const SYS_STATE_SET_STATE_TABLE: i32 = 2;
/// Add an IPC blacklist filter. C: `SYS_STATE_ADD_IPC_BL_FILTER` — com.h:444.
pub const SYS_STATE_ADD_IPC_BL_FILTER: i32 = 3;
/// Add an IPC whitelist filter. C: `SYS_STATE_ADD_IPC_WL_FILTER` — com.h:445.
pub const SYS_STATE_ADD_IPC_WL_FILTER: i32 = 4;
/// Clear all IPC filters. C: `SYS_STATE_CLEAR_IPC_FILTERS` — com.h:446.
pub const SYS_STATE_CLEAR_IPC_FILTERS: i32 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sys_state_opcodes() {
        // C: com.h:442-446.
        assert_eq!(SYS_STATE_CLEAR_IPC_REFS, 1);
        assert_eq!(SYS_STATE_SET_STATE_TABLE, 2);
        assert_eq!(SYS_STATE_ADD_IPC_BL_FILTER, 3);
        assert_eq!(SYS_STATE_ADD_IPC_WL_FILTER, 4);
        assert_eq!(SYS_STATE_CLEAR_IPC_FILTERS, 5);
    }
}
