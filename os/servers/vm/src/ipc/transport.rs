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
//! - [`KernelIpcTransport`] — production impl delegating to the minix-sys
//!   trap backend (`DirectTrapTransport`). The call shape is final; until
//!   the 64-bit trap wiring lands (edge E1, stage plan A-6) the backend
//!   answers `EIO` and the main loop's dropped-message path handles it —
//!   no panic, no `unimplemented!()` (V11/T9).
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
// 匿名导入 minix-sys 的同名 trait：方法解析需要它，且不与本文件的
// `IpcTransport` 冲突（VM 侧策略 trait 才是对外语义）。
use minix_sys::ipc::IpcTransport as _;

/// IPC receive status word, mirroring the `rcv_sts` output parameter of
/// Minix3's `sef_receive_status()`. The kernel fills in the flags describing
/// the message (e.g. notification vs. regular IPC, sender is kernel, etc.).
///
/// The Rust side consumes `is_notify()` (main loop skip, V10-P1-1) and
/// `is_from_kernel()` (pagefault audit branch); further `IPC_STATUS_*`
/// bits join as consumers appear.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct IpcStatus {
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
    /// `KernelIpcTransport::receive` fills `flags` from the minix-sys trap
    /// status word (V11/T9); the pre-E1 trap stub answers `flags == 0`, so
    /// it reads `false` until the real trap wiring lands (edge E1).
    pub fn is_from_kernel(&self) -> bool {
        ((self.flags >> 16) & 1) != 0
    }
}

/// Errors that can occur during IPC transport operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IpcError {
    /// The transport is not connected (test mode) or kernel IPC is
    /// not yet implemented (production mode, before kernel IPC core lands).
    Unimplemented,
    /// Destination endpoint is invalid (`NONE` or out of range).
    InvalidEndpoint,
    /// Send queue is full / non-blocking receive had nothing.
    // C's IPC has a non-blocking receive mode (EWOULDBLOCK); a producer
    // appears when the trap layer (edge E1) surfaces it.
    #[allow(dead_code)]
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
///    real (trap-backed, V11/T9) impl otherwise. Two distinct
///    implementations (kernel vs. test) justify the trait abstraction
///    per the "trait quality" guideline.
pub(crate) trait IpcTransport {
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

    /// Notify a destination. Mirrors C `ipc_notify(dest)` — used by the
    /// SEF ping-pong (`sef_ping.c:61`). Default honest failure: only
    /// transports that can actually notify override it.
    fn notify(&mut self, _dest: Endpoint) -> Result<(), IpcError> {
        Err(IpcError::Unimplemented)
    }
}

// ── Production impl: kernel IPC ──

/// Production IPC transport. Delegates to the minix-sys user-space trap
/// backend to invoke the kernel's IPC primitives.
///
/// **Status (2026-09-06, V11/T9)**: the delegation shape is final —
/// `receive` is `sef_receive_status(ANY, …)` and `send` is `ipc_send`,
/// both through `DirectTrapTransport`. The kernel-side IPC core is real
/// (`os/kernel/src/ipc.rs`); what is pending is the user-space trap
/// instruction sequences themselves (edge E1). Until E1 lands the backend
/// answers `EIO`, which flows through `IpcError::Kernel` into the main
/// loop's dropped-message accounting — the failure mode is observable,
/// graceful, and identical to a slow/broken kernel link.
pub(crate) struct KernelIpcTransport {
    /// Tracks whether the transport has been initialized. The C side
    /// does this implicitly via SEF startup; we expose it explicitly
    /// so tests can assert against it.
    initialized: bool,
    /// 用户态 trap 后端（minix-sys）。`DirectTrapTransport` 的方法体在
    /// 64 位 trap wiring 落地（edge E1，stage plan A-6）前返回 EIO——
    /// 本类型已经按真实调用形态委托，E1 落地时无需再改这里。
    inner: minix_sys::ipc::DirectTrapTransport,
}

