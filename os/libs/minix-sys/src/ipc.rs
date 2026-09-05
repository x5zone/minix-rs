//! Inter-process communication primitives: the six traps plus the queue logic.
//!
//! Every user-space program in Minix talks to the kernel and to the system
//! servers through exactly six low-level operations (C: the six routines in
//! `minix3/minix/lib/libc/arch/i386/sys/_ipc.S`, numbered in
//! `minix3/minix/include/minix/ipcconst.h`):
//!
//! 1. Blocking send: hand a message to one destination and wait until the
//!    destination accepts it.
//! 2. Blocking receive: wait until a message from one source arrives.
//! 3. Send-and-receive: send a request and wait for the reply in one step.
//! 4. Notify: deliver a tiny signal without any message body, never blocking.
//! 5. Non-blocking send: hand a message over only if the destination is ready
//!    right now, otherwise report failure immediately.
//! 6. Asynchronous batch send: hand a whole table of messages over at once
//!    and let the kernel work through it in the background.
//!
//! A seventh trap fetches the kernel information page pointer (C:
//! `ipc_minix_kerninfo` in `.../arch/i386/sys/ipc_minix_kerninfo.S`).
//!
//! This module models all seven operations without copying the assembly. The
//! machine-code trap sequence stays in assembly (it must place values in
//! specific registers before any Rust code may run); everything from the call
//! number selection onwards is ordinary Rust that unit tests can drive
//! through the [`IpcTransport`] trait.
//!
//! # Execution model
//!
//! This crate runs as an unprivileged user-space library. The startup path is
//! single-threaded, and the queue type below takes `&mut self`, so the
//! compiler itself rules out re-entrant use — the C version needed an
//! explicit `inside` flag for the same purpose (see
//! `minix3/minix/lib/libsys/asynsend.c:32-39`).

use alloc::vec::Vec;
use minix_types::{Endpoint, IpcError, Message};

/// Blocking send: deliver a message and wait for acceptance.
///
/// C: `SEND 1` (`minix3/minix/include/minix/ipcconst.h:7`).
pub const CALL_SEND: u32 = 1;
/// Blocking receive: wait for a message.
///
/// C: `RECEIVE 2` (`minix3/minix/include/minix/ipcconst.h:8`).
pub const CALL_RECEIVE: u32 = 2;
/// Send-and-receive: request plus reply in one step.
///
/// C: `SENDREC 3` (`minix3/minix/include/minix/ipcconst.h:9`).
pub const CALL_SENDREC: u32 = 3;
/// Asynchronous notify: signal without a message body.
///
/// C: `NOTIFY 4` (`minix3/minix/include/minix/ipcconst.h:10`).
pub const CALL_NOTIFY: u32 = 4;
/// Non-blocking send: deliver only if the destination is ready now.
///
/// C: `SENDNB 5` (`minix3/minix/include/minix/ipcconst.h:11`).
pub const CALL_SENDNB: u32 = 5;
/// Kernel information page request.
///
/// C: `MINIX_KERNINFO 6` (`minix3/minix/include/minix/ipcconst.h:12`).
pub const CALL_KERNINFO: u32 = 6;
/// Asynchronous batch send: hand a table of messages over at once.
///
/// C: `SENDA 16` (`minix3/minix/include/minix/ipcconst.h:13`).
pub const CALL_SENDA: u32 = 16;

/// Highest valid call number.
///
/// C: `IPCNO_HIGHEST SENDA` (`minix3/minix/include/minix/ipcconst.h:14`).
pub const HIGHEST_CALL_NUMBER: u32 = CALL_SENDA;

