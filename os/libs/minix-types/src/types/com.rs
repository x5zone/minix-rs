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

use super::endpoint::Endpoint;

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

/// PM/RS → scheduler: take over scheduling a process (fresh start).
/// C: `SCHEDULING_START` — com.h:804.
pub const SCHEDULING_START: i32 = SCHEDULING_BASE + 2;
/// PM → scheduler: stop scheduling a process (exit path).
/// C: `SCHEDULING_STOP` — com.h:805.
pub const SCHEDULING_STOP: i32 = SCHEDULING_BASE + 3;
/// PM → scheduler: change a process's nice (priority) value.
/// C: `SCHEDULING_SET_NICE` — com.h:806.
pub const SCHEDULING_SET_NICE: i32 = SCHEDULING_BASE + 4;
/// PM → scheduler: take over scheduling a forked child (inheritance).
/// C: `SCHEDULING_INHERIT` — com.h:807.
pub const SCHEDULING_INHERIT: i32 = SCHEDULING_BASE + 5;

// ── DS_RQ_BASE + DS_* opcodes ──
//
// C: `<minix/com.h>`:498-507 — the Data Store request codes
// (`minix3/minix/servers/ds/main.c:53-78`). A-1 number-first: the routing
// half of the DS protocol lives here so the DS loop compiles; the payload
// half (`mess_ds_req`/`mess_ds_reply`/`union ds_val`, DSF flags) follows in
// 02-ds-message-contract.md. `DS_SNAPSHOT` (+5) is a dead number (A-7):
// declared in C but cased nowhere, so it has a constant but no letter.

// (The DS block below was added for 01-ds-init-main.md; payload types stay
// out until 02. See plan.md §4/A-1.)

/// Base for Data Store request types. C: `DS_RQ_BASE` — com.h:498.
pub const DS_RQ_BASE: i32 = 0x800;
/// Publish data. C: `DS_PUBLISH` — com.h:500.
#[allow(clippy::identity_op)] // +0 keeps the +0..+7 audit pattern against com.h.
pub const DS_PUBLISH: i32 = DS_RQ_BASE + 0;
/// Retrieve data by name. C: `DS_RETRIEVE` — com.h:501.
pub const DS_RETRIEVE: i32 = DS_RQ_BASE + 1;
/// Subscribe to data updates. C: `DS_SUBSCRIBE` — com.h:502.
pub const DS_SUBSCRIBE: i32 = DS_RQ_BASE + 2;
/// Retrieve updated data. C: `DS_CHECK` — com.h:503.
pub const DS_CHECK: i32 = DS_RQ_BASE + 3;
/// Delete data. C: `DS_DELETE` — com.h:504.
pub const DS_DELETE: i32 = DS_RQ_BASE + 4;
/// Retrieve a label's name. C: `DS_RETRIEVE_LABEL` — com.h:506.
pub const DS_RETRIEVE_LABEL: i32 = DS_RQ_BASE + 6;
/// Get system information. C: `DS_GETSYSINFO` — com.h:507.
pub const DS_GETSYSINFO: i32 = DS_RQ_BASE + 7;

// ── DEVMAN_BASE + DEVMAN_* opcodes ──
//
// C: `<minix/com.h>`:846-866 — the device-manager message codes
// (`minix3/minix/servers/devman/main.c:46-58` dispatches four of them).
// 11-stage-devman/05-devm-message-contract.md owns the protocol story
// (field macros, RS-only gate, fall-through fix); this block is the
// numeric authority. Five codes are declared-but-never-cased anywhere
// in the tree (A-6): they get constants but no dispatch letter.

// (The DEVMAN block below was added for 05-devm-message-contract.md;
// the RS endpoint identity travels with it because the RS-only gate
// needs it and no earlier stage centralized endpoint numbers.)

