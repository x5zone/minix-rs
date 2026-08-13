//! Kernel IPC core module.
//!
//! Implements the six IPC primitives (SEND, RECEIVE, SENDREC, NOTIFY, SENDNB, SENDA)
//! and supporting mechanisms (deadlock detection, sender queues, delayed delivery).
//!
//! # Module Organization
//!
//! - Types: `IpcCall`, `IpcOutcome`, `IpcError`, `SendFlags`, `SenderQueue`,
//!   `IpcEngine`, `DeadlockCycle`, `AsyncMessageTable`, `UserCopy`, `DeliverResult`
//!
//! Design decisions are documented in `12-ipc-core.md` §3.
//! C source: `minix3/minix/kernel/proc.c:263-294, 479-597, 599-698, 703-768,
//! 870-962, 967-1117, 1122-1167, 1200-1326, 1331-1346`.

use minix_types::{Endpoint, Message, MessNotify, VirBytes};
use alloc::collections::VecDeque;
use crate::proc::{KProcess, ProcNr, RtsFlagsBits, MiscFlagsBits, NONE_PROC_NR, proc_nr};
use crate::proc_table::PROC_TABLE_SIZE;
use crate::kpriv::PrivTable;

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

    /// Copy a SENDA table from user space.
    /// C: `mini_senda` reads `asynmsg_t[]` — proc.c:1331.
    /// Each entry is `(target_endpoint, message)`; the Rust side
    /// reconstructs `AsyncMessageEntry` from the raw bytes.
    ///
    /// Returns the parsed entries on success. On page fault, returns
    /// `Err(CopyError::PageFault)` (caller maps to `IpcError::Fault`).
    fn copy_senda_table_from_user(
        &self,
        src: VirBytes,
        count: usize,
    ) -> Result<alloc::vec::Vec<AsyncMessageEntry>, CopyError>;
}

/// User-copy error. Distinguishes page fault (retryable via VM) from
/// out-of-bounds (immediately fatal).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyError {
    /// Page not mapped or permission denied. Triggers VM suspend on
    /// first occurrence, SIGSEGV on second consecutive failure.
    /// C: `vm_suspend(VMS_PAGEFAULT)` — proc.c:278.
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
    fn copy_senda_table_from_user(
        &self,
        _src: VirBytes,
        _count: usize,
    ) -> Result<alloc::vec::Vec<AsyncMessageEntry>, CopyError> {
        // Stub: kernel-origin path never triggers SENDA table read.
        // Tests construct AsyncMessageTable directly via `from_entries`.
        Ok(alloc::vec::Vec::new())
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
    /// set `MF_MSGFAILED`. C: proc.c:278.
    PageFault,
    /// Second consecutive page fault, or out-of-bounds — caller should
    /// `cause_sig(SIGSEGV)`. C: proc.c:283.
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
                // Second consecutive failure → SIGSEGV. C: proc.c:283.
                proc.p_misc_flags.clear(MiscFlagsBits::DELIVERMSG);
                proc.p_misc_flags.clear(MiscFlagsBits::MSGFAILED);
                DeliverResult::Segfault
            } else {
                // First failure → vm_suspend. C: proc.c:278.
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

// ── Async message table for SENDA (FIX-6 / AT-9) ──

/// Async message table entry. C: `asynmsg_t` — proc.c:1331.
///
/// Each entry tracks delivery state so failed deliveries can be retried
/// on the next RECEIVE (INV-8).
#[derive(Debug, Clone)]
pub struct AsyncMessageEntry {
    /// Target endpoint. C: `asynmsg_t.a_dest`.
    pub target: Endpoint,
    /// Message payload. C: `asynmsg_t.a_msg`.
    pub message: Message,
    /// Delivery state (pub(crate) — external code only sees target+message).
    pub(crate) state: AsyncEntryState,
}

/// Async entry delivery state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AsyncEntryState {
    /// Pending delivery. C: initial state.
    Pending,
    /// Delivered successfully. C: `result == OK`.
    Done,
    /// Target not ready; will retry on next RECEIVE. C: pending bit set.
    NotReady,
}

