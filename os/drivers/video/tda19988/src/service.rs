//! Service assembly: the TDA19988 block device.
//!
//! C correspondence: `tda19988_tab` (`tda19988.c:169-176`: open, close,
//! transfer, ioctl, partition, and the DS hook) served by
//! `blockdriver_task` (`tda19988.c:1006`). This is a bespoke pump rather
//! than the shared [`minix_blockdriver::pump::BlockService`] because C's
//! transfer does the EDID fetch *and* the caller-grant copy itself
//! (`tda19988.c:255-300`); the shared `BlockDriver` face carries no
//! transport, so this service keeps the transport in hand and mirrors
//! the block protocol shapes from `minix_blockdriver::wire` (the same
//! data-plane gap the wired storage cards document).
//!
//! The block judgments stay shared: `classify` (the restart gate) and the
//! character-open refusal come from `minix-blockdriver`.

use minix_blockdriver::driver::{classify, Route};
use minix_blockdriver::protocol::{BdevRequest, DeviceMinor, OpenDeviceSet};
use minix_blockdriver::wire;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::{
    Endpoint, Message, BDEV_REPLY, CDEV_REPLY_BASE, CDEV_REQUEST_BASE, EACCES, EDONTREPLY, EINVAL,
    ENODEV, ENOTTY, ENXIO, ERESTART, OK,
};

use crate::hdmi::{self, Bus, Hdmi};
use crate::registers::{COPYBUF_SIZE, DEVICE_SIZE, EDID_LEN, NR_DEVS};

/// The CEC bus number (`cec_bus=1` in C's environment).
pub const CEC_BUS: u32 = 1;
/// The CEC device address (`cec_address=0x34`).
pub const CEC_ADDRESS: u16 = 0x34;
/// The HDMI bus number (`hdmi_bus=1`).
pub const HDMI_BUS: u32 = 1;
/// The HDMI device address (`hdmi_address=0x70`).
pub const HDMI_ADDRESS: u16 = 0x70;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

/// The TDA19988 service: the two i2c bindings, the open table, and the
/// EDID path.
pub struct Tda19988Service {
    cec: Option<Bus>,
    hdmi: Option<Bus>,
    opened: OpenDeviceSet,
    open_count: [i32; NR_DEVS],
    access: Hdmi,
    /// The copy buffer C keeps as a file static (`copybuf`).
    copybuf: [u8; COPYBUF_SIZE],
}

impl Tda19988Service {
    /// A service with no buses yet.
    pub fn new() -> Self {
        Tda19988Service {
            cec: None,
            hdmi: None,
            opened: OpenDeviceSet::new(),
            open_count: [0; NR_DEVS],
            access: Hdmi::new(),
            copybuf: [0; COPYBUF_SIZE],
        }
    }

    /// C's fresh-start init: locate and reserve both i2c interfaces.
    pub fn start<T: DriverTransport>(&mut self, transport: &mut T) -> Result<(), i32> {
        let cec_endpoint = client::bus_endpoint(transport, CEC_BUS).ok_or(-minix_types::ENOENT)?;
        client::reserve(transport, cec_endpoint, CEC_ADDRESS)?;
        let hdmi_endpoint =
            client::bus_endpoint(transport, HDMI_BUS).ok_or(-minix_types::ENOENT)?;
        client::reserve(transport, hdmi_endpoint, HDMI_ADDRESS)?;
        self.cec = Some(Bus {
            endpoint: cec_endpoint,
            address: CEC_ADDRESS,
        });
        self.hdmi = Some(Bus {
            endpoint: hdmi_endpoint,
            address: HDMI_ADDRESS,
        });
        Ok(())
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
        //[SD19-gate] Reply sending is best-effort: C's send_reply panics on
        // failure; the single-threaded pump drops the reply instead of wedging,
        // as documented.
        #[allow(clippy::let_underscore_must_use)]
        let _ = transport.asynsend(caller, &mut reply);
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) {
            if msg.m_source == Endpoint::DS {
                self.refresh_bus(transport);
            }
            return;
        }

        // The character-open refusal (`do_char_open`, `driver.c:383-403`).
        if msg.m_type == CDEV_REQUEST_BASE {
            let id = wire::read_i32(msg, wire::char_openclose::ID);
            let mut reply = Message {
                m_type: CDEV_REPLY_BASE,
                ..Message::default()
            };
            wire::write_i32(&mut reply, wire::reply::STATUS, ENXIO);
            wire::write_i32(&mut reply, wire::reply::ID, id);
            //[SD19-gate] Same as reply: refusing a request is best-effort too.
            #[allow(clippy::let_underscore_must_use)]
            let _ = transport.asynsend(msg.m_source, &mut reply);
            return;
        }

