//! VFS service IPC message types.
//!
//! Defines the messages exchanged between VFS and other services (PM, Kernel).

use crate::{
    EAGAIN, EINVAL, EIO, EMFILE, ENOSYS, ESRCH, Endpoint, Message, MessageM7, MessageUnion,
    NR_PROCS, Pid,
};

/// VFS request message types.
///
/// These are the requests that VFS receives from other services.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsRequest {
    /// Fork request from PM.
    ///
    /// PM sends this to duplicate the parent's file descriptor table.
    Fork {
        /// Parent process endpoint.
        parent_endpoint: Endpoint,
        /// Child process endpoint.
        child_endpoint: Endpoint,
    },
}

/// VFS response message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsResponse {
    /// Fork succeeded.
    ForkOk,
    /// Operation failed.
    Error(VfsError),
}

/// VFS error types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    /// Process table is full.
    ProcTableFull,
    /// Invalid endpoint.
    InvalidEndpoint,
    /// Slot is already in use.
    SlotInUse,
    /// Too many open files.
    TooManyOpenFiles,
    /// Internal error.
    InternalError,
    /// Operation not implemented.
    NotImplemented,
}

impl VfsError {
    /// Converts error to errno value.
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::ProcTableFull => EAGAIN,
            Self::InvalidEndpoint => ESRCH,
            Self::SlotInUse => EINVAL,
            Self::TooManyOpenFiles => EMFILE,
            Self::InternalError => EIO,
            Self::NotImplemented => ENOSYS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vfs_fork_request() {
        let req = VfsRequest::Fork {
            parent_endpoint: Endpoint::PM,
            child_endpoint: Endpoint::from_generation_slot(1, 1),
        };

        match req {
            VfsRequest::Fork {
                parent_endpoint,
                child_endpoint,
            } => {
                assert_eq!(parent_endpoint, Endpoint::PM);
                assert_eq!(child_endpoint, Endpoint::from_generation_slot(1, 1));
            }
        }
    }

    #[test]
    fn test_vfs_response_fork_ok() {
        let resp = VfsResponse::ForkOk;
        assert!(matches!(resp, VfsResponse::ForkOk));
    }

    #[test]
    fn test_vfs_error_to_errno() {
        assert_eq!(VfsError::ProcTableFull.to_errno(), EAGAIN);
        assert_eq!(VfsError::TooManyOpenFiles.to_errno(), EMFILE);
        assert_eq!(VfsError::NotImplemented.to_errno(), ENOSYS);
    }
}

// ── VFS_PM protocol (PM → VFS) ──
//
// C: include/minix/com.h:513-531 (VFS_PM_RQ_BASE / the VFS_PM_* message family) +
//    com.h:547-551 (VFS_PM_INIT fields m7_i1/m7_i2/m7_i3, mess_7 layout).
// PM-side sender: os/servers/pm/src/init.rs (04-stage-pm/01-pm-init-main.md §2.1 step 6);
// VFS-side receiver: 05-stage-vfs/01-vfs-init-main.md §2.3.

/// VFS_PM request message type base (com.h:513).
pub const VFS_PM_RQ_BASE: i32 = 0x900;

/// VFS_PM_INIT — process table exchange (com.h:520).
///
/// PM sends boot process information one entry at a time (slot + PID + endpoint);
/// a final endpoint = NONE means "no more system processes" and then it waits
/// synchronously for VFS's OK reply.
#[allow(clippy::identity_op)] // Keep the same shape as C `(VFS_PM_RQ_BASE + 0)`.
pub const VFS_PM_INIT: i32 = VFS_PM_RQ_BASE + 0;

// ── PM → VFS runtime requests (com.h:521-531, excluding boot-time VFS_PM_INIT) ──
/// VFS_PM_SETUID — replace the effective/real UID (com.h:521).
pub const VFS_PM_SETUID: i32 = VFS_PM_RQ_BASE + 1;
/// VFS_PM_SETGID — replace the effective/real GID (com.h:522).
pub const VFS_PM_SETGID: i32 = VFS_PM_RQ_BASE + 2;
/// VFS_PM_SETSID — create a new session (com.h:523).
pub const VFS_PM_SETSID: i32 = VFS_PM_RQ_BASE + 3;
/// VFS_PM_EXIT — process exit (com.h:524).
pub const VFS_PM_EXIT: i32 = VFS_PM_RQ_BASE + 4;
/// VFS_PM_DUMPCORE — process core dump (com.h:525).
pub const VFS_PM_DUMPCORE: i32 = VFS_PM_RQ_BASE + 5;
/// VFS_PM_EXEC — load an executable image (com.h:526).
pub const VFS_PM_EXEC: i32 = VFS_PM_RQ_BASE + 6;
/// VFS_PM_FORK — duplicate the parent's fd table (com.h:527).
pub const VFS_PM_FORK: i32 = VFS_PM_RQ_BASE + 7;
/// VFS_PM_SRV_FORK — service process fork (com.h:528).
pub const VFS_PM_SRV_FORK: i32 = VFS_PM_RQ_BASE + 8;
/// VFS_PM_UNPAUSE — unpause a process (com.h:529).
pub const VFS_PM_UNPAUSE: i32 = VFS_PM_RQ_BASE + 9;
/// VFS_PM_REBOOT — system reboot (com.h:530).
pub const VFS_PM_REBOOT: i32 = VFS_PM_RQ_BASE + 10;
/// VFS_PM_SETGROUPS — replace the supplementary groups (com.h:531).
pub const VFS_PM_SETGROUPS: i32 = VFS_PM_RQ_BASE + 11;

// ── VFS → PM replies (com.h:534-544) ──
/// VFS_PM response message type base (com.h:514).
pub const VFS_PM_RS_BASE: i32 = 0x980;

