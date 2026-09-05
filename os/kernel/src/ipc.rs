//! Kernel IPC core module.
//!
//! Implements the six IPC primitives (SEND, RECEIVE, SENDREC, NOTIFY, SENDNB, SENDA)
//! and supporting mechanisms (deadlock detection, sender queues, delayed delivery).
//!
//! **Zero-heap contract**: see `lib.rs` for the kernel-wide contract. This
//! module allocates nothing at runtime — sender queues use an intrusive
//! FIFO through the process-table slots (caller_q_head/tail in target slot,
//! send_q_link in sender slot).
//!
//! # Module Organization
//!
//! - Types: `IpcCall`, `IpcOutcome`, `IpcError`, `SendFlags`,
//!   `IpcEngine`, `DeadlockCycle`, `UserCopy`, `DeliverResult`
//! - Sender wait queue: free functions `caller_q_push` / `caller_q_find` /
//!   `caller_q_remove` / `caller_q_remove_by_nr` (intrusive FIFO through
//!   the process-table slots — no heap)
//! - SENDA flags: `AMF_*` constants
//!
//! Design decisions are documented in `12-ipc-core.md` §3.
//! C source: `minix3/minix/kernel/proc.c:263-294, 479-597, 599-698, 703-768,
//! 870-962, 967-1117, 1122-1167, 1200-1326, 1331-1346`.

use minix_types::{Endpoint, Message, MessNotify, VirBytes};
use crate::proc::{KProcess, ProcNr, RtsFlagsBits, MiscFlagsBits, NONE_PROC_NR, proc_nr};
use crate::proc_table::PROC_TABLE_SIZE;
use crate::kpriv::{KPriv, PrivTable};
use crate::errno::{OK, EINVAL, EDEADSRCDST, ECALLDENIED};

/// Notification message type. C: `#define NOTIFY_MESSAGE 0x1000` — com.h:90.
///
/// `BuildNotifyMessage` sets `m_type = NOTIFY_MESSAGE` so receivers can
/// distinguish notifications from regular IPC replies. The sender's
/// endpoint is in `m_source` (set by the caller of `BuildNotifyMessage`).
const NOTIFY_MESSAGE: i32 = 0x1000;

// ── IPC status encoding ──
//
// C: `minix3/minix/include/minix/ipcconst.h:21-35` — macros for IPC status
// code manipulation. The IPC status register stores metadata about the
// last IPC delivery (call type + flags) so user-space libraries can
// determine how to reply (e.g., SENDREC needs a reply, NOTIFY does not).

/// Bit shift for the call-type field in the IPC status register.
/// C: `IPC_STATUS_CALL_SHIFT` — ipcconst.h:21
pub const IPC_STATUS_CALL_SHIFT: u32 = 0;

/// Mask for the call-type field (6 bits, values 0-63).
/// C: `IPC_STATUS_CALL_MASK` — ipcconst.h:22
pub const IPC_STATUS_CALL_MASK: u32 = 0x3F;

/// Bit shift for the flags field in the IPC status register.
/// C: `IPC_STATUS_FLAGS_SHIFT` — ipcconst.h:32
pub const IPC_STATUS_FLAGS_SHIFT: u32 = 16;

/// Flag: message originated from the kernel on behalf of a process.
/// Trusted message — user-space must never reply to the sender.
/// C: `IPC_FLG_MSG_FROM_KERNEL` — ipcconst.h:28
pub const IPC_FLG_MSG_FROM_KERNEL: u32 = 1;

/// Encode an `IpcCall` value into the IPC status call field.
/// C: `IPC_STATUS_CALL_TO(call)` — ipcconst.h:25-26
#[inline]
pub const fn ipc_status_call_to(call: IpcCall) -> u64 {
    (call as u64 & IPC_STATUS_CALL_MASK as u64) << IPC_STATUS_CALL_SHIFT
}

/// Encode flags into the IPC status flags field.
/// C: `IPC_STATUS_FLAGS(flags)` — ipcconst.h:33
#[inline]
pub const fn ipc_status_flags(flags: u32) -> u64 {
    (flags as u64) << IPC_STATUS_FLAGS_SHIFT
}

// ── IPC call types ──

/// IPC primitive type. C: `call_nr` in `do_ipc()` — proc.c:599.
///
/// Values align with `minix3/minix/include/minix/ipcconst.h:7-13`:
/// `SEND=1, RECEIVE=2, SENDREC=3, NOTIFY=4, SENDNB=5, SENDA=16`.
///
/// `#[repr(u8)]` keeps the encoding compatible with C's `int call_nr` for
/// the values currently defined (1..=16). The C enum also reserves
/// `MINIX_KERNINFO=6` (kernel info query, not an IPC primitive) which is
/// intentionally NOT modeled here — it is dispatched separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IpcCall {
    /// Blocking send. C: `SEND=1` (ipcconst.h:7)
    Send = 1,
    /// Blocking receive. C: `RECEIVE=2` (ipcconst.h:8)
    Receive = 2,
    /// Atomic send + receive. C: `SENDREC=3` (ipcconst.h:9)
    SendRec = 3,
    /// Asynchronous notification. C: `NOTIFY=4` (ipcconst.h:10)
    Notify = 4,
    /// Non-blocking send. C: `SENDNB=5` (ipcconst.h:11)
    SendNb = 5,
    /// Batch async send. C: `SENDA=16` (ipcconst.h:13)
    SendA = 16,
}

impl IpcCall {
    /// Decode from raw `call_nr`. Returns `None` for invalid / unsupported
    /// values (e.g. `MINIX_KERNINFO=6`). C: `do_ipc` default branch returns
    /// `EBADCALL` — caller maps `None` to `IpcError::BadCall`.
    pub fn from_raw(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::Send),
            2 => Some(Self::Receive),
            3 => Some(Self::SendRec),
            4 => Some(Self::Notify),
            5 => Some(Self::SendNb),
            16 => Some(Self::SendA),
            _ => None,
        }
    }
}

/// IPC operation outcome. Replaces C's errno return pattern.
///
/// Design decision §3.1 / AT-1: distinguish "blocked (normal)" from
/// "error (exception)". C returns a single `OK` for both "message
/// delivered" and "caller is now blocked (RTS_SENDING set)" — the
/// dispatcher infers which from side effects. Rust makes the two states
/// explicit so `switch_to_user` can match without inspecting RTS flags.
///
/// Mapping:
/// - C `OK` + (dst woken or caller enqueued) → `Delivered` or `Blocked`
/// - C `ELOCKED`/`ENOTREADY`/`EDEADSRCDST`/`EFAULT`/`ECALLDENIED`/`ETRAPDENIED`
///   → `Error(IpcError)`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcOutcome {
    /// Message delivered to target (or notification recorded in bitmap).
    /// C: `return OK` from `mini_send`/`mini_receive`/`mini_notify` when
    /// the destination was in RECEIVE and the message was copied to
    /// `p_delivermsg`.
    Delivered,
    /// Caller is now blocked (`RTS_SENDING` or `RTS_RECEIVING` set).
    /// C: implicit — `mini_send` returns `OK` but caller is enqueued on
    /// `p_caller_q`. Rust surfaces this as a distinct state so the
    /// scheduler knows to switch without re-checking RTS flags.
    Blocked,
    /// IPC failed with error. The caller remains runnable.
    Error(IpcError),
}

impl IpcOutcome {
    /// Returns `true` if the outcome is `Delivered`.
    pub fn is_delivered(self) -> bool { matches!(self, Self::Delivered) }

    /// Returns `true` if the outcome is `Blocked`.
    pub fn is_blocked(self) -> bool { matches!(self, Self::Blocked) }

    /// Returns `Some(err)` if the outcome is `Error(err)`, else `None`.
    pub fn err(self) -> Option<IpcError> {
        match self { Self::Error(e) => Some(e), _ => None }
    }
}

/// IPC error codes. C: errno values returned by `mini_*` functions.
///
/// Each variant cites the C errno and the source location where it is
/// produced. Values are not encoded as the C errno integers (negative);
/// the kernel↔user boundary converts to `errno` at the syscall return
/// path. See `12-ipc-core.md` §3.5 for the unification rationale.
///
/// Relationship with `minix_types::ipc::IpcError`: the kernel crate is
/// the authoritative definition. The user-space mirror in
/// `os/libs/minix-types/src/ipc/ipc_error.rs` is a separate, smaller
/// enum used by user-space consumers that do not need to distinguish
/// kernel-internal error subtypes. A future refactor should unify them
/// via a shared `minix-ipc` crate (tracked as design follow-up, not a
/// P0 because the two types are not currently mixed at any API boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    /// Deadlock detected. C: `ELOCKED` — proc.c:931
    Deadlock,
    /// Source or destination endpoint invalid. C: `EDEADSRCDST` — proc.c:889
    DeadSrcDst,
    /// Non-blocking send target not ready. C: `ENOTREADY` — proc.c:926
    NotReady,
    /// Invalid IPC call number. C: `EBADCALL` — proc.c:95
    BadCall,
    /// Message copy failed (page fault). C: `EFAULT` — proc.c:902, 937
    Fault,
    /// IPC target not in whitelist. C: `ECALLDENIED` — `do_sync_ipc`
    CallDenied,
    /// System call trap not permitted. C: `ETRAPDENIED` — `do_sync_ipc`
    TrapDenied,
    /// Caller lacks SYS_PROC privilege. C: `EPERM` — `mini_senda`
    /// (proc.c:1336-1339)
    Permission,
}

// IPC send flags. C: `minix3/minix/kernel/ipc.h:11-12`.
//
// Design decision §3.4 / AT-4: `bitflags!` macro (not bare `u32`).
//
// # P0 FIX (FIX-1 / FIX-2)
//
// Previous code defined `NON_BLOCKING=0x01` and `FROM_KERNEL=0x02`,
// both wrong. The correct values aligned with C source are
// `NON_BLOCKING=0x0080` and `FROM_KERNEL=0x0100`.
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SendFlags: u32 {
        /// Non-blocking mode. C: `NON_BLOCKING=0x0080` — ipc.h:11
        const NON_BLOCKING = 0x0080;
        /// Message from kernel. C: `FROM_KERNEL=0x0100` — ipc.h:12
        const FROM_KERNEL = 0x0100;
        /// Internal: this send originates from SENDA (batch async send).
        /// Not a C flag — Rust uses it to set the correct IPC status call
        /// type (SENDA instead of SEND) when `send` is reused by `senda`.
        /// C's `mini_senda` has its own delivery path; Rust reuses `send`.
        const SENDA = 0x0001;
    }
}

impl SendFlags {
    /// Empty flags (blocking send from user space).
    pub const NONE: Self = Self::empty();
}

/// Deadlock cycle descriptor.
///
/// C: `deadlock()` — proc.c:703-768. Returns `Some` when a cyclic
/// dependency is found along the `P_BLOCKEDON` chain.
///
/// `direction` records which IPC call type produced the cycle so the
/// caller can apply the 2-cycle SEND↔RECEIVE exception (which is NOT a
/// deadlock — it is the normal request/reply pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlockCycle {
    /// Process numbers (in walk order) forming the cycle, starting at the caller.
    /// Maximum chain length is bounded by PROC_TABLE_SIZE.
    pub chain: [ProcNr; PROC_TABLE_SIZE],
    /// Number of valid entries in `chain`.
    pub chain_len: usize,
    /// Direction that produced the cycle. C: corresponds to `function`
    /// passed to `deadlock()` (SEND or RECEIVE).
    pub direction: DeadlockDirection,
    /// Number of processes in the cycle. C: `group_size` return value.
    /// `2` triggers the SEND↔RECEIVE check at the caller.
    pub group_size: usize,
}

/// Deadlock direction.
///
/// Kept distinct from `IpcCall` to prevent the deadlock detector from
/// being semantically coupled with the IPC dispatcher: `IpcCall` is the
/// user-facing API call number, `DeadlockDirection` is the engine's
/// detected wait direction. The two are 1:1 today but the type boundary
/// keeps future extensions (e.g. a third wait direction) local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlockDirection {
    /// SEND-direction cycle (caller is SENDING, waiting for receiver).
    /// C: `deadlock(SEND, ...)` path.
    Send,
    /// RECEIVE-direction cycle (caller is RECEIVING, waiting for sender).
    /// C: `deadlock(RECEIVE, ...)` path.
    Receive,
}

// ── User-space copy abstraction (FIX-10 / AT-8) ──

/// User-space memory copy abstraction.
///
/// Design decision §3.9 / AT-8: trait + arch implementation. C's
/// `copy_msg_from_user` / `copy_msg_to_user` call `virtual_copy`
/// directly, which violates the hardware abstraction principle (page
/// permission checks are arch-specific). The kernel IPC layer depends
/// only on this trait; arch crates provide the concrete implementation.
///
/// The trait is used by `IpcEngine::send` / `receive` / `deliver_message`
/// / `senda` to abstract away user-space page handling. Arch
/// implementations live in `os/arch/src/*/user_copy.rs` (to be added).
pub trait UserCopy {
    /// Copy a `Message` from user space. C: `copy_msg_from_user` —
    /// proc.c:901, 936. Returns `Err(CopyError::PageFault)` on page
    /// fault, `Err(CopyError::OutOfBounds)` if the address is outside
    /// user space.
    fn copy_msg_from_user(&self, src: VirBytes) -> Result<Message, CopyError>;

    /// Copy a `Message` to user space. C: `copy_msg_to_user` — proc.c:185.
    /// Same error semantics as `copy_msg_from_user`.
    fn copy_msg_to_user(&self, dst: VirBytes, msg: &Message) -> Result<(), CopyError>;

    /// Read one SENDA table entry from user space.
    ///
    /// C: `A_RETR(i)` — proc.c:1244 (per-entry copy-in of `asynmsg_t`).
    /// The kernel never caches the whole table; entries are read one at
    /// a time and results written back one at a time, so no kernel-side
    /// copy of the table exists (C stores only the user-space table
    /// address + size in `priv->s_asyntab`/`s_asynsize`, priv.h:28).
    ///
    /// Returns `(dst_endpoint, message, flags)` — C: `asynmsg_t.dst`,
    /// `.msg`, `.flags`. Flag values are `AMF_*` (ipc.h:2754-2761).
    fn read_senda_entry(
        &self,
        table: VirBytes,
        index: usize,
    ) -> Result<(Endpoint, Message, i32), CopyError>;

    /// Write one SENDA table result back to user space.
    ///
    /// C: `A_INSRT(i)` — proc.c:1307 (per-entry copy-out of the result +
    /// `AMF_DONE` flag). The kernel ignores copy errors here, same as C
    /// ("Copy results to caller; ignore errors").
    fn write_senda_result(
        &self,
        table: VirBytes,
        index: usize,
        result: i32,
        flags: i32,
    ) -> Result<(), CopyError>;
}

