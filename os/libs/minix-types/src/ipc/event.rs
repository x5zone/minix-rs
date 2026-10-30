//! PM ↔ subscriber process event protocol.
//!
//! C: `minix3/minix/include/minix/com.h:597-619` (COMMON_RQ/RS + the PROC_EVENT
//! family) + `minix3/minix/include/minix/syslib.h:289-293`
//! (PROC_EVENT_EXIT/SIGNAL bit masks) +
//! `minix3/minix/include/minix/ipc.h:1414/1800` (the mess_* layout).
//!
//! The protocol layer of 04-stage-pm/06-event-subscription.md.

use crate::ipc::message::{MessLsysPmProceventmask, MessPmLsysProcEvent};
use crate::{Endpoint, Message, MessageUnion};

/// `PM_PROCEVENTMASK` — `callnr.h:53` (`PM_BASE + 40`).
pub const PM_PROCEVENTMASK: i32 = 40;

// ── COMMON base addresses and PROC_EVENT types ──

/// `COMMON_RQ_BASE` — `com.h:597`.
pub const COMMON_RQ_BASE: i32 = 0xE00;

/// `COMMON_RS_BASE` — `com.h:598`.
pub const COMMON_RS_BASE: i32 = 0xE80;

/// Forward a received signal to a service. C: `SIGS_SIGNAL_RECEIVED`
/// (`COMMON_RQ_BASE+0`) — com.h:597; the signal manager translates every
/// non-termination signal into this message (RS main.c:699-701).
///
/// Homed here because this module owns the COMMON family base; `ipc::rs`
/// no longer holds a copy.
pub const SIGS_SIGNAL_RECEIVED: i32 = COMMON_RQ_BASE;

/// Common fault-injection control request to all processes. C:
/// `COMMON_REQ_FI_CTL` (`COMMON_RQ_BASE+2`) — com.h:607; RS sends it to
/// the target service with the `m_lsys_fi_ctl` payload (fi_service —
/// utility.c:69-77).
///
/// Homed here; `ipc::rs` no longer holds a copy.
pub const COMMON_REQ_FI_CTL: i32 = COMMON_RQ_BASE + 2;

/// `PROC_EVENT` — event notification from PM to subscribers (`COMMON_RQ_BASE + 3`, `com.h:610`).
pub const PROC_EVENT: i32 = COMMON_RQ_BASE + 3;

/// `PROC_EVENT_REPLY` — the subscriber's reply to PM (`COMMON_RS_BASE + 0`, `com.h:619`).
pub const PROC_EVENT_REPLY: i32 = COMMON_RS_BASE;

/// `PROC_EVENT_EXIT` — process-exit event bit (`syslib.h:292`, `0x01`, usable as a mask).
pub const PROC_EVENT_EXIT: u32 = 0x01;

/// `PROC_EVENT_SIGNAL` — process-signal event bit (`syslib.h:293`, `0x02`, usable as a mask).
pub const PROC_EVENT_SIGNAL: u32 = 0x02;

// ── Typed events ──

/// A process event (the `0x01`/`0x02` values of `syslib.h:292-293`).
///
/// These values are both the event number and the mask bit (the C comment
/// "they form a bit mask"), so `ProcEvent::Exit as u32 == PROC_EVENT_EXIT`, and
/// `ProcEventMask` supports set arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ProcEvent {
    /// The process is exiting (inferred from `EXITING`, `event.c:87`).
    Exit = PROC_EVENT_EXIT,
    /// The process was unpaused by a signal (inferred from `UNPAUSED`, `event.c:89`).
    Signal = PROC_EVENT_SIGNAL,
}

impl ProcEvent {
    /// Parses the raw `event` field (only `0x01` / `0x02` are accepted).
    pub fn from_bits(bits: u32) -> Option<Self> {
        match bits {
            PROC_EVENT_EXIT => Some(Self::Exit),
            PROC_EVENT_SIGNAL => Some(Self::Signal),
            _ => None,
        }
    }

    /// The corresponding mask bit (`0x01` / `0x02`).
    pub const fn mask(self) -> u32 {
        self as u32
    }
}

bitflags::bitflags! {
    /// A process event mask (`subs[i].mask`, `event.c:62`).
    ///
    /// C uses a bare `unsigned int` where `mask & event != 0` is a match; Rust
    /// uses bitflags to express "or of several events" at the type level. The
    /// empty mask `empty()` means unsubscribe (`mask==0`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ProcEventMask: u32 {
        /// Subscribe to the EXIT event (`PROC_EVENT_EXIT`).
        const EXIT = PROC_EVENT_EXIT;
        /// Subscribe to the SIGNAL event (`PROC_EVENT_SIGNAL`).
        const SIGNAL = PROC_EVENT_SIGNAL;
    }
}

impl ProcEventMask {
    /// Whether a given event is contained (`mask & event != 0`, typed).
    #[inline]
    pub fn contains_event(self, event: ProcEvent) -> bool {
        self.contains(ProcEventMask::from_bits_retain(event.mask()))
    }
}

// ── Message encoding and decoding ──

/// Builds a `PROC_EVENT` event notification message (PM → subscriber).
///
/// C: `event.c:99-103` — `memset(&m,0)` + `m.m_type=PROC_EVENT` +
/// `m.m_pm_lsys_proc_event.endpt/event`.
pub fn proc_event_msg(target: Endpoint, event: ProcEvent) -> Message {
    let payload = MessPmLsysProcEvent {
        endpt: target.get(),
        event: event as u32,
        _padding: [0; 48],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: PROC_EVENT,
        m_u: MessageUnion {
            m_pm_lsys_proc_event: payload,
        },
    }
}