/// VFS_PM_SETUID_REPLY (com.h:534).
pub const VFS_PM_SETUID_REPLY: i32 = VFS_PM_RS_BASE + 1;
/// VFS_PM_SETGID_REPLY (com.h:535).
pub const VFS_PM_SETGID_REPLY: i32 = VFS_PM_RS_BASE + 2;
/// VFS_PM_SETSID_REPLY (com.h:536).
pub const VFS_PM_SETSID_REPLY: i32 = VFS_PM_RS_BASE + 3;
/// VFS_PM_EXIT_REPLY (com.h:537).
pub const VFS_PM_EXIT_REPLY: i32 = VFS_PM_RS_BASE + 4;
/// VFS_PM_CORE_REPLY (com.h:538).
pub const VFS_PM_CORE_REPLY: i32 = VFS_PM_RS_BASE + 5;
/// VFS_PM_EXEC_REPLY (com.h:539).
pub const VFS_PM_EXEC_REPLY: i32 = VFS_PM_RS_BASE + 6;
/// VFS_PM_FORK_REPLY (com.h:540).
pub const VFS_PM_FORK_REPLY: i32 = VFS_PM_RS_BASE + 7;
/// VFS_PM_SRV_FORK_REPLY (com.h:541).
pub const VFS_PM_SRV_FORK_REPLY: i32 = VFS_PM_RS_BASE + 8;
/// VFS_PM_UNPAUSE_REPLY (com.h:542).
pub const VFS_PM_UNPAUSE_REPLY: i32 = VFS_PM_RS_BASE + 9;
/// VFS_PM_REBOOT_REPLY (com.h:543).
pub const VFS_PM_REBOOT_REPLY: i32 = VFS_PM_RS_BASE + 10;
/// VFS_PM_SETGROUPS_REPLY (com.h:544).
pub const VFS_PM_SETGROUPS_REPLY: i32 = VFS_PM_RS_BASE + 11;

/// C: `IS_VFS_PM_RQ(type)` — `((type) & ~0x7f) == VFS_PM_RQ_BASE` at com.h:516.
///
/// Note that `VFS_PM_INIT` (0x900) also belongs to the request family.
pub const fn is_vfs_pm_rq(m_type: i32) -> bool {
    (m_type & !0x7f) == VFS_PM_RQ_BASE
}

/// C: `IS_VFS_PM_RS(type)` — `((type) & ~0x7f) == VFS_PM_RS_BASE` at com.h:517.
pub const fn is_vfs_pm_rs(m_type: i32) -> bool {
    (m_type & !0x7f) == VFS_PM_RS_BASE
}

/// Payload of the PM → VFS `VFS_PM_INIT` message.
///
/// C: com.h:520 (m_type) + com.h:547-551 (VFS_PM_ENDPT=m7_i1 /
/// VFS_PM_SLOT=m7_i2 / VFS_PM_PID=m7_i3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VfsPmInit {
    /// Process slot number (proc_nr of the boot image).
    pub slot: i32,
    /// Process PID.
    pub pid: Pid,
    /// Process endpoint.
    pub endpoint: Endpoint,
}

impl VfsPmInit {
    /// Encode as an IPC message (`MessageM7`, fields aligned with C's mess_7).
    pub fn encode(&self) -> Message {
        let m7 = MessageM7 {
            m7i1: self.endpoint.get(),
            m7i2: self.slot,
            m7i3: self.pid,
            m7i4: 0,
            m7i5: 0,
            m7p1: 0,
            m7p2: 0,
            _padding: [0; 16],
        };
        // Union construction (naming only one member) and member writes are both safe operations.
        Message {
            m_source: Endpoint::NONE,
            m_type: VFS_PM_INIT,
            m_u: MessageUnion { m_m7: m7 },
        }
    }

    /// Decode from an IPC message.
    ///
    /// Only `m_type` and the slot range are validated; the endpoint is preserved
    /// as-is (the NONE terminator is judged by the handshake logic, equivalent to
    /// C's `if (NONE == mess.VFS_PM_ENDPT) break;`).
    pub fn decode(msg: &Message) -> Result<Self, VfsPmInitError> {
        if msg.m_type != VFS_PM_INIT {
            return Err(VfsPmInitError::WrongMessageType(msg.m_type));
        }
        // m_type was validated above; reading the union matches C's union semantics.
        let m7 = unsafe { &msg.m_u.m_m7 };
        let slot = m7.m7i2;
        if slot < 0 || slot as usize >= NR_PROCS {
            return Err(VfsPmInitError::SlotOutOfRange(slot));
        }
        Ok(Self {
            slot,
            pid: m7.m7i3,
            endpoint: Endpoint(m7.m7i1),
        })
    }
}

/// `VFS_PM_INIT` decode error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsPmInitError {
    /// `m_type` is not `VFS_PM_INIT`.
    WrongMessageType(i32),
    /// Slot number outside `[0, NR_PROCS)`.
    SlotOutOfRange(i32),
}

#[cfg(test)]
mod vfs_pm_init_tests {
    use super::*;

    #[test]
    fn test_vfs_pm_init_encode_decode_roundtrip() {
        let init = VfsPmInit {
            slot: 11,
            pid: 42,
            endpoint: Endpoint::from_generation_slot(1, 11),
        };
        let msg = init.encode();
        assert_eq!(msg.m_type, VFS_PM_INIT);

        let decoded = VfsPmInit::decode(&msg).unwrap();
        assert_eq!(decoded, init);
    }

    #[test]
    fn test_vfs_pm_init_decode_none_terminator() {
        // Final barrier: endpoint = NONE (peer semantics at com.h:428-431).
        let msg = VfsPmInit {
            slot: 0,
            pid: 0,
            endpoint: Endpoint::NONE,
        }
        .encode();
        let decoded = VfsPmInit::decode(&msg).unwrap();
        assert!(decoded.endpoint.is_none());
    }

    #[test]
    fn test_vfs_pm_init_decode_wrong_type() {
        let msg = Message {
            m_type: VFS_PM_INIT + 1,
            ..Message::default()
        };
        let err = VfsPmInit::decode(&msg).unwrap_err();
        assert_eq!(err, VfsPmInitError::WrongMessageType(VFS_PM_INIT + 1));
    }

    #[test]
    fn test_vfs_pm_init_decode_slot_out_of_range() {
        let msg = VfsPmInit {
            slot: NR_PROCS as i32 + 5,
            pid: 1,
            endpoint: Endpoint::PM,
        }
        .encode();
        let err = VfsPmInit::decode(&msg).unwrap_err();
        assert_eq!(err, VfsPmInitError::SlotOutOfRange(NR_PROCS as i32 + 5));
    }

    #[test]
    fn test_vfs_pm_init_constants() {
        assert_eq!(VFS_PM_RQ_BASE, 0x900);
        assert_eq!(VFS_PM_INIT, 0x900);
    }
}

