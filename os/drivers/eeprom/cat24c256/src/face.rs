//! Block face: the EEPROM's transfer legs over i2c.
//!
//! C correspondence: `cat24c256.c`'s block callbacks — `cat24c256_blk_open`/
//! `_close` (the per-minor open counter, `cat24c256.c:216-240`), the
//! transfer loop (`cat24c256_blk_transfer`, `cat24c256.c:104-200`: slice
//! by `COPYBUF_SIZE`, copy the caller's bytes through the data plane, and
//! book the totals), the two bus legs (`cat24c256_read128`/
//! `cat24c256_read`, `cat24c256.c:251-319`; `cat24c256_write16`/
//! `cat24c256_write`, `cat24c256.c:322-390`), and the partition report
//! (`cat24c256_blk_part`, `cat24c256.c:203-214`).
//!
//! The bus traffic is the part the plane made possible: C's hook calls
//! `sys_safecopyfrom`/`sys_safecopyto` and `i2cdriver_exec` directly, and
//! this face does the same two things through
//! [`minix_driver_rt::plane::TransferPlane`] (the copies) and the i2c
//! client's exec leg (the bus transaction).

use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::protocol::{BlockDriverType, DeviceExtent, DeviceMinor};
use minix_driver_rt::plane::TransferPlane;
use minix_i2cdriver::client::{
    self, EXEC_BUF_MAX, ExecBlock, I2C_OP_READ_WITH_STOP, I2C_OP_WRITE_WITH_STOP,
};
use minix_types::{ENOTTY, Endpoint, OK};

use crate::pages::{DEVICE_SIZE, address_width};

/// Number of devices this driver handles (`NR_DEVS`).
pub const NR_DEVS: usize = 1;
/// Index of the EEPROM device (`EEPROM_DEV`).
pub const EEPROM_DEV: u32 = 0;
/// The driver's staging buffer (`COPYBUF_SIZE`, 4 KiB).
pub const COPYBUF_SIZE: usize = 0x1000;
/// The slave addresses the driver accepts, terminator included
/// (`valid_addrs[9]`, `cat24c256.c:21-23`).
pub const VALID_ADDRESSES: [u16; 9] = [0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x00];
/// The request flag saying the page address is already selected
/// (`BDEV_NOPAGE`, `com.h:988`).
pub const BDEV_NOPAGE: i32 = 0x02;

/// Whether `address` is one the driver accepts (`i2cdriver_env_parse`'s
/// address table, `cat24c256.c:474-481`).
pub fn valid_address(address: u16) -> bool {
    VALID_ADDRESSES[..VALID_ADDRESSES.len() - 1].contains(&address)
}

/// The EEPROM's block face: one device, one bus binding.
pub struct Cat24c256Face {
    /// The bus number this device sits on (C's `bus`).
    bus: u32,
    /// The slave address (C's `address`).
    address: u16,
    /// The bus driver's endpoint (C's `bus_endpoint`), refreshed when the
    /// data store reports the bus came back.
    bus_endpoint: Option<Endpoint>,
    /// Per-minor open counters (C's `openct[]`).
    openct: [i32; NR_DEVS],
    /// The staging buffer the transfer legs slice through (C's `copybuf`).
    copybuf: [u8; COPYBUF_SIZE],
}

impl Cat24c256Face {
    /// A face bound to one bus and address, with no bus driver resolved
    /// yet.
    pub fn new(bus: u32, address: u16) -> Self {
        Cat24c256Face {
            bus,
            address,
            bus_endpoint: None,
            openct: [0; NR_DEVS],
            copybuf: [0; COPYBUF_SIZE],
        }
    }

    /// The bus number.
    pub fn bus(&self) -> u32 {
        self.bus
    }

    /// The slave address.
    pub fn address(&self) -> u16 {
        self.address
    }

    /// The resolved bus endpoint, if the bus driver was found.
    pub fn bus_endpoint(&self) -> Option<Endpoint> {
        self.bus_endpoint
    }

    /// Bind the bus endpoint (startup resolution and the refresh after a
    /// bus-update notification).
    pub fn set_bus_endpoint(&mut self, endpoint: Option<Endpoint>) {
        self.bus_endpoint = endpoint;
    }

    /// The device extent C installs at startup (`geom[EEPROM_DEV]`,
    /// `cat24c256.c:427-428`).
    pub fn extent(&self) -> DeviceExtent {
        DeviceExtent {
            base: 0,
            size: DEVICE_SIZE,
        }
    }

