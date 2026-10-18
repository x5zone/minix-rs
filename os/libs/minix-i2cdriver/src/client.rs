//! The i2c client half: bus lookup, device reservation, and exec requests.
//!
//! C correspondence: `minix3/minix/lib/libi2cdriver/i2cdriver.c` —
//! `i2cdriver_bus_endpoint` (`:81-98`), `i2cdriver_reserve_device`
//! (`:155-172`), `i2cdriver_exec` (`:173-194`), and the register helpers
//! (`__i2creg_read`/`__i2creg_write`, `:196-322`). The label and key
//! strings live in [`crate::keys`]; this module owns the messages and the
//! grant-carrying exec block.
//!
//! Wire: `BUSC_I2C_RESERVE` carries a `uint16_t` address at the first
//! payload word (`ipc.h:982-990`); `BUSC_I2C_EXEC` carries a
//! `cp_grant_id_t` there (`ipc.h:973-980`). The exec block mirrors
//! `minix_i2c_ioctl_exec_t` (`i2c.h:16-23`): a 280-byte record with two
//! 128-byte arrays, granted read-write to the bus driver for the duration
//! of the call (`cpf_grant_direct`, `i2cdriver.c:179-181`).

use minix_driver_rt::transport::DriverTransport;
use minix_types::{Endpoint, Message};

/// Bus request base (`BUSC_RQ_BASE`, `com.h:102`).
pub const BUS_REQUEST_BASE: i32 = 0x300;
/// Reserve one device (`BUSC_I2C_RESERVE`, base plus sixty-four).
pub const BUSC_I2C_RESERVE: i32 = BUS_REQUEST_BASE + 64;
/// Execute one i2c transaction (`BUSC_I2C_EXEC`, base plus sixty-five).
pub const BUSC_I2C_EXEC: i32 = BUS_REQUEST_BASE + 65;

/// Operation kinds (`i2c_io.h:57-63`).
pub const I2C_OP_READ: i32 = 0;
pub const I2C_OP_READ_WITH_STOP: i32 = 1;
pub const I2C_OP_WRITE: i32 = 2;
pub const I2C_OP_WRITE_WITH_STOP: i32 = 3;
pub const I2C_OP_READ_BLOCK: i32 = 5;
pub const I2C_OP_WRITE_BLOCK: i32 = 7;

/// Command and data capacity (`I2C_EXEC_MAX_*`, Minix arm: 128).
pub const EXEC_CMD_MAX: usize = 128;
pub const EXEC_BUF_MAX: usize = 128;

/// The message union payload begins at byte 8 of [`Message`].
const PAYLOAD: usize = 8;

/// The exec block (`minix_i2c_ioctl_exec_t`, `i2c.h:16-23`).
///
/// Field order and widths copy the C struct; the tests pin every offset
/// through `offset_of!` so a layout drift fails before a bus driver reads
/// the wrong field.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ExecBlock {
    /// Operation to perform.
    pub op: i32,
    /// Address of the device on the bus.
    pub address: u16,
    /// Command bytes (a register address, usually).
    pub cmd: [u8; EXEC_CMD_MAX],
    /// Significant command bytes.
    pub cmd_len: u64,
    /// Data bytes (written to the device or read back).
    pub buf: [u8; EXEC_BUF_MAX],
    /// Significant data bytes.
    pub buf_len: u64,
}

impl ExecBlock {
    /// A zeroed block.
    pub const fn zeroed() -> ExecBlock {
        ExecBlock {
            op: I2C_OP_READ,
            address: 0,
            cmd: [0; EXEC_CMD_MAX],
            cmd_len: 0,
            buf: [0; EXEC_BUF_MAX],
            buf_len: 0,
        }
    }
}

impl Default for ExecBlock {
    fn default() -> Self {
        Self::zeroed()
    }
}

#[cfg(test)]
fn read_u16_slot(msg: &Message) -> u16 {
    let base = ::core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; 2];
    // SAFETY: the reserve payload starts with a `uint16_t` at the union
    // base (`ipc.h:982-990`), inside the fixed-size message body.
    unsafe {
        ::core::ptr::copy_nonoverlapping(base.add(PAYLOAD), bytes.as_mut_ptr(), 2);
    }
    u16::from_le_bytes(bytes)
}

