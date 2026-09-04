//! SCHED message surface: five letters, three doors, one reply rule.
//!
//! Mirrors the dispatch half of `main()` (`minix3/minix/servers/sched/
//! main.c:35-106`) plus `no_sys` (`utility.c:18-26`).
//! 02-sched-message-surface.md.
//!
//! The surface owns the role split and nothing else: which letter arrived,
//! which door it knocks, and whether an answer goes back. Handler bodies
//! (06~08), the balancer arm (11), and message packing (ipc) stay out.

use minix_types::{
    SCHEDULING_INHERIT, SCHEDULING_NO_QUANTUM, SCHEDULING_SET_NICE, SCHEDULING_START,
    SCHEDULING_STOP,
};

/// `SUSPEND` (`minix3/minix/include/minix/com.h:1151`): the handler asks
/// for a later reply, so the loop sends none now.
///
/// VFS types the same sentinel as `ReplyIntent` in its own crate; each
/// server owns its verdict (no cross-crate dependency for one number).
pub const SUSPEND: i32 = -998;

/// The five letters (`main.c:57-87`, `com.h:801-807`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedMsg {
    /// Take over a forked child. C: `SCHEDULING_INHERIT` — main.c:58.
    Inherit,
    /// Take over a fresh (system) process. C: `SCHEDULING_START` — main.c:59.
    Start,
    /// Stop scheduling (exit path). C: `SCHEDULING_STOP` — main.c:62.
    Stop,
    /// Change a nice value. C: `SCHEDULING_SET_NICE` — main.c:65.
    SetNice,
    /// Quantum exhausted (kernel only). C: `SCHEDULING_NO_QUANTUM` — main.c:68.
    NoQuantum,
}

impl SchedMsg {
    /// Read the letter; wild numbers refuse (`default` → `no_sys`, `85-86`).
    pub fn from_raw(m_type: i32) -> Option<Self> {
        match m_type {
            SCHEDULING_INHERIT => Some(Self::Inherit),
            SCHEDULING_START => Some(Self::Start),
            SCHEDULING_STOP => Some(Self::Stop),
            SCHEDULING_SET_NICE => Some(Self::SetNice),
            SCHEDULING_NO_QUANTUM => Some(Self::NoQuantum),
            _ => None,
        }
    }
}

/// What arrived (`main.c:44-57`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    /// A CLOCK notification: shake the queues, answer nothing (`47-54`).
    NotifyClock,
    /// Any other notification: pass over in silence (`50-52`).
    NotifyOther,
    /// A call: dispatch it (`57-87`).
    Call,
}

/// Sort the arrival (`main.c:45-55`).
///
/// Notifications never reach dispatch; only CLOCK shakes anything (the
/// shaking itself runs in 11). The status probe (`is_ipc_notify`,
/// `com.h:92`) and the source comparison both stay caller-side; their
/// verdicts arrive as booleans.
pub fn classify(is_notify: bool, is_clock: bool) -> Incoming {
    if !is_notify {
        return Incoming::Call;
    }
    if is_clock {
        Incoming::NotifyClock
    } else {
        Incoming::NotifyOther
    }
}

/// Trust the quantum-exhausted word (`main.c:70-83`).
///
/// Only the kernel may send it (`IPC_FLG_MSG_FROM_KERNEL`,
/// `ipcconst.h:28`); a forged one earns `EPERM` (and a scolding print,
/// caller-side, `79-81`). The demotion itself runs in 08 — this door
/// only checks the seal.
pub fn noquantum_trust(has_kernel_flag: bool) -> bool {
    has_kernel_flag
}

/// What the loop does with a result (`main.c:89-96`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchVerdict {
    /// Stamp the result on the message and send it back (`91-92`).
    Reply(i32),
    /// Send nothing: the handler asked for a later reply (`90`).
    NoReply,
}

/// Settle a handler result (`main.c:90-93`).
///
/// Only `SUSPEND` withholds the answer; every other code — including
/// errors — goes back on the message.
pub fn settle(result: i32) -> DispatchVerdict {
    if result == SUSPEND {
        DispatchVerdict::NoReply
    } else {
        DispatchVerdict::Reply(result)
    }
}

/// The wild-letter answer (`no_sys`, `utility.c:18-26`).
///
/// Always `ENOSYS`; the scolding print (`printf`, `24`) stays
/// caller-side. A verdict, not a type: there is only one way to refuse.
pub fn no_sys_verdict() -> i32 {
    minix_types::ENOSYS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_five_letters() {
        // Five raw numbers, five letters (`com.h:801-807`); wild refuses.
        assert_eq!(
            SchedMsg::from_raw(SCHEDULING_INHERIT),
            Some(SchedMsg::Inherit)
        );
        assert_eq!(SchedMsg::from_raw(SCHEDULING_START), Some(SchedMsg::Start));
        assert_eq!(SchedMsg::from_raw(SCHEDULING_STOP), Some(SchedMsg::Stop));
        assert_eq!(
            SchedMsg::from_raw(SCHEDULING_SET_NICE),
            Some(SchedMsg::SetNice)
        );
        assert_eq!(
            SchedMsg::from_raw(SCHEDULING_NO_QUANTUM),
            Some(SchedMsg::NoQuantum)
        );
        assert_eq!(SchedMsg::from_raw(0), None);
        assert_eq!(SchedMsg::from_raw(0xF06), None);
        // INHERIT and START share one road (`58-61`).
        assert_ne!(SchedMsg::Inherit, SchedMsg::Start);
    }

    #[test]
    fn test_notify_first() {
        // Calls dispatch; notifications never do (`45-55`).
        assert_eq!(classify(false, false), Incoming::Call);
        assert_eq!(classify(false, true), Incoming::Call);
        // Only CLOCK shakes (`47-49`); the rest pass in silence (`50-52`).
        assert_eq!(classify(true, true), Incoming::NotifyClock);
        assert_eq!(classify(true, false), Incoming::NotifyOther);
    }

    #[test]
    fn test_seal_and_settle() {
        // The seal is the whole check (`70-71`); demotion runs in 08.
        assert!(noquantum_trust(true));
        assert!(!noquantum_trust(false));
        // Only SUSPEND withholds (`90`); errors go back too.
        assert_eq!(settle(SUSPEND), DispatchVerdict::NoReply);
        assert_eq!(settle(0), DispatchVerdict::Reply(0));
        assert_eq!(
            settle(minix_types::EPERM),
            DispatchVerdict::Reply(minix_types::EPERM)
        );
        assert_eq!(SUSPEND, -998);
        // Wild letters always refuse (`utility.c:18-26`).
        assert_eq!(no_sys_verdict(), minix_types::ENOSYS);
    }
}
