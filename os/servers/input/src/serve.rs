//! Transport seam + serve loop — the E-INWIRE loop shell.
//!
//! C ground truth: the receive-classify-dispatch tail of `chardriver_task`
//! (`lib/libchardriver/chardriver.c:455-573`) plus `input_other`
//! (`servers/input/input.c:608-641`) plus the reply helpers
//! (`chardriver.c:127-174`).
//!
//! Loop shell policy (E-INWIRE): written directly against `minix-sys`
//! receive/send/senda. **minix-sef switch point**: when input gains SEF
//! lifecycle handling, the transport's `receive` swaps to
//! `minix_sef::sef_receive_status` (RS ping absorption) — the IS/MIB
//! precedent — and nothing else in this module changes.
//!
//! The transport aggregates BOTH hardware legs (a C process owns them
//! simultaneously): the IPC leg (`DirectTrapTransport`) for
//! receive/send/asynsend, and the SYSCALL leg (`DirectKernelCallTransport`)
//! for grant writes and the DS client. `KernelTransport` implements both
//! traits by delegation — the Rust counterpart of "one C process".
//!
//! Announce: DS publishes `drv.chr.input` at startup (C: chardriver
//! announce, `chardriver.c:99`), `sys_statectl CLEAR_IPC_REFS` drops the
//! previous generation's callers, and `TTY_INPUT_UP` goes blocking to the
//! terminal driver (`input.c:672-677`).

use crate::dispatcher::{complete_grant_copy, handle_arrival, Arrival, Outcome};
use crate::effects::Effect;
use minix_sys::ds::DsClient;
use minix_sys::ipc::{AsyncSlot, AsyncSlotFlags, DirectTrapTransport, IpcTransport as _};
use minix_sys::syscall::{
    sys_safecopyto, DirectKernelCallTransport, KernelCallTransport as _,
};
use minix_types::{decode_input_event, decode_setleds, Endpoint, Message, INPUT_EVENT, INPUT_SETLEDS};
use alloc::vec::Vec;

/// CDEV reply message types. C: `com.h:935-937` —
/// `CDEV_REPLY = CDEV_RS_BASE(0x480)`、`CDEV_SEL2_REPLY = CDEV_RS_BASE + 2`.
pub const CDEV_REPLY: i32 = 0x480;
pub const CDEV_SEL2_REPLY: i32 = 0x482;

/// The verbs the serve loop needs from the message transport.
pub trait Transport {
    /// Blocking receive from any source. C: `receive(ANY, &m)`.
    fn receive(&mut self, msg: &mut Message) -> Result<(), i32>;
    /// Blocking send (terminal-facing effects). C: `send`.
    fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32>;
    /// Non-blocking `asynsend3(AMF_NOREPLY)` (reader replies). C:
    /// `chardriver_reply_task:146`.
    fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32>;
    /// Grant write: copy `bytes` into the reader's grant. C:
    /// `sys_safecopyto` in the read path (`input.c:144-151`).
    fn write_grant(&mut self, granter: Endpoint, grant: i32, bytes: &[u8]) -> Result<(), i32>;
    /// DS publish `drv.chr.<label>` (C: chardriver announce).
    fn publish_label(&mut self, name: &str) -> Result<(), i32>;
}

/// The production transport: both hardware legs, aggregated.
///
/// - `ipc` (DirectTrapTransport): the int-33 IPC leg — receive/send/
///   asynsend of this module's [`Transport`] surface.
/// - `kernel` (DirectKernelCallTransport): the SYSCALL leg — grant writes
///   and the DS client's kernel-call half.
///
/// **minix-sef switch point**: the `receive` delegation swaps to
/// `minix_sef::sef_receive_status` when input gains SEF lifecycle handling
/// (the IS/MIB precedent) — callers are unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct KernelTransport {
    pub ipc: DirectTrapTransport,
    pub kernel: DirectKernelCallTransport,
}

impl Transport for KernelTransport {
    fn receive(&mut self, msg: &mut Message) -> Result<(), i32> {
        // minix-sef switch point (see the struct docs).
        minix_sys::receive(Endpoint::ANY, msg).map_err(|_| -minix_types::EIO)
    }

    fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
        minix_sys::send(dst, msg).map_err(|_| -minix_types::EIO)
    }

    fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
        // C: `asynsend3(endpt, &m, AMF_NOREPLY)` — a one-slot SENDA table
        // with the no-reply flag set (`ipc.rs:202`, flag value 8).
        let slot = AsyncSlot {
            flags: AsyncSlotFlags(AsyncSlotFlags::VALID.0 | AsyncSlotFlags::NO_REPLY.0),
            destination: dst,
            result: 0,
            message: *msg,
        };
        self.ipc.senda(&[slot]).map_err(|_| -minix_types::EIO)
    }

    fn write_grant(&mut self, granter: Endpoint, grant: i32, bytes: &[u8]) -> Result<(), i32> {
        sys_safecopyto(&self.kernel, granter.get(), grant, 0, bytes.as_ptr() as u64, bytes.len() as u64)
    }

    fn publish_label(&mut self, name: &str) -> Result<(), i32> {
        // DsClient consumes both legs by value; both carriers are zero
        // sized, so re-constructing the aggregate is free.
        let mut ds = DsClient::new(*self, Endpoint::DS);
        ds.publish_label(name, Endpoint::NONE, minix_types::DsFlags::empty())
    }
}

