//! The SD/MMC block face: slot selection, card bring-up, the block loop,
//! and the reports.
//!
//! C correspondence: `mmcblk.c`'s block callbacks — `block_open`
//! (`mmcblk.c:141-207`: slot lookup, card detection, the open-count fast
//! path, the card bring-up, and the partition parse), `block_close`
//! (`mmcblk.c:213-...`), `block_transfer` (`mmcblk.c:280-433`: the block
//! size and alignment gates, the EOF clip, and the one-block-at-a-time
//! loop), `block_ioctl` (`mmcblk.c:439-470`: the open count and the
//! no-op flush), `block_part` (`mmcblk.c:477-520`: the 5-devices-per-drive
//! minor scheme with 16 subpartitions), and `bdr_alarm`
//! (`mmcblk.c:62-...`: no alarm processing).
//!
//! Registered seam: the host/card layer (`mmchost_mmchs.c` and `emmc.c`:
//! the controller registers, the command/response engine, the card
//! initialization sequence, and the data transfers) sits behind
//! [`MmcHost`]. The command-sequence model is in [`crate::commands`]; the
//! controller traffic is the remaining piece, named here rather than
//! faked — a host that cannot serve answers an error, never a fabricated
//! block.

use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::partition::{NR_PARTITIONS, parse_primary_table, primary_extents};
use minix_blockdriver::protocol::{BlockDriverType, DeviceExtent, DeviceMinor, PartitionGeometry};
use minix_driver_rt::plane::TransferPlane;
use minix_types::{EINVAL, EIO, ENXIO, OK};

use crate::commands::NEGOTIATED_BLOCK_SIZE;

/// The driver's staging buffer (`COPYBUFF_SIZE`, `mmcblk.c:39`).
pub const COPYBUFF_SIZE: usize = 0x1000;
/// Devices one drive exposes (direct plus four partitions, `mmcblk.c:481-484`).
pub const DEV_PER_DRIVE: i32 = 5;
/// Subpartition minors start here (`128 + 16` reserved, `mmcblk.c:514`).
pub const SUBPART_BASE: i32 = 128;
/// Subpartitions modeled per drive (`mmcblk.c:514`).
pub const SUB_PER_DRIVE: usize = 16;
/// Slots the driver serves (the C comment's "8 drives").
pub const NR_SLOTS: usize = 8;

/// What the face needs from the host/card layer.
///
/// C's host object carries `card_detect`, `card_initialize`, `read`, and
/// `write` (`mmcblk.c:161`, `:171`, `:406`, `:390`); the face keeps the
/// block policy and asks the host for the outcomes.
pub trait MmcHost {
    /// Whether a card sits in the slot (`card_detect`).
    fn card_detect(&mut self, slot: usize) -> bool;

    /// Bring the card up (`card_initialize`): the C sequence probes the
    /// card, negotiates the block size, and fills the card's geometry and
    /// partition tables.
    fn card_initialize(&mut self, slot: usize) -> bool;

    /// The negotiated block size for the card in `slot` (`card.blk_size`).
    fn block_size(&mut self, slot: usize) -> u32;

    /// Read `count` blocks starting at `block` into `buf` (`host->read`).
    fn read_blocks(
        &mut self,
        slot: usize,
        block: u64,
        count: u32,
        buf: &mut [u8],
    ) -> Result<(), i32>;

    /// Write `count` blocks starting at `block` from `buf` (`host->write`).
    fn write_blocks(&mut self, slot: usize, block: u64, count: u32, buf: &[u8]) -> Result<(), i32>;
}

/// One slot's card state (`struct sd_card` as `mmcblk.c` uses it).
#[derive(Debug, Clone, Copy)]
struct Card {
    /// The negotiated block size, zero until the card is up.
    blk_size: u32,
    /// Open count (`open_ct`).
    open_ct: i32,
    /// Whether the card is in data-transfer mode.
    data_mode: bool,
    /// The whole device and its partitions (`part[5]`).
    part: [Option<DeviceExtent>; DEV_PER_DRIVE as usize],
    /// The subpartitions (`subpart[16]`).
    subpart: [Option<DeviceExtent>; SUB_PER_DRIVE],
}

