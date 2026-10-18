//! The shared block driver pump: one message at a time, one pump for every
//! block device.
//!
//! C correspondence: `blockdriver_task`/`blockdriver_process`
//! (`driver_st.c:52-89`), `blockdriver_process_on_thread` and the request
//! helpers (`driver.c:379-462`: `do_open`, `do_close`, `do_rdwt`,
//! `do_vrdwt`, `do_ioctl`, `blockdriver_reply`), and the character-open
//! refusal (`do_char_open`, `driver.c:383-403`). The loop shell and the RS
//! birth handshake stay in [`minix_driver_rt::runtime::DriverRuntime`]; a
//! block binary builds [`BlockService`] over its [`BlockDriver`] face and
//! hands it to the runtime.
//!
//! Two C framework paths are deliberately not modeled here and are named
//! where they would run:
//!
//! - the framework's partition ioctls (`DIOCSETP`/`DIOCGETP` and the block
//!   trace controls, `driver.c:311-360`) are not intercepted; control
//!   requests go straight to the face, which owns partition answers
//!   through its `partition()`/`geometry()` hooks;
//! - the multi-threaded worker loop (`driver_mt.c`) is not replicated —
//!   the crate note explains the single-loop rendering.
//!
//! The restart gate is C's (`driver.c:425-437`): a request for a device
//! that was not opened since the restart is answered `ERESTART`, except
//! the open itself, which records the device and then runs the hook.

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message, BDEV_REPLY, CDEV_REPLY_BASE, CDEV_REQUEST_BASE, EDONTREPLY, EINVAL, ENXIO, ERESTART};

use crate::driver::{classify, BlockDriver, Route};
use crate::protocol::{BdevRequest, DeviceMinor, OpenDeviceSet, RequestId};
use crate::wire;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// A block device's service: the face plus the restart-gate open table.
pub struct BlockService<D: BlockDriver> {
    face: D,
    opened: OpenDeviceSet,
}

impl<D: BlockDriver> BlockService<D> {
    /// A fresh service: nothing opened since startup.
    pub fn new(face: D) -> Self {
        BlockService {
            face,
            opened: OpenDeviceSet::new(),
        }
    }

    /// The card/device face.
    pub fn face(&self) -> &D {
        &self.face
    }

    /// The card/device face, mutably.
    pub fn face_mut(&mut self) -> &mut D {
        &mut self.face
    }

    /// The minors opened since startup (the restart gate's table).
    pub fn opened(&self) -> &OpenDeviceSet {
        &self.opened
    }