impl minix_sys::ipc::IpcTransport for KernelTransport {
    fn send(&self, destination: Endpoint, message: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
        self.ipc.send(destination, message)
    }
    fn receive(&self, source: Endpoint, message: &mut Message) -> Result<minix_sys::ipc::IpcStatus, minix_sys::ipc::TrapStatus> {
        self.ipc.receive(source, message)
    }
    fn sendrec(&self, destination: Endpoint, message: &mut Message) -> Result<(), minix_sys::ipc::TrapStatus> {
        self.ipc.sendrec(destination, message)
    }
    fn notify(&self, destination: Endpoint) -> Result<(), minix_sys::ipc::TrapStatus> {
        self.ipc.notify(destination)
    }
    fn sendnb(&self, destination: Endpoint, message: &Message) -> Result<(), minix_sys::ipc::TrapStatus> {
        self.ipc.sendnb(destination, message)
    }
    fn senda(&self, table: &[AsyncSlot]) -> Result<(), minix_sys::ipc::TrapStatus> {
        self.ipc.senda(table)
    }
    fn query_kerninfo_page(&self) -> Result<u64, minix_sys::ipc::TrapStatus> {
        self.ipc.query_kerninfo_page()
    }
}

impl minix_sys::syscall::KernelCallTransport for KernelTransport {
    fn kernel_call(&self, message: &mut Message) -> i32 {
        self.kernel.kernel_call(message)
    }
}

/// Classify a raw message into an [`Arrival`].
///
/// Notifications (source in the low byte — C `is_notify`) are all
/// `DriverStoreChanged` prompts: DS publish/subscribe and driver death both
/// surface this way. Unknown types return `None`; the loop logs nothing and
/// drops them (C "unexpected message" fall-through, fail-closed A-10).
pub fn classify(msg: &Message) -> Option<Arrival> {
    if is_notify(msg.m_type) {
        return Some(Arrival::DriverStoreChanged);
    }
    match msg.m_type {
        INPUT_EVENT => decode_input_event(msg).map(
            |(id, page, code, value, flags)| Arrival::DriverReport {
                source: msg.m_source,
                id,
                page,
                code,
                value,
                flags,
            },
        ),
        INPUT_SETLEDS => decode_setleds(msg)
            .map(|mask| Arrival::TerminalSetleds { source: msg.m_source, mask }),
        _ => None,
    }
}

/// The notify family shares the 0x1000 base with the source in the low
/// byte (C `is_notify`).
fn is_notify(m_type: i32) -> bool {
    (m_type & !0xff) == 0x1000
}

/// Builds the `CDEV_REPLY` message. C: `chardriver_reply_task`
/// (`chardriver.c:142-144`) — `status`@8, `id`@12 inside
/// `mess_lchardriver_vfs_reply` (56 bytes).
fn reply_task_msg(self_ep: Endpoint, request_id: u32, status: i32) -> Message {
    let mut msg = Message::default();
    msg.m_type = CDEV_REPLY;
    msg.m_source = self_ep;
    // Payload bytes at the union start: status (i32) then id (u32) —
    // `mess_lchardriver_vfs_reply` (ipc.h:943-948). The typed arm lands
    // with E-MINTYPES-RUNTIME; the offsets are pinned by C ipc.h.
    unsafe {
        let base = core::ptr::addr_of_mut!(msg) as *mut u8;
        core::ptr::copy_nonoverlapping(status.to_le_bytes().as_ptr(), base.add(8), 4);
        core::ptr::copy_nonoverlapping(request_id.to_le_bytes().as_ptr(), base.add(12), 4);
    }
    msg
}

/// Builds the `CDEV_SEL2_REPLY` message. C: `chardriver_reply_select` —
/// `status`@8 (OK), `minor`@12.
fn reply_select_msg(self_ep: Endpoint, minor: i32) -> Message {
    let mut msg = Message::default();
    msg.m_type = CDEV_SEL2_REPLY;
    msg.m_source = self_ep;
    unsafe {
        let base = core::ptr::addr_of_mut!(msg) as *mut u8;
        core::ptr::copy_nonoverlapping(0i32.to_le_bytes().as_ptr(), base.add(8), 4);
        core::ptr::copy_nonoverlapping(minor.to_le_bytes().as_ptr(), base.add(12), 4);
    }
    msg
}

