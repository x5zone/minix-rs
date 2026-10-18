//! Service assembly: the TPS65950 RTCDEV server.
//!
//! C correspondence: `sef_cb_init` (`tps65950.c:166-222`: bus lookup,
//! reservation of all four sub-module addresses, `check_revision`,
//! `rtc_init`) and the main loop (`tps65950.c:243-352`): notifications
//! only refresh the bus endpoint (all four addresses), and every other
//! message is one of three requests — `RTCDEV_GET_TIME_G` (any caller;
//! the time is copied into the caller's grant), `RTCDEV_SET_TIME_G`
//! (super user only; the time is read from the grant),
//! `RTCDEV_PWR_OFF` (unsupported, `ENOSYS`) — with `EINVAL` for anything
//! else; the reply carries the status in `RTCDEV_REPLY`. The loop shell
//! and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`].
//!
//! Registered seams: the environment plumbing (bus and base-address
//! defaults) and the bus-restart subscription are not wired; the caller
//! identity (C's `getnuid`) is the [`CallerIdentity`] trait.

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client;
use minix_types::{Endpoint, Message, EINVAL, ENOSYS, EPERM, OK};

use crate::device;
use crate::protocol::{self, cwire};
use crate::registers::{ADDRESSES, ID2, ID4};
use crate::rtc::{self, Tm, TM_WIRE_BYTES};

/// The bus number (`bus=1` in C's environment; the generic build carries
/// no environment, so the documented default stands).
pub const BUS_NUMBER: u32 = 1;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
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

fn read_i32(msg: &Message, offset: usize) -> i32 {
    i32::from_le_bytes(read_bytes(msg, offset))
}

fn write_status(msg: &mut Message, status: i32) {
    let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: the reply status is the union's first word.
    unsafe {
        ::core::ptr::copy_nonoverlapping(status.to_le_bytes().as_ptr(), base.add(cwire::STATUS), 4);
    }
}

/// Who the driver asks about a caller before gating a request
/// (C `getnuid(caller) == SUPER_USER`).
pub trait CallerIdentity {
    /// Whether the caller's effective uid is the super user.
    fn is_super_user(&mut self, caller: Endpoint) -> bool;
}

/// Production identity: PM answers the uid query.
pub struct PmIdentity;

impl CallerIdentity for PmIdentity {
    fn is_super_user(&mut self, caller: Endpoint) -> bool {
        // C `SUPER_USER` is uid zero (`const.h:44`); a failed query
        // refuses the operation (fail-closed).
        minix_sys::pm::getnuid_via(&minix_sys::ipc::DirectTrapTransport, caller)
            .map(|uid| uid == 0)
            .unwrap_or(false)
    }
}

/// The PMIC service: bus binding plus the RTCDEV pump.
pub struct Tps65950Service<I: CallerIdentity> {
    bus: Option<Endpoint>,
    identity: I,
}

impl<I: CallerIdentity> Tps65950Service<I> {
    /// A service with no bus yet.
    pub fn new(identity: I) -> Self {
        Tps65950Service {
            bus: None,
            identity,
        }
    }

    /// C's fresh-start init: bus lookup, reserve all four sub-modules,
    /// the revision check, then the RTC start.
    pub fn start<T: DriverTransport>(&mut self, transport: &mut T) -> Result<(), i32> {
        let endpoint = client::bus_endpoint(transport, BUS_NUMBER).ok_or(-minix_types::ENOENT)?;
        for address in ADDRESSES {
            client::reserve(transport, endpoint, address)?;
        }
        device::check_revision(transport, endpoint, ADDRESSES[ID2])?;
        rtc::init(transport, endpoint, ADDRESSES[ID4])?;
        self.bus = Some(endpoint);
        Ok(())
    }

