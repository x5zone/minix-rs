//! Access-control checks for RS request entry points.
//!
//! Mirrors `minix3/minix/servers/rs/manager.c:21-130` —
//! `caller_is_root` (21-34), `caller_can_control` (39-76), and
//! `check_call_permission` (81-130). 04-rs-access-control.md.
//!
//! # Policy summary
//!
//! A caller may drive RS if it is root **or** its isolation policy
//! (`r_control[]`, 08) lists the target service's process name. On top of
//! that, per-call rules apply to the target slot: only RS_EDIT on user
//! processes, no calls while an update or another call is in progress, only
//! RS_DOWN/RS_RESTART on terminated services, and no RS_DOWN for core
//! services.
//!
//! # Kernel boundary (T5)
//!
//! These are **pure decision functions**: the `getnuid` query is performed by
//! the shell (the 19 wiring layer / main-loop dispatch) and its `Result` is
//! passed in. `KernelApi` never appears in this module — the syscall face
//! lives only at the wiring layer (todo §13, T5 — the monitor / functional
//! core pattern of 07-rs-period-heartbeat.md).

use crate::process_table::RProcTable;
use crate::service_slot::{RFlags, ServiceSlot, SysFlags};
use minix_types::{Endpoint, Errno, RS_DOWN, RS_EDIT, RS_RESTART};

/// Checks whether the caller has root euid.
///
/// C: `caller_is_root` — manager.c:21-34. The `getnuid` query
/// (PM_GETEPINFO via `getepinfo`, lib/libsys/getepinfo.c:35-44) is performed
/// by the shell and its `Result` is passed in (T5). C returns a negative
/// errno cast to uid_t on error, which is never 0 — so the check fails
/// closed; the `Result` makes that explicit at the decision site.
pub fn caller_is_root(euid: Result<u32, Errno>) -> bool {
    euid.map(|euid| euid == 0).unwrap_or(false)
}

/// Checks whether the caller's isolation policy lists the target service.
///
/// C: `caller_can_control` — manager.c:39-76. Finds the caller's own slot
/// (first `RS_IN_USE` row whose endpoint matches — manager.c:52-60; Rust uses
/// the `by_endpoint` index, ARCH A-4) and scans its `r_control[]` list
/// (`r_nr_control` entries) for the target's `proc_name` (manager.c:63-68).
pub fn caller_can_control(caller: Endpoint, target: &ServiceSlot, table: &RProcTable) -> bool {
    let Some(caller_slot) = table.endpoint_slot(caller) else {
        return false; // manager.c:61 — caller not in the service table
    };
    let caller = &table.get(caller_slot);
    let proc_name = &target.pub_.proc_name;
    // Fail closed on a corrupt count: `nr_control > RS_NR_CONTROL` would panic
    // on direct indexing; C validates the count at edit time (manager.c:1543-
    // 1556, EINVAL), but this is a pub fn reachable from message handling.
    caller
        .control
        .get(..caller.nr_control.max(0) as usize)
        .is_some_and(|list| list.iter().any(|c| c == proc_name))
}

