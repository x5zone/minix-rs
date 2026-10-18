//! Service assembly: the pty driver's receive-classify-dispatch pump.
//!
//! C correspondence: `pty_main`'s loop plus `pty_master_read/write`
//! (`pty.c:380-460`) — every delivered message is a character request or
//! a notification over the master minors; the loop shell and the RS birth
//! handshake live in [`minix_driver_rt::runtime::DriverRuntime`]. This
//! module owns the one-message-at-a-time decision plus the parked-read
//! wake channel: an empty master read parks in the output ring, and the
//! slave-write path (the tty line, a registered cross-crate seam) later
//! pumps the ring and answers the parked caller through [`wake_parked`].
//!
//! The byte copies stay in the service crate by design (the ring moves
//! counts, `buffer.rs`'s module note): the grant legs between the caller
//! and the slave queues are the registered data-plane seam shared with
//! the tty pump, not a fabricated success.
//!
//! The judgment cores are shared: `minix_chardriver::driver::{classify,
//! reply_decision}` are authoritative, so this file re-implements no
//! routing rule.

use minix_chardriver::driver::{
    classify as classify_char, reply_decision, CharDriver, NotifySource, ReplyDecision,
    Route as CharRoute,
};
use minix_chardriver::protocol::{CdevRequest, DeviceMinor, RequestId};
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message, CDEV_REPLY_BASE};

use crate::char_face::PtyMasterFace;

/// The reply base plus one: a select poll's immediate answer
/// (`CDEV_SEL1_REPLY`, `com.h:936`). minix-types names only the base.
const CDEV_SEL1_REPLY: i32 = CDEV_REPLY_BASE + 1;

/// The message union payload begins at byte 8 of [`Message`].
const PAYLOAD: usize = 8;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// Character-request wire offsets, keyed per payload struct.
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

/// The shared `{ status; id; }` reply payload.
mod reply {
    use super::PAYLOAD;
    pub const STATUS: usize = PAYLOAD;
    pub const ID: usize = PAYLOAD + 4;
}

fn read_bytes<const N: usize>(msg: &Message, offset: usize) -> [u8; N] {
    let base = ::core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; N];
    // SAFETY: `Message` is a `#[repr(C)]` plain-old-data union; every
    // offset comes from the pinned wire layout above and stays inside the
    // fixed-size body.
    unsafe {
        ::core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), N);
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
    let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: the reply payload is two 4-byte words at the union start,
    // inside the message's fixed-size body.
    unsafe {
        ::core::ptr::copy_nonoverlapping(status.to_le_bytes().as_ptr(), base.add(reply::STATUS), 4);
        ::core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(reply::ID), 4);
    }
}

/// Folds a raw status code onto the error lane for [`reply_decision`].
fn raw(code: i32) -> Result<usize, minix_types::Errno> {
    Err(minix_types::Errno::from_i32(code))
}

/// The pty master service: the pair-table face plus the wake channel.
pub struct PtyService {
    /// The chardriver face over the master minors.
    pub face: PtyMasterFace,
    /// Who parked each minor's master read. The face's ring records only
    /// the request id (`park_read`'s caller field is the model's), so the
    /// service owns the caller that gets the wake reply.
    pub parked_callers: alloc::vec::Vec<Endpoint>,
}

