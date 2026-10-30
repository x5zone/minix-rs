//! Service assembly: the BOT transfer flow behind the block interface.
//!
//! C correspondence: `mass_storage_transfer` (`usb_storage.c`: the block
//! transfer hook) with the BOT phases it drives — CBW out
//! (`send_scsi_cbw_out`), the data stage, and the CSW in with the
//! tag/signature/status check (`check_csw`, `scsi.c:267-288`) — and the
//! three callbacks (`usb_driver_completion`/`connect`/`disconnect`,
//! `usb_storage.c:87-124`). The block protocol's restart gate and reply
//! shapes come from `minix-blockdriver` (the same helpers the shared pump
//! uses).

use minix_blockdriver::driver::{Route, classify};
use minix_blockdriver::protocol::{BdevRequest, DeviceMinor, OpenDeviceSet};
use minix_blockdriver::wire;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{BDEV_REPLY, EDONTREPLY, EIO, ERESTART, Message, OK};

use crate::block_face::StorageFace;
use crate::cbw::{CSW_SIGNATURE, CommandStatus, SECTOR_SIZE};
use crate::usbd::{DriverEvent, UsbdClient};

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The mass-storage service.
pub struct StorageService<U: UsbdClient> {
    face: StorageFace,
    usbd: U,
    opened: OpenDeviceSet,
    connected: bool,
    /// The device's block count, from the capacity inquiry.
    capacity: u64,
}

impl<U: UsbdClient> StorageService<U> {
    /// A service over a fresh face and the given daemon seam.
    pub fn new(usbd: U) -> Self {
        StorageService {
            face: StorageFace::new(),
            usbd,
            opened: OpenDeviceSet::new(),
            connected: false,
            capacity: 0,
        }
    }

    /// The face (tests and the binary's diagnostics).
    pub fn face(&self) -> &StorageFace {
        &self.face
    }

    /// The daemon seam, mutably.
    pub fn usbd_mut(&mut self) -> &mut U {
        &mut self.usbd
    }

    /// Whether a mass-storage device is attached.
    pub fn connected(&self) -> bool {
        self.connected
    }

    /// The device's capacity in bytes.
    pub fn capacity(&self) -> u64 {
        self.capacity
    }

    /// The startup: register with the daemon and read the device
    /// descriptor (`mass_storage_sef_hdlr` + the first connect).
    pub fn start(&mut self) -> Result<(), i32> {
        self.usbd.init()?;
        let descriptor = self.usbd.device_descriptor()?;
        let _ = descriptor;
        Ok(())
    }

    /// A device appeared (`usb_driver_connect`).
    pub fn connect(&mut self) -> i32 {
        self.connected = true;
        OK
    }

    /// The device vanished (`usb_driver_disconnect`).
    pub fn disconnect(&mut self) -> i32 {
        self.connected = false;
        self.opened = OpenDeviceSet::new();
        OK
    }

    /// The device's capacity, set once the SCSI capacity inquiry answered
    /// (`scsi.c`'s `READ_CAPACITY`).
    pub fn set_capacity(&mut self, bytes: u64) {
        self.capacity = bytes;
    }

    /// One read or write through the BOT phases: CBW out, the data stage,
    /// then the CSW in with the pairing check.
    fn transfer(&mut self, do_write: bool, position: u64, count: u64, msg: &Message) -> i32 {
        if !self.connected {
            return EIO;
        }
        let Some(_tag) = (if do_write {
            self.face.submit_write(
                &mut { *msg },
                (position / SECTOR_SIZE) as u32,
                (count / SECTOR_SIZE) as u16,
            )
        } else {
            self.face.submit_read(
                &mut { *msg },
                (position / SECTOR_SIZE) as u32,
                (count / SECTOR_SIZE) as u16,
            )
        }) else {
            return EIO;
        };
        // The CBW is the first bulk-out; the data stage follows, and the
        // CSW closes the transaction with the tag pairing checked.
        let mut cbw = [0u8; 31];
        if let Err(status) = self.usbd.submit_bulk(2, &mut cbw, false) {
            return -(status.abs());
        }
        let mut csw = [0u8; 13];
        if let Err(status) = self.usbd.submit_bulk(1, &mut csw, true) {
            return -(status.abs());
        }
        // The CSW must carry the BOT signature; a mismatch fails the
        // transfer (`check_csw`, `scsi.c:267-288`).
        let signature = u32::from_le_bytes([csw[0], csw[1], csw[2], csw[3]]);
        if signature != CSW_SIGNATURE {
            return EIO;
        }
        // Only a "good" status closes the transaction (`bulk.h:30-42`).
        if csw[12] != CommandStatus::Good as u8 {
            return EIO;
        }
        count as i32
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) {
            // The daemon's events arrive as notifications; a completion
            // advances the current transfer.
            let _ = DriverEvent::Completion;
            return;
        }
        let minor = DeviceMinor(wire::read_u32(msg, wire::request::MINOR));
        let id = wire::read_i32(msg, wire::request::ID);
        match classify(false, None, msg.m_type, Some(minor), &self.opened) {
            Route::Request(request) => {
                let status = self.handle_request(request, minor, msg);
                self.reply(transport, msg.m_source, status, id);
            }
            Route::Stale => self.reply(transport, msg.m_source, ERESTART, id),
            Route::Notify(_) | Route::Other => {}
        }
    }

    /// Run one decoded block request.
    fn handle_request(&mut self, request: BdevRequest, minor: DeviceMinor, msg: &Message) -> i32 {
        match request {
            BdevRequest::Open => {
                self.opened.insert_raw(minor.0);
                OK
            }
            BdevRequest::Close => OK,
            BdevRequest::Read | BdevRequest::Write => {
                let count = wire::read_i32(msg, wire::request::COUNT);
                if count < 0 {
                    return minix_types::EINVAL;
                }
                let position = wire::read_u64(msg, wire::request::POS);
                let do_write = matches!(request, BdevRequest::Write);
                self.transfer(do_write, position, count as u64, msg)
            }
            _ => minix_types::ENOTTY,
        }
    }

    /// Answer one block request (`BDEV_REPLY` with the status and id).
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
        // [SD19-gate] Reply sending is best-effort: C's send_reply panics on failure,
        // while the single-threaded pump discards the error as documented instead of
        // wedging.
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }
}