impl Card {
    fn new() -> Self {
        Card {
            blk_size: 0,
            open_ct: 0,
            data_mode: false,
            part: [None; DEV_PER_DRIVE as usize],
            subpart: [None; SUB_PER_DRIVE],
        }
    }
}

/// The SD/MMC block face.
pub struct MmcFace<H: MmcHost> {
    host: H,
    cards: [Card; NR_SLOTS],
    buf: [u8; COPYBUFF_SIZE],
}

impl<H: MmcHost> MmcFace<H> {
    /// A face with every slot empty.
    pub fn new(host: H) -> Self {
        MmcFace {
            host,
            cards: [Card::new(); NR_SLOTS],
            buf: [0; COPYBUFF_SIZE],
        }
    }

    /// The slot a minor belongs to (`get_slot`, `mmcblk.c:42`): the
    /// major/minor scheme puts five devices on each of eight drives.
    pub fn slot_of(minor: i32) -> Option<usize> {
        if minor < 0 {
            return None;
        }
        let slot = (minor / DEV_PER_DRIVE) as usize;
        if slot < NR_SLOTS { Some(slot) } else { None }
    }

    /// The extent of one minor (`block_part`, `mmcblk.c:477-520`).
    pub fn extent(&mut self, minor: i32) -> Option<DeviceExtent> {
        let slot = Self::slot_of(minor)?;
        if !self.host.card_detect(slot) {
            return None;
        }
        let card = &self.cards[slot];
        if minor < DEV_PER_DRIVE {
            return card.part[minor as usize];
        }
        if (SUBPART_BASE..SUBPART_BASE + SUB_PER_DRIVE as i32).contains(&minor) {
            return card.subpart[(minor - SUBPART_BASE) as usize];
        }
        None
    }

    /// The card's block size (zero until the card is up).
    pub fn block_size(&self, slot: usize) -> u32 {
        self.cards[slot].blk_size
    }

    /// The slot's open count (`card.open_ct`, the `DIOCOPENCT` answer).
    pub fn open_count(&self, slot: usize) -> i32 {
        self.cards[slot].open_ct
    }

    /// Open a device (`block_open`, `mmcblk.c:141-207`).
    pub fn open(&mut self, minor: i32) -> i32 {
        let Some(slot) = Self::slot_of(minor) else {
            return EIO;
        };
        if !self.host.card_detect(slot) {
            // No card inserted.
            return EIO;
        }
        if self.cards[slot].data_mode {
            // Already up: count the open.
            self.cards[slot].open_ct += 1;
            return OK;
        }
        // First open: probe the card.
        if !self.host.card_initialize(slot) {
            return EIO;
        }
        self.cards[slot].blk_size = self.host.block_size(slot);
        self.cards[slot].data_mode = true;
        self.parse_partitions(slot);
        self.cards[slot].open_ct += 1;
        OK
    }

    /// Close a device (`block_close`, `mmcblk.c:213-...`).
    pub fn close(&mut self, minor: i32) -> i32 {
        let Some(slot) = Self::slot_of(minor) else {
            return EIO;
        };
        if self.cards[slot].open_ct == 0 {
            return EINVAL;
        }
        self.cards[slot].open_ct -= 1;
        OK
    }

    /// Parse the card's partition table (`partition(..., P_PRIMARY, 0)`,
    /// `mmcblk.c:175-177`): the MBR is read through the host and sliced by
    /// the shared partition helper.
    fn parse_partitions(&mut self, slot: usize) {
        let blk_size = self.cards[slot].blk_size;
        let mut mbr = [0u8; 512];
        let read = if blk_size == 0 || blk_size > COPYBUFF_SIZE as u32 {
            false
        } else {
            self.host
                .read_blocks(slot, 0, 1, &mut mbr[..blk_size as usize])
                .is_ok()
        };
        let table = if read {
            parse_primary_table(&mbr).unwrap_or([None; NR_PARTITIONS])
        } else {
            [None; NR_PARTITIONS]
        };
        // The whole-device extent comes from the card's size; without a
        // card size the partitions stay unset (the host owns the geometry).
        let total_sectors = self.cards[slot]
            .part
            .first()
            .and_then(|entry| *entry)
            .map(|extent| extent.size / 512)
            .unwrap_or(0);
        let extents = primary_extents(&table, total_sectors);
        for (index, extent) in extents.iter().enumerate() {
            self.cards[slot].part[index] = *extent;
        }
        self.cards[slot].subpart = [None; SUB_PER_DRIVE];
    }