// ── PM → VFS runtime requests (excluding boot-time VFS_PM_INIT) ──
//
// C: com.h:521-531 (11 request kinds) + com.h:547-583 (mess_7 field layout).
// The same union slot is reused across requests (e.g. VFS_PM_STATUS / VFS_PM_PENDPT /
// VFS_PM_GROUP_NO / VFS_PM_EID / VFS_PM_SLOT / VFS_PM_TERM_SIG are all m7_i2), so
// typed fields exclude mismatches at compile time (see 04-stage-pm/05-design.v1.md D2).

/// PM → VFS runtime request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsCall {
    /// VFS_PM_SETUID — replace the process's effective/real UID (getset.c:121).
    SetUid {
        /// Target process endpoint (VFS_PM_ENDPT = m7_i1).
        endpoint: Endpoint,
        /// Effective UID (VFS_PM_EID = m7_i2).
        eid: i32,
        /// Real UID (VFS_PM_RID = m7_i3).
        rid: i32,
    },
    /// VFS_PM_SETGID — replace the effective/real GID (getset.c:136).
    SetGid {
        /// Target process endpoint (VFS_PM_ENDPT = m7_i1).
        endpoint: Endpoint,
        /// Effective GID (VFS_PM_EID = m7_i2).
        eid: i32,
        /// Real GID (VFS_PM_RID = m7_i3).
        rid: i32,
    },
    /// VFS_PM_SETGROUPS — replace the supplementary groups (getset.c:151).
    SetGroups {
        /// Target process endpoint (VFS_PM_ENDPT = m7_i1).
        endpoint: Endpoint,
        /// Group count (VFS_PM_GROUP_NO = m7_i2).
        group_no: i32,
        /// Address of the group array (VFS_PM_GROUP_ADDR = m7_p1).
        group_addr: u64,
    },
    /// VFS_PM_SETSID — create a new session (getset.c:199).
    SetSid {
        /// Target process endpoint (VFS_PM_ENDPT = m7_i1).
        endpoint: Endpoint,
    },
    /// VFS_PM_EXEC — load an executable image (exec.c:44).
    Exec {
        /// Target process endpoint (VFS_PM_ENDPT = m7_i1).
        endpoint: Endpoint,
        /// Executable path (VFS_PM_PATH = m7_p1).
        path: u64,
        /// Path length (including NUL, VFS_PM_PATH_LEN = m7_i2).
        path_len: i32,
        /// Argument and environment frame (VFS_PM_FRAME = m7_p2).
        frame: u64,
        /// Frame size (VFS_PM_FRAME_LEN = m7_i3).
        frame_len: i32,
        /// ps_strings pointer (VFS_PM_PS_STR). Under LP64 this is a full 64-bit user
        /// pointer (x86-64 user-stack ps_strings lives in `0x0000_7fff_xxxx_xxxx`,
        /// 47 bits); C in the i386 era stored it in `m7_i5` (lossless for a 32-bit
        /// pointer), and the LP64 port must preserve 64 bits — so encoding splits it
        /// across `m7_i4` (high 32 bits) + `m7_i5` (low 32 bits).
        ps_str: u64,
    },
    /// VFS_PM_EXIT — process exit (forkexit.c:351).
    Exit {
        /// Target process endpoint (VFS_PM_ENDPT = m7_i1).
        endpoint: Endpoint,
    },
    /// VFS_PM_DUMPCORE — process core dump (forkexit.c:351).
    ///
    /// C's `VFS_PM_PATH` (com.h:562 = m7_p1) is a pointer to PM's `mp_name`, which
    /// VFS later fetches with safecopy (forkexit.c:357
    /// `m.VFS_PM_PATH = rmp->mp_name`). This line's internal kernel protocol instead
    /// carries the name **by value**: the name goes in payload tail offset 40..56
    /// (the upper half of `MessageM7`'s trailing pad; 36..40 stays zero) and the
    /// length goes in the m7_i3 slot. The PM side guarantees `name_len` ≤ 16
    /// (C mproc.h `PM_NAME_LEN`); the decode side copies a fixed-size `[u8; 16]`.
    DumpCore {
        /// Target process endpoint (VFS_PM_ENDPT = m7_i1).
        endpoint: Endpoint,
        /// Termination signal (VFS_PM_TERM_SIG = m7_i2).
        term_sig: i32,
        /// Process name byte count (without NUL, ≤ 16; m7_i3 slot).
        name_len: u32,
        /// Process name copied by value (NUL padded; payload offset 40..56).
        name: [u8; 16],
    },
    /// VFS_PM_FORK — duplicate the parent's fd table (forkexit.c:123).
    Fork {
        /// **Child** process endpoint (VFS_PM_ENDPT = m7_i1).
        child: Endpoint,
        /// Parent process endpoint (VFS_PM_PENDPT = m7_i2).
        parent: Endpoint,
        /// Child process PID (VFS_PM_CPID = m7_i3).
        child_pid: Pid,
    },
    /// VFS_PM_SRV_FORK — service process fork (forkexit.c:223).
    SrvFork {
        /// **Child** process endpoint (VFS_PM_ENDPT = m7_i1).
        child: Endpoint,
        /// Parent process endpoint (VFS_PM_PENDPT = m7_i2).
        parent: Endpoint,
        /// Child process PID (VFS_PM_CPID = m7_i3).
        child_pid: Pid,
        /// Real/effective UID (VFS_PM_REUID = m7_i4).
        reuid: i32,
        /// Real/effective GID (VFS_PM_REGID = m7_i5).
        regid: i32,
    },
    /// VFS_PM_UNPAUSE — unpause a process (signal.c:764).
    Unpause {
        /// Target process endpoint (VFS_PM_ENDPT = m7_i1).
        endpoint: Endpoint,
    },
    /// VFS_PM_REBOOT — system reboot (misc.c:228).
    ///
    /// The only request not associated with any user process (comment at com.h:301-302).
    Reboot,
}

impl VfsCall {
    /// Message type (C: `m_type`).
    pub const fn m_type(&self) -> i32 {
        match self {
            Self::SetUid { .. } => VFS_PM_SETUID,
            Self::SetGid { .. } => VFS_PM_SETGID,
            Self::SetGroups { .. } => VFS_PM_SETGROUPS,
            Self::SetSid { .. } => VFS_PM_SETSID,
            Self::Exec { .. } => VFS_PM_EXEC,
            Self::Exit { .. } => VFS_PM_EXIT,
            Self::DumpCore { .. } => VFS_PM_DUMPCORE,
            Self::Fork { .. } => VFS_PM_FORK,
            Self::SrvFork { .. } => VFS_PM_SRV_FORK,
            Self::Unpause { .. } => VFS_PM_UNPAUSE,
            Self::Reboot => VFS_PM_REBOOT,
        }
    }

