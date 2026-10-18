//! Service assembly: the BMP085 sensor's receive-classify-dispatch pump.
//!
//! C correspondence: `bmp085_tab` (`bmp085.c:153-165`: the read hook,
//! which is the only data path) served by `chardriver_task`
//! (`bmp085.c:580`); the fresh-start init (bus lookup, device
//! reservation, chip-id check, calibration read, `bmp085.c:400-500`);
//! the read handler (`bmp085.c:421-465`: measure, format, clamp by
//! position and count, copy to the caller's grant); and the DS
//! notification handler (`bmp085.c:467-485`: refresh the bus endpoint).
//! The loop shell and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`].
//!
//! Registered seams: the data-store subscription that would make bus
//! restarts visible (`i2cdriver_subscribe_bus_updates`) is not wired —
//! without it the DS notification arm below never fires; the conversion
//! delay rides [`minix_driver_rt::transport::DriverTransport::delay_micros`],
//! whose production implementation needs the kernel delay call (a
//! transport without it reports `ENOSYS` and the read fails honestly).

use alloc::format;

use minix_chardriver::protocol::CdevRequest;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::{Endpoint, Message, CDEV_REPLY_BASE, EBADF, EINVAL, EIO, ENOTTY, OK};

use crate::measure::{self, Calibration, PressureCommand, MODE_STANDARD, PRESSURE_COMMANDS};

/// The message union payload begins at byte 8 of [`Message`].
const PAYLOAD: usize = 8;

/// The bus number (`bus=1` in C's environment; the generic build carries
/// no environment, so the documented default stands).
pub const BUS_NUMBER: u32 = 1;

/// The sensor address (`address=0x77` in C's environment; the only value
/// in C's accepted-address table).
pub const SENSOR_ADDRESS: u16 = 0x77;

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
    }
    pub mod readwrite {
        use super::PAYLOAD;
        pub const POSITION: usize = PAYLOAD;
        pub const GRANT: usize = PAYLOAD + 8;
        pub const COUNT: usize = PAYLOAD + 16;
        pub const ID: usize = PAYLOAD + 36;
    }
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

fn read_u32(msg: &Message, offset: usize) -> u32 {
    u32::from_le_bytes(read_bytes(msg, offset))
}

fn read_u64(msg: &Message, offset: usize) -> u64 {
    u64::from_le_bytes(read_bytes(msg, offset))
}

fn write_reply(msg: &mut Message, status: i32, id: i32) {
    let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: the reply payload is two 4-byte words at the union start,
    // inside the message's fixed-size body.
    unsafe {
        ::core::ptr::copy_nonoverlapping(status.to_le_bytes().as_ptr(), base.add(PAYLOAD), 4);
        ::core::ptr::copy_nonoverlapping(id.to_le_bytes().as_ptr(), base.add(PAYLOAD + 4), 4);
    }
}

/// The sensor service: bus binding, calibration, and the read pump.
pub struct Bmp085Service {
    bus: Option<Endpoint>,
    calibration: Option<Calibration>,
    address: u16,
    command: PressureCommand,
}

impl Bmp085Service {
    /// A service with no bus yet and the standard pressure mode.
    pub fn new() -> Self {
        Bmp085Service {
            bus: None,
            calibration: None,
            address: SENSOR_ADDRESS,
            command: PRESSURE_COMMANDS[MODE_STANDARD],
        }
    }

    /// Select the pressure mode (the C build hard-codes the standard
    /// mode; this exists for tests and future environment plumbing).
    pub fn set_command(&mut self, command: PressureCommand) {
        self.command = command;
    }

    /// The C fresh-start init: locate the bus, reserve the device, check
    /// the chip id, and read the calibration.
    ///
    /// Returns the failing step's error; the driver must not enter the
    /// loop (C returns `EXIT_FAILURE`).
    pub fn start<T: DriverTransport>(&mut self, transport: &mut T) -> Result<(), i32> {
        let endpoint = client::bus_endpoint(transport, BUS_NUMBER).ok_or(-minix_types::ENOENT)?;
        client::reserve(transport, endpoint, self.address)?;
        let chip = measure::chip_id(transport, endpoint, self.address)?;
        if chip != measure::BMP085_CHIPID {
            return Err(-minix_types::ENODEV);
        }
        let calibration = Calibration::read(transport, endpoint, self.address)?;
        self.bus = Some(endpoint);
        self.calibration = Some(calibration);
        Ok(())
    }