/// Base for device-manager message types. C: `DEVMAN_BASE` — com.h:846.
pub const DEVMAN_BASE: i32 = 0x1200;
/// Driver registers a device. C: `DEVMAN_ADD_DEV` — com.h:848.
#[allow(clippy::identity_op)] // +0 keeps the +0..+9 audit pattern against com.h.
pub const DEVMAN_ADD_DEV: i32 = DEVMAN_BASE + 0;
/// Driver removes a device. C: `DEVMAN_DEL_DEV` — com.h:849.
pub const DEVMAN_DEL_DEV: i32 = DEVMAN_BASE + 1;
/// Declared but unused tree-wide (A-6). C: `DEVMAN_ADD_BUS` — com.h:850.
pub const DEVMAN_ADD_BUS: i32 = DEVMAN_BASE + 2;
/// Declared but unused tree-wide (A-6). C: `DEVMAN_DEL_BUS` — com.h:851.
pub const DEVMAN_DEL_BUS: i32 = DEVMAN_BASE + 3;
/// Declared but unused tree-wide (A-6). C: `DEVMAN_ADD_DEVFILE` — com.h:852.
pub const DEVMAN_ADD_DEVFILE: i32 = DEVMAN_BASE + 4;
/// Declared but unused tree-wide (A-6). C: `DEVMAN_DEL_DEVFILE` — com.h:853.
pub const DEVMAN_DEL_DEVFILE: i32 = DEVMAN_BASE + 5;
/// Declared but unused tree-wide (A-6). C: `DEVMAN_REQUEST` — com.h:855.
pub const DEVMAN_REQUEST: i32 = DEVMAN_BASE + 6;
/// devman → caller reply. C: `DEVMAN_REPLY` — com.h:856.
pub const DEVMAN_REPLY: i32 = DEVMAN_BASE + 7;
/// RS asks devman to bind a driver. C: `DEVMAN_BIND` — com.h:858.
pub const DEVMAN_BIND: i32 = DEVMAN_BASE + 8;
/// RS asks devman to unbind a driver. C: `DEVMAN_UNBIND` — com.h:859.
pub const DEVMAN_UNBIND: i32 = DEVMAN_BASE + 9;

/// Reincarnation Server endpoint. C: `RS_PROC_NR` — com.h:61.
/// Needed here (not in an RS module) because devman's RS-only gate
/// (bind.c:14,63) compares `m_source` against it.
pub const RS_PROC_NR: Endpoint = Endpoint(2);

// ── DSF_* flags ──
//
// C: `<minix/ds.h>`:12-26 — the Data Store flag set: occupancy, four
// privacy gates, four data types, two request ornaments, and two masks.
// (02-ds-message-contract.md §2.4; the payload half lives in
// `ipc::message::MessDsReq`.)

bitflags::bitflags! {
    /// DS entry flags: occupancy, privacy gates, data types, ornaments.
    ///
    /// C: `DSF_*` — ds.h:12-26. Scattered defines become one named union:
    /// privacy gates guard reads/writes per owner, type bits name the
    /// `DsVal` arm in flight, ornaments modify one call.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DsFlags: u32 {
        /// Entry is in use. C: `DSF_IN_USE` — ds.h:12.
        const IN_USE = 0x001;
        /// Only the owner can retrieve. C: `DSF_PRIV_RETRIEVE` — ds.h:13.
        const PRIV_RETRIEVE = 0x002;
        /// Only the owner can overwrite. C: `DSF_PRIV_OVERWRITE` — ds.h:14.
        const PRIV_OVERWRITE = 0x004;
        /// Only the owner can take a snapshot. C: `DSF_PRIV_SNAPSHOT` —
        /// ds.h:15. Same value as `PRIV_OVERWRITE`: the name is history
        /// (the snapshot road, A-7), the value is the gate.
        const PRIV_SNAPSHOT = 0x004;
        /// Only the owner can subscribe. C: `DSF_PRIV_SUBSCRIBE` — ds.h:16.
        const PRIV_SUBSCRIBE = 0x008;
        /// U32 data. C: `DSF_TYPE_U32` — ds.h:17.
        const TYPE_U32 = 0x010;
        /// String data. C: `DSF_TYPE_STR` — ds.h:18.
        const TYPE_STR = 0x020;
        /// Memory-range data. C: `DSF_TYPE_MEM` — ds.h:19.
        const TYPE_MEM = 0x040;
        /// Label data. C: `DSF_TYPE_LABEL` — ds.h:20.
        const TYPE_LABEL = 0x100;
        /// Overwrite if the entry exists. C: `DSF_OVERWRITE` — ds.h:25.
        const OVERWRITE = 0x01000;
        /// Check subscriptions immediately. C: `DSF_INITIAL` — ds.h:26.
        const INITIAL = 0x02000;
        /// Reserved lane between MEM and LABEL (`0x080`, unnamed in C).
        /// Named so future use has a marker — and present use a warning.
        const _RESERVED_0X80 = 0x080;
    }
}

