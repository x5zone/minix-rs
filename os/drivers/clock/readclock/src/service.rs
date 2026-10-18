//! Service assembly: the readclock driver's receive-classify-dispatch pump.
//!
//! C correspondence: `main()` (`readclock.c:41-131`) — every delivered
//! message is an RTCDEV request or a notification; the loop shell and the
//! RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`]. This module owns the
//! one-message-at-a-time decision plus the two data-plane legs C keeps in
//! `fetch_t`/`store_t`: the broken-down time is copied between the
//! caller's buffer and the driver either by address (`sys_datacopy`) or
//! through a grant, depending on the request variant.
//!
//! Two seams face the hardware and the process table:
//!
//! - The chip is the [`RealTimeClock`] trait; the binary passes a
//!   `NullClock` until an architecture clock lands (every operation then
//!   answers `EIO` honestly).
//! - The caller identity (`getnuid(caller) == SUPER_USER`, `caller ==
//!   PM_PROC_NR`) is the [`CallerIdentity`] trait; the production
//!   implementation queries PM, the host tests answer from a canned set.

use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message};

use crate::char_face::ReadclockFace;
use crate::clock::RealTimeClock;
use crate::device::{dispatch as gate, Caller, Dispatch};
use crate::protocol::{BrokenTime, RtcRequest, NO_FLAGS, RTC_REPLY_BASE};

/// The message union payload begins at byte 8 of [`Message`] (after
/// `m_source` at 0 and `m_type` at 4, with 8-byte union alignment).
const PAYLOAD: usize = 8;

/// Request and reply wire offsets.
///
/// C: `mess_lc_readclock_rtcdev` (`ipc.h:593-600`): a 32-bit grant id,
/// then an 8-aligned `vir_bytes` buffer address, then the flags; the
/// reply struct (`ipc.h:1845-1852`) carries only the status.
mod cwire {
    use super::PAYLOAD;
    /// `cp_grant_id_t grant`.
    pub const GRANT: usize = PAYLOAD;
    /// `vir_bytes tm` (a `struct tm *` in the caller).
    pub const TM: usize = PAYLOAD + 8;
    /// `int flags`.
    pub const FLAGS: usize = PAYLOAD + 16;
    /// `int status` (the reply struct's first field).
    pub const STATUS: usize = PAYLOAD;
}

/// The eight interpreted `struct tm` fields, `i32` each.
const TIME_WIRE_BYTES: usize = 32;

/// The notification family shares the 0x1000 base with the source in the
/// low byte (C `is_ipc_notify`, `const.h`).
const fn is_ipc_notify(message_type: i32) -> bool {
    (message_type & !0xff) == 0x1000
}

fn read_i32(msg: &Message, offset: usize) -> i32 {
    let base = core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; 4];
    // SAFETY: pinned offset inside the fixed-size message body.
    unsafe {
        core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), 4);
    }
    i32::from_le_bytes(bytes)
}

fn read_u64(msg: &Message, offset: usize) -> u64 {
    let base = core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; 8];
    // SAFETY: pinned offset inside the fixed-size message body.
    unsafe {
        core::ptr::copy_nonoverlapping(base.add(offset), bytes.as_mut_ptr(), 8);
    }
    u64::from_le_bytes(bytes)
}

fn write_i32(msg: &mut Message, offset: usize, value: i32) {
    let base = core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: pinned offset inside the fixed-size message body.
    unsafe {
        core::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), base.add(offset), 4);
    }
}

/// Who the driver asks about a caller before gating a request.
pub trait CallerIdentity {
    /// Whether the caller's effective uid is the super user
    /// (C `getnuid(caller) == SUPER_USER`, `readclock.c:85`).
    fn is_super_user(&mut self, caller: Endpoint) -> bool;
    /// Whether the caller is the power manager
    /// (C `caller == PM_PROC_NR`, `readclock.c:103`).
    fn is_power_manager(&self, caller: Endpoint) -> bool;
}

/// Production identity: PM answers the uid query, the PM endpoint is
/// compared directly.
pub struct PmIdentity;

impl CallerIdentity for PmIdentity {
    fn is_super_user(&mut self, caller: Endpoint) -> bool {
        // C `SUPER_USER` is uid zero (`minix3/minix/include/minix/const.h:44`);
        // a failed PM query refuses the operation (fail-closed), matching
        // the C path where an unknown caller is not the super user.
        minix_sys::pm::getnuid_via(&minix_sys::ipc::DirectTrapTransport, caller)
            .map(|uid| uid == 0)
            .unwrap_or(false)
    }

