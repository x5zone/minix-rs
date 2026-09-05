//! IPC server message types.
//!
//! # Architecture: Transport Layer + Semantic Layer
//!
//! Minix3 uses a 56-byte `message` union for all IPC. The IPC server speaks
//! seven request shapes (`mess_lc_ipc_*`, `minix/include/minix/ipc.h:355-422`)
//! plus the process-event shape (`mess_pm_lsys_proc_event`, `ipc.h:1802-1813`).
//!
//! This module provides two layers, mirroring `vm.rs`:
//!
//! **Transport Layer** (`message.rs`):
//! `MessLcIpcSemget` etc. — `#[repr(C)]` binary-compatible with the C
//! structs. Zero-cost reinterpret of the 56-byte payload.
//!
//! **Semantic Layer** (this file):
//! Per-call `In`/`Out` types (e.g. `IpcSemgetIn`, `IpcSemgetOut`) —
//! type-safe views decoded from the transport layer. The caller endpoint
//! always comes from `m_source` (kernel-set, spoof-proof), never from the
//! payload — payload fields that look like endpoints are untrusted.
//!
//! # Naming Convention
//!
//! - `IpcXxxIn`  = request received by the IPC server (user → IPC)
//! - `IpcXxxOut` = reply sent by the IPC server (IPC → user)
//!
//! Behaviour (dispatch, table management, attach semantics) lives in
//! `minix-ipc-server` (documents 01/05-08); only the wire verdicts live here.
//!
//! [ARCH: IPC-02-01] These seven message shapes are new to `minix-types`
//! (no `ipc.rs` existed before); the wire layout matches C 1:1 while the
//! request/response split is a Rust-side semantic reshape. See document
//! `02-ipc-message-contract.md` for the behaviour reference point.

use crate::VirBytes;
use crate::ipc::Message;
use crate::types::Endpoint;

// ============================================================================
// IPC Call Numbers
// ============================================================================
// Defined in Minix3: minix/include/minix/com.h:785-796

/// Base value for IPC server request message types.
pub const IPC_BASE: i32 = 0xD00;

/// Create a shared memory segment. C: `IPC_SHMGET` — com.h:788.
pub const IPC_SHMGET: i32 = IPC_BASE + 1;

/// Attach a shared memory segment. C: `IPC_SHMAT` — com.h:789.
pub const IPC_SHMAT: i32 = IPC_BASE + 2;

/// Detach a shared memory segment. C: `IPC_SHMDT` — com.h:790.
pub const IPC_SHMDT: i32 = IPC_BASE + 3;

/// Control a shared memory segment. C: `IPC_SHMCTL` — com.h:791.
pub const IPC_SHMCTL: i32 = IPC_BASE + 4;

/// Create a semaphore set. C: `IPC_SEMGET` — com.h:794.
pub const IPC_SEMGET: i32 = IPC_BASE + 5;

/// Control a semaphore set. C: `IPC_SEMCTL` — com.h:795.
pub const IPC_SEMCTL: i32 = IPC_BASE + 6;

/// Operate on semaphores. C: `IPC_SEMOP` — com.h:796.
pub const IPC_SEMOP: i32 = IPC_BASE + 7;

/// Total number of IPC calls.
pub const NR_IPC_CALLS: i32 = 7;

/// Suspend marker: "no reply now, the caller stays blocked".
///
/// C: `SUSPEND -998` — com.h:1151. Not an error code (error codes are
/// positive `errno.h` values); the main loop skips the reply when a handler
/// returns it, and a later event completes the call with a real reply.
/// Only the semaphore-wait path produces it today (document 06).
pub const SUSPEND: i32 = -998;

// --- System V limits (decoding guards) ---

/// Maximum operations per `semop` call. C: `SEMOPM 100` — sys/sem.h:184.
pub const SEMOPM: usize = 100;

// ============================================================================
// System V permission bits, commands, and set limits
// ============================================================================
// Defined in Minix3: sys/sys/ipc.h:85-101 (permission bits and control
// commands), sys/sys/sem.h:81-87/93/154/163-184/215-216 (semaphore commands,
// maximum value, allocation bit, table limits, info subtypes).
// Documents 04/05/06 share these; they live here (not per document) so the
// three permission/table/operation modules read the same values.

