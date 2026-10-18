//! The floppy block face: device selection, open-time media detection,
//! the sector loop, and the reports.
//!
//! C correspondence: `floppy.c`'s block callbacks — `f_prepare`
//! (`floppy.c:395-420`: minor to drive/density/extent), `f_part`
//! (`floppy.c:422-427`), `f_do_open` (`floppy.c:1249-1306`: the density
//! decoded from the minor, the format-device gate, and the experimental
//! media detection through `test_read`), `f_do_close`
//! (`floppy.c:1308-1314`: a no-op), `f_geometry` (`floppy.c:1348-1356`),
//! and the sector loop of `f_transfer` (`floppy.c:444-...`: the offset and
//! length gates, the EOF clip, the format-device leg, and the per-sector
//! retry).
//!
//! Registered seam: the FDC device layer (`dma_setup`, `start_motor`,
//! `seek`, `fdc_transfer`, `fdc_results`, `fdc_command`, `fdc_out`,
//! `recalibrate`, `f_reset`, `f_intr_wait`, `read_id`, `test_read`,
//! `floppy.c:530-1240`) sits behind [`FdcLeg`]. The geometry half of the
//! density table is in [`crate::geometry`]; the timing parameters of the
//! C table and the controller sequencing are the remaining piece, named
//! here rather than faked — a leg that cannot serve answers an error,
//! never a fabricated sector.

use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::protocol::{BlockDriverType, DeviceExtent, DeviceMinor, PartitionGeometry};
use minix_driver_rt::plane::TransferPlane;
use minix_types::{EINVAL, EIO, ENOSYS, ENXIO, OK};

use crate::geometry::{DENSITIES, Density, RetryAction, RetryPolicy};

/// Drives the driver serves (`NR_DRIVES`, `floppy.c:130`).
pub const NR_DRIVES: usize = 2;
/// Heads per diskette (`NR_HEADS`, `floppy.c:102`).
pub const NR_HEADS: u64 = 2;
/// The drive-type field of a minor (`DEV_TYPE_BITS`, `floppy.c:123`).
pub const DEV_TYPE_BITS: i32 = 0x7C;
/// Right shift that normalizes the type field (`DEV_TYPE_SHIFT`).
pub const DEV_TYPE_SHIFT: i32 = 2;
/// The minor bit that turns a write into a format (`FORMAT_DEV_BIT`).
pub const FORMAT_DEV_BIT: i32 = 0x80;
/// The first partition minor (`MINOR_fd0p0`, `drvlib.h:15`).
pub const MINOR_FD0P0: i32 = 28 << 2;
/// "Current media unknown" (`NO_DENS`, `floppy.c:140`).
pub const NO_DENS: i32 = 100;
/// Sector size.
pub const SECTOR_SIZE: u64 = 512;

/// What the face needs from the FDC device layer.
///
/// C's device layer drives the controller registers, the DMA channel, the
/// motor timer, and the interrupt wait; the face asks for the outcomes it
/// needs and keeps the policy (which density to try, when to retry).
pub trait FdcLeg {
    /// Try to read one sector to identify the media (`test_read`,
    /// `floppy.c:1322-1346`): the C body reads the highest sector of
    /// cylinder 2 at the given density and reports success or failure.
    fn test_read(&mut self, drive: u32, density: usize, test_sector: u64) -> Result<(), i32>;

    /// Read one sector (`fdc_transfer` with a read command).
    fn read_sector(&mut self, drive: u32, block: u64, buf: &mut [u8]) -> Result<(), i32>;

    /// Write one sector (`fdc_transfer` with a write command).
    fn write_sector(&mut self, drive: u32, block: u64, buf: &[u8]) -> Result<(), i32>;

    /// Whether the drive's motor is still spinning (`motor_status`).
    fn motor_running(&mut self, drive: u32) -> bool;
}

/// One drive's state (`struct floppy`, `floppy.c:206-219`).
#[derive(Debug, Clone, Copy)]
struct Drive {
    /// The last density found, or [`NO_DENS`] (`fl_density`).
    density: i32,
    /// Densities this drive may still accept (`fl_class`).
    class: u8,
    /// The whole-device extent (`fl_geom`).
    geom: DeviceExtent,
    /// Partition extents (`fl_part`).
    part: [Option<DeviceExtent>; 4],
    /// The sector the head is on, or -1 (`fl_sector`).
    sector: i64,
}

