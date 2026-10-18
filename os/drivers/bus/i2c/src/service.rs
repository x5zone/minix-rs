//! Service assembly: the i2c bus driver's receive-classify-dispatch pump.
//!
//! C correspondence: `i2c_tab` (`i2c.c:69-79`: the control hook plus the
//! generic hook) served by `chardriver_task` (`i2c.c:518`); the device
//! reservations (`do_reserve`/`check_reservation`/`update_reservation`,
//! `i2c.c:103-215`: a device is keyed by the owner's data-store label so
//! a restarted owner can reclaim it, and the VFS may reach an unclaimed
//! device); the exec validation (`validate_ioctl_exec`, `i2c.c:218-260`);
//! the grant-carrying exec (`do_i2c_ioctl_exec`, `i2c.c:263-306`: copy
//! the 280-byte block in, validate, check the reservation, run the
//! controller, copy the block back only on success); and the typed
//! dispatcher (`i2c_other`, `i2c.c:324-360`: `BUSC_I2C_RESERVE` and
//! `BUSC_I2C_EXEC`, everything else answers invalid argument with a
//! reply). The loop shell and the RS birth handshake live in
//! [`minix_driver_rt::runtime::DriverRuntime`].
//!
//! `MINIX_I2C_IOCTL_EXEC` is the encoded control request
//! (`_IOWR('I', 1, minix_i2c_ioctl_exec_t)`, `sys/dev/i2c/i2c.h:25`):
//! `IOC_INOUT | (280 << 16) | ('I' << 8) | 1` under the Minix ioccom
//! rules (`sys/sys/ioccom.h:73-89`); the tests derive it from those
//! numbers so a drift shows up as a failure.
//!
//! Registered seams: the data-store subscription and its drain (a
//! restarted owner's endpoint update, `ds_event`) are not wired, so the
//! DS notification arm stays dormant; the live-update state save
//! (`i2c.<bus>.i2cdev`) is not wired; the bus id is fixed to instance
//! zero, which names labels `i2c.1` (the RS service label itself rides
//! the boot table, A/C).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use minix_chardriver::protocol::CdevRequest;
use minix_driver_rt::runtime::DriverHandler;
use minix_driver_rt::transport::DriverTransport;
use minix_i2cdriver::client::{self, ExecBlock};
use minix_types::{Endpoint, Message, CDEV_REPLY_BASE, EBADF, EBUSY, EINVAL, EIO, ENOTTY, EPERM, OK};

use crate::controller::I2cController;

/// The message union payload begins at byte 8 of [`Message`].
const PAYLOAD: usize = 8;

/// Slave addresses the bus accepts (`NR_I2CDEV 0x3ff`).
pub const NR_I2CDEV: u16 = 0x3FF;

/// Control request for one exec (`MINIX_I2C_IOCTL_EXEC`).
pub const MINIX_I2C_IOCTL_EXEC: u64 = 0xC118_4901;

/// The i2c driver instance (C reads it from the environment; the generic
/// build fixes the documented default, so labels read `i2c.1`).
pub const BUS_INSTANCE: u32 = 0;

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
        pub const GRANT: usize = PAYLOAD + 8;
        pub const REQUEST: usize = PAYLOAD + 24;
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

