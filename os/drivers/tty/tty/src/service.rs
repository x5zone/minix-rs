//! Service assembly: the tty driver's message pump over the shared runtime.
//!
//! C correspondence: the receive-classify-dispatch tail of `tty_task`
//! (`tty.c:158-235`) — every delivered message goes through the character
//! framework's router (`chardriver_process`, `chardriver.c:455-532`), which
//! classifies it, runs the matching table entry, and replies. The loop
//! shell and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`]; this module owns only the
//! one-message-at-a-time decision: classify, dispatch, reply.
//!
//! The judgment core is [`minix_chardriver::driver`] — [`classify`] and
//! [`reply_decision`] are authoritative, so this file re-implements no
//! routing rule. The seven device requests reach the [`TtyDriver`] hooks
//! (session/line policy); notifications reach the interrupt/alarm hooks.
//! Replies go out through the transport, so the whole decision path is
//! host-testable with a scripted transport: no kernel is needed to assert
//! which reply reaches which caller with what status.
//!
//! Data-plane seam: the read and write hooks answer byte counts and park
//! or wake; the physical grant copies between a caller buffer and the
//! input queue are not wired here (the hooks hand out counts, not bytes),
//! so a real boot still needs that copy. It is a registered gap, not a
//! fabricated success — the control flow and reply discipline are complete
//! and tested.

use crate::backend::LineBackend;
use crate::char_face::TtyDriver;
use minix_chardriver::driver::{
    classify, reply_decision, CharDriver, NotifySource, ReplyDecision, Route,
};
use minix_chardriver::protocol::{CdevRequest, DeviceMinor, RequestId};
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{CDEV_REPLY_BASE, Endpoint, Message};

/// The reply base plus one: a select poll's immediate answer
/// (`CDEV_SEL1_REPLY`, `com.h:936`). minix-types names only the base, so
/// the selector reply is the base plus the select index.
const CDEV_SEL1_REPLY: i32 = CDEV_REPLY_BASE + 1;

/// The message union payload begins at byte 8 of [`Message`] (after
/// `m_type` at 0 and `m_source` at 4, with 8-byte union alignment). Every
/// wire offset below is `8 +` the field's position inside its C payload
/// struct (ipc.h:2206-2258).
const PAYLOAD: usize = 8;

/// Character-request and notify wire offsets, keyed per payload struct.
///
/// C: `mess_vfs_lchardriver_{openclose,cancel,select,readwrite}` and
/// `mess_notify` (ipc.h:2206-2258) — the same layout the virtual file
/// system side fills, pinned here so decoding and encoding agree.
mod wire {
    use super::PAYLOAD;

    /// `m_vfs_lchardriver_openclose` (`{ id; user; minor; access; }`).
    pub mod openclose {
        use super::PAYLOAD;
        pub const ID: usize = PAYLOAD;
        pub const USER: usize = PAYLOAD + 4;
        pub const MINOR: usize = PAYLOAD + 8;
        pub const ACCESS: usize = PAYLOAD + 12;
    }
    /// `m_vfs_lchardriver_cancel` (`{ id; minor; }`).
    pub mod cancel {
        use super::PAYLOAD;
        pub const ID: usize = PAYLOAD;
        pub const MINOR: usize = PAYLOAD + 4;
    }
    /// `m_vfs_lchardriver_select` (`{ minor; ops; }`).
    pub mod select {
        use super::PAYLOAD;
        pub const MINOR: usize = PAYLOAD;
        pub const OPS: usize = PAYLOAD + 4;
    }
    /// `m_vfs_lchardriver_readwrite` (shared by read, write, and ioctl).
    pub mod readwrite {
        use super::PAYLOAD;
        pub const POS: usize = PAYLOAD;
        pub const GRANT: usize = PAYLOAD + 8;
        pub const COUNT: usize = PAYLOAD + 16;
        pub const REQUEST: usize = PAYLOAD + 24;
        pub const FLAGS: usize = PAYLOAD + 32;
        pub const ID: usize = PAYLOAD + 36;
        pub const USER: usize = PAYLOAD + 40;
        pub const MINOR: usize = PAYLOAD + 44;
    }
    /// `m_notify` (`{ timestamp; interrupts; }`).
    pub mod notify {
        use super::PAYLOAD;
        pub const TIMESTAMP: usize = PAYLOAD;
        pub const INTERRUPTS: usize = PAYLOAD + 8;
    }
    /// `m_lchardriver_vfs_reply` (`{ status; id; }`).
    pub mod reply {
        use super::PAYLOAD;
        pub const STATUS: usize = PAYLOAD;
        pub const ID: usize = PAYLOAD + 4;
    }
}

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// Reads a little-endian array of `N` bytes at an offset into the message.
fn read_bytes<const N: usize>(msg: &Message, offset: usize) -> [u8; N] {
    let base = core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; N];
    // SAFETY: `Message` is a `#[repr(C)]` plain-old-data union; every
    // offset comes from the pinned wire layout above and stays inside the
    // message's fixed-size body.
    unsafe {
        core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), N);
    }
    bytes
}