/// Read permission bit. C: `IPC_R 000400` — sys/ipc.h:85.
pub const IPC_R: u32 = 0o400;

/// Write/alter permission bit. C: `IPC_W 000200` — sys/ipc.h:86.
pub const IPC_W: u32 = 0o200;

/// Control-information permission bit. C: `IPC_M 010000` — sys/ipc.h:87.
pub const IPC_M: u32 = 0o10000;

/// Create entry if key does not exist. C: `IPC_CREAT 001000` — sys/ipc.h:90.
pub const IPC_CREAT: i32 = 0o1000;

/// Fail if key exists (with `IPC_CREAT`). C: `IPC_EXCL 002000` — sys/ipc.h:91.
pub const IPC_EXCL: i32 = 0o2000;

/// Error instead of waiting. C: `IPC_NOWAIT 004000` — sys/ipc.h:92.
pub const IPC_NOWAIT: i32 = 0o4000;

/// Private key: always creates a new object. C: `IPC_PRIVATE (key_t)0` — sys/ipc.h:94.
pub const IPC_PRIVATE: i32 = 0;

/// Remove identifier. C: `IPC_RMID 0` — sys/ipc.h:96.
pub const IPC_RMID: i32 = 0;

/// Set options. C: `IPC_SET 1` — sys/ipc.h:97.
pub const IPC_SET: i32 = 1;

/// Get options. C: `IPC_STAT 2` — sys/ipc.h:98.
pub const IPC_STAT: i32 = 2;

/// Read module information (Minix3 extension for `ipcs`). C: `IPC_INFO 500` — sys/ipc.h:101.
pub const IPC_INFO: i32 = 500;

/// Permission bits kept from the creation flag. C: `ACCESSPERMS 0777` — sys/stat.h:189.
pub const ACCESSPERMS: u32 = 0o777;

/// Return semaphore wait-for-increase count. C: `GETNCNT 3` — sys/sem.h:81.
pub const GETNCNT: i32 = 3;

/// Return last-operation process id. C: `GETPID 4` — sys/sem.h:82.
pub const GETPID: i32 = 4;

/// Return semaphore value. C: `GETVAL 5` — sys/sem.h:83.
pub const GETVAL: i32 = 5;

/// Return all semaphore values. C: `GETALL 6` — sys/sem.h:84.
pub const GETALL: i32 = 6;

/// Return semaphore wait-for-zero count. C: `GETZCNT 7` — sys/sem.h:85.
pub const GETZCNT: i32 = 7;

/// Set semaphore value. C: `SETVAL 8` — sys/sem.h:86.
pub const SETVAL: i32 = 8;

/// Set all semaphore values. C: `SETALL 9` — sys/sem.h:87.
pub const SETALL: i32 = 9;

/// Read by slot index (with identifier reply). C: `SEM_STAT 18` — sys/sem.h:215.
pub const SEM_STAT: i32 = 18;

/// Read module information. C: `SEM_INFO 19` — sys/sem.h:216.
pub const SEM_INFO: i32 = 19;

/// Semaphore maximum value. C: `SEMVMX 32767` — sys/sem.h:93.
pub const SEMVMX: u32 = 32767;

/// Undo-on-exit flag (explicitly unsupported: plan A-7). C: `SEM_UNDO 010000` — sys/sem.h:76.
pub const SEM_UNDO: i32 = 0o10000;

/// Mode bit: slot holds a live set. C: `SEM_ALLOC 01000` — sys/sem.h:154.
pub const SEM_ALLOC: u32 = 0o1000;

/// Maximum semaphore identifiers. C: `SEMMNI 10` — sys/sem.h:164.
pub const SEMMNI: usize = 10;

/// Maximum semaphores per identifier. C: `SEMMSL SEMMNS` — sys/sem.h:181.
pub const SEMMSL: usize = 60;

/// Maximum semaphores system-wide. C: `SEMMNS 60` — sys/sem.h:166.
pub const SEMMNS: usize = 60;

/// Sequence-number mask (fifteen bits). C: `& 0x7fff` — sem.c:135.
pub const SEM_SEQ_MASK: u32 = 0x7fff;