    /// Target process endpoint; `Reboot` is not associated with any process and returns `None`.
    pub const fn endpoint(&self) -> Option<Endpoint> {
        match self {
            Self::SetUid { endpoint, .. }
            | Self::SetGid { endpoint, .. }
            | Self::SetGroups { endpoint, .. }
            | Self::SetSid { endpoint }
            | Self::Exec { endpoint, .. }
            | Self::Exit { endpoint }
            | Self::DumpCore { endpoint, .. }
            | Self::Unpause { endpoint } => Some(*endpoint),
            Self::Fork { child, .. } | Self::SrvFork { child, .. } => Some(*child),
            Self::Reboot => None,
        }
    }

    /// Encode as an IPC message (`MessageM7`, field layout aligned with C's mess_7).
    ///
    /// `m_source` is set to `Endpoint::NONE`: the send target is decided by
    /// `IpcTransport::send`'s target argument, equivalent to C
    /// `asynsend3(VFS_PROC_NR, ...)`. `Fork`'s `REUID`/`REGID` slots are explicitly
    /// set to `-1` (forkexit.c:129-130 comment "Not used by VFS_PM_FORK"); other
    /// unused slots are set to 0.
    pub fn encode(&self) -> Message {
        let (i1, i2, i3, i4, i5, p1, p2) = match *self {
            Self::SetUid { endpoint, eid, rid } => (endpoint.get(), eid, rid, 0, 0, 0, 0),
            Self::SetGid { endpoint, eid, rid } => (endpoint.get(), eid, rid, 0, 0, 0, 0),
            Self::SetGroups {
                endpoint,
                group_no,
                group_addr,
            } => (endpoint.get(), group_no, 0, 0, 0, group_addr, 0),
            Self::SetSid { endpoint } => (endpoint.get(), 0, 0, 0, 0, 0, 0),
            Self::Exec {
                endpoint,
                path,
                path_len,
                frame,
                frame_len,
                ps_str,
            } => (
                endpoint.get(),
                path_len,
                frame_len,
                (ps_str >> 32) as i32,
                ps_str as i32,
                path,
                frame,
            ),
            Self::Exit { endpoint } => (endpoint.get(), 0, 0, 0, 0, 0, 0),
            Self::DumpCore {
                endpoint,
                term_sig,
                name_len,
                name: _,
            } => (endpoint.get(), term_sig, name_len as i32, 0, 0, 0, 0),
            Self::Fork {
                child,
                parent,
                child_pid,
            } => (child.get(), parent.get(), child_pid, -1, -1, 0, 0),
            Self::SrvFork {
                child,
                parent,
                child_pid,
                reuid,
                regid,
            } => (child.get(), parent.get(), child_pid, reuid, regid, 0, 0),
            Self::Unpause { endpoint } => (endpoint.get(), 0, 0, 0, 0, 0, 0),
            Self::Reboot => (0, 0, 0, 0, 0, 0, 0),
        };
        let mut m7 = MessageM7 {
            m7i1: i1,
            m7i2: i2,
            m7i3: i3,
            m7i4: i4,
            m7i5: i5,
            m7p1: p1,
            m7p2: p2,
            _padding: [0; 16],
        };
        // DumpCore's by-value name goes in payload tail 40..56 (upper pad half, see
        // the variant docs).
        if let Self::DumpCore { name, .. } = self {
            // The 16-byte name occupies the whole pad [0..16] = payload absolute
            // [40..56] (C's DumpCore by-value name shape); the old layout [4..20] ran
            // out to 60, past the protocol boundary; this one sits exactly inside it.
            m7._padding[0..16].copy_from_slice(name);
        }
        Message {
            m_source: Endpoint::NONE,
            m_type: self.m_type(),
            m_u: MessageUnion { m_m7: m7 },
        }
    }

    /// Decode from an IPC message (PM → VFS control-plane entry, the payload half
    /// of `service_pm`).
    ///
    /// Inverse of [`Self::encode`]: field order matches C's `mess_7` slots (the
    /// `VFS_PM_PATH`/`VFS_PM_FRAME` names at com.h:560-575 are the encode-side
    /// annotation table). It first checks membership in the RQ family (C: `main.c`
    /// enters `service_pm` on `who_e == PM_PROC_NR`; the family check adds this
    /// guard on the payload side); outside the family returns
    /// [`VfsCallError::NotACall`], inside the family but with no variant returns
    /// [`VfsCallError::UnknownCall`].
    ///
    /// `VFS_PM_INIT` is not decoded here: the process table exchange goes through
    /// [`VfsPmInit`] (a handshake-only payload; see its `decode`).
    pub fn decode(msg: &Message) -> Result<Self, VfsCallError> {
        if !is_vfs_pm_rq(msg.m_type) || msg.m_type == VFS_PM_INIT {
            return Err(VfsCallError::NotACall(msg.m_type));
        }
        // m_type was validated as belonging to the RQ family; reading the union matches C's union semantics.
        let m7 = unsafe { &msg.m_u.m_m7 };
        Ok(match msg.m_type {
            VFS_PM_SETUID => Self::SetUid {
                endpoint: Endpoint(m7.m7i1),
                eid: m7.m7i2,
                rid: m7.m7i3,
            },
            VFS_PM_SETGID => Self::SetGid {
                endpoint: Endpoint(m7.m7i1),
                eid: m7.m7i2,
                rid: m7.m7i3,
            },
            VFS_PM_SETGROUPS => Self::SetGroups {
                endpoint: Endpoint(m7.m7i1),
                group_no: m7.m7i2,
                group_addr: m7.m7p1,
            },
            VFS_PM_SETSID => Self::SetSid {
                endpoint: Endpoint(m7.m7i1),
            },
            VFS_PM_EXEC => Self::Exec {
                endpoint: Endpoint(m7.m7i1),
                path: m7.m7p1,
                path_len: m7.m7i2,
                frame: m7.m7p2,
                frame_len: m7.m7i3,
                ps_str: ((m7.m7i4 as u32 as u64) << 32) | (m7.m7i5 as u32 as u64),
            },
            VFS_PM_EXIT => Self::Exit {
                endpoint: Endpoint(m7.m7i1),
            },
            VFS_PM_DUMPCORE => Self::DumpCore {
                endpoint: Endpoint(m7.m7i1),
                term_sig: m7.m7i2,
                name_len: m7.m7i3 as u32,
                name: {
                    let mut name = [0u8; 16];
                    name.copy_from_slice(&m7._padding[0..16]);
                    name
                },
            },
            VFS_PM_FORK => Self::Fork {
                child: Endpoint(m7.m7i1),
                parent: Endpoint(m7.m7i2),
                child_pid: m7.m7i3,
            },
            VFS_PM_SRV_FORK => Self::SrvFork {
                child: Endpoint(m7.m7i1),
                parent: Endpoint(m7.m7i2),
                child_pid: m7.m7i3,
                reuid: m7.m7i4,
                regid: m7.m7i5,
            },
            VFS_PM_UNPAUSE => Self::Unpause {
                endpoint: Endpoint(m7.m7i1),
            },
            VFS_PM_REBOOT => Self::Reboot,
            _ => return Err(VfsCallError::UnknownCall(msg.m_type)),
        })
    }
}