    /// Handle one delivered message: notifications first, then the
    /// character-open refusal, then the block request gate and switch.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) {
            match msg.m_source {
                Endpoint::HARDWARE => self
                    .face
                    .interrupt(wire::read_u32(msg, wire::notify::INTERRUPTS)),
                Endpoint::CLOCK => self.face.alarm(wire::read_i64(msg, wire::notify::TIMESTAMP)),
                _ => self.face.other(msg.m_type),
            }
            return;
        }

        // C: a character open request addressed to a block driver is
        // refused on the character lane with ENXIO, so a character node
        // over a block device cannot leave its caller blocked
        // (`do_char_open`, `driver.c:383-403`).
        if msg.m_type == CDEV_REQUEST_BASE {
            let id = wire::read_i32(msg, wire::char_openclose::ID);
            let mut reply = Message {
                m_type: CDEV_REPLY_BASE,
                ..Message::default()
            };
            wire::write_i32(&mut reply, wire::reply::STATUS, ENXIO);
            wire::write_i32(&mut reply, wire::reply::ID, id);
            let _ = transport.asynsend(msg.m_source, &mut reply);
            return;
        }

        let minor = DeviceMinor(wire::read_u32(msg, wire::request::MINOR));
        let id = wire::read_i32(msg, wire::request::ID);
        match classify(false, None, msg.m_type, Some(minor), &self.opened) {
            Route::Request(request) => {
                let status = self.handle_request(request, minor, id, msg);
                // C runs the cleanup hook after every served request
                // (`driver.c:451-454`).
                self.face.cleanup();
                self.reply(transport, msg.m_source, status, id);
            }
            // C answers ERESTART for a request on a device nobody opened
            // since the restart (`driver.c:425-433`).
            Route::Stale => self.reply(transport, msg.m_source, ERESTART, id),
            Route::Notify(_) | Route::Other => self.face.other(msg.m_type),
        }
    }

    /// Run one decoded block request and answer its status.
    fn handle_request(&mut self, request: BdevRequest, minor: DeviceMinor, id: i32, msg: &Message) -> i32 {
        match request {
            BdevRequest::Open => {
                // C records the device before running the hook
                // (`driver.c:431-437`).
                self.opened.insert_raw(minor.0);
                self.face.open(minor, wire::read_i32(msg, wire::request::ACCESS))
            }
            BdevRequest::Close => self.face.close(minor),
            BdevRequest::Read
            | BdevRequest::Write
            | BdevRequest::Gather
            | BdevRequest::Scatter => {
                let count = wire::read_i32(msg, wire::request::COUNT);
                // C refuses a negative count before any transfer runs
                // (`do_rdwt`, `driver.c:198-201`).
                if count < 0 {
                    return EINVAL;
                }
                let position = wire::read_u64(msg, wire::request::POS);
                let flags = wire::read_i32(msg, wire::request::FLAGS);
                let do_write = matches!(request, BdevRequest::Write | BdevRequest::Scatter);
                let vectored = matches!(request, BdevRequest::Gather | BdevRequest::Scatter);
                // C folds the vector forms into one hook with the vectored
                // flag, and truncates the ssize_t answer into the int reply
                // (`driver.c:441-451`).
                self.face.transfer(
                    minor,
                    do_write,
                    position,
                    count as u64,
                    flags,
                    RequestId(id),
                    vectored,
                ) as i32
            }
            BdevRequest::Ioctl => {
                let request_code = wire::read_u64(msg, wire::request::REQUEST);
                let grant = wire::read_i32(msg, wire::request::GRANT) as u64;
                let user = wire::read_i32(msg, wire::request::USER) as i64;
                self.face.ioctl(minor, request_code, grant, user)
            }
        }
    }

    /// Answer one block request; `EDONTREPLY` suppresses the reply.
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
        let _ = transport.asynsend(caller, &mut reply);
    }
}