/// Processor trap vectors used on 32-bit Intel Minix.
///
/// C: `KERVEC_INTR 32` (system call trap) and `IPCVEC_INTR 33`
/// (communication trap) in
/// `minix3/minix/include/arch/i386/include/ipcconst.h:5-6`, plus the
/// user-mapped variants 34 and 35. The 64-bit evolution replaces the
/// interrupt instructions with the `syscall` machine instruction behind an
/// architecture trait (stage plan item A-6); this enumeration names the
/// choice so documentation and code cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapVector {
    /// System call trap (C: `KERVEC_INTR`).
    KernelCall,
    /// Communication trap (C: `IPCVEC_INTR`).
    InterProcess,
    /// User-mapped system call trap (C: `KERVEC_UM`).
    UserMappedKernelCall,
    /// User-mapped communication trap (C: `IPCVEC_UM`).
    UserMappedInterProcess,
}

impl TrapVector {
    /// Returns the numeric vector for the trap instruction.
    pub const fn number(self) -> u32 {
        match self {
            TrapVector::KernelCall => 32,
            TrapVector::InterProcess => 33,
            TrapVector::UserMappedKernelCall => 34,
            TrapVector::UserMappedInterProcess => 35,
        }
    }
}

/// Raw integer status reported back by a trap.
///
/// The C trap routines return a plain `int`: zero on success, a status code
/// otherwise. The receive routine additionally writes the decoded call number
/// through a caller-supplied pointer (see `_ipc.S:36-37`: the kernel leaves
/// the status in `ebx` and the routine stores it). This type keeps the raw
/// value visible instead of folding it into a boolean, so the system call
/// layer above can implement the exact C rule "transport failure becomes the
/// message type".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrapStatus(pub i32);

impl TrapStatus {
    /// The success value: every trap reports zero on success.
    pub const SUCCESS: TrapStatus = TrapStatus(0);

    /// Reports whether the trap succeeded.
    pub const fn is_success(self) -> bool {
        self.0 == 0
    }
}

/// Decoded communication status word.
///
/// C packs two things into one integer (`minix3/minix/include/minix/ipcconst.h:21-35`):
/// the call number in the lowest 6 bits (mask `0x3F`) and a flag word shifted
/// left by 16 bits. Bit 0 of the flag word marks a message that originated in
/// the kernel on behalf of a process; such a message is trusted and must
/// never be answered. This type exposes each half through a named method so
/// callers never shift or mask by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpcStatus(pub u32);

/// Mask selecting the call number bits of a status word.
///
/// C: `IPC_STATUS_CALL_MASK 0x3F` (`ipcconst.h:22`).
pub const STATUS_CALL_MASK: u32 = 0x3F;
/// Bit position where the flag word starts.
///
/// C: `IPC_STATUS_FLAGS_SHIFT 16` (`ipcconst.h:32`).
pub const STATUS_FLAGS_SHIFT: u32 = 16;
/// Flag bit marking a kernel-originated message.
///
/// C: `IPC_FLG_MSG_FROM_KERNEL 1` (`ipcconst.h:28-31`). The comment in the C
/// header is explicit: the message is trusted and the receiver must never
/// reply to the sender.
pub const STATUS_FLAG_FROM_KERNEL: u32 = 1;

impl IpcStatus {
    /// Extracts the call number (C: `IPC_STATUS_CALL`).
    pub const fn call(self) -> u32 {
        self.0 & STATUS_CALL_MASK
    }

    /// Builds a status word holding only a call number (C: `IPC_STATUS_CALL_TO`).
    pub const fn from_call(call: u32) -> Self {
        IpcStatus(call & STATUS_CALL_MASK)
    }

    /// Shifts a flag word into flag position (C: `IPC_STATUS_FLAGS`).
    pub const fn with_flags(flags: u32) -> Self {
        IpcStatus(flags << STATUS_FLAGS_SHIFT)
    }

    /// Tests flag bits (C: `IPC_STATUS_FLAGS_TEST`).
    pub const fn flags_test(self, flags: u32) -> bool {
        (self.0 >> STATUS_FLAGS_SHIFT) & flags != 0
    }

    /// Reports whether the message came from the kernel (trusted, no reply).
    pub const fn is_from_kernel(self) -> bool {
        self.flags_test(STATUS_FLAG_FROM_KERNEL)
    }
}