/// Mask for the type bits. C: `DSF_MASK_TYPE` — ds.h:22.
pub const DSF_MASK_TYPE: u32 = 0xFF0;
/// Mask for the internal bits. C: `DSF_MASK_INTERNAL` — ds.h:23.
pub const DSF_MASK_INTERNAL: u32 = 0xFFF;

/// Maximum key length, including the terminator. C: `DS_MAX_KEYLEN` —
/// ds.h:29.
pub const DS_MAX_KEYLEN: usize = 80;

/// Driver-up event. C: `DS_DRIVER_UP` — ds.h:32.
pub const DS_DRIVER_UP: i32 = 1;

// ── MIB_BASE + MIB_* opcodes ──
//
// C: `<minix/com.h>`:1022-1030 — the MIB request codes
// (`minix3/minix/servers/mib/main.c:458-479`). A-1 number-first: the routing
// half of the MIB protocol lives here so the MIB loop compiles; the payload
// half (`mess_lc_mib_sysctl`/`mess_mib_lc_sysctl`/`mess_lsys_mib_register`/
// `mess_mib_lsys_call`/`mess_mib_lsys_info`, `sysctlnode`/`sysctldesc`,
// CTL_*/CTLTYPE_*/CTLFLAG_* tables) follows in 02-mib-message-contract.md.
// Unlike DS (seven letters + one dead number), MIB has exactly three
// letters and no dead numbers: `NR_MIB_CALLS` is 3.

// (The MIB block below was added for 01-mib-init-main.md; payload types stay
// out until 02. See plan.md §4/A-1.)

/// Base for MIB request types. C: `MIB_BASE` — com.h:1022.
pub const MIB_BASE: i32 = 0x1800;
/// sysctl(2) request. C: `MIB_SYSCTL` — com.h:1026.
#[allow(clippy::identity_op)] // +0 keeps the +0..+2 audit pattern against com.h.
pub const MIB_SYSCTL: i32 = MIB_BASE + 0;
/// Mount a remote subtree. C: `MIB_REGISTER` — com.h:1027.
pub const MIB_REGISTER: i32 = MIB_BASE + 1;
/// Unmount a remote subtree. C: `MIB_DEREGISTER` — com.h:1028.
pub const MIB_DEREGISTER: i32 = MIB_BASE + 2;
/// Highest number from base plus one. C: `NR_MIB_CALLS` — com.h:1030.
pub const NR_MIB_CALLS: i32 = 3;

// ── COMMON_MIB_* opcodes ──
//
// C: `<minix/com.h>`:613-622 — the shared request/reply numbers MIB uses
// with remote services (`COMMON_RQ_BASE` 0xE00 :597, `COMMON_RS_BASE`
// 0xE80 :598). The one-way discipline (register/deregister never answered,
// remote replies keyed by `req_id`) is 12's domain; the numbers travel here.

// (Added for 02-mib-message-contract.md; behaviour in 12/22.)