/// User-copy error. Distinguishes page fault (retryable via VM) from
/// out-of-bounds (immediately fatal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyError {
    /// Page not mapped or permission denied. Triggers VM suspend on
    /// first occurrence, SIGSEGV on second consecutive failure.
    /// C: `vm_suspend(VMS_PAGEFAULT)` — proc.c:281-282.
    PageFault,
    /// Address outside user space. Always fatal (SIGSEGV).
    OutOfBounds,
}

/// Stub `UserCopy` implementation used by tests and boot-time paths
/// where no real user space exists. Performs a direct bitwise copy with
/// no page-fault handling.
#[derive(Debug, Default, Clone, Copy)]
pub struct KernelUserCopy;

impl UserCopy for KernelUserCopy {
    fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> {
        // Stub: real implementation reads from the user address space.
        // Boot-time callers always pass `FROM_KERNEL`, bypassing this path.
        Ok(Message::default())
    }
    fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> {
        // Stub: symmetric with `copy_msg_from_user`.
        Ok(())
    }
    fn read_senda_entry(
        &self,
        _table: VirBytes,
        _index: usize,
    ) -> Result<(Endpoint, Message, i32), CopyError> {
        // Stub: kernel-origin path never triggers SENDA table reads.
        // Real SENDA flows inject an arch-specific `UserCopy`.
        Err(CopyError::PageFault)
    }
    fn write_senda_result(
        &self,
        _table: VirBytes,
        _index: usize,
        _result: i32,
        _flags: i32,
    ) -> Result<(), CopyError> {
        // Stub: symmetric with `read_senda_entry`.
        Ok(())
    }
}

/// Result of `IpcEngine::deliver_message`. C: `delivermsg` is `void`;
/// Rust makes the failure path explicit so `switch_to_user` can route
/// to VM suspend or signal delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverResult {
    /// Message copied to user space successfully. C: clear `MF_DELIVERMSG`.
    Delivered,
    /// First page fault — caller should `vm_suspend(VMS_PAGEFAULT)` and
    /// set `MF_MSGFAILED`. C: proc.c:281-282.
    PageFault,
    /// Second consecutive page fault, or out-of-bounds — caller should
    /// `cause_sig(SIGSEGV)`. C: proc.c:278.
    Segfault,
}

/// Deliver pending message to user space.
///
/// Free function extracted from `IpcEngine::deliver_message` so that
/// `ProcessTable::process_misc_flags` can call it without constructing
/// a full `IpcEngine` (which requires `priv_table` + `procs` slice that
/// `ProcessTable` already owns).
///
/// C: `delivermsg(&p)` — proc.c:263-294. Called by `process_misc_flags`
/// (proc.c:~700) when `MF_DELIVERMSG` is set.
///
/// # Two-failure policy
///
/// - Success → clear `MF_DELIVERMSG` (+ `MF_MSGFAILED` if set).
/// - First `PageFault` → set `MF_MSGFAILED`, return `PageFault`
///   (caller routes to `vm_suspend(VMS_PAGEFAULT)`).
/// - Second consecutive failure (`MF_MSGFAILED` already set) →
///   return `Segfault` (caller routes to `cause_sig(SIGSEGV)`).
/// - `OutOfBounds` → `Segfault` immediately.
pub fn delivermsg(proc: &mut crate::proc::KProcess, user_copy: &dyn UserCopy) -> DeliverResult {
    debug_assert!(
        proc.p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG),
        "delivermsg called without MF_DELIVERMSG"
    );

    let msg = proc.p_delivermsg;
    let user_addr = proc.p_delivermsg_vir;

    match user_copy.copy_msg_to_user(user_addr, &msg) {
        Ok(()) => {
            proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
            DeliverResult::Delivered
        }
        Err(CopyError::PageFault) => {
            if proc.p_misc_flags.is_set(MiscFlagsBits::MSGFAILED) {
                // Second consecutive failure → SIGSEGV. C: proc.c:278.
                proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
                proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
                DeliverResult::Segfault
            } else {
                // First failure → vm_suspend. C: proc.c:281-282.
                proc.p_misc_flags.set(MiscFlagsBits::MSGFAILED);
                DeliverResult::PageFault
            }
        }
        Err(CopyError::OutOfBounds) => {
            proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
            proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
            DeliverResult::Segfault
        }
    }
}

// ── Async message passing (SENDA) ──
//
// Design (re-judged from the earlier `AsyncMessageTable` Vec cache):
// C never copies the SENDA table into the kernel. `try_deliver_senda`
// reads entries from user space one at a time (`A_RETR`, proc.c:1244)
// and writes results back one at a time (`A_INSRT`, proc.c:1307). If
// entries remain undelivered, C stores only the table address and size
// in the caller's privilege structure (`s_asyntab`/`s_asynsize`,
// priv.h:28) — retry re-reads the table from user space
// (try_async → try_deliver_senda, proc.c:1348-1410).
//
// The kernel-side Vec cache was a double deviation: it heap-allocated
// (violating the kernel's zero-heap storage model — 06 §2.0 layer 2)
// and changed retry semantics (kernel copy vs C's user-space re-read).
// Delivery state lives where C keeps it: the `AMF_DONE` flag in the
// user table entry, plus the `s_asyn_pending` bitmap on the target
// (set by the delivery pass, cleared by retry delivery).

// ── Sender wait queue (design §2.5 / AT-2 / ARCH-2) ──
//
// C: `struct proc *p_caller_q` (queue head, owned by the TARGET —
// proc.h:73) + `struct proc *p_q_link` (chain link, living on the
// SENDER — proc.h:74) — an intrusive FIFO threaded through the
// process-table slots.
//
// Design (re-judged from the earlier `VecDeque<ProcNr>`): same
// intrusive FIFO, links re-expressed as `Option<ProcNr>` slot indices.
// Index links carry no reference semantics, so Rust's aliasing rules
// are not violated and no heap is involved — enqueue touches only
// `Option<ProcNr>` fields already inside the slots. This restores C's
// properties exactly:
//   1. **Zero allocation** — the kernel has no heap at any point in its
//      lifetime (06-proc-init-boot-proc.md §2.0 layer 2: no malloc/kmalloc
//      in the entire C kernel; minix-rs follows the same storage model).
//   2. **Infallible enqueue** — C's enqueue (two pointer writes,
//      proc.c:960-964) never fails; `VecDeque::push_back` would abort
//      the kernel on allocation failure.
//   3. **Bounded capacity for free** — a process blocks on at most one
//      send (`RTS_SENDING` blocks the whole process, proc.c:948), so
//      each slot appears in at most one queue: total queued entries ≤
//      NR_TASKS + NR_PROCS, statically guaranteed. No overflow path.
//
// Storage (in KProcess, C-isomorphic):
//   - target slot: `caller_q_head` / `caller_q_tail` (C: `p_caller_q`;
//     the tail is an O(1)-append extension — C walks O(n) to the tail,
//     proc.c:960-964; FIFO order identical)
//   - sender slot: `send_q_link` (C: `p_q_link`)
//
// The operations below take `&mut [KProcess]` and resolve links via
// `nr_to_idx` (bounds-checked, same as C's `proc_addr` offset formula).
// C walks find+unlink in one pass (proc.c:1084); Rust's split
// find/remove walks twice — O(n) either way, n ≤ table size.

/// SENDA table entry flags. C: `ipc.h:2754-2761` (octal values).
pub const AMF_EMPTY: i32 = 0o0;
pub const AMF_VALID: i32 = 0o1;
pub const AMF_DONE: i32 = 0o2;
pub const AMF_NOTIFY: i32 = 0o4;
pub const AMF_NOREPLY: i32 = 0o10;
pub const AMF_NOTIFY_ERR: i32 = 0o20;
/// All flag bits the kernel accepts. C: proc.c:1251.
const AMF_ALL: i32 = AMF_VALID | AMF_DONE | AMF_NOTIFY | AMF_NOREPLY | AMF_NOTIFY_ERR;

/// Enqueue `caller_idx`'s process at the tail of `dst_idx`'s sender queue.
///
/// C: `while (*xpp) xpp = &(*xpp)->p_q_link; *xpp = caller_ptr;`
/// — proc.c:960-964 (walk to tail, link). Rust keeps an explicit tail
/// for O(1) append; FIFO order is identical.
///
/// Invariant (INV-1): a process with `RTS_SENDING` set is in exactly one
/// queue; `send_q_link` is `None` outside a queue.
pub(crate) fn caller_q_push(procs: &mut [KProcess], dst_idx: usize, caller_idx: usize) {
    let caller_nr = procs[caller_idx].p_nr;
    procs[caller_idx].send_q_link = None;
    match procs[dst_idx].caller_q_tail {
        Some(tail_nr) => {
            if let Some(tail_idx) = nr_to_idx(tail_nr) {
                procs[tail_idx].send_q_link = Some(caller_nr);
            }
            procs[dst_idx].caller_q_tail = Some(caller_nr);
        }
        None => {
            procs[dst_idx].caller_q_head = Some(caller_nr);
            procs[dst_idx].caller_q_tail = Some(caller_nr);
        }
    }
}

/// Find the first queued sender on `dst_idx`'s queue matching
/// `src_endpoint` (head-first scan). Returns the sender's slot index.
///
/// C: `while (*xpp) { if (CANRECEIVE(...)) break; }` — proc.c:1077-1105.
/// `Endpoint::ANY` matches the head (C: first queue entry).
pub(crate) fn caller_q_find(
    procs: &[KProcess],
    dst_idx: usize,
    src_endpoint: Endpoint,
) -> Option<usize> {
    let mut cur = procs[dst_idx].caller_q_head;
    while let Some(nr) = cur {
        let idx = nr_to_idx(nr)?;
        if src_endpoint == Endpoint::ANY || procs[idx].p_endpoint == src_endpoint {
            return Some(idx);
        }
        cur = procs[idx].send_q_link;
    }
    None
}

/// Remove the sender at slot index `sender_idx` from `dst_idx`'s queue,
/// unlinking it (predecessor link + head/tail fixup).
///
/// C: `*xpp = (*xpp)->p_q_link` — proc.c:1084 (find + unlink in one walk);
/// same walk shape in `clear_ipc` (system.c:520-531).
pub(crate) fn caller_q_remove(procs: &mut [KProcess], dst_idx: usize, sender_idx: usize) -> bool {
    let sender_nr = procs[sender_idx].p_nr;
    let sender_next = procs[sender_idx].send_q_link;
    let mut cur = procs[dst_idx].caller_q_head;
    let mut prev: Option<ProcNr> = None;
    while let Some(nr) = cur {
        let Some(idx) = nr_to_idx(nr) else { return false };
        if idx == sender_idx {
            match prev {
                Some(prev_nr) => {
                    if let Some(prev_idx) = nr_to_idx(prev_nr) {
                        procs[prev_idx].send_q_link = sender_next;
                    }
                }
                None => procs[dst_idx].caller_q_head = sender_next,
            }
            if procs[dst_idx].caller_q_tail == Some(sender_nr) {
                procs[dst_idx].caller_q_tail = prev;
            }
            procs[sender_idx].send_q_link = None;
            return true;
        }
        prev = Some(nr);
        cur = procs[idx].send_q_link;
    }
    false
}

/// Remove `target_nr` from `dst_idx`'s queue by ProcNr value.
///
/// C: `clear_ipc` walk — system.c:520-531 (dead process unlinked from
/// its send target's queue); `abort_proc_ipc_send` — do_update.c:226-234.
pub(crate) fn caller_q_remove_by_nr(
    procs: &mut [KProcess],
    dst_idx: usize,
    target_nr: ProcNr,
) -> bool {
    match nr_to_idx(target_nr) {
        Some(i) => caller_q_remove(procs, dst_idx, i),
        None => false,
    }
}

// ── Test/convenience helpers (queue length + membership) ──

/// Number of senders queued on `dst_idx`'s queue (walks the chain).
#[cfg(test)]
pub(crate) fn caller_q_len(procs: &[KProcess], dst_idx: usize) -> usize {
    let mut n = 0;
    let mut cur = procs[dst_idx].caller_q_head;
    while let Some(nr) = cur {
        let Some(idx) = nr_to_idx(nr) else { break };
        n += 1;
        cur = procs[idx].send_q_link;
    }
    n
}

/// `true` iff `dst_idx`'s queue is empty.
#[cfg(test)]
pub(crate) fn caller_q_is_empty(procs: &[KProcess], dst_idx: usize) -> bool {
    procs[dst_idx].caller_q_head.is_none()
}

// ── IPC Engine (design §2.6 / AT-3 / ARCH-3) ──

/// IPC core engine. Owns process table reference for in-place modification.
///
/// Design decision §3.3 / ARCH-3: NOT a ZST namespace — holds `&mut [KProcess]`.
///
/// C: free functions (`mini_send`, `mini_receive`, ...) taking `struct proc*`
/// on every call. Rust: `IpcEngine` holds the borrow once, methods don't
/// repeat it — eliminating the per-call `&mut [KProcess]` parameter.
///
/// # BKL (Big Kernel Lock)
///
/// **Precondition**: every method requires the caller to hold the BKL.
/// The `&mut [KProcess]` borrow is the Rust-level proxy for "BKL held":
/// it statically prevents aliasing mutable access across CPUs, matching
/// C's `big_kernel_lock` spinlock guarantee that only one CPU at a time
/// may mutate the process table.
///
/// # Lifetime
///
/// `'a` ties the engine to the borrow of `procs` / `priv_table` /
/// `user_copy`. The engine is short-lived (constructed per syscall
/// dispatch, dropped when the syscall returns).
pub struct IpcEngine<'a> {
    /// Process table slice (mutable — IPC modifies RTS flags, queues, etc.).
    procs: &'a mut [KProcess],
    /// Privilege table (mutable — notify updates `s_notify_pending` bitmap).
    priv_table: &'a mut PrivTable,
    /// User-space copy abstraction (arch-specific impl injected).
    user_copy: &'a dyn UserCopy,
    /// Sender whose message was just delivered while `MF_SIG_DELAY` was set.
    ///
    /// C: proc.c:1082-1083 — `if (sender->p_misc_flags & MF_SIG_DELAY)
    /// sig_delay_done(sender)`. The delay-end notification
    /// (`sig_delay_done` → `cause_sig(SIGSNDELAY)`) must run at the
    /// `ProcessTable` level because `cause_signal` sets `RTS_SIGNALED`
    /// through the scheduler-aware `rts_set` (dequeue). The engine only has
    /// a slice, so it records the sender here and the `ProcessTable`-level
    /// dispatcher (`dispatch_ipc`) completes the protocol after `do_ipc`
    /// returns. At most one sender is delivered per IPC operation.
    sig_delay_sender: Option<ProcNr>,
}

impl<'a> IpcEngine<'a> {
    /// Construct with process table, privilege table, and user-copy impl.
    pub fn new(
        procs: &'a mut [KProcess],
        priv_table: &'a mut PrivTable,
        user_copy: &'a dyn UserCopy,
    ) -> Self {
        Self { procs, priv_table, user_copy, sig_delay_sender: None }
    }

