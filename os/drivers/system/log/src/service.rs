//! Service assembly: the log driver's receive-classify-dispatch pump.
//!
//! C correspondence: `main()` (`log.c:41-66`) — every delivered message is
//! a character request or a notification over the single `/dev/klog` minor;
//! the loop shell and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`]. This module owns only the
//! one-message-at-a-time decision: which hook, and whether to reply.
//!
//! The judgment cores are shared: `minix_chardriver::driver::{classify,
//! reply_decision}` are authoritative, so this file re-implements no routing
//! rule. Data-plane seam: the read and write hooks answer byte counts from
//! the ring cursors; the physical grant copies between the caller buffer and
//! the service-owned ring bytes are not wired here (the hooks hand out
//! counts, not bytes) — the same registered gap the memory and tty pumps
//! carry, not a fabricated success. The control flow, restart gate, park
//! plan, and reply discipline are complete and host-tested.

use minix_chardriver::driver::{
    classify as classify_char, reply_decision, CharDriver, NotifySource, ReplyDecision,
    Route as CharRoute,
};
use minix_chardriver::protocol::{CdevRequest, DeviceMinor, RequestId};
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message, CDEV_REPLY_BASE};

use crate::char_face::LogFace;

/// The reply base plus one: a select poll's immediate answer
/// (`CDEV_SEL1_REPLY`, `com.h:936`). minix-types names only the base.
const CDEV_SEL1_REPLY: i32 = CDEV_REPLY_BASE + 1;

/// The message union payload begins at byte 8 of [`Message`] (after
/// `m_type` at 0 and `m_source` at 4, with 8-byte union alignment). Every
/// wire offset below is `8 +` the field's position inside its C payload
/// struct.
const PAYLOAD: usize = 8;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// Character-request wire offsets, keyed per payload struct.
///
/// C: `mess_vfs_lchardriver_{openclose,cancel,select,readwrite}` and
/// `m_notify` (`ipc.h:2206-2258`) — the layout the file-system side fills.
/// No typed character arm exists in `MessageUnion` yet, so these fields
/// are read by pinned byte offset (the same interim the memory and tty
/// pumps use, pending E-MINTYPES-RUNTIME owned by another lane).
mod cwire {
    use super::PAYLOAD;
    pub mod openclose {
        use super::PAYLOAD;
        pub const ID: usize = PAYLOAD;
        pub const USER: usize = PAYLOAD + 4;
        pub const MINOR: usize = PAYLOAD + 8;
        pub const ACCESS: usize = PAYLOAD + 12;
    }
    pub mod cancel {
        use super::PAYLOAD;
        pub const ID: usize = PAYLOAD;
        pub const MINOR: usize = PAYLOAD + 4;
    }
    pub mod select {
        use super::PAYLOAD;
        pub const MINOR: usize = PAYLOAD;
        pub const OPS: usize = PAYLOAD + 4;
    }
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
    pub mod notify {
        use super::PAYLOAD;
        pub const TIMESTAMP: usize = PAYLOAD;
        pub const INTERRUPTS: usize = PAYLOAD + 8;
    }
}

/// The shared `{ status; id; }` reply payload (`wire::reply` for the
/// character family: `mess_vfs_lchardriver_*_reply`, `ipc.h:2206-2258`).
mod reply {
    use super::PAYLOAD;
    pub const STATUS: usize = PAYLOAD;
    pub const ID: usize = PAYLOAD + 4;
}

fn read_bytes<const N: usize>(msg: &Message, offset: usize) -> [u8; N] {
    let base = core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; N];
    // SAFETY: `Message` is a `#[repr(C)]` plain-old-data union; every offset
    // comes from the pinned wire layout above and stays inside the message's
    // fixed-size body.
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

fn read_i64(msg: &Message, offset: usize) -> i64 {
    i64::from_le_bytes(read_bytes(msg, offset))
}

fn read_u64(msg: &Message, offset: usize) -> u64 {
    u64::from_le_bytes(read_bytes(msg, offset))
}

