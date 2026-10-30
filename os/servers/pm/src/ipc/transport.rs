//! PM IPC transport — strategy trait for kernel IPC boundary.
//!
//! C counterpart: `minix3/minix/lib/libsys/ipc_send.c` / `ipc_sendrec`
//!         (libsys.a; a single-OS build picks the kernel IPC implementation
//!         at link time).
//!
//! Same shape as VM's `os/servers/vm/src/ipc/transport.rs`: a trait abstracts
//! the "kernel IPC primitives", and the production path
//! (`KernelIpcTransport`) and the test path (`TestIpcTransport` mock) share
//! one body of calling code.
//!
//! Users: 01's `vfs_init_sync` (VFS_PM_INIT sync), 04's main loop (receive +
//! dispatch + reply), 05's asynchronous VFS reply.

use minix_sys::ipc::IpcTransport as _;
use minix_types::{Endpoint, Message};

/// IPC receive status word, mirroring the `rcv_sts` out-parameter of C's
/// `sef_receive_status`.
///
/// The kernel fills in flags describing the message (notification versus
/// ordinary IPC). The Rust side only needs to know whether the message is a
/// notification (`is_ipc_notify`).
///
/// C: `IPC_STATUS_CALL(status) == NOTIFY` (minix/com.h:92;
///     minix/ipcconst.h:10/16: `NOTIFY = 4`, `IPC_STATUS_CALL_MASK = 0x3F`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IpcStatus {
    /// Raw kernel status bits.
    pub flags: u32,
}

impl IpcStatus {
    /// Whether this is an asynchronous notification.
    ///
    /// A notification is an asynchronous signal (kernel interrupt / clock
    /// tick), not a request message; the main loop must skip it before
    /// endpoint validation (main.c:65-71).
    pub fn is_notify(&self) -> bool {
        // C: IPC_STATUS_CALL(status) = (status >> 0) & 0x3F (ipcconst.h:22-24).
        (self.flags & 0x3F) == 4 // NOTIFY
    }
}

/// IPC transport error — a PM-local type mirroring the raw-errno return
/// convention of C's `ipc_*`.
///
/// Unlike `minix_types::IpcError` (four variants, no raw payload): the
/// error-fidelity rule requires that the kernel's raw negative errno stay
/// reachable without folding (EIO/EINVAL etc. really exist on the minix3 IPC
/// error surface), so this type carries it faithfully as `Kernel(i32)`; the
/// named variants keep only the high-frequency values used by test mocks and
/// common paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcTransportError {
    /// `EAGAIN`/`EWOULDBLOCK` — no message in non-blocking mode.
    WouldBlock,
    /// `EPERM` — not permitted to send to the target.
    NoPerm,
    /// Any other raw kernel negative errno, carried verbatim
    /// (EIO/EINVAL/EINTR/...).
    Kernel(i32),
}

impl IpcTransportError {
    /// Build from a raw kernel status word (faithful: high-frequency values
    /// get a name, the rest pass through).
    pub fn from_errno(code: i32) -> Self {
        match code {
            minix_types::EAGAIN => Self::WouldBlock,
            minix_types::EPERM => Self::NoPerm,
            other => Self::Kernel(other),
        }
    }

    /// The errno value (used by callers passing it through to user space).
    pub fn errno(&self) -> i32 {
        match self {
            Self::WouldBlock => minix_types::EAGAIN,
            Self::NoPerm => minix_types::EPERM,
            Self::Kernel(code) => *code,
        }
    }
}

/// The IPC transport strategy trait.
///
/// # Why a trait (rather than free functions)
///
/// In C, `ipc_send`/`ipc_sendrec` link against libsys.a; on the Rust side
/// kernel IPC is not yet in place, so a trait lets `#[cfg(test)]` substitute a
/// mock while production keeps the same interface (trait quality rule: at
/// least two behaviorally distinct implementations + use through a generic
/// bound).
pub trait IpcTransport {
    /// Receive one IPC message from any source. Mirrors C's
    /// `sef_receive_status(ANY, &msg, &rcv_sts)` (main.c:61).
    ///
    /// Blocks until a message arrives (C semantics); `IpcStatus` lets the main
    /// loop test for a notification.
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcTransportError>;

