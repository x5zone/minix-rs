//! Service assembly: the random driver's receive-classify-dispatch pump.
//!
//! C correspondence: `main()` (`main.c:120-268`) — every delivered message
//! is a character request or a notification over the single `/dev/random`
//! minor; the loop shell and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`]. This module owns the
//! one-message-at-a-time decision plus the two data-plane legs the face
//! leaves to the service: reads generate key-stream bytes into a staging
//! slice and copy them out to the caller, writes copy entropy in and feed
//! pool zero as trusted bits (a reseed fires once pool zero reaches the
//! sample threshold, `random.c:216-232`).
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

use crate::char_face::RandomFace;
use crate::core::BlockCipher;
use crate::device::TRANSFER_CHUNK;
use crate::pool::{PoolHash, PoolSet, POOL_COUNT};

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
///
/// C: `mess_vfs_lchardriver_{openclose,cancel,select,readwrite}` and
/// `m_notify` (`ipc.h:2206-2258`). The same pinned-offset interim the
/// memory/tty/log/readclock pumps use (pending E-MINTYPES-RUNTIME).
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

/// The random service: the generator face plus its pool set and hashes.
///
/// The cipher and the pool hash are type parameters: production wires
/// AES-256 and SHA-256, host tests the folding doubles.
pub struct RandomService<C: BlockCipher, H: PoolHash> {
    /// The chardriver face over `/dev/random`.
    pub face: RandomFace<C>,
    /// The 32 entropy pools plus the reseed sample count.
    pub pools: PoolSet,
    /// One running hash per pool (C `pool_ctx[NR_POOLS]`).
    pub hashes: [H; POOL_COUNT],
}

impl<C: BlockCipher, H: PoolHash> RandomService<C, H> {
    /// Wrap a face and its pool set as a service.
    pub fn new(face: RandomFace<C>, pools: PoolSet, hashes: [H; POOL_COUNT]) -> Self {
        RandomService {
            face,
            pools,
            hashes,
        }
    }

    /// Reseed the generator when pool zero holds enough samples.
    ///
    /// C: `reseed` (`random.c:216-232`) — finalize every pool digest, hash
    /// the digests together, and rekey the keystream. The emptied pool
    /// hash zero doubles as the finalizer (it is reset by the snapshot).
    fn reseed_if_due(&mut self) {
        if !self.pools.reseed_due() {
            return;
        }
        let mut digests = [[0u8; 32]; POOL_COUNT];
        for (hash, out) in self.hashes.iter_mut().zip(digests.iter_mut()) {
            hash.snapshot_reset(out);
        }
        let refs: [&[u8; 32]; POOL_COUNT] = ::core::array::from_fn(|i| &digests[i]);
        self.face
            .generator
            .reseed(&mut self.hashes[0], &refs);
        self.pools.note_reseeded();
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
    ///
    /// Read and write run the face first (the seed/open gates), then the
    /// data leg: reads chunk the key stream out to the caller's grant,
    /// writes copy entropy in and feed pool zero.
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
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                match self
                    .face
                    .read(minor, 0, grant, count, flags, RequestId(id))
                {
                    Ok(size) => match self.stream_out(transport, caller, grant as i32, size) {
                        Ok(()) => Ok(size),
                        Err(code) => Err(minix_types::Errno::from_i32(code)),
                    },
                    Err(error) => Err(error),
                }
            }
            CdevRequest::Write => {
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let count = read_u64(msg, cwire::readwrite::COUNT) as usize;
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                match self
                    .face
                    .write(minor, 0, grant, count, flags, RequestId(id))
                {
                    Ok(size) => match self.absorb_entropy(transport, caller, grant as i32, size) {
                        Ok(()) => Ok(size),
                        Err(code) => Err(minix_types::Errno::from_i32(code)),
                    },
                    Err(error) => Err(error),
                }
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

    /// Generate `size` key-stream bytes in transfer chunks and copy them
    /// out to the caller's grant (C `r_read`'s chunk loop, `main.c:120-143`).
    fn stream_out<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        grant: i32,
        size: usize,
    ) -> Result<(), i32> {
        let mut staging = [0u8; TRANSFER_CHUNK];
        let mut offset = 0u64;
        let mut remaining = size;
        while remaining > 0 {
            let chunk = remaining.min(TRANSFER_CHUNK);
            self.face.generator.generate(&self.face.cipher, &mut staging[..chunk]);
            transport.copy_to_grant(caller, grant, offset, &staging[..chunk])?;
            offset += chunk as u64;
            remaining -= chunk;
        }
        Ok(())
    }

    /// Copy `size` entropy bytes in from the caller and feed pool zero as
    /// trusted bits; a due reseed rekeys the generator
    /// (C `r_write`/`random_putbytes`, `main.c:151-172`, `random.c:115-128`).
    fn absorb_entropy<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        grant: i32,
        size: usize,
    ) -> Result<(), i32> {
        let mut staging = [0u8; TRANSFER_CHUNK];
        let mut offset = 0u64;
        let mut remaining = size;
        while remaining > 0 {
            let chunk = remaining.min(TRANSFER_CHUNK);
            transport.copy_from_grant(caller, grant, offset, &mut staging[..chunk])?;
            if !self.pools.feed_trusted(&mut self.hashes, &staging[..chunk]) {
                return Err(-minix_types::EIO);
            }
            offset += chunk as u64;
            remaining -= chunk;
        }
        self.reseed_if_due();
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

impl<C: BlockCipher, H: PoolHash, T: DriverTransport> DriverHandler<T> for RandomService<C, H> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::FoldCipher;
    use crate::pool::FoldHash;
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

    fn service() -> RandomService<FoldCipher, FoldHash> {
        RandomService::new(
            RandomFace::new(FoldCipher),
            PoolSet::new(),
            ::core::array::from_fn(|_| FoldHash::new()),
        )
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
    fn test_open_answers_zero() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        assert_eq!(
            transport.replies,
            vec![(Endpoint(20), CDEV_REPLY_BASE, 0, 1)]
        );
    }

    #[test]
    fn test_unseeded_read_answers_eagain_never_parks() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 64));
        assert_eq!(
            transport.replies,
            vec![
                (Endpoint(20), CDEV_REPLY_BASE, 0, 1),
                (Endpoint(20), CDEV_REPLY_BASE, minix_types::EAGAIN, 2),
            ]
        );
    }

    #[test]
    fn test_write_feeds_the_pool_and_reseeds_at_the_threshold() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        // MIN_SAMPLES is 256 and trusted bytes count eight samples each:
        // thirty-two bytes trip the reseed.
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Write, 0, 2, 32));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, 32, 2))
        );
        assert!(svc.face.generator.is_seeded());
        assert_eq!(svc.pools.samples(), 0);
    }

    #[test]
    fn test_seeded_read_streams_the_key_into_the_grant() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Write, 0, 2, 32));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 3, 24));
        assert_eq!(
            transport.replies.last(),
            Some(&(Endpoint(20), CDEV_REPLY_BASE, 24, 3))
        );
        assert!(transport.buffer[..24].iter().any(|byte| *byte != 0));
    }

    #[test]
    fn test_select_answers_on_the_sel1_type() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 0, 1, 0));
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Select, 0, 0, 0));
        assert_eq!(transport.replies.len(), 2);
        assert_eq!(transport.replies[1].1, CDEV_SEL1_REPLY);
    }

    #[test]
    fn test_read_before_open_is_dropped() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 0, 2, 8));
        assert!(transport.replies.is_empty());
    }
}