impl PtyService {
    /// A service over fresh pairs.
    pub fn new() -> Self {
        PtyService {
            face: PtyMasterFace::new(),
            parked_callers: alloc::vec![Endpoint(0); crate::pair::PAIR_COUNT],
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
        caller: Endpoint,
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
                let outcome = self
                    .face
                    .read(minor, 0, 0, read_u64(msg, cwire::readwrite::COUNT) as usize, 0, RequestId(id));
                if matches!(&outcome, Err(error) if error.to_i32() == minix_types::EDONTREPLY)
                    && let Some(slot) = self.parked_callers.get_mut(minor.0 as usize)
                {
                    *slot = caller;
                }
                outcome
            }
            CdevRequest::Write => self
                .face
                .write(minor, 0, 0, read_u64(msg, cwire::readwrite::COUNT) as usize, 0, RequestId(id)),
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

    /// Serve a parked master read after slave bytes arrived: pump the
    /// ring and answer the parked caller with the moved count.
    ///
    /// C: `pty_finish` answers only when `rdcum > 0` (`pty.c:430-460`).
    /// The slave-write path calls this once its grant copy landed (a
    /// registered cross-crate seam with the tty line).
    pub fn wake_parked<T: DriverTransport>(
        &mut self,
        minor: u32,
        transport: &mut T,
    ) -> bool {
        let Some(ring) = self.face.rings.get_mut(minor as usize) else {
            return false;
        };
        let packet_mode = self
            .face
            .table
            .get(minor as usize)
            .map(|pair| pair.packet_mode())
            .unwrap_or(false);
        ring.pump(packet_mode);
        let Some((_caller, id, moved)) = ring.finish() else {
            return false;
        };
        let caller = self
            .parked_callers
            .get(minor as usize)
            .copied()
            .unwrap_or(Endpoint(0));
        self.reply(caller, CDEV_REPLY_BASE, moved as i32, id as i32, transport);
        true
    }

    /// Handle one delivered message: route it, then reply if the verdict
    /// says so.
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
            CharRoute::BlockOpen => Some((CDEV_REPLY_BASE, -minix_types::ENXIO, 0)),
            CharRoute::Request(req) => self.handle_char(req, msg, caller),
            CharRoute::Stale | CharRoute::Other => None,
        };
        if let Some((reply_type, status, id)) = pending {
            self.reply(caller, reply_type, status, id as i32, transport);
        }
    }
}

impl Default for PtyService {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: DriverTransport> DriverHandler<T> for PtyService {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// Scripted transport recording every reply.
    struct Scripted {
        replies: Vec<(Endpoint, i32, i32, i32)>,
    }

    impl Scripted {
        fn new() -> Self {
            Scripted {
                replies: Vec::new(),
            }
        }
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, msg: &mut Message) -> Result<(), i32> {
            let _ = msg;
            Err(-minix_types::EIO)
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

    /// Build a character request message with type, minor, id, count.
    fn char_msg(request: CdevRequest, minor: u32, id: u32, count: u64) -> Message {
        let mut msg = Message {
            m_type: request.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(msg) as *mut u8;
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
            ::core::ptr::copy_nonoverlapping(minor.to_le_bytes().as_ptr(), base.add(minor_off), 4);
            if let Some(id_off) = id_off {
                ::core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(id_off), 4);
            }
            if let Some(count_off) = count_off {
                ::core::ptr::copy_nonoverlapping(count.to_le_bytes().as_ptr(), base.add(count_off), 8);
            }
        }
        msg
    }

    #[test]
    fn test_open_and_second_open_refused() {
        let mut svc = PtyService::new();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 2, 0));
        assert_eq!(
            transport.replies[0],
            (Endpoint(20), CDEV_REPLY_BASE, 0, 1)
        );
        assert_ne!(transport.replies[1].2, 0);
    }

    #[test]
    fn test_empty_read_parks_without_reply() {
        let mut svc = PtyService::new();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 64));
        // Only the open reply: the empty read parked.
        assert_eq!(transport.replies.len(), 1);
        assert!(svc.face.rings[0].has_parked_reader());
    }

    #[test]
    fn test_wake_answers_the_parked_read_once_bytes_arrive() {
        let mut svc = PtyService::new();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 64));
        // The slave-write path feeds the ring, then the wake serves the park.
        svc.face.rings[0].feed(b"hello");
        assert!(svc.wake_parked(0, &mut transport));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, 5, 2))
        );
        assert!(!svc.face.rings[0].has_parked_reader());
    }

    #[test]
    fn test_write_admits_what_fits() {
        let mut svc = PtyService::new();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Write, 0, 2, 128));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, 128, 2))
        );
    }

    #[test]
    fn test_unknown_minor_replies_enxio() {
        let mut svc = PtyService::new();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 32, 1, 8));
        assert!(transport.replies.is_empty());
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 32, 2, 0));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, -minix_types::ENXIO, 2))
        );
    }
}