    /// Send one `{ status, id }` character reply.
    fn reply<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        status: i32,
        id: u32,
    ) {
        let mut message = Message {
            m_type: CDEV_REPLY_BASE,
            ..Message::default()
        };
        write_reply(&mut message, status, id as i32);
        let _ = transport.asynsend(caller, &mut message);
    }

    /// Handle one delivered message.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        if is_ipc_notify(msg.m_type) {
            if msg.m_source == Endpoint::DS {
                self.refresh_bus(transport);
            }
            return;
        }
        let caller = msg.m_source;
        let Some(request) = CdevRequest::decode(msg.m_type) else {
            self.reply(transport, caller, EINVAL, 0);
            return;
        };
        match request {
            CdevRequest::Open | CdevRequest::Close => {
                let id = read_u32(msg, cwire::openclose::ID);
                self.reply(transport, caller, OK, id);
            }
            CdevRequest::Read => self.handle_read(transport, msg),
            // The C table registers no write hook; answering an
            // input-output error is the honest reading of the gap.
            CdevRequest::Write => {
                let id = read_u32(msg, cwire::readwrite::ID);
                self.reply(transport, caller, EIO, id);
            }
            CdevRequest::Ioctl => {
                let id = read_u32(msg, cwire::readwrite::ID);
                self.reply(transport, caller, ENOTTY, id);
            }
            CdevRequest::Select => self.reply(transport, caller, EBADF, 0),
            // `chardriver_task`'s default for a missing cancel hook.
            CdevRequest::Cancel => {}
        }
    }

    /// Read the sensor and render C's status buffer (`bmp085_read`).
    fn handle_read<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let caller = msg.m_source;
        let id = read_u32(msg, cwire::readwrite::ID);
        let position = read_u64(msg, cwire::readwrite::POSITION);
        let grant = read_u32(msg, cwire::readwrite::GRANT) as i32;
        let size = read_u64(msg, cwire::readwrite::COUNT) as usize;

        let (Some(bus), Some(calibration)) = (self.bus, self.calibration) else {
            self.reply(transport, caller, EIO, id);
            return;
        };
        let (temperature, pressure) =
            match measure::measure(transport, bus, self.address, &calibration, self.command) {
                Ok(pair) => pair,
                Err(_) => {
                    self.reply(transport, caller, EIO, id);
                    return;
                }
            };

        // C: `"%-16s: %d.%01d\n%-16s: %d\n"` (`bmp085.c:432-435`). The
        // division and remainder are C's: -5 tenths renders "0.-5".
        let text = format!(
            "{:<16}: {}.{:01}\n{:<16}: {}\n",
            "TEMPERATURE",
            temperature / 10,
            temperature % 10,
            "PRESSURE",
            pressure
        );
        let bytes = text.as_bytes();
        let length = bytes.len() as u64;
        if position >= length {
            self.reply(transport, caller, 0, id);
            return;
        }
        let mut count = size;
        if position + count as u64 > length {
            count = (length - position) as usize;
        }
        let start = position as usize;
        match transport.copy_to_grant(caller, grant, 0, &bytes[start..start + count]) {
            Ok(()) => self.reply(transport, caller, count as i32, id),
            Err(error) => self.reply(transport, caller, error, id),
        }
    }

    /// Re-resolve the bus endpoint after a data-store notification
    /// (`i2cdriver_handle_bus_update`).
    ///
    /// The subscription that would deliver these notifications is the
    /// registered seam; when it lands this arm re-reserves the device on
    /// the new endpoint, as C does.
    fn refresh_bus<T: DriverTransport>(&mut self, transport: &mut T) {
        let Some(old) = self.bus else {
            return;
        };
        if let Some(new) = client::bus_endpoint(transport, BUS_NUMBER)
            && new != old
            && client::reserve(transport, new, self.address).is_ok()
        {
            self.bus = Some(new);
        }
    }
}

impl Default for Bmp085Service {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: DriverTransport> DriverHandler<T> for Bmp085Service {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    use crate::measure::{datasheet_calibration_values, PRESSURE_COMMANDS};