    /// Non-blocking send. Mirrors C's `ipc_sendnb(dest, &msg)` — used for
    /// SEF `reply()` and asynsend-style one-shot delivery (returns
    /// `ENOTREADY` when the target is not yet receiving, without suspending
    /// the caller). Nearly all of PM's `send` sites are replies
    /// (exit/trace/signal/fork) or the asynchronous tell_vfs; none of them
    /// blocks in C, which is what this method maps. The one blocking
    /// `ipc_send` case is `send_blocking`.
    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError>;

    /// Blocking send. Mirrors C's `ipc_send(dest, &msg)` (the `SEND` system
    /// call, main.c:226) — suspends the caller until delivery when the target
    /// has not entered receive, absorbing startup-order races. Used only by
    /// the VFS_PM_INIT handshake (PM must start before VFS, and per-message
    /// sends must wait for VFS to enter receive); replies and asynchronous
    /// sends must not use this method (it would break C's non-blocking reply
    /// semantics).
    fn send_blocking(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError>;

    /// Send and await the reply (synchronous call). Mirrors C's
    /// `ipc_sendrec(dest, &msg)`.
    ///
    /// C semantics: the reply lands in the same message buffer (`m_type` is
    /// overwritten with the reply value).
    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcTransportError>;
}

// ── Production implementation: kernel IPC ──

/// Production IPC transport.
///
/// Delegates to minix-sys's user-space trap backend (`DirectTrapTransport`):
/// under the real-trap feature it executes the real trap instruction sequence
/// (E1 went live on 2026-09-16/17), and a host build answers `-EIO` — the
/// failure is explicitly observable (returns an error, never panics) and is
/// handled by the main loop's error path, indistinguishable from "the kernel
/// link is slow or dead".
pub struct KernelIpcTransport {
    /// User-space trap backend. The call shape is the final shape; going live
    /// needs no change to this type.
    inner: minix_sys::ipc::DirectTrapTransport,
}

impl KernelIpcTransport {
    pub const fn new() -> Self {
        Self {
            inner: minix_sys::ipc::DirectTrapTransport,
        }
    }
}

impl Default for KernelIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for KernelIpcTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcTransportError> {
        // C: ipc_receive(ANY, &msg, &rcv_sts) (libsys ipc_kern.c; SEF
        // classification is a separate seam in the main loop above).
        // minix-sys's IpcStatus(u32) is the raw status word this type's
        // `flags` models (NOTIFY = low 6 bits == 4).
        let mut msg = Message::default();
        match self.inner.receive(Endpoint::ANY, &mut msg) {
            Ok(sts) => Ok((msg, IpcStatus { flags: sts.0 })),
            Err(trap) => Err(IpcTransportError::from_errno(trap.0)),
        }
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError> {
        // C: ipc_sendnb(dest, &msg) (non-blocking; returns ENOTREADY when the
        // target is not ready). PM's reply and asynchronous tell_vfs sites
        // both match this semantics (SEF reply() does not block).
        self.inner
            .sendnb(dest, msg)
            .map_err(|trap| IpcTransportError::from_errno(trap.0))
    }