    fn is_power_manager(&self, caller: Endpoint) -> bool {
        caller == minix_sys::pm::pm_endpoint()
    }
}

/// The readclock service: the clock face plus the pump that gates,
/// transfers, and replies.
pub struct ReadclockService<R: RealTimeClock, I: CallerIdentity> {
    /// The device face (permission gates and the chip).
    pub face: ReadclockFace<R>,
    /// How a caller endpoint turns into the two gate verdicts.
    pub identity: I,
}

impl<R: RealTimeClock, I: CallerIdentity> ReadclockService<R, I> {
    /// Build the service over a clock and an identity source.
    pub fn new(clock: R, identity: I) -> Self {
        ReadclockService {
            face: ReadclockFace::new(clock),
            identity,
        }
    }

    /// Pack the eight fields in `struct tm` order (`time.h:75-87`).
    fn encode_time(time: &BrokenTime) -> [u8; TIME_WIRE_BYTES] {
        let mut bytes = [0u8; TIME_WIRE_BYTES];
        let fields = [
            time.seconds,
            time.minutes,
            time.hours,
            time.day,
            time.month,
            time.year,
            time.week_day,
            time.year_day,
        ];
        for (slot, value) in bytes.chunks_exact_mut(4).zip(fields) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Unpack the eight fields back into the typed shape.
    fn decode_time(bytes: &[u8; TIME_WIRE_BYTES]) -> BrokenTime {
        let field = |index: usize| -> i32 {
            let mut raw = [0u8; 4];
            raw.copy_from_slice(&bytes[index * 4..index * 4 + 4]);
            i32::from_le_bytes(raw)
        };
        BrokenTime {
            seconds: field(0),
            minutes: field(1),
            hours: field(2),
            day: field(3),
            month: field(4),
            year: field(5),
            week_day: field(6),
            year_day: field(7),
        }
    }

    /// Copy bytes out to the caller: grant variants use the grant, the
    /// plain variants the buffer address (`store_t`, `readclock.c:185-189`).
    fn store_to_caller<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        request: RtcRequest,
        msg: &Message,
        bytes: &[u8],
    ) -> Result<(), i32> {
        match request {
            RtcRequest::GetTimeGrant | RtcRequest::SetTimeGrant => {
                let grant = read_i32(msg, cwire::GRANT);
                transport.copy_to_grant(caller, grant, 0, bytes)
            }
            _ => {
                let addr = read_u64(msg, cwire::TM);
                transport.copy_to_addr(caller, addr, bytes)
            }
        }
    }

    /// Copy bytes in from the caller (`fetch_t`, `readclock.c:179-183`).
    fn fetch_from_caller<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        request: RtcRequest,
        msg: &Message,
        bytes: &mut [u8],
    ) -> Result<(), i32> {
        match request {
            RtcRequest::GetTimeGrant | RtcRequest::SetTimeGrant => {
                let grant = read_i32(msg, cwire::GRANT);
                transport.copy_from_grant(caller, grant, 0, bytes)
            }
            _ => {
                let addr = read_u64(msg, cwire::TM);
                transport.copy_from_addr(caller, addr, bytes)
            }
        }
    }

    /// Send the RTCDEV reply carrying the status (`readclock.c:117-126`).
    fn reply<T: DriverTransport>(&mut self, caller: Endpoint, status: i32, transport: &mut T) {
        let mut message = Message {
            m_type: RTC_REPLY_BASE,
            ..Message::default()
        };
        write_i32(&mut message, cwire::STATUS, status);
        let _ = transport.asynsend(caller, &mut message);
    }

    /// Handle one delivered message: gate it, run the operation, reply.
    ///
    /// C: the switch in `main` (`readclock.c:70-114`) — notifications and
    /// unknown calls answer before any chip access; get copies the time
    /// out, set copies it in and writes the chip, power-off asks the
    /// power manager's clock.
    pub fn dispatch<T: DriverTransport>(&mut self, transport: &mut T, msg: &Message) {
        let is_notify = is_ipc_notify(msg.m_type);
        let request = if is_notify {
            None
        } else {
            RtcRequest::decode(msg.m_type)
        };
        let caller_endpoint = msg.m_source;
        let caller = Caller {
            super_user: self.identity.is_super_user(caller_endpoint),
            power_manager: self.identity.is_power_manager(caller_endpoint),
        };
        let flags = if request.is_some() {
            read_i32(msg, cwire::FLAGS)
        } else {
            NO_FLAGS
        };
        let status = match gate(is_notify, request, caller) {
            Dispatch::Drop => return,
            Dispatch::Unknown => crate::device::unknown_call(),
            Dispatch::Get => match self.face.clock.get_time(flags) {
                Ok(time) => {
                    let bytes = Self::encode_time(&time);
                    match self.store_to_caller(
                        transport,
                        caller_endpoint,
                        request.expect("gate only reads on a decoded request"),
                        msg,
                        &bytes,
                    ) {
                        Ok(()) => crate::device::SUCCESS,
                        Err(code) => code,
                    }
                }
                Err(code) => code,
            },
            Dispatch::Set(Ok(())) => {
                let mut bytes = [0u8; TIME_WIRE_BYTES];
                match self.fetch_from_caller(
                    transport,
                    caller_endpoint,
                    request.expect("gate only sets on a decoded request"),
                    msg,
                    &mut bytes,
                ) {
                    Ok(()) => match self.face.clock.set_time(&Self::decode_time(&bytes), flags) {
                        Ok(()) => crate::device::SUCCESS,
                        Err(code) => code,
                    },
                    Err(code) => code,
                }
            }
            Dispatch::Set(Err(code)) | Dispatch::PowerOff(Err(code)) => code,
            Dispatch::PowerOff(Ok(())) => match self.face.clock.power_off() {
                Ok(()) => crate::device::SUCCESS,
                Err(code) => code,
            },
        };
        self.reply(caller_endpoint, status, transport);
    }
}

