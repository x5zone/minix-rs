//! Process events: subscription switch, triage, acknowledgement.
//!
//! C: `event_mask` / `update_sub` / `update_sem_sub` / `got_proc_event`
//! (main.c:3-4/:144-214) plus the event bits
//! (`PROC_EVENT_EXIT`/`PROC_EVENT_SIGNAL`, syslib.h:292-293).
//! Document `09-ipc-proc-events.md` §3 (decisions D1-D4).
//!
//! Cancelling the wait itself lives in `sem/waiter.rs`; this module owns
//! the switch (when to ask the process manager) and the triage (what an
//! arrival means). The cross-service call (`proceventmask`) and the reply
//! send stay with the service layer — this module returns the decision.

use minix_types::{Endpoint, PROC_EVENT_EXIT, PROC_EVENT_REPLY, PROC_EVENT_SIGNAL};

// ============================================================================
// Subscription switch
// ============================================================================

/// What the service layer must ask the process manager to do.
///
/// C: `proceventmask(EXIT|SIGNAL)` vs `proceventmask(0)` — main.c:163-166.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    /// Subscribe to exit and signal events.
    Subscribe,
    /// Unsubscribe from all process events.
    Unsubscribe,
}

/// Subscription demand and its synced state.
///
/// C: `event_mask` plus the `SEM_EVENTS` demand bit (main.c:3-4). One
/// demand bit exists today (semaphores); the shape keeps room for more
/// (the message-queue extension C anticipates — main.c:141-142).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription {
    /// Whether semaphore code currently wants events. C: `SEM_EVENTS` bit.
    want_sem: bool,
    /// Whether the process manager currently has us subscribed.
    subscribed: bool,
}

impl Subscription {
    /// Fresh switch: nothing wanted, nothing synced.
    pub const fn new() -> Self {
        Self {
            want_sem: false,
            subscribed: false,
        }
    }

    /// Set the semaphore demand (`update_sem_sub` — main.c:176-189, minus
    /// the actual call): returns the sync action only on a zero/non-zero
    /// edge, `None` when nothing changes (main.c:148).
    pub fn set_sem_want(&mut self, want: bool) -> Option<SyncAction> {
        self.want_sem = want;
        let want_any = self.want_sem;
        if want_any == self.subscribed {
            return None;
        }
        self.subscribed = want_any;
        Some(if want_any {
            SyncAction::Subscribe
        } else {
            SyncAction::Unsubscribe
        })
    }

    /// Current demand (for invariant tests).
    pub const fn wants(&self) -> bool {
        self.want_sem
    }
}

impl Default for Subscription {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Event triage
// ============================================================================

/// Process-event kind: exited or signalled.
///
/// C: `has_exited = (event == PROC_EVENT_EXIT)` — main.c:197. Only the
/// exit value counts as exit; everything else is a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    /// Process exited. C: `PROC_EVENT_EXIT`.
    Exit,
    /// Process signalled. C: `PROC_EVENT_SIGNAL` (or any other value).
    Signal,
}

impl EventKind {
    /// Classify a raw event value.
    pub const fn from_raw(event: u32) -> Self {
        if event == PROC_EVENT_EXIT {
            Self::Exit
        } else {
            Self::Signal
        }
    }

    /// Whether the waiter is gone (exit) or interruptible (signal).
    pub const fn exited(self) -> bool {
        matches!(self, Self::Exit)
    }
}

/// One triaged event: who it happened to, and what happened.
///
/// C: `endpt` + `has_exited` in `got_proc_event` (main.c:196-197).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcEvent {
    /// Process the event happened to. C: `endpt`.
    pub endpoint: Endpoint,
    /// What happened. C: `has_exited`.
    pub kind: EventKind,
}

impl ProcEvent {
    /// Build from the decoded message fields (the 02 decoder already split
    /// endpoint and exit flag; this re-wraps them for the 09 triage).
    pub const fn new(endpoint: Endpoint, exited: bool) -> Self {
        Self {
            endpoint,
            kind: if exited {
                EventKind::Exit
            } else {
                EventKind::Signal
            },
        }
    }

    /// Whether this event reaches the waiter cancellation (always true:
    /// C calls `sem_process_event` unconditionally when subscribed —
    /// main.c:203-204 — and the cancellation itself no-ops for strangers).
    pub const fn needs_cancel(self) -> bool {
        true
    }
}

/// Reply type acknowledging a process event.
///
/// C: `m->m_type = PROC_EVENT_REPLY` — main.c:207. Sent no matter what
/// the triage found (even for leftover events after unsubscribing —
/// main.c:160-161).
pub const fn ack_type() -> i32 {
    PROC_EVENT_REPLY
}

/// Signal event bit (re-export for mask readers).
pub const SIGNAL_BIT: u32 = PROC_EVENT_SIGNAL;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_opens_on_first_want() {
        // C: main.c:148/:163-164 — zero to non-zero subscribes.
        let mut sub = Subscription::new();
        assert_eq!(sub.set_sem_want(true), Some(SyncAction::Subscribe));
        assert!(sub.wants());
    }

    #[test]
    fn sync_closes_on_last_drop() {
        // C: main.c:148/:165-166 — non-zero to zero unsubscribes.
        let mut sub = Subscription::new();
        sub.set_sem_want(true);
        assert_eq!(sub.set_sem_want(false), Some(SyncAction::Unsubscribe));
        assert!(!sub.wants());
    }

    #[test]
    fn sync_ignores_flapping() {
        // C: main.c:148 — no edge, no call (repeated wants are silent).
        let mut sub = Subscription::new();
        assert_eq!(sub.set_sem_want(false), None);
        sub.set_sem_want(true);
        assert_eq!(sub.set_sem_want(true), None);
    }

    #[test]
    fn event_kind_maps_exit_only() {
        // C: main.c:197 — equality with the exit bit, nothing else.
        assert_eq!(EventKind::from_raw(PROC_EVENT_EXIT), EventKind::Exit);
        assert_eq!(EventKind::from_raw(PROC_EVENT_SIGNAL), EventKind::Signal);
        assert_eq!(EventKind::from_raw(0xFFFF), EventKind::Signal);
        assert!(EventKind::Exit.exited());
        assert!(!EventKind::Signal.exited());
        let event = ProcEvent::new(Endpoint(7), true);
        assert_eq!(event.kind, EventKind::Exit);
        assert!(event.needs_cancel());
    }

    #[test]
    fn ack_is_reply_type() {
        // C: main.c:207 — the echo reply type.
        assert_eq!(ack_type(), PROC_EVENT_REPLY);
    }
}
