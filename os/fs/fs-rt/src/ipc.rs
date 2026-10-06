//! IPC verbs behind the runtime, injected for tests.
//!
//! C correspondence: the four verbs every file server's event loop issues —
//! `ipc_receive` (`fsdriver.c:83` via `sef_receive`), the reply send
//! (`fsdriver.c:57` `ipc_send`), and the two grant copies the data adapters
//! use (`utility.c:52` `sys_safecopyto`, and its `from` twin). Production
//! wires [`SysRtIpc`] onto the kernel trap transports; tests script
//! [`ScriptedRtIpc`].

use alloc::vec::Vec;

use minix_sef::SefEvent;
use minix_types::Message;

/// What one receive delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receipt {
    /// An ordinary message, delivered into the caller's buffer. This is
    /// both user requests and the restart server's init request — the
    /// birth face tells them apart by type and sender (C sef.c: the init
    /// request arrives as a plain message from RS, sef.h:33-34). The
    /// status word rides along: the notification marker the loop layer
    /// classifies with lives in it (C `ipc_status`).
    Call {
        /// Decoded status word from the receive.
        status: i32,
    },
    /// A SYSTEM notification: the kernel's signal request.
    ///
    /// The notification carries no payload here on purpose: C's dispatcher
    /// walks the pending bitmap and calls the registered handler once per
    /// signal number (`do_sef_signal_request`, `sef_signal.c:94-113`), and
    /// `minix-sef` already did that walk before this receipt was built — the
    /// signal numbers reached the hook through [`RtIpc::receive`]'s callback
    /// argument. Handing the raw bitmap up a second time would ask each file
    /// server to re-implement a walk the library owns.
    Signal,
}

/// The four verbs the runtime needs, none of them server-specific.
pub trait RtIpc {
    /// Blocking receive; fills `msg` and reports what kind of delivery it
    /// was. C: `ipc_receive(src, &m, &status)` under the SEF loop
    /// (`sef_receive_status`, sef.c:149-260 — pings answered and swallowed
    /// inside).
    ///
    /// `on_signal` is the runtime's signal hook, handed down to the SEF loop
    /// so it can be called with one signal number at a time — the shape C
    /// gives the registered `sef_cb_signal_handler` (`sef.h:269`).
    fn receive(
        &mut self,
        src: minix_types::Endpoint,
        msg: &mut Message,
        on_signal: &mut dyn FnMut(i32),
        cancel: &minix_sef::SefCancel,
    ) -> Result<Receipt, i32>;
    /// Non-blocking send of a reply or a birth report. C: `ipc_send`
    /// (`fsdriver.c:57`) — the caller sits in `sendrec` waiting, so the
    /// non-blocking form has the same observable effect and cannot deadlock
    /// the single-threaded loop.
    fn send(&mut self, dest: minix_types::Endpoint, msg: &Message) -> Result<(), i32>;
    /// Copy bytes in through a grant: read `buf.len()` bytes at `offset`
    /// from `granter`'s granted window. C: `sys_safecopyfrom`.
    fn copy_from(
        &mut self,
        granter: minix_types::Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32>;
    /// Copy bytes out through a grant: write `bytes` at `offset` of
    /// `granter`'s granted window. C: `sys_safecopyto` (`utility.c:52`).
    fn copy_to(
        &mut self,
        granter: minix_types::Endpoint,
        grant: i32,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), i32>;
}

/// Production wiring: everything goes through the kernel trap transports.
///
/// The receive leg goes through `minix-sef`'s loop so ping interception and
/// notification classification behave identically on every server.
pub struct SysRtIpc;

impl RtIpc for SysRtIpc {
    fn receive(
        &mut self,
        src: minix_types::Endpoint,
        msg: &mut Message,
        on_signal: &mut dyn FnMut(i32),
        cancel: &minix_sef::SefCancel,
    ) -> Result<Receipt, i32> {
        let mut sef = SeIpcAdapter;
        // The library's entry point monomorphizes its callback (`impl FnMut`),
        // while this trait is object-safe and can only carry the unsized
        // `dyn FnMut` — a thin closure bridges the two (IS 的同款接线，
        // `os/servers/is/src/sef.rs`).
        let mut handoff = |signo: i32| on_signal(signo);
        let recv = minix_sef::sef_receive_status(&mut sef, src, msg, &mut handoff, cancel);
        match recv {
            // The signal arm is only reached after `minix-sef` has
            // established `is_ipc_notify(status) && source == SYSTEM`, and
            // that call has already handed every pending signal number to
            // `on_signal` — there is nothing left for the loop to decode.
            Ok(r) => match r.event {
                SefEvent::Signal(_) => Ok(Receipt::Signal),
                _ => Ok(Receipt::Call { status: r.status }),
            },
            Err(e) => Err(e),
        }
    }

