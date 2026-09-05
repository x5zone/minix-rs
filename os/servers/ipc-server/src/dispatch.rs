//! IPC server message classification and dispatch verdicts.
//!
//! Mirrors the dispatch half of C `main` (main.c:234-261): three dedicated
//! branches (kernel notification, PM process event, MIB request) ahead of
//! the `call_vec` table lookup, plus the reply verdict (main.c:264).
//!
//! Everything here is pure judgement — no IPC effects. Effects (receiving,
//! sending, handler calls) live in `server.rs` and documents 05-08.
//! Document `01-ipc-init-main.md` §3 (decisions D2/D3/D4).

use minix_types::{ENOSYS, Endpoint, IpcCall, PROC_EVENT, PROC_EVENT_REPLY, SUSPEND};

/// Process manager endpoint. C: `PM_PROC_NR` — com.h:59.
const PM_ENDPOINT: Endpoint = Endpoint::PM;

/// Management information base endpoint. C: `MIB_PROC_NR` — com.h:66.
const MIB_ENDPOINT: Endpoint = Endpoint::MIB;

/// Where an arrived message goes.
///
/// C: the branch ladder in `main` (main.c:234-261). Five outcomes because
/// "not ours" splits three ways (notification vs process event vs MIB) and
/// "ours" splits two ways (known call vs unknown number).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    /// Kernel notification: log and wait for the next message.
    /// C: `is_ipc_notify(ipc_status)` — main.c:234-238.
    Notify,
    /// Process event from the process manager: handled by `got_proc_event`.
    /// C: `m_source == PM_PROC_NR && m_type == PROC_EVENT` — main.c:241-245.
    /// Document 09 owns the handling; this variant only routes there.
    ProcEvent,
    /// Request from the MIB service: handled by `rmib_process`.
    /// C: `m_source == MIB_PROC_NR` — main.c:248-252.
    /// Document 03 owns the handling; this variant only routes there.
    Mib,
    /// Known call: dispatch to the handler for this call number.
    /// C: `call_vec[call_index](&m)` — main.c:257-259.
    /// Documents 05-08 own the handlers; this variant only names the call.
    Dispatch(IpcCall),
    /// Unknown call number: reply "not implemented".
    /// C: the `else r = ENOSYS` arm — main.c:260-261.
    Unknown,
}

/// Classify one arrived message.
///
/// Order matters and matches C: notification first (a notification has no
/// trustworthy source or type), then process event, then MIB, then the
/// table lookup. Reordering would misroute a notification that happens to
/// carry a known-looking type.
///
/// `is_notify` comes from the kernel receive status (`is_ipc_notify`,
/// com.h:92); `source` is `m_source`; `call_type` is `m_type`.
#[inline]
pub const fn classify(is_notify: bool, source: Endpoint, call_type: i32) -> Incoming {
    if is_notify {
        return Incoming::Notify;
    }
    if source.0 == PM_ENDPOINT.0 && call_type == PROC_EVENT {
        return Incoming::ProcEvent;
    }
    if source.0 == MIB_ENDPOINT.0 {
        return Incoming::Mib;
    }
    match IpcCall::from_raw(call_type) {
        Some(call) => Incoming::Dispatch(call),
        None => Incoming::Unknown,
    }
}

/// Result code for an unknown call number.
///
/// C: `r = ENOSYS` — main.c:261. One name so the rule reads at the call site.
#[inline]
pub const fn unknown_call_result() -> i32 {
    ENOSYS
}

/// Whether the main loop sends a reply for this handler result.
///
/// C: `if (r != SUSPEND)` — main.c:264. SUSPEND ("reply later", com.h:1151)
/// suppresses the reply; every other result — success or errno — is written
/// into the reply type field and sent. See document 01 §1.4: SUSPEND is an
/// agreement, not an error.
#[inline]
pub const fn should_reply(result: i32) -> bool {
    result != SUSPEND
}

/// Reply type used for the process-event acknowledgement.
///
/// C: `m->m_type = PROC_EVENT_REPLY` — main.c:207. Kept here (not in 09)
/// because it is part of the main-loop reply vocabulary, next to SUSPEND.
#[inline]
pub const fn proc_event_reply_type() -> i32 {
    PROC_EVENT_REPLY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_notify_takes_priority() {
        // C: main.c:234 — notification is checked before anything else, even
        // when source and type would otherwise match a dedicated branch.
        assert_eq!(classify(true, Endpoint::PM, PROC_EVENT), Incoming::Notify);
        assert_eq!(classify(true, Endpoint::MIB, PROC_EVENT), Incoming::Notify);
        assert_eq!(
            classify(true, Endpoint(42), minix_types::IPC_SEMGET),
            Incoming::Notify
        );
    }

    #[test]
    fn classify_proc_event_route() {
        // C: main.c:241 — PM source plus PROC_EVENT type.
        assert_eq!(
            classify(false, Endpoint::PM, PROC_EVENT),
            Incoming::ProcEvent
        );
        // Same type from another source is not a process event.
        assert_eq!(classify(false, Endpoint(42), PROC_EVENT), Incoming::Unknown);
        // PM source with another type falls through to the table.
        assert_eq!(
            classify(false, Endpoint::PM, minix_types::IPC_SEMGET),
            Incoming::Dispatch(IpcCall::Semget)
        );
    }

    #[test]
    fn classify_mib_route() {
        // C: main.c:248 — any type from the MIB service goes to rmib_process.
        assert_eq!(
            classify(false, Endpoint::MIB, minix_types::IPC_SEMGET),
            Incoming::Mib
        );
        assert_eq!(classify(false, Endpoint::MIB, 0), Incoming::Mib);
    }

    #[test]
    fn classify_dispatch_each_call() {
        // C: main.c:255-259 — all seven numbers hit their handler slot.
        let calls = [
            (minix_types::IPC_SHMGET, IpcCall::Shmget),
            (minix_types::IPC_SHMAT, IpcCall::Shmat),
            (minix_types::IPC_SHMDT, IpcCall::Shmdt),
            (minix_types::IPC_SHMCTL, IpcCall::Shmctl),
            (minix_types::IPC_SEMGET, IpcCall::Semget),
            (minix_types::IPC_SEMCTL, IpcCall::Semctl),
            (minix_types::IPC_SEMOP, IpcCall::Semop),
        ];
        for (raw, call) in calls {
            assert_eq!(classify(false, Endpoint(42), raw), Incoming::Dispatch(call));
        }
    }

    #[test]
    fn classify_unknown_call() {
        // C: main.c:260-261 — out-of-range numbers reply ENOSYS.
        assert_eq!(classify(false, Endpoint(42), 0xD00), Incoming::Unknown);
        assert_eq!(classify(false, Endpoint(42), 0xD08), Incoming::Unknown);
        assert_eq!(classify(false, Endpoint(42), 0), Incoming::Unknown);
        assert_eq!(unknown_call_result(), ENOSYS);
    }

    #[test]
    fn should_reply_suspend_suppresses() {
        // C: main.c:264 — SUSPEND suppresses; everything else replies.
        assert!(!should_reply(SUSPEND));
        assert!(should_reply(0));
        assert!(should_reply(minix_types::EINVAL));
        assert!(should_reply(ENOSYS));
        assert_eq!(proc_event_reply_type(), PROC_EVENT_REPLY);
    }
}
