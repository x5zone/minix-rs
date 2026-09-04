//! DS message surface: seven letters, two refusals, one reply rule.
//!
//! Mirrors the dispatch half of `main()` (`minix3/minix/servers/ds/
//! main.c:45-88`). 01-ds-init-main.md.
//!
//! The surface owns the role split and nothing else: which letter arrived,
//! which refusal it earns, and whether an answer goes back. Handler bodies
//! (07~11), boot mapping (06), and message packing (ipc, 02) stay out.
//!
//! Single-threaded event loop: pure functions, no shared state.

use minix_types::{
    DS_CHECK, DS_DELETE, DS_GETSYSINFO, DS_PUBLISH, DS_RETRIEVE, DS_RETRIEVE_LABEL, DS_SUBSCRIBE,
    EDONTREPLY,
};

/// The seven letters (`main.c:54-72`, `com.h:498-507`).
///
/// `DS_SNAPSHOT` (+5) is deliberately absent: C declares it (`com.h:505`)
/// but cases it nowhere, so it falls into `default` refusal (A-7). Giving
/// it a letter would permit a road C never built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsCall {
    /// Publish data. C: `DS_PUBLISH` — main.c:54.
    Publish,
    /// Retrieve data by name. C: `DS_RETRIEVE` — main.c:57.
    Retrieve,
    /// Retrieve a label's name. C: `DS_RETRIEVE_LABEL` — main.c:60.
    RetrieveLabel,
    /// Delete data. C: `DS_DELETE` — main.c:63.
    Delete,
    /// Subscribe to updates. C: `DS_SUBSCRIBE` — main.c:66.
    Subscribe,
    /// Fetch an update. C: `DS_CHECK` — main.c:69.
    Check,
    /// Read the store image. C: `DS_GETSYSINFO` — main.c:72.
    Getsysinfo,
}

impl DsCall {
    /// Read the letter; wild numbers refuse (`default`, `75-77`).
    pub const fn from_raw(callnr: i32) -> Option<Self> {
        match callnr {
            DS_PUBLISH => Some(Self::Publish),
            DS_RETRIEVE => Some(Self::Retrieve),
            DS_RETRIEVE_LABEL => Some(Self::RetrieveLabel),
            DS_DELETE => Some(Self::Delete),
            DS_SUBSCRIBE => Some(Self::Subscribe),
            DS_CHECK => Some(Self::Check),
            DS_GETSYSINFO => Some(Self::Getsysinfo),
            _ => None,
        }
    }
}

/// What arrived (`main.c:47-78`).
///
/// Two refusals share one answer (`EINVAL` + warning) but not one reason:
/// a notify has no number to dispatch on, a wild number has no road to
/// take. One name per reason keeps the two diagnosable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    /// A notification: nothing dispatchable (`47-51`).
    NotifyRefusal,
    /// A known letter: dispatch it (`54-72`).
    Dispatch(DsCall),
    /// A wild number: no road (`75-77`).
    Unknown,
}

/// Triage an arrival (`main.c:47-78`).
///
/// `is_notify` is the legacy macro (`com.h:93`, FIXME-marked): DS reads
/// with `sef_receive` (no status), so the legacy number test is the only
/// eye — old but load-bearing. Notifications refuse; known letters
/// dispatch; the rest is unknown.
pub const fn triage(callnr: i32) -> Incoming {
    if is_notify(callnr) {
        return Incoming::NotifyRefusal;
    }
    match DsCall::from_raw(callnr) {
        Some(call) => Incoming::Dispatch(call),
        None => Incoming::Unknown,
    }
}

/// The legacy notify test (`com.h:90-93`).
///
/// `(callnr - 0x1000) < 0x100`, **unsigned**: the notification band sits
/// just above the call-number space. The unsigned cast is load-bearing —
/// a signed comparison would misread every call number below the band
/// (e.g. `0x800`) as a notification. Kept as a `const fn` so triage
/// stays pure and testable without IPC.
pub const fn is_notify(callnr: i32) -> bool {
    (callnr.wrapping_sub(0x1000) as u32) < 0x100
}

/// Whether the loop answers (`main.c:82`).
///
/// Only `EDONTREPLY` (203) silences it: a handler keeping the reply for
/// later. Every other outcome — `OK` or any errno — is sent back as
/// `m_type`.
pub const fn should_reply(result: i32) -> bool {
    result != EDONTREPLY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_numbers() {
        // Seven letters home (`com.h:498-507`); the dead +5 has no letter.
        assert_eq!(DsCall::from_raw(DS_PUBLISH), Some(DsCall::Publish));
        assert_eq!(DsCall::from_raw(DS_RETRIEVE), Some(DsCall::Retrieve));
        assert_eq!(
            DsCall::from_raw(DS_RETRIEVE_LABEL),
            Some(DsCall::RetrieveLabel)
        );
        assert_eq!(DsCall::from_raw(DS_DELETE), Some(DsCall::Delete));
        assert_eq!(DsCall::from_raw(DS_SUBSCRIBE), Some(DsCall::Subscribe));
        assert_eq!(DsCall::from_raw(DS_CHECK), Some(DsCall::Check));
        assert_eq!(DsCall::from_raw(DS_GETSYSINFO), Some(DsCall::Getsysinfo));
        // The dead snapshot number (+5) refuses like any wild number (A-7).
        assert_eq!(DsCall::from_raw(0x805), None);
        assert_eq!(DsCall::from_raw(0x1234), None);
    }

    #[test]
    fn test_triage() {
        // Notifications refuse: nothing dispatchable (`47-51`).
        assert_eq!(triage(0x1000), Incoming::NotifyRefusal);
        assert_eq!(triage(0x10FF), Incoming::NotifyRefusal);
        // Known letters dispatch (`54-72`).
        assert_eq!(triage(DS_PUBLISH), Incoming::Dispatch(DsCall::Publish));
        // Wild numbers are unknown (`75-77`), including the dead +5.
        assert_eq!(triage(0x805), Incoming::Unknown);
        assert_eq!(triage(0x42), Incoming::Unknown);
    }

    #[test]
    fn test_should_reply() {
        // Only the silence sentinel silences (`82`).
        assert!(!should_reply(EDONTREPLY));
        assert!(should_reply(0));
        assert!(should_reply(-22));
    }
}