fn read_i32(msg: &Message, offset: usize) -> i32 {
    i32::from_le_bytes(read_bytes(msg, offset))
}

fn read_u32(msg: &Message, offset: usize) -> u32 {
    u32::from_le_bytes(read_bytes(msg, offset))
}

fn read_u64(msg: &Message, offset: usize) -> u64 {
    u64::from_le_bytes(read_bytes(msg, offset))
}

/// Fills a reply message's `{ status; id; }` payload at the union start.
fn write_reply(msg: &mut Message, status: i32, id: u32) {
    let base = core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: the reply payload is two 4-byte words at the union start
    // (`wire::reply`), inside the message's fixed-size body.
    unsafe {
        core::ptr::copy_nonoverlapping(
            status.to_le_bytes().as_ptr(),
            base.add(wire::reply::STATUS),
            4,
        );
        core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(wire::reply::ID), 4);
    }
}

/// A pending reply: the request type decides the reply number, then the
/// status and echoed id ride the general reply payload.
struct PendingReply {
    reply_type: i32,
    status: i32,
    id: u32,
}

/// The tty service: the driver state plus the pump that answers requests.
///
/// Given a delivered message it classifies through the character
/// framework, dispatches to the matching [`TtyDriver`] hook, and sends the
/// framework's reply through the transport. `T` is generic so a host test
/// injects a scripted transport and asserts the replies.
pub struct TtyService<B: LineBackend> {
    /// The chardriver face (sessions, console line, backend, open set).
    pub driver: TtyDriver<B>,
}

impl<B: LineBackend> TtyService<B> {
    /// Wrap a driver face as a service.
    pub fn new(driver: TtyDriver<B>) -> Self {
        TtyService { driver }
    }

    /// The minor a character request addresses (per-struct offset).
    fn minor_of(request: CdevRequest, msg: &Message) -> DeviceMinor {
        let offset = match request {
            CdevRequest::Open | CdevRequest::Close => wire::openclose::MINOR,
            CdevRequest::Cancel => wire::cancel::MINOR,
            CdevRequest::Select => wire::select::MINOR,
            CdevRequest::Read | CdevRequest::Write | CdevRequest::Ioctl => wire::readwrite::MINOR,
        };
        DeviceMinor(read_u32(msg, offset))
    }

    /// The request id a reply must echo (zero for the id-less select).
    fn id_of(request: CdevRequest, msg: &Message) -> u32 {
        match request {
            CdevRequest::Open | CdevRequest::Close => read_u32(msg, wire::openclose::ID),
            CdevRequest::Read | CdevRequest::Write | CdevRequest::Ioctl => {
                read_u32(msg, wire::readwrite::ID)
            }
            CdevRequest::Cancel => read_u32(msg, wire::cancel::ID),
            CdevRequest::Select => 0,
        }
    }