/// Slot state flags for the asynchronous batch table.
///
/// C: `AMF_EMPTY 000` through `AMF_NOTIFY_ERR 020` in
/// `minix3/minix/include/minix/ipc.h:2754-2762` (octal notation, so `010` is
/// eight and `020` is sixteen). A slot starts empty, becomes valid when the
/// sender fills it, gains the done bit when the kernel has processed it, and
/// may request a notification on completion or on failed delivery. The flags
/// must be written last, after the destination and message fields, because
/// the kernel scans the table concurrently (see `asynsend.c:127-131`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncSlotFlags(pub u32);

impl AsyncSlotFlags {
    /// Slot is not in use (C: `AMF_EMPTY`).
    pub const EMPTY: AsyncSlotFlags = AsyncSlotFlags(0);
    /// Slot contains a message (C: `AMF_VALID`).
    pub const VALID: AsyncSlotFlags = AsyncSlotFlags(1);
    /// Kernel has processed the message; the result field holds the outcome
    /// (C: `AMF_DONE`).
    pub const DONE: AsyncSlotFlags = AsyncSlotFlags(2);
    /// Send a notification when processing completes (C: `AMF_NOTIFY`).
    pub const NOTIFY: AsyncSlotFlags = AsyncSlotFlags(4);
    /// Not a reply message for a send-and-receive (C: `AMF_NOREPLY`).
    pub const NO_REPLY: AsyncSlotFlags = AsyncSlotFlags(8);
    /// Send a notification when processing completes with a failed delivery
    /// (C: `AMF_NOTIFY_ERR`).
    pub const NOTIFY_ON_ERROR: AsyncSlotFlags = AsyncSlotFlags(16);

    /// Combines two flag sets.
    pub const fn combined(self, other: AsyncSlotFlags) -> Self {
        AsyncSlotFlags(self.0 | other.0)
    }

    /// Reports whether all bits of `flag` are set.
    pub const fn contains(self, flag: AsyncSlotFlags) -> bool {
        self.0 & flag.0 == flag.0
    }

    /// Reports whether the kernel has finished with this slot: both the
    /// valid bit (set by the sender) and the done bit (set by the kernel)
    /// are present (see `asynsend.c:54`).
    pub const fn is_processed(self) -> bool {
        self.contains(AsyncSlotFlags::VALID) && self.contains(AsyncSlotFlags::DONE)
    }

    /// Reports whether a failed delivery still needs acknowledgement: the
    /// slot was processed with a nonzero result and carries a notification
    /// request (see `asynsend.c:171-172`).
    pub const fn needs_error_acknowledgement(self, result: i32) -> bool {
        self.is_processed()
            && result != 0
            && (self.contains(AsyncSlotFlags::NOTIFY)
                || self.contains(AsyncSlotFlags::NOTIFY_ON_ERROR))
    }
}

/// One entry of the asynchronous batch table.
///
/// C: `struct asynmsg { unsigned flags; endpoint_t dst; int result; message
/// msg; }` (`minix3/minix/include/minix/ipc.h:2745-2751`). The result field is
/// written by the kernel; the sender must not read it before the done bit
/// appears.
#[derive(Debug, Clone, Copy)]
pub struct AsyncSlot {    /// State flags; written last when publishing a slot.
    pub flags: AsyncSlotFlags,
    /// Destination endpoint.
    pub destination: Endpoint,
    /// Kernel-reported outcome; valid only after the done bit is set.
    pub result: i32,
    /// The message itself.
    pub message: Message,
}

impl AsyncSlot {
    /// Creates an empty slot.
    pub const fn empty() -> Self {
        AsyncSlot {
            flags: AsyncSlotFlags::EMPTY,
            destination: Endpoint(0),
            result: 0,
            message: Message::zeroed(),
        }
    }
}

