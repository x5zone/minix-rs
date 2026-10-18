//! Service assembly: the control requests and the shared block pump.
//!
//! C correspondence: `vnd_ioctl` (`vnd.c:355-510`) and the loop shell
//! `blockdriver_task` (`vnd.c:592-600`). The loop and the RS birth
//! handshake live in the shared driver runtime; the pump (restart gate,
//! open gate, reply shapes) lives in `minix-blockdriver::pump`.
//!
//! The control requests are decoded here rather than in the face: C's
//! `bdr_ioctl` receives the caller's endpoint and grant, while the
//! rewrite's control hook carries the raw grant but no data plane (the
//! registered `ioctl`-plane gap). The service owns the transport, so it
//! copies the structure out of the caller's grant, calls the face, and
//! answers — the same three steps C's hook performs inline.

use minix_blockdriver::pump::BlockService;
use minix_blockdriver::wire;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{BDEV_REPLY, EDONTREPLY, Message};

use crate::backend::FileBackend;
use crate::face::VndFace;
use crate::protocol::{DIOCFLUSH, DIOCOPENCT, VNDIOCCLR, VNDIOCGET, VNDIOCSET, VndIoctl, VndUser};

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The VND service: control requests first, then the shared pump.
pub struct VndService<B: FileBackend> {
    pump: BlockService<VndFace<B>>,
}

impl<B: FileBackend> VndService<B> {
    /// A service over a fresh face.
    pub fn new(face: VndFace<B>) -> Self {
        VndService {
            pump: BlockService::new(face),
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &VndFace<B> {
        self.pump.face()
    }

    /// The face, mutably.
    pub fn face_mut(&mut self) -> &mut VndFace<B> {
        self.pump.face_mut()
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if !is_ipc_notify(msg.m_type)
            && msg.m_type == minix_blockdriver::protocol::BdevRequest::Ioctl.message_type()
        {
            let request = wire::read_u64(msg, wire::request::REQUEST);
            if let Some(status) = self.handle_control(transport, msg, request) {
                self.reply(
                    transport,
                    msg.m_source,
                    status,
                    wire::read_i32(msg, wire::request::ID),
                );
                return;
            }
        }
        self.pump.dispatch(transport, msg);
    }

    /// Run one control request; `None` means "not ours" and the request
    /// falls through to the pump (whose control hook answers "inappropriate
    /// control", C's fall-through).
    fn handle_control<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        msg: &Message,
        request: u64,
    ) -> Option<i32> {
        let caller = msg.m_source;
        let grant = wire::read_i32(msg, wire::request::GRANT);
        match request {
            VNDIOCSET => {
                // Copy the structure in, configure, copy the answer back
                // (`vnd.c:369-436`).
                let mut raw = [0u8; core::mem::size_of::<VndIoctl>()];
                if let Err(status) = transport.copy_from_grant(caller, grant, 0, &mut raw) {
                    return Some(status);
                }
                // SAFETY: the client wrote one `struct vnd_ioctl` into the
                // buffer; the type is `repr(C)` over plain words.
                let mut vnd: VndIoctl =
                    unsafe { core::ptr::read_unaligned(raw.as_ptr() as *const VndIoctl) };
                let user = wire::read_i32(msg, wire::request::USER) as i64;
                match self.pump.face_mut().configure(user as i32, &vnd) {
                    Ok(size) => {
                        vnd.size = size;
                        // SAFETY: same layout as the read above.
                        let answer = unsafe {
                            core::slice::from_raw_parts(
                                (&raw const vnd) as *const u8,
                                core::mem::size_of::<VndIoctl>(),
                            )
                        };
                        match transport.copy_to_grant(caller, grant, 0, answer) {
                            Ok(()) => Some(0),
                            Err(status) => Some(status),
                        }
                    }
                    Err(status) => Some(status),
                }
            }
            VNDIOCCLR => {
                let mut raw = [0u8; core::mem::size_of::<VndIoctl>()];
                if let Err(status) = transport.copy_from_grant(caller, grant, 0, &mut raw) {
                    return Some(status);
                }
                // SAFETY: as above.
                let vnd: VndIoctl =
                    unsafe { core::ptr::read_unaligned(raw.as_ptr() as *const VndIoctl) };
                Some(match self.pump.face_mut().clear(vnd.flags) {
                    Ok(()) => 0,
                    Err(status) => status,
                })
            }
            VNDIOCGET => {
                let (unit, dev, ino) = self.pump.face().unit_info();
                let user = VndUser {
                    unit: unit as i32,
                    _padding: 0,
                    dev,
                    ino,
                };
                // SAFETY: plain words; the client reads the same layout.
                let bytes = unsafe {
                    core::slice::from_raw_parts(
                        (&raw const user) as *const u8,
                        core::mem::size_of::<VndUser>(),
                    )
                };
                Some(match transport.copy_to_grant(caller, grant, 0, bytes) {
                    Ok(()) => 0,
                    Err(status) => status,
                })
            }
            DIOCOPENCT => {
                let count = self.pump.face().open_count().to_le_bytes();
                Some(match transport.copy_to_grant(caller, grant, 0, &count) {
                    Ok(()) => 0,
                    Err(status) => status,
                })
            }
            DIOCFLUSH => Some(match self.pump.face_mut().flush() {
                Ok(()) => 0,
                Err(status) => status,
            }),
            _ => None,
        }
    }

    /// Answer one control request (`BDEV_REPLY` with the status and id).
    fn reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: minix_types::Endpoint,
        status: i32,
        id: i32,
    ) {
        if status == EDONTREPLY {
            return;
        }
        let mut reply = Message {
            m_type: BDEV_REPLY,
            ..Message::default()
        };
        wire::write_i32(&mut reply, wire::reply::STATUS, status);
        wire::write_i32(&mut reply, wire::reply::ID, id);
        //[SD19-gate] 回复发送尽力而为：C 的 send_reply 失败即 panic；单线程泵按文档丢弃而不是卡死。
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }
}

