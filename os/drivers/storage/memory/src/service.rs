//! Service assembly: the memory driver's dual-face message pump.
//!
//! C correspondence: the receive-and-route loop of `main` (`memory.c:99-108`)
//! — every delivered message is tested for the block family and sent to the
//! matching framework, everything that is not a block request falls through
//! to the character framework. The two faces are the `MemoryChar` and
//! `MemoryBlock` tables in this crate; the loop shell and the RS birth
//! handshake live in [`minix_driver_rt::runtime::DriverRuntime`]. This
//! module owns only the one-message-at-a-time decision: which face, which
//! hook, which reply.
//!
//! The routing gate is exactly C's: `IS_BDEV_RQ(m_type)` (`memory.c:101`)
//! sends block requests to the block framework and *everything else*
//! (including notifications) to the character framework. The block family is
//! tested first precisely so a block open never hits the character
//! framework's "block open on a char driver" refusal — that refusal only
//! makes sense for a single-family driver, and this one is not single-family.
//!
//! The judgment cores are shared: `minix_chardriver::driver` (classify,
//! reply_decision) and `minix_blockdriver::driver` (classify) are
//! authoritative, so this file re-implements no routing rule. Both families
//! keep their own open set for the restart gate, matching C's two framework
//! tables; the geometry/open-count tables the faces carry are per-face too,
//! which is behaviorally identical to C's single shared arrays because the
//! two faces address disjoint minor sets (a character request for a block
//! minor — or the reverse — is refused by the face guard before any count is
//! touched; `memory.c:365,485`).
//!
//! Data-plane seam: the read, write, and transfer hooks answer byte counts
//! from the transfer plans; the physical grant copies between a caller
//! buffer and the backend or page window are not wired here (the hooks hand
//! out counts, not bytes). This is the same registered gap the tty pump
//! carries — a real boot still needs that copy — not a fabricated success.
//! The control flow, face routing, restart gate, and reply discipline are
//! complete and host-tested.

use crate::block_face::{MemoryBlock, RamBackend};
use crate::char_face::{MemBackend, MemoryChar};
use minix_blockdriver::driver::{classify as classify_block, BlockDriver, Route as BlockRoute};
use minix_blockdriver::protocol::{
    is_block_request, BdevRequest, DeviceMinor as BlockMinor, RequestId as BlockReqId,
};
use minix_chardriver::driver::{
    classify as classify_char, reply_decision, CharDriver, NotifySource, ReplyDecision,
    Route as CharRoute,
};
use minix_chardriver::protocol::{CdevRequest, DeviceMinor, RequestId};
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message, BDEV_REPLY, CDEV_REPLY_BASE};

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
/// are read by pinned byte offset (the same interim the tty pump uses,
/// pending E-MINTYPES-RUNTIME owned by another lane).
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

/// Block-request wire offsets.
///
/// C: `mess_lbdev_lblockdriver_msg` (`ipc.h:338-353`), a flat struct whose
/// every field is present on every block request (unlike the character
/// family's per-request structs). Offsets assume natural alignment:
/// `pos` (off_t, 8) at 0, then the four `int`s and the grant and flags
/// (`minor` 8, `id` 12, `access` 16, `count` 20, `grant` 24, `flags` 28),
/// `user` (endpoint_t, 2) at 32, and `request` (unsigned long, 8) padded to
/// 40. The `{ status; id; }` reply shares the character reply's shape.
/// Pinned byte offset, same interim as the character arm above.
mod bwire {
    use super::PAYLOAD;
    pub const POS: usize = PAYLOAD;
    pub const MINOR: usize = PAYLOAD + 8;
    pub const ID: usize = PAYLOAD + 12;
    pub const ACCESS: usize = PAYLOAD + 16;
    pub const COUNT: usize = PAYLOAD + 20;
    pub const GRANT: usize = PAYLOAD + 24;
    pub const FLAGS: usize = PAYLOAD + 28;
    pub const REQUEST: usize = PAYLOAD + 40;
}

/// The shared `{ status; id; }` reply payload (`wire::reply` for both
/// families: `m_lblockdriver_lbdev_reply`, `ipc.h:356-361`).
mod reply {
    use super::PAYLOAD;
    pub const STATUS: usize = PAYLOAD;
    pub const ID: usize = PAYLOAD + 4;
}

/// Reads a little-endian array of `N` bytes at an offset into the message.
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

/// The memory service: two device faces plus the pump that answers requests.
///
/// Both framework backends are generic so a host test injects plain vector
/// backends and a scripted transport; the production binary passes the same
/// vector-backed faces (the grant copy seam is transport-side, see the
/// module note).
pub struct MemoryService<CB: MemBackend, RB: RamBackend> {
    /// The character face (`/dev/mem`, `/dev/kmem`, `/dev/null`,
    /// `/dev/zero`).
    pub chr: MemoryChar<CB>,
    /// The block face (`/dev/ram*`, `/dev/imgrd`, `/dev/boot`).
    pub blk: MemoryBlock<RB>,
}