    /// Send the RTCDEV reply carrying the status.
    fn reply<T: DriverTransport>(&mut self, caller: Endpoint, status: i32, transport: &mut T) {
        let mut message = Message {
            m_type: protocol::RTC_REPLY_BASE,
            ..Message::default()
        };
        write_status(&mut message, status);
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
        let status = match msg.m_type {
            protocol::GET_TIME_GRANT => self.get_time(transport, caller, msg),
            protocol::SET_TIME_GRANT => self.set_time(transport, caller, msg),
            protocol::PWR_OFF => ENOSYS,
            _ => EINVAL,
        };
        self.reply(caller, status, transport);
    }

    /// `RTCDEV_GET_TIME_G`: latch, read, and copy the time into the
    /// caller's grant.
    fn get_time<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        msg: &Message,
    ) -> i32 {
        let Some(bus) = self.bus else {
            return -minix_types::EIO;
        };
        let grant = read_i32(msg, cwire::GRANT);
        let tm = match rtc::get_time(transport, bus, ADDRESSES[ID4]) {
            Ok(tm) => tm,
            Err(_) => return -minix_types::EIO,
        };
        match transport.copy_to_grant(caller, grant, 0, &tm.to_wire()) {
            Ok(()) => OK,
            Err(status) => status,
        }
    }

    /// `RTCDEV_SET_TIME_G`: gate on the caller's uid, read the time from
    /// its grant, and write the registers.
    fn set_time<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        msg: &Message,
    ) -> i32 {
        if !self.identity.is_super_user(caller) {
            return EPERM;
        }
        let Some(bus) = self.bus else {
            return -minix_types::EIO;
        };
        let grant = read_i32(msg, cwire::GRANT);
        let mut bytes = [0u8; TM_WIRE_BYTES];
        if let Err(status) = transport.copy_from_grant(caller, grant, 0, &mut bytes) {
            return status;
        }
        let tm = Tm::from_wire(&bytes);
        match rtc::set_time(transport, bus, ADDRESSES[ID4], &tm) {
            Ok(()) => OK,
            Err(_) => -minix_types::EIO,
        }
    }

    /// Re-resolve the bus endpoint after a data-store notification and
    /// re-reserve all four sub-modules on it.
    fn refresh_bus<T: DriverTransport>(&mut self, transport: &mut T) {
        let Some(old) = self.bus else {
            return;
        };
        if let Some(new) = client::bus_endpoint(transport, BUS_NUMBER)
            && new != old
            && ADDRESSES
                .iter()
                .all(|address| client::reserve(transport, new, *address).is_ok())
        {
            self.bus = Some(new);
        }
    }
}