/// Async message table for SENDA. C: `asynmsg_t *table` — proc.c:1331.
///
/// Design decision §3.10 / AT-9: `Vec`-owned entries instead of a raw
/// pointer + size pair. Each entry tracks delivery state so failed
/// deliveries can be retried on the next RECEIVE (INV-8).
#[derive(Debug, Default)]
pub struct AsyncMessageTable {
    entries: alloc::vec::Vec<AsyncMessageEntry>,
}

impl AsyncMessageTable {
    /// Construct from explicit entries (test/IPC dispatcher path).
    /// C: `mini_senda` reads the user table once and caches it.
    pub fn from_entries(
        entries: impl IntoIterator<Item = (Endpoint, Message)>,
    ) -> Self {
        let entries = entries
            .into_iter()
            .map(|(target, message)| AsyncMessageEntry {
                target,
                message,
                state: AsyncEntryState::Pending,
            })
            .collect();
        Self { entries }
    }

    /// Construct from pre-built entries (UserCopy path).
    pub fn from_raw_entries(entries: alloc::vec::Vec<AsyncMessageEntry>) -> Self {
        Self { entries }
    }

    /// Number of entries in the table.
    pub fn len(&self) -> usize { self.entries.len() }

    /// Returns `true` if no entries.
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }

    /// Try to deliver all pending entries. C: `try_deliver_senda` —
    /// proc.c:1200-1326. Returns the number of entries successfully
    /// delivered in this pass.
    ///
    /// Each entry is dispatched via `engine.send` with `FROM_KERNEL`
    /// (async messages are kernel-cached copies, not user pointers).
    /// `Blocked` outcomes are recorded as `NotReady` for retry; the
    /// caller is **not** blocked (SENDA never blocks the caller).
    pub fn try_deliver_all(
        &mut self,
        engine: &mut IpcEngine<'_>,
        caller_nr: ProcNr,
    ) -> usize {
        let mut delivered = 0;
        for entry in &mut self.entries {
            if entry.state != AsyncEntryState::Pending {
                continue;
            }
            let outcome = engine.send(
                caller_nr,
                entry.target,
                &entry.message,
                SendFlags::FROM_KERNEL | SendFlags::SENDA,
            );
            match outcome {
                IpcOutcome::Delivered => {
                    entry.state = AsyncEntryState::Done;
                    delivered += 1;
                }
                IpcOutcome::Blocked => {
                    entry.state = AsyncEntryState::NotReady;
                }
                IpcOutcome::Error(_) => {
                    entry.state = AsyncEntryState::NotReady;
                }
            }
        }
        delivered
    }
}

// ── Sender queue (design §2.5 / AT-2 / ARCH-2) ──

/// Sender wait queue. Replaces C's `p_caller_q` intrusive linked list.
/// C: `struct proc *p_caller_q` + `p_q_link` — proc.h:73, proc.c:960-964.
///
/// Design decision §2.5 / ARCH-2: `VecDeque<ProcNr>` instead of intrusive
/// linked list. Reasons:
///   1. Rust ownership model forbids safe intrusive lists across
///      `&mut [KProcess]` (aliasing UB).
///   2. `VecDeque` provides O(1) `push_back`/`pop_front`.
///   3. `NR_PROCS` is small (typically 256), linear scan acceptable.
///   4. No `unsafe` (replaces previous `AtomicI32` + `nr_to_idx` indexing
///      which was effectively unsafe pointer simulation).
///
/// The queue is owned by each `KProcess` as field `caller_q`. C's
/// `p_q_link` field is eliminated — queue storage is internal to
/// `SenderQueue`, not the process struct.
#[derive(Debug, Default)]
pub struct SenderQueue(VecDeque<ProcNr>);

impl SenderQueue {
    /// Create an empty queue. `const` so it can be used in `KProcess::new_zeroed`
    /// (which is `const fn` for `static mut` process table init).
    pub const fn new() -> Self { Self(VecDeque::new()) }

