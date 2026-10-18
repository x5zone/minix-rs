//! Service assembly: the framebuffer driver's receive-classify-dispatch pump.
//!
//! C correspondence: the character loop plus `fb_read`/`fb_write`
//! (`fb.c:196-240`) and the ioctl handlers (`fb.c:145-192`) — every
//! delivered message is a character request or a notification over the
//! single `/dev/fb` device; the loop shell and the RS birth handshake
//! live in [`minix_driver_rt::runtime::DriverRuntime`]. This module owns
//! the one-message-at-a-time decision plus the two data-plane legs the
//! face leaves to the service: bytes copy between the caller's grant and
//! the framebuffer memory behind the [`Framebuffer`] trait.
//!
//! The restart window is C's: after a restart the driver keeps displaying
//! the old contents and refuses writes and the variable/pan ioctls with
//! `EAGAIN` until the frame-clock window passes (`has_restarted` +
//! `keep_displaying_restarted`, `fb.c:77-78,168,186,221`). The window
//! predicate is a service field the binary drives from the frame clock;
//! host tests set it directly.
//!
//! The judgment cores are shared: `minix_chardriver::driver::{classify,
//! reply_decision}` are authoritative, so this file re-implements no
//! routing rule. The ioctl payload copies (variable/fixed screen info,
//! pan) are the registered service seam.

use minix_chardriver::driver::{
    classify as classify_char, reply_decision, CharDriver, NotifySource, ReplyDecision,
    Route as CharRoute,
};
use minix_chardriver::protocol::{CdevRequest, DeviceMinor, RequestId};
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message, CDEV_REPLY_BASE};

use crate::char_face::FbFace;
use crate::display::{decode_ioctl, FbIoctl};

/// The reply base plus one: a select poll's immediate answer
/// (`CDEV_SEL1_REPLY`, `com.h:936`). minix-types names only the base.
const CDEV_SEL1_REPLY: i32 = CDEV_REPLY_BASE + 1;

/// Byte count of one staging chunk for framebuffer copies.
const COPY_CHUNK: usize = 1024;

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

/// The framebuffer memory the service copies bytes to and from.
///
/// The mapped framebuffer arrives with the device (PCI/ACPI on the real
/// board); this trait is the seam between the service's copy legs and
/// that memory.
pub trait Framebuffer {
    /// Copy `buf.len()` bytes at `offset` out of the framebuffer.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), i32>;
    /// Copy `buf` into the framebuffer at `offset`.
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), i32>;
}

/// No framebuffer mapped yet: every access answers `EIO` honestly.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullFramebuffer;

impl Framebuffer for NullFramebuffer {
    fn read_at(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<(), i32> {
        Err(-minix_types::EIO)
    }
    fn write_at(&mut self, _offset: u64, _buf: &[u8]) -> Result<(), i32> {
        Err(-minix_types::EIO)
    }
}

/// The framebuffer service: the face, its memory, and the restart window.
pub struct FbService<B: Framebuffer> {
    /// The chardriver face over `/dev/fb`.
    pub face: FbFace,
    /// The framebuffer memory behind the copy legs.
    pub framebuffer: B,
    /// Set once the driver has been restarted (C `has_restarted`).
    pub has_restarted: bool,
    /// Whether the restart window still asks the driver to keep the old
    /// contents (C `keep_displaying_restarted`, a frame-clock window).
    pub keep_displaying: bool,
}

impl<B: Framebuffer> FbService<B> {
    /// Wrap a face and its framebuffer as a service.
    pub fn new(face: FbFace, framebuffer: B) -> Self {
        FbService {
            face,
            framebuffer,
            has_restarted: false,
            keep_displaying: false,
        }
    }

