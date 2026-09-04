//! MIB semantic layer: validated views over the six wire payloads.
//!
//! Mirrors the `vm.rs` In/Out split: `message.rs` owns the 56-byte wire
//! shapes, this module owns what they *mean* after the message-level
//! checks in `main.c`/`remote.c`. Every constructor returns the same
//! errno C returns at that check, so callers never re-derive verdicts.
//!
//! 02-mib-message-contract.md. Behaviour (dispatch, mount policy, relay)
//! lives in `minix-mib` (01/10/12); only the wire verdicts live here.

use super::message::{
    MessLcMibSysctl, MessLsysMibRegister, MessLsysMibReply, MessMibLcSysctl, MessMibLsysCall,
    MessMibLsysInfo,
};
use crate::types::{CTL_MAXNAME, CTL_SHORTNAME, EDONTREPLY, EINVAL, Endpoint};

/// A decoded sysctl(2) request: who asks, what name, which sinks.
///
/// C: `mib_sysctl` decode (`main.c:291-351`) minus the two effects (long
/// name fetch, handler call — 06/10). `namelen` already survived
/// name fetch, handler call — 06/10). `namelen` already survived
/// `check_namelen` (01) or this constructor: both judge the same bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysctlRequest {
    /// Caller endpoint (`m_source`). C: `call.call_endpt`.
    pub caller: Endpoint,
    /// Name length in components. C: `call.call_namelen`.
    pub name_len: u32,
    /// Where the name bytes live. C: inline `name[8]` vs `namep` fetch.
    pub name: SysctlName,
    /// Old-data sink, if the caller opened one. C: `oldpp`.
    pub old: Option<DataSink>,
    /// New data, if the caller supplied both halves. C: `newpp`.
    pub new: Option<NewData>,
}

/// Name source after the length verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysctlName {
    /// `namelen <= CTL_SHORTNAME`: bytes ride in the message.
    /// C: `memcpy(name, m.name, ...)` — main.c:314-315.
    Inline([i32; 8]),
    /// `namelen > CTL_SHORTNAME`: fetch from this user address first.
    /// C: `sys_datacopy(namep → &name)` — main.c:310-312 (effect: 06).
    FetchFrom(u32),
}

/// An open old-data sink. C: `struct mib_oldp` — main.c:70-74.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataSink {
    /// Owner endpoint. C: `oldp_endpt`.
    pub endpt: Endpoint,
    /// Sink address. C: `oldp_addr`.
    pub addr: u32,
    /// Sink length. C: `oldp_len`.
    pub len: u32,
}

/// Supplied new data. C: `struct mib_newp` — main.c:79-83.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewData {
    /// Owner endpoint. C: `newp_endpt`.
    pub endpt: Endpoint,
    /// Data address. C: `newp_addr`.
    pub addr: u32,
    /// Data length. C: `newp_len`.
    pub len: u32,
}

impl SysctlRequest {
    /// Decode and judge a sysctl request wire payload.
    ///
    /// Judges, in C order: length bound (`main.c:302-303` → `EINVAL`),
    /// name path (`:309-315`), old pairing with bare-length forgiveness
    /// (`:322-328`), new pairing with half-write discard (`:334-340`).
    /// A `namelen` past `CTL_MAXNAME` is refused, never truncated.
    pub fn decode(caller: Endpoint, w: &MessLcMibSysctl) -> Result<Self, i32> {
        if w.namelen == 0 || w.namelen > CTL_MAXNAME {
            return Err(EINVAL);
        }
        let name = if w.namelen > CTL_SHORTNAME {
            SysctlName::FetchFrom(w.namep)
        } else {
            SysctlName::Inline(w.name)
        };
        let old = if w.oldp != 0 {
            Some(DataSink {
                endpt: caller,
                addr: w.oldp,
                len: w.oldlen,
            })
        } else {
            None
        };
        let new = if w.newp != 0 && w.newlen != 0 {
            Some(NewData {
                endpt: caller,
                addr: w.newp,
                len: w.newlen,
            })
        } else {
            None
        };
        Ok(Self {
            caller,
            name_len: w.namelen,
            name,
            old,
            new,
        })
    }
}

/// A sysctl reply: the code plus the length the caller retries with.
///
/// C: `m_mib_lc_sysctl.oldlen` + `m_type` (`main.c:368-377`). The
/// *mapping* (when `ENOMEM` replaces success) is 01's `map_sysctl_reply`;
/// this type only carries the pair onto the wire, so the mapping is
/// judged once, never twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysctlReply {
    /// Reply code (`OK`, `ENOMEM`, or a handler errno). C: `m_type`.
    pub code: i32,
    /// Full result length (or staged `call_reslen`). C: `oldlen`.
    pub oldlen: u32,
}

