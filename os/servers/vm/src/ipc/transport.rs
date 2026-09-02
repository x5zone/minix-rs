//! IPC transport abstraction for VM server.
//!
//! # Motivation
//!
//! The VM main loop in `vm_server.rs::run()` calls `ipc_receive()` and
//! `ipc_send()` to communicate with other system services. In Minix3, these
//! are the C library `sef_receive_status(ANY, &msg, &rcv_sts)` and
//! `ipc_send(dest, &msg)` calls, which delegate to kernel syscalls.
//!
//! Until the kernel IPC layer (`minix-kernel` kernel IPC core) is ready, VM cannot
//! make real IPC calls. The previous implementation hard-coded `Err(())` in
//! two free functions, which (a) made the main loop panic on the first
//! iteration, and (b) made it impossible to inject test doubles.
//!
//! # Design
//!
//! [`IpcTransport`] is a strategy trait that abstracts the IPC backend.
//! Two impls are provided:
//!
//! - [`KernelIpcTransport`] — production impl that calls `sys_ipc_*`.
//!   Currently `unimplemented!()` because the kernel IPC primitive is
//!   not yet ready (depends on kernel IPC core). Selecting this impl in non-test
//!   builds will still panic at runtime, but now with a clear "wiring
//!   pending" message instead of a misleading `Err(())`.
//! - [`TestIpcTransport`] — mock impl that buffers/returns test data.
//!   `#[cfg(test)]`-only; injected through `VmServer::new_for_test`
//!   (V10-P0-2). Lets unit tests drive the main loop without a live
//!   kernel.
//!
//! # See also
//!
//! - VM IPC blocking bug analysis (original P0 blocking bug description)
//! - `Doc 24-vm-ipc-dispatch.md` §3 — main loop and IPC message flow
//! - Minix3 `lib/syslib/sys_ipc_receive.c` — kernel IPC primitive

use minix_types::{Endpoint, Message};

/// IPC receive status word, mirroring the `rcv_sts` output parameter of
/// Minix3's `sef_receive_status()`. The kernel fills in the flags describing
/// the message (e.g. notification vs. regular IPC, sender is kernel, etc.).
///
/// The Rust side currently only needs to know whether the message is a
/// notification (`is_ipc_notify`); the rest is `#[allow(dead_code)]` until
/// the kernel IPC returns the full status word.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IpcStatus {
    /// Raw status bits from the kernel. See Minix3 `ipc.h` `IPC_STATUS_*`.
    pub flags: u32,
}

impl IpcStatus {
    /// C: `is_ipc_notify(ipc_status)` — `IPC_STATUS_CALL(status) == NOTIFY`
    /// (minix/com.h:92, minix/ipcconst.h:16: `NOTIFY == 4`).
    ///
    /// Notifications are async signals (kernel interrupts / RS pings), not
    /// request messages; the main loop must skip them before endpoint
    /// validation (V10-P1-1, main.c:126-129).
    pub fn is_notify(&self) -> bool {
        (self.flags & 0x3F) == 4 // NOTIFY
    }

    /// C: `IPC_STATUS_FLAGS_TEST(rcv_sts, IPC_FLG_MSG_FROM_KERNEL)`
    /// (minix/ipcconst.h:22-24) — the message originated in the kernel on
    /// behalf of a process (pagefaults, signals), never reply to it.
    ///
    /// DEAD until kernel IPC core: `KernelIpcTransport::receive` fills
    /// `flags` for real; until then every status is `default()` and this
    /// returns `false`.
    pub fn is_from_kernel(&self) -> bool {
        ((self.flags >> 16) & 1) != 0
    }
}

/// Errors that can occur during IPC transport operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    /// The transport is not connected (test mode) or kernel IPC is
    /// not yet implemented (production mode, before kernel IPC core lands).
    Unimplemented,
    /// Destination endpoint is invalid (`NONE` or out of range).
    InvalidEndpoint,
    /// Send queue is full.
    // V10-P2-1 (DEFERRED): no producer until kernel IPC core returns
    // EAGAIN-style errors.
    #[allow(dead_code)]
    WouldBlock,
    /// The kernel returned a generic error (carries the raw code).
    #[allow(dead_code)]
    Kernel(i32),
}

impl core::fmt::Display for IpcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            IpcError::Unimplemented => write!(f, "IPC transport not implemented"),
            IpcError::InvalidEndpoint => write!(f, "invalid IPC endpoint"),
            IpcError::WouldBlock => write!(f, "IPC would block (queue full)"),
            IpcError::Kernel(code) => write!(f, "kernel IPC error: {}", code),
        }
    }
}