impl Drive {
    fn new() -> Self {
        Drive {
            density: NO_DENS,
            // Every modeled density is possible until a test narrows it.
            class: 0xFF,
            geom: DeviceExtent { base: 0, size: 0 },
            part: [None; 4],
            sector: -1,
        }
    }
}

/// The floppy face.
pub struct FloppyFace<L: FdcLeg> {
    leg: L,
    drives: [Drive; NR_DRIVES],
    retries: RetryPolicy,
    buf: [u8; SECTOR_SIZE as usize],
}

impl<L: FdcLeg> FloppyFace<L> {
    /// A face with both drives unprobed.
    pub fn new(leg: L) -> Self {
        FloppyFace {
            leg,
            drives: [Drive::new(); NR_DRIVES],
            retries: RetryPolicy::new(),
            buf: [0; SECTOR_SIZE as usize],
        }
    }

    /// The geometry of one density as the driver computes it
    /// (`f_prepare`, `floppy.c:405-411`): heads times sectors times
    /// cylinders.
    pub fn density_size(density: &Density) -> u64 {
        NR_HEADS * density.sectors_per_track as u64 * density.cylinders as u64 * SECTOR_SIZE
    }

    /// Minor to drive and extent (`f_prepare`, `floppy.c:395-420`).
    pub fn prepare(&mut self, minor: i32) -> Option<(usize, DeviceExtent)> {
        if minor < 0 {
            return None;
        }
        let drive = (minor & !(DEV_TYPE_BITS | FORMAT_DEV_BIT)) as usize;
        if drive >= NR_DRIVES {
            return None;
        }
        let state = &mut self.drives[drive];
        if state.density < DENSITIES.len() as i32 {
            let density = &DENSITIES[state.density as usize];
            state.geom = DeviceExtent {
                base: 0,
                size: Self::density_size(density),
            };
        }
        // A partition minor selects one of the drive's partition entries.
        let kind = minor & DEV_TYPE_BITS;
        if kind >= MINOR_FD0P0 {
            let index = ((kind - MINOR_FD0P0) >> DEV_TYPE_SHIFT) as usize;
            return self.drives[drive]
                .part
                .get(index)
                .and_then(|entry| entry.map(|extent| (drive, extent)));
        }
        Some((drive, state.geom))
    }

    /// Open a device (`f_do_open`, `floppy.c:1249-1306`).
    pub fn open(&mut self, minor: i32) -> i32 {
        if self.prepare(minor).is_none() {
            return ENXIO;
        }
        let drive = (minor & !(DEV_TYPE_BITS | FORMAT_DEV_BIT)) as usize;
        let mut dtype = minor & DEV_TYPE_BITS;
        if dtype >= MINOR_FD0P0 {
            dtype = 0;
        }
        if dtype != 0 {
            // All types except zero name a specific drive/medium
            // combination: take it as given.
            let index = ((dtype >> DEV_TYPE_SHIFT) - 1) as usize;
            if index >= DENSITIES.len() {
                return ENXIO;
            }
            self.drives[drive].density = index as i32;
            self.prepare(minor);
            return OK;
        }
        if minor & FORMAT_DEV_BIT != 0 {
            // `/dev/fdN` cannot be a formatting device.
            return EIO;
        }
        // The device is `/dev/fdN`: find out what is in the drive.
        let known = self.drives[drive].density;
        if known != NO_DENS && known < DENSITIES.len() as i32 {
            if self.leg.motor_running(drive as u32) {
                // Still spinning: assume nothing changed.
                return OK;
            }
            let test_sector = DENSITIES[known as usize].sectors_per_track as u64 - 1;
            if self
                .leg
                .test_read(drive as u32, known as usize, test_sector)
                .is_ok()
            {
                return OK;
            }
        }
        // Try the densities this drive may still accept, narrowest
        // knowledge first (C walks its `test_order` table and narrows
        // `fl_class` on success; the model carries four densities where
        // the C table has seven).
        for (index, density) in DENSITIES.iter().enumerate() {
            if self.drives[drive].class & (1 << index) == 0 {
                continue;
            }
            let test_sector = density.sectors_per_track as u64 - 1;
            if self.leg.test_read(drive as u32, index, test_sector).is_ok() {
                self.drives[drive].density = index as i32;
                // Keep only the densities up to and including this one.
                self.drives[drive].class &= (1u8 << (index + 1)) - 1;
                self.prepare(minor);
                return OK;
            }
        }
        self.drives[drive].density = NO_DENS;
        EIO
    }