impl KernelIpcTransport {
    pub const fn new() -> Self {
        Self {
            initialized: false,
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
    fn mark_initialized(&mut self) {
        // Today this only flips the readiness flag; once kernel IPC core
        // lands it will also seed any kernel-side state (SEF startup).
        self.initialized = true;
    }

    fn receive(&mut self) -> Result<(Message, IpcStatus), IpcError> {
        // C: sef_receive_status(ANY, &msg, &rcv_sts) — libsys.so.
        // Delegates to the minix-sys trap backend (V11/T9): the call shape
        // is final; until edge E1 lands the backend answers EIO and the
        // main loop's dropped-message path handles it (no panic).
        if !self.initialized {
            return Err(IpcError::Unimplemented);
        }
        let mut msg = Message::default();
        match self.inner.receive(Endpoint::ANY, &mut msg) {
            // minix-sys `IpcStatus(u32)` is the raw kernel status word —
            // identical to the `flags` this type models (V10-P1-1 bit
            // decoding: NOTIFY = low 6 bits, FROM_KERNEL = bit 16).
            Ok(sts) => Ok((msg, IpcStatus { flags: sts.0 })),
            Err(trap) => Err(IpcError::Kernel(trap.0)),
        }
    }

    fn send(&mut self, dest: Endpoint, msg: &Message) -> Result<(), IpcError> {
        if dest == Endpoint::NONE {
            return Err(IpcError::InvalidEndpoint);
        }
        // C: ipc_send(dest, &msg) — libsys.so (same delegation as receive).
        if !self.initialized {
            return Err(IpcError::Unimplemented);
        }
        self.inner.send(dest, msg).map_err(|trap| IpcError::Kernel(trap.0))
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), IpcError> {
        if !self.initialized {
            return Err(IpcError::Unimplemented);
        }
        self.inner.notify(dest).map_err(|trap| IpcError::Kernel(trap.0))
    }
}

// ── SEF classification adapter (V14-P2-1 plan A) ──

/// Presents any [`IpcTransport`] as the `minix_sef::SefIpc` verb pair so
/// the shared `minix_sef::sef_receive_status` classifier can front the VM
/// main loop. VM is minix-sef's fourth consumer (IS/MIB/devman precede) —
/// one C protocol (`libsys sef.c`), one Rust classification.
///
/// errno mapping for transport failures mirrors C `ipc_receive`'s return
/// convention (raw errno): the trap backend's codes pass through, the
/// local [`IpcError`] variants map to their Minix3 errno values.
pub(crate) struct SefAdapter<'a>(pub(crate) &'a mut dyn IpcTransport);

impl minix_sef::SefIpc for SefAdapter<'_> {
    fn receive(&mut self, _src: Endpoint, msg: &mut Message) -> Result<i32, i32> {
        self.0.receive().map(|(m, sts)| {
            *msg = m;
            sts.flags as i32
        })
        .map_err(|e| match e {
            IpcError::Kernel(code) => code,
            IpcError::InvalidEndpoint => minix_types::EINVAL,
            IpcError::WouldBlock => minix_types::EAGAIN,
            IpcError::Unimplemented => minix_types::EIO,
        })
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), i32> {
        self.0.notify(dest).map_err(|e| match e {
            IpcError::Kernel(code) => code,
            IpcError::InvalidEndpoint => minix_types::EINVAL,
            IpcError::WouldBlock => minix_types::EAGAIN,
            IpcError::Unimplemented => minix_types::EIO,
        })
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
pub(crate) struct TestIpcTransport {
    shared: alloc::rc::Rc<TestTransportShared>,
}

#[cfg(test)]
impl TestIpcTransport {
    pub fn new() -> Self {
        Self {
            shared: alloc::rc::Rc::new(TestTransportShared {
                next_receive: core::cell::RefCell::new(
                    alloc::collections::VecDeque::new(),
                ),
                sent: core::cell::RefCell::new(alloc::vec::Vec::new()),
                notifies: core::cell::RefCell::new(alloc::vec::Vec::new()),
                should_fail: core::cell::Cell::new(false),
            }),
        }
    }

    /// A handle to this transport's state, kept by the test after the
    /// transport is moved into `VmServer` (V10-P0-2).
    pub fn handle(&self) -> TestTransportHandle {
        TestTransportHandle { shared: alloc::rc::Rc::clone(&self.shared) }
    }

    /// Queue a message to be returned by the next `receive` call (FIFO).
    pub fn queue_receive(&mut self, msg: Message, sts: IpcStatus) {
        self.shared.next_receive.borrow_mut().push_back((msg, sts));
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
    /// FIFO receive queue (V14-P2-1: a ping-pong round consumes two
    /// slots — the intercepted ping plus the follow-up message).
    next_receive: core::cell::RefCell<alloc::collections::VecDeque<(Message, IpcStatus)>>,
    sent: core::cell::RefCell<alloc::vec::Vec<(Endpoint, Message)>>,
    /// Recorded notify destinations (SEF ping-pong pongs).
    notifies: core::cell::RefCell<alloc::vec::Vec<Endpoint>>,
    should_fail: core::cell::Cell<bool>,
}

/// Test-side handle to a [`TestIpcTransport`]'s state (V10-P0-2).
///
/// The test keeps a clone of this handle while the transport itself runs
/// inside `VmServer`; queueing a message before `run_once` and inspecting
/// the recorded replies afterwards exercises the real main-loop path.
#[derive(Clone)]
#[cfg(test)]
pub(crate) struct TestTransportHandle {
    shared: alloc::rc::Rc<TestTransportShared>,
}

#[cfg(test)]
impl TestTransportHandle {
    /// Queue a message to be returned by the next `receive` call (FIFO).
    pub fn queue_receive(&self, msg: Message, sts: IpcStatus) {
        self.shared.next_receive.borrow_mut().push_back((msg, sts));
    }

    /// Destinations recorded by `notify` (SEF ping-pong pongs).
    pub fn notified(&self) -> alloc::vec::Vec<Endpoint> {
        self.shared.notifies.borrow().clone()
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
            .pop_front()
            .ok_or(IpcError::Unimplemented)
    }

    fn notify(&mut self, dest: Endpoint) -> Result<(), IpcError> {
        self.shared.notifies.borrow_mut().push(dest);
        Ok(())
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