/// `VfsCall::decode` error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsCallError {
    /// `m_type` is not in the RQ request family (or is the handshake-only `VFS_PM_INIT`).
    NotACall(i32),
    /// `m_type` is in the RQ family but has no matching variant.
    UnknownCall(i32),
}

/// VFS → PM reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsReply {
    /// VFS_PM_SETUID_REPLY (main.c:335 grouping).
    SetUid,
    /// VFS_PM_SETGID_REPLY.
    SetGid,
    /// VFS_PM_SETGROUPS_REPLY.
    SetGroups,
    /// VFS_PM_SETSID_REPLY.
    SetSid,
    /// VFS_PM_EXEC_REPLY — carries the new program entry and stack (main.c:398-407).
    Exec {
        /// Execution status (OK or failure, VFS_PM_STATUS = m7_i2).
        status: i32,
        /// Program counter (VFS_PM_PC = m7_p1).
        pc: u64,
        /// Possibly updated stack pointer (VFS_PM_NEWSP = m7_p2).
        newsp: u64,
        /// Possibly updated ps_strings pointer (VFS_PM_NEWPS_STR). Full 64 bits under
        /// LP64; encoding splits it into `m7_i4` (high) + `m7_i5` (low) (see
        /// `VfsCall::Exec::ps_str`).
        newps_str: u64,
    },
    /// VFS_PM_CORE_REPLY — carries core dump status (main.c:409-415).
    Core {
        /// Status (OK or failure, VFS_PM_STATUS = m7_i2).
        status: i32,
    },
    /// VFS_PM_EXIT_REPLY (main.c:419-421).
    Exit,
    /// VFS_PM_FORK_REPLY (main.c:372-396).
    Fork,
    /// VFS_PM_SRV_FORK_REPLY (main.c:372-396).
    SrvFork,
    /// VFS_PM_UNPAUSE_REPLY (falls through the tail after main.c:417).
    Unpause,
    /// VFS_PM_REBOOT_REPLY (special case at main.c:304-312).
    Reboot,
}

impl VfsReply {
    /// Message type (C: `m_type`).
    pub const fn m_type(&self) -> i32 {
        match self {
            Self::SetUid => VFS_PM_SETUID_REPLY,
            Self::SetGid => VFS_PM_SETGID_REPLY,
            Self::SetGroups => VFS_PM_SETGROUPS_REPLY,
            Self::SetSid => VFS_PM_SETSID_REPLY,
            Self::Exec { .. } => VFS_PM_EXEC_REPLY,
            Self::Core { .. } => VFS_PM_CORE_REPLY,
            Self::Exit => VFS_PM_EXIT_REPLY,
            Self::Fork => VFS_PM_FORK_REPLY,
            Self::SrvFork => VFS_PM_SRV_FORK_REPLY,
            Self::Unpause => VFS_PM_UNPAUSE_REPLY,
            Self::Reboot => VFS_PM_REBOOT_REPLY,
        }
    }

    /// Decode from an IPC message.
    ///
    /// First checks membership in the RS family (C: the `IS_VFS_PM_RS` family check
    /// at `main.c:84` guarantees only RS-family messages reach `handle_vfs_reply`);
    /// `NotAReply` covers out-of-family `m_type`, `UnknownReply` covers undefined
    /// numbers inside the family (matching C `default: panic("unknown
    /// reply code")`).
    pub fn decode(msg: &Message) -> Result<Self, VfsReplyError> {
        if !is_vfs_pm_rs(msg.m_type) {
            return Err(VfsReplyError::NotAReply(msg.m_type));
        }
        // m_type was validated as RS-family above; reading the union matches C's union semantics.
        let m7 = unsafe { &msg.m_u.m_m7 };
        Ok(match msg.m_type {
            VFS_PM_SETUID_REPLY => Self::SetUid,
            VFS_PM_SETGID_REPLY => Self::SetGid,
            VFS_PM_SETGROUPS_REPLY => Self::SetGroups,
            VFS_PM_SETSID_REPLY => Self::SetSid,
            VFS_PM_EXEC_REPLY => Self::Exec {
                status: m7.m7i2,
                pc: m7.m7p1,
                newsp: m7.m7p2,
                newps_str: ((m7.m7i4 as u32 as u64) << 32) | (m7.m7i5 as u32 as u64),
            },
            VFS_PM_CORE_REPLY => Self::Core { status: m7.m7i2 },
            VFS_PM_EXIT_REPLY => Self::Exit,
            VFS_PM_FORK_REPLY => Self::Fork,
            VFS_PM_SRV_FORK_REPLY => Self::SrvFork,
            VFS_PM_UNPAUSE_REPLY => Self::Unpause,
            VFS_PM_REBOOT_REPLY => Self::Reboot,
            _ => return Err(VfsReplyError::UnknownReply(msg.m_type)),
        })
    }