    fn send(&mut self, dest: minix_types::Endpoint, msg: &Message) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .sendnb(dest, msg)
            .map_err(|t| t.0)
    }

    fn copy_from(
        &mut self,
        granter: minix_types::Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        minix_sys::syscall::sys_safecopyfrom(
            &minix_sys::syscall::DirectKernelCallTransport,
            granter.get(),
            grant,
            offset,
            buf.as_ptr() as u64,
            buf.len() as u64,
        )
    }

    fn copy_to(
        &mut self,
        granter: minix_types::Endpoint,
        grant: i32,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), i32> {
        minix_sys::syscall::sys_safecopyto(
            &minix_sys::syscall::DirectKernelCallTransport,
            granter.get(),
            grant,
            offset,
            bytes.as_ptr() as u64,
            bytes.len() as u64,
        )
    }
}

/// `minix-sef`'s receive-side adapter over the trap transport.
struct SeIpcAdapter;

impl minix_sef::SefIpc for SeIpcAdapter {
    fn receive(&mut self, src: minix_types::Endpoint, msg: &mut Message) -> Result<i32, i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport
            .receive(src, msg)
            .map(|status| status.0 as i32)
            .map_err(|t| t.0)
    }

    fn notify(&mut self, dest: minix_types::Endpoint) -> Result<(), i32> {
        use minix_sys::ipc::IpcTransport;
        minix_sys::ipc::DirectTrapTransport.notify(dest).map_err(|t| t.0)
    }
}

/// Scripted transport for hosted tests: every verb reads from a recorded
/// transcript the test builds up front.
#[derive(Default)]
pub struct ScriptedRtIpc {
    /// Deliveries handed out by `receive` in order; past the end the last
    /// one repeats (short scripts drive long loops).
    pub inbox: Vec<(Receipt, Message)>,
    pub received_at: usize,
    /// `(destination, message)` pairs recorded by `send`.
    pub sent: Vec<(minix_types::Endpoint, Message)>,
    /// Recorded grant reads: `(granter, grant, offset, length)`.
    pub reads: Vec<(minix_types::Endpoint, i32, u64, usize)>,
    /// Recorded grant writes: `(granter, grant, offset, length)`.
    pub writes: Vec<(minix_types::Endpoint, i32, u64, usize)>,
    /// Bytes handed back by `copy_from` (the test stages grant data here).
    pub grant_data: Vec<u8>,
}

impl ScriptedRtIpc {
    /// A transport delivering `deliveries` in order.
    pub fn new(deliveries: Vec<(Receipt, Message)>) -> Self {
        Self::staging(deliveries, Vec::new())
    }

    /// A transport that also stages the bytes grant reads return.
    pub fn staging(deliveries: Vec<(Receipt, Message)>, grant_data: Vec<u8>) -> Self {
        Self {
            inbox: deliveries,
            received_at: 0,
            sent: Vec::new(),
            reads: Vec::new(),
            writes: Vec::new(),
            grant_data,
        }
    }
}

impl RtIpc for ScriptedRtIpc {
    fn receive(
        &mut self,
        _src: minix_types::Endpoint,
        msg: &mut Message,
        on_signal: &mut dyn FnMut(i32),
        cancel: &minix_sef::SefCancel,
    ) -> Result<Receipt, i32> {
        let last = self.inbox.len().saturating_sub(1);
        let (receipt, m) = self.inbox[self.received_at.min(last)];
        self.received_at += 1;
        *msg = m;
        if matches!(receipt, Receipt::Signal) {
            // Same walk the library runs on a real SYSTEM notification, so
            // the scripted loop cannot drift from production shape. Same
            // escape hatch too (PD-27): a hook that cancels turns this
            // delivery into `Err(EINTR)` instead of a receipt, exactly what
            // `sef_receive_status` does after a cancelled callback.
            minix_sef::dispatch_kernel_notify(msg.notify_sigset(), on_signal);
            if cancel.is_cancelled() {
                return Err(minix_types::EINTR);
            }
        }
        Ok(receipt)
    }

    fn send(&mut self, dest: minix_types::Endpoint, msg: &Message) -> Result<(), i32> {
        self.sent.push((dest, *msg));
        Ok(())
    }

    fn copy_from(
        &mut self,
        granter: minix_types::Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32> {
        self.reads.push((granter, grant, offset, buf.len()));
        for (i, b) in buf.iter_mut().enumerate() {
            *b = self.grant_data.get(offset as usize + i).copied().unwrap_or(0);
        }
        Ok(())
    }

    fn copy_to(
        &mut self,
        granter: minix_types::Endpoint,
        grant: i32,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), i32> {
        self.writes.push((granter, grant, offset, bytes.len()));
        self.grant_data.extend_from_slice(bytes);
        Ok(())
    }
}