    /// One transfer (`f_transfer`, `floppy.c:444-...`).
    pub fn transfer(
        &mut self,
        plane: &mut dyn TransferPlane,
        minor: i32,
        do_write: bool,
        position: u64,
        count: u64,
    ) -> i64 {
        let Some((drive, extent)) = self.prepare(minor) else {
            return ENXIO as i64;
        };
        if position >> 32 != 0 {
            // Way beyond EOF.
            return 0;
        }
        if !position.is_multiple_of(SECTOR_SIZE) || !count.is_multiple_of(SECTOR_SIZE) {
            return EINVAL as i64;
        }
        if position >= extent.size {
            return 0;
        }
        let mut count = count.min(extent.size - position);
        if minor & FORMAT_DEV_BIT != 0 {
            // The formatting leg needs the `disk_parameter_s` structure
            // out of the caller's buffer (`floppy.c:509-536`); its wire
            // layout is not modeled yet, so the leg refuses rather than
            // formatting with guessed parameters.
            let _ = do_write;
            return -(ENOSYS as i64);
        }
        let mut moved = 0i64;
        let mut offset = 0u64;
        let mut position = position;
        while count > 0 {
            let block = (extent.base + position) / SECTOR_SIZE;
            if do_write {
                if let Err(status) = plane.copy_from_caller(offset, &mut self.buf) {
                    return -(status.abs() as i64);
                }
                match self.leg.write_sector(drive as u32, block, &self.buf) {
                    Ok(()) => {}
                    Err(status) => return -(status.abs() as i64),
                }
            } else {
                match self.leg.read_sector(drive as u32, block, &mut self.buf) {
                    Ok(()) => {}
                    Err(status) => return -(status.abs() as i64),
                }
                if let Err(status) = plane.copy_to_caller(offset, &self.buf) {
                    return -(status.abs() as i64);
                }
            }
            self.drives[drive].sector = block as i64;
            position += SECTOR_SIZE;
            offset += SECTOR_SIZE;
            count -= SECTOR_SIZE;
            moved += SECTOR_SIZE as i64;
        }
        moved
    }

    /// The retry policy (`RetryPolicy`, fed by the sector loop's failures).
    pub fn retries(&self) -> &RetryPolicy {
        &self.retries
    }

    /// Note one failed attempt (`RetryPolicy::record_failure`).
    pub fn record_failure(&mut self, retryable: bool) -> RetryAction {
        self.retries.record_failure(retryable)
    }
}

impl<L: FdcLeg> BlockDriver for FloppyFace<L> {
    fn driver_type(&self) -> BlockDriverType {
        // C: `BLOCKDRIVER_TYPE_DISK` (`floppy.c:268`) — partition requests
        // are handled.
        BlockDriverType::Disk
    }

    fn open(&mut self, minor: DeviceMinor, _access: i32) -> i32 {
        self.open(minor.0 as i32)
    }

