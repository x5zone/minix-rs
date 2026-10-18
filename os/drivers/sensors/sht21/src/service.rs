//! Service assembly: the SHT21 sensor's receive-classify-dispatch pump.
//!
//! C correspondence: `sht21_tab` (`sht21.c:121-125`: read plus the
//! generic hook) served by `chardriver_task` (`sht21.c:486`); the
//! fresh-start init (`sht21.c:400-460`: bus lookup, reservation, soft
//! reset and register checks); `measure` (`sht21.c:232-270`: the
//! one-sample-per-second cache against self-heating, temperature then
//! humidity); and `sht21_read` (`sht21.c:332-368`: format, clamp by
//! position and count, copy to the caller's grant). The loop shell and
//! the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`].
//!
//! The cache keeps C's shape: the entry timestamp gates the sample, the
//! cached pair answers a second read in the same second, and the stored
//! timestamp is re-read after the measurement (C calls `time(NULL)`
//! twice). The clock comes from
//! [`minix_driver_rt::transport::DriverTransport::now_seconds`]; the
//! production implementation rides PM's `gettimeofday`, and a transport
//! without a clock reports `ENOSYS`, so a read fails instead of printing
//! an invented sample time.
//!
//! Registered seams: the bus-restart subscription
//! (`i2cdriver_subscribe_bus_updates`) is not wired, so the DS
//! notification arm below stays dormant; the environment plumbing is
//! fixed to `BUS_NUMBER` and `SENSOR_ADDRESS`.

use alloc::format;

use minix_chardriver::protocol::CdevRequest;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::{Endpoint, Message, CDEV_REPLY_BASE, EBADF, EINVAL, EIO, ENOTTY, OK};

use crate::driver::{self, Sensor};

/// The message union payload begins at byte 8 of [`Message`].
const PAYLOAD: usize = 8;

/// The bus number (`bus=1` in C's environment; the generic build carries
/// no environment, so the documented default stands).
pub const BUS_NUMBER: u32 = 1;

/// The sensor address (`address=0x40` in C's accepted-address table).
pub const SENSOR_ADDRESS: u16 = 0x40;

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

/// The sensor service: bus binding, the sampling cache, and the read pump.
pub struct Sht21Service {
    bus: Option<Endpoint>,
    address: u16,
    /// Timestamp of the last stored sample (`last_sample_time`).
    last_sample_time: i64,
    /// Cached temperature in millidegrees (`cached_t`).
    cached_t: i32,
    /// Cached humidity in milli-percent (`cached_rh`).
    cached_rh: i32,
}

impl Sht21Service {
    /// A service with no bus yet and C's zeroed cache.
    pub fn new() -> Self {
        Sht21Service {
            bus: None,
            address: SENSOR_ADDRESS,
            last_sample_time: 0,
            cached_t: 0,
            cached_rh: 0,
        }
    }