    /// Take the sender whose message was delivered while `MF_SIG_DELAY`
    /// was set (and whose delay-end notification is now due), clearing the
    /// record. See [`Self::sig_delay_sender`].
    ///
    /// Called by the `ProcessTable`-level dispatcher after `do_ipc`
    /// returns; `Some(sender_nr)` means "call `sig_delay_done(sender_nr)`".
    pub fn take_sig_delay_sender(&mut self) -> Option<ProcNr> {
        self.sig_delay_sender.take()
    }

    // ── Helpers ──

    /// Index into process table by `ProcNr`. Returns `None` for invalid nr.
    fn idx_of(&self, nr: ProcNr) -> Option<usize> { nr_to_idx(nr) }

    /// Find index by endpoint.
    fn idx_by_endpoint(&self, ep: Endpoint) -> Option<usize> {
        self.procs.iter().position(|p| p.p_endpoint == ep)
    }

    /// Dynamic field selection for "blocked-on endpoint".
    ///
    /// C: `P_BLOCKEDON(p)` macro — proc.h:187-194. Returns the endpoint
    /// this process is blocked on, or `None` if it is runnable.
    ///
    /// # P0 FIX (FIX-3 / AT-6)
    ///
    /// The previous `detect_deadlock` fixed the field via the `function`
    /// argument (SEND→`p_sendto_e`, RECEIVE→`p_getfrom_e`). That could
    /// not detect mixed-chain deadlocks (A send→B, B receive←C, C
    /// send→A) because the walk broke at B (RECEIVING, not SENDING).
    /// This method mirrors the C macro's dynamic dispatch on RTS flags,
    /// delegating to `KProcess::blocked_on` (proc.rs).
    fn blocked_on(proc_: &KProcess) -> Option<Endpoint> {
        proc_.blocked_on()
    }

    /// Check if `dst` is willing to receive a message from `src`.
    ///
    /// C: `WILLRECEIVE(src_e, dst, m, mp)` macro — ipc.h:14. The IPC
    /// filter extension (`m_src_v` / `m_src_p` arguments) is handled by
    /// `ipc_filter.rs` and not modeled here; this predicate covers only
    /// the RTS-state + source-endpoint match.
    ///
    /// Matches C: `RTS_ISSET(dst, RTS_RECEIVING) && !RTS_ISSET(dst, RTS_SENDING)
    /// && (dst->p_getfrom_e == ANY || dst->p_getfrom_e == src_e)`.
    fn is_willing_to_receive(dst: &KProcess, src: Endpoint) -> bool {
        !dst.p_rts_flags.is_set(RtsFlagsBits::SENDING)
            && dst.p_rts_flags.is_set(RtsFlagsBits::RECEIVING)
            && (dst.p_getfrom_e == Endpoint::ANY || dst.p_getfrom_e == src)
    }

    // ── Deadlock detection ──

    /// Deadlock detection.
    ///
    /// C: `deadlock()` — proc.c:703-768. Follows the `P_BLOCKEDON` chain
    /// starting at `dst_endpoint`. Each step dynamically selects
    /// `p_sendto_e` or `p_getfrom_e` based on the target's RTS flags
    /// (see `blocked_on`). If the chain returns to `caller_nr`, a cycle
    /// is reported.
    ///
    /// # 2-cycle SEND↔RECEIVE exception (proc.c:744-749)
    ///
    /// When the cycle has exactly 2 members (caller ↔ target), C applies
    /// a magic encoding: `RTS_SENDING = 0x04 = 1 << 2`, so
    /// `function << 2` lines up with the SENDING bit. SEND↔RECEIVE
    /// (the normal request/reply pattern) returns "not a deadlock";
    /// SEND↔SEND or RECEIVE↔RECEIVE are deadlocks.
    ///
    /// The caller is responsible for interpreting `direction` and
    /// `group_size` — this function only reports the cycle, mirroring C.
    pub fn detect_deadlock(
        &mut self,
        function: IpcCall,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
    ) -> Option<DeadlockCycle> {
        // The 2-cycle check uses C's encoding: SEND=1 → 0x04, RECEIVE=2 → 0x08.
        // `function << 2` must align with `RTS_SENDING = 0x04` for the XOR test.
        let direction = match function {
            IpcCall::Send | IpcCall::SendRec | IpcCall::SendNb => DeadlockDirection::Send,
            IpcCall::Receive => DeadlockDirection::Receive,
            _ => return None,
        };

        let caller_idx = self.idx_of(caller_nr)?;
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        let mut chain: [ProcNr; PROC_TABLE_SIZE] = [ProcNr(NONE_PROC_NR); PROC_TABLE_SIZE];
        let mut chain_len: usize = 0;
        chain[chain_len] = caller_nr;
        chain_len += 1;

        let mut current_ep = dst_endpoint;
        let mut group_size: usize = 1;

        loop {
            if current_ep == Endpoint::ANY {
                return None;
            }
            let target_idx = self.idx_by_endpoint(current_ep)?;
            let target_nr = self.procs[target_idx].p_nr;
            group_size += 1;
            chain[chain_len] = target_nr;
            chain_len += 1;

            let next_ep = Self::blocked_on(&self.procs[target_idx])?;

            if next_ep == caller_endpoint {
                if group_size == 2 {
                    // 2-cycle exception. C: `(xp->p_rts_flags ^ (function << 2)) & RTS_SENDING`
                    // — proc.c:746. If the XOR is non-zero on the SENDING bit, this is
                    // SEND↔RECEIVE (normal request/reply) and NOT a deadlock.
                    let xp_rts = self.procs[target_idx].p_rts_flags.get().bits();
                    let function_shifted = (function as u32) << 2;
                    if (xp_rts ^ function_shifted) & RtsFlagsBits::SENDING.bits() != 0 {
                        return None;
                    }
                }
                return Some(DeadlockCycle {
                    chain,
                    chain_len,
                    direction,
                    group_size,
                });
            }
            current_ep = next_ep;
        }
    }

    // ── Send ──

    /// Sync send. C: `mini_send()` — proc.c:870-962.
    ///
    /// Flow:
    /// 1. Endpoint validity check (`RTS_NO_ENDPOINT` → `DeadSrcDst`).
    /// 2. If dst is willing to receive → direct delivery (path A).
    /// 3. Else if `NON_BLOCKING` → `NotReady`.
    /// 4. Else deadlock check → `Deadlock` if cycle found.
    /// 5. Else block caller: cache message, set `RTS_SENDING` +
    ///    `p_sendto_e`, enqueue on `dst.caller_q`.
    ///
    /// # BKL
    ///
    /// Caller must hold the BKL. The `&mut self` borrow is the
    /// type-level proxy (see struct doc).
    pub fn send(
        &mut self,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
        flags: SendFlags,
    ) -> IpcOutcome {
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        let dst_idx = match self.idx_by_endpoint(dst_endpoint) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };

