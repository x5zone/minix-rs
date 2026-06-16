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
//!   Selected via `cfg(test)` or by explicit `VmServer::run_with()`
//!   parameterization. Lets unit tests drive the main loop without a
//!   live kernel.
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

/// Errors that can occur during IPC transport operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcError {
    /// The transport is not connected (test mode) or kernel IPC is
    /// not yet implemented (production mode, before kernel IPC core lands).
    Unimplemented,
    /// Destination endpoint is invalid (`NONE` or out of range).
    InvalidEndpoint,
    /// Send queue is full.
    WouldBlock,
    /// The kernel returned a generic error (carries the raw code).
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

    /// Mark the transport as initialized. Called from `VmServer::init()`
    /// after the kernel IPC channel is ready (today: a no-op; once
    /// kernel IPC core lands, this will also seed any kernel-side state).
    pub fn mark_initialized(&mut self) {
        self.initialized = true;
    }
}

impl Default for KernelIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for KernelIpcTransport {
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

// ── Test impl: in-memory mock ──

/// Test-only IPC transport. Holds a single pre-loaded message that
/// `receive` returns, and records every `send` so tests can inspect
/// what the main loop would have replied.
///
/// This impl is selected automatically in `#[cfg(test)]` builds via
/// the `ipc_transport_for_test()` constructor.
pub struct TestIpcTransport {
    /// The next message `receive` will return.
    next_receive: Option<(Message, IpcStatus)>,
    /// All `send` calls recorded for inspection.
    sent: alloc::vec::Vec<(Endpoint, Message)>,
    /// If `true`, `receive` returns `Err(Unimplemented)` like the old
    /// stub. Default `false` so tests can opt into the panic.
    should_fail: bool,
}

impl TestIpcTransport {
    pub fn new() -> Self {
        Self {
            next_receive: None,
            sent: alloc::vec::Vec::new(),
            should_fail: false,
        }
    }

    /// Queue a message to be returned by the next `receive` call.
    pub fn queue_receive(&mut self, msg: Message, sts: IpcStatus) {
        self.next_receive = Some((msg, sts));
    }

    /// Return the list of recorded sends (oldest first).
    pub fn sent(&self) -> &[(Endpoint, Message)] {
        &self.sent
    }

    /// Make `receive` return `Err(Unimplemented)` (matches old stub).
    pub fn set_should_fail(&mut self, v: bool) {
        self.should_fail = v;
    }
}

impl Default for TestIpcTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcTransport for TestIpcTransport {
    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError> {
        if self.should_fail {
            return Err(IpcError::Unimplemented);
        }
        self.next_receive
            .take()
            .ok_or(IpcError::Unimplemented)
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
        self.sent.push((dest, msg.clone()));
        Ok(())
    }
}

// ── Selector ──

/// Select the appropriate IPC transport for the current build.
///
/// In `#[cfg(test)]` builds, returns a [`TestIpcTransport`] so unit
/// tests can drive the main loop. In production builds, returns a
/// [`KernelIpcTransport`] (which will panic until kernel IPC core lands).
#[cfg(test)]
pub fn ipc_transport_for_build() -> TestIpcTransport {
    TestIpcTransport::new()
}

#[cfg(not(test))]
pub fn ipc_transport_for_build() -> KernelIpcTransport {
    KernelIpcTransport::new()
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use minix_types::Message;

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