    /// Run one character request and decide its reply.
    ///
    /// The seven hooks split on the return lane: transfer requests
    /// (read/write) yield a `Result<usize, Errno>` directly; the rest
    /// yield a raw status folded onto the error lane so [`reply_decision`]
    /// sees the same vocabulary of codes (its sentinel checks run on the
    /// integer, whichever lane carries it).
    fn handle_request(
        &mut self,
        request: CdevRequest,
        msg: &Message,
        transport: &mut impl DriverTransport,
    ) -> Option<PendingReply> {
        let minor = Self::minor_of(request, msg);
        let id = Self::id_of(request, msg);
        let outcome: Result<usize, minix_types::Errno> = match request {
            CdevRequest::Open => {
                let access = read_i32(msg, wire::openclose::ACCESS);
                let user = read_u32(msg, wire::openclose::USER) as i64;
                raw(self.driver.open(minor, access, user))
            }
            CdevRequest::Close => raw(self.driver.close(minor)),
            CdevRequest::Read => {
                let grant = read_u32(msg, wire::readwrite::GRANT);
                let count = read_u64(msg, wire::readwrite::COUNT) as usize;
                let position = read_u64(msg, wire::readwrite::POS);
                let flags = read_i32(msg, wire::readwrite::FLAGS);
                let result = self.driver.read(minor, position, grant as u64, count, flags, RequestId(id));
                // A completed read's bytes would be copied into the
                // caller's grant here (the data-plane seam, see module
                // doc); the count and park decision are already correct.
                let _ = transport;
                result
            }
            CdevRequest::Write => {
                let grant = read_u32(msg, wire::readwrite::GRANT);
                let count = read_u64(msg, wire::readwrite::COUNT) as usize;
                let position = read_u64(msg, wire::readwrite::POS);
                let flags = read_i32(msg, wire::readwrite::FLAGS);
                self.driver
                    .write(minor, position, grant as u64, count, flags, RequestId(id))
            }
            CdevRequest::Ioctl => {
                let request_code = read_u64(msg, wire::readwrite::REQUEST);
                let grant = read_u32(msg, wire::readwrite::GRANT);
                let flags = read_i32(msg, wire::readwrite::FLAGS);
                let user = read_u32(msg, wire::readwrite::USER) as i64;
                raw(self.driver.ioctl(minor, request_code, grant as u64, flags, user, RequestId(id)))
            }
            CdevRequest::Cancel => raw(self.driver.cancel(minor, RequestId(id))),
            CdevRequest::Select => raw(self.driver.select(minor, read_u32(msg, wire::select::OPS))),
        };
        let reply_type = if request == CdevRequest::Select {
            CDEV_SEL1_REPLY
        } else {
            CDEV_REPLY_BASE
        };
        match reply_decision(request, outcome) {
            ReplyDecision::Reply(status) => Some(PendingReply {
                reply_type,
                status,
                id,
            }),
            ReplyDecision::Parked | ReplyDecision::SwallowedRestart => None,
        }
    }

    /// Which notification arrived, from the sender and payload.
    fn notify_source(msg: &Message) -> NotifySource {
        if msg.m_source == Endpoint::CLOCK {
            NotifySource::Clock(read_u64(msg, wire::notify::TIMESTAMP) as i64)
        } else if msg.m_source == Endpoint::HARDWARE {
            NotifySource::Hardware(read_u64(msg, wire::notify::INTERRUPTS) as u32)
        } else {
            NotifySource::Other
        }
    }

    /// Handle one delivered message: classify, then route and reply.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let notification = is_ipc_notify(msg.m_type);
        let notify = if notification {
            Some(Self::notify_source(msg))
        } else {
            None
        };
        // The minor feeds the restart gate inside `classify`; only a
        // character request carries one, and its offset depends on the
        // request kind.
        let request = if notification {
            None
        } else {
            CdevRequest::decode(msg.m_type)
        };
        let minor = request.map(|req| Self::minor_of(req, msg));
        let caller = msg.m_source;
        let route = classify(notification, notify, msg.m_type, minor, &self.driver.opened);
        let pending = match route {
            Route::Notify(source) => {
                match source {
                    NotifySource::Hardware(mask) => self.driver.interrupt(mask),
                    NotifySource::Clock(stamp) => self.driver.alarm(stamp),
                    NotifySource::Other => self.driver.other(msg.m_type),
                }
                None
            }
            Route::Request(req) => self.handle_request(req, msg, transport),
            Route::BlockOpen => Some(PendingReply {
                reply_type: CDEV_REPLY_BASE,
                status: -minix_types::ENXIO,
                id: 0,
            }),
            // Stale (unopened after a restart) and Other take no reply.
            Route::Stale | Route::Other => None,
        };
        if let Some(reply) = pending {
            let mut message = Message {
                m_type: reply.reply_type,
                ..Message::default()
            };
            write_reply(&mut message, reply.status, reply.id);
            // Character replies are one-way (`asynsend3(AMF_NOREPLY)`). A
            // failed send is unrecoverable in C (panic) and here drops the
            // reply rather than wedging the single-threaded loop.
            let _ = transport.asynsend(caller, &mut message);
        }
    }
}