    /// The transfer loop (`cat24c256_blk_transfer`, `cat24c256.c:104-200`):
    /// clip to the device size and the staging buffer, move the bytes
    /// through the plane, and book the totals.
    fn transfer_bytes(
        &mut self,
        plane: &mut dyn TransferPlane,
        do_write: bool,
        position: u64,
        count: u64,
        flags: i32,
    ) -> i64 {
        let Some(bus) = self.bus_endpoint else {
            return -(minix_types::ENOENT as i64);
        };
        let address = self.address;
        let extent = self.extent();
        if position >= extent.base + extent.size {
            // Beyond EOF answers zero.
            return 0;
        }
        let mut position = position;
        let mut offset = 0u64;
        let mut total = 0i64;
        let mut remaining = count;
        while remaining > 0 {
            // Do not go past the end of the device, nor overflow the
            // staging buffer.
            let mut slice = remaining.min(extent.base + extent.size - position);
            slice = slice.min(COPYBUF_SIZE as u64);
            if slice == 0 {
                break;
            }
            let len = slice as usize;
            if do_write {
                if let Err(status) = plane.copy_from_caller(offset, &mut self.copybuf[..len]) {
                    return -(status.abs() as i64);
                }
                if let Err(status) = write(
                    plane,
                    bus,
                    address,
                    position as u16,
                    &self.copybuf[..len],
                    flags,
                ) {
                    return -(status.abs() as i64);
                }
            } else {
                if let Err(status) = read(
                    plane,
                    bus,
                    address,
                    position as u16,
                    &mut self.copybuf[..len],
                    flags,
                ) {
                    return -(status.abs() as i64);
                }
                if let Err(status) = plane.copy_to_caller(offset, &self.copybuf[..len]) {
                    return -(status.abs() as i64);
                }
            }
            position += slice;
            offset += slice;
            total += slice as i64;
            remaining -= slice;
        }
        total
    }
}

/// One bus read of at most one slice (`cat24c256_read128`,
/// `cat24c256.c:251-293`).
///
/// The memory address travels as one or two command bytes depending on the
/// no-page flag; the answer lands in the exec block's buffer.
fn read128(
    plane: &mut dyn TransferPlane,
    bus: Endpoint,
    address: u16,
    memaddr: u16,
    out: &mut [u8],
    flags: i32,
) -> Result<(), i32> {
    if out.len() > client::EXEC_BUF_MAX || out.is_empty() {
        return Err(-minix_types::EINVAL);
    }
    let mut block = ExecBlock::zeroed();
    block.op = I2C_OP_READ_WITH_STOP;
    block.address = address;
    let width = address_width(flags & BDEV_NOPAGE == BDEV_NOPAGE);
    write_memory_address(&mut block.cmd, memaddr, width);
    block.cmd_len = width as u64;
    block.buf_len = out.len() as u64;
    client::exec(plane, bus, &mut block)?;
    out.copy_from_slice(&block.buf[..out.len()]);
    Ok(())
}

/// Read `buf.len()` bytes in read slices (`cat24c256_read`,
/// `cat24c256.c:296-319`).
fn read(
    plane: &mut dyn TransferPlane,
    bus: Endpoint,
    address: u16,
    memaddr: u16,
    buf: &mut [u8],
    flags: i32,
) -> Result<(), i32> {
    let mut at = 0usize;
    for slice in crate::pages::split_read(buf.len() as u64) {
        let len = slice as usize;
        read128(
            plane,
            bus,
            address,
            memaddr.wrapping_add(at as u16),
            &mut buf[at..at + len],
            flags,
        )?;
        at += len;
    }
    Ok(())
}

/// One bus write of at most one slice (`cat24c256_write16`,
/// `cat24c256.c:322-364`).
///
/// The memory address leads the data buffer (one or two bytes), so the
/// transaction carries `len + width` bytes.
fn write16(
    plane: &mut dyn TransferPlane,
    bus: Endpoint,
    address: u16,
    memaddr: u16,
    data: &[u8],
    flags: i32,
) -> Result<(), i32> {
    let width = address_width(flags & BDEV_NOPAGE == BDEV_NOPAGE);
    if data.len() + width > EXEC_BUF_MAX {
        return Err(-minix_types::EINVAL);
    }
    let mut block = ExecBlock::zeroed();
    block.op = I2C_OP_WRITE_WITH_STOP;
    block.address = address;
    block.cmd_len = 0;
    write_memory_address(&mut block.buf, memaddr, width);
    block.buf[width..width + data.len()].copy_from_slice(data);
    block.buf_len = (width + data.len()) as u64;
    client::exec(plane, bus, &mut block)
}