fn read_u16(msg: &Message, offset: usize) -> u16 {
    u16::from_le_bytes(read_bytes(msg, offset))
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

/// One device reservation: address, owner endpoint, owner label key.
type Reservation = (u16, Endpoint, String);

/// The bus service: reservations plus the controller seam.
pub struct I2cBusService<C: I2cController> {
    bus_instance: u32,
    controller: C,
    reservations: Vec<Reservation>,
}

impl<C: I2cController> I2cBusService<C> {
    /// A service over the given controller with an empty reservation table.
    pub fn new(controller: C) -> Self {
        I2cBusService {
            bus_instance: BUS_INSTANCE,
            controller,
            reservations: Vec::new(),
        }
    }

    /// The controller seam (tests inspect the exec traffic).
    pub fn controller(&self) -> &C {
        &self.controller
    }

    /// The controller seam, mutably (tests script failures).
    pub fn controller_mut(&mut self) -> &mut C {
        &mut self.controller
    }

    /// Bring the controller up; the driver must not enter the loop on a
    /// refusal.
    pub fn start(&mut self) -> Result<(), i32> {
        self.controller.probe()
    }

    /// Send one typed reply: the status rides the message type, as in C.
    fn reply_typed<T: DriverTransport>(&mut self, transport: &mut T, caller: Endpoint, status: i32) {
        let mut message = Message {
            m_type: status,
            ..Message::default()
        };
        let _ = transport.asynsend(caller, &mut message);
    }

    /// Send one character reply with `{ status, id }`.
    fn reply_char<T: DriverTransport>(
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
            // The DS subscription/drain is a registered seam; with it the
            // reservation table would be refreshed here (`ds_event`).
            return;
        }
        let caller = msg.m_source;
        if let Some(request) = CdevRequest::decode(msg.m_type) {
            self.handle_char(transport, request, msg);
            return;
        }
        let status = match msg.m_type {
            client::BUSC_I2C_RESERVE => {
                let address = read_u16(msg, PAYLOAD);
                self.do_reserve(transport, caller, address)
            }
            client::BUSC_I2C_EXEC => {
                let grant = read_u32(msg, PAYLOAD) as i32;
                self.do_exec(transport, caller, grant)
            }
            // C's default arm answers invalid argument with a reply.
            _ => EINVAL,
        };
        self.reply_typed(transport, caller, status);
    }

    /// The character surface: open/close succeed, the control hook serves
    /// the encoded exec request, everything else follows the framework
    /// defaults.
    fn handle_char<T: DriverTransport>(&mut self, transport: &mut T, request: CdevRequest, msg: &Message) {
        let caller = msg.m_source;
        match request {
            CdevRequest::Open | CdevRequest::Close => {
                let id = read_u32(msg, cwire::openclose::ID);
                self.reply_char(transport, caller, OK, id);
            }
            CdevRequest::Ioctl => {
                let id = read_u32(msg, cwire::readwrite::ID);
                let code = read_u64(msg, cwire::readwrite::REQUEST);
                let status = if code == MINIX_I2C_IOCTL_EXEC {
                    let grant = read_u32(msg, cwire::readwrite::GRANT) as i32;
                    self.do_exec(transport, caller, grant)
                } else {
                    ENOTTY
                };
                self.reply_char(transport, caller, status, id);
            }
            CdevRequest::Read | CdevRequest::Write => {
                let id = read_u32(msg, cwire::readwrite::ID);
                self.reply_char(transport, caller, EIO, id);
            }
            CdevRequest::Select => self.reply_char(transport, caller, EBADF, 0),
            CdevRequest::Cancel => {}
        }
    }

    /// The label key a driver's reservation carries
    /// (`drv.i2c.<bus>.<label>`, `i2c.c:118-120`).
    fn reservation_key<T: DriverTransport>(
        &self,
        transport: &mut T,
        caller: Endpoint,
    ) -> Result<String, i32> {
        let label = transport.label_name(caller).ok_or(EIO)?;
        Ok(format!("drv.i2c.{}.{}", self.bus_instance + 1, label))
    }

    /// Claim an unclaimed device, or refresh the endpoint of the same
    /// key (`do_reserve`, `i2c.c:103-148`).
    fn do_reserve<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        address: u16,
    ) -> i32 {
        if address >= NR_I2CDEV {
            return EINVAL;
        }
        let key = match self.reservation_key(transport, caller) {
            Ok(key) => key,
            Err(status) => return status,
        };
        match self.reservations.iter_mut().find(|entry| entry.0 == address) {
            Some(entry) => {
                if entry.2 != key {
                    return EBUSY;
                }
                entry.1 = caller;
                OK
            }
            None => {
                self.reservations.push((address, caller, key));
                OK
            }
        }
    }

    /// Permission check for one operation (`check_reservation`,
    /// `i2c.c:150-181`): the VFS may reach an unclaimed device; a direct
    /// sender must own the device.
    fn check_reservation(&self, caller: Endpoint, address: u16) -> Result<(), i32> {
        if address >= NR_I2CDEV {
            return Err(EINVAL);
        }
        match self.reservations.iter().find(|entry| entry.0 == address) {
            None => {
                if caller == Endpoint::VFS {
                    Ok(())
                } else {
                    Err(EPERM)
                }
            }
            Some(entry) => {
                if entry.1 != caller {
                    Err(EBUSY)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Copy in, validate, check, run, copy out (`do_i2c_ioctl_exec`,
    /// `i2c.c:263-306`).
    fn do_exec<T: DriverTransport>(
        &mut self,
        transport: &mut T,
        caller: Endpoint,
        grant: i32,
    ) -> i32 {
        let mut block = ExecBlock::zeroed();
        if let Err(status) = copy_block_in(transport, caller, grant, &mut block) {
            return status;
        }
        if let Err(status) = validate(&block) {
            return status;
        }
        if let Err(status) = self.check_reservation(caller, block.address) {
            return status;
        }
        let status = self.controller.process(&mut block);
        if status != 0 {
            // C skips the result copy on a controller failure.
            return status;
        }
        if let Err(status) = copy_block_out(transport, caller, grant, &block) {
            return status;
        }
        OK
    }
}

/// Copy the 280-byte exec block out of the caller's grant.
fn copy_block_in<T: DriverTransport>(
    transport: &mut T,
    caller: Endpoint,
    grant: i32,
    block: &mut ExecBlock,
) -> Result<(), i32> {
    let bytes = unsafe {
        // SAFETY: a live `&mut ExecBlock` reborrowed as bytes for the
        // duration of the copy; the C block is plain data.
        ::core::slice::from_raw_parts_mut((block as *mut ExecBlock).cast::<u8>(), ::core::mem::size_of::<ExecBlock>())
    };
    transport.copy_from_grant(caller, grant, 0, bytes)
}

/// Copy the exec block back into the caller's grant.
fn copy_block_out<T: DriverTransport>(
    transport: &mut T,
    caller: Endpoint,
    grant: i32,
    block: &ExecBlock,
) -> Result<(), i32> {
    let bytes = unsafe {
        // SAFETY: a live `&ExecBlock` reborrowed as bytes for the
        // duration of the copy; the C block is plain data.
        ::core::slice::from_raw_parts((block as *const ExecBlock).cast::<u8>(), ::core::mem::size_of::<ExecBlock>())
    };
    transport.copy_to_grant(caller, grant, 0, bytes)
}

/// Field validation (`validate_ioctl_exec`, `i2c.c:218-260`).
fn validate(block: &ExecBlock) -> Result<(), i32> {
    match block.op {
        client::I2C_OP_READ
        | client::I2C_OP_READ_WITH_STOP
        | client::I2C_OP_WRITE
        | client::I2C_OP_WRITE_WITH_STOP
        | client::I2C_OP_READ_BLOCK
        | client::I2C_OP_WRITE_BLOCK => {}
        _ => return Err(EINVAL),
    }
    if block.address >= NR_I2CDEV {
        return Err(EINVAL);
    }
    if block.cmd_len > client::EXEC_CMD_MAX as u64 {
        return Err(EINVAL);
    }
    if block.buf_len > client::EXEC_BUF_MAX as u64 {
        return Err(EINVAL);
    }
    Ok(())
}

impl<C: I2cController> Default for I2cBusService<C>
where
    C: Default,
{
    fn default() -> Self {
        Self::new(C::default())
    }
}

impl<C: I2cController, T: DriverTransport> DriverHandler<T> for I2cBusService<C> {
    fn handle(&mut self, transport: &mut T, msg: &Message) {
        self.dispatch(transport, msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::vec::Vec;

    use crate::controller::NullController;

    /// Controller spy: records calls, can fill one buffer byte, can fail.
    #[derive(Default)]
    struct SpyController {
        calls: u32,
        fail_with: Option<i32>,
        fill: Option<u8>,
    }

    impl I2cController for SpyController {
        fn probe(&mut self) -> Result<(), i32> {
            Ok(())
        }
        fn process(&mut self, block: &mut ExecBlock) -> i32 {
            self.calls += 1;
            if let Some(status) = self.fail_with {
                return status;
            }
            if let Some(byte) = self.fill {
                block.buf[0] = byte;
            }
            0
        }
    }

    /// Scripted transport: label map, grant copies, reply log.
    #[derive(Default)]
    struct Scripted {
        labels: HashMap<i32, String>,
        in_block: Option<ExecBlock>,
        copied_out: Vec<(Endpoint, i32, Vec<u8>)>,
        replies: Vec<(Endpoint, i32)>,
    }

    impl Scripted {
        fn with_labels(entries: &[(i32, &str)]) -> Self {
            let mut transport = Scripted::default();
            for (endpoint, label) in entries {
                transport.labels.insert(*endpoint, (*label).into());
            }
            transport
        }
    }

    impl DriverTransport for Scripted {
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
            self.replies.push((dst, msg.m_type));
            Ok(())
        }
        fn copy_from_grant(
            &mut self,
            _granter: Endpoint,
            _grant: i32,
            _offset: u64,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            let block = self.in_block.as_ref().expect("a programmed block");
            let bytes = unsafe {
                // SAFETY: a live block reborrowed as bytes for the copy.
                ::core::slice::from_raw_parts(
                    (block as *const ExecBlock).cast::<u8>(),
                    ::core::mem::size_of::<ExecBlock>(),
                )
            };
            let take = buf.len().min(bytes.len());
            buf[..take].copy_from_slice(&bytes[..take]);
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
        fn label_name(&mut self, endpoint: Endpoint) -> Option<String> {
            self.labels.get(&endpoint.0).cloned()
        }
    }

    fn block_of(op: i32, address: u16, cmd_len: u64, buf_len: u64) -> ExecBlock {
        let mut block = ExecBlock::zeroed();
        block.op = op;
        block.address = address;
        block.cmd_len = cmd_len;
        block.buf_len = buf_len;
        block
    }

    fn typed(message_type: i32, payload: &[u8]) -> Message {
        let mut msg = Message {
            m_type: message_type,
            m_source: Endpoint(5),
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: test-only writer into the fixed-size message body.
        unsafe {
            ::core::ptr::copy_nonoverlapping(payload.as_ptr(), base.add(PAYLOAD), payload.len());
        }
        msg
    }

    #[test]
    fn test_ioctl_code_matches_the_ioccom_arithmetic() {
        const IOC_INOUT: u64 = 0x8000_0000 | 0x4000_0000;
        let derived = IOC_INOUT
            | (((::core::mem::size_of::<ExecBlock>() as u64) & 0xfff) << 16)
            | ((b'I' as u64) << 8)
            | 1;
        assert_eq!(derived, MINIX_I2C_IOCTL_EXEC);
        assert_eq!(::core::mem::size_of::<ExecBlock>(), 280);
    }

    #[test]
    fn test_null_controller_refuses_to_start() {
        let mut service = I2cBusService::new(NullController);
        assert_eq!(service.start(), Err(-minix_types::ENODEV));
        let mut service = I2cBusService::new(SpyController::default());
        assert_eq!(service.start(), Ok(()));
    }

    #[test]
    fn test_reserve_semantics_are_label_keyed() {
        let mut service = I2cBusService::new(SpyController::default());
        let mut transport = Scripted::with_labels(&[
            (5, "sensorA"),
            (6, "sensorB"),
            (7, "sensorA"),
        ]);

        // Reserve succeeds for the first caller.
        let msg = typed(client::BUSC_I2C_RESERVE, &0x77u16.to_le_bytes());
        let mut msg5 = msg;
        msg5.m_source = Endpoint(5);
        service.dispatch(&mut transport, &msg5);
        assert_eq!(transport.replies.last().unwrap().1, OK);

        // A different label gets "busy".
        let mut msg6 = msg;
        msg6.m_source = Endpoint(6);
        service.dispatch(&mut transport, &msg6);
        assert_eq!(transport.replies.last().unwrap().1, EBUSY);

        // The same label from a new endpoint reclaims (restart path).
        let mut msg7 = msg;
        msg7.m_source = Endpoint(7);
        service.dispatch(&mut transport, &msg7);
        assert_eq!(transport.replies.last().unwrap().1, OK);
        assert_eq!(service.reservations[0].1, Endpoint(7));

        // Out of range and unknown labels are refused.
        let mut msg = typed(client::BUSC_I2C_RESERVE, &0x0400u16.to_le_bytes());
        msg.m_source = Endpoint(5);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, EINVAL);

        let mut msg = typed(client::BUSC_I2C_RESERVE, &0x77u16.to_le_bytes());
        msg.m_source = Endpoint(99);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, EIO);
    }

    #[test]
    fn test_exec_respects_the_reservation() {
        let mut service = I2cBusService::new(SpyController::default());
        let mut transport = Scripted::with_labels(&[(5, "sensorA"), (6, "sensorB")]);
        transport.in_block = Some(block_of(client::I2C_OP_READ_WITH_STOP, 0x77, 1, 1));

        // Unclaimed device from a direct sender: not permitted.
        let mut msg = typed(client::BUSC_I2C_EXEC, &7i32.to_le_bytes());
        msg.m_source = Endpoint(5);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, EPERM);
        assert_eq!(service.controller().calls, 0);

        // The VFS may reach an unclaimed device.
        let mut msg = typed(client::BUSC_I2C_EXEC, &7i32.to_le_bytes());
        msg.m_source = Endpoint::VFS;
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, OK);
        assert_eq!(service.controller().calls, 1);

        // Claim it, then a stranger is busy while the owner runs.
        let mut reserve = typed(client::BUSC_I2C_RESERVE, &0x77u16.to_le_bytes());
        reserve.m_source = Endpoint(5);
        service.dispatch(&mut transport, &reserve);
        let mut msg = typed(client::BUSC_I2C_EXEC, &7i32.to_le_bytes());
        msg.m_source = Endpoint(6);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, EBUSY);
        let mut msg = typed(client::BUSC_I2C_EXEC, &7i32.to_le_bytes());
        msg.m_source = Endpoint(5);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, OK);
        assert_eq!(service.controller().calls, 2);
    }

    #[test]
    fn test_exec_validation_and_controller_failure() {
        let mut service = I2cBusService::new(SpyController {
            fill: Some(0xAB),
            ..SpyController::default()
        });
        let mut transport = Scripted::with_labels(&[(5, "sensorA")]);

        // Invalid operation, address, and lengths answer EINVAL.
        for block in [
            block_of(9, 0x77, 0, 0),
            block_of(client::I2C_OP_READ_WITH_STOP, 0x400, 0, 0),
            block_of(client::I2C_OP_READ_WITH_STOP, 0x77, 200, 0),
            block_of(client::I2C_OP_READ_WITH_STOP, 0x77, 0, 200),
        ] {
            transport.in_block = Some(block);
            let mut msg = typed(client::BUSC_I2C_EXEC, &7i32.to_le_bytes());
            msg.m_source = Endpoint(5);
            service.dispatch(&mut transport, &msg);
            assert_eq!(transport.replies.last().unwrap().1, EINVAL);
        }
        assert_eq!(service.controller().calls, 0);

        // Claim the device for the caller, then: a successful exec copies
        // the block back with the controller's bytes; a failing controller
        // skips the copy.
        let mut reserve = typed(client::BUSC_I2C_RESERVE, &0x77u16.to_le_bytes());
        reserve.m_source = Endpoint(5);
        service.dispatch(&mut transport, &reserve);
        assert_eq!(transport.replies.last().unwrap().1, OK);
        transport.in_block = Some(block_of(client::I2C_OP_READ_WITH_STOP, 0x77, 1, 1));
        let mut msg = typed(client::BUSC_I2C_EXEC, &7i32.to_le_bytes());
        msg.m_source = Endpoint(5);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, OK);
        assert_eq!(transport.copied_out.len(), 1);
        assert_eq!(transport.copied_out[0].2[144], 0xAB, "the controller's byte");

        service.controller_mut().fail_with = Some(EIO);
        let before = transport.copied_out.len();
        let mut msg = typed(client::BUSC_I2C_EXEC, &7i32.to_le_bytes());
        msg.m_source = Endpoint(5);
        service.dispatch(&mut transport, &msg);
        assert_eq!(transport.replies.last().unwrap().1, EIO);
        assert_eq!(transport.copied_out.len(), before, "no copy on failure");
    }

    #[test]
    fn test_ioctl_leg_and_defaults() {
        let mut service = I2cBusService::new(SpyController::default());
        let mut transport = Scripted::with_labels(&[(5, "sensorA")]);
        transport.in_block = Some(block_of(client::I2C_OP_READ_WITH_STOP, 0x77, 0, 0));

        // The encoded exec request runs the exec path.
        let mut msg = Message {
            m_type: CdevRequest::Ioctl.message_type(),
            m_source: Endpoint::VFS,
            ..Message::default()
        };
        let base = ::core::ptr::addr_of_mut!(msg) as *mut u8;
        // SAFETY: test-only writers into the pinned readwrite layout.
        unsafe {
            ::core::ptr::copy_nonoverlapping(
                MINIX_I2C_IOCTL_EXEC.to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::REQUEST),
                8,
            );
            ::core::ptr::copy_nonoverlapping(
                7u32.to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::GRANT),
                4,
            );
            ::core::ptr::copy_nonoverlapping(
                3u32.to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::ID),
                4,
            );
        }
        service.dispatch(&mut transport, &msg);
        assert_eq!(service.controller().calls, 1);
        assert_eq!(transport.replies.last().unwrap().1, CDEV_REPLY_BASE);

        // A foreign ioctl answers ENOTTY; the rest keep the defaults.
        let mut foreign = msg;
        let base = ::core::ptr::addr_of_mut!(foreign) as *mut u8;
        // SAFETY: same writer as above.
        unsafe {
            ::core::ptr::copy_nonoverlapping(
                0x1234u64.to_le_bytes().as_ptr(),
                base.add(cwire::readwrite::REQUEST),
                8,
            );
        }
        let before = transport.replies.len();
        service.dispatch(&mut transport, &foreign);
        assert_eq!(transport.replies.len(), before + 1);
        assert_eq!(service.controller().calls, 1);

        for kind in [CdevRequest::Open, CdevRequest::Close, CdevRequest::Read, CdevRequest::Write, CdevRequest::Select] {
            let msg = Message {
                m_type: kind.message_type(),
                m_source: Endpoint(5),
                ..Message::default()
            };
            service.dispatch(&mut transport, &msg);
        }
        // C's unknown-type arm answers invalid argument with a reply.
        let unknown = Message {
            m_type: 0x2222,
            m_source: Endpoint(5),
            ..Message::default()
        };
        service.dispatch(&mut transport, &unknown);
        assert_eq!(transport.replies.last().unwrap().1, EINVAL);
    }
}