// ============================================================================
// Shared-memory flags, commands, and limits
// ============================================================================
// Defined in Minix3: sys/sys/shm.h:76-77 (attach flags), :205-206 (info
// subtypes), :218-223 (table limits and flags). Document 02 §2.6 lists
// them; documents 07/08 consume them.

/// Attach read-only. C: `SHM_RDONLY 010000` — sys/shm.h:76.
pub const SHM_RDONLY: i32 = 0o10000;

/// Round attach address. C: `SHM_RND 020000` — sys/shm.h:77.
pub const SHM_RND: i32 = 0o20000;

/// Destroy on last detach. C: `SHM_DEST 0x0400` — sys/shm.h:222.
pub const SHM_DEST: u32 = 0x0400;

/// Slot-in-use marker, private to the server (synced with the NetBSD
/// kernel value). C: `SHM_ALLOC 0x0800` — servers/ipc/shm.c:4.
pub const SHM_ALLOC: u32 = 0x0800;

/// Read by slot index (with identifier reply). C: `SHM_STAT 13` — sys/shm.h:205.
pub const SHM_STAT: i32 = 13;

/// Read module information. C: `SHM_INFO 14` — sys/shm.h:206.
pub const SHM_INFO: i32 = 14;

/// Maximum shared memory identifiers. C: `SHMMNI 1024` — sys/shm.h:218.
pub const SHMMNI: usize = 1024;

/// Maximum attached segments per process. C: `SHMSEG 32` — sys/shm.h:219.
pub const SHMSEG: usize = 32;

// ============================================================================
// Call-number enum
// ============================================================================

/// The seven IPC server calls, in wire order (shared memory first, then
/// semaphores — the historical `com.h` order, unrelated to dispatch order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcCall {
    /// `IPC_SHMGET` — create a shared memory segment (document 07).
    Shmget,
    /// `IPC_SHMAT` — attach a shared memory segment (document 08).
    Shmat,
    /// `IPC_SHMDT` — detach a shared memory segment (document 08).
    Shmdt,
    /// `IPC_SHMCTL` — control a shared memory segment (document 08).
    Shmctl,
    /// `IPC_SEMGET` — create a semaphore set (document 05).
    Semget,
    /// `IPC_SEMCTL` — control a semaphore set (document 05).
    Semctl,
    /// `IPC_SEMOP` — operate on semaphores (document 06).
    Semop,
}

impl IpcCall {
    /// Decode a raw message type. Returns `None` for anything outside the
    /// seven calls — the main loop maps that to `ENOSYS` (document 01).
    #[inline(always)]
    pub const fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            IPC_SHMGET => Some(Self::Shmget),
            IPC_SHMAT => Some(Self::Shmat),
            IPC_SHMDT => Some(Self::Shmdt),
            IPC_SHMCTL => Some(Self::Shmctl),
            IPC_SEMGET => Some(Self::Semget),
            IPC_SEMCTL => Some(Self::Semctl),
            IPC_SEMOP => Some(Self::Semop),
            _ => None,
        }
    }

    /// Encode back to the wire value.
    #[inline(always)]
    pub const fn to_raw(self) -> i32 {
        match self {
            Self::Shmget => IPC_SHMGET,
            Self::Shmat => IPC_SHMAT,
            Self::Shmdt => IPC_SHMDT,
            Self::Shmctl => IPC_SHMCTL,
            Self::Semget => IPC_SEMGET,
            Self::Semctl => IPC_SEMCTL,
            Self::Semop => IPC_SEMOP,
        }
    }
}

// ============================================================================
// Semantic Layer — Per-Call Request/Reply Types
// ============================================================================

// ---------------------------------------------------------------------------
// IPC_SEMGET (user → IPC, IPC → user)
// ---------------------------------------------------------------------------
// C: mess_lc_ipc_semget { key, nr, flag, retid } — ipc.h:374-380.