    /// Scripted bus plus caller-grant and reply recorder.
    struct FakeBus {
        endpoint: Endpoint,
        bus_present: bool,
        block: u64,
        sensor_reads: u32,
        delays: Vec<u32>,
        writes: Vec<(u8, u8)>,
        copied: Vec<(Endpoint, i32, u64, Vec<u8>)>,
        replies: Vec<(Endpoint, i32, i32)>,
        fail_sensor: bool,
        chip_id: u8,
    }

    impl FakeBus {
        fn new() -> Self {
            FakeBus {
                endpoint: Endpoint(42),
                bus_present: true,
                block: 0,
                sensor_reads: 0,
                delays: Vec::new(),
                writes: Vec::new(),
                copied: Vec::new(),
                replies: Vec::new(),
                fail_sensor: false,
                chip_id: measure::BMP085_CHIPID,
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
        fn send_rec(&mut self, _dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            match msg.m_type {
                client::BUSC_I2C_RESERVE => {
                    msg.m_type = 0;
                    Ok(())
                }
                client::BUSC_I2C_EXEC => {
                    let block = self.block as *mut client::ExecBlock;
                    // SAFETY: the test granted its own exec block.
                    let block = unsafe { &mut *block };
                    match block.op {
                        client::I2C_OP_READ_WITH_STOP => {
                            if self.fail_sensor {
                                msg.m_type = minix_types::EIO;
                                return Ok(());
                            }
                            match block.cmd[0] {
                                measure::CHIPID_REG => {
                                    block.buf[0] = self.chip_id;
                                }
                                0xAA..=0xBF => {
                                    let index = ((block.cmd[0] - 0xAA) / 2) as usize;
                                    let value = datasheet_calibration_values()[index];
                                    block.buf[..2].copy_from_slice(&value.to_be_bytes());
                                }
                                measure::SENSOR_VALUE_MSB => {
                                    self.sensor_reads += 1;
                                    if block.buf_len == 2 {
                                        block.buf[..2]
                                            .copy_from_slice(&27898u16.to_be_bytes());
                                    } else {
                                        // Mode 0: the raw value carries no
                                        // oversampling shift.
                                        let raw = 23843u32 << 8;
                                        block.buf[..3].copy_from_slice(&raw.to_be_bytes()[1..]);
                                    }
                                }
                                _ => {}
                            }
                        }
                        client::I2C_OP_WRITE_WITH_STOP => {
                            self.writes.push((block.buf[0], block.buf[1]));
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
            let base = ::core::ptr::addr_of!(*msg) as *const u8;
            let mut status = [0u8; 4];
            let mut id = [0u8; 4];
            // SAFETY: the reply payload is `{ status, id }` at the union
            // start (the layout `reply` writes).
            unsafe {
                ::core::ptr::copy_nonoverlapping(base.add(PAYLOAD), status.as_mut_ptr(), 4);
                ::core::ptr::copy_nonoverlapping(base.add(PAYLOAD + 4), id.as_mut_ptr(), 4);
            }
            self.replies
                .push((dst, i32::from_le_bytes(status), i32::from_le_bytes(id)));
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
            offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.copied.push((granter, grant, offset, Vec::from(buf)));
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
        fn delay_micros(&mut self, micros: u32) -> Result<(), i32> {
            self.delays.push(micros);
            Ok(())
        }
    }

    fn read_message(caller: Endpoint, id: u32, position: u64, grant: i32, count: u64) -> Message {
        let mut msg = Message {
            m_type: CdevRequest::Read.message_type(),
            m_source: caller,
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: test-only writers into the fixed-size message body; the
        // offsets are the pinned readwrite layout.
        unsafe {
            ::core::ptr::copy_nonoverlapping(
                position.to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::POSITION),
                8,
            );
            ::core::ptr::copy_nonoverlapping(
                (grant as u32).to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::GRANT),
                4,
            );
            ::core::ptr::copy_nonoverlapping(
                count.to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::COUNT),
                8,
            );
            ::core::ptr::copy_nonoverlapping(
                id.to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::ID),
                4,
            );
        }
        msg
    }

    fn last_reply(bus: &FakeBus) -> (i32, i32) {
        let (_, status, id) = bus.replies.last().expect("a reply");
        (*status, *id)
    }

    #[test]
    fn test_start_needs_the_bus_chip_and_calibration() {
        let mut service = Bmp085Service::new();
        let mut bus = FakeBus::new();
        assert_eq!(service.start(&mut bus), Ok(()));

        let mut no_bus = FakeBus {
            bus_present: false,
            ..FakeBus::new()
        };
        let mut service = Bmp085Service::new();
        assert_eq!(service.start(&mut no_bus), Err(-minix_types::ENOENT));

        let mut wrong_chip = FakeBus {
            chip_id: 0x00,
            ..FakeBus::new()
        };
        let mut service = Bmp085Service::new();
        assert_eq!(service.start(&mut wrong_chip), Err(-minix_types::ENODEV));
    }

    #[test]
    fn test_read_measures_formats_and_copies() {
        let mut bus = FakeBus::new();
        let mut service = Bmp085Service::new();
        service.set_command(PRESSURE_COMMANDS[0]);
        assert_eq!(service.start(&mut bus), Ok(()));

        let msg = read_message(Endpoint(5), 3, 0, 9, 64);
        service.dispatch(&mut bus, &msg);

        assert_eq!(bus.copied.len(), 1);
        let (caller, grant, offset, bytes) = &bus.copied[0];
        assert_eq!(*caller, Endpoint(5));
        assert_eq!(*grant, 9);
        assert_eq!(*offset, 0);
        let expected = b"TEMPERATURE     : 15.0\nPRESSURE        : 69964\n";
        assert_eq!(bytes.as_slice(), expected);
        assert_eq!(last_reply(&bus), (expected.len() as i32, 3));
        assert_eq!(bus.writes.len(), 2, "temperature and pressure triggers");
        assert_eq!(
            bus.writes[0],
            (measure::CTRL_REG, measure::CMD_TRIGGER_TEMPERATURE)
        );
        assert_eq!(
            bus.writes[1],
            (measure::CTRL_REG, PRESSURE_COMMANDS[0].command)
        );
        assert_eq!(
            bus.delays,
            vec![measure::UDELAY_TEMPERATURE, PRESSURE_COMMANDS[0].udelay]
        );
        assert_eq!(bus.sensor_reads, 2);
    }

    #[test]
    fn test_read_clamps_to_the_formatted_length() {
        let mut bus = FakeBus::new();
        let mut service = Bmp085Service::new();
        service.set_command(PRESSURE_COMMANDS[0]);
        assert_eq!(service.start(&mut bus), Ok(()));

        // The formatted text is 47 bytes; ask for the last four.
        let msg = read_message(Endpoint(5), 4, 43, 9, 100);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.copied[0].3.as_slice(), b"964\n");
        assert_eq!(last_reply(&bus), (4, 4));

        bus.copied.clear();
        let msg = read_message(Endpoint(5), 5, 47, 9, 16);
        service.dispatch(&mut bus, &msg);
        assert!(bus.copied.is_empty());
        assert_eq!(last_reply(&bus), (0, 5));
    }

    #[test]
    fn test_sensor_failure_answers_eio_without_copy() {
        let mut bus = FakeBus::new();
        let mut service = Bmp085Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        bus.fail_sensor = true;
        let msg = read_message(Endpoint(5), 6, 0, 9, 64);
        service.dispatch(&mut bus, &msg);
        assert!(bus.copied.is_empty());
        assert_eq!(last_reply(&bus).0, EIO);
        assert_eq!(bus.delays, vec![measure::UDELAY_TEMPERATURE]);
    }

    #[test]
    fn test_missing_hooks_answer_like_the_c_defaults() {
        let mut bus = FakeBus::new();
        let mut service = Bmp085Service::new();
        for (kind, expected) in [
            (CdevRequest::Open, OK),
            (CdevRequest::Close, OK),
            (CdevRequest::Write, EIO),
            (CdevRequest::Ioctl, ENOTTY),
            (CdevRequest::Select, EBADF),
        ] {
            let msg = Message {
                m_type: kind.message_type(),
                m_source: Endpoint(5),
                ..Message::default()
            };
            service.dispatch(&mut bus, &msg);
            assert_eq!(last_reply(&bus).0, expected, "{:?}", kind);
        }
        let _ = &mut service;
    }
}
