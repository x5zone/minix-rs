//! Service assembly: the shared block pump.
//!
//! C correspondence: `main`'s startup (`floppy.c:1270-...`: the option
//! parse, the IRQ policy, the DMA buffer, the controller reset) and the
//! block loop (`floppy_task`, `floppy.c:290-...`). The loop and the RS
//! birth handshake live in the shared driver runtime; the pump (restart
//! gate, open gate, reply shapes) lives in `minix-blockdriver::pump`.
//!
//! The driver serves no control of its own (C registers no control hook),
//! so the service is the pump plus the alarm path the device layer needs:
//! C's `f_expire_tmrs` runs on clock notifications, which the pump's alarm
//! hook receives.

use minix_blockdriver::pump::BlockService;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::Message;

use crate::face::{FdcLeg, FloppyFace};

/// The floppy service.
pub struct FloppyService<L: FdcLeg> {
    pump: BlockService<FloppyFace<L>>,
}

impl<L: FdcLeg> FloppyService<L> {
    /// A service over a fresh face.
    pub fn new(face: FloppyFace<L>) -> Self {
        FloppyService {
            pump: BlockService::new(face),
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &FloppyFace<L> {
        self.pump.face()
    }

    /// The face, mutably.
    pub fn face_mut(&mut self) -> &mut FloppyFace<L> {
        self.pump.face_mut()
    }

    /// Handle one delivered message: everything goes to the shared pump.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        self.pump.dispatch(transport, msg);
    }
}

impl<L: FdcLeg, T: DriverTransport> DriverHandler<T> for FloppyService<L> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face::FloppyFace;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_blockdriver::protocol::BdevRequest;
    use minix_blockdriver::wire;
    use minix_types::{BDEV_REPLY, Endpoint};

    /// A leg that cannot serve: the device layer is the registered seam.
    struct NullFdc;

    impl FdcLeg for NullFdc {
        fn test_read(&mut self, _drive: u32, _density: usize, _test: u64) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
        fn read_sector(&mut self, _drive: u32, _block: u64, _buf: &mut [u8]) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
        fn write_sector(&mut self, _drive: u32, _block: u64, _buf: &[u8]) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
        fn motor_running(&mut self, _drive: u32) -> bool {
            false
        }
    }

    struct FakeTransport {
        replies: Vec<(Endpoint, i32, i32, i32)>,
        grants: Vec<u8>,
    }

    impl FakeTransport {
        fn new() -> Self {
            FakeTransport {
                replies: Vec::new(),
                grants: Vec::new(),
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
            buf.copy_from_slice(&self.grants[..buf.len()]);
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            _buf: &[u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    #[test]
    fn test_the_open_reaches_the_face_through_the_pump() {
        let mut svc = FloppyService::new(FloppyFace::new(NullFdc));
        let mut transport = FakeTransport::new();
        let mut open = Message {
            m_type: BdevRequest::Open.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut open, wire::request::MINOR, 0);
        wire::write_i32(&mut open, wire::request::ID, 1);
        svc.dispatch(&mut transport, &open);
        // The device layer refuses the probe, so the open fails honestly.
        let last = transport.replies.last().unwrap();
        assert_eq!(last.1, BDEV_REPLY);
        assert_eq!(last.2, minix_types::EIO);
        let _ = vec![0u8; 0];
    }
}