/// MIB information request for a registered subtree root.
/// C: `COMMON_MIB_INFO` — com.h:613.
pub const COMMON_MIB_INFO: i32 = crate::ipc::COMMON_RQ_BASE + 4;
/// MIB sysctl request on a registered subtree.
/// C: `COMMON_MIB_CALL` — com.h:616.
pub const COMMON_MIB_CALL: i32 = crate::ipc::COMMON_RQ_BASE + 5;
/// Reply to a MIB information or sysctl request.
/// C: `COMMON_MIB_REPLY` — com.h:622.
pub const COMMON_MIB_REPLY: i32 = crate::ipc::COMMON_RS_BASE + 1;

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

    #[test]
    fn test_sched_messages() {
        // C: com.h:801-807.
        assert_eq!(SCHEDULING_BASE, 0xF00);
        assert_eq!(SCHEDULING_NO_QUANTUM, 0xF01);
        assert_eq!(SCHEDULING_START, 0xF02);
        assert_eq!(SCHEDULING_STOP, 0xF03);
        assert_eq!(SCHEDULING_SET_NICE, 0xF04);
        assert_eq!(SCHEDULING_INHERIT, 0xF05);
    }

    #[test]
    fn test_mib_messages() {
        // C: com.h:1022-1030. Three letters, no dead numbers (NR_MIB_CALLS=3).
        assert_eq!(MIB_BASE, 0x1800);
        assert_eq!(MIB_SYSCTL, 0x1800);
        assert_eq!(MIB_REGISTER, 0x1801);
        assert_eq!(MIB_DEREGISTER, 0x1802);
        assert_eq!(NR_MIB_CALLS, 3);
    }

    #[test]
    fn test_common_mib_messages() {
        // C: com.h:613-622 (bases live in ipc::event).
        assert_eq!(COMMON_MIB_INFO, 0xE04);
        assert_eq!(COMMON_MIB_CALL, 0xE05);
        assert_eq!(COMMON_MIB_REPLY, 0xE81);
    }

    #[test]
    fn test_ds_messages() {
        // C: com.h:498-507. +5 (DS_SNAPSHOT) is dead (A-7): no constant.
        assert_eq!(DS_RQ_BASE, 0x800);
        assert_eq!(DS_PUBLISH, 0x800);
        assert_eq!(DS_RETRIEVE, 0x801);
        assert_eq!(DS_SUBSCRIBE, 0x802);
        assert_eq!(DS_CHECK, 0x803);
        assert_eq!(DS_DELETE, 0x804);
        assert_eq!(DS_RETRIEVE_LABEL, 0x806);
        assert_eq!(DS_GETSYSINFO, 0x807);
    }

    #[test]
    fn test_devman_messages() {
        // C: com.h:846-866 — the full set, bit for bit (05-devm-message-contract).
        assert_eq!(DEVMAN_BASE, 0x1200);
        assert_eq!(DEVMAN_ADD_DEV, 0x1200);
        assert_eq!(DEVMAN_DEL_DEV, 0x1201);
        assert_eq!(DEVMAN_ADD_BUS, 0x1202);
        assert_eq!(DEVMAN_DEL_BUS, 0x1203);
        assert_eq!(DEVMAN_ADD_DEVFILE, 0x1204);
        assert_eq!(DEVMAN_DEL_DEVFILE, 0x1205);
        assert_eq!(DEVMAN_REQUEST, 0x1206);
        assert_eq!(DEVMAN_REPLY, 0x1207);
        assert_eq!(DEVMAN_BIND, 0x1208);
        assert_eq!(DEVMAN_UNBIND, 0x1209);
        // C: com.h:61.
        assert_eq!(RS_PROC_NR, Endpoint(2));
    }

    #[test]
    fn test_ds_flags() {
        // C: ds.h:12-26 — the full set, bit for bit.
        assert_eq!(DsFlags::IN_USE.bits(), 0x001);
        assert_eq!(DsFlags::PRIV_RETRIEVE.bits(), 0x002);
        assert_eq!(DsFlags::PRIV_OVERWRITE.bits(), 0x004);
        // Same value, kept name: history (A-7), not a second gate.
        assert_eq!(DsFlags::PRIV_SNAPSHOT.bits(), 0x004);
        assert_eq!(DsFlags::PRIV_SUBSCRIBE.bits(), 0x008);
        assert_eq!(DsFlags::TYPE_U32.bits(), 0x010);
        assert_eq!(DsFlags::TYPE_STR.bits(), 0x020);
        assert_eq!(DsFlags::TYPE_MEM.bits(), 0x040);
        assert_eq!(DsFlags::TYPE_LABEL.bits(), 0x100);
        assert_eq!(DsFlags::OVERWRITE.bits(), 0x01000);
        assert_eq!(DsFlags::INITIAL.bits(), 0x02000);
        assert_eq!(DSF_MASK_TYPE, 0xFF0);
        assert_eq!(DSF_MASK_INTERNAL, 0xFFF);
        // The type mask covers all four type bits, and nothing else below.
        let types = DsFlags::TYPE_U32 | DsFlags::TYPE_STR | DsFlags::TYPE_MEM | DsFlags::TYPE_LABEL;
        assert_eq!(types.bits() & DSF_MASK_TYPE, types.bits());
        assert_eq!(DS_MAX_KEYLEN, 80);
        assert_eq!(DS_DRIVER_UP, 1);
    }
}
