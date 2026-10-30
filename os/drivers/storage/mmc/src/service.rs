//! Service assembly: the open-count control and the shared block pump.
//!
//! C correspondence: `main`'s startup (`mmcblk.c:600-...`: the host
//! selection, the slot scan, the announcements) and the block loop
//! (`mmcblk.c:96-130`). The loop and the RS birth handshake live in the
//! shared driver runtime; the pump (restart gate, open gate, reply shapes)
//! lives in `minix-blockdriver::pump`.
//!
//! The open-count control is decoded here: it carries the caller's grant
//! and the control hook has no data plane (`mmcblk.c:455-460`).

use minix_blockdriver::protocol::BdevRequest;
use minix_blockdriver::pump::BlockService;
use minix_blockdriver::wire;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{BDEV_REPLY, EDONTREPLY, Endpoint, Message};

use crate::face::{DIOCOPENCT, MmcFace, MmcHost};

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The SD/MMC service.
pub struct MmcService<H: MmcHost> {
    pump: BlockService<MmcFace<H>>,
}

impl<H: MmcHost> MmcService<H> {
    /// A service over a fresh face.
    pub fn new(face: MmcFace<H>) -> Self {
        MmcService {
            pump: BlockService::new(face),
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &MmcFace<H> {
        self.pump.face()
    }

    /// The face, mutably.
    pub fn face_mut(&mut self) -> &mut MmcFace<H> {
        self.pump.face_mut()
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if !is_ipc_notify(msg.m_type) && msg.m_type == BdevRequest::Ioctl.message_type() {
            let request = wire::read_u64(msg, wire::request::REQUEST);
            if request == DIOCOPENCT {
                let id = wire::read_i32(msg, wire::request::ID);
                let status = self.open_count_control(transport, msg);
                self.reply(transport, msg.m_source, status, id);
                return;
            }
        }
        self.pump.dispatch(transport, msg);
    }

    /// `DIOCOPENCT`: copy the slot's open count back
    /// (`mmcblk.c:455-460`).
    fn open_count_control<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) -> i32 {
        let caller = msg.m_source;
        let grant = wire::read_i32(msg, wire::request::GRANT);
        let minor = wire::read_u32(msg, wire::request::MINOR) as i32;
        let Some(slot) = MmcFace::<H>::slot_of(minor) else {
            return minix_types::EINVAL;
        };
        let count = self.pump.face().open_count(slot).to_le_bytes();
        match transport.copy_to_grant(caller, grant, 0, &count) {
            Ok(()) => 0,
            Err(status) => status,
        }
    }

    /// Answer one control request (`BDEV_REPLY` with the status and id).
    fn reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
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
        //[SD19-gate] Reply sending is best-effort: C's send_panic panics on
        // failure; the single-threaded pump drops the reply instead of wedging,
        // as documented.
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }
}

impl<H: MmcHost, T: DriverTransport> DriverHandler<T> for MmcService<H> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use minix_types::BDEV_REPLY as REPLY;

    struct NullHost;

    impl MmcHost for NullHost {
        fn card_detect(&mut self, _slot: usize) -> bool {
            false
        }
        fn card_initialize(&mut self, _slot: usize) -> bool {
            false
        }
        fn block_size(&mut self, _slot: usize) -> u32 {
            0
        }
        fn read_blocks(
            &mut self,
            _slot: usize,
            _block: u64,
            _count: u32,
            _buf: &mut [u8],
        ) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
        fn write_blocks(
            &mut self,
            _slot: usize,
            _block: u64,
            _count: u32,
            _buf: &[u8],
        ) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
    }

    struct FakeTransport {
        copied_out: Vec<Vec<u8>>,
        replies: Vec<(Endpoint, i32, i32, i32)>,
    }

    impl FakeTransport {
        fn new() -> Self {
            FakeTransport {
                copied_out: Vec::new(),
                replies: Vec::new(),
            }
        }
        fn last_copy(&self) -> &[u8] {
            self.copied_out.last().expect("a copy happened")
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
            _buf: &mut [u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.copied_out.push(buf.to_vec());
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    #[test]
    fn test_the_open_count_control_answers_the_slots_count() {
        let mut svc = MmcService::new(MmcFace::new(NullHost));
        let mut transport = FakeTransport::new();
        let mut msg = Message {
            m_type: BdevRequest::Ioctl.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u64(&mut msg, wire::request::REQUEST, DIOCOPENCT);
        wire::write_u32(&mut msg, wire::request::MINOR, 0);
        wire::write_i32(&mut msg, wire::request::GRANT, 5);
        wire::write_i32(&mut msg, wire::request::ID, 3);
        svc.dispatch(&mut transport, &msg);
        let last = transport.replies.last().unwrap();
        assert_eq!((last.1, last.2, last.3), (REPLY, 0, 3));
        assert_eq!(
            i32::from_le_bytes(transport.last_copy()[..4].try_into().unwrap()),
            0
        );

        // An unknown slot is refused.
        let mut msg2 = msg;
        wire::write_u32(&mut msg2, wire::request::MINOR, 400);
        svc.dispatch(&mut transport, &msg2);
        assert_eq!(transport.replies.last().unwrap().2, minix_types::EINVAL);
    }
}