    fn send_blocking(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError> {
        // C: ipc_send(dest, &msg) (main.c:226, the SEND system call) —
        // suspends the caller when the target has not entered receive.
        // Delegates to the backend's blocking send (SEND_NR), not sendnb.
        self.inner
            .send(dest, msg)
            .map_err(|trap| IpcTransportError::from_errno(trap.0))
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcTransportError> {
        // C: ipc_sendrec(dest, &msg) — the reply lands in the same message
        // buffer.
        self.inner
            .sendrec(dest, msg)
            .map_err(|trap| IpcTransportError::from_errno(trap.0))
    }
}

// ── Test implementation: in-memory mock ──

/// Test-only IPC transport.
///
/// Records the destination and message of every `send` (for assertions), and
/// preloads replies for `sendrec`: with a script
/// (`queue_sendrec_reply`) the whole reply message is dequeued and written
/// (payload fields included, for protocols like VM_FORK where "the reply is
/// the payload"); without a script it falls back to the old behavior —
/// overwriting only the reply `m_type` (default OK=0, modeling a VFS
/// acknowledgement).
///
/// ⚠️ **For test and integration-test injection only**. `send` merely records
/// messages into a local buffer and `receive` returns WouldBlock at once —
/// production code constructing this type means all outbound communication on
/// that path is lost (the TSTL pattern, see todo.md §11.5). No
/// `#[cfg(test)]` gate because integration tests (`tests/`) compile this crate
/// as an ordinary dependency and cannot see cfg(test) items.
#[derive(Debug)]
pub struct TestIpcTransport {
    /// Records of all `send` calls, in order.
    sent: alloc::vec::Vec<(Endpoint, Message)>,
    /// Queue of scripted `sendrec` replies (whole messages, dequeued in
    /// order).
    sendrec_replies: alloc::collections::VecDeque<Message>,
    /// Reply `m_type` written into the message by `sendrec` when no script is
    /// set.
    reply_type: i32,
    /// Message (plus status word) returned by the next `receive`.
    next_receive: Option<(Message, IpcStatus)>,
}

impl TestIpcTransport {
    /// Create an empty mock (`sendrec` replies OK; `receive` returns `Err`
    /// when there is no message, modeling "no message").
    pub fn new() -> Self {
        Self {
            sent: alloc::vec::Vec::new(),
            sendrec_replies: alloc::collections::VecDeque::new(),
            reply_type: 0,
            next_receive: None,
        }
    }

    /// Set the reply `m_type` for `sendrec` (default 0 = OK; only effective
    /// when no script is queued).
    pub fn set_reply_type(&mut self, reply: i32) {
        self.reply_type = reply;
    }

    /// Enqueue one scripted `sendrec` reply (whole message, dequeued in
    /// order).
    ///
    /// Difference from `set_reply_type`: a scripted reply **overwrites the
    /// caller's whole message buffer** — `m_type` and payload fields such as
    /// m1/m3 all come from the script, letting protocols like `vm_fork`
    /// ("the reply is the payload", VMF_CHILD_ENDPOINT in m1i3) build a
    /// realistically shaped answer; the fallback without a script overwrites
    /// only `m_type`.
    pub fn queue_sendrec_reply(&mut self, reply: Message) {
        self.sendrec_replies.push_back(reply);
    }

    /// Preload the message returned by the next `receive` (for main-loop
    /// tests).
    pub fn queue_receive(&mut self, msg: Message, status: IpcStatus) {
        self.next_receive = Some((msg, status));
    }

    /// The (destination, message) list already sent, in order.
    pub fn sent(&self) -> &[(Endpoint, Message)] {
        &self.sent
    }

    /// Destination endpoint of the most recent `send` (for `tell_vfs`
    /// tests).
    pub fn last_sent_dest(&self) -> Option<Endpoint> {
        self.sent.last().map(|(dest, _)| *dest)
    }
}

impl Default for TestIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for TestIpcTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcTransportError> {
        self.next_receive.take().ok_or(IpcTransportError::WouldBlock)
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError> {
        self.sent.push((dest, *msg));
        Ok(())
    }

    fn send_blocking(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcTransportError> {
        // The mock does not distinguish blocking from non-blocking: it just
        // records the delivery (same as send).
        self.sent.push((dest, *msg));
        Ok(())
    }

    fn sendrec(&mut self, dest: Endpoint, msg: &mut Message) -> Result<(), IpcTransportError> {
        self.send(dest, msg)?;
        // C: the reply lands in the same message buffer (main.c:246-249
        // checks `mess.m_type != OK`). With a script → whole-message
        // overwrite; without → only m_type.
        if let Some(reply) = self.sendrec_replies.pop_front() {
            *msg = reply;
        } else {
            msg.m_type = self.reply_type;
        }
        Ok(())
    }
}

// ── Selector ──

/// Select the IPC transport for the current build.
///
/// `#[cfg(test)]` returns the mock; production returns the kernel
/// implementation (which would `unimplemented!()` before kernel IPC landed).
#[cfg(test)]
pub fn ipc_transport_for_build() -> TestIpcTransport {
    TestIpcTransport::new()
}

#[cfg(not(test))]
pub fn ipc_transport_for_build() -> KernelIpcTransport {
    KernelIpcTransport::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_records_send() {
        let mut t = TestIpcTransport::new();
        let msg = Message { m_type: 0x900, ..Message::default() }; // VFS_PM_INIT
        t.send(Endpoint::VFS, &msg).unwrap();
        assert_eq!(t.sent().len(), 1);
        assert_eq!(t.sent()[0].0, Endpoint::VFS);
        assert_eq!(t.sent()[0].1.m_type, 0x900);
    }

    #[test]
    fn test_mock_records_send_blocking() {
        // A blocking send (VFS_PM_INIT matching C's ipc_send) records the
        // delivery like send.
        let mut t = TestIpcTransport::new();
        let msg = Message { m_type: 0x900, ..Message::default() }; // VFS_PM_INIT
        t.send_blocking(Endpoint::VFS, &msg).unwrap();
        assert_eq!(t.sent().len(), 1);
        assert_eq!(t.sent()[0].0, Endpoint::VFS);
        assert_eq!(t.sent()[0].1.m_type, 0x900);
    }

    #[test]
    fn ipc_status_notify_bit_matches_minix3() {
        // C: IPC_STATUS_CALL(status) = (status >> 0) & 0x3F; NOTIFY = 4
        // (ipcconst.h:10/16/22-24).
        assert!(IpcStatus { flags: 4 }.is_notify());
        assert!(
            !IpcStatus {
                flags: 3 /* SENDREC */
            }
            .is_notify()
        );
        assert!(!IpcStatus { flags: 0 }.is_notify());
        // High-order flags (e.g. IPC_FLG_MSG_FROM_KERNEL, bit 16) do not
        // affect the low 6 CALL bits.
        assert!(
            IpcStatus {
                flags: 4 | (1 << 16)
            }
            .is_notify()
        );
    }

    #[test]
    fn test_mock_receive_returns_queued_message() {
        let mut t = TestIpcTransport::new();
        let mut msg = Message { m_type: 0x900, ..Message::default() }; // VFS_PM_INIT
        msg.m_source = Endpoint::VFS;
        t.queue_receive(msg, IpcStatus { flags: 0 });
        let (got, sts) = t.receive().unwrap();
        assert_eq!(got.m_type, 0x900);
        assert_eq!(got.m_source, Endpoint::VFS);
        assert!(!sts.is_notify());
        // The queue is consumed once; a second receive has no message
        // (WouldBlock).
        assert!(matches!(t.receive(), Err(IpcTransportError::WouldBlock)));
    }

    #[test]
    fn test_mock_sendrec_overwrites_type() {
        let mut t = TestIpcTransport::new();
        let mut msg = Message { m_type: 0x900, ..Message::default() };
        t.sendrec(Endpoint::VFS, &mut msg).unwrap();
        // Default reply OK=0 (modeling a VFS acknowledgement).
        assert_eq!(msg.m_type, 0);
    }

    #[test]
    fn test_mock_sendrec_custom_reply() {
        let mut t = TestIpcTransport::new();
        t.set_reply_type(5);
        let mut msg = Message::default();
        t.sendrec(Endpoint::VFS, &mut msg).unwrap();
        assert_eq!(msg.m_type, 5);
    }
}