        // C: `if (RTS_ISSET(dst_ptr, RTS_NO_ENDPOINT)) return EDEADSRCDST;`
        if self.procs[dst_idx].p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
            return IpcOutcome::Error(IpcError::DeadSrcDst);
        }

        // Phase 2: WILLRECEIVE check (path A — direct delivery).
        if Self::is_willing_to_receive(&self.procs[dst_idx], caller_endpoint) {
            // C: `copy_msg_from_user` (user path) or direct copy (FROM_KERNEL).
            if !flags.contains(SendFlags::FROM_KERNEL) {
                // User-origin send: route through UserCopy trait.
                // C: proc.c:901-906.
                let user_src = self.procs[caller_idx].p_delivermsg_vir;
                match self.user_copy.copy_msg_from_user(user_src) {
                    Ok(m) => self.procs[dst_idx].p_delivermsg = m,
                    Err(_) => return IpcOutcome::Error(IpcError::Fault),
                }
            } else {
                self.procs[dst_idx].p_delivermsg = *msg;
            }
            self.procs[dst_idx].p_delivermsg.m_source = caller_endpoint;
            self.procs[dst_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
            if flags.contains(SendFlags::FROM_KERNEL) {
                self.procs[dst_idx].p_misc_flags.set(MiscFlagsBits::SENDING_FROM_KERNEL);
                // C: proc.c:905 — `IPC_STATUS_ADD_FLAGS(dst_ptr, IPC_FLG_MSG_FROM_KERNEL)`
                crate::proc::ipc_status_add_flags(&mut self.procs[dst_idx], IPC_FLG_MSG_FROM_KERNEL);
            }
            // C: proc.c:911-913 — determine call type and add to IPC status.
            //   call = (MF_REPLY_PEND ? SENDREC : (NON_BLOCKING ? SENDNB : SEND))
            //   IPC_STATUS_ADD_CALL(dst_ptr, call)
            //
            // Rust extension: SENDA flag overrides to IpcCall::SendA, matching
            // C's `mini_senda` direct delivery (proc.c:1287 sets SENDA, not SEND).
            let delivered_call = if flags.contains(SendFlags::SENDA) {
                IpcCall::SendA
            } else if self.procs[caller_idx].p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
                IpcCall::SendRec
            } else if flags.contains(SendFlags::NON_BLOCKING) {
                IpcCall::SendNb
            } else {
                IpcCall::Send
            };
            crate::proc::ipc_status_add_call(&mut self.procs[dst_idx], delivered_call);
            // C: `RTS_UNSET(dst, RTS_RECEIVING)` — wake up target.
            self.procs[dst_idx].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
            return IpcOutcome::Delivered;
        }

        // Path B: caller must block.
        // C: `if (flags & NON_BLOCKING) return ENOTREADY;` — proc.c:925-927.
        if flags.contains(SendFlags::NON_BLOCKING) {
            return IpcOutcome::Error(IpcError::NotReady);
        }

        // C: `if (deadlock(SEND, caller, dst_e)) return ELOCKED;` — proc.c:930-932.
        if self.detect_deadlock(IpcCall::Send, caller_nr, dst_endpoint).is_some() {
            return IpcOutcome::Error(IpcError::Deadlock);
        }

        // Cache message + set RTS_SENDING + enqueue.
        // C: proc.c:938-960.
        if !flags.contains(SendFlags::FROM_KERNEL) {
            let user_src = self.procs[caller_idx].p_delivermsg_vir;
            match self.user_copy.copy_msg_from_user(user_src) {
                Ok(m) => self.procs[caller_idx].p_sendmsg = m,
                Err(_) => return IpcOutcome::Error(IpcError::Fault),
            }
        } else {
            self.procs[caller_idx].p_sendmsg = *msg;
            self.procs[caller_idx].p_misc_flags.set(MiscFlagsBits::SENDING_FROM_KERNEL);
        }
        self.procs[caller_idx].p_rts_flags.set(RtsFlagsBits::SENDING);
        self.procs[caller_idx].p_sendto_e = dst_endpoint;
        caller_q_push(self.procs, dst_idx, caller_idx);
        IpcOutcome::Blocked
    }

    // ── Receive ──

    /// Sync receive. C: `mini_receive()` — proc.c:967-1117.
    ///
    /// # P0 FIX (FIX-5) + P0-12-5 (REPLY_PEND semantics)
    ///
    /// Previous implementation was a 30-line stub that skipped all three
    /// message-source checks and unconditionally blocked the caller.
    /// A later attempt added a `MF_REPLY_PEND` shortcut that returned
    /// `Delivered` immediately — but this was a P0 semantic bug: C
    /// (proc.c:999-1005) only skips the *notify* check when
    /// `MF_REPLY_PEND` is set, it still falls through to async + caller_q.
    ///
    /// This implementation follows C's three-tier priority exactly:
    ///
    /// 1. **Pending notifications** (`s_notify_pending` bitmap) — skipped
    ///    when `MF_REPLY_PEND` is set (SENDREC atomicity). C: 1000-1030.
    /// 2. **Pending async messages** (`s_asyn_pending` bitmap) — always
    ///    checked. C: 1031-1070.
    /// 3. **Sync sender queue** (`caller_q`) — first matching sender.
    ///    C: 1071-1095.
    /// 4. None of the above → block caller (`RTS_RECEIVING`).
    ///
    /// Returns:
    /// - `Delivered` — a message was placed in `p_delivermsg`.
    /// - `Blocked` — no matching message; caller is now in `RTS_RECEIVING`.
    pub fn receive(
        &mut self,
        caller_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> IpcOutcome {
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };

        // C: proc.c:999-1005 — `MF_REPLY_PEND` only skips the *notify*
        // check. It does NOT skip async or caller_q. The reply for
        // SENDREC arrives via the sender's `mini_send` path, which
        // delivers directly to `p_delivermsg` when the caller is in
        // RECEIVE — that path is handled by Phase 3 (caller_q).
        let reply_pend = self.procs[caller_idx]
            .p_misc_flags
            .is_set(MiscFlagsBits::REPLY_PEND);

        // Phase 1: pending notifications (skipped when MF_REPLY_PEND).
        // C: `has_pending` (NOTIFY) — proc.c:1000-1030.
        if !reply_pend
            && let Some(notify_src) = self.take_pending_notify(caller_nr, src_endpoint) {
                self.build_notify_message(
                    caller_idx,
                    NotifySource::from_caller_nr(notify_src),
                );
                self.procs[caller_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
                // C: proc.c:1033 — `IPC_STATUS_ADD_CALL(caller_ptr, NOTIFY)`
                crate::proc::ipc_status_add_call(&mut self.procs[caller_idx], IpcCall::Notify);
                return IpcOutcome::Delivered;
            }

        // Phase 2: pending async messages.
        // C: `has_pending` (ASEND) + `try_async` — proc.c:1031-1070.
        // `try_async` failure (EAGAIN — table empty/endpoint mismatch)
        // falls through to the caller_q check, same as C.
        if let Some(async_src) = self.take_pending_async(caller_nr, src_endpoint) {
            if self.deliver_async(caller_idx, async_src) {
                // C: proc.c:1047 — `IPC_STATUS_ADD_CALL(caller_ptr, SENDA)`
                crate::proc::ipc_status_add_call(&mut self.procs[caller_idx], IpcCall::SendA);
                return IpcOutcome::Delivered;
            }
        }

        // Phase 3: sync sender queue. C: proc.c:1071-1095.
        // Intrusive chain walk: `caller_q_find` scans head-first via
        // `send_q_link`; `caller_q_remove` unlinks (fixing predecessor +
        // head/tail). The old VecDeque split find/remove existed to work
        // around queue-owns-subobject aliasing — with links living in the
        // sender slots, the borrows are ordinary sequential slot accesses.
        if let Some(sender_idx) = caller_q_find(self.procs, caller_idx, src_endpoint) {
            caller_q_remove(self.procs, caller_idx, sender_idx);
            // Copy sender's cached message into caller's deliver buffer.
            let sender_msg = self.procs[sender_idx].p_sendmsg;
            let sender_ep = self.procs[sender_idx].p_endpoint;
            let sender_from_kernel = self.procs[sender_idx]
                .p_misc_flags
                .is_set(MiscFlagsBits::SENDING_FROM_KERNEL);
            self.procs[caller_idx].p_delivermsg = sender_msg;
            self.procs[caller_idx].p_delivermsg.m_source = sender_ep;
            self.procs[caller_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
            // Wake up the sender. C: `RTS_UNSET(sender, RTS_SENDING)`.
            self.procs[sender_idx].p_rts_flags.clear(RtsFlagsBits::SENDING);
            // C: clear `SENDING_FROM_KERNEL` if it was set.
            self.procs[sender_idx].p_misc_flags.clear(MiscFlagsBits::SENDING_FROM_KERNEL);
            // C: proc.c:1070-1071 — determine call type and add to IPC status.
            //   call = (sender->p_misc_flags & MF_REPLY_PEND ? SENDREC : SEND)
            //   IPC_STATUS_ADD_CALL(caller_ptr, call)
            let delivered_call = if self.procs[sender_idx].p_misc_flags.is_set(MiscFlagsBits::REPLY_PEND) {
                IpcCall::SendRec
            } else {
                IpcCall::Send
            };
            crate::proc::ipc_status_add_call(&mut self.procs[caller_idx], delivered_call);
            // C: proc.c:1077-1078 — if message was from kernel, add flag.
            //   if (sender->p_misc_flags & MF_SENDING_FROM_KERNEL)
            //       IPC_STATUS_ADD_FLAGS(caller_ptr, IPC_FLG_MSG_FROM_KERNEL)
            if sender_from_kernel {
                crate::proc::ipc_status_add_flags(&mut self.procs[caller_idx], IPC_FLG_MSG_FROM_KERNEL);
            }
            // Clear MF_REPLY_PEND if this was a SENDREC reply delivery.
            if reply_pend {
                self.procs[caller_idx].p_misc_flags.clear(MiscFlagsBits::REPLY_PEND);
            }
            // C: proc.c:1082-1083 — if (sender->p_misc_flags & MF_SIG_DELAY)
            //   sig_delay_done(sender).
            // The sender is now no longer sending (RTS_SENDING cleared
            // above): if PM had requested a delayed stop on it, this is the
            // quiescent point that ends the delay. We record the sender and
            // let the ProcessTable-level dispatcher run `sig_delay_done`
            // (scheduler-aware `cause_signal`), see
            // `IpcEngine::sig_delay_sender` / `take_sig_delay_sender`.
            if self.procs[sender_idx].p_misc_flags.is_set(MiscFlagsBits::SIG_DELAY) {
                self.sig_delay_sender = Some(self.procs[sender_idx].p_nr);
            }
            return IpcOutcome::Delivered;
        }

        // Phase 4: block. C: proc.c:1096-1110.
        self.procs[caller_idx].p_getfrom_e = src_endpoint;
        self.procs[caller_idx].p_rts_flags.set(RtsFlagsBits::RECEIVING);
        IpcOutcome::Blocked
    }

    /// Pop a pending notification matching `src_endpoint` from the
    /// caller's `s_notify_pending` bitmap.
    ///
    /// C: `has_pending(caller, src_e, ...) -> endpoint_t` — proc.c:967-989.
    /// Returns the source endpoint of the highest-priority pending
    /// notification, or `None` if no match.
    ///
    /// The bit position in `s_notify_pending` is the sender's `priv_id`
    /// (NOT its `proc_nr`). We translate back to endpoint via the
    /// process table.
    fn take_pending_notify(
        &mut self,
        caller_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> Option<ProcNr> {
        let caller_idx = self.idx_of(caller_nr)?;
        let caller_priv_id = self.procs[caller_idx].priv_id?;
        let bitmap = {
            let caller_priv = self.priv_table.get(caller_priv_id)?;
            caller_priv.signals.s_notify_pending
        };
        if bitmap == 0 {
            return None;
        }

        // Scan bits low→high (C order).
        let mut bit = 0u32;
        while bit < 64 {
            if (bitmap & (1u64 << bit)) != 0 {
                // Find the process whose `priv_id` == bit.
                if let Some(sender_idx) = self.procs.iter().position(|p| p.priv_id == Some(bit as u16)) {
                    let sender_ep = self.procs[sender_idx].p_endpoint;
                    if src_endpoint == Endpoint::ANY || src_endpoint == sender_ep {
                        // Clear the bit (C: caller_priv->s_notify_pending &= ~(1<<bit)).
                        if let Some(caller_priv) = self.priv_table.get_mut(caller_priv_id) {
                            caller_priv.signals.s_notify_pending &= !(1u64 << bit);
                        }
                        return Some(self.procs[sender_idx].p_nr);
                    }
                }
            }
            bit += 1;
        }
        None
    }

    /// Pop a pending async message matching `src_endpoint` from the
    /// caller's `s_asyn_pending` bitmap.
    ///
    /// C: `has_pending(caller, src_e, ...) -> endpoint_t` (ASEND branch) —
    /// proc.c:1031-1070. Returns the source endpoint of the highest-priority
    /// pending async message, or `None` if no match.
    ///
    /// The bit position is the sender's `priv_id`, same as notify.
    /// The actual message lives in the sender's user-space SENDA table —
    /// no kernel copy exists; `deliver_async` re-reads it from user
    /// space (C: `try_one`, proc.c:1390-1497).
    fn take_pending_async(
        &mut self,
        caller_nr: ProcNr,
        src_endpoint: Endpoint,
    ) -> Option<Endpoint> {
        let caller_idx = self.idx_of(caller_nr)?;
        let caller_priv_id = self.procs[caller_idx].priv_id?;
        let bitmap = {
            let caller_priv = self.priv_table.get(caller_priv_id)?;
            caller_priv.signals.s_asyn_pending
        };
        if bitmap == 0 {
            return None;
        }

        // Scan bits low→high (C order).
        let mut bit = 0u32;
        while bit < 64 {
            if (bitmap & (1u64 << bit)) != 0
                && let Some(sender_idx) = self.procs.iter().position(|p| p.priv_id == Some(bit as u16)) {
                    let sender_ep = self.procs[sender_idx].p_endpoint;
                    if src_endpoint == Endpoint::ANY || src_endpoint == sender_ep {
                        // Clear the bit.
                        if let Some(caller_priv) = self.priv_table.get_mut(caller_priv_id) {
                            caller_priv.signals.s_asyn_pending &= !(1u64 << bit);
                        }
                        return Some(sender_ep);
                    }
                }
            bit += 1;
        }
        None
    }

    /// Deliver a pending async message from `sender_ep` to `caller_idx`.
    ///
    /// C: `try_one(ANY, src_ptr, dst_ptr)` — proc.c:1390-1497. Re-reads
    /// the sender's SENDA table from user space (the sender may have
    /// altered entries since `mini_senda` — proc.c:1425-1427), delivers
    /// the first entry aimed at the receiver, and marks that entry
    /// `AMF_DONE` in the user table. When every remaining entry is
    /// done/empty, the sender's table pointer is cleared
    /// (`s_asyntab`/`s_asynsize`, proc.c:1496-1497).
    ///
    /// Returns `true` if a message was delivered. The caller's pending
    /// bit was already cleared by `take_pending_async` (C clears it at
    /// try_one entry, proc.c:1409 — same ordering).
    fn deliver_async(&mut self, caller_idx: usize, sender_ep: Endpoint) -> bool {
        let Some(sender_idx) = self.idx_by_endpoint(sender_ep) else {
            return false;
        };
        let Some(sender_priv_id) = self.procs[sender_idx].priv_id else {
            return false;
        };

        // C: table + size from the sender's privilege structure
        // (proc.c:1405-1406). size == 0 or endpoint mismatch → EAGAIN
        // (proc.c:1411-1412) — nothing to deliver.
        let (table, size, asynendpoint) = {
            let Some(priv_) = self.priv_table.get(sender_priv_id) else {
                return false;
            };
            (priv_.signals.s_asyntab, priv_.signals.s_asynsize, priv_.signals.s_asynendpoint)
        };
        if size == 0 || asynendpoint != sender_ep {
            return false;
        }
        let table = VirBytes(table);
        let sender_ep_final = sender_ep;
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        // C: scan the table (proc.c:1422-1491). Delivery stops at the
        // first entry that matches (break — one message per receive).
        let mut done = true;
        let mut do_notify = false;
        let mut delivered = false;

        for i in 0..size {
            // C: A_RETR(i) — per-entry copy-in; failure skips the entry.
            let (dst_ep, msg, flags) = match self.user_copy.read_senda_entry(table, i) {
                Ok(t) => t,
                Err(_) => continue,
            };

            // C: flags == 0 → skip (proc.c:1434).
            if flags == AMF_EMPTY {
                continue;
            }
            // C: flags validation (proc.c:1436-1441). EINVAL entries
            // fall through to result write-back.
            let invalid = (flags & !AMF_ALL) != 0 || (flags & AMF_VALID) == 0;
            if (flags & AMF_DONE) != 0 {
                // C: already done (proc.c:1441).
                continue;
            }
            // C: done = FALSE — a not-yet-done entry exists (proc.c:1449).
            done = false;

            if invalid {
                // C: goto store_result with r = EINVAL (proc.c:1451-1452).
                let _ = self
                    .user_copy
                    .write_senda_result(table, i, EINVAL, flags | AMF_DONE);
                // C: do_notify on NOTIFY / error+NOTIFY_ERR (proc.c:1486-1487).
                if (flags & AMF_NOTIFY) != 0 {
                    do_notify = true;
                }
                break;
            }

            // C: message must be directed at the receiver (proc.c:1455).
            if dst_ep != caller_endpoint {
                continue;
            }
            // C: CANRECEIVE — the receiver must want this source
            // (proc.c:1457-1461; receive_e is ANY from try_async).
            if !Self::is_willing_to_receive(&self.procs[caller_idx], sender_ep_final) {
                continue;
            }
            // C: AMF_NOREPLY must not satisfy the receive part of a
            // SENDREC (proc.c:1463-1468).
            let noreply_block = (flags & AMF_NOREPLY) != 0
                && self.procs[caller_idx]
                    .p_misc_flags
                    .is_set(MiscFlagsBits::REPLY_PEND);
            if noreply_block {
                continue;
            }

            // C: deliver (proc.c:1470-1474).
            self.procs[caller_idx].p_delivermsg = msg;
            self.procs[caller_idx].p_delivermsg.m_source = sender_ep_final;
            self.procs[caller_idx]
                .p_misc_flags
                .set(MiscFlagsBits::DELIVERMSG);
            delivered = true;

            // C: store_result (proc.c:1479-1488) — result OK + AMF_DONE.
            let _ = self.user_copy.write_senda_result(table, i, OK, flags | AMF_DONE);
            if (flags & AMF_NOTIFY) != 0 {
                do_notify = true;
            }
            // C: break — one entry per receive (proc.c:1490).
            break;
        }

        if do_notify {
            // C: mini_notify(proc_addr(ASYNCM), src_ptr->p_endpoint)
            // — proc.c:1493-1494. ASYNCM = -5 (com.h:47).
            let _ = mini_notify_core(
                self.procs,
                self.priv_table,
                ProcNr(-5),
                sender_ep_final,
            );
        }

        if done {
            // C: all entries done/empty — clear the table pointer
            // (proc.c:1496-1497).
            if let Some(priv_) = self.priv_table.get_mut(sender_priv_id) {
                priv_.signals.s_asyntab = u64::MAX; // C: (vir_bytes) -1
                priv_.signals.s_asynsize = 0;
            }
        }

        delivered
    }

    /// Build a notification message in `dst.p_delivermsg`.
    ///
    /// Delegates to the free function [`build_notify_message`] — see
    /// that function's documentation for the full C reference and
    /// field semantics.
    fn build_notify_message(&mut self, dst_idx: usize, src: NotifySource) {
        build_notify_message(self.procs, self.priv_table, dst_idx, src);
    }

    // ── Notify ──

    /// Async notification. C: `mini_notify()` — proc.c:1122-1167.
    ///
    /// Delegates to [`mini_notify_core`]. See that function for the
    /// full C reference and implementation notes.
    pub fn notify(
        &mut self,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
    ) -> IpcOutcome {
        mini_notify_core(self.procs, self.priv_table, caller_nr, dst_endpoint)
    }

    // ── SendRec ──

    /// Atomic SEND + RECEIVE. C: `mini_sendrec()` — proc.c:1135-1175.
    ///
    /// 1. `send(caller, dst, msg, NONE)`.
    /// 2. If send delivered synchronously (dst was in RECEIVE), proceed
    ///    to `receive(caller, ANY)`.
    /// 3. If send blocked caller, set `MF_REPLY_PEND` so the eventual
    ///    reply routes through the SENDREC shortcut in `receive`.
    ///
    /// # MF_REPLY_PEND semantics (P0-12-5 fix)
    ///
    /// `MF_REPLY_PEND` only skips the *notify* check in `receive`; it
    /// does NOT skip async or caller_q. The reply arrives via the
    /// sender's `mini_send` path, which delivers directly to
    /// `p_delivermsg` when the caller is in RECEIVE — that path is
    /// handled by `receive` Phase 3 (caller_q).
    pub fn sendrec(
        &mut self,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        msg: &Message,
    ) -> IpcOutcome {
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };

        let send_outcome = self.send(caller_nr, dst_endpoint, msg, SendFlags::NONE);

        match send_outcome {
            IpcOutcome::Delivered => {
                // SEND phase delivered; now RECEIVE-ANY for the reply.
                self.receive(caller_nr, Endpoint::ANY)
            }
            IpcOutcome::Blocked => {
                // Caller is blocked in SENDING. When the reply arrives,
                // the target's `send` path will deliver directly (caller
                // is in RECEIVE for the reply). Set MF_REPLY_PEND so
                // caller's `receive` skips notify checks (preserving
                // SENDREC atomicity — the reply should come from the
                // target, not be intercepted by a pending notify).
                self.procs[caller_idx].p_misc_flags.set(MiscFlagsBits::REPLY_PEND);
                IpcOutcome::Blocked
            }
            IpcOutcome::Error(e) => IpcOutcome::Error(e),
        }
    }

    // ── SENDA ──

    /// Batch async send. C: `mini_senda()` — proc.c:1331-1342, delegating
    /// to `try_deliver_senda` — proc.c:1200-1326.
    ///
    /// The kernel never caches the table: entries are read from user
    /// space one at a time (`UserCopy::read_senda_entry`, C: `A_RETR` —
    /// proc.c:1244) and per-entry results written back one at a time
    /// (`UserCopy::write_senda_result`, C: `A_INSRT` — proc.c:1307).
    /// Undelivered entries are retried by re-reading the user table
    /// when the target next receives (`deliver_async`, C: `try_one` —
    /// proc.c:1390-1497).
    ///
    /// The caller never blocks. Per-entry errors go into the table's
    /// `result` field (with `AMF_DONE`), not the return value; the
    /// function returns `Delivered` (C: always `OK`) unless a
    /// pre-check fails: non-`SYS_PROC` caller (C: `EPERM`, proc.c:1336)
    /// or the duplicated size sanity check (C: `EDOM`, proc.c:1233;
    /// primary check is the SENDA syscall path — proc.c:681).
    pub fn senda(&mut self, caller_nr: ProcNr, table: VirBytes, size: usize) -> IpcOutcome {
        // C: mini_senda — SYS_PROC check (proc.c:1331-1342).
        let caller_idx = match self.idx_of(caller_nr) {
            Some(i) => i,
            None => return IpcOutcome::Error(IpcError::DeadSrcDst),
        };
        let caller_priv_id = match self.procs[caller_idx].priv_id {
            Some(id) => id,
            // C: "caller has no privilege structure" → EPERM (proc.c:1337).
            None => return IpcOutcome::Error(IpcError::Permission),
        };
        let caller_is_sys = self
            .priv_table
            .get(caller_priv_id)
            .map(KPriv::is_sys_proc)
            .unwrap_or(false);
        if !caller_is_sys {
            return IpcOutcome::Error(IpcError::Permission);
        }
        let caller_endpoint = self.procs[caller_idx].p_endpoint;

        // C: clear table first (proc.c:1217-1219); restored only if
        // entries remain undelivered (proc.c:1320-1323).
        {
            let Some(priv_) = self.priv_table.get_mut(caller_priv_id) else {
                return IpcOutcome::Error(IpcError::Permission);
            };
            priv_.signals.s_asyntab = u64::MAX; // C: (vir_bytes) -1
            priv_.signals.s_asynsize = 0;
            priv_.signals.s_asynendpoint = caller_endpoint;
        }

        // C: size == 0 — nothing to do (proc.c:1221).
        if size == 0 {
            return IpcOutcome::Delivered;
        }

        // C: duplicated size sanity check (proc.c:1233). Same EDOM →
        // BadCall mapping as the syscall path.
        if size > 16 * PROC_TABLE_SIZE {
            return IpcOutcome::Error(IpcError::BadCall);
        }

        let mut done = true;
        let mut do_notify = false;

        for i in 0..size {
            // C: A_RETR(i) — per-entry copy-in. On copy failure C
            // complains and skips the entry (asyn_error has no result
            // write-back); the entry stays un-DONE for retry.
            let (mut dst_ep, msg, flags) = match self.user_copy.read_senda_entry(table, i) {
                Ok(t) => t,
                Err(_) => continue,
            };

            // C: flags == 0 → skip empty entries (proc.c:1248).
            if flags == AMF_EMPTY {
                continue;
            }
            // C: flags must contain only valid bits (proc.c:1251) and
            // must contain a message (proc.c:1255-1258). C's asyn_error
            // path prints and moves on without write-back.
            if (flags & !AMF_ALL) != 0 || (flags & AMF_VALID) == 0 {
                continue;
            }
            // C: AMF_DONE → already processed (proc.c:1259).
            if (flags & AMF_DONE) != 0 {
                continue;
            }

            // C: A_RETR SELF replacement (proc.c:1183-1185).
            if dst_ep == Endpoint::SELF {
                dst_ep = caller_endpoint;
            }

            // C: destination checks (proc.c:1261-1274).
            //   isokendpt fail            → EDEADSRCDST
            //   iskerneln(dst_p)          → ECALLDENIED (no asyn to kernel)
            //   !may_asynsend_to          → ECALLDENIED (IPC mask; self
            //                               always allowed — priv.h:87)
            //   RTS_NO_ENDPOINT on target → EDEADSRCDST
            let mut r: i32 = OK;
            let mut dst_idx_opt: Option<usize> = None;
            if let Some(di) = self.idx_by_endpoint(dst_ep) {
                if self.procs[di].p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) {
                    r = EDEADSRCDST;
                } else if self.procs[di].p_nr.0 <= 0 {
                    // iskerneln: slot number in the task region (C:
                    // `dst_p <= 0` — proc.h iskerneln).
                    r = ECALLDENIED;
                } else {
                    let may = self.procs[di]
                        .priv_id
                        .map(|pid| {
                            self.priv_table
                                .get(caller_priv_id)
                                .map(|cp| cp.may_send_to(pid))
                                .unwrap_or(false)
                        })
                        .unwrap_or(false)
                        || self.procs[di].p_nr == caller_nr;
                    if !may {
                        r = ECALLDENIED;
                    } else {
                        dst_idx_opt = Some(di);
                    }
                }
            } else {
                r = EDEADSRCDST;
            }

            // C: check if dst is blocked waiting for this message
            // (proc.c:1276-1291). AMF_NOREPLY must not satisfy the
            // receive part of a SENDREC (MF_REPLY_PEND).
            let delivered = match dst_idx_opt {
                Some(di) if r == OK => {
                    let willing = Self::is_willing_to_receive(&self.procs[di], caller_endpoint);
                    let noreply_block = (flags & AMF_NOREPLY) != 0
                        && self.procs[di]
                            .p_misc_flags
                            .is_set(MiscFlagsBits::REPLY_PEND);
                    if willing && !noreply_block {
                        // Direct delivery: C: proc.c:1284-1288.
                        self.procs[di].p_delivermsg = msg;
                        self.procs[di].p_delivermsg.m_source = caller_endpoint;
                        self.procs[di]
                            .p_misc_flags
                            .set(MiscFlagsBits::DELIVERMSG);
                        crate::proc::ipc_status_add_call(&mut self.procs[di], IpcCall::SendA);
                        self.procs[di].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
                        true
                    } else {
                        // C: set_sys_bit(priv(dst)->s_asyn_pending,
                        // priv(caller)->s_id) — proc.c:1293-1297. The
                        // bit index is the sender's sys_id (priv_id).
                        if let Some(dst_pid) = self.procs[di].priv_id {
                            if let Some(dst_priv) = self.priv_table.get_mut(dst_pid) {
                                dst_priv.signals.s_asyn_pending |= 1u64 << caller_priv_id;
                            }
                        }
                        done = false;
                        false
                    }
                }
                _ => false,
            };
            if !delivered && r == OK && dst_idx_opt.is_some() {
                // Pending (not delivered, no error) — C: `continue`
                // without result write-back (proc.c:1292-1298).
                continue;
            }
            if delivered {
                // fall through to result write-back with r == OK
            }

            // C: store results (proc.c:1300-1307).
            //   tabent.result = r; tabent.flags = flags | AMF_DONE;
            //   A_INSRT ignores copy errors.
            let _ = self
                .user_copy
                .write_senda_result(table, i, r, flags | AMF_DONE);
            if (flags & AMF_NOTIFY) != 0 {
                do_notify = true;
            } else if r != OK && (flags & AMF_NOTIFY_ERR) != 0 {
                do_notify = true;
            }
        }

        if do_notify {
            // C: mini_notify(proc_addr(ASYNCM), caller_ptr->p_endpoint)
            // — proc.c:1317-1318. ASYNCM = -5 (com.h:47).
            let _ = mini_notify_core(self.procs, self.priv_table, ProcNr(-5), caller_endpoint);
        }

        if !done {
            // C: proc.c:1320-1323 — remember table for retry.
            if let Some(priv_) = self.priv_table.get_mut(caller_priv_id) {
                priv_.signals.s_asyntab = table.0;
                priv_.signals.s_asynsize = size;
            }
        }

        IpcOutcome::Delivered
    }

    // ── Deliver (delayed copy to user space) ──

    /// Deliver pending message to user space.
    ///
    /// C: `delivermsg()` — proc.c:263-294. Called by `switch_to_user`
    /// before restoring user context.
    ///
    /// # P0 FIX (FIX-7)
    ///
    /// Previous implementation was missing entirely. This version
    /// implements the two-failure policy:
    /// - Success → clear `MF_DELIVERMSG` (+ `MF_MSGFAILED` if set).
    /// - First `PageFault` → set `MF_MSGFAILED`, return `PageFault`
    ///   (caller routes to `vm_suspend(VMS_PAGEFAULT)`).
    /// - Second consecutive failure (`MF_MSGFAILED` already set) →
    ///   return `Segfault` (caller routes to `cause_sig(SIGSEGV)`).
    /// - `OutOfBounds` → `Segfault` immediately.
    ///
    /// # UserCopy injection
    ///
    /// The actual copy is delegated to the injected `UserCopy` impl.
    /// Tests use `KernelUserCopy` (no-op); production code injects the
    /// arch-specific implementation.
    pub fn deliver_message(&mut self, nr: ProcNr) -> DeliverResult {
        let idx = match self.idx_of(nr) {
            Some(i) => i,
            // No process — shouldn't happen (caller checked). Treat as
            // segfault to surface the bug.
            None => return DeliverResult::Segfault,
        };

        // Delegate to the free function `delivermsg` (FIX-20, Phase 1B).
        // This shares the two-failure policy logic with
        // `ProcessTable::process_misc_flags`, which also calls `delivermsg`.
        delivermsg(&mut self.procs[idx], self.user_copy)
    }

    // ── Permission check ──

    /// Check IPC permission. C: `do_sync_ipc` permission layers —
    /// proc.c:479-597.
    ///
    /// # P0 FIX (FIX-9)
    ///
    /// Previous implementation was missing. This version implements
    /// four layers aligned with C:
    /// 1. Endpoint validity (`RTS_NO_ENDPOINT` → `DeadSrcDst`).
    /// 2. IPC whitelist (`s_ipc_to` bitmap → `CallDenied`).
    /// 3. Trap mask (`s_trap_mask` bit for `call` → `TrapDenied`).
    /// 4. Kernel task restriction (only `SendRec` allowed → `TrapDenied`).
    pub fn check_ipc_permission(
        &self,
        caller_nr: ProcNr,
        dst_endpoint: Endpoint,
        call: IpcCall,
    ) -> Result<(), IpcError> {
        let caller_idx = self.idx_of(caller_nr).ok_or(IpcError::DeadSrcDst)?;
        let caller_priv_id = self.procs[caller_idx].priv_id.ok_or(IpcError::CallDenied)?;
        let caller_priv = self.priv_table.get(caller_priv_id).ok_or(IpcError::CallDenied)?;

        // Layer 1: endpoint validity. C: proc.c:487-495.
        let dst_idx = self.idx_by_endpoint(dst_endpoint);
        match dst_idx {
            None => return Err(IpcError::DeadSrcDst),
            Some(i) if self.procs[i].p_rts_flags.is_set(RtsFlagsBits::NO_ENDPOINT) => {
                return Err(IpcError::DeadSrcDst);
            }
            _ => {}
        }

        // Layer 2: IPC whitelist. C: `may_send_to` — ipc.h.
        if let Some(i) = dst_idx
            && let Some(dst_pid) = self.procs[i].priv_id
                && !caller_priv.may_send_to(dst_pid) {
                    return Err(IpcError::CallDenied);
                }

        // Layer 3: trap mask. C: `priv(caller)->s_trap_mask & (1 << call_nr)`
        // — proc.c:552. C's `short s_trap_mask` sign-extends to int for the
        // AND, so `SRV_T = ~0` (stored 0xFFFF) allows SENDA (call_nr=16).
        // The sign extension happens once at the wire boundary
        // (`TrapMask::from_wire` in kpriv.rs), so the stored mask is already
        // in the effective form C checks against. `call as u32` covers
        // SENDA=16 (out of `u16` bit range).
        let call_bit = crate::capability::TrapMask::from_bits(1u32 << (call as u32));
        if !caller_priv.ipc.s_trap_mask.contains(call_bit) {
            return Err(IpcError::TrapDenied);
        }

        // Layer 4: kernel task restriction. C: proc.c:560-566.
        // Calls TO kernel tasks (p_nr < 0) may only be SENDREC or RECEIVE
        // — kernel tasks always reply and may not block if the caller
        // doesn't receive. C checks the TARGET (`iskerneln(src_dst_p)`),
        // not the caller: `call_nr != SENDREC && call_nr != RECEIVE &&
        // iskerneln(src_dst_p)`.
        if call != IpcCall::SendRec
            && call != IpcCall::Receive
            && let Some(i) = dst_idx
            && self.procs[i].is_kernel_task()
        {
            return Err(IpcError::TrapDenied);
        }

        Ok(())
    }

    // ── Top-level dispatch ──

    /// IPC entry point. C: `do_ipc(r1, r2, r3)` — proc.c:599-698.
    ///
    /// Dispatches to `send`/`receive`/`sendrec`/`notify`/`senda` based
    /// on `call`. Performs permission check first (FIX-9).
    ///
    /// # SENDA table passing (P0-12-3 fix)
    ///
    /// C passes the SENDA table pointer and size via trap-frame registers
    /// `r3` and `r2` (proc.c:673, 683), NOT via message fields. The Rust
    /// API mirrors this: `senda_table` is a separate `Option<(ptr, count)>`
    /// parameter, only used when `call == SendA`. The syscall dispatcher
    /// extracts `r2`/`r3` from the trap frame and passes them here.
    ///
    /// # Skeleton
    ///
    /// The full `do_ipc` also handles `MINIX_KERNINFO` (not yet implemented).
    /// IPC status encoding (`IPC_STATUS_ADD_CALL`) is implemented — see
    /// `proc::ipc_status_add_call` / `ipc_status_add_flags`, wired into
    /// all delivery paths (send, receive, mini_notify, senda).
    pub fn do_ipc(
        &mut self,
        caller_nr: ProcNr,
        call: IpcCall,
        dst_endpoint: Endpoint,
        msg: &Message,
        flags: SendFlags,
        senda_table: Option<(VirBytes, usize)>,
    ) -> IpcOutcome {
        // Permission check first (except for NOTIFY which has relaxed
        // rules in C — TODO: align with C's notify permission path).
        if let Err(e) = self.check_ipc_permission(caller_nr, dst_endpoint, call) {
            return IpcOutcome::Error(e);
        }

        match call {
            IpcCall::Send | IpcCall::SendNb => self.send(
                caller_nr, dst_endpoint, msg,
                if call == IpcCall::SendNb {
                    flags | SendFlags::NON_BLOCKING
                } else {
                    flags
                },
            ),
            IpcCall::Receive => self.receive(caller_nr, dst_endpoint),
            IpcCall::SendRec => self.sendrec(caller_nr, dst_endpoint, msg),
            IpcCall::Notify => self.notify(caller_nr, dst_endpoint),
            IpcCall::SendA => {
                // C: `size_t msg_size = (size_t) r2;` (proc.c:673)
                //     `return mini_senda(caller_ptr, (asynmsg_t *) r3, msg_size);` (proc.c:683)
                let (table_ptr, count) = match senda_table {
                    Some(tc) => tc,
                    None => return IpcOutcome::Error(IpcError::BadCall),
                };
                // C: limit size to 16*(NR_TASKS + NR_PROCS) — proc.c:681.
                let max_count = 16 * PROC_TABLE_SIZE;
                if count > max_count {
                    // C: returns EDOM — mapped to BadCall (out-of-domain).
                    return IpcOutcome::Error(IpcError::BadCall);
                }
                // No table pre-copy: `senda` reads entries from user space
                // one at a time (C: `A_RETR`), keeping the kernel heap-free
                // and retry semantics C-isomorphic (re-read on retry).
                self.senda(caller_nr, table_ptr, count)
            }
        }
    }
}