    /// C `has_restarted && keep_displaying_restarted()` (`fb.c:168`).
    fn window_gated(&self) -> bool {
        self.has_restarted && self.keep_displaying
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
    fn handle_char<T: DriverTransport>(
        &mut self,
        request: CdevRequest,
        msg: &Message,
        transport: &mut T,
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
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let count = read_u64(msg, cwire::readwrite::COUNT) as usize;
                let position = read_u64(msg, cwire::readwrite::POS);
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                match self.face.read(minor, position, grant, count, flags, RequestId(id)) {
                    Ok(size) => match self.stream_out(transport, caller, grant as i32, position, size) {
                        Ok(()) => Ok(size),
                        Err(code) => Err(minix_types::Errno::from_i32(code)),
                    },
                    Err(error) => Err(error),
                }
            }
            CdevRequest::Write => {
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let count = read_u64(msg, cwire::readwrite::COUNT) as usize;
                let position = read_u64(msg, cwire::readwrite::POS);
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                if self.window_gated() {
                    raw(-minix_types::EAGAIN)
                } else {
                    match self.face.write(minor, position, grant, count, flags, RequestId(id)) {
                        Ok(size) => match self.stream_in(transport, caller, grant as i32, position, size) {
                            Ok(()) => Ok(size),
                            Err(code) => Err(minix_types::Errno::from_i32(code)),
                        },
                        Err(error) => Err(error),
                    }
                }
            }
            CdevRequest::Ioctl => {
                let request_code = read_u64(msg, cwire::readwrite::REQUEST);
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                let user = read_u32(msg, cwire::readwrite::USER) as i64;
                let gated = matches!(
                    decode_ioctl(request_code as u32),
                    Some(FbIoctl::PutVarScreenInfo) | Some(FbIoctl::PanDisplay)
                );
                if gated && self.window_gated() {
                    raw(-minix_types::EAGAIN)
                } else {
                    raw(self
                        .face
                        .ioctl(minor, request_code, grant, flags, user, RequestId(id)))
                }
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

    /// Copy `size` framebuffer bytes at `position` out to the caller's
    /// grant in transfer chunks (C `fb_read`'s truncating copy).
    fn stream_out<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        grant: i32,
        position: u64,
        size: usize,
    ) -> Result<(), i32> {
        let mut staging = [0u8; COPY_CHUNK];
        let mut offset = 0u64;
        let mut remaining = size;
        while remaining > 0 {
            let chunk = remaining.min(COPY_CHUNK);
            self.framebuffer
                .read_at(position + offset, &mut staging[..chunk])?;
            transport.copy_to_grant(caller, grant, offset, &staging[..chunk])?;
            offset += chunk as u64;
            remaining -= chunk;
        }
        Ok(())
    }

    /// Copy `size` caller bytes at `position` into the framebuffer (C
    /// `fb_write`'s `sys_safecopyfrom` leg).
    fn stream_in<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        grant: i32,
        position: u64,
        size: usize,
    ) -> Result<(), i32> {
        let mut staging = [0u8; COPY_CHUNK];
        let mut offset = 0u64;
        let mut remaining = size;
        while remaining > 0 {
            let chunk = remaining.min(COPY_CHUNK);
            transport.copy_from_grant(caller, grant, offset, &mut staging[..chunk])?;
            self.framebuffer
                .write_at(position + offset, &staging[..chunk])?;
            offset += chunk as u64;
            remaining -= chunk;
        }
        Ok(())
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
            CharRoute::Request(req) => self.handle_char(req, msg, transport, caller),
            CharRoute::Stale | CharRoute::Other => None,
        };
        if let Some((reply_type, status, id)) = pending {
            self.reply(caller, reply_type, status, id as i32, transport);
        }
    }
}

impl<B: Framebuffer, T: DriverTransport> DriverHandler<T> for FbService<B> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

/// A byte-vector framebuffer (host tests).
#[cfg(test)]
pub struct VecFramebuffer(pub alloc::vec::Vec<u8>);