impl<I: CallerIdentity, T: DriverTransport> DriverHandler<T> for Tps65950Service<I> {
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
        IDCODE_7_0_REG, IDCODE_15_8_REG, IDCODE_23_16_REG, IDCODE_31_24_REG, MINUTES_REG,
        MONTHS_REG, RTC_CTRL_REG, RTC_STATUS_REG, RUN_BIT, SECONDS_REG, STOP_RTC_BIT, YEARS_REG,
        DAYS_REG, HOURS_REG,
    };
    use crate::rtc::dec_to_bcd;

    /// Canned identity: a fixed super-user set.
    struct CannedIdentity {
        super_users: Vec<i32>,
    }

    impl CallerIdentity for CannedIdentity {
        fn is_super_user(&mut self, caller: Endpoint) -> bool {
            self.super_users.contains(&caller.0)
        }
    }

    /// Scripted bus: register file, grant copies, replies.
    struct FakeBus {
        endpoint: Endpoint,
        bus_present: bool,
        registers: HashMap<u8, u8>,
        reserves: Vec<(Endpoint, u16)>,
        writes: Vec<(u16, u8, u8)>,
        copied_out: Vec<(Endpoint, i32, Vec<u8>)>,
        grant_in: Vec<u8>,
        replies: Vec<(Endpoint, i32, i32)>,
        block: u64,
    }

    impl FakeBus {
        fn with_revision(revision: u32) -> Self {
            let mut registers = HashMap::new();
            registers.insert(IDCODE_7_0_REG, (revision & 0xFF) as u8);
            registers.insert(IDCODE_15_8_REG, ((revision >> 8) & 0xFF) as u8);
            registers.insert(IDCODE_23_16_REG, ((revision >> 16) & 0xFF) as u8);
            registers.insert(IDCODE_31_24_REG, ((revision >> 24) & 0xFF) as u8);
            // The RTC reports running.
            registers.insert(RTC_STATUS_REG, 1 << RUN_BIT);
            FakeBus {
                endpoint: Endpoint(42),
                bus_present: true,
                registers,
                reserves: Vec::new(),
                writes: Vec::new(),
                copied_out: Vec::new(),
                grant_in: Vec::new(),
                replies: Vec::new(),
                block: 0,
            }
        }

        fn last_status(&self) -> i32 {
            self.replies.last().expect("a reply").2
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
                        client::I2C_OP_READ_WITH_STOP => {
                            block.buf[0] = self
                                .registers
                                .get(&block.cmd[0])
                                .copied()
                                .unwrap_or(0);
                        }
                        client::I2C_OP_WRITE_WITH_STOP => {
                            self.writes.push((block.address, block.buf[0], block.buf[1]));
                            self.registers.insert(block.buf[0], block.buf[1]);
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
            let status = read_i32(msg, cwire::STATUS);
            self.replies.push((dst, msg.m_type, status));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            let take = buf.len().min(self.grant_in.len());
            buf[..take].copy_from_slice(&self.grant_in[..take]);
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            granter: Endpoint,
            grant: i32,
            _offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            self.copied_out.push((granter, grant, Vec::from(buf)));
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
    }

    fn request(message_type: i32, caller: Endpoint, grant: i32) -> Message {
        let mut msg = Message {
            m_type: message_type,
            m_source: caller,
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: test-only writer into the pinned request layout.
        unsafe {
            ::core::ptr::copy_nonoverlapping(
                grant.to_le_bytes().as_ptr(),
                base.add(cwire::GRANT),
                4,
            );
        }
        msg
    }

    #[test]
    fn test_start_reserves_all_four_and_checks_the_chip() {
        let mut bus = FakeBus::with_revision(crate::registers::IDCODE_REV_1_2);
        let mut service = Tps65950Service::new(CannedIdentity { super_users: vec![] });
        assert_eq!(service.start(&mut bus), Ok(()));
        assert_eq!(
            bus.reserves,
            vec![
                (Endpoint(42), 0x48),
                (Endpoint(42), 0x49),
                (Endpoint(42), 0x4A),
                (Endpoint(42), 0x4B)
            ]
        );
        // The RTC control write sets the stop bit before the run check.
        assert_eq!(
            bus.writes.iter().find(|(address, register, _)| {
                *address == 0x4B && *register == RTC_CTRL_REG
            }),
            Some(&(0x4B, RTC_CTRL_REG, 1 << STOP_RTC_BIT))
        );

        let mut wrong = FakeBus::with_revision(0xDEAD_BEEF);
        let mut service = Tps65950Service::new(CannedIdentity { super_users: vec![] });
        assert_eq!(service.start(&mut wrong), Err(-minix_types::EIO));
    }

    #[test]
    fn test_get_time_copies_the_latched_time() {
        let mut bus = FakeBus::with_revision(crate::registers::IDCODE_REV_1_0);
        bus.registers.insert(SECONDS_REG, 0x59);
        bus.registers.insert(MINUTES_REG, 0x23);
        bus.registers.insert(HOURS_REG, 0x12);
        bus.registers.insert(DAYS_REG, 0x31);
        bus.registers.insert(MONTHS_REG, 0x12);
        bus.registers.insert(YEARS_REG, 0x13);
        let mut service = Tps65950Service::new(CannedIdentity { super_users: vec![] });
        assert_eq!(service.start(&mut bus), Ok(()));

        service.dispatch(&mut bus, &request(protocol::GET_TIME_GRANT, Endpoint(5), 9));
        assert_eq!(bus.replies.last().unwrap().1, protocol::RTC_REPLY_BASE);
        assert_eq!(bus.last_status(), OK);
        let (caller, grant, bytes) = &bus.copied_out[0];
        assert_eq!(*caller, Endpoint(5));
        assert_eq!(*grant, 9);
        let tm = Tm::from_wire(bytes.as_slice().try_into().unwrap());
        assert_eq!(
            tm,
            Tm {
                sec: 59,
                min: 23,
                hour: 12,
                mday: 31,
                mon: 11,
                year: 113,
                wday: 0,
                yday: 0
            }
        );
    }

    #[test]
    fn test_cold_start_writes_the_default_date_back() {
        let mut bus = FakeBus::with_revision(crate::registers::IDCODE_REV_1_0);
        bus.registers.insert(YEARS_REG, 0x00);
        let mut service = Tps65950Service::new(CannedIdentity { super_users: vec![] });
        assert_eq!(service.start(&mut bus), Ok(()));
        let writes_before = bus.writes.len();

        service.dispatch(&mut bus, &request(protocol::GET_TIME_GRANT, Endpoint(5), 9));
        assert_eq!(bus.last_status(), OK);
        let tm = Tm::from_wire(bus.copied_out[0].2.as_slice().try_into().unwrap());
        assert_eq!(tm.year, 113);
        assert_eq!(tm.mday, 1);
        assert_eq!(tm.mon, 0);
        // The cold-start path wrote the six time fields back (the control
        // latch write rides in front of them).
        let new_writes: Vec<_> = bus.writes[writes_before..]
            .iter()
            .filter(|(address, register, _)| {
                *address == 0x4B
                    && matches!(
                        *register,
                        SECONDS_REG | MINUTES_REG | HOURS_REG | DAYS_REG | MONTHS_REG | YEARS_REG
                    )
            })
            .copied()
            .collect();
        assert_eq!(new_writes.len(), 6);
        assert_eq!(new_writes[5], (0x4B, YEARS_REG, dec_to_bcd(13)));
    }

    #[test]
    fn test_set_time_gates_on_the_super_user() {
        let mut bus = FakeBus::with_revision(crate::registers::IDCODE_REV_1_0);
        let mut service = Tps65950Service::new(CannedIdentity { super_users: vec![5] });
        assert_eq!(service.start(&mut bus), Ok(()));
        let tm = Tm {
            sec: 1,
            min: 2,
            hour: 3,
            mday: 4,
            mon: 5,
            year: 106,
            wday: 0,
            yday: 0,
        };
        bus.grant_in = Vec::from(tm.to_wire().as_slice());

        let writes_before = bus.writes.len();
        // A non-super caller is refused without touching the chip.
        service.dispatch(&mut bus, &request(protocol::SET_TIME_GRANT, Endpoint(6), 9));
        assert_eq!(bus.last_status(), EPERM);
        assert_eq!(bus.writes.len(), writes_before);

        // The super user writes the six fields in BCD.
        service.dispatch(&mut bus, &request(protocol::SET_TIME_GRANT, Endpoint(5), 7));
        assert_eq!(bus.last_status(), OK);
        let new_writes = &bus.writes[writes_before..];
        assert_eq!(new_writes.len(), 6);
        assert_eq!(new_writes[0], (0x4B, SECONDS_REG, dec_to_bcd(1)));
        assert_eq!(new_writes[4], (0x4B, MONTHS_REG, dec_to_bcd(6)));
        assert_eq!(new_writes[5], (0x4B, YEARS_REG, dec_to_bcd(6)));
    }

    #[test]
    fn test_power_off_and_unknown_requests() {
        let mut bus = FakeBus::with_revision(crate::registers::IDCODE_REV_1_0);
        let mut service = Tps65950Service::new(CannedIdentity { super_users: vec![] });
        assert_eq!(service.start(&mut bus), Ok(()));

        service.dispatch(&mut bus, &request(protocol::PWR_OFF, Endpoint(5), 0));
        assert_eq!(bus.last_status(), ENOSYS);
        service.dispatch(&mut bus, &request(0x1234, Endpoint(5), 0));
        assert_eq!(bus.last_status(), EINVAL);
        assert_eq!(bus.replies.last().unwrap().1, protocol::RTC_REPLY_BASE);
    }
}