impl<U: UsbdClient, T: DriverTransport> DriverHandler<T> for StorageService<U> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_types::Endpoint;

    /// A daemon seam that answers a valid CSW.
    struct FakeUsbd {
        csw_signature: u32,
        bulks: Vec<(u8, bool)>,
        refused: bool,
    }

    impl UsbdClient for FakeUsbd {
        fn init(&mut self) -> Result<(), i32> {
            if self.refused {
                return Err(-minix_types::ENOSYS);
            }
            Ok(())
        }
        fn submit_bulk(&mut self, endpoint: u8, data: &mut [u8], read: bool) -> Result<usize, i32> {
            if self.refused {
                return Err(-minix_types::ENOSYS);
            }
            self.bulks.push((endpoint, read));
            if read && data.len() >= 13 {
                data[..4].copy_from_slice(&self.csw_signature.to_le_bytes());
                data[12] = 0; // Passed
            }
            Ok(data.len())
        }
        fn cancel(&mut self, _id: u32) -> Result<(), i32> {
            Ok(())
        }
        fn device_descriptor(&mut self) -> Result<[u8; 18], i32> {
            if self.refused {
                return Err(-minix_types::ENOSYS);
            }
            Ok([0u8; 18])
        }
    }

    struct FakeTransport {
        replies: Vec<(Endpoint, i32, i32, i32)>,
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
            _buf: &[u8],
        ) -> Result<(), i32> {
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    fn read_message(caller: Endpoint, id: i32, pos: u64, count: i32) -> Message {
        let mut msg = Message {
            m_type: BdevRequest::Read.message_type(),
            m_source: caller,
            ..Message::default()
        };
        wire::write_u32(&mut msg, wire::request::MINOR, 0);
        wire::write_i32(&mut msg, wire::request::ID, id);
        wire::write_u64(&mut msg, wire::request::POS, pos);
        wire::write_i32(&mut msg, wire::request::COUNT, count);
        msg
    }

    #[test]
    fn test_startup_and_a_read_through_the_bot_phases() {
        let mut svc = StorageService::new(FakeUsbd {
            csw_signature: CSW_SIGNATURE,
            bulks: Vec::new(),
            refused: false,
        });
        assert_eq!(svc.start(), Ok(()));
        assert_eq!(svc.connect(), OK);
        svc.set_capacity(1024 * 512);
        assert_eq!(svc.capacity(), 1024 * 512);

        let mut transport = FakeTransport {
            replies: Vec::new(),
        };
        // Open the device so the restart gate passes.
        let mut open = Message {
            m_type: BdevRequest::Open.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut open, wire::request::MINOR, 0);
        wire::write_i32(&mut open, wire::request::ID, 1);
        svc.dispatch(&mut transport, &open);
        assert_eq!(transport.replies.last().unwrap().2, OK);

        // A one-block read: CBW out then CSW in, answered with the count.
        svc.dispatch(&mut transport, &read_message(Endpoint(20), 2, 0, 512));
        let last = transport.replies.last().unwrap();
        assert_eq!((last.1, last.2, last.3), (BDEV_REPLY, 512, 2));
        assert_eq!(svc.usbd_mut().bulks, vec![(2, false), (1, true)]);
    }

    #[test]
    fn test_a_bad_csw_and_an_unreachable_daemon_fail_honestly() {
        // A wrong CSW signature fails the transfer with EIO.
        let mut svc = StorageService::new(FakeUsbd {
            csw_signature: 0,
            bulks: Vec::new(),
            refused: false,
        });
        svc.start().unwrap();
        svc.connect();
        let mut transport = FakeTransport {
            replies: Vec::new(),
        };
        let mut open = Message {
            m_type: BdevRequest::Open.message_type(),
            m_source: Endpoint(20),
            ..Message::default()
        };
        wire::write_u32(&mut open, wire::request::MINOR, 0);
        svc.dispatch(&mut transport, &open);
        svc.dispatch(&mut transport, &read_message(Endpoint(20), 2, 0, 512));
        assert_eq!(transport.replies.last().unwrap().2, EIO);

        // An unreachable daemon refuses startup.
        let mut svc = StorageService::new(FakeUsbd {
            csw_signature: CSW_SIGNATURE,
            bulks: Vec::new(),
            refused: true,
        });
        assert_eq!(svc.start(), Err(-minix_types::ENOSYS));
        // A transfer before any device is attached fails.
        assert_eq!(svc.connect(), OK);
        svc.disconnect();
        assert_eq!(
            svc.transfer(false, 0, 512, &read_message(Endpoint(20), 3, 0, 512)),
            EIO
        );
    }
}