/// Decodes the payload of a `PROC_EVENT` message (PM → subscriber).
pub fn decode_proc_event(msg: &Message) -> Option<(Endpoint, ProcEvent)> {
    if msg.m_type != PROC_EVENT {
        return None;
    }
    let pl = unsafe { msg.m_u.m_pm_lsys_proc_event };
    let ep = Endpoint(pl.endpt);
    let ev = ProcEvent::from_bits(pl.event)?;
    Some((ep, ev))
}

/// Builds a `PROC_EVENT_REPLY` message (subscriber → PM).
///
/// C: after receiving `PROC_EVENT` the subscriber echoes `m_pm_lsys_proc_event`
/// back unchanged (`endpt`/`event` identical to what it received; the
/// consistency check at `event.c:274-278` relies on that echo).
pub fn proc_event_reply_msg(target: Endpoint, event: ProcEvent) -> Message {
    let payload = MessPmLsysProcEvent {
        endpt: target.get(),
        event: event as u32,
        _padding: [0; 48],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: PROC_EVENT_REPLY,
        m_u: MessageUnion {
            m_pm_lsys_proc_event: payload,
        },
    }
}

/// Decodes the payload of a `PROC_EVENT_REPLY` message (subscriber → PM).
pub fn decode_proc_event_reply(msg: &Message) -> Option<(Endpoint, ProcEvent)> {
    if msg.m_type != PROC_EVENT_REPLY {
        return None;
    }
    let pl = unsafe { msg.m_u.m_pm_lsys_proc_event };
    let ep = Endpoint(pl.endpt);
    let ev = ProcEvent::from_bits(pl.event)?;
    Some((ep, ev))
}

/// Builds a `PROCEVENTMASK` subscription-update message (subscriber → PM).
/// C: `m_in.m_lsys_pm_proceventmask.mask` at `event.c:179`.
pub fn proceventmask_msg(mask: ProcEventMask) -> Message {
    let payload = MessLsysPmProceventmask {
        mask: mask.bits(),
        _padding: [0; 52],
    };
    Message {
        m_source: Endpoint::NONE,
        m_type: PM_PROCEVENTMASK,
        m_u: MessageUnion {
            m_lsys_pm_proceventmask: payload,
        },
    }
}

/// Decodes the mask of a `PROCEVENTMASK` message.
pub fn decode_proceventmask(msg: &Message) -> Option<ProcEventMask> {
    if msg.m_type != PM_PROCEVENTMASK {
        return None;
    }
    let pl = unsafe { msg.m_u.m_lsys_pm_proceventmask };
    Some(ProcEventMask::from_bits_truncate(pl.mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proc_event_constants_match_com_h() {
        assert_eq!(COMMON_RQ_BASE, 0xE00);
        assert_eq!(COMMON_RS_BASE, 0xE80);
        assert_eq!(PROC_EVENT, COMMON_RQ_BASE + 3);
        assert_eq!(PROC_EVENT_REPLY, COMMON_RS_BASE + 0);
        assert_eq!(PROC_EVENT_EXIT, 0x01);
        assert_eq!(PROC_EVENT_SIGNAL, 0x02);
    }

    #[test]
    fn test_proc_event_mask_contains() {
        let empty = ProcEventMask::empty();
        assert!(!empty.contains_event(ProcEvent::Exit));
        assert!(!empty.contains_event(ProcEvent::Signal));

        let both = ProcEventMask::EXIT | ProcEventMask::SIGNAL;
        assert!(both.contains_event(ProcEvent::Exit));
        assert!(both.contains_event(ProcEvent::Signal));

        let only_exit = ProcEventMask::EXIT;
        assert!(only_exit.contains_event(ProcEvent::Exit));
        assert!(!only_exit.contains_event(ProcEvent::Signal));
    }

    #[test]
    fn test_proc_event_msg_roundtrip() {
        let ep = Endpoint::from_generation_slot(1, 5);
        for ev in [ProcEvent::Exit, ProcEvent::Signal] {
            let msg = proc_event_msg(ep, ev);
            assert_eq!(msg.m_type, PROC_EVENT);
            let (dec_ep, dec_ev) = decode_proc_event(&msg).unwrap();
            assert_eq!(dec_ep, ep);
            assert_eq!(dec_ev, ev);

            let reply = proc_event_reply_msg(ep, ev);
            assert_eq!(reply.m_type, PROC_EVENT_REPLY);
            let (r_ep, r_ev) = decode_proc_event_reply(&reply).unwrap();
            assert_eq!(r_ep, ep);
            assert_eq!(r_ev, ev);
        }
    }

    #[test]
    fn test_proceventmask_msg_roundtrip() {
        for mask in [
            ProcEventMask::empty(),
            ProcEventMask::EXIT,
            ProcEventMask::SIGNAL,
            ProcEventMask::EXIT | ProcEventMask::SIGNAL,
        ] {
            let msg = proceventmask_msg(mask);
            let dec = decode_proceventmask(&msg).unwrap();
            assert_eq!(dec, mask);
        }
        // Unknown bits are truncated away (same behavior as C: the mask is any unsigned)
        let mask = ProcEventMask::from_bits_truncate(0xFF);
        let msg = proceventmask_msg(mask);
        let dec = decode_proceventmask(&msg).unwrap();
        assert_eq!(dec.bits(), 0x03); // only EXIT|SIGNAL survive (truncate)
    }
}