    /// The C fresh-start init: locate the bus, reserve the device, then
    /// run the soft reset and register checks.
    pub fn start<T: DriverTransport>(&mut self, transport: &mut T) -> Result<(), i32> {
        let endpoint = client::bus_endpoint(transport, BUS_NUMBER).ok_or(-minix_types::ENOENT)?;
        client::reserve(transport, endpoint, self.address)?;
        driver::init(transport, endpoint, self.address)?;
        self.bus = Some(endpoint);
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
            CdevRequest::Write => {
                let id = read_u32(msg, cwire::readwrite::ID);
                self.reply(transport, caller, EIO, id);
            }
            CdevRequest::Ioctl => {
                let id = read_u32(msg, cwire::readwrite::ID);
                self.reply(transport, caller, ENOTTY, id);
            }
            CdevRequest::Select => self.reply(transport, caller, EBADF, 0),
            CdevRequest::Cancel => {}
        }
    }

    /// Sample (or reuse the cache) and render C's status buffer.
    fn handle_read<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let caller = msg.m_source;
        let id = read_u32(msg, cwire::readwrite::ID);
        let position = read_u64(msg, cwire::readwrite::POSITION);
        let grant = read_u32(msg, cwire::readwrite::GRANT) as i32;
        let size = read_u64(msg, cwire::readwrite::COUNT) as usize;

        let Some(bus) = self.bus else {
            self.reply(transport, caller, EIO, id);
            return;
        };
        let now = match transport.now_seconds() {
            Ok(value) => value,
            Err(_) => {
                self.reply(transport, caller, EIO, id);
                return;
            }
        };
        if now != self.last_sample_time {
            let temperature =
                match driver::sensor_read(transport, bus, self.address, Sensor::Temperature) {
                    Ok(value) => value,
                    Err(_) => {
                        self.reply(transport, caller, EIO, id);
                        return;
                    }
                };
            let humidity =
                match driver::sensor_read(transport, bus, self.address, Sensor::Humidity) {
                    Ok(value) => value,
                    Err(_) => {
                        self.reply(transport, caller, EIO, id);
                        return;
                    }
                };
            self.cached_t = temperature;
            self.cached_rh = humidity;
            // C re-reads the clock after measuring (`measure`'s second
            // `time(NULL)`), so the next real sample is at least one
            // second after the measurement finished.
            self.last_sample_time = match transport.now_seconds() {
                Ok(value) => value,
                Err(_) => {
                    self.reply(transport, caller, EIO, id);
                    return;
                }
            };
        }

        // C: `"%-16s: %d.%03d\n%-16s: %d.%03d\n"` (`sht21.c:345-348`).
        let text = format!(
            "{:<16}: {}.{:03}\n{:<16}: {}.{:03}\n",
            "TEMPERATURE",
            self.cached_t / 1000,
            self.cached_t % 1000,
            "HUMIDITY",
            self.cached_rh / 1000,
            self.cached_rh % 1000
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

    /// Re-resolve the bus endpoint after a data-store notification.
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

impl Default for Sht21Service {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: DriverTransport> DriverHandler<T> for Sht21Service {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    /// Scripted bus: answers the user register and the two sensor reads.
    struct FakeBus {
        endpoint: Endpoint,
        bus_present: bool,
        block: u64,
        now: i64,
        user_register: u8,
        sensor_transactions: u32,
        raw_writes: Vec<u8>,
        copied: Vec<(Endpoint, i32, u64, Vec<u8>)>,
        replies: Vec<(Endpoint, i32, i32)>,
        bad_crc: bool,
    }

    impl FakeBus {
        fn new() -> Self {
            FakeBus {
                endpoint: Endpoint(42),
                bus_present: true,
                block: 0,
                now: 1_000,
                user_register: driver::EXPECTED_POWER_UP_VALUE,
                sensor_transactions: 0,
                raw_writes: Vec::new(),
                copied: Vec::new(),
                replies: Vec::new(),
                bad_crc: false,
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
                        client::I2C_OP_READ_WITH_STOP => match block.cmd[0] {
                            driver::CMD_READ_USER_REGISTER => {
                                block.buf[0] = self.user_register;
                            }
                            driver::CMD_TRIGGER_TEMPERATURE
                            | driver::CMD_TRIGGER_HUMIDITY => {
                                self.sensor_transactions += 1;
                                let raw: u16 = if block.cmd[0] == driver::CMD_TRIGGER_TEMPERATURE
                                {
                                    26674
                                } else {
                                    32768
                                };
                                let high = (raw >> 8) as u8;
                                let low = raw as u8;
                                let mut crc = driver::crc8(driver::CRC8_INITIAL, high);
                                crc = driver::crc8(crc, low);
                                if self.bad_crc {
                                    crc ^= 0xFF;
                                }
                                block.buf[0] = high;
                                block.buf[1] = low;
                                block.buf[2] = crc;
                            }
                            _ => {}
                        },
                        client::I2C_OP_WRITE_WITH_STOP => {
                            assert_eq!(block.buf_len, 1, "raw write carries one byte");
                            self.raw_writes.push(block.buf[0]);
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
            // SAFETY: the reply payload is `{ status, id }` at the union start.
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
        fn delay_micros(&mut self, _micros: u32) -> Result<(), i32> {
            Ok(())
        }
        fn now_seconds(&mut self) -> Result<i64, i32> {
            Ok(self.now)
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
    fn test_start_runs_the_reset_and_register_checks() {
        let mut bus = FakeBus::new();
        let mut service = Sht21Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        assert_eq!(bus.raw_writes, vec![driver::CMD_SOFT_RESET]);

        let mut no_bus = FakeBus {
            bus_present: false,
            ..FakeBus::new()
        };
        let mut service = Sht21Service::new();
        assert_eq!(service.start(&mut no_bus), Err(-minix_types::ENOENT));

        let mut low_battery = FakeBus {
            user_register: driver::EXPECTED_POWER_UP_VALUE
                | driver::USER_REGISTER_END_OF_BATTERY,
            ..FakeBus::new()
        };
        let mut service = Sht21Service::new();
        assert_eq!(service.start(&mut low_battery), Err(-minix_types::EIO));

        let mut odd_register = FakeBus {
            user_register: 0x00,
            ..FakeBus::new()
        };
        let mut service = Sht21Service::new();
        assert_eq!(service.start(&mut odd_register), Err(-minix_types::EIO));
    }

    #[test]
    fn test_read_formats_and_caches_within_seconds() {
        let mut bus = FakeBus::new();
        let mut service = Sht21Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));

        let msg = read_message(Endpoint(5), 3, 0, 9, 64);
        service.dispatch(&mut bus, &msg);
        // The raw temperature 0x6832 loses its two status bits (0x6830 =
        // 26672) before conversion.
        let expected = b"TEMPERATURE     : 24.664\nHUMIDITY        : 56.500\n";
        assert_eq!(bus.copied[0].3.as_slice(), expected);
        assert_eq!(last_reply(&bus), (expected.len() as i32, 3));
        assert_eq!(bus.sensor_transactions, 2);

        // A second read in the same second answers from the cache.
        let msg = read_message(Endpoint(5), 4, 0, 9, 64);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.sensor_transactions, 2, "cache suppresses the sample");
        assert_eq!(bus.copied.len(), 2);

        // The next second samples again.
        bus.now += 1;
        let msg = read_message(Endpoint(5), 5, 0, 9, 64);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.sensor_transactions, 4);
    }

    #[test]
    fn test_read_clamps_to_the_formatted_length() {
        let mut bus = FakeBus::new();
        let mut service = Sht21Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));

        // The formatted text is 50 bytes; ask for the last four.
        let msg = read_message(Endpoint(5), 6, 46, 9, 64);
        service.dispatch(&mut bus, &msg);
        assert_eq!(bus.copied[0].3.as_slice(), b"500\n");
        assert_eq!(last_reply(&bus), (4, 6));

        bus.copied.clear();
        let msg = read_message(Endpoint(5), 7, 50, 9, 16);
        service.dispatch(&mut bus, &msg);
        assert!(bus.copied.is_empty());
        assert_eq!(last_reply(&bus), (0, 7));
    }

    #[test]
    fn test_bad_crc_answers_eio_without_copy() {
        let mut bus = FakeBus::new();
        let mut service = Sht21Service::new();
        assert_eq!(service.start(&mut bus), Ok(()));
        bus.bad_crc = true;
        let msg = read_message(Endpoint(5), 8, 0, 9, 64);
        service.dispatch(&mut bus, &msg);
        assert!(bus.copied.is_empty());
        assert_eq!(last_reply(&bus).0, EIO);
    }

    #[test]
    fn test_missing_hooks_answer_like_the_c_defaults() {
        let mut bus = FakeBus::new();
        let mut service = Sht21Service::new();
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
    }
}
