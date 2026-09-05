//! Main-loop message classification skeleton.
//!
//! Mirrors the `main()` dispatch structure (`minix3/minix/servers/rs/main.c:
//! 70-127`): four message classes are distinguished before any handler runs.
//! The *mechanisms* of each class belong to their owning docs
//! (01-rs-boot-init.md §1.4); this module only delivers the classifier and
//! the handler-ownership table.
//!
//! ```text
//! | Class            | C branch (main.c)     | Handler        | Doc |
//! |------------------|-----------------------|----------------|-----|
//! | ClockNotify      | case CLOCK (82-84)    | do_period      | 07  |
//! | HeartbeatNotify  | default notify (85-91)| r_alive_tm     | 07  |
//! | InitReady        | case RS_INIT (116)    | do_init_ready  | 12  |
//! | LuPrepareReady   | case RS_LU_PREPARE    | do_upd_ready   | 12/16 |
//! | Request(n)       | RS_* cases (102-114)  | do_up/do_*     | 13/14/16 |
//! ```

use minix_types::{
    Clock, Endpoint, Errno, RS_CLONE, RS_DOWN, RS_EDIT, RS_FI, RS_GETSYSINFO, RS_INIT, RS_LOOKUP,
    RS_LU_PREPARE, RS_REFRESH, RS_RESTART, RS_SHUTDOWN, RS_SYSCTL, RS_UNCLONE, RS_UP, RS_UPDATE,
};

// RS message types are defined in `minix-types::ipc::rs` (ARCH A-2,
// 19-rs-external-interfaces.md / 99-rs-global-concepts.md §3.1 — the unique
// authority); this module imports them.

/// IPC receive status word.
///
/// C: the `ipc_status` output of `sef_receive_status`; `is_ipc_notify` is
/// `IPC_STATUS_CALL(status) == NOTIFY` — `minix3/minix/include/minix/com.h:92`.
/// The full status-word parsing belongs to 06-rs-main-loop.md; this module
/// only needs the notify bit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IpcStatus {
    pub flags: u32,
}

impl IpcStatus {
    /// C: `is_ipc_notify(ipc_status)` — com.h:92.
    pub fn is_notify(&self) -> bool {
        // IPC_STATUS_CALL: low 6 bits — ipcconst.h:21-24.
        let call = self.flags & 0x3F;
        call == 4 // NOTIFY — ipcconst.h:10 (SEND=1, RECEIVE=2, SENDREC=3, NOTIFY=4)
    }
}

/// The four message classes of the RS main loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchKind {
    /// CLOCK notification → `do_period` (07). C: main.c:80-83.
    ClockNotify,
    /// Heartbeat notification from a service (07). C: main.c:85-91 — the
    /// kernel timestamp carried by every notify (ipc.h:1715, `u64_t`) is
    /// what the main loop writes into `r_alive_tm`, so the classification
    /// result carries it (R25, todo §18): a `HeartbeatNotify` is
    /// self-contained and the handler needs no second look at the message.
    /// `timestamp` uses the crate-wide tick type `Clock` (= `i64`, matching
    /// `r_alive_tm`/`SlotMutations::alive_tm`); the u64 wire value converts
    /// once at the receive boundary (19).
    HeartbeatNotify { source: Endpoint, timestamp: Clock },
    /// `RS_INIT` → `do_init_ready` (12). C: main.c:116.
    InitReady,
    /// `RS_LU_PREPARE` → `do_upd_ready` (12/16). C: main.c:117.
    LuPrepareReady,
    /// `RS_*` request → `do_*` (13/14/16). C: main.c:102-114.
    Request(i32),
}

/// Classifies a received message.
///
/// C: `main()` classification — main.c:70-127. `who_p` is the sender's slot
/// (validated by `rs_isokendpt`, 02, before classification); `call_nr` is
/// `m.m_type`; `timestamp` is `m.m_notify.timestamp` (ipc.h:1715 — valid for
/// every notify; the non-notify classes ignore it). R12: the 06 main-loop
/// wiring must run the `isokendpt` gate (main.c:63-66) before classifying —
/// the O(1) endpoint fast index is total (out-of-range endpoints yield
/// `None`) but only the gate rejects them.
pub fn classify(
    ipc_status: &IpcStatus,
    who_p: Endpoint,
    call_nr: i32,
    timestamp: Clock,
) -> DispatchKind {
    if ipc_status.is_notify() {
        if who_p == Endpoint::CLOCK {
            return DispatchKind::ClockNotify;
        }
        return DispatchKind::HeartbeatNotify {
            source: who_p,
            timestamp,
        };
    }
    match call_nr {
        RS_INIT => DispatchKind::InitReady,
        RS_LU_PREPARE => DispatchKind::LuPrepareReady,
        n => DispatchKind::Request(n),
    }
}