/// User → IPC: semaphore-set create request.
///
/// `caller` comes from `m_source` (kernel-set); `key`/`count`/`flag` come
/// from the payload. Corresponds to Minix3 `do_semget` inputs (sem.c:93).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcSemgetIn {
    /// Caller endpoint (from `m_source`, spoof-proof).
    pub caller: Endpoint,
    /// Lookup or create key (`IPC_PRIVATE` = 0 always creates).
    pub key: i32,
    /// Number of semaphores in the set.
    pub count: i32,
    /// Creation flags (`IPC_CREAT`/`IPC_EXCL`/permission bits).
    pub flag: i32,
}

/// IPC → user: semaphore-set create reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcSemgetOut {
    /// New or found set identifier.
    pub id: i32,
}

impl IpcSemgetIn {
    /// Decode an `IPC_SEMGET` request.
    ///
    /// Reads the payload from the dedicated `m_lc_ipc_semget` union member,
    /// matching the C wire format as sent by libc `semget` (which memsets
    /// the message, so `retid` arrives zeroed and is ignored here).
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_ipc_semget` is the active union arm for IPC_SEMGET
        // messages.
        let p = unsafe { msg.m_u.m_lc_ipc_semget };
        Self {
            caller: msg.m_source,
            key: p.key,
            count: p.nr,
            flag: p.flag,
        }
    }
}

// ---------------------------------------------------------------------------
// IPC_SEMCTL (user → IPC, IPC → user)
// ---------------------------------------------------------------------------
// C: mess_lc_ipc_semctl { id, num, cmd, opt, ret } — ipc.h:362-371.

/// User → IPC: semaphore-set control request.
///
/// `opt` is the multi-meaning C field (`vir_bytes opt`): an integer value
/// for `SETVAL`, a user-space address for `GETALL`/`SETALL`/`IPC_STAT`/
/// `IPC_SET`. It is carried as a raw 32-bit value here; each command
/// interprets it (document 05). Corresponds to `do_semctl` inputs (sem.c:469).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcSemctlIn {
    /// Caller endpoint (from `m_source`, spoof-proof).
    pub caller: Endpoint,
    /// Set identifier.
    pub id: i32,
    /// Semaphore index within the set.
    pub number: i32,
    /// Control command.
    pub command: i32,
    /// Command argument (integer or user address, per command).
    pub option: u32,
}

/// IPC → user: semaphore-set control reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcSemctlOut {
    /// Command result.
    pub value: i32,
}

impl IpcSemctlIn {
    /// Decode an `IPC_SEMCTL` request.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_ipc_semctl` is the active union arm for IPC_SEMCTL
        // messages.
        let p = unsafe { msg.m_u.m_lc_ipc_semctl };
        Self {
            caller: msg.m_source,
            id: p.id,
            number: p.num,
            command: p.cmd,
            option: p.opt,
        }
    }
}

// ---------------------------------------------------------------------------
// IPC_SEMOP (user → IPC)
// ---------------------------------------------------------------------------
// C: mess_lc_ipc_semop { id, ops, size } — ipc.h:383-388.

/// User → IPC: semaphore operations request.
///
/// The operation array itself does not fit in 56 bytes, so the message
/// carries its user-space address plus the element count; the server copies
/// the array with the kernel data-copy primitive (document 06).
/// There is no `Out` type: success replies with a status code, suspension
/// replies later (SUSPEND), errors reply with errno.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcSemopIn {
    /// Caller endpoint (from `m_source`, spoof-proof).
    pub caller: Endpoint,
    /// Set identifier.
    pub id: i32,
    /// User-space address of the operation array.
    pub operations_address: u32,
    /// Operation count (protocol cap: `SEMOPM` = 100).
    pub operation_count: u32,
}

impl IpcSemopIn {
    /// Decode an `IPC_SEMOP` request.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_ipc_semop` is the active union arm for IPC_SEMOP
        // messages.
        let p = unsafe { msg.m_u.m_lc_ipc_semop };
        Self {
            caller: msg.m_source,
            id: p.id,
            operations_address: p.ops,
            operation_count: p.size,
        }
    }
}

// ---------------------------------------------------------------------------
// IPC_SHMGET (user → IPC, IPC → user)
// ---------------------------------------------------------------------------
// C: mess_lc_ipc_shmget { key, size, flag, retid } — ipc.h:415-421.