/// Convert a logical process number to an array index.
/// Same as `proc_table::nr_to_idx`.
#[inline]
fn nr_to_idx(nr: ProcNr) -> Option<usize> {
    use crate::proc_table::NR_TASKS;
    let offset = nr.0 as isize + NR_TASKS as isize;
    if offset < 0 || offset as usize >= PROC_TABLE_SIZE {
        return None;
    }
    Some(offset as usize)
}

// ── Free-function notification core (shared by syscall + IRQ paths) ──

/// Notification source type — determines which `MessNotify` fields to fill.
///
/// C: `BuildNotifyMessage` switches on `src` (a ProcNr) and checks for
/// `HARDWARE` (com.h:52, `= KERNEL = -1`) or `SYSTEM` (com.h:50, `= -2`).
/// Rust uses a typed enum instead of magic-number ProcNr comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifySource {
    /// Regular process source — only `timestamp` is filled.
    Process(ProcNr),
    /// Hardware interrupt source (`HARDWARE = KERNEL = -1`).
    /// Fills `interrupts` from `s_int_pending`, then clears it.
    Hardware,
    /// System event source (`SYSTEM = -2`).
    /// Fills `sigset` from `s_sig_pending`, then clears it.
    System,
}

impl NotifySource {
    /// Classify a caller ProcNr into a [`NotifySource`].
    ///
    /// C: `switch (src) { case HARDWARE: ... case SYSTEM: ... }` — proc.c:102-114.
    fn from_caller_nr(caller_nr: ProcNr) -> Self {
        if caller_nr == proc_nr::KERNEL {
            NotifySource::Hardware
        } else if caller_nr == proc_nr::SYSTEM {
            NotifySource::System
        } else {
            NotifySource::Process(caller_nr)
        }
    }
}

