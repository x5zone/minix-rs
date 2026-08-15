//! RS control requests: the handlers' pure validation and bookkeeping.
//!
//! Mirrors `minix3/minix/servers/rs/request.c:15-457` (`do_up` — 15,
//! `do_down` — 111, `do_restart` — 160, `do_clone` — 208, `do_unclone` —
//! 253, `do_edit` — 298, `do_refresh` — 390, `do_shutdown` — 431) and
//! `manager.c:988-1008` (`stop_service`). 13-rs-control-requests.md.
//!
//! The kernel/IPC-coupled steps (`copy_rs_start`/`copy_label`/`init_slot`/
//! `edit_slot` — 08, `sys_getpriv`/`sched_stop`/`sys_privctl`/`vm_set_priv`/
//! `sched_init_proc` — 19, `start_service` — 12, `cleanup_service`/
//! `restart_service` — 15) are stated as call sites; this module owns the
//! pure flags mapping, duplicate checks, late-reply bookkeeping, the stop
//! primitive and the shutdown sweep.

use crate::process_table::RProcTable;
use crate::service_slot::{RFlags, ServiceSlot, SlotId};
use crate::slot::RssFlags;
use minix_types::{Clock, EBUSY, Endpoint};

/// C: `SEF_INIT_CRASH` — sef.h:98.
pub const SEF_INIT_CRASH: u32 = 0x1;
/// C: `SEF_INIT_FAIL` — sef.h:99.
pub const SEF_INIT_FAIL: u32 = 0x2;
/// C: `SEF_INIT_TIMEOUT` — sef.h:100.
pub const SEF_INIT_TIMEOUT: u32 = 0x4;
/// C: `SEF_INIT_DEFCB` — sef.h:101.
pub const SEF_INIT_DEFCB: u32 = 0x8;

/// Maps the `RSS_FORCE_INIT_*` debugging flags onto SEF init flags.
///
/// C: `do_up` — request.c:50-61. The RSS_* flags (rs.h:42-45) are the
/// request-side hooks; the SEF_INIT_* flags (sef.h:98-101) ride in the
/// RS_INIT message's `flags` field (12-rs-init-run.md §2.1).
pub fn up_init_flags(rss: RssFlags) -> u32 {
    let mut f = 0;
    if rss.contains(RssFlags::FORCE_INIT_CRASH) {
        f |= SEF_INIT_CRASH;
    }
    if rss.contains(RssFlags::FORCE_INIT_FAIL) {
        f |= SEF_INIT_FAIL;
    }
    if rss.contains(RssFlags::FORCE_INIT_TIMEOUT) {
        f |= SEF_INIT_TIMEOUT;
    }
    if rss.contains(RssFlags::FORCE_INIT_DEFCB) {
        f |= SEF_INIT_DEFCB;
    }
    f
}

/// Checks label / device-number / domain uniqueness before `do_up` proceeds.
///
/// C: `do_up` — request.c:70-87. All three checks run before `start_service`
/// (request.c:90) so a duplicate never reaches the creation path.
pub fn check_duplicates(
    table: &RProcTable,
    label: &crate::service_slot::Label,
    dev_nr: u32,
    domains: &[i32],
) -> Result<(), i32> {
    if table
        .lookup_by_label(label.as_str().unwrap_or(""))
        .is_some()
    {
        return Err(EBUSY); // request.c:71-75
    }
    if dev_nr > 0 && table.lookup_by_dev_nr(dev_nr).is_some() {
        return Err(EBUSY); // request.c:76-80
    }
    if domains.iter().any(|d| table.lookup_by_domain(*d).is_some()) {
        return Err(EBUSY); // request.c:81-87
    }
    Ok(())
}

/// Marks a slot for a late reply.
///
/// C: `do_up` — request.c:101-103, `do_down` — request.c:150-152,
/// `do_refresh` — request.c:421-423. The three fields are written together
/// so the 06 `late_reply` consumer (utility.c:332-347) always finds a
/// consistent caller record.
pub fn mark_late_reply(slot: &mut ServiceSlot, caller: Endpoint, request: i32) {
    slot.flags |= RFlags::LATEREPLY;
    slot.caller = caller;
    slot.caller_request = request;
}

/// Signal used by `stop_service`. C: `SIGTERM`/`SIGHUP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopSignal {
    /// C: `SIGTERM` — the friendly stop for ordinary services.
    Term,
    /// C: `SIGHUP` — RS itself uses SIGHUP (manager.c:1003).
    Hangup,
}

impl StopSignal {
    /// The C signal number (libc value; the `sys_kill` wiring lives in 19).
    pub const fn as_i32(self) -> i32 {
        match self {
            StopSignal::Term => 15,  // SIGTERM
            StopSignal::Hangup => 1, // SIGHUP
        }
    }
}

/// Stops a service: records the exit intent and the friendly signal.
///
/// C: `stop_service` — manager.c:988-1008. The `sys_kill` send is injected
/// (19); this function applies the slot-side effects and returns the signal
/// to send. `how` is `RS_EXITING` (do_down) or `RS_REFRESHING` (do_refresh).
pub fn stop_service(table: &mut RProcTable, rp: SlotId, how: RFlags, ticks: Clock) -> StopSignal {
    let slot = table.get_mut(rp);
    // C: manager.c:1003 — RS itself is stopped with SIGHUP (its SEF signal
    // handler treats it as the stop request; 06/18).
    let signal = if slot.pub_.endpoint == Endpoint::RS {
        StopSignal::Hangup
    } else {
        StopSignal::Term
    };
    slot.flags |= how; // manager.c:1005
    slot.stop_tm = ticks; // manager.c:1007
    signal
}

