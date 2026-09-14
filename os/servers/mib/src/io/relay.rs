//! Relay verdicts: who may read/write whose bytes, and how failures speak.
//!
//! Mirrors the pure halves of `mib_relay_oldp` / `mib_relay_newp`
//! (`main.c:204-252`) plus the grant half itself: [`RelayRequest::open`]
//! drives `cpf_grant_magic` and [`RelayGrant::close`] drives `cpf_revoke`
//! over the kernel transport. Creation failure must never speak `ENOMEM`
//! (`:208`, `:236`: "must not be ENOMEM") — [`RELAY_FAIL`] is the only
//! error the open half speaks.
//!
//! 06-mib-copy-io.md.

use minix_types::{CPF_READ, CPF_WRITE, EINVAL, Endpoint, GrantId};

use crate::transport::MibKernel;

/// Invalid grant: no region behind it. C: `GRANT_INVALID` — safecopies.h:52.
pub const GRANT_INVALID: GrantId = -1;

/// Whether a grant id names a region. C: `GRANT_VALID(g)` — safecopies.h:53.
pub const fn grant_valid(g: GrantId) -> bool {
    g > GRANT_INVALID
}

/// Relay direction: which way the service may move the bytes.
///
/// C: `CPF_WRITE` for old regions (the service writes answers into the
/// user's sink — main.c:216-217), `CPF_READ` for new regions (the service
/// reads what the user wrote — :241-242). The names read from MIB's side:
/// "I grant you write into my old sink."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayDir {
    /// Service may write (old-data sink). C: `CPF_WRITE`.
    Write,
    /// Service may read (new-data source). C: `CPF_READ`.
    Read,
}

impl RelayDir {
    /// Wire flag for the grant. C: `CPF_WRITE`/`CPF_READ` — safecopies.h:64-65.
    pub const fn flag(self) -> i32 {
        match self {
            Self::Write => CPF_WRITE,
            Self::Read => CPF_READ,
        }
    }
}

/// A relayed region: present with a length, or absent.
///
/// Presence is judged here; the grant *id* is a transport product and
/// lives in [`RelayGrant`] — a placeholder id in this struct would be
/// an invitation to send a fake grant downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayRegion {
    /// Whether the region is open at all. C: `GRANT_VALID(*grantp)`.
    pub present: bool,
    /// Region length (0 when shut). C: `*lenp`.
    pub len: u64,
}

impl RelayRegion {
    /// Judge an old-data region for relay (`main.c:210-227`).
    ///
    /// Shut sinks relay absent + zero (`:221-224`); open ones relay
    /// their length with a write grant to be created (`:215-220`).
    pub const fn relay_old(sink: Option<(u32, u64)>) -> Self {
        match sink {
            None => Self {
                present: false,
                len: 0,
            },
            Some((_, len)) => Self {
                present: true,
                len,
            },
        }
    }

    /// Judge a new-data region for relay (`main.c:235-252`).
    /// Mirror of [`RelayRegion::relay_old`] with read direction.
    pub const fn relay_new(data: Option<(u32, u64)>) -> Self {
        match data {
            None => Self {
                present: false,
                len: 0,
            },
            Some((_, len)) => Self {
                present: true,
                len,
            },
        }
    }
}

/// One grant to open: whose memory, which span, which way.
///
/// Built from a judged [`RelayRegion`] plus the coordinates the verdict
/// layer deliberately does not carry (the caller's endpoint and base
/// address); `open` is the only road to a live [`RelayGrant`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayRequest {
    /// The caller whose memory the service may touch. C: `endpoint`.
    pub caller: Endpoint,
    /// Region base in the caller's space. C: `oldp->oldp_addr` 一带.
    pub addr: u64,
    /// Region length. C: `*lenp`.
    pub len: u64,
    /// Which way the service may move the bytes.
    pub dir: RelayDir,
}

impl RelayRequest {
    /// Create the magic grant.
    ///
    /// C: `cpf_grant_magic(endpoint, addr, len, flags)` — main.c:216-217
    /// (old/write), :241-242 (new/read). Any failure — including an
    /// invalid id the transport somehow returns — speaks [`RELAY_FAIL`]
    /// (:218-219, :242-243: "must not be ENOMEM").
    pub fn open(&self, t: &mut impl MibKernel) -> Result<RelayGrant, i32> {
        match t.grant_magic(self.caller, self.addr, self.len, self.dir) {
            Ok(id) if grant_valid(id) => Ok(RelayGrant { id }),
            _ => Err(RELAY_FAIL),
        }
    }
}

/// A live relay grant. `close` consumes it — a grant cannot be revoked
/// twice through this road.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayGrant {
    /// The live grant id. C: `*grantp` after a valid `cpf_grant_magic`.
    pub id: GrantId,
}