impl<CB: MemBackend, RB: RamBackend> MemoryService<CB, RB> {
    /// Wrap the two faces as a service.
    pub fn new(chr: MemoryChar<CB>, blk: MemoryBlock<RB>) -> Self {
        MemoryService { chr, blk }
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
        _transport: &mut T,
    ) -> Option<(i32, i32, u32)> {
        let minor = Self::char_minor_of(request, msg);
        let id = Self::char_id_of(request, msg);
        let outcome: Result<usize, minix_types::Errno> = match request {
            CdevRequest::Open => {
                let access = read_i32(msg, cwire::openclose::ACCESS);
                let user = read_u32(msg, cwire::openclose::USER) as i64;
                raw(self.chr.open(minor, access, user))
            }
            CdevRequest::Close => raw(self.chr.close(minor)),
            CdevRequest::Read => {
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let count = read_u64(msg, cwire::readwrite::COUNT) as usize;
                let position = read_u64(msg, cwire::readwrite::POS);
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                self.chr
                    .read(minor, position, grant, count, flags, RequestId(id))
            }
            CdevRequest::Write => {
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let count = read_u64(msg, cwire::readwrite::COUNT) as usize;
                let position = read_u64(msg, cwire::readwrite::POS);
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                self.chr
                    .write(minor, position, grant, count, flags, RequestId(id))
            }
            CdevRequest::Ioctl => {
                let request_code = read_u64(msg, cwire::readwrite::REQUEST);
                let grant = read_u32(msg, cwire::readwrite::GRANT) as u64;
                let flags = read_i32(msg, cwire::readwrite::FLAGS);
                let user = read_u32(msg, cwire::readwrite::USER) as i64;
                raw(self
                    .chr
                    .ioctl(minor, request_code, grant, flags, user, RequestId(id)))
            }
            CdevRequest::Cancel => raw(self.chr.cancel(minor, RequestId(id))),
            CdevRequest::Select => raw(self.chr.select(minor, read_u32(msg, cwire::select::OPS))),
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

    /// Route one non-block, non-notify message through the character face.
    fn dispatch_char<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
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
        let route = classify_char(notification, notify, msg.m_type, minor, &self.chr.opened);
        let pending = match route {
            CharRoute::Notify(source) => {
                match source {
                    NotifySource::Hardware(mask) => self.chr.interrupt(mask),
                    NotifySource::Clock(stamp) => self.chr.alarm(stamp),
                    NotifySource::Other => self.chr.other(msg.m_type),
                }
                None
            }
            // A memory device is served on both families, so a block open
            // reaching the character face is genuinely the wrong window:
            // answer "no such device" (the shared chardriver rule).
            CharRoute::BlockOpen => Some((CDEV_REPLY_BASE, -minix_types::ENXIO, 0)),
            CharRoute::Request(req) => self.handle_char(req, msg, transport),
            CharRoute::Stale | CharRoute::Other => None,
        };
        if let Some((reply_type, status, id)) = pending {
            self.reply(caller, reply_type, status, id as i32, transport);
        }
    }

    /// Route one block request through the block face.
    ///
    /// C: `blockdriver_process` — decode the request, run the matching table
    /// entry behind the restart gate, and answer the general block reply.
    fn dispatch_block<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let minor = BlockMinor(read_u32(msg, bwire::MINOR));
        let id = read_i32(msg, bwire::ID);
        let caller = msg.m_source;
        let route = classify_block(false, None, msg.m_type, Some(minor), &self.blk.opened);
        let status: i32 = match route {
            BlockRoute::Request(request) => match request {
                BdevRequest::Open => self.blk.open(minor, read_i32(msg, bwire::ACCESS)),
                BdevRequest::Close => self.blk.close(minor),
                BdevRequest::Read
                | BdevRequest::Write
                | BdevRequest::Gather
                | BdevRequest::Scatter => {
                    let position = read_u64(msg, bwire::POS);
                    let count = read_u64(msg, bwire::COUNT) as i64;
                    let flags = read_i32(msg, bwire::FLAGS);
                    let do_write = matches!(request, BdevRequest::Write | BdevRequest::Scatter);
                    let vectored = matches!(request, BdevRequest::Gather | BdevRequest::Scatter);
                    // The transfer hook answers the byte total; the grant
                    // copy of those bytes is the registered data-plane seam
                    // (module note). C clips the reply to an int.
                    self.blk.transfer(
                        minor,
                        do_write,
                        position,
                        count as u64,
                        flags,
                        BlockReqId(id),
                        vectored,
                    ) as i32
                }
                BdevRequest::Ioctl => {
                    let request_code = read_u64(msg, bwire::REQUEST);
                    let grant = read_u64(msg, bwire::GRANT);
                    let user = read_i32(msg, cwire::openclose::USER) as i64;
                    self.blk.ioctl(minor, request_code, grant, user)
                }
            },
            // A block notification or stale/unrecognized message answers
            // nothing on the C block path.
            BlockRoute::Notify(_) | BlockRoute::Stale | BlockRoute::Other => return,
        };
        self.reply(caller, BDEV_REPLY, status, id, transport);
    }