impl<D: BlockDriver, T: DriverTransport> DriverHandler<T> for BlockService<D> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    use crate::protocol::BdevRequest;

    /// Scripted transport recording every reply.
    #[derive(Default)]
    struct Scripted {
        replies: Vec<(Endpoint, i32, i32, i32)>,
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.record(dst, msg);
            Ok(())
        }
        fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.record(dst, msg);
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.record(dst, msg);
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
            _buf: &[u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    impl Scripted {
        fn record(&mut self, dst: Endpoint, msg: &Message) {
            self.replies.push((
                dst,
                msg.m_type,
                wire::read_i32(msg, wire::reply::STATUS),
                wire::read_i32(msg, wire::reply::ID),
            ));
        }
    }

    /// Scriptable device: records every hook call.
    struct TestCard {
        open_result: i32,
        transfer_result: i64,
        opens: Vec<(u32, i32)>,
        closes: Vec<u32>,
        transfers: Vec<(u32, bool, u64, u64, i32, i32, bool)>,
        ioctls: Vec<(u32, u64, u64, i64)>,
        interrupts: u32,
        alarms: Vec<i64>,
        others: Vec<i32>,
        cleanups: u32,
    }

    impl TestCard {
        fn new() -> Self {
            TestCard {
                open_result: 0,
                transfer_result: 512,
                opens: Vec::new(),
                closes: Vec::new(),
                transfers: Vec::new(),
                ioctls: Vec::new(),
                interrupts: 0,
                alarms: Vec::new(),
                others: Vec::new(),
                cleanups: 0,
            }
        }
    }

    impl BlockDriver for TestCard {
        fn open(&mut self, minor: DeviceMinor, access: i32) -> i32 {
            self.opens.push((minor.0, access));
            self.open_result
        }
        fn close(&mut self, minor: DeviceMinor) -> i32 {
            self.closes.push(minor.0);
            0
        }
        fn transfer(
            &mut self,
            minor: DeviceMinor,
            do_write: bool,
            position: u64,
            count: u64,
            flags: i32,
            id: RequestId,
            vectored: bool,
        ) -> i64 {
            self.transfers
                .push((minor.0, do_write, position, count, flags, id.0, vectored));
            self.transfer_result
        }
        fn ioctl(&mut self, minor: DeviceMinor, request: u64, grant: u64, user: i64) -> i32 {
            self.ioctls.push((minor.0, request, grant, user));
            0
        }
        fn interrupt(&mut self, mask: u32) {
            self.interrupts |= mask;
        }
        fn alarm(&mut self, stamp: i64) {
            self.alarms.push(stamp);
        }
        fn other(&mut self, message_type: i32) {
            self.others.push(message_type);
        }
        fn cleanup(&mut self) {
            self.cleanups += 1;
        }
    }

    fn block_message(request: BdevRequest, caller: Endpoint, minor: u32, id: i32) -> Message {
        let mut msg = Message {
            m_type: request.message_type(),
            m_source: caller,
            ..Message::default()
        };
        wire::write_u32(&mut msg, wire::request::MINOR, minor);
        wire::write_i32(&mut msg, wire::request::ID, id);
        msg
    }

    fn transfer_message(
        request: BdevRequest,
        caller: Endpoint,
        minor: u32,
        id: i32,
        count: i32,
        position: u64,
        flags: i32,
    ) -> Message {
        let mut msg = block_message(request, caller, minor, id);
        wire::write_i32(&mut msg, wire::request::COUNT, count);
        wire::write_u64(&mut msg, wire::request::POS, position);
        wire::write_i32(&mut msg, wire::request::FLAGS, flags);
        msg
    }

    fn notification(source: Endpoint, interrupts: u32, stamp: i64) -> Message {
        let mut msg = Message {
            m_type: 0x1000,
            m_source: source,
            ..Message::default()
        };
        wire::write_u32(&mut msg, wire::notify::INTERRUPTS, interrupts);
        wire::write_i64(&mut msg, wire::notify::TIMESTAMP, stamp);
        msg
    }

    #[test]
    fn test_notifications_route_without_reply() {
        let mut service = BlockService::new(TestCard::new());
        let mut transport = Scripted::default();
        service.dispatch(&mut transport, &notification(Endpoint::HARDWARE, 0b1010, 0));
        service.dispatch(&mut transport, &notification(Endpoint::CLOCK, 0, 1234));
        service.dispatch(&mut transport, &notification(Endpoint::PM, 0, 0));
        assert_eq!(service.face().interrupts, 0b1010);
        assert_eq!(service.face().alarms, [1234]);
        assert_eq!(service.face().others.len(), 1);
        assert!(transport.replies.is_empty(), "notifications never reply");
    }

    #[test]
    fn test_char_open_guard_answers_enxio_on_the_char_lane() {
        let caller = Endpoint(5);
        let mut service = BlockService::new(TestCard::new());
        let mut transport = Scripted::default();
        let mut msg = Message {
            m_type: CDEV_REQUEST_BASE,
            m_source: caller,
            ..Message::default()
        };
        wire::write_i32(&mut msg, wire::char_openclose::ID, 0x77);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.len(), 1);
        assert_eq!(transport.replies[0].0, caller);
        assert_eq!(transport.replies[0].1, CDEV_REPLY_BASE);
        assert_eq!(transport.replies[0].2, ENXIO);
        assert_eq!(transport.replies[0].3, 0x77);
    }

    #[test]
    fn test_stale_request_is_refused_with_erestart() {
        let mut service = BlockService::new(TestCard::new());
        let mut transport = Scripted::default();
        let msg = transfer_message(BdevRequest::Read, Endpoint(5), 7, 9, 512, 0, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.len(), 1);
        assert_eq!(transport.replies[0].1, BDEV_REPLY);
        assert_eq!(transport.replies[0].2, ERESTART);
        assert_eq!(transport.replies[0].3, 9);
        assert!(service.face().transfers.is_empty());
    }

    #[test]
    fn test_open_records_the_device_and_admits_later_requests() {
        let mut service = BlockService::new(TestCard::new());
        let mut transport = Scripted::default();
        let mut open = block_message(BdevRequest::Open, Endpoint(5), 7, 1);
        wire::write_i32(&mut open, wire::request::ACCESS, 2);
        service.dispatch(&mut transport, &open);
        assert_eq!(service.face().opens, [(7, 2)]);
        assert!(service.opened().contains_raw(7));
        assert_eq!(transport.replies[0].2, 0);

        let read = transfer_message(BdevRequest::Read, Endpoint(5), 7, 2, 512, 1024, 0);
        service.dispatch(&mut transport, &read);
        assert_eq!(transport.replies[1].2, 512);
    }

    #[test]
    fn test_transfer_maps_direction_and_vector_flags() {
        let mut service = BlockService::new(TestCard::new());
        let mut transport = Scripted::default();
        service.dispatch(&mut transport, &block_message(BdevRequest::Open, Endpoint(5), 7, 1));
        let cases = [
            (BdevRequest::Read, false, false),
            (BdevRequest::Write, true, false),
            (BdevRequest::Gather, false, true),
            (BdevRequest::Scatter, true, true),
        ];
        for (index, (request, do_write, vectored)) in cases.iter().enumerate() {
            let msg = transfer_message(*request, Endpoint(5), 7, 10 + index as i32, 512, 4096, 3);
            service.dispatch(&mut transport, &msg);
            let last = service.face().transfers.last().unwrap();
            assert_eq!(last.0, 7);
            assert_eq!(last.1, *do_write);
            assert_eq!(last.2, 4096);
            assert_eq!(last.3, 512);
            assert_eq!(last.4, 3);
            assert_eq!(last.5, 10 + index as i32);
            assert_eq!(last.6, *vectored);
        }
    }

    #[test]
    fn test_negative_count_is_refused_without_calling_the_card() {
        let mut service = BlockService::new(TestCard::new());
        let mut transport = Scripted::default();
        service.dispatch(&mut transport, &block_message(BdevRequest::Open, Endpoint(5), 7, 1));
        let msg = transfer_message(BdevRequest::Write, Endpoint(5), 7, 2, -1, 0, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies[1].2, EINVAL);
        assert_eq!(service.face().transfers.len(), 0);
    }

    #[test]
    fn test_edontreply_suppresses_the_reply() {
        let mut service = BlockService::new(TestCard::new());
        service.face_mut().transfer_result = EDONTREPLY as i64;
        let mut transport = Scripted::default();
        service.dispatch(&mut transport, &block_message(BdevRequest::Open, Endpoint(5), 7, 1));
        let msg = transfer_message(BdevRequest::Read, Endpoint(5), 7, 2, 512, 0, 0);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.len(), 1, "only the open reply");
        assert_eq!(service.face().transfers.len(), 1);
    }

    #[test]
    fn test_cleanup_runs_after_every_served_request() {
        let mut service = BlockService::new(TestCard::new());
        let mut transport = Scripted::default();
        service.dispatch(&mut transport, &block_message(BdevRequest::Open, Endpoint(5), 7, 1));
        service.dispatch(&mut transport, &block_message(BdevRequest::Close, Endpoint(5), 7, 2));
        assert_eq!(service.face().cleanups, 2);
        assert_eq!(service.face().closes, [7]);
        assert_eq!(transport.replies[1].2, 0);
    }

    #[test]
    fn test_ioctl_forwards_request_grant_and_user() {
        let mut service = BlockService::new(TestCard::new());
        let mut transport = Scripted::default();
        service.dispatch(&mut transport, &block_message(BdevRequest::Open, Endpoint(5), 7, 1));
        let mut msg = block_message(BdevRequest::Ioctl, Endpoint(5), 7, 2);
        wire::write_u64(&mut msg, wire::request::REQUEST, 0x4000_0000);
        wire::write_i32(&mut msg, wire::request::GRANT, 42);
        wire::write_i32(&mut msg, wire::request::USER, 9);
        service.dispatch(&mut transport, &msg);
        assert_eq!(service.face().ioctls, [(7, 0x4000_0000, 42, 9)]);
        assert_eq!(transport.replies[1].2, 0);
    }
}
