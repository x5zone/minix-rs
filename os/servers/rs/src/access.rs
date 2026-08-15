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

use crate::boot::KernelApi;
use crate::process_table::RProcTable;
use crate::service_slot::{RFlags, ServiceSlot, SysFlags};
use minix_types::{EBUSY, EPERM, Endpoint, RS_DOWN, RS_EDIT, RS_RESTART};

/// Checks whether the caller has root euid.
///
/// C: `caller_is_root` — manager.c:21-34. `getnuid` (PM_GETEPINFO via
/// `getepinfo`, lib/libsys/getepinfo.c:35-44) returns the effective uid; on
/// error it returns a negative errno cast to uid_t, which is never 0 — so the
/// check fails closed. The Rust `Result` makes that explicit.
pub fn caller_is_root(endpoint: Endpoint, sys: &mut dyn KernelApi) -> bool {
    sys.getnuid(endpoint).map(|euid| euid == 0).unwrap_or(false)
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
    caller.control[..caller.nr_control.max(0) as usize]
        .iter()
        .any(|c| c == proc_name)
}

/// Checks whether the caller may execute `call` against the target slot.
///
/// C: `check_call_permission` — manager.c:81-130. `rp` is `None` for calls
/// without a target (`RS_UP`, `RS_SHUTDOWN`, `RS_GETSYSINFO` — request.c:27,
/// 439, 1104); those require root only. `updating` is
/// `RUPDATE_IS_UPDATING()` (const.h:105) — the live-update in-progress flag
/// owned by 16-rs-live-update.md.
pub fn check_call_permission(
    caller: Endpoint,
    call: i32,
    rp: Option<&ServiceSlot>,
    table: &RProcTable,
    updating: bool,
    sys: &mut dyn KernelApi,
) -> Result<(), i32> {
    // Caller should be either root or have control privileges (manager.c:91-97).
    let call_allowed = caller_is_root(caller, sys)
        || rp.is_some_and(|target| caller_can_control(caller, target, table));
    if !call_allowed {
        return Err(EPERM);
    }

    if let Some(rp) = rp {
        // Only allow RS_EDIT if the target is a user process (manager.c:103-105).
        if !rp.priv_.is_sys_proc() && call != RS_EDIT {
            return Err(EPERM);
        }

        // Disallow the call if an update is in progress (manager.c:108-110).
        if updating {
            return Err(EBUSY);
        }

        // Disallow if another call is in progress for the service
        // (manager.c:113-116).
        if rp.flags.contains(RFlags::LATEREPLY) || rp.flags.contains(RFlags::INITIALIZING) {
            return Err(EBUSY);
        }

        // Only allow RS_DOWN and RS_RESTART if the service has terminated
        // (manager.c:119-121).
        if rp.flags.contains(RFlags::TERMINATED) && call != RS_DOWN && call != RS_RESTART {
            return Err(EPERM);
        }

        // Disallow RS_DOWN for core system services (manager.c:124-126).
        if rp.pub_.sys_flags.contains(SysFlags::CORE_SRV) && call == RS_DOWN {
            return Err(EPERM);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privilege::{PrivFlags, Privilege};
    use crate::service_slot::{Label, PublicSlot, SlotId};
    use alloc::vec::Vec;
    use minix_types::RS_UP;

    /// Minimal KernelApi recording getnuid results.
    struct MockSys {
        uid: Result<u32, i32>,
        calls: Vec<Endpoint>,
    }

    impl KernelApi for MockSys {
        fn get_machine(&mut self) -> Result<crate::boot::Machine, i32> {
            unimplemented!()
        }
        fn get_hz(&mut self) -> Result<u32, i32> {
            unimplemented!()
        }
        fn privctl(
            &mut self,
            _proc: Endpoint,
            _op: crate::privilege::PrivCtlOp,
            _priv_: Option<&Privilege>,
        ) -> Result<(), i32> {
            unimplemented!()
        }
        fn getpriv(&mut self, _proc: Endpoint) -> Result<Privilege, i32> {
            unimplemented!()
        }
        fn sched_init_proc(&mut self, _proc: Endpoint) -> Result<(), i32> {
            unimplemented!()
        }
        fn getnuid(&mut self, proc: Endpoint) -> Result<u32, i32> {
            self.calls.push(proc);
            self.uid
        }
        fn getnpid(&mut self, _proc: Endpoint) -> Result<i32, i32> {
            unimplemented!()
        }
        fn setalarm(&mut self, _delay_ticks: u32) -> Result<(), i32> {
            unimplemented!()
        }
    }

    /// A table with caller (VFS) and target (TTY) slots, via the public API.
    fn table() -> (RProcTable, SlotId, SlotId) {
        use crate::table::{BootImageDev, BootImagePriv, BootImageSys};
        let mut t = RProcTable::new();
        let cid = SlotId::new(0);
        let tid = SlotId::new(1);
        let boot = |ep: Endpoint, name: &'static str| BootImagePriv {
            endpoint: ep,
            label: name,
            flags: 0,
        };
        let sys = BootImageSys {
            endpoint: Endpoint::NONE,
            flags: 0,
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
        )
        .unwrap();
        (t, cid, tid)
    }

    #[test]
    fn test_caller_is_root() {
        let mut sys = MockSys {
            uid: Ok(0),
            calls: Vec::new(),
        };
        assert!(caller_is_root(Endpoint::PM, &mut sys));
        let mut sys = MockSys {
            uid: Ok(1000),
            calls: Vec::new(),
        };
        assert!(!caller_is_root(Endpoint::PM, &mut sys));
        // getnuid failure → fail closed (negative errno cast to uid_t, never 0).
        let mut sys = MockSys {
            uid: Err(1),
            calls: Vec::new(),
        };
        assert!(!caller_is_root(Endpoint::PM, &mut sys));
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
    fn test_check_call_permission_root() {
        let (t, _, tid) = table();
        let mut sys = MockSys {
            uid: Ok(0),
            calls: Vec::new(),
        };
        // Root with no target (RS_UP) → OK.
        assert!(check_call_permission(Endpoint::PM, RS_UP, None, &t, false, &mut sys).is_ok());
        // Non-root with no target → EPERM.
        let mut sys = MockSys {
            uid: Ok(1),
            calls: Vec::new(),
        };
        assert_eq!(
            check_call_permission(Endpoint::PM, RS_UP, None, &t, false, &mut sys),
            Err(EPERM)
        );
    }

    #[test]
    fn test_check_call_permission_target_rules() {
        let (mut t, _, tid) = table();
        let mut sys = MockSys {
            uid: Ok(0),
            calls: Vec::new(),
        };

        // RS_EDIT on a user process is allowed (manager.c:103-105).
        t.get_mut(tid).priv_.flags = PrivFlags::empty(); // not SYS_PROC
        assert!(
            check_call_permission(
                Endpoint::PM,
                RS_EDIT,
                Some(&t.get(tid).clone()),
                &t,
                false,
                &mut sys
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
                &mut sys
            ),
            Err(EPERM)
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
                &mut sys
            ),
            Err(EBUSY)
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
                &mut sys
            ),
            Err(EBUSY)
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
                &mut sys
            ),
            Err(EPERM)
        );
        assert!(
            check_call_permission(
                Endpoint::PM,
                RS_DOWN,
                Some(&t.get(tid).clone()),
                &t,
                false,
                &mut sys
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
                &mut sys
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
                &mut sys
            ),
            Err(EPERM)
        );
        // Non-DOWN call on core service → OK (manager.c:124-126 only blocks DOWN).
        assert!(
            check_call_permission(
                Endpoint::PM,
                RS_RESTART,
                Some(&t.get(tid).clone()),
                &t,
                false,
                &mut sys
            )
            .is_ok()
        );
    }
}