fn write_u16_slot(msg: &mut Message, value: u16) {
    let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: same layout as `read_u16_slot`.
    unsafe {
        ::core::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), base.add(PAYLOAD), 2);
    }
}

#[cfg(test)]
fn read_i32_slot(msg: &Message) -> i32 {
    let base = ::core::ptr::addr_of!(*msg) as *const u8;
    let mut bytes = [0u8; 4];
    // SAFETY: the exec payload starts with a `cp_grant_id_t` at the union
    // base (`ipc.h:973-980`).
    unsafe {
        ::core::ptr::copy_nonoverlapping(base.add(PAYLOAD), bytes.as_mut_ptr(), 4);
    }
    i32::from_le_bytes(bytes)
}

fn write_i32_slot(msg: &mut Message, value: i32) {
    let base = ::core::ptr::addr_of_mut!(*msg) as *mut u8;
    // SAFETY: same layout as `read_i32_slot`.
    unsafe {
        ::core::ptr::copy_nonoverlapping(value.to_le_bytes().as_ptr(), base.add(PAYLOAD), 4);
    }
}

/// Resolve the bus driver's endpoint from the DS label (`i2c.<bus>`).
pub fn bus_endpoint<T: DriverTransport>(transport: &mut T, bus: u32) -> Option<Endpoint> {
    transport.lookup_label(&alloc::format!("i2c.{bus}"))
}

/// Reserve the device at `address` (`i2cdriver_reserve_device`).
///
/// The bus answers its own status in the reply type (`OK`, `EBUSY`,
/// `EINVAL`); anything but zero is returned to the caller.
pub fn reserve<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
) -> Result<(), i32> {
    let mut msg = Message {
        m_type: BUSC_I2C_RESERVE,
        ..Message::default()
    };
    write_u16_slot(&mut msg, address);
    transport.send_rec(bus, &mut msg)?;
    if msg.m_type == 0 {
        Ok(())
    } else {
        Err(msg.m_type)
    }
}

/// Run one exec block (`i2cdriver_exec`).
///
/// The block is granted read-write to the bus driver, its grant id rides
/// the request, and the reply type carries the transaction status. The
/// grant is revoked whether or not the call succeeded, as in C.
pub fn exec<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    block: &mut ExecBlock,
) -> Result<(), i32> {
    let address = block as *mut ExecBlock as u64;
    let grant = transport.grant_direct(bus, address, ::core::mem::size_of::<ExecBlock>(), true)?;
    let mut msg = Message {
        m_type: BUSC_I2C_EXEC,
        ..Message::default()
    };
    write_i32_slot(&mut msg, grant);
    let outcome = transport.send_rec(bus, &mut msg);
    //[SD19-gate] C ignores the revoke answer (`i2cdriver.c:186-187`); the
    // transaction outcome is decided by the call above.
    #[allow(clippy::let_underscore_must_use)]
    let _ = transport.grant_revoke(grant);
    outcome?;
    if msg.m_type == 0 {
        Ok(())
    } else {
        Err(msg.m_type)
    }
}

/// Read `data.len()` bytes from a register (`__i2creg_read`).
///
/// The register address travels in the command array; the data comes back
/// in the buffer array (`i2cdriver.c:196-240`).
pub fn read_bytes<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    register: u8,
    data: &mut [u8],
) -> Result<(), i32> {
    if data.is_empty() || data.len() > EXEC_BUF_MAX {
        return Err(-minix_types::EINVAL);
    }
    let mut block = ExecBlock::zeroed();
    block.op = I2C_OP_READ_WITH_STOP;
    block.address = address;
    block.cmd[0] = register;
    block.cmd_len = 1;
    block.buf_len = data.len() as u64;
    exec(transport, bus, &mut block)?;
    data.copy_from_slice(&block.buf[..data.len()]);
    Ok(())
}