/// Build a notification message in `dst.p_delivermsg`.
///
/// C: `BuildNotifyMessage(&dst->p_delivermsg, src_proc_nr, dst_ptr)` —
/// proc.c:98-114 (macro) + proc.c:1029-1030 / 1150-1151 (call sites).
///
/// # C behavior
///
/// 1. `memset(m_ptr, 0, sizeof(*m_ptr))` — zero the message.
/// 2. `m_type = NOTIFY_MESSAGE` (= 0x1000, com.h:90).
/// 3. `m_notify.timestamp = get_monotonic()`.
/// 4. If `src == HARDWARE`: copy `priv(dst)->s_int_pending` →
///    `m_notify.interrupts`, clear `s_int_pending`.
/// 5. If `src == SYSTEM`: copy `priv(dst)->s_sig_pending` →
///    `m_notify.sigset`, clear `s_sig_pending`.
/// 6. For regular process sources: only `timestamp` + `m_type` are set.
///
/// The caller then sets `m_source = sender_endpoint` (proc.c:1030, 1151).
fn build_notify_message(
    procs: &mut [KProcess],
    priv_table: &mut PrivTable,
    dst_idx: usize,
    src: NotifySource,
) {
    use crate::clock::get_monotonic;

    // C: memset(m_ptr, 0, ...) — zero the entire message.
    procs[dst_idx].p_delivermsg = Message::default();
    // C: m_type = NOTIFY_MESSAGE — com.h:90.
    procs[dst_idx].p_delivermsg.m_type = NOTIFY_MESSAGE;

    // Fill m_notify payload. Safe because MessNotify is Copy + zeroable.
    // C: m_notify.timestamp = get_monotonic()
    let timestamp = get_monotonic();
    let mut interrupts = 0u64;
    let mut sigset = 0u64;

    match src {
        NotifySource::Hardware => {
            // C: m_notify.interrupts = priv(dst)->s_int_pending; clear it.
            if let Some(dst_priv_id) = procs[dst_idx].priv_id
                && let Some(dst_priv) = priv_table.get_mut(dst_priv_id) {
                    interrupts = dst_priv.signals.s_int_pending as u64;
                    dst_priv.signals.s_int_pending = 0;
                }
        }
        NotifySource::System => {
            // C: m_notify.sigset = priv(dst)->s_sig_pending; clear it.
            if let Some(dst_priv_id) = procs[dst_idx].priv_id
                && let Some(dst_priv) = priv_table.get_mut(dst_priv_id) {
                    sigset = dst_priv.signals.s_sig_pending.get();
                    dst_priv.signals.s_sig_pending = crate::proc::SigSet::empty();
                }
        }
        NotifySource::Process(_) => {
            // Regular process: only timestamp is set (C falls through switch).
        }
    }

    // Write the MessNotify payload into the message union.
    // SAFETY: MessNotify is #[repr(C)] and fits within MESSAGE_PAYLOAD_SIZE
    // (compile-time asserted in notify.rs). We're writing to a zeroed
    // MessageUnion, so all fields are valid.
    procs[dst_idx].p_delivermsg.m_u.m_notify = MessNotify::new(timestamp, interrupts, sigset);
}