/// User → IPC: shared-memory-segment create request.
///
/// Corresponds to Minix3 `do_shmget` inputs (shm.c:51).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcShmgetIn {
    /// Caller endpoint (from `m_source`, spoof-proof).
    pub caller: Endpoint,
    /// Lookup or create key (`IPC_PRIVATE` = 0 always creates).
    pub key: i32,
    /// Requested segment size in bytes.
    pub size: u64,
    /// Creation flags (`IPC_CREAT`/`IPC_EXCL`/permission bits).
    pub flag: i32,
}

/// IPC → user: shared-memory-segment create reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcShmgetOut {
    /// New or found segment identifier.
    pub id: i32,
}

impl IpcShmgetIn {
    /// Decode an `IPC_SHMGET` request.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_ipc_shmget` is the active union arm for IPC_SHMGET
        // messages.
        let p = unsafe { msg.m_u.m_lc_ipc_shmget };
        Self {
            caller: msg.m_source,
            key: p.key,
            size: p.size as u64,
            flag: p.flag,
        }
    }
}

// ---------------------------------------------------------------------------
// IPC_SHMAT (user → IPC, IPC → user)
// ---------------------------------------------------------------------------
// C: mess_lc_ipc_shmat { id, addr, flag, retaddr } — ipc.h:391-397.

/// User → IPC: shared-memory attach request.
///
/// Corresponds to Minix3 `do_shmat` inputs (shm.c:130).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcShmatIn {
    /// Caller endpoint (from `m_source`, spoof-proof).
    pub caller: Endpoint,
    /// Segment identifier.
    pub id: i32,
    /// Requested attach address (zero lets the server pick).
    pub address: VirBytes,
    /// Attach flags (`SHM_RDONLY`/`SHM_RND`).
    pub flag: i32,
}

/// IPC → user: shared-memory attach reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcShmatOut {
    /// Actual attach address.
    pub attached_address: VirBytes,
}

impl IpcShmatIn {
    /// Decode an `IPC_SHMAT` request.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_ipc_shmat` is the active union arm for IPC_SHMAT
        // messages.
        let p = unsafe { msg.m_u.m_lc_ipc_shmat };
        Self {
            caller: msg.m_source,
            id: p.id,
            address: VirBytes(p.addr as u64),
            flag: p.flag,
        }
    }
}

// ---------------------------------------------------------------------------
// IPC_SHMDT (user → IPC)
// ---------------------------------------------------------------------------
// C: mess_lc_ipc_shmdt { addr } — ipc.h:409-412.

/// User → IPC: shared-memory detach request.
///
/// The whole request is one address — no segment identifier. The address
/// itself locates the attach record (document 08). Success replies with a
/// status code; there is no `Out` type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcShmdtIn {
    /// Caller endpoint (from `m_source`, spoof-proof).
    pub caller: Endpoint,
    /// Attach address to detach.
    pub address: VirBytes,
}

impl IpcShmdtIn {
    /// Decode an `IPC_SHMDT` request.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_ipc_shmdt` is the active union arm for IPC_SHMDT
        // messages.
        let p = unsafe { msg.m_u.m_lc_ipc_shmdt };
        Self {
            caller: msg.m_source,
            address: VirBytes(p.addr as u64),
        }
    }
}

// ---------------------------------------------------------------------------
// IPC_SHMCTL (user → IPC, IPC → user)
// ---------------------------------------------------------------------------
// C: mess_lc_ipc_shmctl { id, cmd, buf, ret } — ipc.h:400-406.

/// User → IPC: shared-memory control request.
///
/// `buffer` is the same pointer-in-message pattern as `semop.ops`: the
/// user-space address of the command data area; the server copies it with
/// the kernel data-copy primitive (document 08).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcShmctlIn {
    /// Caller endpoint (from `m_source`, spoof-proof).
    pub caller: Endpoint,
    /// Segment identifier.
    pub id: i32,
    /// Control command (`IPC_RMID`/`IPC_SET`/`IPC_STAT`).
    pub command: i32,
    /// User-space address of the command data area.
    pub buffer: u32,
}

/// IPC → user: shared-memory control reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcShmctlOut {
    /// Command result.
    pub value: i32,
}