/// Fills a reply message's `{ status; id; }` payload at the union start.
fn write_reply(msg: &mut Message, status: i32, id: i32) {
    let base = core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: the reply payload is two 4-byte words at the union start
    // (`reply`), inside the message's fixed-size body.
    unsafe {
        core::ptr::copy_nonoverlapping(status.to_le_bytes().as_ptr(), base.add(reply::STATUS), 4);
        core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(reply::ID), 4);
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

/// The log service: one device face plus the pump that answers requests.
///
/// The face is generic-free: the log ring is a counter pair with no
/// storage backend, so a host test exercises the same type the binary
/// builds. The production binary passes the same face; the grant-copy
/// seam is transport-side (see the module note).
pub struct LogService {
    /// The chardriver face over `/dev/klog` (open/read/write/cancel/select).
    pub face: LogFace,
}

impl LogService {
    /// Wrap a fresh face as a service.
    pub fn new() -> Self {
        LogService {
            face: LogFace::new(),
        }
    }

    /// The minor a character request addresses (offset depends on kind).
    fn char_minor_of(request: CdevRequest, msg: &Message) -> DeviceMinor {
        let offset = match request {
            CdevRequest::Open | CdevRequest::Close => cwire::openclose::MINOR,
            CdevRequest::Cancel => cwire::cancel::MINOR,
            CdevRequest::Select => cwire::select::MINOR,
            CdevRequest::Read | CdevRequest::Write | CdevRequest::Ioctl => cwire::readwrite::MINOR,
        };
        DeviceMinor(read_u32(msg, offset))
    }

    /// The request id a character reply must echo (zero for select).
    fn char_id_of(request: CdevRequest, msg: &Message) -> u32 {
        match request {
            CdevRequest::Open | CdevRequest::Close => read_u32(msg, cwire::openclose::ID),
            CdevRequest::Read | CdevRequest::Write | CdevRequest::Ioctl => {
                read_u32(msg, cwire::readwrite::ID)
            }
            CdevRequest::Cancel => read_u32(msg, cwire::cancel::ID),
            CdevRequest::Select => 0,
        }
    }

    /// Which notification arrived, from the sender and payload.
    fn notify_source(msg: &Message) -> NotifySource {
        if msg.m_source == Endpoint::CLOCK {
            NotifySource::Clock(read_i64(msg, cwire::notify::TIMESTAMP))
        } else if msg.m_source == Endpoint::HARDWARE {
            NotifySource::Hardware(read_u32(msg, cwire::notify::INTERRUPTS))
        } else {
            NotifySource::Other
        }
    }

    /// Run one character request and decide its reply.
    fn handle_char(
        &mut self,
        request: CdevRequest,
        msg: &Message,
    ) -> Option<(i32, i32, u32)> {
        let minor = Self::char_minor_of(request, msg);
        let id = Self::char_id_of(request, msg);
        let outcome: Result<usize, minix_types::Errno> = match request {
            CdevRequest::Open => {
                let access = read_i32(msg, cwire::openclose::ACCESS);
                let user = read_u32(msg, cwire::openclose::USER) as i64;
                raw(self.face.open(minor, access, user))
            }
            CdevRequest::Close => raw(self.face.close(minor)),
            CdevRequest::Read => {
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let count = read_u64(msg, cwire::readwrite::COUNT) as usize;
                let position = read_u64(msg, cwire::readwrite::POS);
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                self.face.read(minor, position, grant, count, flags, RequestId(id))
            }
            CdevRequest::Write => {
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let count = read_u64(msg, cwire::readwrite::COUNT) as usize;
                let position = read_u64(msg, cwire::readwrite::POS);
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                self.face.write(minor, position, grant, count, flags, RequestId(id))
            }
            CdevRequest::Ioctl => {
                let request_code = read_u64(msg, cwire::readwrite::REQUEST);
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                let user = read_u32(msg, cwire::readwrite::USER) as i64;
                raw(self
                    .face
                    .ioctl(minor, request_code, grant, flags, user, RequestId(id)))
            }
            CdevRequest::Cancel => raw(self.face.cancel(minor, RequestId(id))),
            CdevRequest::Select => raw(self.face.select(minor, read_u32(msg, cwire::select::OPS))),
        };
        let reply_type = if request == CdevRequest::Select {
            CDEV_SEL1_REPLY
        } else {
            CDEV_REPLY_BASE
        };
        match reply_decision(request, outcome) {
            ReplyDecision::Reply(status) => Some((reply_type, status, id)),
            ReplyDecision::Parked | ReplyDecision::SwallowedRestart => None,
        }
    }

    /// Send one framework reply back to the caller.
    ///
    /// Character replies are one-way (`asynsend`, `AMF_NOREPLY`). A failed
    /// send is unrecoverable in C (panic); here it drops the reply rather
    /// than wedging the single-threaded loop.
    fn reply<T: DriverTransport>(
        &mut self,
        caller: Endpoint,
        reply_type: i32,
        status: i32,
        id: i32,
        transport: &mut T,
    ) {
        let mut message = Message {
            m_type: reply_type,
            ..Message::default()
        };
        write_reply(&mut message, status, id);
        let _ = transport.asynsend(caller, &mut message);
    }

    /// Handle one delivered message: route it, then reply if the verdict
    /// says so.
    ///
    /// C: `chardriver_process` (`chardriver.c:503-513`) with the same
    /// four-way classification: notifications go to the hardware/clock/
    /// other hooks, a block open to the character face is the wrong window
    /// (answer "no such device"), a stale request (device not opened since
    /// the restart) is dropped, and a real request runs its hook.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let notification = is_ipc_notify(msg.m_type);
        let notify = if notification {
            Some(Self::notify_source(msg))
        } else {
            None
        };
        let request = if notification {
            None
        } else {
            CdevRequest::decode(msg.m_type)
        };
        let minor = request.map(|req| Self::char_minor_of(req, msg));
        let caller = msg.m_source;
        let route = classify_char(notification, notify, msg.m_type, minor, &self.face.opened);
        let pending = match route {
            CharRoute::Notify(source) => {
                match source {
                    NotifySource::Hardware(mask) => self.face.interrupt(mask),
                    NotifySource::Clock(stamp) => self.face.alarm(stamp),
                    NotifySource::Other => self.face.other(msg.m_type),
                }
                None
            }
            // The log driver is single-family; a block open reaching this
            // face is genuinely the wrong window (the shared chardriver
            // rule answers "no such device").
            CharRoute::BlockOpen => Some((CDEV_REPLY_BASE, -minix_types::ENXIO, 0)),
            CharRoute::Request(req) => self.handle_char(req, msg),
            CharRoute::Stale | CharRoute::Other => None,
        };
        if let Some((reply_type, status, id)) = pending {
            self.reply(caller, reply_type, status, id as i32, transport);
        }
    }
}