/// Strategy trait for IPC transport. The VM main loop dispatches through
/// this trait so that production and test paths share a single code path.
///
/// # Why a trait (not a free function)?
///
/// C: Minix3 uses free functions `ipc_send` / `sef_receive_status` that
///    link against `libsys.a`. The single-OS build picks the kernel IPC
///    implementation at link time.
///
/// Rust: The kernel IPC is not yet available (kernel IPC core). Using a trait
///    allows us to substitute a mock in `#[cfg(test)]` builds and a
///    real (currently `unimplemented!()`) impl otherwise. Two distinct
///    implementations (kernel vs. test) justify the trait abstraction
///    per the "trait quality" guideline.
pub trait IpcTransport {
    /// Receive an IPC message from any source. Mirrors C
    /// `sef_receive_status(ANY, &msg, &rcv_sts)`.
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError>;

    /// Send an IPC message to a destination endpoint. Mirrors C
    /// `ipc_send(dest, &msg)`.
    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcError>;

    /// Mark the transport as ready before the main loop starts.
    ///
    /// C: SEF startup (`__minix_init`, main.c:480) initializes the IPC
    /// vectors; production impls seed kernel-side state here. Default
    /// no-op so test transports need no special handling (V10-P0-2).
    fn mark_initialized(&mut self) {}
}

// ── Production impl: kernel IPC ──

/// Production IPC transport. Calls into `minix-syscall` (or equivalent)
/// to invoke the kernel's IPC primitives.
///
/// **Status (2026-06-13)**: The `sys_ipc_*` primitives are not yet
/// available because the kernel IPC engine (kernel IPC core) is pending. This
/// impl is wired into `vm_server.rs` via a static instance and panics
/// with a clear "wiring pending" message — *not* the previous opaque
/// `Err(())` — so the failure mode is self-documenting.
///
/// Once kernel IPC core lands, the body of `receive`/`send` should be replaced
/// with the real syscall invocations. The trait surface is stable.
pub struct KernelIpcTransport {
    /// Tracks whether the transport has been initialized. The C side
    /// does this implicitly via SEF startup; we expose it explicitly
    /// so tests can assert against it.
    initialized: bool,
}

impl KernelIpcTransport {
    pub const fn new() -> Self {
        Self { initialized: false }
    }
}

impl Default for KernelIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for KernelIpcTransport {
    fn mark_initialized(&mut self) {
        // Today this only flips the readiness flag; once kernel IPC core
        // lands it will also seed any kernel-side state (SEF startup).
        self.initialized = true;
    }

    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError> {
        // C: sef_receive_status(ANY, &msg, &rcv_sts) — libsys.so
        // Rust: blocked on kernel IPC core. The `initialized`
        // flag is checked to give a more informative error than a
        // bare `unimplemented!()`.
        if !self.initialized {
            return Err(IpcError::Unimplemented);
        }
        // Once the kernel IPC primitive is available, replace this
        // body with the syscall call. The signature is stable.
        unimplemented!(
            "KernelIpcTransport::receive — wiring pending kernel IPC core"
        );
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
        if dest == Endpoint::NONE {
            return Err(IpcError::InvalidEndpoint);
        }
        // C: ipc_send(dest, &msg) — libsys.so
        if !self.initialized {
            return Err(IpcError::Unimplemented);
        }
        let _ = msg;
        unimplemented!(
            "KernelIpcTransport::send — wiring pending kernel IPC core"
        );
    }
}

// ── Test impl: in-memory mock (cfg(test) only; V10-P0-2) ──

/// Test-only IPC transport. Holds a single pre-loaded message that
/// `receive` returns, and records every `send` so tests can inspect
/// what the main loop would have replied.
///
/// The mutable state lives behind an `Rc` so a test can keep a
/// [`TestTransportHandle`] after the transport itself is boxed into
/// `VmServer` (`VmServer::new_for_test`, V10-P0-2) — the server runs
/// `receive`/`send` through the trait object while the test queues
/// messages and inspects replies through the handle.
#[cfg(test)]
pub struct TestIpcTransport {
    shared: alloc::rc::Rc<TestTransportShared>,
}

#[cfg(test)]
impl TestIpcTransport {
    pub fn new() -> Self {
        Self {
            shared: alloc::rc::Rc::new(TestTransportShared {
                next_receive: core::cell::RefCell::new(None),
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                should_fail: core::cell::Cell::new(false),
            }),
        }
    }

    /// A handle to this transport's state, kept by the test after the
    /// transport is moved into `VmServer` (V10-P0-2).
    pub fn handle(&self) -> TestTransportHandle {
        TestTransportHandle { shared: alloc::rc::Rc::clone(&self.shared) }
    }

    /// Queue a message to be returned by the next `receive` call.
    pub fn queue_receive(&mut self, msg: Message, sts: IpcStatus) {
        *self.shared.next_receive.borrow_mut() = Some((msg, sts));
    }

    /// Return a snapshot of the recorded sends (oldest first).
    pub fn sent(&self) -> alloc::vec::Vec<(Endpoint, Message)> {
        self.shared.sent.borrow().clone()
    }

    /// Make `receive` return `Err(Unimplemented)` (matches old stub).
    pub fn set_should_fail(&mut self, v: bool) {
        self.shared.should_fail.set(v);
    }
}

