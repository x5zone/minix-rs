//! MIB message surface: three letters, two refusals, one reply rule.
//!
//! Mirrors the dispatch half of `main()` (`minix3/minix/servers/mib/
//! main.c:442-488`) plus the pure decision skeleton of `mib_sysctl`
//! (`main.c:277-379`). 01-mib-init-main.md.
//!
//! The surface owns the verdict split and nothing else: which letter
//! arrived, which refusal it earns, whether an answer goes back, and how
//! a sysctl request is judged before any byte moves. Handler bodies
//! (09/13~20), dispatch proper (10), the tree (03~05/08), copy effects
//! (06), auth (07), and remote mounting (12) stay out.
//!
//! Single-threaded event loop: pure functions, no shared state. The two
//! effectful steps of `mib_sysctl` — the long-name `sys_datacopy`
//! (`main.c:310-312`) and the handler call itself (`:353`) — stay behind
//! the copy/dispatch traits owned by 06/10; this module only decides
//! *which* path the loop takes and *what* the reply carries.

use minix_types::{
    EDONTREPLY, EINVAL, ENOMEM, ENOSYS, MIB_DEREGISTER, MIB_REGISTER, MIB_SYSCTL, OK,
};

/// Largest sysctl name the loop accepts, in components.
///
/// C: `CTL_MAXNAME` — sys/sys/sysctl.h:75. The full `CTL_*`/`CTLTYPE_*`/
/// `CTLFLAG_*` tables live in 02-mib-message-contract.md; only the two
/// decode thresholds this module judges on are repeated here.
pub const CTL_MAXNAME: u32 = 12;

/// Longest name that rides inside the request message, in components.
///
/// C: `CTL_SHORTNAME` — minix/ipc.h:15 (`name[CTL_SHORTNAME]` in
/// `mess_lc_mib_sysctl`, ipc.h:431). Names at or below this length are
/// copied out of the message inline (`main.c:314-315`); longer names
/// need one kernel copy (`:310-312`, effect owned by 06).
pub const CTL_SHORTNAME: u32 = 8;

/// The three letters (`main.c:459-472`, `com.h:1026-1028`).
///
/// MIB has no dead numbers: `NR_MIB_CALLS` is 3 and all three are cased,
/// unlike DS where `DS_SNAPSHOT` is declared but cased nowhere (A-7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MibCall {
    /// sysctl(2) request. C: `MIB_SYSCTL` — main.c:459.
    Sysctl,
    /// Mount a remote subtree. C: `MIB_REGISTER` — main.c:464.
    Register,
    /// Unmount a remote subtree. C: `MIB_DEREGISTER` — main.c:469.
    Deregister,
}

impl MibCall {
    /// Read the letter; wild numbers refuse (`default`, `main.c:474-479`).
    pub const fn from_raw(mtype: i32) -> Option<Self> {
        match mtype {
            MIB_SYSCTL => Some(Self::Sysctl),
            MIB_REGISTER => Some(Self::Register),
            MIB_DEREGISTER => Some(Self::Deregister),
            _ => None,
        }
    }
}

/// What arrived (`main.c:449-479`).
///
/// The two refusals share no answer but share one property: neither
/// reaches a handler. A notification is not a request at all (the kernel
/// said "something happened", `is_ipc_notify`, com.h:92); a wild number
/// is a request with no road (`default`). One name per reason keeps the
/// two diagnosable — same split as DS, different detector (see `triage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incoming {
    /// A notification: log and take the next letter (`449-454`).
    NotifyRefusal,
    /// A known letter: run it (`459-472`).
    Dispatch(MibCall),
    /// A wild number: `default` (`474-479`).
    Unknown,
}

/// Triage an arrival (`main.c:449-479`).
///
/// `notify` is the kernel's own verdict (`is_ipc_notify(ipc_status)`,
/// com.h:92) — MIB reads with `sef_receive_status`, so unlike DS (which
/// reads with `sef_receive` and must guess from the call number with the
/// FIXME-marked `is_notify` macro, com.h:90-93) there is nothing to guess.
/// Notifications refuse before the message type is even looked at.
pub const fn triage(notify: bool, mtype: i32) -> Incoming {
    if notify {
        return Incoming::NotifyRefusal;
    }
    match MibCall::from_raw(mtype) {
        Some(call) => Incoming::Dispatch(call),
        None => Incoming::Unknown,
    }
}

/// The `default` verdict (`main.c:474-479`).
///
/// Blocking (`SENDREC`) callers get a spoken refusal, `ENOSYS`; anything
/// else (one-way sends, notifications that slipped through) gets silence,
/// `EDONTREPLY` — a sender that never blocked for a reply has no slot
/// waiting for one, so sending it would be a misdelivery, not an answer.
/// DS answers `EINVAL` unconditionally; MIB asks the call shape first.
pub const fn default_outcome(is_sendrec: bool) -> i32 {
    if is_sendrec { ENOSYS } else { EDONTREPLY }
}