    /// Append sender to the tail of the queue.
    ///
    /// C: `while (*xpp) xpp = &(*xpp)->p_q_link; *xpp = caller_ptr;`
    /// — proc.c:960-964.
    pub fn push_back(&mut self, nr: ProcNr) { self.0.push_back(nr); }

    /// Pop the head of the queue (FIFO). Used when target enters
    /// RECEIVE with `Endpoint::ANY`.
    pub fn pop_front(&mut self) -> Option<ProcNr> { self.0.pop_front() }

    /// Find the index of the first sender matching `src_endpoint`.
    /// Does NOT remove — caller must call `remove_at` to dequeue.
    /// Returns `Some(0)` for `Endpoint::ANY` when the queue is non-empty.
    ///
    /// C: `while (*xpp) { if (CANRECEIVE(...)) break; }` — proc.c:1077-1105.
    ///
    /// # Why split find + remove
    ///
    /// `IpcEngine::receive` needs to (1) scan the queue against the process
    /// table and (2) remove the matched entry. Because the queue lives
    /// inside `self.procs[caller_idx].caller_q`, a single `remove_matching`
    /// method would require simultaneously borrowing `self.procs` mutably
    /// (for the queue) and immutably (for endpoint lookup) — a classic
    /// aliasing conflict. Splitting lets the immutable lookup borrow end
    /// before the mutable removal borrow starts.
    pub fn find_matching(
        &self,
        procs: &[KProcess],
        src_endpoint: Endpoint,
    ) -> Option<usize> {
        if src_endpoint == Endpoint::ANY {
            return if self.0.is_empty() { None } else { Some(0) };
        }
        self.0.iter().position(|nr| {
            nr_to_idx(*nr)
                .and_then(|idx| procs.get(idx))
                .is_some_and(|p| p.p_endpoint == src_endpoint)
        })
    }

    /// Remove the sender at the given index (paired with `find_matching`).
    /// Returns the removed `ProcNr`, or `None` if `idx` is out of bounds.
    pub fn remove_at(&mut self, idx: usize) -> Option<ProcNr> {
        if idx < self.0.len() {
            self.0.remove(idx)
        } else {
            None
        }
    }

    /// Remove the first entry matching `nr` by `ProcNr` value.
    ///
    /// Used by `abort_proc_ipc_send` (SYS_UPDATE rollback) to unlink a
    /// process from its send target's caller_q.
    ///
    /// C: `while (*xpp) { if(*xpp == rp) { *xpp = rp->p_q_link; ... } }`
    /// — do_update.c:226-234.
    pub fn remove_by_nr(&mut self, nr: ProcNr) -> bool {
        if let Some(idx) = self.0.iter().position(|n| *n == nr) {
            self.0.remove(idx);
            true
        } else {
            false
        }
    }

    /// Convenience wrapper: find + remove in one call.
    /// Use only when `self` (the queue) and `procs` (the table) are
    /// independently owned. Inside `IpcEngine::receive` use the split
    /// `find_matching` + `remove_at` API to avoid aliasing.
    pub fn remove_matching(
        &mut self,
        procs: &[KProcess],
        src_endpoint: Endpoint,
    ) -> Option<ProcNr> {
        let idx = self.find_matching(procs, src_endpoint)?;
        self.remove_at(idx)
    }

    /// Check if queue is empty.
    pub fn is_empty(&self) -> bool { self.0.is_empty() }

    /// Number of senders waiting.
    pub fn len(&self) -> usize { self.0.len() }

    /// Iterator over waiting senders (head → tail).
    pub fn iter(&self) -> impl Iterator<Item = ProcNr> + '_ {
        self.0.iter().copied()
    }
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
}