        let minor = DeviceMinor(wire::read_u32(msg, wire::request::MINOR));
        let id = wire::read_i32(msg, wire::request::ID);
        match classify(false, None, msg.m_type, Some(minor), &self.opened) {
            Route::Request(request) => {
                let status = self.handle_request(request, minor, msg, transport);
                self.reply(transport, msg.m_source, status, id);
            }
            // A request for a device nobody opened since the restart
            // (`driver.c:425-433`).
            Route::Stale => self.reply(transport, msg.m_source, ERESTART, id),
            Route::Notify(_) | Route::Other => {}
        }
    }

    /// Run one decoded block request.
    fn handle_request<T: DriverTransport>(
        &mut self,
        request: BdevRequest,
        minor: DeviceMinor,
        msg: &Message,
        transport: &mut T,
    ) -> i32 {
        if minor.0 as usize >= NR_DEVS {
            return ENXIO;
        }
        match request {
            BdevRequest::Open => {
                // C records the device before running the hook.
                self.opened.insert_raw(minor.0);
                self.open_count[minor.0 as usize] += 1;
                OK
            }
            BdevRequest::Close => {
                if self.open_count[minor.0 as usize] < 1 {
                    return EINVAL;
                }
                self.open_count[minor.0 as usize] -= 1;
                OK
            }
            BdevRequest::Read
            | BdevRequest::Write
            | BdevRequest::Gather
            | BdevRequest::Scatter => {
                let do_write = matches!(request, BdevRequest::Write | BdevRequest::Scatter);
                self.transfer(transport, msg, do_write)
            }
            // The C table's control hook always answers "inappropriate
            // control".
            BdevRequest::Ioctl => ENOTTY,
        }
    }

    /// The EDID transfer (`tda19988_blk_transfer`).
    fn transfer<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message, do_write: bool) -> i32 {
        let count = wire::read_i32(msg, wire::request::COUNT);
        if count < 0 {
            return EINVAL;
        }
        let position = wire::read_u64(msg, wire::request::POS);
        let grant = wire::read_i32(msg, wire::request::GRANT);

        // Beyond EOF answers zero bytes.
        if position >= DEVICE_SIZE {
            return 0;
        }
        if count == 0 {
            return 0;
        }
        let mut moved = count as u64;
        if position + moved > DEVICE_SIZE {
            moved = DEVICE_SIZE - position;
        }
        if moved > COPYBUF_SIZE as u64 {
            moved = COPYBUF_SIZE as u64;
        }

        let (Some(cec), Some(hdmi)) = (self.cec, self.hdmi) else {
            return -minix_types::EIO;
        };
        if do_write {
            // The device is read-only.
            return EACCES;
        }
        match hdmi::is_display_connected(transport, cec) {
            Ok(true) => {}
            Ok(false) => return ENODEV,
            Err(_) => return -minix_types::EIO,
        }
        if let Err(status) = self.access.init(transport, cec, hdmi) {
            return status;
        }
        let mut block = [0u8; EDID_LEN];
        if let Err(status) = self.access.read_edid(transport, hdmi, &mut block) {
            return status;
        }
        self.copybuf[..EDID_LEN].copy_from_slice(&block);
        if let Err(status) = transport.copy_to_grant(msg.m_source, grant, 0, &self.copybuf[..moved as usize]) {
            return status;
        }
        // C answers the original request size, not the clamped amount
        // (`tda19988.c:298`).
        count
    }

    /// Re-resolve both bus endpoints after a data-store notification.
    fn refresh_bus<T: DriverTransport>(&mut self, transport: &mut T) {
        if let Some(cec) = self.cec
            && let Some(endpoint) = client::bus_endpoint(transport, CEC_BUS)
            && client::reserve(transport, endpoint, cec.address).is_ok()
        {
            self.cec = Some(Bus { endpoint, address: cec.address });
        }
        if let Some(hdmi) = self.hdmi
            && let Some(endpoint) = client::bus_endpoint(transport, HDMI_BUS)
            && client::reserve(transport, endpoint, hdmi.address).is_ok()
        {
            self.hdmi = Some(Bus { endpoint, address: hdmi.address });
        }
    }
}

impl Default for Tda19988Service {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: DriverTransport> DriverHandler<T> for Tda19988Service {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::vec;
    use std::vec::Vec;

    use crate::registers::{
        HDMI_CTRL_INT_EDID_MASK, HDMI_CTRL_INT_REG, HDMI_CTRL_PAGE, HDMI_CTRL_REV_HI_REG,
        HDMI_CTRL_REV_LO_REG, HDMI_EDID_DATA_REG, HDMI_EDID_PAGE, HDMI_PAGE_SELECT_REG,
        HDMI_REV_TDA19988,
    };