/// Applies the shutdown sweep and reports the new `shutting_down` state.
///
/// C: `do_shutdown` — request.c:431-457. Every in-use slot gains
/// `RS_EXITING` (449-455) so the recovery paths (15) do not restart dead
/// services. Returns `true` (C: `shutting_down = TRUE`, request.c:447).
pub fn shutdown_apply(table: &mut RProcTable) -> bool {
    let ids: alloc::vec::Vec<SlotId> = table.iter_in_use().map(|(id, _)| id).collect();
    for id in ids {
        table.get_mut(id).flags |= RFlags::EXITING;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_slot::Label;
    use minix_types::Endpoint;

    #[test]
    fn test_up_init_flags() {
        // C: request.c:50-61 — RSS_FORCE_INIT_* → SEF_INIT_* (sef.h:98-101).
        let f = up_init_flags(RssFlags::FORCE_INIT_CRASH | RssFlags::FORCE_INIT_TIMEOUT);
        assert_eq!(f, SEF_INIT_CRASH | SEF_INIT_TIMEOUT);
        assert_eq!(up_init_flags(RssFlags::empty()), 0);
        assert_eq!(up_init_flags(RssFlags::FORCE_INIT_DEFCB), SEF_INIT_DEFCB);
    }

    #[test]
    fn test_check_duplicates_label() {
        // C: request.c:71-75 — duplicate label → EBUSY.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        // C: lookup_slot_by_label requires RS_ACTIVE (manager.c:1944-1946).
        t.get_mut(rp).flags |= RFlags::IN_USE | RFlags::ACTIVE;
        t.get_mut(rp).pub_.label = Label::from_bytes(b"vm");
        assert_eq!(
            check_duplicates(&t, &Label::from_bytes(b"vm"), 0, &[]),
            Err(EBUSY)
        );
        assert_eq!(
            check_duplicates(&t, &Label::from_bytes(b"pm"), 0, &[]),
            Ok(())
        );
    }

    #[test]
    fn test_check_duplicates_dev_nr_domain() {
        // C: request.c:76-87 — dev_nr/domain duplicates → EBUSY.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        // C: lookup_slot_by_dev_nr / _by_domain require RS_ACTIVE.
        t.get_mut(rp).flags |= RFlags::IN_USE | RFlags::ACTIVE;
        t.get_mut(rp).pub_.dev_nr = 3;
        // C: lookup_slot_by_domain scans `rpub->nr_domain` entries.
        t.get_mut(rp).pub_.nr_domain = 1;
        t.get_mut(rp).pub_.domain[0] = 42;
        assert_eq!(
            check_duplicates(&t, &Label::from_bytes(b"x"), 3, &[]),
            Err(EBUSY)
        );
        assert_eq!(
            check_duplicates(&t, &Label::from_bytes(b"x"), 0, &[42]),
            Err(EBUSY)
        );
    }

    #[test]
    fn test_mark_late_reply() {
        // C: request.c:101-103 — the three late-reply fields written together.
        let mut s = ServiceSlot::vacant();
        mark_late_reply(&mut s, Endpoint::PM, minix_types::ipc::RS_UP);
        assert!(s.flags.contains(RFlags::LATEREPLY));
        assert_eq!(s.caller, Endpoint::PM);
        assert_eq!(s.caller_request, minix_types::ipc::RS_UP);
    }

    #[test]
    fn test_stop_service_signal_choice() {
        // C: manager.c:1003 — RS → SIGHUP; others → SIGTERM.
        let mut t = RProcTable::new();
        let rp = t.alloc_slot().unwrap();
        let mut s = ServiceSlot::vacant();
        s.pub_.endpoint = Endpoint::VFS;
        *t.get_mut(rp) = s;
        let sig = stop_service(&mut t, rp, RFlags::EXITING, 77);
        assert_eq!(sig, StopSignal::Term);
        assert!(t.get(rp).flags.contains(RFlags::EXITING));
        assert_eq!(t.get(rp).stop_tm, 77);

        let rp2 = t.alloc_slot().unwrap();
        let mut s2 = ServiceSlot::vacant();
        s2.pub_.endpoint = Endpoint::RS;
        *t.get_mut(rp2) = s2;
        let sig2 = stop_service(&mut t, rp2, RFlags::REFRESHING, 1);
        assert_eq!(sig2, StopSignal::Hangup);
        assert!(t.get(rp2).flags.contains(RFlags::REFRESHING));
    }

    #[test]
    fn test_shutdown_apply() {
        // C: request.c:449-455 — every in-use slot gains RS_EXITING.
        let mut t = RProcTable::new();
        let a = t.alloc_slot().unwrap();
        let b = t.alloc_slot().unwrap();
        t.get_mut(a).flags |= RFlags::IN_USE;
        t.get_mut(b).flags |= RFlags::IN_USE | RFlags::ACTIVE;
        let free = t.alloc_slot().unwrap(); // stays vacant
        assert!(shutdown_apply(&mut t));
        assert!(t.get(a).flags.contains(RFlags::EXITING));
        assert!(t.get(b).flags.contains(RFlags::EXITING));
        assert!(!t.get(free).flags.contains(RFlags::EXITING));
    }
}