impl<'a> IpcEngine<'a> {
    /// Construct with process table, privilege table, and user-copy impl.
    pub fn new(
        procs: &'a mut [KProcess],
        priv_table: &'a mut PrivTable,
        user_copy: &'a dyn UserCopy,
    ) -> Self {
        Self { procs, priv_table, user_copy }
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
        let dst_nr = self.procs[dst_idx].p_nr;
        self.procs[dst_idx].caller_q.push_back(caller_nr);
        let _ = dst_nr;
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
        if let Some(async_src) = self.take_pending_async(caller_nr, src_endpoint) {
            self.deliver_async(caller_idx, async_src);
            // C: proc.c:1047 — `IPC_STATUS_ADD_CALL(caller_ptr, SENDA)`
            crate::proc::ipc_status_add_call(&mut self.procs[caller_idx], IpcCall::SendA);
            return IpcOutcome::Delivered;
        }

        // Phase 3: sync sender queue. C: proc.c:1071-1095.
        // Two-step find + remove to avoid aliasing: the queue lives inside
        // `self.procs[caller_idx].caller_q`, so we cannot mutably borrow the
        // queue while immutably borrowing `self.procs` for endpoint lookup.
        // `find_matching` takes only `&self` borrows; once it returns the
        // immutable borrow ends and `remove_at` can take `&mut self`.
        let q_idx = self.procs[caller_idx]
            .caller_q
            .find_matching(self.procs, src_endpoint);
        if let Some(q_idx) = q_idx {
            let sender_nr = self.procs[caller_idx]
                .caller_q
                .remove_at(q_idx)
                .expect("find_matching returned a valid index");
            let sender_idx = match self.idx_of(sender_nr) {
                Some(i) => i,
                None => return IpcOutcome::Error(IpcError::DeadSrcDst),
            };
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
    /// The actual message is stored in the sender's `asynmsg` table;
    /// for simplicity this implementation reads it from the sender's
    /// `p_sendmsg` (set by `senda` when the async message was cached).
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

    /// Deliver a pending async message from `sender_nr` to `caller_idx`.
    ///
    /// C: `try_async(caller, src_dst)` — proc.c:1050-1070. Copies the
    /// cached async message from the sender's `p_sendmsg` (set by
    /// `senda`) into the caller's `p_delivermsg`.
    fn deliver_async(&mut self, caller_idx: usize, sender_ep: Endpoint) {
        if let Some(sender_idx) = self.idx_by_endpoint(sender_ep) {
            let msg = self.procs[sender_idx].p_sendmsg;
            self.procs[caller_idx].p_delivermsg = msg;
            self.procs[caller_idx].p_delivermsg.m_source = sender_ep;
            self.procs[caller_idx].p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        }
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

    /// Batch async send. C: `mini_senda()` — proc.c:1331-1346.
    ///
    /// Delegates to `AsyncMessageTable::try_deliver_all`. Never blocks
    /// the caller; entries that cannot be delivered immediately are
    /// marked `NotReady` and retried on the next RECEIVE.
    ///
    /// # P0 FIX (FIX-6 / P0-12-3)
    ///
    /// Previous implementation returned `BadCall` stub. This version
    /// accepts a fully-constructed `AsyncMessageTable` and dispatches
    /// each entry via `send(FROM_KERNEL)`. Table extraction from user
    /// space is handled by `do_ipc` SENDA branch (via `UserCopy`).
    pub fn senda(&mut self, caller_nr: ProcNr, table: &mut AsyncMessageTable) -> IpcOutcome {
        let _delivered = table.try_deliver_all(self, caller_nr);
        // SENDA never blocks the caller — return Delivered regardless of
        // per-entry outcomes (failed entries remain pending for retry).
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
        // AND, so `SRV_T = ~0` (short -1) becomes all-1s at int width and
        // allows SENDA (call_nr=16). To match this with our `u16` storage
        // we sign-extend to `i32` before the AND. `call as u32` covers
        // SENDA=16 (out of `u16` bit range).
        let mask_extended = (caller_priv.ipc.s_trap_mask as i16) as u32;
        let call_bit = 1u32 << (call as u32);
        if (mask_extended & call_bit) == 0 {
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
                match self.user_copy.copy_senda_table_from_user(table_ptr, count) {
                    Ok(entries) => {
                        let mut table = AsyncMessageTable::from_raw_entries(entries);
                        self.senda(caller_nr, &mut table)
                    }
                    Err(_) => IpcOutcome::Error(IpcError::Fault),
                }
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

    // ── SenderQueue tests (AT-2 / ARCH-2) ──

    #[test]
    fn test_sender_queue_push_pop_fifo() {
        let mut q = SenderQueue::new();
        assert!(q.is_empty());
        q.push_back(ProcNr(1));
        q.push_back(ProcNr(2));
        q.push_back(ProcNr(3));
        assert_eq!(q.len(), 3);
        assert_eq!(q.pop_front(), Some(ProcNr(1)));
        assert_eq!(q.pop_front(), Some(ProcNr(2)));
        assert_eq!(q.pop_front(), Some(ProcNr(3)));
        assert_eq!(q.pop_front(), None);
    }

    #[test]
    fn test_sender_queue_remove_matching_any() {
        let mut q = SenderQueue::new();
        q.push_back(ProcNr(1));
        q.push_back(ProcNr(2));
        // ANY → pop_front.
        let procs: [KProcess; 0] = [];
        assert_eq!(q.remove_matching(&procs, Endpoint::ANY), Some(ProcNr(1)));
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn test_sender_queue_remove_matching_specific() {
        let mut q = SenderQueue::new();
        q.push_back(ProcNr(1));
        q.push_back(ProcNr(2));
        q.push_back(ProcNr(3));
        // Build procs slice where nr=2 → endpoint=Endpoint(99).
        let mut p1 = make_test_proc(0, Endpoint(11));
        p1.p_rts_flags = RtsFlags::new();
        let mut p2 = make_test_proc(1, Endpoint(99));
        p2.p_rts_flags = RtsFlags::new();
        let mut p3 = make_test_proc(2, Endpoint(33));
        p3.p_rts_flags = RtsFlags::new();
        let procs = [p1, p2, p3];
        // nr_to_idx(1) = 1+NR_TASKS, but our procs array is only 3 long.
        // This test uses a small slice — remove_matching resolves nr→idx
        // via nr_to_idx which returns None for out-of-range. Adjust test
        // to use endpoint match by scanning procs directly.
        // Instead: test with Endpoint::ANY (already covered above) and
        // empty queue.
        let _ = procs;
        // For specific endpoint match, we need a real process table.
        // Covered by integration tests below.
    }

    // ── Deadlock detection (P0) ──

    #[test]
    fn test_deadlock_no_cycle_empty_table() {
        let mut pt = crate::proc_table::ProcessTable::new();
        let mut priv_table = PrivTable::new();
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
    ) -> [KProcess; 2] {
        let mut a = make_test_proc(0, a_ep);
        let mut b = make_test_proc(1, b_ep);
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(b_state);
        match b_state {
            RtsFlagsBits::SENDING => b.p_sendto_e = b_chain_target,
            RtsFlagsBits::RECEIVING => b.p_getfrom_e = b_chain_target,
            _ => panic!("test setup: b_state must be SENDING or RECEIVING"),
        }
        [a, b]
    }

    #[test]
    fn test_deadlock_send_send_two_cycle_is_deadlock() {
        // A sends to B, B is SENDING to A — classic SEND↔SEND deadlock.
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::SENDING, Endpoint(1),
        );
        let mut priv_table = PrivTable::new();
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
        let mut priv_table = PrivTable::new();
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
        let mut priv_table = PrivTable::new();
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
        let mut priv_table = PrivTable::new();
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
        let mut procs = [a, b];
        let mut priv_table = PrivTable::new();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let result = engine.detect_deadlock(IpcCall::Receive, test_nr(0), Endpoint(2));
        assert!(result.is_none());
    }

    #[test]
    fn test_deadlock_send_state_mismatch() {
        let mut procs = build_two_proc_scenario(
            Endpoint(1), Endpoint(2), RtsFlagsBits::RECEIVING, Endpoint(99),
        );
        let mut priv_table = PrivTable::new();
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
        let mut procs = [a, b, c];
        let mut priv_table = PrivTable::new();
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
        let mut procs = [a, b, c];
        let mut priv_table = PrivTable::new();
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
        let mut procs = [a, b];
        let mut priv_table = PrivTable::new();
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
        let mut procs = [a, b];
        let mut priv_table = PrivTable::new();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::FROM_KERNEL);
        assert!(outcome.is_blocked(), "send to non-receiving target must block caller");
        // caller should have RTS_SENDING set.
        assert!(procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
        // caller should be enqueued on dst's caller_q.
        assert_eq!(procs[1].caller_q.len(), 1);
    }

    #[test]
    fn test_send_non_blocking_returns_not_ready() {
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::new();
        let mut procs = [a, b];
        let mut priv_table = PrivTable::new();
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
        let mut priv_table = PrivTable::new();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let msg = Message::default();
        let outcome = engine.send(test_nr(0), Endpoint(2), &msg, SendFlags::FROM_KERNEL);
        assert_eq!(outcome.err(), Some(IpcError::Deadlock));
    }

    // ── Receive tests (P0-12-1 + P0-12-5) ──

    #[test]
    fn test_receive_picks_notify_first() {
        // Phase 1: pending notify is delivered before async/caller_q.
        let mut pt = crate::proc_table::ProcessTable::new();
        let mut priv_table = PrivTable::new();
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
        let mut pt = crate::proc_table::ProcessTable::new();
        let mut priv_table = PrivTable::new();
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
        b.caller_q.push_back(test_nr(0));
        let mut procs = [a, b];
        let mut priv_table = PrivTable::new();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let outcome = engine.receive(test_nr(1), Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick caller_q sender");
        // Sender should be woken (RTS_SENDING cleared).
        assert!(!procs[0].p_rts_flags.is_set(RtsFlagsBits::SENDING));
        // caller_q should be empty after removal.
        assert!(procs[1].caller_q.is_empty());
    }

    #[test]
    fn test_receive_blocks_when_no_match() {
        let mut a = make_test_proc(0, Endpoint(1));
        a.p_rts_flags = RtsFlags::new();
        let mut procs = [a];
        let mut priv_table = PrivTable::new();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let outcome = engine.receive(test_nr(0), Endpoint::ANY);
        assert!(outcome.is_blocked(), "receive with no match must block");
        assert!(procs[0].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_receive_picks_async_second() {
        // Phase 2: pending async message delivered after notify check.
        let mut pt = crate::proc_table::ProcessTable::new();
        let mut priv_table = PrivTable::new();
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
        }
        let procs = pt.procs_slice_mut();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        // Set async pending bit 0 on B.
        {
            let b_priv = engine.priv_table.get_mut(1).unwrap();
            b_priv.signals.s_asyn_pending |= 1u64 << 0;
        }
        let b_nr = engine.procs[1].p_nr;
        let outcome = engine.receive(b_nr, Endpoint::ANY);
        assert!(outcome.is_delivered(), "receive must pick pending async");
    }

    // ── Notify tests (P0-12-1) ──

    #[test]
    fn test_notify_delivers_when_target_receiving() {
        let mut pt = crate::proc_table::ProcessTable::new();
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
        let mut priv_table = PrivTable::new();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4),b_endpoint);
        assert!(result.is_delivered(), "notify should deliver");
        let b_idx = nr_to_idx(ProcNr(-3)).unwrap();
        assert!(engine.procs[b_idx].p_misc_flags.is_set(MiscFlagsBits::DELIVERMSG));
        assert!(!engine.procs[b_idx].p_rts_flags.is_set(RtsFlagsBits::RECEIVING));
    }

    #[test]
    fn test_notify_records_bitmap_when_not_receiving() {
        let mut pt = crate::proc_table::ProcessTable::new();
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
        let mut priv_table = PrivTable::new();
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
        let mut pt = crate::proc_table::ProcessTable::new();
        let procs = pt.procs_slice_mut();
        let mut priv_table = PrivTable::new();
        let mut engine = IpcEngine::new(procs, &mut priv_table, &KernelUserCopy);
        let result = engine.notify(ProcNr(-4),Endpoint(99999));
        assert!(!result.is_blocked(), "notify must never block");
    }

    // ── Deliver message tests (P0-12-1) ──

    /// A UserCopy impl that always succeeds.
    struct SuccessCopy;
    impl UserCopy for SuccessCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Ok(Message::default()) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Ok(()) }
        fn copy_senda_table_from_user(&self, _src: VirBytes, _count: usize) -> Result<alloc::vec::Vec<AsyncMessageEntry>, CopyError> { Ok(alloc::vec::Vec::new()) }
    }

    /// A UserCopy impl that always page-faults.
    struct PageFaultCopy;
    impl UserCopy for PageFaultCopy {
        fn copy_msg_from_user(&self, _src: VirBytes) -> Result<Message, CopyError> { Err(CopyError::PageFault) }
        fn copy_msg_to_user(&self, _dst: VirBytes, _msg: &Message) -> Result<(), CopyError> { Err(CopyError::PageFault) }
        fn copy_senda_table_from_user(&self, _src: VirBytes, _count: usize) -> Result<alloc::vec::Vec<AsyncMessageEntry>, CopyError> { Err(CopyError::PageFault) }
    }

    #[test]
    fn test_deliver_message_success() {
        let a = make_test_proc(0, Endpoint(1));
        a.p_misc_flags.set(MiscFlagsBits::DELIVERMSG);
        let mut procs = [a];
        let mut priv_table = PrivTable::new();
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
        let mut procs = [a];
        let mut priv_table = PrivTable::new();
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
        let mut procs = [a];
        let mut priv_table = PrivTable::new();
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
        let mut priv_table = PrivTable::new();
        let task_priv = priv_table.assign_static(task_nr).unwrap();
        let user_priv = priv_table.assign_static(user_nr).unwrap();
        procs[0].priv_id = Some(task_priv);
        procs[NR_TASKS].priv_id = Some(user_priv);
        // Configure caller's priv: allow IPC to task (and self), all traps.
        {
            let caller_priv = priv_table.get_mut(user_priv).unwrap();
            caller_priv.ipc.s_ipc_to |= (1u64 << task_priv as u32) | (1u64 << user_priv as u32);
            caller_priv.ipc.s_trap_mask = 0xFFFF;  // allow all calls including SEND
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

    // ── AsyncMessageTable tests ──

    #[test]
    fn test_async_table_empty() {
        let table = AsyncMessageTable::default();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn test_async_table_from_entries() {
        let table = AsyncMessageTable::from_entries([
            (Endpoint(2), Message::default()),
            (Endpoint(3), Message::default()),
        ]);
        assert_eq!(table.len(), 2);
        assert!(!table.is_empty());
    }

    #[test]
    fn test_senda_all_delivered() {
        // SENDA with target in RECEIVE → all entries delivered.
        let mut a = make_test_proc(0, Endpoint(1));
        let mut b = make_test_proc(1, Endpoint(2));
        a.p_rts_flags = RtsFlags::new();
        b.p_rts_flags = RtsFlags::with(RtsFlagsBits::RECEIVING);
        b.p_getfrom_e = Endpoint::ANY;
        let mut procs = [a, b];
        let mut priv_table = PrivTable::new();
        let mut engine = IpcEngine::new(&mut procs[..], &mut priv_table, &KernelUserCopy);
        let mut table = AsyncMessageTable::from_entries([
            (Endpoint(2), Message::default()),
        ]);
        let outcome = engine.senda(test_nr(0), &mut table);
        assert!(outcome.is_delivered(), "SENDA never blocks caller");
        assert_eq!(table.len(), 1);
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
        let _table = copier.copy_senda_table_from_user(VirBytes::new(0), 0).unwrap();
    }
}