impl RelayGrant {
    /// Retire the grant. C: `cpf_revoke` — remote.c:441-446 revokes all
    /// three in reverse creation order; the sequencing belongs to the
    /// caller (walker), the verb belongs here.
    pub fn close(self, t: &mut impl MibKernel) {
        t.grant_revoke(self.id);
    }
}

/// Failure code for grant creation. C: "must not be ENOMEM" —
/// main.c:208,236. Allocation pressure must never masquerade as a small
/// sink (01 §1.4): sinks speak `ENOMEM`, allocators speak `EINVAL`.
/// Callers convert `GrantOutcome::Failed` with this, never by invention.
pub const RELAY_FAIL: i32 = EINVAL;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_validity() {
        // C: GRANT_VALID(g) = g > GRANT_INVALID (safecopies.h:52-53).
        assert_eq!(GRANT_INVALID, -1);
        assert!(!grant_valid(-1));
        assert!(grant_valid(0));
        assert!(grant_valid(41));
    }

    #[test]
    fn test_relay_directions() {
        // Old regions grant write, new regions grant read (main.c:216-217,241-242).
        assert_eq!(RelayDir::Write.flag(), CPF_WRITE);
        assert_eq!(RelayDir::Read.flag(), CPF_READ);
        assert_eq!((CPF_WRITE, CPF_READ), (2, 1));
    }

    #[test]
    fn test_relay_presence() {
        // Shut regions relay absent + zero (main.c:221-224,246-249).
        assert_eq!(
            RelayRegion::relay_old(None),
            RelayRegion {
                present: false,
                len: 0
            }
        );
        assert_eq!(
            RelayRegion::relay_new(None),
            RelayRegion {
                present: false,
                len: 0
            }
        );
        // Open regions relay their length; the id comes from transport.
        assert_eq!(RelayRegion::relay_old(Some((0x1000, 64))).len, 64);
        assert!(RelayRegion::relay_old(Some((0x1000, 64))).present);
        assert_eq!(RelayRegion::relay_new(Some((0x3000, 9))).len, 9);
        // Creation failure speaks EINVAL, never ENOMEM (main.c:208,236).
        assert_eq!(RELAY_FAIL, EINVAL);
    }

    #[test]
    fn test_grant_open_close_records_verbs() {
        use crate::transport::recording::{Call, Recorder};
        use minix_types::Endpoint;

        let mut t = Recorder::default();
        t.grants = vec![Ok(7), Ok(8), Ok(9)];
        let old = RelayRequest {
            caller: Endpoint::PM,
            addr: 0x1000,
            len: 64,
            dir: RelayDir::Write,
        };
        let g = old.open(&mut t).unwrap();
        assert_eq!(g.id, 7);
        g.close(&mut t);
        // The verb pair lands in order: create then revoke.
        assert_eq!(
            t.calls.borrow().len(),
            2,
            "grant_magic + revoke recorded"
        );
        assert!(matches!(
            t.calls.borrow()[0],
            Call::GrantMagic(Endpoint::PM, 0x1000, 64, RelayDir::Write)
        ));
        assert!(matches!(t.calls.borrow()[1], Call::Revoke(7)));
    }

    #[test]
    fn test_grant_open_failure_speaks_relay_fail() {
        use crate::transport::recording::Recorder;
        use minix_types::Endpoint;

        let mut t = Recorder::default();
        t.fail = true; // grant_magic fails
        let req = RelayRequest {
            caller: Endpoint::PM,
            addr: 0x1000,
            len: 64,
            dir: RelayDir::Read,
        };
        // Any open failure — transport errno or invalid id — is EINVAL.
        assert_eq!(req.open(&mut t), Err(RELAY_FAIL));
        let mut t2 = Recorder::default();
        t2.grants = vec![Ok(-1)]; // an invalid id counts as failure too
        assert_eq!(req.open(&mut t2), Err(RELAY_FAIL));
    }
}

// ── The remote-call bundle (12's execution half) ──
//
// `RelayedCall`（minix-types，wire 包装视图）携带三个 grant id 与转发
// 参数；`RemoteReplyWire` 承接应答。Send 本身是 `MibServices::remote_call`
// 的体（P1-4 形状修订：这一动词初版漏列，walker 落地时补全并记录）。

use minix_types::MessMibLsysCall;

/// One relayed call, ready to send: the wire message with the three
/// grant ids already stamped.
#[derive(Debug, Clone)]
pub struct RemoteCall {
    /// The wire lanes (12 lanes, ipc.h:1554-1569).
    pub wire: MessMibLsysCall,
}

/// The reply carrier: `req_id` + `status` lanes the service fills.
#[derive(Debug, Clone, Copy, Default)]
pub struct RemoteReplyWire {
    /// Answered request id (always the reserved 0 today).
    pub req_id: u32,
    /// Service status — may be `ERESTART`.
    pub status: i32,
}