impl<B: FileBackend, T: DriverTransport> DriverHandler<T> for VndService<B> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::FileInfo;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_types::{EPERM, Endpoint};

    /// The backend seam with a file in memory.
    struct FakeFile {
        data: Vec<u8>,
        regular: bool,
    }

    impl FileBackend for FakeFile {
        fn copyfd(&mut self, _user_endpoint: i32, _fd: i32) -> Result<i32, i32> {
            Ok(7)
        }
        fn pread(&mut self, _fd: i32, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
            let start = offset as usize;
            if start >= self.data.len() {
                return Ok(0);
            }
            let len = buf.len().min(self.data.len() - start);
            buf[..len].copy_from_slice(&self.data[start..start + len]);
            Ok(len)
        }
        fn pwrite(&mut self, _fd: i32, offset: u64, buf: &[u8]) -> Result<usize, i32> {
            let start = offset as usize;
            self.data[start..start + buf.len()].copy_from_slice(buf);
            Ok(buf.len())
        }
        fn fsync(&mut self, _fd: i32) -> Result<(), i32> {
            Ok(())
        }
        fn fstat(&mut self, _fd: i32) -> Result<FileInfo, i32> {
            Ok(FileInfo {
                size: self.data.len() as u64,
                dev: 262,
                ino: 3,
                regular: self.regular,
            })
        }
        fn close(&mut self, _fd: i32) -> Result<(), i32> {
            Ok(())
        }
    }

    /// A transport that records replies and serves the caller's grants.
    struct FakeTransport {
        replies: Vec<(Endpoint, i32, i32, i32)>,
        grants: Vec<u8>,
        copied_out: Vec<Vec<u8>>,
        refuse_copy: bool,
    }

    impl FakeTransport {
        fn new(grants: Vec<u8>) -> Self {
            FakeTransport {
                replies: Vec::new(),
                grants,
                copied_out: Vec::new(),
                refuse_copy: false,
            }
        }
    }

    impl DriverTransport for FakeTransport {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.replies.push((
                dst,
                msg.m_type,
                wire::read_i32(msg, wire::reply::STATUS),
                wire::read_i32(msg, wire::reply::ID),
            ));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            if self.refuse_copy {
                return Err(-EPERM);
            }
            buf.copy_from_slice(&self.grants[..buf.len()]);
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            if self.refuse_copy {
                return Err(-EPERM);
            }
            self.copied_out.push(buf.to_vec());
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    impl FakeTransport {
        /// The bytes of the most recent copy to the caller.
        fn last_copy(&self) -> &[u8] {
            self.copied_out.last().expect("a copy happened")
        }
    }

    /// One `BDEV_IOCTL` request with the fields set.
    fn ioctl_message(caller: Endpoint, request: u64, grant: i32, user: i32, id: i32) -> Message {
        let mut msg = Message {
            m_type: minix_blockdriver::protocol::BdevRequest::Ioctl.message_type(),
            m_source: caller,
            ..Message::default()
        };
        wire::write_u64(&mut msg, wire::request::REQUEST, request);
        wire::write_i32(&mut msg, wire::request::GRANT, grant);
        wire::write_i32(&mut msg, wire::request::USER, user);
        wire::write_i32(&mut msg, wire::request::ID, id);
        msg
    }

    /// The `struct vnd_ioctl` bytes for a set request.
    fn set_request(fildes: i32, flags: i32) -> Vec<u8> {
        let vnd = VndIoctl {
            fildes,
            flags,
            geom: Default::default(),
            size: 0,
        };
        // SAFETY: plain words.
        unsafe {
            core::slice::from_raw_parts(
                (&raw const vnd) as *const u8,
                core::mem::size_of::<VndIoctl>(),
            )
        }
        .to_vec()
    }

    /// An open request through the pump, so the restart gate records it.
    fn open_request(caller: Endpoint, id: i32) -> Message {
        let mut msg = Message {
            m_type: minix_blockdriver::protocol::BdevRequest::Open.message_type(),
            m_source: caller,
            ..Message::default()
        };
        wire::write_u32(&mut msg, wire::request::MINOR, 0);
        wire::write_i32(&mut msg, wire::request::ID, id);
        msg
    }

    fn service(len: usize) -> VndService<FakeFile> {
        VndService::new(VndFace::new(
            2,
            FakeFile {
                data: vec![0u8; len],
                regular: true,
            },
        ))
    }

    #[test]
    fn test_set_control_configures_and_answers_the_size() {
        let mut svc = service(4096);
        // The caller holds the device open (vnconfig's own open).
        let mut transport = FakeTransport::new(set_request(3, 0));
        svc.dispatch(&mut transport, &open_request(Endpoint(20), 99));
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), VNDIOCSET, 5, 21, 9),
        );

        // Two replies: the pump's open answer and the control answer.
        assert_eq!(transport.replies.len(), 2);
        assert_eq!(transport.replies[0].2, 0, "the open succeeded");
        let (dst, m_type, status, id) = transport.replies[1];
        assert_eq!((dst, m_type, status, id), (Endpoint(20), BDEV_REPLY, 0, 9));
        // The answer carries the device size back to the caller.
        let size = u64::from_le_bytes(transport.last_copy()[24..32].try_into().unwrap());
        assert_eq!(size, 4096);
        assert!(svc.face().configured());
        assert_eq!(svc.face().unit_info().0, 2);
    }

    #[test]
    fn test_get_openct_and_flush_controls() {
        let mut svc = service(512);
        let mut transport = FakeTransport::new(set_request(3, 0));
        svc.dispatch(&mut transport, &open_request(Endpoint(20), 99));
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), VNDIOCSET, 5, 21, 1),
        );

        // VNDIOCGET reports the unit triple.
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), VNDIOCGET, 5, 21, 2),
        );
        assert_eq!(transport.replies.last().unwrap().2, 0);
        let unit = i32::from_le_bytes(transport.last_copy()[0..4].try_into().unwrap());
        assert_eq!(unit, 2);
        let dev = u64::from_le_bytes(transport.last_copy()[8..16].try_into().unwrap());
        assert_eq!(dev, 262);

        // DIOCOPENCT reports the open count.
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), DIOCOPENCT, 5, 21, 3),
        );
        assert_eq!(transport.replies.last().unwrap().2, 0);
        let count = i32::from_le_bytes(transport.last_copy()[0..4].try_into().unwrap());
        assert_eq!(count, 1);

        // DIOCFLUSH on the configured unit succeeds; on an unconfigured one
        // it answers ENXIO.
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), DIOCFLUSH, 5, 21, 4),
        );
        assert_eq!(transport.replies.last().unwrap().2, 0);
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), VNDIOCCLR, 5, 21, 5),
        );
        assert_eq!(transport.replies.last().unwrap().2, 0);
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), DIOCFLUSH, 5, 21, 6),
        );
        assert_eq!(transport.replies.last().unwrap().2, minix_types::ENXIO);
    }

    #[test]
    fn test_a_failed_grant_copy_is_reported_and_a_foreign_control_falls_through() {
        let mut svc = service(512);
        let mut transport = FakeTransport::new(set_request(3, 0));
        svc.dispatch(&mut transport, &open_request(Endpoint(20), 99));
        transport.refuse_copy = true;
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), VNDIOCSET, 5, 21, 1),
        );
        assert_eq!(transport.replies.last().unwrap().2, -EPERM);

        // A control the driver does not serve goes to the pump, whose hook
        // answers "inappropriate control" (C's fall-through).
        transport.refuse_copy = false;
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), 0x1234, 5, 21, 2),
        );
        assert_eq!(transport.replies.last().unwrap().2, minix_types::ENOTTY);
    }

    #[test]
    fn test_a_read_request_reaches_the_pump() {
        let mut svc = service(512);
        let mut transport = FakeTransport::new(set_request(3, 0));
        svc.dispatch(&mut transport, &open_request(Endpoint(20), 99));
        svc.dispatch(
            &mut transport,
            &ioctl_message(Endpoint(20), VNDIOCSET, 5, 21, 1),
        );

        // A read of 16 bytes at offset 0 (the restart gate passes because
        // the device was opened above).
        let mut msg = Message {
            m_type: minix_blockdriver::protocol::BdevRequest::Read.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut msg, wire::request::MINOR, 0);
        wire::write_i32(&mut msg, wire::request::ID, 7);
        wire::write_u64(&mut msg, wire::request::POS, 0);
        wire::write_i32(&mut msg, wire::request::COUNT, 16);
        wire::write_i32(&mut msg, wire::request::GRANT, 5);
        svc.dispatch(&mut transport, &msg);
        let last = transport.replies.last().unwrap();
        assert_eq!((last.1, last.2), (BDEV_REPLY, 16));
        assert_eq!(transport.last_copy().len(), 16);
    }
}