impl SysctlReply {
    /// Pack the reply onto the wire. C: `m_mib_lc_sysctl` — ipc.h:1548-1552.
    pub const fn encode(self) -> MessMibLcSysctl {
        MessMibLcSysctl {
            oldlen: self.oldlen,
            _padding: [0; 52],
        }
    }
}

/// A decoded mount/unmount request.
///
/// C: `mib_register`/`mib_deregister` message check
/// (`remote.c:218-224,293-297`). Both directions share one gate: a
/// blocking (`SENDREC`) mount is refused with `ENOSYS` (answering would
/// cross with MIB→service traffic mid-mount — plan M-8); the length
/// bound below is judged only for register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountRequest {
    /// Service endpoint (`m_source`). C: passed as `endpt` to `mib_do_*`.
    pub service: Endpoint,
    /// Remote root id. C: `root_id` (the only lane deregister reads).
    pub root_id: u32,
}

/// A validated register view: whose mount, over which path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterView {
    /// The mounting service. C: `endpt` into `mib_do_register`.
    pub mount: MountRequest,
    /// Mount path (at most 8 components). C: `mib` + `miblen`.
    pub mib: [i32; 8],
    /// Mount-path length. C: `miblen`.
    pub miblen: u32,
}

impl MountRequest {
    /// Judge the register length bound (`remote.c:221-224`).
    ///
    /// `miblen > 8` (the `mib[8]` lane count) is **silently dropped**
    /// (`EDONTREPLY`): register is one-way, so there is nobody to tell,
    /// and a loud failure would only invite retries of a malformed mount.
    /// Returns the mount view on success.
    pub fn decode_register(
        service: Endpoint,
        w: &MessLsysMibRegister,
    ) -> Result<RegisterView, i32> {
        if w.miblen > CTL_SHORTNAME {
            return Err(EDONTREPLY);
        }
        Ok(RegisterView {
            mount: Self {
                service,
                root_id: w.root_id,
            },
            mib: w.mib,
            miblen: w.miblen,
        })
    }

    /// Read a deregister request: only the root id travels
    /// (`remote.c:303`); flags/lengths are the register shape reused.
    pub const fn decode_deregister(service: Endpoint, w: &MessLsysMibRegister) -> Self {
        Self {
            service,
            root_id: w.root_id,
        }
    }
}

/// A remote service's answer, keyed by request.
///
/// C: `mess_lsys_mib_reply` (`remote.c:359-364,461-464`). MIB always sends
/// `req_id = 0` ("reserved for future async support", `:344`, `:425`) and
/// requires the reply to echo 0 (`req_id != 0` → `EINVAL`); the service
/// side echoes whatever it received (`rmib.c:1078`). Nonzero pairing is a
/// designed-but-unexercised future — today every live reply carries 0,
/// and the refusal verdict lives in the relay (12), not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteReply {
    /// Answered request id: always the reserved 0 on live traffic.
    /// C: `req_id`.
    pub req_id: u32,
    /// Service status (may be `ERESTART`, 12's domain). C: `status`.
    pub status: i32,
}

impl RemoteReply {
    /// Read a remote reply off the wire. No verdict: id refusal and
    /// `ERESTART` handling belong to the relay (12).
    pub const fn decode(w: &MessLsysMibReply) -> Self {
        Self {
            req_id: w.req_id,
            status: w.status,
        }
    }

    /// Whether the reply carries the reserved zero id (`remote.c:361,463`).
    pub const fn is_reserved_zero(self) -> bool {
        self.req_id == 0
    }
}

/// A relayed call view (MIB → service): re-granted regions, no data.
///
/// C: `mess_mib_lsys_call` — ipc.h:1554-1569. Construction (grant
/// creation, version snapshot) is the relay's work (12); this view only
/// names the lanes so both sides read the same struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayedCall<'a> {
    /// Backing wire message.
    pub wire: &'a MessMibLsysCall,
}

impl<'a> RelayedCall<'a> {
    /// Wrap the wire message. No verdict: the service validates.
    pub const fn from_wire(wire: &'a MessMibLsysCall) -> Self {
        Self { wire }
    }
}

/// A description-fetch view (MIB → service at mount).
///
/// C: `mess_mib_lsys_info` — ipc.h:1571-1580. Same wrap-only contract
/// as [`RelayedCall`]: mount (12) fills the grants, the service answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoFetch<'a> {
    /// Backing wire message.
    pub wire: &'a MessMibLsysInfo,
}