/// Stamp the server's own endpoint and send asynsend-style.
fn send_from(t: &mut dyn Transport, dst: Endpoint, message: &mut Message, self_ep: Endpoint) {
    message.m_source = self_ep;
    let _ = t.asynsend(dst, message);
}

/// Performs one effect against the transport. Every outgoing message is
/// stamped with the server's own endpoint first (`m_source` back-fill).
pub fn perform(effect: &Effect, t: &mut dyn Transport, self_ep: Endpoint) {
    match effect {
        Effect::ReplyTask {
            caller,
            request_id,
            value,
        } => {
            let status = match value {
                crate::effects::ReplyValue::Bytes(count) => count.0 as i32,
                crate::effects::ReplyValue::Code(code) => *code,
            };
            let mut msg = reply_task_msg(self_ep, *request_id, status);
            let _ = t.asynsend(*caller, &mut msg);
        }
        Effect::ReplySelect { selector, minor } => {
            let mut msg = reply_select_msg(self_ep, minor.0 as i32);
            let _ = t.asynsend(*selector, &mut msg);
        }
        Effect::SendDriverAsync { to, message } => {
            let mut m = *message;
            m.m_source = self_ep;
            let _ = t.asynsend(*to, &mut m);
        }
        Effect::SendTerminalBlocking { message } => {
            let mut m = *message;
            m.m_source = self_ep;
            let _ = t.send(Endpoint::TTY, &mut m);
        }
    }
}

fn perform_all(effects: &[Effect], t: &mut dyn Transport, self_ep: Endpoint) {
    for effect in effects {
        perform(effect, t, self_ep);
    }
}

/// Drives the server: receive → classify → dispatch → perform effects.
///
/// `self_ep` is the server's own endpoint (stamped into every outgoing
/// message's `m_source`). The DS announce (`drv.chr.input`) and the
/// blocking `TTY_INPUT_UP` handshake run before the first receive — wire
/// them here when the startup sequencing lands with E-ISWIRE(3).
pub fn serve(t: &mut dyn Transport, self_ep: Endpoint, server: &mut crate::dispatcher::Server) {
    loop {
        let mut msg = Message::default();
        if t.receive(&mut msg).is_err() {
            continue; // receive failure: retry (C logs and re-enters)
        }
        let outcome = match classify(&msg) {
            Some(arrival) => handle_arrival(server, arrival),
            None => continue,
        };
        match outcome {
            Outcome::Done(effects) => perform_all(&effects, t, self_ep),
            Outcome::GrantCopy(grant_copy) => {
                // Move the planned events through the reader's grant, then
                // let the completion decide commit vs. discard.
                let device = &server.table.devices[grant_copy.slot.0];
                let bytes = crate::eventbuf::drain_ordered(
                    &device.events,
                    device.tail,
                    grant_copy.plan.plan.first_len + grant_copy.plan.plan.second_len,
                );
                let mut copied: Vec<u8> = Vec::new();
                for event in &bytes {
                    // SAFETY: `InputEvent` is 24-byte repr(C) POD; the byte
                    // image is what the reader's grant expects.
                    unsafe {
                        let base = core::ptr::addr_of!(event) as *const u8;
                        for offset in 0..24 {
                            copied.push(core::ptr::read(base.add(offset)));
                        }
                    }
                }
                let transported = t
                    .write_grant(grant_copy.caller, grant_copy.grant, &copied)
                    .map_err(|_| crate::error::InputError::DeviceNotActive);
                let effects = complete_grant_copy(server, grant_copy, transported);
                perform_all(&effects, t, self_ep);
            }
        }
    }
}

#[cfg(test)]
mod serve_tests {
    use super::*;
    use minix_types::input_event_msg;

    /// 分类：INPUT_EVENT → DriverReport（m_source 自消息回填）。
    #[test]
    fn test_classify_input_event() {
        let mut msg = input_event_msg(3, 1, 30, 2, 1);
        msg.m_source = Endpoint(9);
        match classify(&msg) {
            Some(Arrival::DriverReport {
                source,
                id,
                page,
                code,
                value,
                flags,
            }) => {
                assert_eq!((source.0, id, page, code, value, flags), (9, 3, 1, 30, 2, 1));
            }
            other => panic!("expected DriverReport, got {other:?}"),
        }
    }

    /// 通知家族（0x1000 基 + 低字节源）→ DriverStoreChanged 提示。
    #[test]
    fn test_classify_notify_is_store_changed() {
        let mut msg = Message::default();
        msg.m_type = 0x1000 + 7;
        assert!(matches!(
            classify(&msg),
            Some(Arrival::DriverStoreChanged)
        ));
    }
}