/// Result of dispatching one request.
///
/// C: `result` in `main()` — the value replied to the caller (`m.m_type =
/// result` — main.c:126). Errors are errno values; `EDONTREPLY` suppresses
/// the reply (main.c:124-129, 06-rs-main-loop.md §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchResult(pub i32);

impl DispatchResult {
    /// Whether the handler suppressed the reply.
    ///
    /// C: `if (result != EDONTREPLY) { ... reply(...); }` — main.c:124-129.
    pub const fn is_reply_suppressed(&self) -> bool {
        self.0 == minix_types::EDONTREPLY
    }
}

/// Dispatches an `RS_*` request by call number.
///
/// C: the `switch(call_nr)` — main.c:102-114. Each arm's handler ownership is
/// annotated; handlers land with their owning docs. Until then, all requests
/// fail closed with `ENOSYS`, matching the C `default` branch — main.c:118-121.
pub fn dispatch_request(call_nr: i32) -> DispatchResult {
    match call_nr {
        // → 13-rs-control-requests.md
        RS_UP | RS_DOWN | RS_REFRESH | RS_RESTART | RS_SHUTDOWN | RS_CLONE | RS_UNCLONE
        | RS_EDIT => DispatchResult(Errno::ENOSYS.to_i32()),
        // → 16-rs-live-update.md
        RS_UPDATE => DispatchResult(Errno::ENOSYS.to_i32()),
        // → 14-rs-query-requests.md
        RS_SYSCTL | RS_FI | RS_GETSYSINFO | RS_LOOKUP => DispatchResult(Errno::ENOSYS.to_i32()),
        // Unknown request → ENOSYS (C default, main.c:118-121).
        _ => DispatchResult(Errno::ENOSYS.to_i32()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_clock_notify() {
        let st = IpcStatus { flags: 4 }; // NOTIFY (ipcconst.h:10)
        assert_eq!(
            classify(&st, Endpoint::CLOCK, 0, 0),
            DispatchKind::ClockNotify
        );
    }

    #[test]
    fn test_classify_heartbeat_notify() {
        let st = IpcStatus { flags: 4 };
        assert_eq!(
            classify(&st, Endpoint::VFS, 0, 0),
            DispatchKind::HeartbeatNotify {
                source: Endpoint::VFS,
                timestamp: 0
            }
        );
    }

    #[test]
    fn test_classify_heartbeat_carries_timestamp() {
        // R25: the notify's kernel timestamp (ipc.h:1715) must survive
        // classification — main.c:87 writes it into `r_alive_tm` verbatim,
        // so the classifier loses nothing on the way (main.c:85-91).
        let st = IpcStatus { flags: 4 };
        assert_eq!(
            classify(&st, Endpoint::VFS, 0, 777),
            DispatchKind::HeartbeatNotify {
                source: Endpoint::VFS,
                timestamp: 777
            }
        );
    }

    #[test]
    fn test_classify_ready() {
        let st = IpcStatus { flags: 0 };
        assert_eq!(
            classify(&st, Endpoint::RS, RS_INIT, 0),
            DispatchKind::InitReady
        );
        assert_eq!(
            classify(&st, Endpoint::RS, RS_LU_PREPARE, 0),
            DispatchKind::LuPrepareReady
        );
    }

    #[test]
    fn test_classify_request() {
        let st = IpcStatus { flags: 0 };
        assert_eq!(
            classify(&st, Endpoint::PM, RS_UP, 0),
            DispatchKind::Request(RS_UP)
        );
    }

    #[test]
    fn test_classify_request_unknown() {
        let st = IpcStatus { flags: 0 };
        let kind = classify(&st, Endpoint::PM, 9999, 0);
        match kind {
            DispatchKind::Request(n) => {
                assert_eq!(dispatch_request(n), DispatchResult(Errno::ENOSYS.to_i32()))
            }
            other => panic!("expected Request, got {other:?}"),
        }
    }

    #[test]
    fn test_dispatch_result_reply_suppression() {
        // C: main.c:124-129 — EDONTREPLY skips the reply path.
        assert!(DispatchResult(minix_types::EDONTREPLY).is_reply_suppressed());
        assert!(!DispatchResult(minix_types::ENOSYS).is_reply_suppressed());
    }

    #[test]
    fn test_rs_constants_match_c() {
        // C: com.h:465-482 — the 15 RS message types.
        assert_eq!(minix_types::RS_RQ_BASE, 0x700);
        assert_eq!(RS_INIT, 0x700 + 20);
        assert_eq!(RS_LU_PREPARE, 0x700 + 21);
        assert_eq!(RS_FI, 0x700 + 24);
    }
}