/// Write `buf` in write slices (`cat24c256_write`, `cat24c256.c:367-390`).
fn write(
    plane: &mut dyn TransferPlane,
    bus: Endpoint,
    address: u16,
    memaddr: u16,
    buf: &[u8],
    flags: i32,
) -> Result<(), i32> {
    let mut at = 0usize;
    for slice in crate::pages::split_write(buf.len() as u64) {
        let len = slice as usize;
        write16(
            plane,
            bus,
            address,
            memaddr.wrapping_add(at as u16),
            &buf[at..at + len],
            flags,
        )?;
        at += len;
    }
    Ok(())
}

/// Lay the memory address into the command or data buffer
/// (`cat24c256.c:269-275`, `:342-350`): two bytes normally, the low byte
/// only when the page address is already selected.
fn write_memory_address(buffer: &mut [u8], memaddr: u16, width: usize) {
    if width == 1 {
        buffer[0] = (memaddr & 0xff) as u8;
    } else {
        buffer[0] = ((memaddr >> 8) & 0xff) as u8;
        buffer[1] = (memaddr & 0xff) as u8;
    }
}

impl BlockDriver for Cat24c256Face {
    fn driver_type(&self) -> BlockDriverType {
        // C: `BLOCKDRIVER_TYPE_OTHER` (`cat24c256.c:39`) — the EEPROM is
        // not a disk and never answers partition requests.
        BlockDriverType::Other
    }

    fn open(&mut self, minor: DeviceMinor, _access: i32) -> i32 {
        // C `cat24c256_blk_open` (`cat24c256.c:216-240`).
        if minor.0 as usize >= NR_DEVS {
            return -minix_types::ENXIO;
        }
        self.openct[minor.0 as usize] += 1;
        OK
    }

    fn close(&mut self, minor: DeviceMinor) -> i32 {
        // C `cat24c256_blk_close` (`cat24c256.c:243-248`): a close on a
        // device nobody opened is invalid.
        if minor.0 as usize >= NR_DEVS || self.openct[minor.0 as usize] < 1 {
            return -minix_types::EINVAL;
        }
        self.openct[minor.0 as usize] -= 1;
        OK
    }

    fn transfer(
        &mut self,
        plane: &mut dyn TransferPlane,
        minor: DeviceMinor,
        do_write: bool,
        position: u64,
        count: u64,
        flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
    ) -> i64 {
        if minor.0 as usize >= NR_DEVS {
            return -(minix_types::ENXIO as i64);
        }
        self.transfer_bytes(plane, do_write, position, count, flags)
    }

    fn transfer_vectored(
        &mut self,
        _plane: &mut dyn TransferPlane,
        _minor: DeviceMinor,
        _do_write: bool,
        _position: u64,
        _elements: &[minix_blockdriver::protocol::BlockIovec],
        _flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
    ) -> i64 {
        // The device leg serves the contiguous forms only; the pump now
        // decodes the vector (`E-BLK-VEC` closed), so the refusal here is
        // the device's own, not a framework gap.
        -(minix_types::ENOSYS as i64)
    }

    fn ioctl(&mut self, _minor: DeviceMinor, _request: u64, _grant: u64, _user: i64) -> i32 {
        // C's `cat24c256_blk_ioctl` always answers "inappropriate control"
        // (`cat24c256.c:41`).
        ENOTTY
    }

