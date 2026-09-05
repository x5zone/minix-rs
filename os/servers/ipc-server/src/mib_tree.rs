//! The `kern.ipc` remote MIB subtree: static description and query routing.
//!
//! The IPC server mounts a small subtree under `kern.ipc` so that `ipcs(1)`
//! and sysctl(2) can list semaphore and shared-memory objects. The branch
//! is registered with the MIB service at startup; the data and the handler
//! stay in this server (a "remote subtree": the mount point lives elsewhere,
//! the content lives here).
//!
//! C: `kern_ipc_table` / `kern_ipc_node` / `kern_ipc_info`
//! (main.c:27-79), `sef_cb_init_fresh` (main.c:80-95),
//! `sys/sys/sysctl.h:275,684-705` (identifier constants).
//! Document `03-ipc-mib-registration.md` §3 (decisions D1-D6).
//!
//! Only judgement lives here (which child, which branch). The information
//! assembly (`get_sem_mib_info`, `get_shm_mib_info`) belongs to documents
//! 05/08; the slot table is reused from `minix-sys` (`rmib::MountTable`).

use minix_types::{CTL_KERN, EINVAL, EOPNOTSUPP};

// ============================================================================
// Identifier constants
// ============================================================================
// C: sys/sys/sysctl.h:275 (KERN_SYSVIPC) and :684-705 (subtypes).

/// SysV IPC node under CTL_KERN. C: `KERN_SYSVIPC 82` — sysctl.h:275.
pub const KERN_SYSVIPC: i32 = 82;

/// Function node: information query entry. C: `KERN_SYSVIPC_INFO 1`.
pub const KERN_SYSVIPC_INFO: i32 = 1;

/// Integer node: message-queue support (always zero — no SysV message
/// queues in Minix3). C: `KERN_SYSVIPC_MSG 2`.
pub const KERN_SYSVIPC_MSG: i32 = 2;

/// Integer node: semaphore support (always one). C: `KERN_SYSVIPC_SEM 3`.
pub const KERN_SYSVIPC_SEM: i32 = 3;

/// Integer node: shared-memory support (always one). C: `KERN_SYSVIPC_SHM 4`.
pub const KERN_SYSVIPC_SHM: i32 = 4;

/// Query subtype: semaphore information. C: `KERN_SYSVIPC_SEM_INFO 5`.
pub const KERN_SYSVIPC_SEM_INFO: i32 = 5;

/// Query subtype: shared-memory information. C: `KERN_SYSVIPC_SHM_INFO 6`.
pub const KERN_SYSVIPC_SHM_INFO: i32 = 6;

/// Reserved slots: present as commented-out rows in C, absent as nodes.
/// C: `KERN_SYSVIPC_SHMMAX..KERN_SYSVIPC_SHMUSEPHYS` (5..9) —
/// main.c:64-68 ("not yet supported").
///
/// Numbering overlap to be aware of: the *information-query* subtypes reuse
/// the values 5 and 6 (`KERN_SYSVIPC_SEM_INFO 5`, `KERN_SYSVIPC_SHM_INFO 6`
/// — sysctl.h:703-704) for a different purpose (the extra name component
/// under the INFO function node). So as *direct children* 5..9 are absent,
/// but as *query values* 5 and 6 route to the two assemblers. Only 7..9 are
/// unsupported in both senses.
///
/// [ARCH: IPC-03-01] The five slots are intentionally absent as nodes:
/// exposing them would promise information this server cannot supply.
pub const RESERVED_SLOT_FIRST: i32 = 5;
/// Last reserved slot. C: `KERN_SYSVIPC_SHMUSEPHYS 9` — sysctl.h:694.
pub const RESERVED_SLOT_LAST: i32 = 9;

// ============================================================================
// Static subtree description
// ============================================================================

/// One effective child of the `kern.ipc` node.
///
/// C: `kern_ipc_table[]` — main.c:54-70. Four effective rows: one function
/// node plus three integer nodes. The five reserved slots are not children
/// (see `RESERVED_SLOT_FIRST`), so they have no variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernIpcChild {
    /// Information query entry (routes to `route_info_query`).
    /// C: `[KERN_SYSVIPC_INFO]` function node — main.c:55-57.
    InfoFunction,
    /// Message-queue support flag, fixed zero. C: `[KERN_SYSVIPC_MSG]` — main.c:58-59.
    MsgZero,
    /// Semaphore support flag, fixed one. C: `[KERN_SYSVIPC_SEM]` — main.c:60-61.
    SemOne,
    /// Shared-memory support flag, fixed one. C: `[KERN_SYSVIPC_SHM]` — main.c:62-63.
    ShmOne,
}

impl KernIpcChild {
    /// The fixed integer carried by integer nodes (`None` for the function node).
    pub const fn fixed_value(self) -> Option<i32> {
        match self {
            Self::InfoFunction => None,
            Self::MsgZero => Some(0),
            Self::SemOne => Some(1),
            Self::ShmOne => Some(1),
        }
    }
}

/// The effective children of `kern.ipc`, in C table order.
///
/// C: `kern_ipc_table[]` — main.c:54-70 (four effective rows).
pub const KERN_IPC_TABLE: [KernIpcChild; 4] = [
    KernIpcChild::InfoFunction,
    KernIpcChild::MsgZero,
    KernIpcChild::SemOne,
    KernIpcChild::ShmOne,
];

/// Mount path of the subtree: `kern.ipc`.
///
/// C: `const int mib[] = { CTL_KERN, KERN_SYSVIPC }` — main.c:82.
pub const MOUNT_PATH: [i32; 2] = [CTL_KERN, KERN_SYSVIPC];