/// Owned queue managing an asynchronous batch table.
///
/// This is the testable half of `asynsend3` plus `asyn_geterror` in
/// `minix3/minix/lib/libsys/asynsend.c:25-188`. The C version keeps the table
/// in static globals and rescans it through kernel calls; this version owns
/// its slots and cursors, so each unit test gets a fresh queue and tests
/// cannot pollute each other. The kernel rescan itself (handing the pending
/// slice to the trap) stays with the caller, which owns the hardware
/// boundary through [`IpcTransport`].
///
/// The queue mirrors the C cursor discipline exactly: `first` points at the
/// oldest slot that may still need attention, `next` points past the newest
/// filled slot, and a full table is compacted by moving unprocessed and
/// unacknowledged entries to the front (see `asynsend.c:81-119`).
#[derive(Debug)]
pub struct AsyncSendQueue<const CAPACITY: usize> {
    slots: [AsyncSlot; CAPACITY],
    first: usize,
    next: usize,
    initialized: bool,
}

/// Why a new message could not be queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueError {
    /// The table is full even after compaction; unacknowledged entries still
    /// occupy every slot (C: `panic("asynsend: msgtable full")` at
    /// `asynsend.c:121`).
    TableFull,
}

impl<const CAPACITY: usize> AsyncSendQueue<CAPACITY> {
    /// Creates an empty queue. Slots are marked empty on first use, matching
    /// the C lazy initialization (see `asynsend.c:41-46`).
    pub const fn new() -> Self {
        AsyncSendQueue {
            slots: [AsyncSlot::empty(); CAPACITY],
            first: 0,
            next: 0,
            initialized: false,
        }
    }

    /// Number of slots the queue can hold (C: `ASYN_NR`, twice the process
    /// count, in `asynsend.c:17`).
    pub const fn capacity(&self) -> usize {
        CAPACITY
    }

    /// Number of slots currently awaiting kernel processing or acknowledgement.
    pub fn pending_count(&self) -> usize {
        self.next.saturating_sub(self.first)
    }

    fn ensure_initialized(&mut self) {
        if !self.initialized {
            let mut index = 0;
            while index < CAPACITY {
                self.slots[index].flags = AsyncSlotFlags::EMPTY;
                index += 1;
            }
            self.initialized = true;
        }
    }

    /// Advances the front cursor past entries the kernel has finished with,
    /// remembering whether any finished entry reported an error that still
    /// needs acknowledgement (see `asynsend.c:52-73`).
    fn advance_front(&mut self) -> bool {
        let mut needs_ack = false;
        while self.first < self.next {
            let slot = &self.slots[self.first];
            if slot.flags.is_processed() {
                if slot.result != 0
                    && (slot.flags.contains(AsyncSlotFlags::NOTIFY)
                        || slot.flags.contains(AsyncSlotFlags::NOTIFY_ON_ERROR))
                {
                    needs_ack = true;
                }
                self.first += 1;
            } else if slot.flags == AsyncSlotFlags::EMPTY {
                self.first += 1;
            } else {
                break;
            }
        }
        if self.first >= self.next && !needs_ack {
            self.first = 0;
            self.next = 0;
        }
        needs_ack
    }

    /// Moves unprocessed and unacknowledged entries to the front, mirroring
    /// the compaction loop in `asynsend.c:82-119`. Returns false when even
    /// after compaction no slot is free.
    fn compact(&mut self) -> bool {
        let mut target = 0;
        let mut source = self.first;
        while source < self.next {
            let flags = self.slots[source].flags;
            let result = self.slots[source].result;
            let keep = if flags == AsyncSlotFlags::EMPTY {
                false
            } else if flags.is_processed() {
                result != 0 && flags.needs_error_acknowledgement(result)
            } else {
                true
            };
            if keep {
                if source != target {
                    self.slots[target] = self.slots[source];
                }
                target += 1;
            }
            source += 1;
        }
        let mut index = target;
        while index < CAPACITY {
            self.slots[index].flags = AsyncSlotFlags::EMPTY;
            index += 1;
        }
        self.first = 0;
        self.next = target;
        target < CAPACITY
    }