    /// Encode as an IPC message (the format PM's `handle_vfs_reply` consumes;
    /// inverse of [`Self::decode`]).
    ///
    /// C `reply(who_e, result)` is a bare reply (`memset(&m, 0, …)` then write
    /// only `m_type`); the two payload-carrying paths fill slots per com.h:570-575:
    /// `Exec`'s status/pc/newsp/newps_str and `Core`'s status. `m_source` is set to
    /// `Endpoint::NONE`; the send target is decided by the transport layer.
    pub fn encode(&self) -> Message {
        let mut msg = Message {
            m_source: Endpoint::NONE,
            m_type: self.m_type(),
            m_u: MessageUnion {
                m_m7: MessageM7 {
                    m7i1: 0,
                    m7i2: 0,
                    m7i3: 0,
                    m7i4: 0,
                    m7i5: 0,
                    m7p1: 0,
                    m7p2: 0,
                    _padding: [0; 16],
                },
            },
        };
        match *self {
            Self::Exec {
                status,
                pc,
                newsp,
                newps_str,
            } => {
                let m7 = unsafe { &mut msg.m_u.m_m7 };
                m7.m7i2 = status;
                m7.m7p1 = pc;
                m7.m7p2 = newsp;
                m7.m7i4 = (newps_str >> 32) as i32;
                m7.m7i5 = newps_str as i32;
            }
            Self::Core { status } => {
                let m7 = unsafe { &mut msg.m_u.m_m7 };
                m7.m7i2 = status;
            }
            // C's bare reply: the payload area stays all zero (reply()'s memset semantics).
            Self::SetUid
            | Self::SetGid
            | Self::SetGroups
            | Self::SetSid
            | Self::Exit
            | Self::Fork
            | Self::SrvFork
            | Self::Unpause
            | Self::Reboot => {}
        }
        msg
    }

    /// Encode as an IPC reply that echoes the target endpoint.
    ///
    /// Every reply path in C `service_pm` (main.c) does
    /// `proc_e = m_in.VFS_PM_ENDPT; ... m_out.VFS_PM_ENDPT = proc_e;`
    /// — echoing the request's target process endpoint back into the reply's
    /// `VFS_PM_ENDPT` (`m7_i1`). PM's `handle_vfs_reply` (main.c:315-321) takes the
    /// endpoint from `m_in.VFS_PM_ENDPT`, resolves the slot with `pm_isokendpt`,
    /// then asserts that slot has a `VFS_CALL` attached. [`Self::encode`] is the
    /// bare reply shape (`m7_i1` stays 0); delivering it directly makes PM resolve
    /// slot 0 and `panic("reply without request")`. This method writes
    /// `m7_i1 = target` on top of the bare reply, restoring C's endpoint-echo
    /// contract.
    ///
    /// `target` comes from `VfsCall::endpoint()` (fork/srv_fork use the **child**
    /// endpoint, matching PM's semantics of attaching `VFS_CALL` to the child slot).
    pub fn encode_reply_for(&self, target: Endpoint) -> Message {
        let mut msg = self.encode();
        // SAFETY: `encode` just built the union with the `m_m7` arm; reading and writing that same arm is valid.
        unsafe { msg.m_u.m_m7.m7i1 = target.get() };
        msg
    }
}

/// `VfsReply::decode` / `handle_vfs_reply` error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsReplyError {
    /// `m_type` is not in the RS family (see `is_vfs_pm_rs`).
    NotAReply(i32),
    /// `m_type` is in the RS family but has no matching variant.
    UnknownReply(i32),
    /// The reply's endpoint does not resolve to a valid slot (C: `pm_isokendpt` fails).
    BadEndpoint(Endpoint),
}

#[cfg(test)]
mod vfs_call_reply_tests {
    use super::*;

    /// Round-trip encode → decode for one request path, also checking m_type and family classification.
    fn roundtrip(call: VfsCall) {
        assert!(is_vfs_pm_rq(call.m_type()));
        assert!(!is_vfs_pm_rs(call.m_type()));
        let msg = call.encode();
        assert_eq!(msg.m_type, call.m_type());
        // Apart from REBOOT, the target process endpoint lands in m7_i1.
        if let Some(ep) = call.endpoint() {
            let m7 = unsafe { &msg.m_u.m_m7 };
            assert_eq!(m7.m7i1, ep.get(), "endpoint 必须编码到 m7_i1");
        }
        // encode → decode is fully inverse (the payload half of service_pm).
        assert_eq!(VfsCall::decode(&msg), Ok(call));
    }

    /// `VfsReply::Exec` encode → decode round trip (the format PM's
    /// `handle_vfs_reply` consumes, slots at com.h:570-575).
    #[test]
    fn test_vfs_reply_exec_encode_decode_roundtrip() {
        // newps_str uses an LP64 high address (non-zero high 32 bits) to verify the m7_i4/m7_i5 split round trip.
        let reply = VfsReply::Exec {
            status: 0,
            pc: 0x401_000,
            newsp: 0x7fff_ffff_e000,
            newps_str: 0x0000_7fff_ffff_efe0,
        };
        let msg = reply.encode();
        assert_eq!(msg.m_type, VFS_PM_EXEC_REPLY);
        assert_eq!(VfsReply::decode(&msg), Ok(reply));
        // A bare reply (C reply()'s memset semantics) also round-trips the same way.
        let bare = VfsReply::Fork.encode();
        assert_eq!(VfsReply::decode(&bare), Ok(VfsReply::Fork));
    }

    /// Regression: every `service_pm` reply must echo the target process endpoint
    /// into `VFS_PM_ENDPT` (m7_i1), otherwise PM's `handle_vfs_reply` resolves slot
    /// 0 and `panic("reply without request")`. `encode_reply_for` restores that
    /// contract.
    #[test]
    fn test_vfs_reply_encode_reply_for_echoes_endpoint() {
        let target = Endpoint::from_generation_slot(1, 12); // fork child endpoint
        // Bare encode carries no endpoint (old bug: m7_i1=0 → PM resolves slot 0).
        let bare = VfsReply::Fork.encode();
        assert_eq!(unsafe { bare.m_u.m_m7 }.m7i1, 0);
        // encode_reply_for echoes the endpoint; the payload m_type matches the bare reply.
        let reply = VfsReply::Fork.encode_reply_for(target);
        assert_eq!(reply.m_type, VFS_PM_FORK_REPLY);
        assert_eq!(unsafe { reply.m_u.m_m7 }.m7i1, target.get());
        // Exec's extra payload (status/pc/newsp) is unaffected; m7_i1 remains the endpoint.
        let exec = VfsReply::Exec {
            status: 0,
            pc: 0x401_000,
            newsp: 0x7fff_ffff_e000,
            newps_str: 0,
        }
        .encode_reply_for(target);
        let m7 = unsafe { exec.m_u.m_m7 };
        assert_eq!(m7.m7i1, target.get());
        assert_eq!(m7.m7p1, 0x401_000);
    }