    /// Scripted bus: page-tracked HDMI registers, the CEC status, and a
    /// canned EDID block.
    struct FakeBus {
        endpoint: Endpoint,
        bus_present: bool,
        cec_connected: bool,
        edid_ready: bool,
        page: u8,
        registers: HashMap<(u8, u8), u8>,
        edid: [u8; EDID_LEN],
        page_writes: Vec<u8>,
        reserves: Vec<(Endpoint, u16)>,
        copies: Vec<(Endpoint, i32, Vec<u8>)>,
        replies: Vec<(Endpoint, i32, i32)>,
        block: u64,
    }

    impl FakeBus {
        fn new() -> Self {
            let mut registers = HashMap::new();
            registers.insert((HDMI_CTRL_PAGE, HDMI_CTRL_REV_LO_REG), (HDMI_REV_TDA19988 & 0xFF) as u8);
            registers.insert((HDMI_CTRL_PAGE, HDMI_CTRL_REV_HI_REG), (HDMI_REV_TDA19988 >> 8) as u8);
            let mut edid = [0u8; EDID_LEN];
            for (index, byte) in edid.iter_mut().enumerate() {
                *byte = index as u8;
            }
            FakeBus {
                endpoint: Endpoint(42),
                bus_present: true,
                cec_connected: true,
                edid_ready: true,
                page: crate::registers::HDMI_PAGELESS,
                registers,
                edid,
                page_writes: Vec::new(),
                reserves: Vec::new(),
                copies: Vec::new(),
                replies: Vec::new(),
                block: 0,
            }
        }
    }

    impl DriverTransport for FakeBus {
        fn receive(&mut self, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::EIO)
        }
        fn send(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            match msg.m_type {
                client::BUSC_I2C_RESERVE => {
                    let base = ::core::ptr::addr_of!(*msg) as *const u8;
                    let mut bytes = [0u8; 2];
                    // SAFETY: the reserve payload's first word.
                    unsafe {
                        ::core::ptr::copy_nonoverlapping(base.add(8), bytes.as_mut_ptr(), 2);
                    }
                    self.reserves.push((dst, u16::from_le_bytes(bytes)));
                    msg.m_type = 0;
                    Ok(())
                }
                client::BUSC_I2C_EXEC => {
                    let block = self.block as *mut client::ExecBlock;
                    // SAFETY: the test granted its own exec block.
                    let block = unsafe { &mut *block };
                    match block.op {
                        client::I2C_OP_WRITE_WITH_STOP => {
                            let register = block.buf[0];
                            let value = block.buf[1];
                            if register == HDMI_PAGE_SELECT_REG {
                                self.page = value;
                                self.page_writes.push(value);
                            } else if block.address == CEC_ADDRESS {
                                // The CEC enable write needs no record.
                            } else {
                                self.registers.insert((self.page, register), value);
                            }
                        }
                        client::I2C_OP_READ_WITH_STOP => {
                            let register = block.cmd[0];
                            if block.address == CEC_ADDRESS {
                                block.buf[0] = if self.cec_connected {
                                    crate::registers::CEC_STATUS_CONNECTED_MASK
                                } else {
                                    0
                                };
                            } else if self.page == HDMI_EDID_PAGE
                                && register == HDMI_EDID_DATA_REG
                                && block.buf_len == EDID_LEN as u64
                            {
                                block.buf[..EDID_LEN].copy_from_slice(&self.edid);
                            } else if self.page == HDMI_CTRL_PAGE
                                && register == HDMI_CTRL_INT_REG
                            {
                                block.buf[0] = if self.edid_ready {
                                    HDMI_CTRL_INT_EDID_MASK
                                } else {
                                    0
                                };
                            } else {
                                block.buf[0] = self
                                    .registers
                                    .get(&(self.page, register))
                                    .copied()
                                    .unwrap_or(0);
                            }
                        }
                        _ => {}
                    }
                    msg.m_type = 0;
                    Ok(())
                }
                _ => Ok(()),
            }
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            let status = wire::read_i32(msg, wire::reply::STATUS);
            self.replies.push((dst, msg.m_type, status));
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
            granter: Endpoint,
            grant: i32,
            _offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.copies.push((granter, grant, Vec::from(buf)));
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
        fn lookup_label(&mut self, label: &str) -> Option<Endpoint> {
            if self.bus_present && label == "i2c.1" {
                Some(self.endpoint)
            } else {
                None
            }
        }
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            self.block = address;
            Ok(1)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
        fn delay_micros(&mut self, _micros: u32) -> Result<(), i32> {
            Ok(())
        }
    }