impl<'a> InfoFetch<'a> {
    /// Wrap the wire message. No verdict: mount validates.
    pub const fn from_wire(wire: &'a MessMibLsysInfo) -> Self {
        Self { wire }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire_req(namelen: u32, oldp: u32, oldlen: u32, newp: u32, newlen: u32) -> MessLcMibSysctl {
        MessLcMibSysctl {
            oldp,
            oldlen,
            newp,
            newlen,
            namelen,
            namep: 0x2000,
            name: [1, 2, 3, 0, 0, 0, 0, 0],
            ..Default::default()
        }
    }

    #[test]
    fn test_sysctl_decode_bounds() {
        let me = Endpoint(5);
        // Zero refused, never "the root" (01 §1.4).
        assert_eq!(
            SysctlRequest::decode(me, &wire_req(0, 0, 0, 0, 0)),
            Err(EINVAL)
        );
        // Past-12 refused, never truncated.
        assert_eq!(
            SysctlRequest::decode(me, &wire_req(13, 0, 0, 0, 0)),
            Err(EINVAL)
        );
        assert_eq!(CTL_MAXNAME, 12);
    }

    #[test]
    fn test_sysctl_name_paths() {
        let me = Endpoint(5);
        // Short: bytes ride along (main.c:314-315).
        let short = SysctlRequest::decode(me, &wire_req(3, 0, 0, 0, 0)).unwrap();
        assert_eq!(short.name, SysctlName::Inline([1, 2, 3, 0, 0, 0, 0, 0]));
        assert_eq!(short.name_len, 3);
        // Long: address staged for the 06 fetch (main.c:310-312).
        let long = SysctlRequest::decode(me, &wire_req(9, 0, 0, 0, 0)).unwrap();
        assert_eq!(long.name, SysctlName::FetchFrom(0x2000));
    }

    #[test]
    fn test_sysctl_pairing_rules() {
        let me = Endpoint(5);
        // Bare old length forgiven (main.c:322-328).
        let bare = SysctlRequest::decode(me, &wire_req(2, 0, 999, 0, 0)).unwrap();
        assert_eq!(bare.old, None);
        let sink = SysctlRequest::decode(me, &wire_req(2, 0x1000, 64, 0, 0)).unwrap();
        assert_eq!(
            sink.old,
            Some(DataSink {
                endpt: me,
                addr: 0x1000,
                len: 64
            })
        );
        // Half writes discarded, both halves (main.c:334-340).
        let halves = [(0, 0, None), (0x3000, 0, None), (0, 64, None)];
        for (addr, len, want) in halves {
            let r = SysctlRequest::decode(me, &wire_req(2, 0, 0, addr, len)).unwrap();
            assert_eq!(r.new, want);
        }
        let full = SysctlRequest::decode(me, &wire_req(2, 0, 0, 0x3000, 64)).unwrap();
        assert_eq!(
            full.new,
            Some(NewData {
                endpt: me,
                addr: 0x3000,
                len: 64
            })
        );
    }

    #[test]
    fn test_sysctl_reply_encode() {
        // Mapping judged once (01); this only packs (ipc.h:1548-1552).
        let rep = SysctlReply {
            code: 12,
            oldlen: 64,
        }
        .encode();
        assert_eq!(rep.oldlen, 64);
        assert_eq!(core::mem::size_of_val(&rep), 56);
    }

    #[test]
    fn test_mount_register_bound() {
        let svc = Endpoint(7);
        // At the bound: path travels (remote.c:221-224).
        let ok = MessLsysMibRegister {
            miblen: 8,
            mib: [4, 1, 0, 0, 0, 0, 0, 0],
            ..Default::default()
        };
        assert_eq!(
            MountRequest::decode_register(svc, &ok).unwrap(),
            RegisterView {
                mount: MountRequest {
                    service: svc,
                    root_id: 0
                },
                mib: [4, 1, 0, 0, 0, 0, 0, 0],
                miblen: 8,
            }
        );
        // Past the bound: silent drop, one-way discipline (EDONTREPLY).
        let over = MessLsysMibRegister {
            miblen: 9,
            ..Default::default()
        };
        assert_eq!(MountRequest::decode_register(svc, &over), Err(EDONTREPLY));
        // Deregister reads only root_id (remote.c:303).
        let de = MountRequest::decode_deregister(svc, &over);
        assert_eq!((de.service, de.root_id), (svc, 0));
    }

    #[test]
    fn test_remote_reply_routing() {
        // Live traffic is 0↔0: MIB sends the reserved 0 (remote.c:344,425)
        // and requires 0 back (remote.c:361,463); the refusal verdict
        // itself lives in the relay (12, `check_reply`).
        let live = RemoteReply::decode(&MessLsysMibReply {
            req_id: 0,
            status: 0,
            ..Default::default()
        });
        assert!(live.is_reserved_zero());
        let odd = RemoteReply::decode(&MessLsysMibReply {
            req_id: 41,
            status: 200,
            ..Default::default()
        });
        assert!(!odd.is_reserved_zero());
        assert_eq!((odd.req_id, odd.status), (41, 200));
    }
}