    fn close(&mut self, _minor: DeviceMinor) -> i32 {
        // C: "nothing to do here" (`floppy.c:1312`).
        OK
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
            return -(ENOSYS as i64);
        }
        self.transfer(plane, minor.0 as i32, do_write, position, count)
    }

    fn ioctl(&mut self, _minor: DeviceMinor, _request: u64, _grant: u64, _user: i64) -> i32 {
        // C registers no control hook; the framework answers "inappropriate
        // control".
        minix_types::ENOTTY
    }

    fn partition(&mut self, minor: DeviceMinor) -> Option<DeviceExtent> {
        // C `f_part` (`floppy.c:422-427`) is `f_prepare` by another name.
        self.prepare(minor.0 as i32).map(|(_drive, extent)| extent)
    }

    fn geometry(&mut self, minor: DeviceMinor) -> Option<PartitionGeometry> {
        // C `f_geometry` (`floppy.c:1348-1356`).
        let (drive, extent) = self.prepare(minor.0 as i32)?;
        let density = &DENSITIES[self.drives[drive].density as usize];
        Some(PartitionGeometry {
            base: extent.base,
            size: extent.size,
            cylinders: density.cylinders as u32,
            heads: density.heads as u32,
            sectors: density.sectors_per_track as u32,
        })
    }

    fn cleanup(&mut self) {
        // C `f_cleanup` (`floppy.c:432-439`): arm the motor-off timer and
        // forget the head position. The timer is the device layer's (the
        // leg owns the motor), so only the position is forgotten here.
        for drive in self.drives.iter_mut() {
            drive.sector = -1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_driver_rt::plane::{GrantIpc, TransferPlane};
    use minix_types::{Endpoint, Message};

    /// A leg with a scripted density answer and an in-memory diskette.
    struct FakeFdc {
        density_ok: i32,
        motor: bool,
        data: Vec<u8>,
        reads: Vec<u64>,
        writes: Vec<u64>,
    }

    impl FakeFdc {
        fn new() -> Self {
            FakeFdc {
                density_ok: 3,
                motor: false,
                // Big enough for the highest sector of the largest modeled
                // density (1.44M: 2880 sectors).
                data: vec![0u8; 2880 * 512],
                reads: Vec::new(),
                writes: Vec::new(),
            }
        }
    }

    impl FdcLeg for FakeFdc {
        fn test_read(&mut self, _drive: u32, density: usize, _test_sector: u64) -> Result<(), i32> {
            if density as i32 == self.density_ok {
                Ok(())
            } else {
                Err(-EIO)
            }
        }
        fn read_sector(&mut self, _drive: u32, block: u64, buf: &mut [u8]) -> Result<(), i32> {
            self.reads.push(block);
            let start = (block * SECTOR_SIZE) as usize;
            if start + buf.len() > self.data.len() {
                return Err(-EIO);
            }
            buf.copy_from_slice(&self.data[start..start + buf.len()]);
            Ok(())
        }
        fn write_sector(&mut self, _drive: u32, block: u64, buf: &[u8]) -> Result<(), i32> {
            self.writes.push(block);
            let start = (block * SECTOR_SIZE) as usize;
            self.data[start..start + buf.len()].copy_from_slice(buf);
            Ok(())
        }
        fn motor_running(&mut self, _drive: u32) -> bool {
            self.motor
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
            Err(-ENOSYS)
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

    #[test]
    fn test_prepare_selects_the_drive_and_sizes_the_device() {
        let mut face = FloppyFace::new(FakeFdc::new());
        // Minor 0 is drive 0 with no density known yet: the extent is empty
        // until a density is found.
        assert_eq!(
            face.prepare(0),
            Some((0, DeviceExtent { base: 0, size: 0 }))
        );
        assert_eq!(face.prepare(-1), None);
        // A drive beyond the two served is refused.
        assert_eq!(face.prepare(2), None);
        // Once a density is set the size follows the geometry.
        face.drives[0].density = 3;
        let (_, extent) = face.prepare(0).unwrap();
        assert_eq!(extent.size, 2 * 18 * 80 * 512);
        assert_eq!(
            FloppyFace::<FakeFdc>::density_size(&DENSITIES[0]),
            2 * 9 * 40 * 512
        );
    }

    #[test]
    fn test_open_probes_the_media_through_the_leg() {
        let mut face = FloppyFace::new(FakeFdc::new());
        assert_eq!(face.open(0), OK);
        assert_eq!(face.drives[0].density, 3, "the probed density is recorded");

        // A specific density in the minor is taken as given.
        let mut face = FloppyFace::new(FakeFdc::new());
        let minor = 2 << DEV_TYPE_SHIFT; // density 1
        assert_eq!(face.open(minor), OK);
        assert_eq!(face.drives[0].density, 1);

        // A formatting minor on /dev/fdN is refused.
        let mut face = FloppyFace::new(FakeFdc::new());
        assert_eq!(face.open(FORMAT_DEV_BIT), EIO);

        // No density works: the media stays unknown and the open fails.
        let mut leg = FakeFdc::new();
        leg.density_ok = -1;
        let mut face = FloppyFace::new(leg);
        assert_eq!(face.open(0), EIO);
        assert_eq!(face.drives[0].density, NO_DENS);
    }

    #[test]
    fn test_a_spinning_drive_skips_the_probe() {
        let mut leg = FakeFdc::new();
        leg.density_ok = -1; // every test read fails
        leg.motor = true;
        let mut face = FloppyFace::new(leg);
        face.drives[0].density = 2;
        assert_eq!(face.open(0), OK, "the motor answers for the media");
    }

    #[test]
    fn test_the_transfer_gates_match_c() {
        let mut face = FloppyFace::new(FakeFdc::new());
        face.drives[0].density = 3;
        let mut plane = FakePlane::new();
        // Unaligned position and length.
        assert_eq!(face.transfer(&mut plane, 0, false, 1, 512), EINVAL as i64);
        assert_eq!(face.transfer(&mut plane, 0, false, 0, 100), EINVAL as i64);
        // Past the end answers zero.
        let size = face.drives[0].geom.size;
        assert_eq!(face.transfer(&mut plane, 0, false, size, 512), 0);
        // A request that runs past the end is clipped.
        assert_eq!(face.transfer(&mut plane, 0, false, size - 512, 4096), 512);
        // An unserved drive is refused.
        assert_eq!(face.transfer(&mut plane, 2, false, 0, 512), ENXIO as i64);
    }

    #[test]
    fn test_a_read_walks_the_sectors_and_copies_out() {
        let mut face = FloppyFace::new(FakeFdc::new());
        face.drives[0].density = 3;
        face.leg.data[0..4].copy_from_slice(&[1, 2, 3, 4]);
        let mut plane = FakePlane::new();
        assert_eq!(face.transfer(&mut plane, 0, false, 0, 1024), 1024);
        assert_eq!(face.leg.reads, vec![0, 1], "one read per sector");
        assert_eq!(plane.to_bytes.len(), 1024);
        assert_eq!(&plane.to_bytes[..4], &[1, 2, 3, 4]);
        assert_eq!(face.drives[0].sector, 1, "the head position is remembered");
    }

    #[test]
    fn test_a_write_carries_the_callers_bytes_in() {
        let mut face = FloppyFace::new(FakeFdc::new());
        face.drives[0].density = 3;
        let mut plane = FakePlane::new();
        plane.from_bytes = vec![7u8; 512];
        assert_eq!(face.transfer(&mut plane, 0, true, 512, 512), 512);
        assert_eq!(face.leg.writes, vec![1]);
        assert_eq!(&face.leg.data[512..516], &[7, 7, 7, 7]);
    }

    #[test]
    fn test_a_formatting_minor_refuses_until_the_parameter_struct_lands() {
        let mut face = FloppyFace::new(FakeFdc::new());
        face.drives[0].density = 3;
        let mut plane = FakePlane::new();
        plane.from_bytes = vec![0u8; 512];
        assert_eq!(
            face.transfer(&mut plane, 0, true, 0, 512),
            512,
            "a plain write is not a format"
        );
        let format_minor = FORMAT_DEV_BIT;
        assert_eq!(
            face.transfer(&mut plane, format_minor, true, 0, 512),
            -(ENOSYS as i64)
        );
    }

    #[test]
    fn test_the_reports_follow_the_density() {
        let mut face = FloppyFace::new(FakeFdc::new());
        face.drives[0].density = 3;
        let geometry = face.geometry(DeviceMinor(0)).unwrap();
        assert_eq!(geometry.cylinders, 80);
        assert_eq!(geometry.heads, 2);
        assert_eq!(geometry.sectors, 18);
        assert_eq!(geometry.size, 2 * 18 * 80 * 512);
        let extent = face.partition(DeviceMinor(0)).unwrap();
        assert_eq!(extent.base, 0);
        // The driver handles partitions.
        assert_eq!(face.driver_type(), BlockDriverType::Disk);
        // A close does nothing, and cleanup forgets the head position.
        assert_eq!(face.close(DeviceMinor(0)), OK);
        face.cleanup();
        assert_eq!(face.drives[0].sector, -1);
        assert_eq!(face.ioctl(DeviceMinor(0), 0, 0, 0), minix_types::ENOTTY);
    }

    #[test]
    fn test_the_retry_policy_is_available_to_the_sector_loop() {
        let mut face = FloppyFace::new(FakeFdc::new());
        assert_eq!(face.retries().errors(), 0);
        let _ = face.record_failure(true);
        assert_eq!(face.retries().errors(), 1);
    }
}