/// Length of the mount path in components.
pub const MOUNT_PATH_LENGTH: usize = 2;

// ============================================================================
// Query routing
// ============================================================================

/// Where an information query goes.
///
/// C: the `switch` in `kern_ipc_info` (main.c:41-50). Four exits because
/// the length check runs before the value dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoRoute {
    /// Assemble semaphore information (document 05: `get_sem_mib_info`).
    SemInfo,
    /// Assemble shared-memory information (document 08: `get_shm_mib_info`).
    ShmInfo,
    /// Name length is not exactly one component.
    /// C: `call_namelen != 1 → EINVAL` — main.c:31-32.
    BadLength,
    /// Anything else, including the reserved slots.
    /// C: `default → EOPNOTSUPP` — main.c:48-49.
    NotSupported,
}

impl InfoRoute {
    /// Map a route to the errno the C handler returns.
    pub const fn to_errno(self) -> i32 {
        match self {
            Self::SemInfo | Self::ShmInfo => 0,
            Self::BadLength => EINVAL,
            Self::NotSupported => EOPNOTSUPP,
        }
    }
}

/// Route one information query.
///
/// `name_length` is `call_namelen`, `name_value` is `call_name[0]`
/// (main.c:31,41). No caller parameter: listing queries carry no privilege
/// check (NetBSD semantics, main.c:34-40, document 03 §1.4) — the signature
/// expresses that by taking no endpoint.
#[inline]
pub const fn route_info_query(name_length: u32, name_value: i32) -> InfoRoute {
    if name_length != 1 {
        return InfoRoute::BadLength;
    }
    match name_value {
        KERN_SYSVIPC_SEM_INFO => InfoRoute::SemInfo,
        KERN_SYSVIPC_SHM_INFO => InfoRoute::ShmInfo,
        _ => InfoRoute::NotSupported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_four_children() {
        // C: main.c:54-70 — four effective rows (the five reserved slots
        // are commented-out rows, not nodes).
        assert_eq!(KERN_IPC_TABLE.len(), 4);
        assert_eq!(KERN_IPC_TABLE[0], KernIpcChild::InfoFunction);
    }

    #[test]
    fn integer_nodes_carry_expected_values() {
        // C: main.c:58-63 — msg 0 (no message queues), sem 1, shm 1.
        assert_eq!(KernIpcChild::MsgZero.fixed_value(), Some(0));
        assert_eq!(KernIpcChild::SemOne.fixed_value(), Some(1));
        assert_eq!(KernIpcChild::ShmOne.fixed_value(), Some(1));
        assert_eq!(KernIpcChild::InfoFunction.fixed_value(), None);
    }

    #[test]
    fn route_sem_info() {
        // C: main.c:42-43 — one component naming SEM_INFO.
        assert_eq!(
            route_info_query(1, KERN_SYSVIPC_SEM_INFO),
            InfoRoute::SemInfo
        );
        assert_eq!(InfoRoute::SemInfo.to_errno(), 0);
    }

    #[test]
    fn route_shm_info() {
        // C: main.c:45-46 — one component naming SHM_INFO.
        assert_eq!(
            route_info_query(1, KERN_SYSVIPC_SHM_INFO),
            InfoRoute::ShmInfo
        );
        assert_eq!(InfoRoute::ShmInfo.to_errno(), 0);
    }

    #[test]
    fn route_bad_length() {
        // C: main.c:31-32 — length must be exactly one.
        assert_eq!(
            route_info_query(0, KERN_SYSVIPC_SEM_INFO),
            InfoRoute::BadLength
        );
        assert_eq!(
            route_info_query(2, KERN_SYSVIPC_SEM_INFO),
            InfoRoute::BadLength
        );
        assert_eq!(InfoRoute::BadLength.to_errno(), EINVAL);
    }

    #[test]
    fn route_reserved_slots_unsupported() {
        // Numbering overlap (see RESERVED_SLOT_FIRST docs): as direct
        // children 5..9 are absent, but as INFO-query values 5 and 6 are
        // the two valid assemblers. Only 7..9 are unsupported in both
        // senses; the C default arm (main.c:48-49) catches them.
        // (KERN_SYSVIPC_MSG_INFO = 4 is likewise unsupported here: only
        // 5/6 route.)
        for slot in 7..=RESERVED_SLOT_LAST {
            assert_eq!(
                route_info_query(1, slot),
                InfoRoute::NotSupported,
                "slot {slot} must be unsupported"
            );
        }
        assert_eq!(InfoRoute::NotSupported.to_errno(), EOPNOTSUPP);
    }

    #[test]
    fn route_unknown_value_unsupported() {
        assert_eq!(route_info_query(1, 0), InfoRoute::NotSupported);
        assert_eq!(route_info_query(1, 99), InfoRoute::NotSupported);
        assert_eq!(
            route_info_query(1, KERN_SYSVIPC_INFO),
            InfoRoute::NotSupported
        );
    }

    #[test]
    fn mount_path_is_kern_ipc() {
        // C: main.c:82 — { CTL_KERN, KERN_SYSVIPC }.
        assert_eq!(MOUNT_PATH, [CTL_KERN, KERN_SYSVIPC]);
        assert_eq!(MOUNT_PATH_LENGTH, 2);
        assert_eq!(MOUNT_PATH[1], KERN_SYSVIPC);
        assert_eq!(KERN_SYSVIPC, 82);
    }
}