    fn partition(&mut self, minor: DeviceMinor) -> Option<DeviceExtent> {
        // C `cat24c256_blk_part` (`cat24c256.c:203-214`).
        if minor.0 as usize >= NR_DEVS {
            None
        } else {
            Some(self.extent())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_blockdriver::protocol::RequestId;
    use minix_driver_rt::plane::{GrantIpc, TransferPlane};
    use minix_types::Message;

    /// A plane that records the copies and serves the caller's bytes from
    /// a scratch buffer, and whose bus transactions are scripted.
    struct FakePlane {
        from_bytes: Vec<u8>,
        to_bytes: Vec<u8>,
        copies: Vec<(u64, usize)>,
        /// Scripted bus replies: the exec's message type (0 = success).
        bus_reply: i32,
        /// Every exec block handed to the bus, in order.
        execs: Vec<ExecBlock>,
        granted: usize,
    }

    impl FakePlane {
        fn new() -> Self {
            FakePlane {
                from_bytes: Vec::new(),
                to_bytes: Vec::new(),
                copies: Vec::new(),
                bus_reply: 0,
                execs: Vec::new(),
                granted: 0,
            }
        }
    }

    impl GrantIpc for FakePlane {
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            self.granted += 1;
            // The bus leg grants the exec block; the fake records the block
            // through the address it was handed.
            // SAFETY(test): the address is the caller's live ExecBlock.
            let block = unsafe { &*(address as *const ExecBlock) };
            self.execs.push(*block);
            Ok(3)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, msg: &mut Message) -> Result<(), i32> {
            msg.m_type = self.bus_reply;
            Ok(())
        }
    }

    impl TransferPlane for FakePlane {
        fn caller(&self) -> Endpoint {
            Endpoint(20)
        }
        fn copy_from_caller(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), i32> {
            self.copies.push((offset, buf.len()));
            buf.copy_from_slice(&self.from_bytes[..buf.len()]);
            Ok(())
        }
        fn copy_to_caller(&mut self, offset: u64, buf: &[u8]) -> Result<(), i32> {
            self.copies.push((offset, buf.len()));
            self.to_bytes.extend_from_slice(buf);
            Ok(())
        }
    }

    fn face() -> Cat24c256Face {
        let mut face = Cat24c256Face::new(1, 0x50);
        face.set_bus_endpoint(Some(Endpoint(6)));
        face
    }

    #[test]
    fn test_address_table_matches_c() {
        assert_eq!(VALID_ADDRESSES.len(), 9);
        assert!(valid_address(0x50));
        assert!(valid_address(0x57));
        assert!(!valid_address(0x58));
        assert!(!valid_address(0x00));
        assert_eq!(NR_DEVS, 1);
        assert_eq!(EEPROM_DEV, 0);
        assert_eq!(COPYBUF_SIZE, 0x1000);
        assert_eq!(BDEV_NOPAGE, 0x02);
    }

    #[test]
    fn test_read_slices_at_128_and_sets_the_two_address_bytes() {
        let mut plane = FakePlane::new();
        let mut out = [0u8; 300];
        // Three slices: 128 + 128 + 44.
        assert_eq!(
            read(&mut plane, Endpoint(6), 0x50, 0x0100, &mut out, 0),
            Ok(())
        );
        assert_eq!(plane.execs.len(), 3);
        assert_eq!(plane.execs[0].op, I2C_OP_READ_WITH_STOP);
        assert_eq!(plane.execs[0].address, 0x50);
        assert_eq!(plane.execs[0].cmd_len, 2);
        assert_eq!(&plane.execs[0].cmd[..2], &[0x01, 0x00]);
        assert_eq!(plane.execs[0].buf_len, 128);
        assert_eq!(plane.execs[1].buf_len, 128);
        assert_eq!(&plane.execs[1].cmd[..2], &[0x01, 0x80]);
        assert_eq!(plane.execs[2].buf_len, 44);
        assert_eq!(&plane.execs[2].cmd[..2], &[0x02, 0x00]);
    }

    #[test]
    fn test_no_page_flag_sends_one_address_byte() {
        let mut plane = FakePlane::new();
        let mut out = [0u8; 16];
        assert_eq!(
            read(&mut plane, Endpoint(6), 0x50, 0x0140, &mut out, BDEV_NOPAGE),
            Ok(())
        );
        assert_eq!(plane.execs.len(), 1);
        assert_eq!(plane.execs[0].cmd_len, 1);
        assert_eq!(plane.execs[0].cmd[0], 0x40);
    }

    #[test]
    fn test_write_slices_at_16_and_leads_with_the_address() {
        let mut plane = FakePlane::new();
        let data: Vec<u8> = (0..20).collect();
        assert_eq!(
            write(&mut plane, Endpoint(6), 0x50, 0x0200, &data, 0),
            Ok(())
        );
        assert_eq!(plane.execs.len(), 2);
        assert_eq!(plane.execs[0].op, I2C_OP_WRITE_WITH_STOP);
        assert_eq!(plane.execs[0].cmd_len, 0);
        assert_eq!(
            plane.execs[0].buf_len, 18,
            "16 data bytes + 2 address bytes"
        );
        assert_eq!(&plane.execs[0].buf[..2], &[0x02, 0x00]);
        assert_eq!(&plane.execs[0].buf[2..18], &data[..16]);
        assert_eq!(plane.execs[1].buf_len, 6, "4 data bytes + 2 address bytes");
        assert_eq!(&plane.execs[1].buf[..2], &[0x02, 0x10]);
    }

    #[test]
    fn test_read_transfer_copies_out_and_clips_at_the_device_end() {
        let mut face = face();
        let mut plane = FakePlane::new();
        let moved = BlockDriver::transfer(
            &mut face,
            &mut plane,
            DeviceMinor(0),
            false,
            0,
            64,
            0,
            RequestId(1),
        );
        assert_eq!(moved, 64);
        assert_eq!(plane.copies, vec![(0, 64)]);
        assert_eq!(plane.to_bytes.len(), 64);

        // The last bytes of the device: a request that runs past the end
        // is clipped.
        let mut plane = FakePlane::new();
        let moved = BlockDriver::transfer(
            &mut face,
            &mut plane,
            DeviceMinor(0),
            false,
            DEVICE_SIZE - 10,
            64,
            0,
            RequestId(2),
        );
        assert_eq!(moved, 10);
        assert_eq!(plane.copies, vec![(0, 10)]);

        // Entirely past the end answers zero and touches nothing.
        let mut plane = FakePlane::new();
        let moved = BlockDriver::transfer(
            &mut face,
            &mut plane,
            DeviceMinor(0),
            false,
            DEVICE_SIZE,
            64,
            0,
            RequestId(3),
        );
        assert_eq!(moved, 0);
        assert!(plane.copies.is_empty());
    }

    #[test]
    fn test_write_transfer_pulls_from_the_caller_and_clips_the_staging_buffer() {
        let mut face = face();
        let mut plane = FakePlane::new();
        plane.from_bytes = (0..COPYBUF_SIZE + 8).map(|i| i as u8).collect();
        // More than one staging buffer's worth: the loop slices.
        let moved = BlockDriver::transfer(
            &mut face,
            &mut plane,
            DeviceMinor(0),
            true,
            0,
            COPYBUF_SIZE as u64 + 8,
            0,
            RequestId(4),
        );
        assert_eq!(moved, COPYBUF_SIZE as i64 + 8);
        assert_eq!(
            plane.copies,
            vec![(0, COPYBUF_SIZE), (COPYBUF_SIZE as u64, 8)]
        );
        // The bus saw one write per 16-byte slice, plus the final 8.
        assert_eq!(plane.execs.len(), COPYBUF_SIZE / 16 + 1);
    }

    #[test]
    fn test_transfer_refusals_and_the_other_hook_defaults() {
        let mut face = face();
        let mut plane = FakePlane::new();
        // A minor nobody serves.
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                &mut plane,
                DeviceMinor(1),
                false,
                0,
                8,
                0,
                RequestId(5)
            ),
            -(minix_types::ENXIO as i64)
        );
        // The vectored pair is refused by the device leg itself.
        assert_eq!(
            BlockDriver::transfer_vectored(
                &mut face,
                &mut plane,
                DeviceMinor(0),
                false,
                0,
                &[],
                0,
                RequestId(6)
            ),
            -(minix_types::ENOSYS as i64)
        );
        assert_eq!(face.driver_type(), BlockDriverType::Other);
        assert_eq!(face.ioctl(DeviceMinor(0), 0, 0, 0), ENOTTY);
        assert_eq!(face.partition(DeviceMinor(0)).unwrap().size, DEVICE_SIZE);
        assert!(face.partition(DeviceMinor(1)).is_none());
    }

    #[test]
    fn test_open_close_counters() {
        let mut face = face();
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.close(DeviceMinor(0)), OK);
        assert_eq!(face.close(DeviceMinor(0)), OK);
        assert_eq!(face.close(DeviceMinor(0)), -minix_types::EINVAL);
        assert_eq!(face.open(DeviceMinor(1), 0), -minix_types::ENXIO);
    }

    #[test]
    fn test_a_bus_failure_comes_back_as_a_negative_status() {
        let mut face = face();
        let mut plane = FakePlane::new();
        plane.bus_reply = -minix_types::EIO;
        let moved = BlockDriver::transfer(
            &mut face,
            &mut plane,
            DeviceMinor(0),
            false,
            0,
            8,
            0,
            RequestId(7),
        );
        assert_eq!(moved, -(minix_types::EIO as i64));
        assert!(
            plane.to_bytes.is_empty(),
            "nothing is copied out on failure"
        );
    }

    #[test]
    fn test_an_unresolved_bus_refuses_the_transfer() {
        let mut face = Cat24c256Face::new(1, 0x50);
        let mut plane = FakePlane::new();
        let moved = BlockDriver::transfer(
            &mut face,
            &mut plane,
            DeviceMinor(0),
            false,
            0,
            8,
            0,
            RequestId(8),
        );
        assert_eq!(moved, -(minix_types::ENOENT as i64));
    }
}