/// Write one raw byte without a register address (`i2creg_raw_write8`).
///
/// The whole payload is the value (`i2cdriver.c:292-296`).
pub fn write_raw_byte<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    value: u8,
) -> Result<(), i32> {
    let mut block = ExecBlock::zeroed();
    block.op = I2C_OP_WRITE_WITH_STOP;
    block.address = address;
    block.buf[0] = value;
    block.buf_len = 1;
    exec(transport, bus, &mut block)
}

/// Write one register with one byte (`__i2creg_write`).
///
/// Both the register address and the value travel in the buffer array
/// (`i2cdriver.c:280-308`).
pub fn write_byte<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    register: u8,
    value: u8,
) -> Result<(), i32> {
    let mut block = ExecBlock::zeroed();
    block.op = I2C_OP_WRITE_WITH_STOP;
    block.address = address;
    block.buf[0] = register;
    block.buf[1] = value;
    block.buf_len = 2;
    exec(transport, bus, &mut block)
}

/// Read one register byte (`i2creg_read8`).
pub fn read_byte<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    register: u8,
) -> Result<u8, i32> {
    let mut byte = [0u8; 1];
    read_bytes(transport, bus, address, register, &mut byte)?;
    Ok(byte[0])
}

/// Big-endian sixteen-bit register read (`i2creg_read16`).
pub fn read_u16<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    register: u8,
) -> Result<u16, i32> {
    let mut bytes = [0u8; 2];
    read_bytes(transport, bus, address, register, &mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

/// Big-endian twenty-four-bit register read (`i2creg_read24`).
pub fn read_u24<T: DriverTransport>(
    transport: &mut T,
    bus: Endpoint,
    address: u16,
    register: u8,
) -> Result<u32, i32> {
    let mut bytes = [0u8; 3];
    read_bytes(transport, bus, address, register, &mut bytes)?;
    Ok(((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    use core::mem::{offset_of, size_of};

    /// The C-layout mirror (`minix_i2c_ioctl_exec_t`).
    #[repr(C)]
    struct Mirror {
        op: i32,
        address: u16,
        cmd: [u8; 128],
        cmd_len: u64,
        buf: [u8; 128],
        buf_len: u64,
    }

    #[test]
    fn test_exec_block_matches_the_c_struct() {
        assert_eq!(size_of::<ExecBlock>(), 280);
        assert_eq!(size_of::<Mirror>(), 280);
        assert_eq!(offset_of!(ExecBlock, op), 0);
        assert_eq!(offset_of!(ExecBlock, address), 4);
        assert_eq!(offset_of!(ExecBlock, cmd), 6);
        assert_eq!(offset_of!(ExecBlock, cmd_len), 136);
        assert_eq!(offset_of!(ExecBlock, buf), 144);
        assert_eq!(offset_of!(ExecBlock, buf_len), 272);
    }

    /// Scripted transport: a fake bus that answers reserve/exec.
    struct FakeBus {
        endpoint: Endpoint,
        reserve_reply: i32,
        block_address: u64,
        grant_id: i32,
        revoked: Vec<i32>,
        exec_actions: Vec<(i32, u16, u64, u64)>,
        exec_reply: i32,
        /// Bytes the fake device returns through the grant.
        reply_bytes: [u8; 3],
        reply_len: usize,
    }

    impl FakeBus {
        fn new() -> Self {
            FakeBus {
                endpoint: Endpoint(42),
                reserve_reply: 0,
                block_address: 0,
                grant_id: 7,
                revoked: Vec::new(),
                exec_actions: Vec::new(),
                exec_reply: 0,
                reply_bytes: [0; 3],
                reply_len: 0,
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
            assert_eq!(dst, self.endpoint);
            match msg.m_type {
                BUSC_I2C_RESERVE => {
                    assert_eq!(read_u16_slot(msg), 0x77);
                    msg.m_type = self.reserve_reply;
                }
                BUSC_I2C_EXEC => {
                    assert_eq!(read_i32_slot(msg), self.grant_id);
                    let block = self.block_address as *mut ExecBlock;
                    // SAFETY: the test granted its own block; the transport
                    // recorded its address in `grant_direct`.
                    let block = unsafe { &mut *block };
                    self.exec_actions
                        .push((block.op, block.address, block.cmd_len, block.buf_len));
                    block.buf[..self.reply_len]
                        .copy_from_slice(&self.reply_bytes[..self.reply_len]);
                    msg.m_type = self.exec_reply;
                }
                _ => unreachable!(),
            }
            Ok(())
        }
        fn asynsend(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
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
        fn lookup_label(&mut self, label: &str) -> Option<Endpoint> {
            if label == "i2c.1" { Some(self.endpoint) } else { None }
        }
        fn grant_direct(
            &mut self,
            dest: Endpoint,
            address: u64,
            len: usize,
            write: bool,
        ) -> Result<i32, i32> {
            assert_eq!(dest, self.endpoint);
            assert_eq!(len, size_of::<ExecBlock>());
            assert!(write, "the exec block is granted read-write");
            self.block_address = address;
            Ok(self.grant_id)
        }
        fn grant_revoke(&mut self, grant: i32) -> Result<(), i32> {
            self.revoked.push(grant);
            Ok(())
        }
    }

    #[test]
    fn test_bus_endpoint_follows_the_label() {
        let mut bus = FakeBus::new();
        assert_eq!(bus_endpoint(&mut bus, 1), Some(Endpoint(42)));
        assert_eq!(bus_endpoint(&mut bus, 2), None);
    }

    #[test]
    fn test_reserve_maps_the_bus_reply() {
        let mut bus = FakeBus::new();
        let endpoint = bus.endpoint;
        assert_eq!(reserve(&mut bus, endpoint, 0x77), Ok(()));
        bus.reserve_reply = minix_types::EBUSY;
        assert_eq!(
            reserve(&mut bus, endpoint, 0x77),
            Err(minix_types::EBUSY)
        );
    }

    #[test]
    fn test_read_u16_and_u24_run_exec_and_revoke() {
        let mut bus = FakeBus::new();
        bus.reply_bytes = [0x12, 0x34, 0x56];
        bus.reply_len = 2;
        let endpoint = bus.endpoint;
        let value = read_u16(&mut bus, endpoint, 0x77, 0xAA).unwrap();
        assert_eq!(value, 0x1234);
        assert_eq!(bus.exec_actions[0].0, I2C_OP_READ_WITH_STOP);
        assert_eq!(bus.exec_actions[0].1, 0x77);
        assert_eq!(bus.exec_actions[0].2, 1, "one command byte");
        assert_eq!(bus.exec_actions[0].3, 2, "two data bytes");
        assert_eq!(bus.revoked, vec![7]);

        bus.reply_len = 3;
        let value = read_u24(&mut bus, endpoint, 0x77, 0xF6).unwrap();
        assert_eq!(value, 0x123456);
        assert_eq!(bus.revoked, vec![7, 7]);
    }

    #[test]
    fn test_write_byte_puts_register_and_value_in_the_buffer() {
        let mut bus = FakeBus::new();
        let endpoint = bus.endpoint;
        write_byte(&mut bus, endpoint, 0x77, 0xF4, 0x2E).unwrap();
        assert_eq!(bus.exec_actions[0].0, I2C_OP_WRITE_WITH_STOP);
        assert_eq!(bus.exec_actions[0].2, 0, "no command bytes");
        assert_eq!(bus.exec_actions[0].3, 2, "register plus value in the buffer");
        assert_eq!(bus.revoked, vec![7]);
    }

    #[test]
    fn test_exec_failure_propagates_and_still_revokes() {
        let mut bus = FakeBus::new();
        bus.exec_reply = minix_types::EIO;
        bus.reply_len = 2;
        let endpoint = bus.endpoint;
        assert_eq!(
            read_u16(&mut bus, endpoint, 0x77, 0xAA),
            Err(minix_types::EIO)
        );
        assert_eq!(bus.revoked, vec![7]);
    }
}