    /// Install the card size the host reported (the whole-device extent).
    pub fn set_card_size(&mut self, slot: usize, size: u64) {
        self.cards[slot].part[0] = Some(DeviceExtent { base: 0, size });
    }

    /// One transfer (`block_transfer`, `mmcblk.c:280-433`).
    pub fn transfer(
        &mut self,
        plane: &mut dyn TransferPlane,
        minor: i32,
        do_write: bool,
        position: u64,
        count: u64,
    ) -> i64 {
        let Some(dev) = self.extent(minor) else {
            return ENXIO as i64;
        };
        let Some(slot) = Self::slot_of(minor) else {
            return ENXIO as i64;
        };
        let blk_size = self.cards[slot].blk_size;
        if blk_size == 0 || blk_size as usize > COPYBUFF_SIZE {
            return EINVAL as i64;
        }
        if !position.is_multiple_of(blk_size as u64) {
            return EINVAL as i64;
        }
        if position >= dev.size {
            return 0;
        }
        if !count.is_multiple_of(blk_size as u64) || count == 0 {
            return EINVAL as i64;
        }
        let mut count = count.min(dev.size - position);
        let mut moved = 0u64;
        let mut offset = 0u64;
        let mut position = position;
        // One block at a time (`mmcblk.c:388-428`).
        while count > 0 {
            let block = (dev.base + position) / blk_size as u64;
            let len = blk_size as usize;
            if do_write {
                if let Err(status) = plane.copy_from_caller(offset, &mut self.buf[..len]) {
                    return -(status.abs() as i64);
                }
                if let Err(status) = self.host.write_blocks(slot, block, 1, &self.buf[..len]) {
                    return -(status.abs() as i64);
                }
            } else {
                if let Err(status) = self.host.read_blocks(slot, block, 1, &mut self.buf[..len]) {
                    return -(status.abs() as i64);
                }
                if let Err(status) = plane.copy_to_caller(offset, &self.buf[..len]) {
                    return -(status.abs() as i64);
                }
            }
            position += blk_size as u64;
            offset += blk_size as u64;
            count -= blk_size as u64;
            moved += blk_size as u64;
        }
        moved as i64
    }
}

impl<H: MmcHost> BlockDriver for MmcFace<H> {
    fn driver_type(&self) -> BlockDriverType {
        // The C table sets no type; the driver serves one card and its
        // partitions through the disk minor scheme.
        BlockDriverType::Disk
    }

    fn open(&mut self, minor: DeviceMinor, _access: i32) -> i32 {
        self.open(minor.0 as i32)
    }

    fn close(&mut self, minor: DeviceMinor) -> i32 {
        self.close(minor.0 as i32)
    }

    fn transfer(
        &mut self,
        plane: &mut dyn TransferPlane,
        minor: DeviceMinor,
        do_write: bool,
        position: u64,
        count: u64,
        _flags: i32,
        _id: minix_blockdriver::protocol::RequestId,
        vectored: bool,
    ) -> i64 {
        if vectored {
            return -(minix_types::ENOSYS as i64);
        }
        self.transfer(plane, minor.0 as i32, do_write, position, count)
    }

    fn ioctl(&mut self, _minor: DeviceMinor, request: u64, _grant: u64, _user: i64) -> i32 {
        // The open-count control is decoded in the service (the control
        // hook has no data plane); the flush is a no-op, and anything else
        // is "inappropriate control" (`mmcblk.c:455-470`).
        match request {
            DIOCOPENCT => EINVAL,
            DIOCFLUSH => OK,
            _ => minix_types::ENOTTY,
        }
    }