impl Default for LogService {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: DriverTransport> DriverHandler<T> for LogService {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Scripted transport: feeds queued messages to `receive`, records every
    /// reply sent (destination, type, status, echoed id).
    struct Scripted {
        incoming: Vec<Message>,
        replies: Vec<(Endpoint, i32, i32, i32)>,
    }

    impl Scripted {
        fn new() -> Self {
            Scripted {
                incoming: Vec::new(),
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
            self.replies.push((
                dst,
                msg.m_type,
                read_i32(msg, reply::STATUS),
                read_i32(msg, reply::ID),
            ));
            Ok(())
        }
        fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            // Birth-report leg; not exercised by these service-loop tests.
            let _ = (dst, msg);
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.replies.push((
                dst,
                msg.m_type,
                read_i32(msg, reply::STATUS),
                read_i32(msg, reply::ID),
            ));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _g: Endpoint,
            _grant: i32,
            _off: u64,
            _buf: &mut [u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _g: Endpoint,
            _grant: i32,
            _off: u64,
            _buf: &[u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    /// Build a character request message with the given type, minor, id.
    /// `count` fills the read/write byte count (zero elsewhere).
    fn char_msg(request: CdevRequest, minor: u32, id: u32, count: u64) -> Message {
        let mut msg = Message {
            m_type: request.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        let base = core::ptr::addr_of_mut!(msg) as *mut u8;
        let (minor_off, id_off, count_off): (usize, Option<usize>, Option<usize>) = match request {
            CdevRequest::Open | CdevRequest::Close => {
                (cwire::openclose::MINOR, Some(cwire::openclose::ID), None)
            }
            CdevRequest::Cancel => (cwire::cancel::MINOR, Some(cwire::cancel::ID), None),
            CdevRequest::Select => (cwire::select::MINOR, None, None),
            CdevRequest::Read | CdevRequest::Write | CdevRequest::Ioctl => (
                cwire::readwrite::MINOR,
                Some(cwire::readwrite::ID),
                Some(cwire::readwrite::COUNT),
            ),
        };
        // SAFETY: pinned offsets inside the fixed-size message body.
        unsafe {
            core::ptr::copy_nonoverlapping(minor.to_le_bytes().as_ptr(), base.add(minor_off), 4);
            if let Some(id_off) = id_off {
                core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(id_off), 4);
            }
            if let Some(count_off) = count_off {
                core::ptr::copy_nonoverlapping(count.to_le_bytes().as_ptr(), base.add(count_off), 8);
            }
        }
        msg
    }

    #[test]
    fn test_open_replies_zero_and_records_open() {
        let mut service = LogService::new();
        let mut transport = Scripted::new();
        let msg = char_msg(CdevRequest::Open, 0, 7, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies, alloc::vec![(Endpoint(20), CDEV_REPLY_BASE, 0, 7)]);
        assert!(service.face.opened.contains_raw(0));
    }

    #[test]
    fn test_write_admits_then_read_answers() {
        let mut service = LogService::new();
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        service.dispatch(&mut transport, &char_msg(CdevRequest::Write, 0, 2, 100));
        service.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 3, 40));
        assert_eq!(
            transport.replies,
            alloc::vec![
                (Endpoint(20), CDEV_REPLY_BASE, 0, 1),
                (Endpoint(20), CDEV_REPLY_BASE, 100, 2),
                (Endpoint(20), CDEV_REPLY_BASE, 40, 3),
            ]
        );
    }

    #[test]
    fn test_empty_read_parks_without_reply() {
        let mut service = LogService::new();
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        service.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 40));
        // Only the open reply: the empty read parked (EDONTREPLY sentinel).
        assert_eq!(transport.replies, alloc::vec![(Endpoint(20), CDEV_REPLY_BASE, 0, 1)]);
    }

    #[test]
    fn test_request_before_open_is_stale_and_dropped() {
        let mut service = LogService::new();
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 40));
        assert!(transport.replies.is_empty());
    }

    #[test]
    fn test_select_replies_on_the_sel1_type() {
        let mut service = LogService::new();
        let mut transport = Scripted::new();
        service.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        service.dispatch(&mut transport, &char_msg(CdevRequest::Select, 0, 0, 0));
        assert_eq!(transport.replies.len(), 2);
        assert_eq!(transport.replies[1].1, CDEV_SEL1_REPLY);
        assert!(transport.replies[1].2 >= 0);
    }
}