    #[test]
    fn test_vfs_call_decode_rejects_out_of_family() {
        // Out-of-family (the FS request family) and the handshake-only INIT never enter service_pm payload decoding.
        assert_eq!(
            VfsCall::decode(&Message { m_type: 0xA00, ..Message::default() }),
            Err(VfsCallError::NotACall(0xA00))
        );
        assert_eq!(
            VfsCall::decode(&Message {
                m_type: VFS_PM_INIT,
                ..Message::default()
            }),
            Err(VfsCallError::NotACall(VFS_PM_INIT))
        );
        // A number inside the family but undefined.
        let bogus = VFS_PM_RQ_BASE + 12;
        assert!(
            is_vfs_pm_rq(bogus),
            "bogus 必须落在 RQ 族内以测试 UnknownCall 分支"
        );
        assert_eq!(
            VfsCall::decode(&Message { m_type: bogus, ..Message::default() }),
            Err(VfsCallError::UnknownCall(bogus))
        );
    }

    #[test]
    fn test_vfs_call_roundtrip_all() {
        roundtrip(VfsCall::SetUid {
            endpoint: Endpoint::from_generation_slot(1, 5),
            eid: 100,
            rid: 100,
        });
        roundtrip(VfsCall::SetGid {
            endpoint: Endpoint::from_generation_slot(1, 5),
            eid: 200,
            rid: 200,
        });
        roundtrip(VfsCall::SetGroups {
            endpoint: Endpoint::from_generation_slot(1, 5),
            group_no: 3,
            group_addr: 0x8000_1000,
        });
        roundtrip(VfsCall::SetSid {
            endpoint: Endpoint::from_generation_slot(1, 5),
        });
        roundtrip(VfsCall::Exec {
            endpoint: Endpoint::from_generation_slot(1, 5),
            path: 0x4000_0000,
            path_len: 12,
            frame: 0x4000_1000,
            frame_len: 256,
            ps_str: 0x0000_7fff_ffff_efe0,
        });
        roundtrip(VfsCall::Exit {
            endpoint: Endpoint::from_generation_slot(1, 5),
        });
        roundtrip(VfsCall::DumpCore {
            endpoint: Endpoint::from_generation_slot(1, 5),
            term_sig: 11,
            name_len: 4,
            name: {
                let mut n = [0u8; 16];
                n[..4].copy_from_slice(b"init");
                n
            },
        });
        roundtrip(VfsCall::Fork {
            child: Endpoint::from_generation_slot(1, 6),
            parent: Endpoint::from_generation_slot(1, 5),
            child_pid: 1234,
        });
        roundtrip(VfsCall::SrvFork {
            child: Endpoint::from_generation_slot(1, 6),
            parent: Endpoint::from_generation_slot(1, 5),
            child_pid: 1234,
            reuid: 7,
            regid: 8,
        });
        roundtrip(VfsCall::Unpause {
            endpoint: Endpoint::from_generation_slot(1, 5),
        });
        roundtrip(VfsCall::Reboot);
    }

    #[test]
    fn test_vfs_call_fork_sentinels() {
        // VFS_PM_FORK's REUID/REGID slots must be -1 (forkexit.c:129-130).
        let msg = VfsCall::Fork {
            child: Endpoint::from_generation_slot(1, 6),
            parent: Endpoint::from_generation_slot(1, 5),
            child_pid: 1234,
        }
        .encode();
        let m7 = unsafe { &msg.m_u.m_m7 };
        assert_eq!(m7.m7i4, -1);
        assert_eq!(m7.m7i5, -1);
        // The parent endpoint is in m7_i2, the child pid in m7_i3.
        assert_eq!(m7.m7i2, Endpoint::from_generation_slot(1, 5).get());
        assert_eq!(m7.m7i3, 1234);
    }

    #[test]
    fn test_vfs_call_exec_layout() {
        // Regression: under x86-64 LP64 the user ps_strings lives in
        // `0x0000_7fff_xxxx_xxxx` (47 bits) and must travel intact through both
        // `m7_i4` (high 32 bits) and `m7_i5` (low 32 bits); the old `i32` carrier
        // truncated it to `0xffff_efe0`, which PM then sign-extended to
        // `0xffff_ffff_ffff_efe0`.
        let ps: u64 = 0x0000_7fff_ffff_efe0;
        let msg = VfsCall::Exec {
            endpoint: Endpoint::from_generation_slot(1, 5),
            path: 0x4000_0000,
            path_len: 12,
            frame: 0x4000_1000,
            frame_len: 256,
            ps_str: ps,
        }
        .encode();
        let m7 = unsafe { &msg.m_u.m_m7 };
        assert_eq!(m7.m7i2, 12);
        assert_eq!(m7.m7i3, 256);
        assert_eq!(m7.m7p1, 0x4000_0000);
        assert_eq!(m7.m7p2, 0x4000_1000);
        assert_eq!(m7.m7i4, (ps >> 32) as u32 as i32, "高 32 位落 m7_i4");
        assert_eq!(m7.m7i5, ps as u32 as i32, "低 32 位落 m7_i5");
        match VfsCall::decode(&msg).unwrap() {
            VfsCall::Exec { ps_str, .. } => {
                assert_eq!(ps_str, ps, "B43: 64 位 ps_str 无损往返");
            }
            _ => panic!("decode 不匹配 Exec"),
        }
    }

    /// Byte layout of DumpCore's by-value name: the name lands in payload tail
    /// 40..56 (the whole `_padding[0..16]`; this is exactly inside the 56-byte
    /// protocol boundary, whereas the old layout [4..20] = absolute 44..60 ran past
    /// it), the length lands in m7_i3, and the pointer slot m7_p1 stays zero (the
    /// by-value carrier replaces C's m7_p1 pointer plus VFS safecopy fetch).
    #[test]
    fn test_vfs_call_dumpcore_by_value_name_layout() {
        let mut name = [0u8; 16];
        name[..9].copy_from_slice(b"getty.bin");
        let msg = VfsCall::DumpCore {
            endpoint: Endpoint::from_generation_slot(1, 5),
            term_sig: 11,
            name_len: 9,
            name,
        }
        .encode();
        let m7 = unsafe { &msg.m_u.m_m7 };
        assert_eq!(m7.m7i3, 9, "name_len 必须编码到 m7_i3");
        assert_eq!(m7.m7p1, 0, "指针槽必须清零（按值协议不再用 m7_p1）");
        assert_eq!(&m7._padding[0..9], b"getty.bin", "名字落载荷绝对 40..49");
        assert_eq!(m7._padding[9], 0, "名字 NUL 填充");
        assert_eq!(&m7._padding[10..16], &[0; 6], "名字后补零到协议边界 56");
        assert_eq!(
            VfsCall::decode(&msg).unwrap(),
            VfsCall::DumpCore {
                endpoint: Endpoint::from_generation_slot(1, 5),
                term_sig: 11,
                name_len: 9,
                name,
            }
        );
    }