#[cfg(test)]
impl Framebuffer for VecFramebuffer {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), i32> {
        let start = offset as usize;
        if start + buf.len() > self.0.len() {
            return Err(-minix_types::EIO);
        }
        buf.copy_from_slice(&self.0[start..start + buf.len()]);
        Ok(())
    }
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), i32> {
        let start = offset as usize;
        if start + buf.len() > self.0.len() {
            return Err(-minix_types::EIO);
        }
        self.0[start..start + buf.len()].copy_from_slice(buf);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Scripted transport: a caller grant buffer plus recorded replies.
    struct Scripted {
        buffer: Vec<u8>,
        replies: Vec<(Endpoint, i32, i32, i32)>,
    }

    impl Scripted {
        fn new() -> Self {
            Scripted {
                buffer: vec![0u8; 4096],
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
            offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            let start = offset as usize;
            buf.copy_from_slice(&self.buffer[start..start + buf.len()]);
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _g: Endpoint,
            _grant: i32,
            offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            let start = offset as usize;
            self.buffer[start..start + buf.len()].copy_from_slice(buf);
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    fn service(device_size: u64) -> FbService<VecFramebuffer> {
        FbService::new(
            FbFace::new(device_size, 4),
            VecFramebuffer(vec![0x5A; device_size as usize]),
        )
    }

    /// Build a character request message with type, minor, id, and the
    /// read/write position, count, and request fields.
    fn char_msg(
        request: CdevRequest,
        minor: u32,
        id: u32,
        position: u64,
        count: u64,
        request_code: u64,
    ) -> Message {
        let mut msg = Message {
            m_type: request.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(msg) as *mut u8;
        let (minor_off, id_off, rw_off): (usize, Option<usize>, bool) = match request {
            CdevRequest::Open | CdevRequest::Close => {
                (cwire::openclose::MINOR, Some(cwire::openclose::ID), false)
            }
            CdevRequest::Cancel => (cwire::cancel::MINOR, Some(cwire::cancel::ID), false),
            CdevRequest::Select => (cwire::select::MINOR, None, false),
            CdevRequest::Read | CdevRequest::Write | CdevRequest::Ioctl => {
                (cwire::readwrite::MINOR, Some(cwire::readwrite::ID), true)
            }
        };
        // SAFETY: pinned offsets inside the fixed-size message body.
        unsafe {
            ::core::ptr::copy_nonoverlapping(minor.to_le_bytes().as_ptr(), base.add(minor_off), 4);
            if let Some(id_off) = id_off {
                ::core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(id_off), 4);
            }
            if rw_off {
                ::core::ptr::copy_nonoverlapping(
                    position.to_le_bytes().as_ptr(),
                    base.add(cwire::readwrite::POS),
                    8,
                );
                ::core::ptr::copy_nonoverlapping(
                    count.to_le_bytes().as_ptr(),
                    base.add(cwire::readwrite::COUNT),
                    8,
                );
                ::core::ptr::copy_nonoverlapping(
                    request_code.to_le_bytes().as_ptr(),
                    base.add(cwire::readwrite::REQUEST),
                    8,
                );
            }
        }
        msg
    }

    #[test]
    fn test_open_close_track_the_counter() {
        let mut svc = service(64);
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0, 0, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Close, 0, 2, 0, 0, 0));
        assert_eq!(
            transport.replies,
            vec![
                (Endpoint(20), CDEV_REPLY_BASE, 0, 1),
                (Endpoint(20), CDEV_REPLY_BASE, 0, 2),
            ]
        );
    }

    #[test]
    fn test_read_copies_framebuffer_bytes_into_the_grant() {
        let mut svc = service(64);
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0, 0, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 0, 16, 0));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, 16, 2))
        );
        assert_eq!(&transport.buffer[..16], &[0x5A; 16]);
    }

    #[test]
    fn test_write_lands_in_the_framebuffer() {
        let mut svc = service(64);
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0, 0, 0));
        transport.buffer[..4].copy_from_slice(&[1, 2, 3, 4]);
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Write, 0, 2, 8, 4, 0));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, 4, 2))
        );
        assert_eq!(&svc.framebuffer.0[8..12], &[1, 2, 3, 4]);
    }

    #[test]
    fn test_read_past_the_end_truncates() {
        let mut svc = service(64);
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0, 0, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 60, 16, 0));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, 4, 2))
        );
    }

    #[test]
    fn test_ioctl_decodes_known_and_refuses_unknown() {
        let mut svc = service(64);
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0, 0, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Ioctl, 0, 2, 0, 0, 1));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Ioctl, 0, 3, 0, 0, 99));
        assert_eq!(
            &transport.replies[1..],
            &[
                (Endpoint(20), CDEV_REPLY_BASE, 0, 2),
                (Endpoint(20), CDEV_REPLY_BASE, -minix_types::ENOTTY, 3),
            ]
        );
    }

    #[test]
    fn test_restart_window_gates_writes_and_put_ioctls() {
        let mut svc = service(64);
        svc.has_restarted = true;
        svc.keep_displaying = true;
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0, 0, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Write, 0, 2, 0, 4, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Ioctl, 0, 3, 0, 0, 2));
        // Reads keep serving the old contents during the window.
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 4, 0, 4, 0));
        assert_eq!(
            &transport.replies[1..],
            &[
                (Endpoint(20), CDEV_REPLY_BASE, -minix_types::EAGAIN, 2),
                (Endpoint(20), CDEV_REPLY_BASE, -minix_types::EAGAIN, 3),
                (Endpoint(20), CDEV_REPLY_BASE, 4, 4),
            ]
        );
    }

    #[test]
    fn test_null_framebuffer_answers_eio_honestly() {
        let mut svc = FbService::new(FbFace::new(64, 4), NullFramebuffer);
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0, 0, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 0, 8, 0));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, -minix_types::EIO, 2))
        );
    }
}
