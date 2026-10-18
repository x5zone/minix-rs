//! Service assembly: the size control, the lower-driver binding, and the
//! shared block pump.
//!
//! C correspondence: `main`'s startup (`main.c:250-...`: option parse,
//! label/minor resolution for the two lower drivers, the driver start) and
//! the block loop (`blockdriver_task`). The loop and the RS birth
//! handshake live in the shared driver runtime; the pump (restart gate,
//! open gate, reply shapes) lives in `minix-blockdriver::pump`.
//!
//! The one control the filter serves is decoded here: `DIOCGETP` carries
//! the caller's grant and the control hook has no data plane, so the
//! service copies the `struct part_geom` back (the same three steps C's
//! hook performs inline).

use minix_blockdriver::protocol::BdevRequest;
use minix_blockdriver::pump::BlockService;
use minix_blockdriver::wire;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{BDEV_REPLY, EDONTREPLY, Endpoint, Message};

use crate::face::{DIOCGETP, FilterFace, LowerLeg};

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// `struct part_geom` (`drvlib.h`): base, size, cylinders, heads, sectors.
pub const PART_GEOM_SIZE: usize = 32;

/// The filter service: the size control first, then the pump.
pub struct FilterService<L: LowerLeg> {
    pump: BlockService<FilterFace<L>>,
}

impl<L: LowerLeg> FilterService<L> {
    /// A service over a fresh face.
    pub fn new(face: FilterFace<L>) -> Self {
        FilterService {
            pump: BlockService::new(face),
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &FilterFace<L> {
        self.pump.face()
    }

    /// The face, mutably.
    pub fn face_mut(&mut self) -> &mut FilterFace<L> {
        self.pump.face_mut()
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if !is_ipc_notify(msg.m_type) && msg.m_type == BdevRequest::Ioctl.message_type() {
            let request = wire::read_u64(msg, wire::request::REQUEST);
            if request == DIOCGETP {
                let id = wire::read_i32(msg, wire::request::ID);
                let status = self.size_control(transport, msg);
                self.reply(transport, msg.m_source, status, id);
                return;
            }
        }
        self.pump.dispatch(transport, msg);
    }

    /// `DIOCGETP`: answer the checksum-adjusted size in a `struct part_geom`
    /// (`main.c:212-230`).
    fn size_control<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) -> i32 {
        let caller = msg.m_source;
        let grant = wire::read_i32(msg, wire::request::GRANT);
        let size = self.pump.face_mut().presented_size();
        let mut geom = [0u8; PART_GEOM_SIZE];
        geom[8..16].copy_from_slice(&size.to_le_bytes());
        match transport.copy_to_grant(caller, grant, 0, &geom) {
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
        //[SD19-gate] 回复发送尽力而为：C 的 send_panic 失败即 panic；单线程泵按文档丢弃而不是卡死。
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }
}

impl<L: LowerLeg, T: DriverTransport> DriverHandler<T> for FilterService<L> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use minix_blockdriver::protocol::DeviceMinor;

    struct FakeLeg {
        size: u64,
    }

    impl LowerLeg for FakeLeg {
        fn transfer(
            &mut self,
            _position: u64,
            _buf: &mut [u8],
            size: u64,
            _do_write: bool,
        ) -> Result<u64, i32> {
            Ok(size)
        }
        fn check_drivers(&mut self) -> Result<(), i32> {
            Ok(())
        }
        fn raw_size(&mut self) -> u64 {
            self.size
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

    fn ioctl_message(request: u64, id: i32) -> Message {
        let mut msg = Message {
            m_type: BdevRequest::Ioctl.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u64(&mut msg, wire::request::REQUEST, request);
        wire::write_i32(&mut msg, wire::request::GRANT, 5);
        wire::write_i32(&mut msg, wire::request::ID, id);
        msg
    }

    #[test]
    fn test_the_size_control_answers_the_converted_size() {
        let mut svc =
            FilterService::new(FilterFace::new(FakeLeg { size: 9 * 512 }).with_sum_layout(true, 8));
        // The device must be open for the pump's gate, but the control is
        // intercepted before the pump, so no open is needed here.
        let mut transport = FakeTransport::new();
        svc.dispatch(&mut transport, &ioctl_message(DIOCGETP, 7));
        let last = transport.replies.last().unwrap();
        assert_eq!((last.1, last.2, last.3), (BDEV_REPLY, 0, 7));
        assert_eq!(transport.last_copy().len(), PART_GEOM_SIZE);
        let size = u64::from_le_bytes(transport.last_copy()[8..16].try_into().unwrap());
        assert_eq!(size, 8 * 512);
        let _ = svc.face().layout();
        let _ = DeviceMinor(0);
    }
}