    #[test]
    fn test_vfs_call_reboot_has_no_endpoint() {
        assert_eq!(VfsCall::Reboot.endpoint(), None);
        let msg = VfsCall::Reboot.encode();
        assert_eq!(msg.m_type, VFS_PM_REBOOT);
        let m7 = unsafe { &msg.m_u.m_m7 };
        assert_eq!(m7.m7i1, 0);
    }

    #[test]
    fn test_vfs_reply_decode_all() {
        let mk = |m_type: i32| Message {
            m_type,
            ..Message::default()
        };
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SETUID_REPLY)).unwrap(),
            VfsReply::SetUid
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SETGID_REPLY)).unwrap(),
            VfsReply::SetGid
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SETGROUPS_REPLY)).unwrap(),
            VfsReply::SetGroups
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SETSID_REPLY)).unwrap(),
            VfsReply::SetSid
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_EXIT_REPLY)).unwrap(),
            VfsReply::Exit
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_FORK_REPLY)).unwrap(),
            VfsReply::Fork
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_SRV_FORK_REPLY)).unwrap(),
            VfsReply::SrvFork
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_UNPAUSE_REPLY)).unwrap(),
            VfsReply::Unpause
        );
        assert_eq!(
            VfsReply::decode(&mk(VFS_PM_REBOOT_REPLY)).unwrap(),
            VfsReply::Reboot
        );
    }

    #[test]
    fn test_vfs_reply_decode_exec_payload() {
        let mut msg = Message {
            m_type: VFS_PM_EXEC_REPLY,
            ..Message::default()
        };
        let m7 = unsafe { &mut msg.m_u.m_m7 };
        m7.m7i2 = 0; // status OK
        m7.m7p1 = 0x1000;
        m7.m7p2 = 0x2000;
        m7.m7i5 = 0x3000;
        assert_eq!(
            VfsReply::decode(&msg).unwrap(),
            VfsReply::Exec {
                status: 0,
                pc: 0x1000,
                newsp: 0x2000,
                newps_str: 0x3000,
            }
        );
    }

    #[test]
    fn test_vfs_reply_decode_core_status() {
        let mut msg = Message {
            m_type: VFS_PM_CORE_REPLY,
            ..Message::default()
        };
        let m7 = unsafe { &mut msg.m_u.m_m7 };
        m7.m7i2 = -1;
        assert_eq!(
            VfsReply::decode(&msg).unwrap(),
            VfsReply::Core { status: -1 }
        );
    }

    #[test]
    fn test_vfs_reply_decode_not_a_reply() {
        // Neither request-family messages nor unknown numbers should pass decode.
        assert_eq!(
            VfsReply::decode(&Message {
                m_type: VFS_PM_EXEC,
                ..Message::default()
            })
            .unwrap_err(),
            VfsReplyError::NotAReply(VFS_PM_EXEC)
        );
        let bogus = VFS_PM_RS_BASE + 12; // inside the family but no variant
        assert!(
            is_vfs_pm_rs(bogus),
            "bogus 必须落在 RS 族内以测试 UnknownReply 分支"
        );
        assert_eq!(
            VfsReply::decode(&Message {
                m_type: bogus,
                ..Message::default()
            })
            .unwrap_err(),
            VfsReplyError::UnknownReply(bogus)
        );
    }

    #[test]
    fn test_vfs_pm_family_predicates() {
        assert!(is_vfs_pm_rq(VFS_PM_INIT));
        assert!(is_vfs_pm_rq(VFS_PM_EXEC));
        assert!(is_vfs_pm_rs(VFS_PM_EXEC_REPLY));
        // Boundary: 0x980 + 0x80 = 0xA00 must not hit the RS family (the low 7 bits are masked off).
        assert!(!is_vfs_pm_rs(VFS_PM_RS_BASE + 0x80));
        // Ordinary PM calls (e.g. PM_GETPROCNR = 0~) are in neither family.
        assert!(!is_vfs_pm_rq(0));
    }

    #[test]
    fn test_vfs_pm_constants_match_com_h() {
        // Locks the wire protocol values to C com.h:520-544 exactly, preventing
        // offset drift (an enumeration order differing from C's macro numbers yields
        // wrong wire values — a wire-value defect).
        // RQ family (com.h:521-531)
        assert_eq!(VFS_PM_SETUID, 0x901);
        assert_eq!(VFS_PM_SETGID, 0x902);
        assert_eq!(VFS_PM_SETSID, 0x903);
        assert_eq!(VFS_PM_EXIT, 0x904);
        assert_eq!(VFS_PM_DUMPCORE, 0x905);
        assert_eq!(VFS_PM_EXEC, 0x906);
        assert_eq!(VFS_PM_FORK, 0x907);
        assert_eq!(VFS_PM_SRV_FORK, 0x908);
        assert_eq!(VFS_PM_UNPAUSE, 0x909);
        assert_eq!(VFS_PM_REBOOT, 0x90a);
        assert_eq!(VFS_PM_SETGROUPS, 0x90b);
        // RS family (com.h:534-544)
        assert_eq!(VFS_PM_SETUID_REPLY, 0x981);
        assert_eq!(VFS_PM_SETGID_REPLY, 0x982);
        assert_eq!(VFS_PM_SETSID_REPLY, 0x983);
        assert_eq!(VFS_PM_EXIT_REPLY, 0x984);
        assert_eq!(VFS_PM_CORE_REPLY, 0x985);
        assert_eq!(VFS_PM_EXEC_REPLY, 0x986);
        assert_eq!(VFS_PM_FORK_REPLY, 0x987);
        assert_eq!(VFS_PM_SRV_FORK_REPLY, 0x988);
        assert_eq!(VFS_PM_UNPAUSE_REPLY, 0x989);
        assert_eq!(VFS_PM_REBOOT_REPLY, 0x98a);
        assert_eq!(VFS_PM_SETGROUPS_REPLY, 0x98b);
    }
}