/// Whether the loop answers (`main.c:482-487`).
///
/// Only `EDONTREPLY` (203) silences it. Every other outcome — `OK` or any
/// errno — is sent back as `m_type` via `ipc_sendnb`, whose failure is
/// logged, never panicked (`:485-486`): a dead caller must not kill the
/// registry. Contrast receive: `sef_receive_status` failure panics
/// (`:446`) — a deaf loop is unrecoverable, a deaf caller is routine.
pub const fn should_reply(result: i32) -> bool {
    result != EDONTREPLY
}

/// Judge the requested name length (`main.c:302-303`).
///
/// `0 < namelen <= CTL_MAXNAME`, else `EINVAL`. Zero is not "the root" —
/// the root is unreachable by design (mib.h, `mib_root` is internal-only)
/// — and past-12 is not truncated, it is refused: silently shortening a
/// name would resolve a different node than the caller named.
pub const fn check_namelen(namelen: u32) -> Result<u32, i32> {
    if namelen == 0 || namelen > CTL_MAXNAME {
        Err(EINVAL)
    } else {
        Ok(namelen)
    }
}

/// Where the name bytes come from (`main.c:309-315`).
///
/// Short names ride in the message (`name[CTL_SHORTNAME]`, ipc.h:431) so
/// the loop avoids a kernel copy; long names are fetched with one
/// `sys_datacopy` from `namep`. The copy itself is an effect (06); the
/// *choice* is pure and testable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamePath {
    /// `namelen <= CTL_SHORTNAME`: copy out of the message (`:313-315`).
    Inline,
    /// `namelen > CTL_SHORTNAME`: one kernel copy from `namep` (`:309-312`).
    Copy,
}

/// Pick the name path for a validated length.
///
/// Callers must pass a length that survived `check_namelen`; an
/// out-of-range length takes the `Copy` arm rather than panicking, so a
/// programming slip degrades to an extra copy attempt, never a trap.
pub const fn classify_name(namelen: u32) -> NamePath {
    if namelen > CTL_SHORTNAME {
        NamePath::Copy
    } else {
        NamePath::Inline
    }
}

/// Whether the caller supplied an old-data sink (`main.c:322-328`).
///
/// `oldaddr != 0` opens the sink; a nonzero `oldlen` with a zero address
/// is forgiven (`:318-321`, callers often pass an uninitialized length
/// pointer) — the sink stays shut and the length is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OldpPresence {
    /// No sink: lengths are computed but nothing is stored.
    Absent,
    /// Sink open at (`endpt`, `addr`) for up to `len` bytes.
    Present,
}

/// Pair the old-data arguments (`main.c:322-328`).
pub const fn pair_oldp(oldaddr: u64, _oldlen: u64) -> OldpPresence {
    if oldaddr != 0 {
        OldpPresence::Present
    } else {
        OldpPresence::Absent
    }
}

/// Whether the caller supplied new data (`main.c:334-340`).
///
/// Both halves must be nonzero — like NetBSD, half a write is no write:
/// one of `newaddr`/`newlen` zero discards both. There is no forgiveness
/// here to mirror the old-data side, because a half write names no data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewpPresence {
    /// No new data: read-only call.
    Absent,
    /// `newlen` bytes wait at (`endpt`, `newaddr`).
    Present,
}

/// Pair the new-data arguments (`main.c:334-340`).
pub const fn pair_newp(newaddr: u64, newlen: u64) -> NewpPresence {
    if newaddr != 0 && newlen != 0 {
        NewpPresence::Present
    } else {
        NewpPresence::Absent
    }
}