impl IpcShmctlIn {
    /// Decode an `IPC_SHMCTL` request.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_lc_ipc_shmctl` is the active union arm for IPC_SHMCTL
        // messages.
        let p = unsafe { msg.m_u.m_lc_ipc_shmctl };
        Self {
            caller: msg.m_source,
            id: p.id,
            command: p.cmd,
            buffer: p.buf,
        }
    }
}

// ---------------------------------------------------------------------------
// PROC_EVENT (PM → IPC)
// ---------------------------------------------------------------------------
// C: mess_pm_lsys_proc_event { endpt, event } — ipc.h:1802-1813.

/// PM → IPC: process-event notification.
///
/// The sender is always the process manager (checked by the main loop
/// before decoding — document 01); `endpoint` names the process the event
/// happened to, `exited` distinguishes exit from signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcEventIn {
    /// Process the event happened to.
    pub endpoint: Endpoint,
    /// True for exit, false for signal.
    pub exited: bool,
}

impl ProcEventIn {
    /// Process-exit event bit. C: `PROC_EVENT_EXIT 0x01` — syslib.h:292.
    pub const EXIT: u32 = 0x01;
    /// Process-signal event bit. C: `PROC_EVENT_SIGNAL 0x02` — syslib.h:293.
    pub const SIGNAL: u32 = 0x02;

    /// Decode a `PROC_EVENT` message.
    #[inline(always)]
    pub fn decode_message(msg: &Message) -> Self {
        // SAFETY: `m_pm_lsys_proc_event` is the active union arm for
        // PROC_EVENT messages.
        let p = unsafe { msg.m_u.m_pm_lsys_proc_event };
        Self {
            endpoint: Endpoint(p.endpt),
            exited: p.event == Self::EXIT,
        }
    }
}

// ============================================================================
// Privilege surface (ipc.conf)
// ============================================================================
// C: minix3/minix/servers/ipc/ipc.conf (18 lines).

/// Endpoints allowed to send to the IPC server.
///
/// C: the `ipc { … }` block of `ipc.conf`: `SYSTEM USER pm rs log tty ds vm`.
/// `Endpoint::PM` stands for the process manager; the remaining user-side
/// senders (rs/log/tty/ds/vm/user processes) arrive with their own endpoint
/// values and are accepted by the main loop's source checks (document 01).
/// `SYSTEM`/`USER` denote kernel-task and ordinary-user classes rather than
/// single endpoints, so they are documented here, not enumerated.
pub const PRIV_SYSTEM_UMAP: u32 = 14;
/// Kernel privilege: cross-address-space copy. C: `VIRCOPY` — ipc.conf:5.
pub const PRIV_SYSTEM_VIRCOPY: u32 = 15;