    /// Queues one message, marking the slot valid as the last write.
    ///
    /// The caller-supplied flags are combined with the valid bit (C:
    /// `fl |= AMF_VALID` at `asynsend.c:124`). When the table is full, the
    /// queue first compacts; if compaction frees nothing, it reports
    /// [`QueueError::TableFull`] instead of terminating the process — the C
    /// version panics there, but a library cannot decide for its caller
    /// whether a full table is fatal.
    pub fn enqueue(
        &mut self,
        destination: Endpoint,
        message: Message,
        flags: AsyncSlotFlags,
    ) -> Result<(), QueueError> {
        self.ensure_initialized();
        self.advance_front();
        if self.next >= CAPACITY && !self.compact() {
            return Err(QueueError::TableFull);
        }
        let slot = &mut self.slots[self.next];
        slot.destination = destination;
        slot.message = message;
        slot.result = 0;
        // The flags write must happen last: the kernel scans the table
        // concurrently and treats a valid flag as permission to read the
        // other fields (see asynsend.c:127-131).
        slot.flags = flags.combined(AsyncSlotFlags::VALID);
        self.next += 1;
        Ok(())
    }

    /// Returns the pending slice the caller should hand to the trap.
    ///
    /// C: `senda_reload` passes `&msgtable[first_slot]` with length
    /// `next_slot - first_slot` (see `asynsend.c:144-155`).
    pub fn pending_slice(&self) -> &[AsyncSlot] {
        &self.slots[self.first..self.next]
    }

    /// Records kernel-reported outcomes for the pending slots.
    ///
    /// Test and driver code uses this to model what the kernel does in the
    /// background: each pending slot whose destination matches gains the done
    /// bit and the given result. Real binaries never call this; the kernel
    /// writes the results itself.
    pub fn record_completion(&mut self, destination: Endpoint, result: i32) {
        let mut index = self.first;
        while index < self.next {
            if self.slots[index].destination == destination
                && self.slots[index].flags.contains(AsyncSlotFlags::VALID)
                && !self.slots[index].flags.contains(AsyncSlotFlags::DONE)
            {
                self.slots[index].result = result;
                self.slots[index].flags =
                    self.slots[index].flags.combined(AsyncSlotFlags::DONE);
            }
            index += 1;
        }
    }

    /// Retrieves one completed-with-error delivery, acknowledging it.
    ///
    /// This mirrors `asyn_geterror` (see `asynsend.c:160-188`): it finds the
    /// first processed slot with a nonzero result and a notification request,
    /// copies out the destination, message, and error, resets the result to
    /// success so the entry can be reclaimed, and reports it once. Returns
    /// `None` when no such entry exists (including before initialization).
    pub fn take_error(&mut self) -> Option<(Endpoint, Message, i32)> {
        if !self.initialized {
            return None;
        }
        let mut index = 0;
        while index < self.next {
            let flags = self.slots[index].flags;
            let result = self.slots[index].result;
            if flags.needs_error_acknowledgement(result) {
                let found = (
                    self.slots[index].destination,
                    self.slots[index].message,
                    result,
                );
                self.slots[index].result = 0;
                return Some(found);
            }
            index += 1;
        }
        None
    }
}

impl<const CAPACITY: usize> Default for AsyncSendQueue<CAPACITY> {
    fn default() -> Self {
        Self::new()
    }
}