impl<B: LineBackend, T: DriverTransport> DriverHandler<T> for TtyService<B> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

/// Folds a raw status code onto the error lane for [`reply_decision`].
///
/// The non-transfer hooks return an `i32` status directly; carrying it as
/// `Err(Errno::from_i32(code))` keeps the integer intact (`to_i32` is the
/// identity) while letting the shared decision function see its sentinel
/// vocabulary uniformly.
fn raw(code: i32) -> Result<usize, minix_types::Errno> {
    Err(minix_types::Errno::from_i32(code))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::NullBackend;
    use crate::session::CONTROLLED;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_chardriver::protocol::{CdevRequest, DeviceMinor};

    /// Scripted transport: feeds queued messages to `receive`, records
    /// every reply sent (destination, type, decoded status, echoed id).
    struct Scripted {
        incoming: Vec<Message>,
        replies: Vec<(Endpoint, i32, i32, u32)>,
    }

    impl Scripted {
        fn new(incoming: Vec<Message>) -> Self {
            Scripted {
                incoming,
                replies: Vec::new(),
            }
        }
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, msg: &mut Message) -> Result<(), i32> {
            if self.incoming.is_empty() {
                return Err(-minix_types::EIO);
            }
            *msg = self.incoming.remove(0);
            Ok(())
        }
        fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            let _ = (dst, msg);
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.replies.push((
                dst,
                msg.m_type,
                read_i32(msg, wire::reply::STATUS),
                read_u32(msg, wire::reply::ID),
            ));
            Ok(())
        }
        fn copy_from_grant(&mut self, _g: Endpoint, _gr: i32, _o: u64, _b: &mut [u8]) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_grant(&mut self, _g: Endpoint, _gr: i32, _o: u64, _b: &[u8]) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    fn service() -> TtyService<NullBackend> {
        TtyService::new(TtyDriver::new(8, 0, NullBackend))
    }

    /// Writes a little-endian word at an absolute message offset.
    fn put(msg: &mut Message, offset: usize, bytes: &[u8]) {
        let base = core::ptr::addr_of_mut!(*msg) as *mut u8;
        // SAFETY: test-owned message; offsets come from the wire layout.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), base.add(offset), bytes.len());
        }
    }

    fn request(request: CdevRequest, caller: Endpoint) -> Message {
        Message {
            m_type: request.message_type(),
            m_source: caller,
            ..Message::default()
        }
    }

    fn open_msg(minor: u32, id: u32, caller: Endpoint) -> Message {
        let mut msg = request(CdevRequest::Open, caller);
        put(&mut msg, wire::openclose::ID, &id.to_le_bytes());
        put(&mut msg, wire::openclose::MINOR, &minor.to_le_bytes());
        msg
    }

    fn read_msg(minor: u32, count: u64, flags: i32, id: u32, caller: Endpoint) -> Message {
        let mut msg = request(CdevRequest::Read, caller);
        put(&mut msg, wire::readwrite::MINOR, &minor.to_le_bytes());
        put(&mut msg, wire::readwrite::COUNT, &count.to_le_bytes());
        put(&mut msg, wire::readwrite::FLAGS, &flags.to_le_bytes());
        put(&mut msg, wire::readwrite::ID, &id.to_le_bytes());
        msg
    }

    fn write_msg(minor: u32, count: u64, id: u32, caller: Endpoint) -> Message {
        let mut msg = request(CdevRequest::Write, caller);
        put(&mut msg, wire::readwrite::MINOR, &minor.to_le_bytes());
        put(&mut msg, wire::readwrite::COUNT, &count.to_le_bytes());
        put(&mut msg, wire::readwrite::ID, &id.to_le_bytes());
        msg
    }

    fn select_msg(minor: u32, ops: u32, caller: Endpoint) -> Message {
        let mut msg = request(CdevRequest::Select, caller);
        put(&mut msg, wire::select::MINOR, &minor.to_le_bytes());
        put(&mut msg, wire::select::OPS, &ops.to_le_bytes());
        msg
    }

    /// A first open of the console line records the device and answers with
    /// the controlling-terminal flag (`CDEV_CTTY`) and the echoed id, the
    /// way C `chardriver_reply` echoes `do_open`'s raw result.
    #[test]
    fn test_open_answers_and_records_device() {
        let caller = Endpoint(9);
        let mut svc = service();
        let msg = open_msg(0, 7, caller);
        let mut transport = Scripted::new(vec![msg]);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies, vec![(caller, CDEV_REPLY_BASE, CONTROLLED, 7)]);
        assert!(svc.driver.opened.contains_raw(0));
    }

    /// A read for a never-opened minor is stale: dropped without a reply.
    #[test]
    fn test_read_on_unopened_device_is_dropped() {
        let caller = Endpoint(9);
        let mut svc = service();
        let msg = read_msg(0, 64, 0, 3, caller);
        let mut transport = Scripted::new(vec![msg]);
        svc.dispatch(&mut transport, &msg);
        assert!(transport.replies.is_empty());
    }

    /// A read of a line with queued bytes answers the byte count.
    #[test]
    fn test_read_answers_queued_line_count() {
        let caller = Endpoint(9);
        let mut svc = service();
        svc.driver.open(DeviceMinor(0), 0, 42);
        svc.driver.sessions[0].feed_input(b"ab\n");
        let msg = read_msg(0, 64, 0, 5, caller);
        let mut transport = Scripted::new(vec![msg]);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies, vec![(caller, CDEV_REPLY_BASE, 3, 5)]);
    }

    /// A write answers the byte count the backend accepted (null accepts 0).
    #[test]
    fn test_write_answers_backend_accepted_count() {
        let caller = Endpoint(9);
        let mut svc = service();
        svc.driver.open(DeviceMinor(0), 0, 42);
        let msg = write_msg(0, 10, 6, caller);
        let mut transport = Scripted::new(vec![msg]);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies, vec![(caller, CDEV_REPLY_BASE, 0, 6)]);
    }

    /// A block-side open on this character driver answers "no such device".
    #[test]
    fn test_block_open_is_answered_enxio() {
        let caller = Endpoint(9);
        let mut svc = service();
        let msg = Message {
            m_type: minix_chardriver::driver::BLOCK_OPEN_MESSAGE,
            m_source: caller,
            ..Message::default()
        };
        let mut transport = Scripted::new(vec![msg]);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(
            transport.replies,
            vec![(caller, CDEV_REPLY_BASE, -minix_types::ENXIO, 0)]
        );
    }

    /// A clock notification runs no reply path at all.
    #[test]
    fn test_clock_notify_takes_no_reply() {
        let mut svc = service();
        // The transport folds a notification into the 0x1000 type family;
        // the source identifies it as the clock.
        let msg = Message {
            m_type: 0x1000,
            m_source: Endpoint::CLOCK,
            ..Message::default()
        };
        let mut transport = Scripted::new(vec![msg]);
        svc.dispatch(&mut transport, &msg);
        assert!(transport.replies.is_empty());
    }

    /// A select on an opened line takes the selector reply number
    /// (`CDEV_SEL1_REPLY`) and echoes no id, distinct from the base reply
    /// used by the other six requests.
    #[test]
    fn test_select_uses_selector_reply_number() {
        let caller = Endpoint(9);
        let mut svc = service();
        svc.driver.open(DeviceMinor(0), 0, 42);
        let msg = select_msg(0, 0o3, caller);
        let mut transport = Scripted::new(vec![msg]);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(
            transport.replies,
            vec![(caller, CDEV_SEL1_REPLY, minix_types::EBADF, 0)]
        );
    }
}