    fn partition(&mut self, minor: DeviceMinor) -> Option<DeviceExtent> {
        self.extent(minor.0 as i32)
    }

    fn geometry(&mut self, minor: DeviceMinor) -> Option<PartitionGeometry> {
        // The C table registers no geometry hook; the extent is the
        // answer the framework can report.
        let extent = self.extent(minor.0 as i32)?;
        Some(PartitionGeometry {
            base: extent.base,
            size: extent.size,
            cylinders: 0,
            heads: 0,
            sectors: 0,
        })
    }

    fn alarm(&mut self, _stamp: i64) {
        // `bdr_alarm` does nothing (`mmcblk.c:62-...`).
    }
}

/// `DIOCOPENCT`: the open count (`ioc_disk.h`).
pub const DIOCOPENCT: u64 = 0x4004_6407;
/// `DIOCFLUSH`: flush the device (a no-op here).
pub const DIOCFLUSH: u64 = 0x0000_6408;
/// The negotiated block size the driver expects (`NEGOTIATED_BLOCK_SIZE`).
pub const BLOCK_SIZE: u32 = NEGOTIATED_BLOCK_SIZE;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_driver_rt::plane::{GrantIpc, TransferPlane};
    use minix_types::{Endpoint, Message};

    /// A host with a card that is present and up.
    struct FakeHost {
        present: bool,
        init_ok: bool,
        blk_size: u32,
        data: Vec<u8>,
    }

    impl FakeHost {
        fn new() -> Self {
            FakeHost {
                present: true,
                init_ok: true,
                blk_size: 512,
                data: vec![0u8; 64 * 512],
            }
        }
    }

    impl MmcHost for FakeHost {
        fn card_detect(&mut self, _slot: usize) -> bool {
            self.present
        }
        fn card_initialize(&mut self, _slot: usize) -> bool {
            self.init_ok
        }
        fn block_size(&mut self, _slot: usize) -> u32 {
            self.blk_size
        }
        fn read_blocks(
            &mut self,
            _slot: usize,
            block: u64,
            count: u32,
            buf: &mut [u8],
        ) -> Result<(), i32> {
            let start = (block * self.blk_size as u64) as usize;
            let len = count as usize * self.blk_size as usize;
            if start + len > self.data.len() {
                return Err(-EIO);
            }
            buf[..len].copy_from_slice(&self.data[start..start + len]);
            Ok(())
        }
        fn write_blocks(
            &mut self,
            _slot: usize,
            block: u64,
            count: u32,
            buf: &[u8],
        ) -> Result<(), i32> {
            let start = (block * self.blk_size as u64) as usize;
            let len = count as usize * self.blk_size as usize;
            self.data[start..start + len].copy_from_slice(&buf[..len]);
            Ok(())
        }
    }

    struct FakePlane {
        from_bytes: Vec<u8>,
        to_bytes: Vec<u8>,
    }

    impl FakePlane {
        fn new() -> Self {
            FakePlane {
                from_bytes: Vec::new(),
                to_bytes: Vec::new(),
            }
        }
    }

    impl GrantIpc for FakePlane {
        fn grant_direct(
            &mut self,
            _dest: Endpoint,
            _address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            Ok(5)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Ok(())
        }
        fn send_rec(&mut self, _dst: Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
    }

    impl TransferPlane for FakePlane {
        fn caller(&self) -> Endpoint {
            Endpoint(20)
        }
        fn copy_from_caller(&mut self, _offset: u64, buf: &mut [u8]) -> Result<(), i32> {
            buf.copy_from_slice(&self.from_bytes[..buf.len()]);
            Ok(())
        }
        fn copy_to_caller(&mut self, _offset: u64, buf: &[u8]) -> Result<(), i32> {
            self.to_bytes.extend_from_slice(buf);
            Ok(())
        }
    }

    fn face() -> MmcFace<FakeHost> {
        let mut face = MmcFace::new(FakeHost::new());
        face.set_card_size(0, 64 * 512);
        face
    }

    #[test]
    fn test_the_minor_scheme_maps_minors_to_slots_and_devices() {
        assert_eq!(MmcFace::<FakeHost>::slot_of(0), Some(0));
        assert_eq!(MmcFace::<FakeHost>::slot_of(4), Some(0));
        assert_eq!(MmcFace::<FakeHost>::slot_of(5), Some(1));
        assert_eq!(MmcFace::<FakeHost>::slot_of(40), None);
        assert_eq!(MmcFace::<FakeHost>::slot_of(-1), None);
    }

    #[test]
    fn test_open_brings_the_card_up_and_counts() {
        let mut face = face();
        assert_eq!(face.open(0), OK);
        assert_eq!(face.block_size(0), 512);
        assert_eq!(face.open(0), OK, "the second open just counts");
        assert_eq!(face.close(0), OK);
        assert_eq!(face.close(0), OK);
        assert_eq!(face.close(0), EINVAL, "closing a closed device");

        // No card, and a failing bring-up, are both refused.
        let mut absent = MmcFace::new(FakeHost {
            present: false,
            ..FakeHost::new()
        });
        assert_eq!(absent.open(0), EIO);
        let mut broken = MmcFace::new(FakeHost {
            init_ok: false,
            ..FakeHost::new()
        });
        assert_eq!(broken.open(0), EIO);
    }

    #[test]
    fn test_the_transfer_gates_match_c() {
        let mut face = face();
        face.open(0);
        let mut plane = FakePlane::new();
        // Unaligned position.
        assert_eq!(face.transfer(&mut plane, 0, false, 1, 512), EINVAL as i64);
        // A length that is not a whole number of blocks.
        assert_eq!(face.transfer(&mut plane, 0, false, 0, 100), EINVAL as i64);
        // Past the end answers zero.
        assert_eq!(face.transfer(&mut plane, 0, false, 64 * 512, 512), 0);
        // A request running past the end is clipped.
        assert_eq!(face.transfer(&mut plane, 0, false, 63 * 512, 4096), 512);
        // An unknown minor is refused.
        assert_eq!(face.transfer(&mut plane, 1, false, 0, 512), ENXIO as i64);
    }

    #[test]
    fn test_a_read_walks_blocks_and_copies_out() {
        let mut face = face();
        face.open(0);
        face.host.data[0..4].copy_from_slice(&[9, 8, 7, 6]);
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, 0, false, 0, 1024), 1024);
        assert_eq!(plane.to_bytes.len(), 1024);
        assert_eq!(&plane.to_bytes[..4], &[9, 8, 7, 6]);
    }

    #[test]
    fn test_a_write_carries_the_callers_blocks_in() {
        let mut face = face();
        face.open(0);
        let mut plane = FakePlane::new();
        plane.from_bytes = vec![3u8; 512];
        assert_eq!(face.transfer(&mut plane, 0, true, 512, 512), 512);
        assert_eq!(&face.host.data[512..516], &[3, 3, 3, 3]);
    }

    #[test]
    fn test_the_reports_and_controls() {
        let mut face = face();
        face.open(0);
        let extent = face.partition(DeviceMinor(0)).unwrap();
        assert_eq!(extent.size, 64 * 512);
        assert!(
            face.partition(DeviceMinor(1)).is_none(),
            "no partition table"
        );
        let geometry = face.geometry(DeviceMinor(0)).unwrap();
        assert_eq!(geometry.size, 64 * 512);
        // The flush is a no-op; the open count is the service's control;
        // anything else is refused.
        assert_eq!(face.ioctl(DeviceMinor(0), DIOCFLUSH, 0, 0), OK);
        assert_eq!(
            face.ioctl(DeviceMinor(0), 0x1234, 0, 0),
            minix_types::ENOTTY
        );
        assert_eq!(face.ioctl(DeviceMinor(0), DIOCOPENCT, 0, 0), EINVAL);
        assert_eq!(BLOCK_SIZE, 512);
        assert_eq!(face.driver_type(), BlockDriverType::Disk);
    }
}