/// Map a handler outcome to the reply (`main.c:368-377`).
///
/// Returns `(reply_code, out_oldlen)` for `m_mib_lc_sysctl.oldlen`
/// (ipc.h:1551). Two arms, both NetBSD-inherited:
///
/// - `r >= 0`: `r` is the full result length. It is always reported —
///   even when it did not fit — and when the sink was given but too
///   small, success degrades to `ENOMEM` (`:371-374`). Partial bytes plus
///   the full length: the caller learns *how much to retry with*.
/// - `r < 0`: a real error. Whatever `call_reslen` the handler staged
///   (via `mib_setoldlen`, `:147-152` — today only node-create
///   collisions, `EEXIST` plus the existing node) rides along (`:376`).
pub const fn map_sysctl_reply(
    handler_result: i64,
    oldaddr: u64,
    oldlen: u64,
    error_reslen: u64,
) -> (i32, u64) {
    if handler_result >= 0 {
        let full = handler_result as u64;
        if oldaddr != 0 && oldlen < full {
            (ENOMEM, full)
        } else {
            (OK, full)
        }
    } else {
        (handler_result as i32, error_reslen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_numbers_cover_all_three() {
        // C: com.h:1026-1028. Three letters, no dead numbers (NR_MIB_CALLS=3).
        assert_eq!(MibCall::from_raw(MIB_SYSCTL), Some(MibCall::Sysctl));
        assert_eq!(MibCall::from_raw(MIB_REGISTER), Some(MibCall::Register));
        assert_eq!(MibCall::from_raw(MIB_DEREGISTER), Some(MibCall::Deregister));
        assert_eq!(MibCall::from_raw(0x42), None);
        assert_eq!(MibCall::from_raw(MIB_BASE_PLUS_THREE), None);
    }

    #[test]
    fn test_triage_kernel_says_notify() {
        // Notifications refuse before the type is read (`449-454`).
        assert_eq!(triage(true, MIB_SYSCTL), Incoming::NotifyRefusal);
        assert_eq!(triage(true, 0x42), Incoming::NotifyRefusal);
        // Known letters dispatch (`459-472`); wild numbers are unknown.
        assert_eq!(
            triage(false, MIB_SYSCTL),
            Incoming::Dispatch(MibCall::Sysctl)
        );
        assert_eq!(
            triage(false, MIB_REGISTER),
            Incoming::Dispatch(MibCall::Register)
        );
        assert_eq!(
            triage(false, MIB_DEREGISTER),
            Incoming::Dispatch(MibCall::Deregister)
        );
        assert_eq!(triage(false, 0x42), Incoming::Unknown);
    }

    #[test]
    fn test_default_outcome_asks_call_shape() {
        // Blocking callers hear ENOSYS; the rest hear silence (`474-479`).
        assert_eq!(default_outcome(true), ENOSYS);
        assert_eq!(default_outcome(false), EDONTREPLY);
    }

    #[test]
    fn test_should_reply_only_silence_silences() {
        // Only the silence sentinel silences (`482-487`).
        assert!(!should_reply(EDONTREPLY));
        assert!(should_reply(OK));
        assert!(should_reply(ENOMEM));
        assert!(should_reply(EINVAL));
    }

    #[test]
    fn test_check_namelen_bounds() {
        // `0 < namelen <= CTL_MAXNAME` (`302-303`); CTL_MAXNAME=12.
        assert_eq!(CTL_MAXNAME, 12);
        assert_eq!(check_namelen(0), Err(EINVAL));
        assert_eq!(check_namelen(1), Ok(1));
        assert_eq!(check_namelen(12), Ok(12));
        assert_eq!(check_namelen(13), Err(EINVAL));
    }

    #[test]
    fn test_classify_name_short_long() {
        // CTL_SHORTNAME=8: at/below rides along, above is fetched (`309-315`).
        assert_eq!(CTL_SHORTNAME, 8);
        assert_eq!(classify_name(1), NamePath::Inline);
        assert_eq!(classify_name(8), NamePath::Inline);
        assert_eq!(classify_name(9), NamePath::Copy);
        assert_eq!(classify_name(12), NamePath::Copy);
    }

    #[test]
    fn test_pair_oldp_forgiving_newp_strict() {
        // Old: bare length forgiven (`322-328`); new: halves discarded (`334-340`).
        assert_eq!(pair_oldp(0, 999), OldpPresence::Absent);
        assert_eq!(pair_oldp(0x1000, 0), OldpPresence::Present);
        assert_eq!(pair_newp(0, 0), NewpPresence::Absent);
        assert_eq!(pair_newp(0x1000, 0), NewpPresence::Absent);
        assert_eq!(pair_newp(0, 64), NewpPresence::Absent);
        assert_eq!(pair_newp(0x1000, 64), NewpPresence::Present);
    }

    #[test]
    fn test_map_sysctl_reply_overflow_becomes_enomem() {
        // Success fits: OK + full length (`368-374`).
        assert_eq!(map_sysctl_reply(4, 0x1000, 64, 0), (OK, 4));
        // Success overflows the sink: partial bytes + full length + ENOMEM.
        assert_eq!(map_sysctl_reply(64, 0x1000, 4, 0), (ENOMEM, 64));
        // No sink: length reported, no complaint (`oldaddr == 0` skips).
        assert_eq!(map_sysctl_reply(64, 0, 0, 0), (OK, 64));
        // Error: staged reslen rides along (`375-376`, EEXIST path).
        assert_eq!(map_sysctl_reply(-17, 0x1000, 64, 20), (-17, 20));
    }

    /// `MIB_BASE + 3`: first number past the three letters (com.h:1030).
    const MIB_BASE_PLUS_THREE: i32 = 0x1803;
}