/// VM requests the IPC server may issue.
///
/// C: the `vm { … }` block of `ipc.conf`: `REMAP REMAP_RO SHM_UNMAP GETPHYS
/// GETREF`. Each maps to a `VM_*` call number (documents 07/08); the table
/// exists so the privilege surface is reviewable in one place.
pub const PRIV_VM_REQUEST_COUNT: usize = 5;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::message::{
        MessLcIpcSemctl, MessLcIpcSemget, MessLcIpcSemop, MessLcIpcShmat, MessLcIpcShmctl,
        MessLcIpcShmdt, MessLcIpcShmget,
    };

    #[test]
    fn call_numbers_match_c() {
        // C: com.h:785-796 — base 0xD00, seven calls base+1..base+7.
        assert_eq!(IPC_BASE, 0xD00);
        assert_eq!(IPC_SHMGET, 0xD01);
        assert_eq!(IPC_SHMAT, 0xD02);
        assert_eq!(IPC_SHMDT, 0xD03);
        assert_eq!(IPC_SHMCTL, 0xD04);
        assert_eq!(IPC_SEMGET, 0xD05);
        assert_eq!(IPC_SEMCTL, 0xD06);
        assert_eq!(IPC_SEMOP, 0xD07);
        assert_eq!(NR_IPC_CALLS, 7);
        // C: com.h:1151 — SUSPEND is negative, not an errno.
        assert_eq!(SUSPEND, -998);
    }

    #[test]
    fn sysv_constants_match_c() {
        // C: sys/sys/ipc.h:85-101 (permission bits and commands),
        // sys/sys/sem.h:76-87/93/154/163-184/215-216 (undo flag, commands,
        // maximum value, allocation bit, table limits, info subtypes),
        // sys/stat.h:189 (ACCESSPERMS), sem.c:135 (sequence mask).
        assert_eq!((IPC_R, IPC_W, IPC_M), (0o400, 0o200, 0o10000));
        assert_eq!((IPC_CREAT, IPC_EXCL, IPC_NOWAIT), (0o1000, 0o2000, 0o4000));
        assert_eq!(IPC_PRIVATE, 0);
        assert_eq!((IPC_RMID, IPC_SET, IPC_STAT, IPC_INFO), (0, 1, 2, 500));
        assert_eq!((GETNCNT, GETPID, GETVAL, GETALL, GETZCNT), (3, 4, 5, 6, 7));
        assert_eq!((SETVAL, SETALL), (8, 9));
        assert_eq!((SEM_STAT, SEM_INFO), (18, 19));
        assert_eq!(SEMVMX, 32767);
        assert_eq!(SEM_UNDO, 0o10000);
        assert_eq!(SEM_ALLOC, 0o1000);
        assert_eq!((SEMMNI, SEMMSL, SEMMNS, SEMOPM), (10, 60, 60, 100));
        assert_eq!(ACCESSPERMS, 0o777);
        assert_eq!(SEM_SEQ_MASK, 0x7fff);
        assert_eq!((SHM_RDONLY, SHM_RND), (0o10000, 0o20000));
        assert_eq!(SHM_DEST, 0x0400);
        assert_eq!(SHM_ALLOC, 0x0800);
        assert_eq!((SHM_STAT, SHM_INFO), (13, 14));
        assert_eq!((SHMMNI, SHMSEG), (1024, 32));
    }

    #[test]
    fn from_raw_roundtrip() {
        // All seven numbers decode; neighbours do not.
        let calls = [
            (IPC_SHMGET, IpcCall::Shmget),
            (IPC_SHMAT, IpcCall::Shmat),
            (IPC_SHMDT, IpcCall::Shmdt),
            (IPC_SHMCTL, IpcCall::Shmctl),
            (IPC_SEMGET, IpcCall::Semget),
            (IPC_SEMCTL, IpcCall::Semctl),
            (IPC_SEMOP, IpcCall::Semop),
        ];
        for (raw, call) in calls {
            assert_eq!(IpcCall::from_raw(raw), Some(call));
            assert_eq!(call.to_raw(), raw);
        }
        assert_eq!(IpcCall::from_raw(IPC_BASE), None);
        assert_eq!(IpcCall::from_raw(IPC_BASE + 8), None);
        assert_eq!(IpcCall::from_raw(0), None);
    }

    /// Fresh envelope: source and type set, zeroed payload.
    ///
    /// Union-field writes need no `unsafe` (only reads do): every value
    /// written is a valid value of the field's type.
    fn envelope(source: Endpoint, call_type: i32) -> Message {
        Message {
            m_source: source,
            m_type: call_type,
            ..Message::default()
        }
    }

    #[test]
    fn semget_decode() {
        let mut msg = envelope(Endpoint::from_generation_slot(1, 10), IPC_SEMGET);
        msg.m_u.m_lc_ipc_semget = MessLcIpcSemget {
            key: 0x1234,
            nr: 3,
            flag: 0o1000,
            retid: 0,
            ..MessLcIpcSemget::default()
        };
        let req = IpcSemgetIn::decode_message(&msg);
        assert_eq!(req.caller, msg.m_source);
        assert_eq!((req.key, req.count, req.flag), (0x1234, 3, 0o1000));
    }

    #[test]
    fn semctl_decode() {
        let mut msg = envelope(Endpoint::PM, IPC_SEMCTL);
        msg.m_u.m_lc_ipc_semctl = MessLcIpcSemctl {
            id: 7,
            num: 2,
            cmd: 5,
            opt: 0xDEAD,
            ret: 0,
            ..MessLcIpcSemctl::default()
        };
        let req = IpcSemctlIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint::PM);
        assert_eq!(
            (req.id, req.number, req.command, req.option),
            (7, 2, 5, 0xDEAD)
        );
    }

    #[test]
    fn semop_decode() {
        let mut msg = envelope(Endpoint::from_generation_slot(0, 20), IPC_SEMOP);
        msg.m_u.m_lc_ipc_semop = MessLcIpcSemop {
            id: 4,
            ops: 0x4000_1000,
            size: 3,
            ..MessLcIpcSemop::default()
        };
        let req = IpcSemopIn::decode_message(&msg);
        assert_eq!(req.caller, msg.m_source);
        assert_eq!(req.id, 4);
        assert_eq!(req.operations_address, 0x4000_1000);
        assert_eq!(req.operation_count, 3);
    }

    #[test]
    fn shmget_decode() {
        let mut msg = envelope(Endpoint::PM, IPC_SHMGET);
        msg.m_u.m_lc_ipc_shmget = MessLcIpcShmget {
            key: 0x5678,
            size: 0x2000,
            flag: 0o1000,
            retid: 0,
            ..MessLcIpcShmget::default()
        };
        let req = IpcShmgetIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint::PM);
        assert_eq!((req.key, req.size, req.flag), (0x5678, 0x2000, 0o1000));
    }

    #[test]
    fn shmat_decode() {
        let mut msg = envelope(Endpoint::PM, IPC_SHMAT);
        msg.m_u.m_lc_ipc_shmat = MessLcIpcShmat {
            id: 9,
            addr: 0,
            flag: 0,
            retaddr: 0,
            ..MessLcIpcShmat::default()
        };
        let req = IpcShmatIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint::PM);
        assert_eq!(req.id, 9);
        assert_eq!(req.address.0, 0);
    }

    #[test]
    fn shmdt_decode() {
        let mut msg = envelope(Endpoint::PM, IPC_SHMDT);
        msg.m_u.m_lc_ipc_shmdt = MessLcIpcShmdt {
            addr: 0x7000_0000,
            ..MessLcIpcShmdt::default()
        };
        let req = IpcShmdtIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint::PM);
        assert_eq!(req.address.0, 0x7000_0000);
    }

    #[test]
    fn shmctl_decode() {
        let mut msg = envelope(Endpoint::PM, IPC_SHMCTL);
        msg.m_u.m_lc_ipc_shmctl = MessLcIpcShmctl {
            id: 11,
            cmd: 2,
            buf: 0x5000_0000,
            ret: 0,
            ..MessLcIpcShmctl::default()
        };
        let req = IpcShmctlIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint::PM);
        assert_eq!((req.id, req.command, req.buffer), (11, 2, 0x5000_0000));
    }

    #[test]
    fn caller_comes_from_envelope() {
        // The payload carries no endpoint: even if a sender crafts payload
        // bytes that look like an endpoint, decoding still reports m_source.
        let mut msg = envelope(Endpoint::from_generation_slot(2, 33), IPC_SEMGET);
        // key bytes chosen to look like a plausible endpoint value.
        msg.m_u.m_lc_ipc_semget = MessLcIpcSemget {
            key: 0,
            nr: 1,
            flag: 0,
            retid: 0,
            ..MessLcIpcSemget::default()
        };
        let req = IpcSemgetIn::decode_message(&msg);
        assert_eq!(req.caller, Endpoint::from_generation_slot(2, 33));
    }

    #[test]
    fn wire_layout_is_56_bytes() {
        use core::mem::size_of;
        // C: ipc.h `_ASSERT_MSG_SIZE` — every payload is 56 bytes.
        assert_eq!(size_of::<MessLcIpcSemget>(), 56);
        assert_eq!(size_of::<MessLcIpcSemctl>(), 56);
        assert_eq!(size_of::<MessLcIpcSemop>(), 56);
        assert_eq!(size_of::<MessLcIpcShmget>(), 56);
        assert_eq!(size_of::<MessLcIpcShmat>(), 56);
        assert_eq!(size_of::<MessLcIpcShmdt>(), 56);
        assert_eq!(size_of::<MessLcIpcShmctl>(), 56);
    }
}