/// Hardware boundary for communication traps.
///
/// Every method corresponds to one assembly routine in
/// `minix3/minix/lib/libc/arch/i386/sys/_ipc.S` (plus `query_kerninfo_page`
/// for `ipc_minix_kerninfo.S`). The trait has two behaviorally different
/// implementations (direct trap vs. scripted test double) and is used as a
/// generic bound by the system call layer, which keeps the abstraction
/// justified under the project rule for traits.
pub trait IpcTransport {
    /// Blocking send (C: `_ipc_send_intr`, `_ipc.S:16-26`).
    fn send(&self, destination: Endpoint, message: &Message) -> Result<(), TrapStatus>;
    /// Blocking receive; returns the decoded status word (C:
    /// `_ipc_receive_intr`, `_ipc.S:28-40`, status via the third parameter).
    fn receive(&self, source: Endpoint, message: &mut Message) -> Result<IpcStatus, TrapStatus>;
    /// Send-and-receive (C: `_ipc_sendrec_intr`, `_ipc.S:42-52`).
    fn sendrec(&self, destination: Endpoint, message: &mut Message) -> Result<(), TrapStatus>;
    /// Notify without a message body (C: `_ipc_notify_intr`, `_ipc.S:54-63`).
    fn notify(&self, destination: Endpoint) -> Result<(), TrapStatus>;
    /// Non-blocking send (C: `_ipc_sendnb_intr`, `_ipc.S:65-75`).
    fn sendnb(&self, destination: Endpoint, message: &Message) -> Result<(), TrapStatus>;
    /// Batch send of a table slice (C: `_ipc_senda_intr`, `_ipc.S:77-87`;
    /// note the swapped register order: count first, table second).
    fn senda(&self, table: &[AsyncSlot]) -> Result<(), TrapStatus>;
    /// Fetch the kernel information page address (C: `ipc_minix_kerninfo`,
    /// `ipc_minix_kerninfo.S:4-16`: zeroes both registers, traps with call
    /// number 6, stores the returned pointer through the caller's pointer).
    fn query_kerninfo_page(&self) -> Result<u64, TrapStatus>;
}

/// Direct-trap transport used by real binaries.
///
/// Each method executes the corresponding trap instruction. In a hosted test
/// environment no kernel is present to answer, so every method reports a
/// failure status instead of faulting; the failure is explicit (a returned
/// error, never a panic), and callers handle it through the normal error
/// paths. The real trap instruction sequences will replace these bodies when
/// the 64-bit trap wiring lands (stage plan item A-6); until then this type
/// marks the intent "go through the real trap".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirectTrapTransport;

impl IpcTransport for DirectTrapTransport {
    fn send(&self, _destination: Endpoint, _message: &Message) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn receive(
        &self,
        _source: Endpoint,
        _message: &mut Message,
    ) -> Result<IpcStatus, TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn sendrec(
        &self,
        _destination: Endpoint,
        _message: &mut Message,
    ) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn notify(&self, _destination: Endpoint) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn sendnb(&self, _destination: Endpoint, _message: &Message) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn senda(&self, _table: &[AsyncSlot]) -> Result<(), TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
    fn query_kerninfo_page(&self) -> Result<u64, TrapStatus> {
        Err(TrapStatus(minix_types::EIO))
    }
}

/// Scripted transport used by unit tests.
///
/// Each test configures the replies its scenario needs. Call counters use
/// [`core::cell::Cell`] because the transport trait borrows shared while a
/// script must advance: instances are test-local and never shared across
/// threads, so the lack of synchronization is safe by construction.
#[derive(Debug, Default)]
pub struct CannedTransport {
    /// Replies handed out in order for send-and-receive calls.
    pub sendrec_replies: Vec<Result<Message, TrapStatus>>,
    /// How many send-and-receive calls happened so far.
    pub sendrec_calls: core::cell::Cell<usize>,
}

impl CannedTransport {
    /// Creates an empty script.
    pub fn new() -> Self {
        CannedTransport {
            sendrec_replies: Vec::new(),
            sendrec_calls: core::cell::Cell::new(0),
        }
    }

    /// Appends a send-and-receive reply to the script.
    pub fn reply_sendrec(&mut self, reply: Result<Message, TrapStatus>) {
        self.sendrec_replies.push(reply);
    }

    /// Takes the next scripted send-and-receive reply.
    fn next_sendrec_reply(&self) -> Result<Message, TrapStatus> {
        let index = self.sendrec_calls.get();
        self.sendrec_calls.set(index + 1);
        self.sendrec_replies
            .get(index)
            .cloned()
            .unwrap_or(Err(TrapStatus(minix_types::EIO)))
    }
}