impl<R: RealTimeClock, I: CallerIdentity> Default for ReadclockService<R, I>
where
    R: Default,
    I: Default,
{
    fn default() -> Self {
        Self::new(R::default(), I::default())
    }
}

impl<R: RealTimeClock, I: CallerIdentity, T: DriverTransport> DriverHandler<T>
    for ReadclockService<R, I>
{
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::MemClock;
    use crate::device::test_time;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Canned identity: these endpoints are the super user, that one is PM.
    #[derive(Default)]
    struct CannedIdentity {
        super_users: Vec<Endpoint>,
        power_manager: Endpoint,
    }

    impl CallerIdentity for CannedIdentity {
        fn is_super_user(&mut self, caller: Endpoint) -> bool {
            self.super_users.contains(&caller)
        }
        fn is_power_manager(&self, caller: Endpoint) -> bool {
            caller == self.power_manager
        }
    }

    /// Scripted transport: one caller buffer shared by the address and
    /// grant copy legs, plus the recorded replies.
    struct Scripted {
        buffer: Vec<u8>,
        replies: Vec<(Endpoint, i32, i32)>,
    }

    impl Scripted {
        fn new() -> Self {
            Scripted {
                buffer: vec![0u8; 64],
                replies: Vec::new(),
            }
        }
        fn reply_statuses(&self) -> Vec<i32> {
            self.replies.iter().map(|r| r.2).collect()
        }
    }

    impl DriverTransport for Scripted {
        fn receive(&mut self, msg: &mut Message) -> Result<(), i32> {
            let _ = msg;
            Err(-minix_types::EIO)
        }
        fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.replies
                .push((dst, msg.m_type, read_i32(msg, cwire::STATUS)));
            Ok(())
        }
        fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            let _ = (dst, msg);
            Ok(())
        }
        fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            self.replies
                .push((dst, msg.m_type, read_i32(msg, cwire::STATUS)));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _g: Endpoint,
            _grant: i32,
            offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            let start = offset as usize;
            buf.copy_from_slice(&self.buffer[start..start + buf.len()]);
            Ok(())
        }
        fn copy_to_grant(
            &mut self,
            _g: Endpoint,
            _grant: i32,
            offset: u64,
            buf: &[u8],
        ) -> Result<(), i32> {
            let start = offset as usize;
            self.buffer[start..start + buf.len()].copy_from_slice(buf);
            Ok(())
        }
        fn copy_to_addr(&mut self, _dest: Endpoint, addr: u64, buf: &[u8]) -> Result<(), i32> {
            let start = addr as usize;
            self.buffer[start..start + buf.len()].copy_from_slice(buf);
            Ok(())
        }
        fn copy_from_addr(&mut self, _src: Endpoint, addr: u64, buf: &mut [u8]) -> Result<(), i32> {
            let start = addr as usize;
            buf.copy_from_slice(&self.buffer[start..start + buf.len()]);
            Ok(())
        }
        fn publish_label(&mut self, _label: &str) -> Result<(), i32> {
            Ok(())
        }
    }

    /// Build one RTCDEV request message (plain or grant variant).
    fn rtc_msg(request: RtcRequest, caller: Endpoint, grant: i32, tm: u64, flags: i32) -> Message {
        let mut msg = Message {
            m_type: request.message_type(),
            m_source: caller,
            ..Message::default()
        };
        write_i32(&mut msg, cwire::GRANT, grant);
        write_i32(&mut msg, cwire::FLAGS, flags);
        let base = core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: pinned offset inside the fixed-size message body.
        unsafe {
            core::ptr::copy_nonoverlapping(tm.to_le_bytes().as_ptr(), base.add(cwire::TM), 8);
        }
        msg
    }

    fn service(super_users: Vec<Endpoint>, power: Endpoint) -> ReadclockService<MemClock, CannedIdentity> {
        ReadclockService::new(
            MemClock::new(test_time()),
            CannedIdentity {
                super_users,
                power_manager: power,
            },
        )
    }

    #[test]
    fn test_notification_drops_without_reply() {
        let mut svc = service(vec![], Endpoint(0));
        let mut transport = Scripted::new();
        let notify = Message {
            m_type: 0x1000,
            m_source: Endpoint(20),
            ..Message::default()
        };
        svc.dispatch(&mut transport, &notify);
        assert!(transport.replies.is_empty());
    }

    #[test]
    fn test_get_copies_time_to_the_caller_buffer_and_replies_zero() {
        let mut svc = service(vec![], Endpoint(0));
        let mut transport = Scripted::new();
        let msg = rtc_msg(RtcRequest::GetTime, Endpoint(20), 0, 8, NO_FLAGS);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.reply_statuses(), vec![crate::device::SUCCESS]);
        let copy = ReadclockService::<MemClock, CannedIdentity>::decode_time(
            transport.buffer[8..40].try_into().unwrap(),
        );
        assert_eq!(copy, test_time());
    }

    #[test]
    fn test_get_grant_variant_copies_through_the_grant() {
        let mut svc = service(vec![], Endpoint(0));
        let mut transport = Scripted::new();
        let msg = rtc_msg(RtcRequest::GetTimeGrant, Endpoint(20), 0, 0, NO_FLAGS);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.reply_statuses(), vec![crate::device::SUCCESS]);
        let copy = ReadclockService::<MemClock, CannedIdentity>::decode_time(
            transport.buffer[0..32].try_into().unwrap(),
        );
        assert_eq!(copy, test_time());
    }

    #[test]
    fn test_set_needs_super_user_and_never_reads_the_buffer() {
        let mut svc = service(vec![], Endpoint(0));
        let mut transport = Scripted::new();
        let msg = rtc_msg(RtcRequest::SetTime, Endpoint(20), 0, 8, NO_FLAGS);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.reply_statuses(), vec![-minix_types::EPERM]);
        assert_eq!(svc.face.clock.stored(), test_time());
    }

    #[test]
    fn test_set_fetches_the_buffer_and_writes_the_chip() {
        let caller = Endpoint(30);
        let mut svc = service(vec![caller], Endpoint(0));
        let mut transport = Scripted::new();
        let mut evening = test_time();
        evening.hours = 20;
        transport.buffer[8..40].copy_from_slice(
            &ReadclockService::<MemClock, CannedIdentity>::encode_time(&evening),
        );
        let msg = rtc_msg(RtcRequest::SetTime, caller, 0, 8, NO_FLAGS);
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.reply_statuses(), vec![crate::device::SUCCESS]);
        assert_eq!(svc.face.clock.stored().hours, 20);
    }

    #[test]
    fn test_power_off_needs_the_power_manager() {
        let pm = Endpoint(40);
        let mut svc = service(vec![], pm);
        let mut transport = Scripted::new();
        svc.dispatch(
            &mut transport,
            &rtc_msg(RtcRequest::PowerOff, Endpoint(20), 0, 0, NO_FLAGS),
        );
        assert_eq!(transport.reply_statuses(), vec![-minix_types::EPERM]);
        svc.dispatch(
            &mut transport,
            &rtc_msg(RtcRequest::PowerOff, pm, 0, 0, NO_FLAGS),
        );
        assert_eq!(
            transport.reply_statuses(),
            vec![-minix_types::EPERM, crate::device::SUCCESS]
        );
    }

    #[test]
    fn test_unknown_request_replies_einval() {
        let mut svc = service(vec![], Endpoint(0));
        let mut transport = Scripted::new();
        let msg = Message {
            m_type: 0x1405,
            m_source: Endpoint(20),
            ..Message::default()
        };
        svc.dispatch(&mut transport, &msg);
        assert_eq!(transport.reply_statuses(), vec![crate::device::unknown_call()]);
    }
}