/// Shared mutable state of [`TestIpcTransport`], owned by both the
/// transport and any [`TestTransportHandle`] clones.
#[cfg(test)]
struct TestTransportShared {
    next_receive: core::cell::RefCell<Option<(Message, IpcStatus)>>,
    sent: core::cell::RefCell<alloc::vec::Vec<(Endpoint, Message)>>,
    should_fail: core::cell::Cell<bool>,
}

/// Test-side handle to a [`TestIpcTransport`]'s state (V10-P0-2).
///
/// The test keeps a clone of this handle while the transport itself runs
/// inside `VmServer`; queueing a message before `run_once` and inspecting
/// the recorded replies afterwards exercises the real main-loop path.
#[derive(Clone)]
#[cfg(test)]
pub struct TestTransportHandle {
    shared: alloc::rc::Rc<TestTransportShared>,
}

#[cfg(test)]
impl TestTransportHandle {
    /// Queue a message to be returned by the next `receive` call.
    pub fn queue_receive(&self, msg: Message, sts: IpcStatus) {
        *self.shared.next_receive.borrow_mut() = Some((msg, sts));
    }

    /// Return a snapshot of the recorded sends (oldest first).
    pub fn sent(&self) -> alloc::vec::Vec<(Endpoint, Message)> {
        self.shared.sent.borrow().clone()
    }

    /// Make `receive` return `Err(Unimplemented)`.
    pub fn set_should_fail(&self, v: bool) {
        self.shared.should_fail.set(v);
    }
}

#[cfg(test)]
impl Default for TestIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl IpcTransport for TestIpcTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError> {
        if self.shared.should_fail.get() {
            return Err(IpcError::Unimplemented);
        }
        self.shared
            .next_receive
            .borrow_mut()
            .take()
            .ok_or(IpcError::Unimplemented)
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
        self.shared.sent.borrow_mut().push((dest, *msg));
        Ok(())
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Message;

    #[test]
    fn ipc_status_call_bits_match_minix3() {
        // C: IPC_STATUS_CALL(status) = (status >> 0) & 0x3F (ipcconst.h:16-18).
        assert!(IpcStatus { flags: 4 /* NOTIFY */ }.is_notify());
        assert!(!IpcStatus { flags: 3 /* SENDREC */ }.is_notify());
        assert!(!IpcStatus { flags: 0 }.is_notify());
        // C: IPC_STATUS_FLAGS_TEST(status, IPC_FLG_MSG_FROM_KERNEL)
        //    = ((status >> 16) & 1) != 0 (ipcconst.h:22-24).
        assert!(IpcStatus { flags: 1 << 16 }.is_from_kernel());
        assert!(!IpcStatus { flags: 0 }.is_from_kernel());
        // Notify + from-kernel can coexist (kernel notification).
        let both = IpcStatus { flags: 4 | (1 << 16) };
        assert!(both.is_notify());
        assert!(both.is_from_kernel());
    }

    #[test]
    fn kernel_transport_uninitialized_returns_unimplemented() {
        // Without explicit `mark_initialized`, the transport must
        // refuse to receive — distinct from the old opaque `Err(())`.
        let mut t = KernelIpcTransport::new();
        let r = t.receive();
        assert!(matches!(r, Err(IpcError::Unimplemented)));
    }

    #[test]
    fn kernel_transport_send_to_none_is_invalid() {
        // C: ipc_send to NONE endpoint is rejected by the kernel.
        let mut t = KernelIpcTransport::new();
        t.mark_initialized();
        let r = t.send(Endpoint::NONE, &Message::default());
        assert!(matches!(r, Err(IpcError::InvalidEndpoint)));
    }

    #[test]
    fn test_transport_empty_returns_unimplemented() {
        let mut t = TestIpcTransport::new();
        let r = t.receive();
        assert!(matches!(r, Err(IpcError::Unimplemented)));
    }

    #[test]
    fn test_transport_queue_then_receive() {
        let mut t = TestIpcTransport::new();
        let mut msg = Message::default();
        msg.m_type = 42;
        t.queue_receive(msg.clone(), IpcStatus::default());

        let (got_msg, _sts) = t.receive().expect("should return queued message");
        assert_eq!(got_msg.m_type, 42);
        // Second call sees empty queue.
        let r = t.receive();
        assert!(matches!(r, Err(IpcError::Unimplemented)));
    }

    #[test]
    fn test_transport_send_is_recorded() {
        let mut t = TestIpcTransport::new();
        let dest = Endpoint(7);
        let mut msg = Message::default();
        msg.m_type = 99;
        t.send(dest, &msg).expect("send should succeed in mock");

        let sent = t.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, dest);
        assert_eq!(sent[0].1.m_type, 99);
    }

    #[test]
    fn test_transport_should_fail_flag() {
        let mut t = TestIpcTransport::new();
        let mut msg = Message::default();
        msg.m_type = 1;
        t.queue_receive(msg, IpcStatus::default());
        t.set_should_fail(true);
        // Even with a queued message, should_fail forces Unimplemented.
        let r = t.receive();
        assert!(matches!(r, Err(IpcError::Unimplemented)));
    }
}