/// Core notification logic — shared by `IpcEngine::notify` (syscall path)
/// and `kernel_mini_notify` (IRQ path).
///
/// C: `mini_notify()` — proc.c:1122-1167.
///
/// Never blocks, never fails (the only error path is invalid dst
/// endpoint, which maps to `Error(DeadSrcDst)`).
///
/// - If dst is in RECEIVE matching caller → deliver directly to
///   `p_delivermsg` + wake dst.
/// - Else set bit in `priv(dst).s_notify_pending` for later delivery.
pub fn mini_notify_core(
    procs: &mut [KProcess],
    priv_table: &mut PrivTable,
    caller_nr: ProcNr,
    dst_endpoint: Endpoint,
) -> IpcOutcome {
    let caller_idx = match nr_to_idx(caller_nr) {
        Some(i) => i,
        None => return IpcOutcome::Error(IpcError::DeadSrcDst),
    };
    let caller_endpoint = procs[caller_idx].p_endpoint;
    let caller_priv_id = procs[caller_idx].priv_id;

    let dst_idx = match procs.iter().position(|p| p.p_endpoint == dst_endpoint) {
        Some(i) => i,
        None => return IpcOutcome::Error(IpcError::DeadSrcDst),
    };

    // Direct delivery if dst is willing to receive from caller.
    if IpcEngine::is_willing_to_receive(&procs[dst_idx], caller_endpoint) {
        let src = NotifySource::from_caller_nr(caller_nr);
        build_notify_message(procs, priv_table, dst_idx, src);
        procs[dst_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        // C: proc.c:1154 — `IPC_STATUS_ADD_CALL(dst_ptr, NOTIFY)`
        crate::proc::ipc_status_add_call(&mut procs[dst_idx], IpcCall::Notify);
        procs[dst_idx].p_rts_flags.clear(RtsFlagsBits::RECEIVING);
        return IpcOutcome::Delivered;
    }

    // C: `priv(dst)->s_notify_pending |= (1 << priv_id(caller))` — proc.c:1165.
    // The bit position is the CALLER's priv_id, not proc_nr.
    if let Some(caller_pid) = caller_priv_id
        && (caller_pid as u32) < 64
            && let Some(dst_priv_id) = procs[dst_idx].priv_id
                && let Some(dst_priv) = priv_table.get_mut(dst_priv_id) {
                    dst_priv.signals.s_notify_pending |= 1u64 << caller_pid;
                }
    // If the caller has no priv_id (user process), the notification
    // is silently dropped — matching Minix3 behavior where only
    // system processes have privilege entries. User-process
    // notifications route through PM.
    IpcOutcome::Delivered
}

/// Kernel-level `mini_notify` using global process/privilege tables.
///
/// This is the IRQ-context entry point — called by [`crate::irq_manager`]
/// after a hardware interrupt handler returns. It uses the global
/// `PROC_TABLE` / `PRIV_TABLE` (both `static mut`, BKL-protected).
///
/// C: `mini_notify(proc_addr(HARDWARE), hook->proc_nr_e)` — do_irqctl.c:170.
///
/// # Safety
///
/// Caller must hold the BKL (Big Kernel Lock). This is satisfied by
/// the trap entry path, which acquires the BKL before dispatching.
pub fn kernel_mini_notify(caller_nr: ProcNr, dst_endpoint: Endpoint) -> IpcOutcome {
    // SAFETY: Caller must hold BKL. The trap entry path acquires BKL
    // before exception/IRQ dispatch; the syscall path holds BKL
    // throughout. Both `proc_table()` and `priv_table()` return
    // `&'static mut` to global BSS — we only borrow each once per call.
    let procs = unsafe { crate::proc_table() }.procs_slice_mut();
    let priv_table = unsafe { crate::priv_table() };
    mini_notify_core(procs, priv_table, caller_nr, dst_endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use crate::proc::RtsFlags;
    use crate::proc_table::NR_TASKS;

    // ── Test helpers ──

    /// ProcNr for a test process placed at `array_idx` in a small slice.
    ///
    /// `nr_to_idx(nr) = nr + NR_TASKS`, so `nr = array_idx - NR_TASKS`.
    /// Using this helper ensures tests exercise the real `nr_to_idx`
    /// mapping rather than bypassing it with unrealistic ProcNr values
    /// (e.g. `KProcess::new(1, ...)` would map to index 6, panicking on
    /// a 2-element slice).
    fn test_nr(array_idx: usize) -> ProcNr {
        ProcNr(array_idx as i32 - NR_TASKS as i32)
    }

    /// Create a test process at `array_idx` with the given endpoint.
    /// The process's `p_nr` is set so that `nr_to_idx(p_nr) == array_idx`.
    fn make_test_proc(array_idx: usize, ep: Endpoint) -> KProcess {
        KProcess::new(test_nr(array_idx), ep)
    }

    // ── Type / encoding tests ──

    #[test]
    fn test_ipc_call_variants() {
        assert_eq!(IpcCall::Send as u8, 1);
        assert_eq!(IpcCall::Receive as u8, 2);
        assert_eq!(IpcCall::SendRec as u8, 3);
        assert_eq!(IpcCall::Notify as u8, 4);
        assert_eq!(IpcCall::SendNb as u8, 5);
        assert_eq!(IpcCall::SendA as u8, 16);
    }

    #[test]
    fn test_ipc_call_from_raw() {
        assert_eq!(IpcCall::from_raw(1), Some(IpcCall::Send));
        assert_eq!(IpcCall::from_raw(16), Some(IpcCall::SendA));
        assert_eq!(IpcCall::from_raw(6), None); // MINIX_KERNINFO not modeled
        assert_eq!(IpcCall::from_raw(99), None);
    }

    #[test]
    fn test_ipc_error_variants() {
        let _ = IpcError::Deadlock;
        let _ = IpcError::DeadSrcDst;
        let _ = IpcError::NotReady;
        let _ = IpcError::BadCall;
        let _ = IpcError::Fault;
        let _ = IpcError::CallDenied;
        let _ = IpcError::TrapDenied;
    }

    #[test]
    fn test_ipc_outcome_predicates() {
        assert!(IpcOutcome::Delivered.is_delivered());
        assert!(!IpcOutcome::Delivered.is_blocked());
        assert!(IpcOutcome::Blocked.is_blocked());
        assert_eq!(IpcOutcome::Error(IpcError::Deadlock).err(), Some(IpcError::Deadlock));
        assert_eq!(IpcOutcome::Delivered.err(), None);
    }

    #[test]
    fn test_send_flags_p0_fix_values() {
        // FIX-1 / FIX-2: values must align with C `ipc.h:11-12`.
        assert_eq!(SendFlags::NON_BLOCKING.bits(), 0x0080);
        assert_eq!(SendFlags::FROM_KERNEL.bits(), 0x0100);
        // The previous wrong values must NOT be present.
        assert_ne!(SendFlags::NON_BLOCKING.bits(), 0x01);
        assert_ne!(SendFlags::FROM_KERNEL.bits(), 0x02);

        // bitflags! API works.
        let both = SendFlags::NON_BLOCKING | SendFlags::FROM_KERNEL;
        assert!(both.contains(SendFlags::NON_BLOCKING));
        assert!(both.contains(SendFlags::FROM_KERNEL));
        assert_eq!(SendFlags::NONE, SendFlags::empty());
    }

    // ── Sender wait queue tests (AT-2 / ARCH-2 — intrusive FIFO) ──

    #[test]
    fn test_caller_q_push_find_remove_fifo() {
        // Three slots; queue on slot 2 (target). FIFO order via
        // caller_q_push; find walks head-first; remove unlinks middle.
        let mut procs = crate::test_helpers::scratch_procs([
            make_test_proc(0, Endpoint(11)),
            make_test_proc(1, Endpoint(22)),
            make_test_proc(2, Endpoint(33)),
        ]);
        assert!(caller_q_is_empty(&procs, 2));
        caller_q_push(&mut procs, 2, 0);
        caller_q_push(&mut procs, 2, 1);
        assert_eq!(caller_q_len(&procs, 2), 2);
        // Chain: head=nr(0) → nr(1); tail=nr(1).
        assert_eq!(procs[2].caller_q_head, Some(test_nr(0)));
        assert_eq!(procs[2].caller_q_tail, Some(test_nr(1)));
        assert_eq!(procs[0].send_q_link, Some(test_nr(1)));
        assert_eq!(procs[1].send_q_link, None);
        // ANY matches the head.
        assert_eq!(caller_q_find(&procs, 2, Endpoint::ANY), Some(0));
        // Specific endpoint match walks the chain.
        assert_eq!(caller_q_find(&procs, 2, Endpoint(22)), Some(1));
        // Remove the middle sender: head link must be fixed.
        assert!(caller_q_remove(&mut procs, 2, 1));
        assert_eq!(caller_q_len(&procs, 2), 1);
        assert_eq!(procs[2].caller_q_head, Some(test_nr(0)));
        assert_eq!(procs[2].caller_q_tail, Some(test_nr(0)));
        assert!(procs[1].send_q_link.is_none());
        // Remove the last sender: queue becomes empty.
        assert!(caller_q_remove(&mut procs, 2, 0));
        assert!(caller_q_is_empty(&procs, 2));
        // Removing a non-member returns false.
        assert!(!caller_q_remove_by_nr(&mut procs, 2, test_nr(1)));
    }

    #[test]
    fn test_caller_q_remove_by_nr_unlinks() {
        // remove_by_nr resolves the ProcNr → slot index itself (same
        // walk as C's clear_ipc / abort_proc_ipc_send loops).
        let mut procs = crate::test_helpers::scratch_procs([
            make_test_proc(0, Endpoint(11)),
            make_test_proc(1, Endpoint(22)),
        ]);
        caller_q_push(&mut procs, 1, 0);
        assert!(caller_q_remove_by_nr(&mut procs, 1, test_nr(0)));
        assert!(caller_q_is_empty(&procs, 1));
        assert!(procs[0].send_q_link.is_none());
    }

    // ── Deadlock detection (P0) ──

    #[test]
    fn test_deadlock_no_cycle_empty_table() {
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, ProcNr(0), Endpoint(1));
        assert!(result.is_none());
    }

    /// Build a 2-proc scenario: caller A, dst B. B is in SENDING or
    /// RECEIVING state and waiting on `b_chain_target`.
    fn build_two_proc_scenario(
        a_ep: Endpoint,
        b_ep: Endpoint,
        b_state: RtsFlagsBits,
        b_chain_target: Endpoint,
    ) -> crate::test_helpers::TestProcArray<2> {
        let mut a = make_test_proc(0, a_ep);
        let mut b = make_test_proc(1, b_ep);
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(b_state);
        match b_state {
            RtsFlagsBits::SENDING => b.p_sendto_e = b_chain_target,
            RtsFlagsBits::RECEIVING => b.p_getfrom_e = b_chain_target,
            _ => panic!("test setup: b_state must be SENDING or RECEIVING"),
        }
        crate::test_helpers::scratch_procs([a, b])
    }

    #[test]
    fn test_deadlock_send_send_two_cycle_is_deadlock() {
        // A sends to B, B is SENDING to A — classic SEND↔SEND deadlock.
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::SENDING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(result.is_some(), "SEND↔SEND 2-cycle must be a deadlock");
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 2);
        assert_eq!(cycle.direction, DeadlockDirection::Send);
    }

    #[test]
    fn test_deadlock_send_receive_two_cycle_not_deadlock() {
        // A sends to B, B is RECEIVING from A — request/reply pattern,
        // NOT a deadlock.
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(
            result.is_none(),
            "SEND↔RECEIVE 2-cycle must NOT be a deadlock (request/reply pattern)"
        );
    }

    #[test]
    fn test_deadlock_receive_send_two_cycle_not_deadlock() {
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::SENDING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Receive, test_nr(0), Endpoint(2));
        assert!(
            result.is_none(),
            "RECEIVE↔SEND 2-cycle must NOT be a deadlock"
        );
    }

    #[test]
    fn test_deadlock_receive_receive_two_cycle_is_deadlock() {
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Receive, test_nr(0), Endpoint(2));
        assert!(
            result.is_some(),
            "RECEIVE↔RECEIVE 2-cycle must be a deadlock"
        );
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 2);
        assert_eq!(cycle.direction, DeadlockDirection::Receive);
    }

    #[test]
    fn test_deadlock_no_cycle_when_target_runnable() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::new();
        b.p_getfrom_e = Endpoint::NONE;
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Receive, test_nr(0), Endpoint(2));
        assert!(result.is_none());
    }

    #[test]
    fn test_deadlock_send_state_mismatch() {
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(99),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(result.is_none());
    }

    #[test]
    fn test_deadlock_three_proc_send_cycle() {
        // 3-proc SEND cycle: A → B → C → A.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        let mut c = make_test_proc(2, Endpoint(3));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        b.p_sendto_e = Endpoint(3);
        c.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        c.p_sendto_e = Endpoint(1);
        let mut procs = crate::test_helpers::scratch_procs([a, b, c]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(result.is_some(), "3-proc SEND cycle must be detected");
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 3);
        assert!(cycle.chain[..cycle.chain_len].contains(&test_nr(0)));
        assert!(cycle.chain[..cycle.chain_len].contains(&test_nr(1)));
        assert!(cycle.chain[..cycle.chain_len].contains(&test_nr(2)));
    }

    #[test]
    fn test_deadlock_mixed_chain_cycle() {
        // P0 FIX-3 regression test: mixed-chain deadlock.
        // A send→B, B receive←C, C send→A.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        let mut c = make_test_proc(2, Endpoint(3));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
        b.p_getfrom_e = Endpoint(3);
        c.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        c.p_sendto_e = Endpoint(1);
        let mut procs = crate::test_helpers::scratch_procs([a, b, c]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Send, test_nr(0), Endpoint(2));
        assert!(
            result.is_some(),
            "Mixed-chain deadlock (A send→B, B receive←C, C send→A) must be detected \
             by dynamic blocked_on() — FIX-3 regression"
        );
        let cycle = result.unwrap();
        assert_eq!(cycle.group_size, 3);
    }

    // ── Send tests (P0-12-1) ──

    #[test]
    fn test_send_when_target_receiving() {
        // Path A: dst is in RECEIVE matching caller → direct delivery.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
        b.p_getfrom_e = Endpoint(1);
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::FROM_KERNEL);
        assert!(outcome.is_delivered(), "send to receiving target must deliver");
        // dst.p_delivermsg should be set + MF_DELIVERMSG.
        assert!(procs[1].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        // dst should be woken (RTS_RECEIVING cleared).
        assert!(!procs[1].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_send_when_target_not_receiving() {
        // Path B: dst not in RECEIVE → caller blocks, enqueued.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::FROM_KERNEL);
        assert!(outcome.is_blocked(), "send to non-receiving target must block caller");
        // caller should have RTS_SENDING set.
        assert!(procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
        // caller should be enqueued on dst's caller_q (intrusive chain).
        assert_eq!(caller_q_len(&procs, 1), 1);
        assert_eq!(procs[1].caller_q_head, Some(test_nr(0)));
    }

    #[test]
    fn test_send_non_blocking_returns_not_ready() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::NON_BLOCKING);
        assert_eq!(outcome.err(), Some(IpcError::NotReady));
        // caller should NOT be blocked.
        assert!(!procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
    }

    #[test]
    fn test_send_detects_deadlock() {
        // A sends to B, B is SENDING to A → SEND↔SEND deadlock.
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::SENDING, Endpoint(1),
        );
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::FROM_KERNEL);
        assert_eq!(outcome.err(), Some(IpcError::Deadlock));
    }

    // ── Receive tests (P0-12-1 + P0-12-5) ──

    #[test]
    fn test_receive_picks_notify_first() {
        // Phase 1: pending notify is delivered before async/caller_q.
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        // Assign priv slots so notify bitmap can be set.
        priv_table.assign_static(ProcNr(-4)).unwrap();
        priv_table.assign_static(ProcNr(-3)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(0).unwrap(); // nr=-4 (assuming NR_TASKS=4)
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(0);
            let b = procs.get_mut(1).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(1);
        }
        let _b_ep = pt.procs_slice()[1].p_endpoint;
        let b_nr = pt.procs_slice()[1].p_nr;
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        // Set notify pending bit 0 (caller A's priv_id) on B.
        {
            let b_priv = engine.priv_table.get_mut(1).unwrap();
            b_priv.signals.s_notify_pending |= 1u64 << 0;
        }
        let outcome = engine.receive(b_nr, Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick pending notify first");
    }

    #[test]
    fn test_receive_skips_notify_when_reply_pend() {
        // MF_REPLY_PEND set → notify check skipped, falls through to caller_q.
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        priv_table.assign_static(ProcNr(-4)).unwrap();
        priv_table.assign_static(ProcNr(-3)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(0).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(0);
            let b = procs.get_mut(1).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(1);
            // Set MF_REPLY_PEND — should skip notify.
            b.p_misc_flags.set(MiscFlagsBits::REPLY_PEND);
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        // Set notify pending bit on B.
        {
            let b_priv = engine.priv_table.get_mut(1).unwrap();
            b_priv.signals.s_notify_pending |= 1u64 << 0;
        }
        let b_nr = engine.procs[1].p_nr;
        let outcome = engine.receive(b_nr, Endpoint::ANY);
        // No caller_q entries → should block (notify was skipped).
        assert!(outcome.is_blocked(), "MF_REPLY_PEND must skip notify");
    }

    #[test]
    fn test_receive_picks_caller_q_last() {
        // Phase 3: when no notify/async, pick from caller_q.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        a.p_sendto_e = Endpoint(2);
        a.p_sendmsg = Message::default();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        // Enqueue a on b's queue (intrusive link in a's slot).
        caller_q_push(&mut procs, 1, 0);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick caller_q sender");
        // Sender should be woken (RTS_SENDING cleared).
        assert!(!procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
        // caller_q should be empty after removal.
        assert!(caller_q_is_empty(&procs, 1));
    }

    /// C: proc.c:1082-1083 — when the receiver takes a message from a
    /// sender with `MF_SIG_DELAY` set, the engine records the sender so the
    /// `ProcessTable`-level dispatcher can run `sig_delay_done` (which
    /// needs the scheduler-aware `rts_set`). The flag itself is left for
    /// `sig_delay_done` to clear.
    #[test]
    fn test_receive_sig_delay_sender_records_pending_delay() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        a.p_sendto_e = Endpoint(2);
        a.p_sendmsg = Message::default();
        // PM requested a delayed stop (RC_DELAY) while `a` was sending.
        a.p_misc_flags.set(MiscFlagsBits::SIG_DELAY);
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        caller_q_push(&mut procs, 1, 0);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);

        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick caller_q sender");

        // Sender's delay-end is now due: recorded for the ProcessTable-level
        // dispatcher (dispatch_ipc), exactly once.
        assert_eq!(
            engine.take_sig_delay_sender(),
            Some(test_nr(0)),
            "MF_SIG_DELAY sender must be reported"
        );
        assert_eq!(engine.take_sig_delay_sender(), None, "record is one-shot");
        // The flag stays set — sig_delay_done (ProcessTable level) clears it.
        assert!(procs[0].p_misc_flags.is_set(MiscFlagsBits::SIG_DELAY));
    }

    /// C: proc.c:1082-1083 — a sender *without* `MF_SIG_DELAY` must not be
    /// reported (no delayed stop to end).
    #[test]
    fn test_receive_plain_sender_has_no_pending_delay() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::with(RtsFlagsBits::SENDING);
        a.p_sendto_e = Endpoint(2);
        a.p_sendmsg = Message::default();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a, b]);
        caller_q_push(&mut procs, 1, 0);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);

        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick caller_q sender");
        assert_eq!(
            engine.take_sig_delay_sender(),
            None,
            "plain sender has no stop-delay to end"
        );
    }

    #[test]
    fn test_receive_blocks_when_no_match() {
        let mut a = make_test_proc(0, Endpoint(1));
        a.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let outcome = engine.receive(test_nr(0), Endpoint::ANY);
        assert!(outcome.is_blocked(), "receive with no match must block");
        assert!(procs[0].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_receive_picks_async_second() {
        // Phase 2: pending async message delivered after notify check.
        //
        // Realistic state (C: proc.c:1039-1050 → try_one, 1390-1497): the
        // sender holds a live SENDA table (s_asyntab/s_asynsize on its
        // priv) and the receiver carries the sender's bit in
        // s_asyn_pending. receive → deliver_async re-reads the table from
        // "user space" (SuccessCopy: one VALID entry aimed at Endpoint(2))
        // and delivers.
        //
        // Targets must be user-region slots (p_nr > 0): SENDA to task
        // slots is ECALLDENIED in C (iskerneln, proc.c:1266).
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        let b_priv = priv_table.assign_static(ProcNr(2)).unwrap();
        let a_endpoint = pt.get(ProcNr(1)).unwrap().p_endpoint; // Endpoint(1)
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
            let b = procs.get_mut(nr_to_idx(ProcNr(2)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(b_priv);
        }
        // Sender's pending SENDA table (C: proc.c:1405-1406 re-reads these).
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.signals.s_asyntab = 0x1000;
            p.signals.s_asynsize = 1;
            p.signals.s_asynendpoint = a_endpoint;
        }
        // Receiver's async-pending bitmap carries the sender's priv bit.
        priv_table.get_mut(b_priv).unwrap().signals.s_asyn_pending |= 1u64 << a_priv;

        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &SuccessCopy);
        let outcome = engine.receive(ProcNr(2), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick pending async");
        let b_idx = nr_to_idx(ProcNr(2)).unwrap();
        assert!(engine.procs[b_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    // ── Notify tests (P0-12-1) ──

    #[test]
    fn test_notify_delivers_when_target_receiving() {
        let mut pt = crate::test_helpers::test_proc_table();
        {
            let a = pt.get_mut(ProcNr(-4)).unwrap();
            a.p_rts_flags = RtsFlags::new();
        }
        {
            let b = pt.get_mut(ProcNr(-3)).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
        }
        let b_endpoint = pt.get(ProcNr(-3)).unwrap().p_endpoint;
        let procs = pt.procs_slice_mut();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4),b_endpoint);
        assert!(result.is_delivered(), "notify should deliver");
        let b_idx = nr_to_idx(ProcNr(-3)).unwrap();
        assert!(engine.procs[b_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!engine.procs[b_idx].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_notify_records_bitmap_when_not_receiving() {
        let mut pt = crate::test_helpers::test_proc_table();
        {
            let a = pt.get_mut(ProcNr(-4)).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(0);
        }
        {
            let b = pt.get_mut(ProcNr(-3)).unwrap();
            b.p_rts_flags = RtsFlags::new();
            b.priv_id = Some(1);
        }
        let b_endpoint = pt.get(ProcNr(-3)).unwrap().p_endpoint;
        let procs = pt.procs_slice_mut();
        let mut priv_table = crate::test_helpers::test_priv_table();
        priv_table.assign_static(ProcNr(-4)).unwrap();
        priv_table.assign_static(ProcNr(-3)).unwrap();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4),b_endpoint);
        assert!(result.is_delivered());
        let dst_priv = engine.priv_table.get(1).unwrap();
        assert_ne!(
            dst_priv.signals.s_notify_pending & (1u64 << 0), 0,
            "s_notify_pending bit 0 (caller's priv_id) should be set"
        );
    }

    #[test]
    fn test_notify_never_blocks() {
        // Notify to a non-existent endpoint returns Error, not Blocked.
        let mut pt = crate::test_helpers::test_proc_table();
        let procs = pt.procs_slice_mut();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4),Endpoint(99999));
        assert!(!result.is_blocked(), "notify must never block");
    }

    // ── Deliver message tests (P0-12-1) ──

    /// A UserCopy impl that always succeeds. SENDA reads return a
    /// single VALID entry aimed at `Endpoint(2)`.
    struct SuccessCopy;
    impl UserCopy for SuccessCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Ok(Message::default()) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Ok(()) }
        fn read_senda_entry(&self, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> {
            Ok((Endpoint(2), Message::default(), AMF_VALID))
        }
        fn write_senda_result(&self, _table: VirBytes, _index: usize, _result: i32, _flags: i32) -> Result<(), CopyError> { Ok(()) }
    }

    /// D-17 fixture: the single table entry is addressed to SELF
    /// (resolves to the caller's own endpoint — proc.c:1183-1185).
    struct SelfEntryCopy;
    impl UserCopy for SelfEntryCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Ok(Message::default()) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Ok(()) }
        fn read_senda_entry(&self, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> {
            Ok((Endpoint::SELF, Message::default(), AMF_VALID))
        }
        fn write_senda_result(&self, _table: VirBytes, _index: usize, _result: i32, _flags: i32) -> Result<(), CopyError> { Ok(()) }
    }

    /// D-17 fixture: entry aimed at a fixed endpoint; captures the
    /// per-entry result written back by `write_senda_result`.
    struct CapturingCopy {
        target: Endpoint,
        result: core::cell::Cell<Option<i32>>,
    }
    impl UserCopy for CapturingCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Ok(Message::default()) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Ok(()) }
        fn read_senda_entry(&self, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> {
            Ok((self.target, Message::default(), AMF_VALID))
        }
        fn write_senda_result(&self, _table: VirBytes, _index: usize, result: i32, _flags: i32) -> Result<(), CopyError> {
            self.result.set(Some(result));
            Ok(())
        }
    }

    /// A UserCopy impl that always page-faults.
    struct PageFaultCopy;
    impl UserCopy for PageFaultCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Err(CopyError::PageFault) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Err(CopyError::PageFault) }
        fn read_senda_entry(&self, _table: VirBytes, _index: usize) -> Result<(Endpoint, Message, i32), CopyError> { Err(CopyError::PageFault) }
        fn write_senda_result(&self, _table: VirBytes, _index: usize, _result: i32, _flags: i32) -> Result<(), CopyError> { Err(CopyError::PageFault) }
    }

    #[test]
    fn test_deliver_message_success() {
        let a = make_test_proc(0, Endpoint(1));
        a.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &SuccessCopy);
        let result = engine.deliver_message(test_nr(0));
        assert_eq!(result, DeliverResult::Delivered);
        // MF_DELIVERMSG cleared on success.
        assert!(!engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    #[test]
    fn test_deliver_message_first_page_fault() {
        let a = make_test_proc(0, Endpoint(1));
        a.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &PageFaultCopy);
        let result = engine.deliver_message(test_nr(0));
        assert_eq!(result, DeliverResult::PageFault);
        // MF_MSGFAILED set on first failure.
        assert!(engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::MSGFAILED));
        // MF_DELIVERMSG still set (will retry).
        assert!(engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    #[test]
    fn test_deliver_message_second_consecutive_fault() {
        let a = make_test_proc(0, Endpoint(1));
        a.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        a.p_misc_flags.set(MiscFlagsBits::MSGFAILED); // already failed once
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &PageFaultCopy);
        let result = engine.deliver_message(test_nr(0));
        assert_eq!(result, DeliverResult::Segfault, "second consecutive fault → Segfault");
        // Both flags cleared.
        assert!(!engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!engine.procs[0].p_misc_flags.is_set(MiscFlagsBits::MSGFAILED));
    }

    // ── Permission check tests (P0-12-1) ──

    #[test]
    fn test_check_ipc_permission_target_kernel_task_restriction() {
        // Layer 4 (C: proc.c:560-566): calls TO a kernel task target
        // (p_nr < 0) may only be SENDREC or RECEIVE — kernel tasks always
        // reply and may not block if the caller doesn't receive. C checks
        // the TARGET (`iskerneln(src_dst_p)`), not the caller.
        // To reach layer 4, layers 1-3 must pass: endpoint valid,
        // whitelist allows target, trap_mask allows SEND call.
        let task_nr = test_nr(0);              // p_nr = -NR_TASKS (kernel task)
        let user_nr = test_nr(NR_TASKS);       // p_nr = 0 (regular process)
        let mut procs: Vec<KProcess> = (0..=NR_TASKS)
            .map(|i| make_test_proc(i, Endpoint(1000 + i as i32)))
            .collect();
        procs[0] = make_test_proc(0, Endpoint(2));  // kernel task target
        procs[NR_TASKS].p_endpoint = Endpoint(1);   // user's own endpoint
        let mut priv_table = crate::test_helpers::test_priv_table();
        let task_priv = priv_table.assign_static(task_nr).unwrap();
        let user_priv = priv_table.assign_static(user_nr).unwrap();
        procs[0].priv_id = Some(task_priv);
        procs[NR_TASKS].priv_id = Some(user_priv);
        // Configure caller's priv: allow IPC to task (and self), all traps.
        {
            let caller_priv = priv_table.get_mut(user_priv).unwrap();
            caller_priv.ipc.s_ipc_to = caller_priv.ipc.s_ipc_to.union(
                crate::capability::IpcMask::from_bits(
                    (1u64 << task_priv as u32) | (1u64 << user_priv as u32),
                ),
            );
            caller_priv.ipc.s_trap_mask = crate::capability::TrapMask::ALL; // allow all calls including SEND
        }
        let engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        // SEND to a kernel task target is denied at layer 4.
        let r = engine.check_ipc_permission(user_nr, Endpoint(2), IpcCall::Send);
        assert_eq!(r, Err(IpcError::TrapDenied));
        // SENDREC and RECEIVE to a kernel task target pass layer 4.
        // Layer 3 trap_mask also needs SENDREC bit (bit 3); 0xFFFF covers it.
        let r2 = engine.check_ipc_permission(user_nr, Endpoint(2), IpcCall::SendRec);
        assert_eq!(r2, Ok(()), "SENDREC to kernel task must be allowed");
        let r3 = engine.check_ipc_permission(user_nr, Endpoint(2), IpcCall::Receive);
        assert_eq!(r3, Ok(()), "RECEIVE from kernel task must be allowed");
        // SEND to a non-kernel-task target passes layer 4 (restriction
        // applies to kernel task targets only).
        let r4 = engine.check_ipc_permission(user_nr, Endpoint(1), IpcCall::Send);
        assert_eq!(r4, Ok(()), "SEND to regular target must pass layer 4");
    }

    // ── SENDA tests (no kernel-side table cache — A_RETR/A_INSRT per entry) ──

    #[test]
    fn test_senda_requires_sys_proc() {
        // C: mini_senda — proc.c:1336-1339. Caller without SYS_PROC
        // privilege gets EPERM.
        let mut a = make_test_proc(0, Endpoint(1));
        a.p_rts_flags = RtsFlags::new();
        let mut procs = crate::test_helpers::scratch_procs([a]);
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(test_nr(0)).unwrap();
        procs[0].priv_id = Some(a_priv);
        // Leave SYS_PROC unset → EPERM (IpcError::Permission).
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let outcome = engine.senda(test_nr(0), VirBytes::new(0x1000), 1);
        assert_eq!(outcome.err(), Some(IpcError::Permission));
    }

    #[test]
    fn test_senda_delivers_to_receiving_target() {
        // SENDA with target in RECEIVE → entry delivered via
        // read_senda_entry/write_senda_result (no kernel table copy).
        //
        // Realistic state (C: mini_senda, proc.c:1231-1323): user-region
        // slots (p_nr > 0 — task slots get ECALLDENIED, iskerneln
        // proc.c:1266), caller privileged (SYS_PROC, proc.c:1336), and
        // the caller's s_ipc_to carrying the target's bit (may_send_to,
        // priv.h:87 — a fresh priv has an empty mask).
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        let b_priv = priv_table.assign_static(ProcNr(2)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
            let b = procs.get_mut(nr_to_idx(ProcNr(2)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(b_priv);
        }
        // Caller: SYS_PROC + IPC send permission for the target's sys_id.
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.flags.s_flags.insert(crate::capability::ProcessCapability::SYS_PROC);
            p.ipc.s_ipc_to = p.ipc.s_ipc_to.union(
                crate::capability::IpcMask::from_bits(1u64 << b_priv),
            );
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &SuccessCopy);
        // SuccessCopy reads one AMF_VALID entry aimed at Endpoint(2).
        let outcome = engine.senda(ProcNr(1), VirBytes::new(0x1000), 1);
        assert!(outcome.is_delivered(), "SENDA never blocks caller");
        // Fully delivered → caller's table pointer stays cleared.
        {
            let p = engine.priv_table.get(a_priv).unwrap();
            assert_eq!(p.signals.s_asynsize, 0);
        }
        // Target got the message and was woken.
        let b_idx = nr_to_idx(ProcNr(2)).unwrap();
        assert!(engine.procs[b_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!engine.procs[b_idx].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
        assert_eq!(engine.procs[b_idx].p_delivermsg.m_source, Endpoint(1));
    }

    /// D-17 (priv.h:87): `may_asynsend_to = may_send_to || self` — a
    /// SENDA entry addressed to SELF (resolving to the caller's own
    /// endpoint) passes the IPC mask gate even though the caller's own
    /// `s_ipc_to` bit is clear (the boot convention deliberately keeps
    /// the self bit clear). The target — the caller itself — is not
    /// RECEIVING while executing senda, so the entry takes the pending
    /// path: `s_asyn_pending` bit set for the caller's sys_id and the
    /// table pointer kept for the next receive (C: proc.c:1293-1298,
    /// 1320-1323).
    #[test]
    fn test_senda_self_target_allowed_without_mask_bit() {
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
        }
        // Caller: SYS_PROC but an EMPTY s_ipc_to — the self exception
        // must not depend on any mask bit.
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.flags.s_flags.insert(crate::capability::ProcessCapability::SYS_PROC);
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &SelfEntryCopy);
        let outcome = engine.senda(ProcNr(1), VirBytes::new(0x1000), 1);
        // Permission gate passed (CallDenied would surface as a table
        // result, not the aggregate outcome) → aggregate OK.
        assert!(outcome.is_delivered());
        {
            let p = engine.priv_table.get(a_priv).unwrap();
            // Pending path: s_asyn_pending bit for the caller's own
            // sys_id + table pointer kept for retry.
            assert_eq!(p.signals.s_asyn_pending, 1u64 << a_priv);
            assert_eq!(p.signals.s_asynsize, 1);
        }
    }

    /// D-17 asymmetry counterpart: the same EMPTY `s_ipc_to` DENIES an
    /// entry aimed at a different process (the sync-path rule
    /// `may_send_to` has no self exception — priv.h:86 vs :87). The
    /// denial is per-entry: the aggregate outcome stays OK and the
    /// result is written back to the table entry (C: proc.c:1300-1307).
    #[test]
    fn test_senda_other_without_mask_bit_denied() {
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        let b_priv = priv_table.assign_static(ProcNr(2)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
            let b = procs.get_mut(nr_to_idx(ProcNr(2)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
            b.p_getfrom_e = Endpoint::ANY;
            b.priv_id = Some(b_priv);
        }
        // Caller: SYS_PROC, EMPTY s_ipc_to — no bit for the target.
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.flags.s_flags.insert(crate::capability::ProcessCapability::SYS_PROC);
        }
        let copy = CapturingCopy { target: Endpoint(2), result: core::cell::Cell::new(None) };
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &copy);
        let outcome = engine.senda(ProcNr(1), VirBytes::new(0x1000), 1);
        // Aggregate outcome is OK (SENA never fails as a whole); the
        // denial lives in the per-entry result.
        assert!(outcome.is_delivered());
        assert_eq!(copy.result.get(), Some(ECALLDENIED));
    }

    #[test]
    fn test_senda_pending_sets_bitmap_and_retries_via_receive() {
        // Target not receiving → s_asyn_pending bit set on target,
        // table pointer kept in caller's priv; the next receive from
        // the target re-reads the table and delivers (C: try_one).
        //
        // Realistic state: user-region slots (task slots get
        // ECALLDENIED — iskerneln, proc.c:1266), both privs assigned,
        // caller SYS_PROC with the target's bit in s_ipc_to.
        let mut pt = crate::test_helpers::test_proc_table();
        let mut priv_table = crate::test_helpers::test_priv_table();
        let a_priv = priv_table.assign_static(ProcNr(1)).unwrap();
        let b_priv = priv_table.assign_static(ProcNr(2)).unwrap();
        {
            let procs = pt.procs_slice_mut();
            let a = procs.get_mut(nr_to_idx(ProcNr(1)).unwrap()).unwrap();
            a.p_rts_flags = RtsFlags::new();
            a.priv_id = Some(a_priv);
            let b = procs.get_mut(nr_to_idx(ProcNr(2)).unwrap()).unwrap();
            b.p_rts_flags = RtsFlags::new();
            b.priv_id = Some(b_priv);
        }
        // Caller: SYS_PROC + IPC send permission for the target's sys_id.
        {
            let p = priv_table.get_mut(a_priv).unwrap();
            p.flags.s_flags.insert(crate::capability::ProcessCapability::SYS_PROC);
            p.ipc.s_ipc_to = p.ipc.s_ipc_to.union(
                crate::capability::IpcMask::from_bits(1u64 << b_priv),
            );
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &SuccessCopy);
        let outcome = engine.senda(ProcNr(1), VirBytes::new(0x1000), 1);
        assert!(outcome.is_delivered());
        // Target's async-pending bitmap has the sender's bit.
        {
            let p = engine.priv_table.get(b_priv).unwrap();
            assert_ne!(p.signals.s_asyn_pending & (1u64 << a_priv), 0,
                "s_asyn_pending must carry the sender's priv bit");
        }
        // Caller's table pointer retained for retry (C: proc.c:1320-1323).
        {
            let p = engine.priv_table.get(a_priv).unwrap();
            assert_eq!(p.signals.s_asyntab, 0x1000);
            assert_eq!(p.signals.s_asynsize, 1);
        }
        // Target now receives → pending async delivered via re-read.
        let b_idx = nr_to_idx(ProcNr(2)).unwrap();
        engine.procs[b_idx].p_rts_flags.set(RtsFlagsBits::RECEIVING);
        engine.procs[b_idx].p_getfrom_e = Endpoint::ANY;
        let r = engine.receive(ProcNr(2), Endpoint::ANY);
        assert!(r.is_delivered(), "receive must deliver pending async");
        assert!(engine.procs[b_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
    }

    // ── Deliver result + KernelUserCopy stub ──

    #[test]
    fn test_deliver_result_variants() {
        let _ = DeliverResult::Delivered;
        let _ = DeliverResult::PageFault;
        let _ = DeliverResult::Segfault;
    }

    #[test]
    fn test_kernel_user_copy_stub() {
        let copier = KernelUserCopy;
        let _msg = copier.copy_msg_from_user(VirBytes::new(0)).unwrap();
        copier.copy_msg_to_user(VirBytes::new(0), &Message::default()).unwrap();
        // SENDA stub: reads fault (no real user table in tests), writes
        // succeed (C ignores A_INSRT errors).
        assert!(copier.read_senda_entry(VirBytes::new(0), 0).is_err());
        copier.write_senda_result(VirBytes::new(0), 0, 0, 0).unwrap();
    }
}