/// Checks whether the caller may execute `call` against the target slot.
///
/// C: `check_call_permission` — manager.c:81-130. `rp` is `None` for calls
/// without a target (`RS_UP`, `RS_SHUTDOWN`, `RS_GETSYSINFO` — request.c:27,
/// 439, 1104); those require root only. `updating` is
/// `RUPDATE_IS_UPDATING()` (const.h:105) — the live-update in-progress flag
/// owned by 16-rs-live-update.md. `caller_euid` is the shell's `getnuid`
/// result for `caller` (T5 — the decision stays pure; the syscall face is
/// only at the wiring layer).
pub fn check_call_permission(
    caller: Endpoint,
    call: i32,
    rp: Option<&ServiceSlot>,
    table: &RProcTable,
    updating: bool,
    caller_euid: Result<u32, Errno>,
) -> Result<(), Errno> {
    // Caller should be either root or have control privileges (manager.c:91-97).
    let call_allowed = caller_is_root(caller_euid)
        || rp.is_some_and(|target| caller_can_control(caller, target, table));
    if !call_allowed {
        return Err(Errno::EPERM);
    }

    if let Some(rp) = rp {
        // Only allow RS_EDIT if the target is a user process (manager.c:103-105).
        if !rp.priv_.is_sys_proc() && call != RS_EDIT {
            return Err(Errno::EPERM);
        }

        // Disallow the call if an update is in progress (manager.c:108-110).
        if updating {
            return Err(Errno::EBUSY);
        }

        // Disallow if another call is in progress for the service
        // (manager.c:113-116).
        if rp.flags.contains(RFlags::LATEREPLY) || rp.flags.contains(RFlags::INITIALIZING) {
            return Err(Errno::EBUSY);
        }

        // Only allow RS_DOWN and RS_RESTART if the service has terminated
        // (manager.c:119-121).
        if rp.flags.contains(RFlags::TERMINATED) && call != RS_DOWN && call != RS_RESTART {
            return Err(Errno::EPERM);
        }

        // Disallow RS_DOWN for core system services (manager.c:124-126).
        if rp.pub_.sys_flags.contains(SysFlags::CORE_SRV) && call == RS_DOWN {
            return Err(Errno::EPERM);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privilege::{PrivFlags, Privilege};
    use crate::service_slot::{Label, SlotId};
    use minix_types::RS_UP;

    /// A table with caller (VFS) and target (TTY) slots, via the public API.
    fn table() -> (RProcTable, SlotId, SlotId) {
        use crate::table::{BootImageDev, BootImagePriv, BootImageSys};
        let mut t = RProcTable::new();
        let cid = SlotId::new(0);
        let tid = SlotId::new(1);
        let boot = |ep: Endpoint, name: &'static str| BootImagePriv {
            endpoint: ep,
            label: name,
            flags: crate::privilege::PrivFlags::empty(),
        };
        let sys = BootImageSys {
            endpoint: Endpoint::NONE,
            flags: crate::service_slot::SysFlags::empty(),
        };
        let dev = BootImageDev {
            endpoint: Endpoint::NONE,
            dev_nr: 0,
        };
        t.activate_boot_slot(
            cid,
            Endpoint::VFS,
            Label::from_bytes(b"vfs"),
            &boot(Endpoint::VFS, "vfs"),
            &sys,
            &dev,
            Privilege::boot_priv(PrivFlags::SYS_PROC, Endpoint::VFS.slot()),
            0, // ticks (S2)
        )
        .unwrap();
        t.activate_boot_slot(
            tid,
            Endpoint::TTY,
            Label::from_bytes(b"tty"),
            &boot(Endpoint::TTY, "tty"),
            &sys,
            &dev,
            Privilege::boot_priv(PrivFlags::SYS_PROC, Endpoint::TTY.slot()),
            0, // ticks (S2)
        )
        .unwrap();
        (t, cid, tid)
    }

    #[test]
    fn test_caller_is_root() {
        // T5: the decision receives the shell's getnuid result.
        assert!(caller_is_root(Ok(0)));
        assert!(!caller_is_root(Ok(1000)));
        // getnuid failure → fail closed (negative errno cast to uid_t, never 0).
        assert!(!caller_is_root(Err(Errno::from_i32(1))));
    }

    #[test]
    fn test_caller_can_control_policy() {
        let (mut t, cid, tid) = table();
        let target = t.get(tid).clone();
        // No control list → denied.
        assert!(!caller_can_control(Endpoint::VFS, &target, &t));
        // Add TTY to VFS's control list → allowed.
        t.get_mut(cid).control[0] = Label::from_bytes(b"tty");
        t.get_mut(cid).nr_control = 1;
        assert!(caller_can_control(Endpoint::VFS, &target, &t));
        // Unknown caller → denied.
        assert!(!caller_can_control(Endpoint::MIB, &target, &t));
    }

    #[test]
    fn test_caller_can_control_corrupt_count_fails_closed() {
        // D3: a count beyond the array length must deny, not panic.
        let (mut t, cid, tid) = table();
        let target = t.get(tid).clone();
        t.get_mut(cid).control[0] = Label::from_bytes(b"tty");
        t.get_mut(cid).nr_control = crate::service_slot::RS_NR_CONTROL as i32 + 1;
        assert!(!caller_can_control(Endpoint::VFS, &target, &t));
    }

    #[test]
    fn test_check_call_permission_root() {
        let (t, _, _) = table();
        // Root with no target (RS_UP) → OK.
        assert!(check_call_permission(Endpoint::PM, RS_UP, None, &t, false, Ok(0)).is_ok());
        // Non-root with no target → EPERM.
        assert_eq!(
            check_call_permission(Endpoint::PM, RS_UP, None, &t, false, Ok(1)),
            Err(Errno::EPERM)
        );
        // getnuid failure → fail closed at the permission gate too.
        assert_eq!(
            check_call_permission(
                Endpoint::PM,
                RS_UP,
                None,
                &t,
                false,
                Err(Errno::from_i32(1))
            ),
            Err(Errno::EPERM)
        );
    }

    #[test]
    fn test_check_call_permission_target_rules() {
        let (mut t, _, tid) = table();

        // RS_EDIT on a user process is allowed (manager.c:103-105).
        t.get_mut(tid).priv_.flags = PrivFlags::empty(); // not SYS_PROC
        assert!(
            check_call_permission(
                Endpoint::PM,
                RS_EDIT,
                Some(&t.get(tid).clone()),
                &t,
                false,
                Ok(0),
            )
            .is_ok()
        );
        // Other calls on a user process → EPERM.
        assert_eq!(
            check_call_permission(
                Endpoint::PM,
                RS_DOWN,
                Some(&t.get(tid).clone()),
                &t,
                false,
                Ok(0),
            ),
            Err(Errno::EPERM)
        );

        // Update in progress → EBUSY.
        t.get_mut(tid).priv_.flags = PrivFlags::SYS_PROC;
        assert_eq!(
            check_call_permission(
                Endpoint::PM,
                RS_DOWN,
                Some(&t.get(tid).clone()),
                &t,
                true,
                Ok(0),
            ),
            Err(Errno::EBUSY)
        );

        // LATEREPLY → EBUSY.
        t.get_mut(tid).flags |= RFlags::LATEREPLY;
        assert_eq!(
            check_call_permission(
                Endpoint::PM,
                RS_DOWN,
                Some(&t.get(tid).clone()),
                &t,
                false,
                Ok(0),
            ),
            Err(Errno::EBUSY)
        );
        t.get_mut(tid).flags.remove(RFlags::LATEREPLY);

        // TERMINATED → only RS_DOWN/RS_RESTART.
        t.get_mut(tid).flags |= RFlags::TERMINATED;
        assert_eq!(
            check_call_permission(
                Endpoint::PM,
                RS_EDIT,
                Some(&t.get(tid).clone()),
                &t,
                false,
                Ok(0),
            ),
            Err(Errno::EPERM)
        );
        assert!(
            check_call_permission(
                Endpoint::PM,
                RS_DOWN,
                Some(&t.get(tid).clone()),
                &t,
                false,
                Ok(0),
            )
            .is_ok()
        );
        assert!(
            check_call_permission(
                Endpoint::PM,
                RS_RESTART,
                Some(&t.get(tid).clone()),
                &t,
                false,
                Ok(0),
            )
            .is_ok()
        );
        t.get_mut(tid).flags.remove(RFlags::TERMINATED);

        // CORE_SRV → RS_DOWN forbidden.
        t.get_mut(tid).pub_.sys_flags |= SysFlags::CORE_SRV;
        assert_eq!(
            check_call_permission(
                Endpoint::PM,
                RS_DOWN,
                Some(&t.get(tid).clone()),
                &t,
                false,
                Ok(0),
            ),
            Err(Errno::EPERM)
        );
        // Non-DOWN call on core service → OK (manager.c:124-126 only blocks DOWN).
        assert!(
            check_call_permission(
                Endpoint::PM,
                RS_RESTART,
                Some(&t.get(tid).clone()),
                &t,
                false,
                Ok(0),
            )
            .is_ok()
        );
    }
}