    fn request(kind: BdevRequest, caller: Endpoint, minor: u32, id: i32, pos: u64, count: i32) -> Message {
        let mut msg = Message {
            m_type: kind.message_type(),
            m_source: caller,
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: test-only writers into the pinned block request layout.
        unsafe {
            ::core::ptr::copy_nonoverlapping(pos.to_le_bytes().as_ptr(), base.add(wire::request::POS), 8);
            ::core::ptr::copy_nonoverlapping(minor.to_le_bytes().as_ptr(), base.add(wire::request::MINOR), 4);
            ::core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(wire::request::ID), 4);
            ::core::ptr::copy_nonoverlapping(count.to_le_bytes().as_ptr(), base.add(wire::request::COUNT), 4);
        }
        msg
    }

    #[test]
    fn test_start_reserves_both_interfaces() {
        let mut bus = FakeBus::new();
        let mut service = Tda19988Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        assert_eq!(
            bus.reserves,
            vec![(Endpoint(42), CEC_ADDRESS), (Endpoint(42), HDMI_ADDRESS)]
        );

        let mut absent = FakeBus { bus_present: false, ..FakeBus::new() };
        let mut service = Tda19988Service::new();
        assert_eq!(service.start(&mut absent), Err(-minix_types::ENOENT));
    }

    #[test]
    fn test_open_close_and_stale_gate() {
        let mut bus = FakeBus::new();
        let mut service = Tda19988Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));

        // A request before any open answers ERESTART.
        let msg = request(BdevRequest::Read, Endpoint(5), 0, 1, 0, 128);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().1, BDEV_REPLY);
        assert_eq!(bus.replies.last().unwrap().2, ERESTART);

        let msg = request(BdevRequest::Open, Endpoint(5), 0, 2, 0, 0);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, OK);

        let msg = request(BdevRequest::Close, Endpoint(5), 0, 3, 0, 0);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, OK);
        // Closing again is invalid.
        let msg = request(BdevRequest::Close, Endpoint(5), 0, 4, 0, 0);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, EINVAL);
    }

    #[test]
    fn test_read_fetches_edid_and_copies_the_block() {
        let mut bus = FakeBus::new();
        let mut service = Tda19988Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        let msg = request(BdevRequest::Open, Endpoint(5), 0, 1, 0, 0);
        service.dispatch(&mut bus, &msg);

        let msg = request(BdevRequest::Read, Endpoint(5), 0, 2, 0, 128);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, 128, "the original request size");
        assert_eq!(bus.copies.len(), 1);
        let (caller, _grant, bytes) = &bus.copies[0];
        assert_eq!(*caller, Endpoint(5));
        assert_eq!(bytes.len(), EDID_LEN);
        assert_eq!(bytes[127], 127);

        // The page cache never writes the selector twice in a row, and
        // both the EDID page and the OTP page were selected.
        assert!(bus.page_writes.contains(&HDMI_EDID_PAGE));
        assert!(bus.page_writes.contains(&crate::registers::HDMI_HDCP_OTP_PAGE));
        for pair in bus.page_writes.windows(2) {
            assert_ne!(pair[0], pair[1], "page selector rewritten without a change");
        }
    }

    #[test]
    fn test_transfer_refusals() {
        let mut bus = FakeBus { cec_connected: false, ..FakeBus::new() };
        let mut service = Tda19988Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        let msg = request(BdevRequest::Open, Endpoint(5), 0, 1, 0, 0);
        service.dispatch(&mut bus, &msg);

        // No display attached.
        let msg = request(BdevRequest::Read, Endpoint(5), 0, 2, 0, 128);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, ENODEV);
        assert!(bus.copies.is_empty());

        bus.cec_connected = true;
        // Writing is refused outright.
        let msg = request(BdevRequest::Write, Endpoint(5), 0, 3, 0, 128);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, EACCES);

        // Beyond EOF answers zero.
        let msg = request(BdevRequest::Read, Endpoint(5), 0, 4, 128, 128);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, 0);

        // The control hook always refuses.
        let msg = request(BdevRequest::Ioctl, Endpoint(5), 0, 5, 0, 0);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, ENOTTY);
    }

    #[test]
    fn test_edid_not_ready_answers_busy_and_char_open_is_refused() {
        let mut bus = FakeBus { edid_ready: false, ..FakeBus::new() };
        let mut service = Tda19988Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        let msg = request(BdevRequest::Open, Endpoint(5), 0, 1, 0, 0);
        service.dispatch(&mut bus, &msg);

        let msg = request(BdevRequest::Read, Endpoint(5), 0, 2, 0, 128);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().2, -minix_types::EBUSY);
        assert!(bus.copies.is_empty());

        // A character open is refused on the character lane.
        let msg = Message {
            m_type: CDEV_REQUEST_BASE,
            m_source: Endpoint(6),
            ..Message::default()
        };
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.replies.last().unwrap().1, CDEV_REPLY_BASE);
        assert_eq!(bus.replies.last().unwrap().2, ENXIO);
    }
}