impl IpcTransport for CannedTransport {
    fn send(&self, _destination: Endpoint, _message: &Message) -> Result<(), TrapStatus> {
        Ok(())
    }
    fn receive(
        &self,
        _source: Endpoint,
        _message: &mut Message,
    ) -> Result<IpcStatus, TrapStatus> {
        Ok(IpcStatus::from_call(CALL_SENDREC))
    }
    fn sendrec(&self, _destination: Endpoint, message: &mut Message) -> Result<(), TrapStatus> {
        // Apply the scripted reply to the message, like a real round trip.
        match self.next_sendrec_reply() {
            Ok(reply) => {
                *message = reply;
                Ok(())
            }
            Err(status) => Err(status),
        }
    }
    fn notify(&self, _destination: Endpoint) -> Result<(), TrapStatus> {
        Ok(())
    }
    fn sendnb(&self, _destination: Endpoint, _message: &Message) -> Result<(), TrapStatus> {
        Ok(())
    }
    fn senda(&self, _table: &[AsyncSlot]) -> Result<(), TrapStatus> {
        Ok(())
    }
    fn query_kerninfo_page(&self) -> Result<u64, TrapStatus> {
        Ok(0x1000)
    }
}

/// Maps a trap failure to the public communication error type.
///
/// The kernel reports failures as raw status codes; the user-space library
/// surfaces them as [`IpcError`]. A failed trap without further detail maps
/// to `Interrupted`: the operation did not complete and the caller should
/// decide whether to retry. The mapping lives in one place so later trap
/// wiring can refine it without touching callers.
pub const fn trap_status_to_ipc_error(_status: TrapStatus) -> IpcError {
    IpcError::Interrupted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_message(message_type: i32) -> Message {
        Message {
            m_source: Endpoint(0),
            m_type: message_type,
            m_u: unsafe { core::mem::zeroed() },
        }
    }

    #[test]
    fn test_call_numbers_match_ipcconst_header() {
        assert_eq!(CALL_SEND, 1);
        assert_eq!(CALL_RECEIVE, 2);
        assert_eq!(CALL_SENDREC, 3);
        assert_eq!(CALL_NOTIFY, 4);
        assert_eq!(CALL_SENDNB, 5);
        assert_eq!(CALL_KERNINFO, 6);
        assert_eq!(CALL_SENDA, 16);
        assert_eq!(HIGHEST_CALL_NUMBER, 16);
    }

    #[test]
    fn test_trap_vector_numbers_match_arch_header() {
        assert_eq!(TrapVector::KernelCall.number(), 32);
        assert_eq!(TrapVector::InterProcess.number(), 33);
        assert_eq!(TrapVector::UserMappedKernelCall.number(), 34);
        assert_eq!(TrapVector::UserMappedInterProcess.number(), 35);
    }

    #[test]
    fn test_status_call_round_trip() {
        let status = IpcStatus::from_call(CALL_SENDREC);
        assert_eq!(status.call(), CALL_SENDREC);
        // Bits above the six call bits are masked away.
        assert_eq!(IpcStatus(0xFFFF_FFFF).call(), STATUS_CALL_MASK);
    }

    #[test]
    fn test_status_flags_round_trip() {
        let status = IpcStatus::with_flags(STATUS_FLAG_FROM_KERNEL);
        assert!(status.flags_test(STATUS_FLAG_FROM_KERNEL));
        assert!(status.is_from_kernel());
        assert!(!IpcStatus(0).is_from_kernel());
    }

    #[test]
    fn test_async_flags_match_ipc_header_octal_values() {
        assert_eq!(AsyncSlotFlags::EMPTY.0, 0);
        assert_eq!(AsyncSlotFlags::VALID.0, 1);
        assert_eq!(AsyncSlotFlags::DONE.0, 2);
        assert_eq!(AsyncSlotFlags::NOTIFY.0, 4);
        // C writes these in octal: 010 is eight, 020 is sixteen.
        assert_eq!(AsyncSlotFlags::NO_REPLY.0, 8);
        assert_eq!(AsyncSlotFlags::NOTIFY_ON_ERROR.0, 16);
    }

    #[test]
    fn test_processed_detection_needs_both_bits() {
        let valid_only = AsyncSlotFlags::VALID;
        let done_only = AsyncSlotFlags::DONE;
        assert!(!valid_only.is_processed());
        assert!(!done_only.is_processed());
        assert!(valid_only.combined(AsyncSlotFlags::DONE).is_processed());
    }

    #[test]
    fn test_enqueue_marks_slot_valid_last() {
        let mut queue = AsyncSendQueue::<4>::new();
        queue
            .enqueue(Endpoint(7), test_message(42), AsyncSlotFlags::NOTIFY)
            .unwrap();
        assert_eq!(queue.pending_count(), 1);
        let pending = queue.pending_slice();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].destination, Endpoint(7));
        assert_eq!(pending[0].message.m_type, 42);
        assert!(pending[0].flags.contains(AsyncSlotFlags::VALID));
        assert!(pending[0].flags.contains(AsyncSlotFlags::NOTIFY));
    }

    #[test]
    fn test_completed_ok_entries_are_reclaimed() {
        let mut queue = AsyncSendQueue::<4>::new();
        queue
            .enqueue(Endpoint(7), test_message(1), AsyncSlotFlags::EMPTY)
            .unwrap();
        queue.record_completion(Endpoint(7), 0);
        // Reclaiming happens on the next enqueue, like the C front advance.
        queue
            .enqueue(Endpoint(8), test_message(2), AsyncSlotFlags::EMPTY)
            .unwrap();
        assert_eq!(queue.pending_count(), 1);
        assert_eq!(queue.pending_slice()[0].destination, Endpoint(8));
    }

    #[test]
    fn test_error_needs_notification_request_to_surface() {
        let mut queue = AsyncSendQueue::<4>::new();
        // Without a notification request, the error is silently dropped on
        // reclaim, matching the C cleanup condition.
        queue
            .enqueue(Endpoint(7), test_message(1), AsyncSlotFlags::EMPTY)
            .unwrap();
        queue.record_completion(Endpoint(7), -5);
        assert!(queue.take_error().is_none());

        let mut queue = AsyncSendQueue::<4>::new();
        queue
            .enqueue(Endpoint(7), test_message(1), AsyncSlotFlags::NOTIFY_ON_ERROR)
            .unwrap();
        queue.record_completion(Endpoint(7), -5);
        let found = queue.take_error().expect("error must surface");
        assert_eq!(found.0, Endpoint(7));
        assert_eq!(found.2, -5);
        // Acknowledged exactly once.
        assert!(queue.take_error().is_none());
    }

    #[test]
    fn test_full_table_reports_error_instead_of_panicking() {
        let mut queue = AsyncSendQueue::<2>::new();
        queue
            .enqueue(Endpoint(1), test_message(1), AsyncSlotFlags::EMPTY)
            .unwrap();
        queue
            .enqueue(Endpoint(2), test_message(2), AsyncSlotFlags::EMPTY)
            .unwrap();
        match queue.enqueue(Endpoint(3), test_message(3), AsyncSlotFlags::EMPTY) {
            Err(QueueError::TableFull) => {}
            other => panic!("expected TableFull, got {:?}", other),
        }
    }

    #[test]
    fn test_uninitialized_queue_reports_no_error() {
        let mut queue = AsyncSendQueue::<4>::new();
        assert!(queue.take_error().is_none());
    }

    #[test]
    fn test_direct_trap_reports_explicit_failure() {
        let transport = DirectTrapTransport;
        assert!(transport.send(Endpoint(1), &test_message(0)).is_err());
        assert!(transport.notify(Endpoint(1)).is_err());
        assert!(transport.query_kerninfo_page().is_err());
    }
}