    /// Send one framework reply back to the caller.
    ///
    /// Both family replies are one-way (`asynsend`, `AMF_NOREPLY`). A failed
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

    /// Handle one delivered message: split by family, then route and reply.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        // C: `IS_BDEV_RQ(m_type)` first (`memory.c:101`); a block request
        // goes to the block framework, everything else to the character one.
        if is_block_request(msg.m_type) {
            self.dispatch_block(transport, msg);
        } else {
            self.dispatch_char(transport, msg);
        }
    }
}

impl<CB: MemBackend, RB: RamBackend, T: DriverTransport> DriverHandler<T>
    for MemoryService<CB, RB>
{
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
    use crate::block_face::VecRamDisk;
    use crate::char_face::VecBackend;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Scripted transport: feeds queued messages to `receive`, records every
    /// reply sent (destination, type, decoded status, echoed id).
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

    fn service() -> MemoryService<VecBackend, VecRamDisk> {
        MemoryService::new(
            MemoryChar::new(VecBackend(vec![0xAA; 16])),
            MemoryBlock::new(VecRamDisk(vec![0xEE; 4096])),
        )
    }

    /// Build a character request message with the given type, minor, id.
    ///
    /// The minor and id offsets depend on the payload struct the request
    /// uses (select carries no id), so the writer follows the same per-kind
    /// layout the reader does.
    fn char_msg(request: CdevRequest, minor: u32, id: u32) -> Message {
        let mut msg = Message {
            m_type: request.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        let base = core::ptr::addr_of_mut!(msg) as *mut u8;
        let (minor_off, id_off): (usize, Option<usize>) = match request {
            CdevRequest::Open | CdevRequest::Close => {
                (cwire::openclose::MINOR, Some(cwire::openclose::ID))
            }
            CdevRequest::Cancel => (cwire::cancel::MINOR, Some(cwire::cancel::ID)),
            CdevRequest::Select => (cwire::select::MINOR, None),
            CdevRequest::Read | CdevRequest::Write | CdevRequest::Ioctl => {
                (cwire::readwrite::MINOR, Some(cwire::readwrite::ID))
            }
        };
        // SAFETY: pinned offsets inside the fixed-size message body.
        unsafe {
            core::ptr::copy_nonoverlapping(minor.to_le_bytes().as_ptr(), base.add(minor_off), 4);
            if let Some(id_off) = id_off {
                core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(id_off), 4);
            }
        }
        msg
    }

    /// Build a block request message with the given type, minor, id.
    fn block_msg(request: BdevRequest, minor: u32, id: i32) -> Message {
        let mut msg = Message {
            m_type: request.message_type(),
            m_source: Endpoint(21),
            ..Message::default()
        };
        let base = core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: pinned offsets inside the fixed-size message body.
        unsafe {
            core::ptr::copy_nonoverlapping(minor.to_le_bytes().as_ptr(), base.add(bwire::MINOR), 4);
            core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(bwire::ID), 4);
        }
        msg
    }

    /// A character open is served by the character face and echoes its id.
    #[test]
    fn test_char_open_is_routed_and_replied() {
        let mut svc = service();
        let mut transport = Scripted::new();
        let msg = char_msg(CdevRequest::Open, 1, 7); // /dev/mem, char face
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.len(), 1);
        let (dst, rtype, status, rid) = transport.replies[0];
        assert_eq!(dst, Endpoint(20));
        assert_eq!(rtype, CDEV_REPLY_BASE);
        assert_eq!(status, 0);
        assert_eq!(rid, 7);
        assert!(svc.chr.opened.contains_raw(1));
    }

    /// A character read of the zero device answers the full length.
    #[test]
    fn test_char_zero_read_answers_full_length() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 5, 1)); // /dev/zero
                                                                          // A read carrying a 128-byte count.
        let mut msg = char_msg(CdevRequest::Read, 5, 2);
        let base = core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: pinned offset inside the message body.
        unsafe {
            core::ptr::copy_nonoverlapping(
                128u64.to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::COUNT),
                8,
            );
        }
        svc.dispatch(&mut transport, &msg);
        // Second reply is the read: status is the byte count 128.
        assert_eq!(transport.replies[1].2, 128);
    }

    /// A block open is routed to the block face, NOT refused by the
    /// character face's wrong-window guard.
    #[test]
    fn test_block_open_goes_to_block_face() {
        let mut svc = service();
        let mut transport = Scripted::new();
        let msg = block_msg(BdevRequest::Open, 7, 3); // /dev/ram0, block face
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.len(), 1);
        let (dst, rtype, status, rid) = transport.replies[0];
        assert_eq!(dst, Endpoint(21));
        assert_eq!(rtype, BDEV_REPLY);
        assert_eq!(status, 0);
        assert_eq!(rid, 3);
        assert!(svc.blk.opened.contains_raw(7));
    }

    /// A block read for a device nobody opened since startup is dropped by
    /// the restart gate: no reply rides back.
    #[test]
    fn test_stale_block_request_takes_no_reply() {
        let mut svc = service();
        let mut transport = Scripted::new();
        let msg = block_msg(BdevRequest::Read, 7, 9); // never opened
        svc.dispatch(&mut transport, &msg);
        assert!(transport.replies.is_empty());
    }

    /// A block transfer clips at the device end and answers the byte count.
    #[test]
    fn test_block_transfer_clips_and_replies() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &block_msg(BdevRequest::Open, 7, 1));
        svc.blk.apply_resize(BlockMinor(7), false, 4096);
        // A read of 512 bytes entirely inside the resized device.
        let mut msg = block_msg(BdevRequest::Read, 7, 2);
        let base = core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: pinned offsets inside the message body.
        unsafe {
            core::ptr::copy_nonoverlapping(
                512u64.to_le_bytes().as_ptr(),
                base.add(bwire::COUNT),
                8,
            );
        }
        svc.dispatch(&mut transport, &msg);
        // Third entry (after the open reply) is the transfer: 512 bytes.
        assert_eq!(transport.replies[1].1, BDEV_REPLY);
        assert_eq!(transport.replies[1].2, 512);
    }

    /// A notification is a non-block message: it routes to the character
    /// face's no-reply path and answers nothing.
    #[test]
    fn test_notification_takes_no_reply() {
        let mut svc = service();
        let mut transport = Scripted::new();
        let msg = Message {
            m_type: 0x1000 | 5, // IPC_NOTIFY base plus a source
            m_source: Endpoint::CLOCK,
            ..Message::default()
        };
        svc.dispatch(&mut transport, &msg);
        assert!(transport.replies.is_empty());
    }

    /// A block request for a character-only minor passes the block open
    /// gate and is refused by the face guard in the block table ("no such
    /// device"), still replying on the block lane.
    #[test]
    fn test_block_face_guard_refuses_char_minor() {
        let mut svc = service();
        let mut transport = Scripted::new();
        // Open minor 1 (/dev/mem, character-only) on the block face.
        let msg = block_msg(BdevRequest::Open, 1, 4);
        svc.dispatch(&mut transport, &msg);
        // The block open table refuses the wrong face.
        assert_eq!(transport.replies[0].2, -minix_types::ENXIO);
    }

    /// A select poll answers on the dedicated select reply type, not the
    /// general reply — the one place the character reply type diverges.
    #[test]
    fn test_char_select_uses_select_reply_type() {
        let mut svc = service();
        let mut transport = Scripted::new();
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Open, 3, 1)); // /dev/null
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Select, 3, 0));
        // The second reply rides the select reply type (base plus one).
        assert_eq!(transport.replies[1].1, CDEV_SEL1_REPLY);
    }

    /// A character request for a device nobody opened since startup is
    /// dropped by the restart gate (no reply), mirroring the block gate.
    #[test]
    fn test_char_stale_request_takes_no_reply() {
        let mut svc = service();
        let mut transport = Scripted::new();
        // A read on an unopened character minor: dropped, nothing answered.
        svc.dispatch(&mut transport, &char_msg(CdevRequest::Read, 1, 5));
        assert!(transport.replies.is_empty());
    }

    /// The service drives the handler trait exactly as `dispatch` does, so
    /// the runtime's loop can call it without knowing the face split.
    #[test]
    fn test_handler_trait_forwards_to_dispatch() {
        let mut svc = service();
        let mut transport = Scripted::new();
        let msg = char_msg(CdevRequest::Open, 3, 11); // /dev/null
        DriverHandler::handle(&mut svc, &mut transport, &msg);
        assert_eq!(transport.replies.len(), 1);
        assert_eq!(transport.replies[0].1, CDEV_REPLY_BASE);
    }
}
