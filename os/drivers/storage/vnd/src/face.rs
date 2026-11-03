//! The VND block face: file-backed transfers, partitions, and geometry.
//!
//! C correspondence: `vnd.c`'s block callbacks — `vnd_open`
//! (`vnd.c:66-105`: the unconfigured/short-device gates, the read-only
//! gate, and the re-parse of the partition table on the first open),
//! `vnd_close` (`vnd.c:110-124`), the transfer loop (`vnd_transfer`,
//! `vnd.c:212-303`: clip to the device extent, move at most one
//! `VND_BUF_SIZE` chunk per step through the intermediate buffer, copy the
//! caller's bytes through the data plane, and flush on force-write), and
//! the partition/geometry reports (`vnd_part`/`vnd_geometry`,
//! `vnd.c:513-535`).
//!
//! The configure and unconfigure bodies (`VNDIOCSET`/`VNDIOCCLR`) also live
//! here as [`VndFace::configure`]/[`VndFace::clear`]: the service decodes
//! the ioctl structure out of the caller's grant (the ioctl hook carries no
//! data plane yet) and calls them.

use minix_blockdriver::driver::BlockDriver;
use minix_blockdriver::partition::{MbrEntry, NR_PARTITIONS, parse_primary_table, primary_extents};
use minix_blockdriver::protocol::{BlockDriverType, DeviceExtent, DeviceMinor, PartitionGeometry};
use minix_driver_rt::plane::TransferPlane;
use minix_types::{BDEV_FORCEWRITE, EACCES, EBUSY, EINVAL, EIO, ENXIO, OK};

use crate::backend::{FileBackend, FileInfo};
use crate::layout::{CHUNK_SIZE, Geometry, SECTOR_SIZE, derive_geometry};
use crate::protocol::{VNDIOF_FORCE, VNDIOF_HASGEOM, VNDIOF_READONLY, VndIoctl};

/// Devices one drive exposes (`DEV_PER_DRIVE`, `drvlib.h:10`).
pub const DEV_PER_DRIVE: usize = 1 + NR_PARTITIONS;
/// Subpartitions per drive (`SUB_PER_DRIVE`, `drvlib.h:11`).
pub const SUB_PER_DRIVE: usize = NR_PARTITIONS * NR_PARTITIONS;
/// The minor of the first subpartition (`MINOR_d0p0s0`, `drvlib.h:14`).
pub const MINOR_D0P0S0: i32 = 128;

/// The VND block face: one unit, backed by one file.
pub struct VndFace<B: FileBackend> {
    /// The unit number reported to `VNDIOCGET` (C's `instance`).
    instance: u32,
    /// The backing-file operations (the seam).
    backend: B,
    /// The backing file's descriptor, `None` while unconfigured (C's
    /// `state.fd`, `-1` when unconfigured).
    fd: Option<i32>,
    /// Open count (C's `state.openct`).
    openct: i32,
    /// Exit once the last user closes (C's `state.exiting`, set by the
    /// termination signal handler).
    exiting: bool,
    /// Configured read-only (C's `state.rdonly`).
    rdonly: bool,
    /// The backing file's device and inode (C's `state.dev`/`state.ino`).
    dev: u64,
    ino: u64,
    /// Primary-partition extents, index zero being the whole device
    /// (C's `state.part`).
    part: [Option<DeviceExtent>; DEV_PER_DRIVE],
    /// Subpartition extents (C's `state.subpart`).
    subpart: [Option<DeviceExtent>; SUB_PER_DRIVE],
    /// The geometry the unit reports (C's `state.geom`).
    geom: Geometry,
    /// The intermediate transfer buffer (C's `state.buf`, `VND_BUF_SIZE`).
    buf: [u8; CHUNK_SIZE as usize],
}

impl<B: FileBackend> VndFace<B> {
    /// A fresh face for one unit (C `vnd_init`, `vnd.c:540-565`).
    pub fn new(instance: u32, backend: B) -> Self {
        VndFace {
            instance,
            backend,
            fd: None,
            openct: 0,
            exiting: false,
            rdonly: false,
            dev: 0,
            ino: 0,
            part: [None; DEV_PER_DRIVE],
            subpart: [None; SUB_PER_DRIVE],
            geom: derive_geometry(0),
            buf: [0; CHUNK_SIZE as usize],
        }
    }

    /// The unit number.
    pub fn instance(&self) -> u32 {
        self.instance
    }

    /// Whether a backing file is configured.
    pub fn configured(&self) -> bool {
        self.fd.is_some()
    }

    /// The open count (the `DIOCOPENCT` answer).
    pub fn open_count(&self) -> i32 {
        self.openct
    }

    /// The unit/device/inode triple (the `VNDIOCGET` answer;
    /// `vnd.c:470-490`: the device and inode stay zero while the unit is
    /// unconfigured).
    pub fn unit_info(&self) -> (u32, u64, u64) {
        (self.instance, self.dev, self.ino)
    }

    /// Configure the unit from a decoded `VNDIOCSET` request
    /// (`vnd.c:369-436`), returning the device size for the reply.
    ///
    /// `user_endpoint` is the process whose descriptor is stolen; the
    /// descriptor itself comes through the backend's `copyfd` leg.
    pub fn configure(&mut self, user_endpoint: i32, vnd: &VndIoctl) -> Result<u64, i32> {
        // The VND must not be busy: the caller holds the one open.
        if self.fd.is_some() || self.openct != 1 {
            return Err(EBUSY);
        }
        let fd = self.backend.copyfd(user_endpoint, vnd.fildes)?;
        // From here on a failure must release the descriptor again.
        let info: FileInfo = match self.backend.fstat(fd) {
            Ok(info) => info,
            Err(status) => {
                let _ = self.backend.close(fd);
                return Err(status);
            }
        };
        if !info.regular {
            let _ = self.backend.close(fd);
            return Err(EINVAL);
        }
        self.fd = Some(fd);
        self.dev = info.dev;
        self.ino = info.ino;
        self.rdonly = vnd.flags & VNDIOF_READONLY != 0;
        match self.layout(info.size, vnd) {
            Ok(size) => Ok(size),
            Err(status) => {
                self.unconfigure();
                Err(status)
            }
        }
    }

    /// Unconfigure the unit (`VNDIOCCLR`, `vnd.c:438-458`).
    ///
    /// The caller has the device open to issue the request; other users
    /// keep it busy unless `VNDIOF_FORCE` is set.
    pub fn clear(&mut self, flags: i32) -> Result<(), i32> {
        if self.fd.is_none() {
            return Err(ENXIO);
        }
        if flags & VNDIOF_FORCE == 0 && self.openct != 1 {
            return Err(EBUSY);
        }
        self.unconfigure();
        Ok(())
    }

    /// Flush the backing file (`DIOCFLUSH`, `vnd.c:500-506`).
    pub fn flush(&mut self) -> Result<(), i32> {
        let Some(fd) = self.fd else {
            return Err(ENXIO);
        };
        self.backend.fsync(fd)
    }

    /// The termination signal's effect (`vnd_signal`, `vnd.c:570-585`):
    /// stop once the device is fully closed.
    ///
    /// C calls `blockdriver_terminate` when the count is already zero; the
    /// rewrite records the intent and the binary's loop decides, so this
    /// method only reports whether the process may stop now.
    pub fn note_termination(&mut self) -> bool {
        self.exiting = true;
        self.openct == 0
    }

    /// Release the backing file and reset the device state
    /// (`vnd.c:445-452` and the failure path of `VNDIOCSET`).
    fn unconfigure(&mut self) {
        if let Some(fd) = self.fd.take() {
            let _ = self.backend.close(fd);
        }
        self.dev = 0;
        self.ino = 0;
        self.part = [None; DEV_PER_DRIVE];
        self.subpart = [None; SUB_PER_DRIVE];
        self.geom = derive_geometry(0);
    }

    /// Size and geometry for the unit (`vnd_layout`, `vnd.c:311-353`).
    fn layout(&mut self, size: u64, vnd: &VndIoctl) -> Result<u64, i32> {
        if vnd.flags & VNDIOF_HASGEOM != 0 {
            // The geometry determines the accessible part of the file, and
            // the resulting size must not exceed the file size.
            let sectors = vnd.geom.secsize as u64
                * vnd.geom.nsectors as u64
                * vnd.geom.ntracks as u64
                * vnd.geom.ncylinders as u64;
            let wanted = sectors * vnd.geom.secsize as u64;
            if vnd.geom.secsize == 0 || wanted > size {
                return Err(EINVAL);
            }
            self.geom = Geometry {
                cylinders: vnd.geom.ncylinders,
                heads: vnd.geom.ntracks,
                sectors_per_track: vnd.geom.nsectors,
            };
            self.set_extents(wanted);
            Ok(wanted)
        } else {
            let sectors = size / SECTOR_SIZE;
            self.geom = derive_geometry(sectors);
            self.set_extents(sectors * SECTOR_SIZE);
            Ok(sectors * SECTOR_SIZE)
        }
    }

    /// Install the whole-device extent and parse the partition table
    /// (`vnd_partition` + `drvlib.c`'s `partition`, `vnd.c:60-63`).
    fn set_extents(&mut self, size: u64) {
        let sectors = size / SECTOR_SIZE;
        let table = self.read_partition_table();
        self.part = primary_extents(&table, sectors);
        // Subpartitions are not parsed (C's `partition()` only fills the
        // primary table for this driver's `P_PRIMARY` call); the slots stay
        // empty, so a subpartition minor answers ENXIO.
        self.subpart = [None; SUB_PER_DRIVE];
    }

    /// Read and parse the boot sector for the partition table
    /// (`drvlib.c`'s `get_part_table` reading through the driver's own
    /// transfer).
    fn read_partition_table(&mut self) -> [Option<MbrEntry>; NR_PARTITIONS] {
        let Some(fd) = self.fd else {
            return [None; NR_PARTITIONS];
        };
        let mut mbr = [0u8; 512];
        match self.backend.pread(fd, 0, &mut mbr) {
            Ok(read) if read == mbr.len() => {
                parse_primary_table(&mbr).unwrap_or([None; NR_PARTITIONS])
            }
            _ => [None; NR_PARTITIONS],
        }
    }

    /// The extent of one minor (`vnd_part`, `vnd.c:513-525`).
    fn extent(&self, minor: u32) -> Option<DeviceExtent> {
        let index = minor as i32;
        if (0..DEV_PER_DRIVE as i32).contains(&index) {
            self.part[index as usize]
        } else {
            let sub = index - MINOR_D0P0S0;
            if (0..SUB_PER_DRIVE as i32).contains(&sub) {
                self.subpart[sub as usize]
            } else {
                None
            }
        }
    }
}

impl<B: FileBackend> BlockDriver for VndFace<B> {
    fn driver_type(&self) -> BlockDriverType {
        // C: `BLOCKDRIVER_TYPE_DISK` (`vnd.c:38`) — the VND handles
        // partition requests.
        BlockDriverType::Disk
    }

    fn open(&mut self, minor: DeviceMinor, access: i32) -> i32 {
        // C `vnd_open` (`vnd.c:66-105`).
        if self.fd.is_none() && minor.0 != 0 {
            return ENXIO;
        }
        if self.fd.is_some() && self.extent(minor.0).is_none() {
            return ENXIO;
        }
        if access & BDEV_W_BIT != 0 {
            if self.fd.is_none() {
                return ENXIO;
            }
            if self.rdonly {
                return EACCES;
            }
        }
        // A device opened after having been fully closed re-parses its
        // partition table.
        if self.fd.is_some() && self.openct == 0 {
            let size = self.part[0].map(|extent| extent.size).unwrap_or(0);
            self.set_extents(size);
            if self.extent(minor.0).is_none() {
                return ENXIO;
            }
        }
        self.openct += 1;
        OK
    }

    fn close(&mut self, _minor: DeviceMinor) -> i32 {
        // C `vnd_close` (`vnd.c:110-124`).
        if self.openct == 0 {
            return EINVAL;
        }
        self.openct -= 1;
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
        let Some(fd) = self.fd else {
            return -(ENXIO as i64);
        };
        let Some(extent) = self.extent(minor.0) else {
            return -(ENXIO as i64);
        };
        if do_write && self.rdonly {
            return -(EACCES as i64);
        }
        if position >= extent.size {
            return 0;
        }
        let mut position = position + extent.base;
        let mut offset = 0u64;
        let mut total = 0i64;
        let mut remaining = count.min(extent.size - (position - extent.base));
        while remaining > 0 {
            let mut chunk = remaining.min(CHUNK_SIZE);
            if do_write {
                let len = chunk as usize;
                if let Err(status) = plane.copy_from_caller(offset, &mut self.buf[..len]) {
                    return -(status.abs() as i64);
                }
                let written = match self.backend.pwrite(fd, position, &self.buf[..len]) {
                    Ok(written) if written > 0 => written as u64,
                    Ok(_) => return total.max(EIO as i64),
                    Err(status) => return -(status.abs() as i64),
                };
                chunk = written;
            } else {
                let read = match self
                    .backend
                    .pread(fd, position, &mut self.buf[..chunk as usize])
                {
                    Ok(read) => read as u64,
                    Err(status) => return -(status.abs() as i64),
                };
                if read == 0 {
                    // End of file: stop, as C's `pread` returning zero does.
                    break;
                }
                chunk = read;
                let len = chunk as usize;
                if let Err(status) = plane.copy_to_caller(offset, &self.buf[..len]) {
                    return -(status.abs() as i64);
                }
            }
            position += chunk;
            offset += chunk;
            total += chunk as i64;
            remaining -= chunk;
        }
        if do_write && flags & BDEV_FORCEWRITE != 0 {
            // A failed flush is reported: the data did not reach the medium.
            if let Err(status) = self.backend.fsync(fd) {
                return -(status.abs() as i64);
            }
        }
        total
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
        // The file-backed leg serves the contiguous forms only.
        -(minix_types::ENOSYS as i64)
    }

    fn ioctl(&mut self, _minor: DeviceMinor, _request: u64, _grant: u64, _user: i64) -> i32 {
        // The control hook carries the grant but no data plane, so the
        // service decodes the vnd ioctls out of the caller's grant and
        // calls the face directly; anything reaching here is a control the
        // driver does not serve (C's fall-through answer).
        minix_types::ENOTTY
    }

    fn partition(&mut self, minor: DeviceMinor) -> Option<DeviceExtent> {
        self.extent(minor.0)
    }

    fn geometry(&mut self, minor: DeviceMinor) -> Option<PartitionGeometry> {
        // C `vnd_geometry` (`vnd.c:530-535`) fills the three geometry
        // words; the base and size come from the partition report.
        let extent = self
            .extent(minor.0)
            .unwrap_or(DeviceExtent { base: 0, size: 0 });
        Some(PartitionGeometry {
            base: extent.base,
            size: extent.size,
            cylinders: self.geom.cylinders,
            heads: self.geom.heads,
            sectors: self.geom.sectors_per_track,
        })
    }
}

/// Open for writing (`BDEV_W_BIT`, `com.h`).
pub const BDEV_W_BIT: i32 = 0x02;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use minix_blockdriver::protocol::RequestId;
    use minix_driver_rt::plane::{GrantIpc, NullPlane, TransferPlane};
    use minix_types::Message;

    /// A file in memory: the backend seam with scripted contents.
    struct FakeFile {
        data: Vec<u8>,
        copyfd_result: Result<i32, i32>,
        closes: Vec<i32>,
        fsyncs: Vec<i32>,
        /// Whether the file reports itself as regular.
        regular: bool,
    }

    impl FakeFile {
        fn new(len: usize) -> Self {
            FakeFile {
                data: vec![0u8; len],
                copyfd_result: Ok(7),
                closes: Vec::new(),
                fsyncs: Vec::new(),
                regular: true,
            }
        }
    }

    impl FileBackend for FakeFile {
        fn copyfd(&mut self, _user_endpoint: i32, _fd: i32) -> Result<i32, i32> {
            self.copyfd_result
        }
        fn pread(&mut self, _fd: i32, offset: u64, buf: &mut [u8]) -> Result<usize, i32> {
            let start = offset as usize;
            if start >= self.data.len() {
                return Ok(0);
            }
            let len = buf.len().min(self.data.len() - start);
            buf[..len].copy_from_slice(&self.data[start..start + len]);
            Ok(len)
        }
        fn pwrite(&mut self, _fd: i32, offset: u64, buf: &[u8]) -> Result<usize, i32> {
            let start = offset as usize;
            if start + buf.len() > self.data.len() {
                return Err(-minix_types::ENOSPC);
            }
            self.data[start..start + buf.len()].copy_from_slice(buf);
            Ok(buf.len())
        }
        fn fsync(&mut self, fd: i32) -> Result<(), i32> {
            self.fsyncs.push(fd);
            Ok(())
        }
        fn fstat(&mut self, _fd: i32) -> Result<FileInfo, i32> {
            Ok(FileInfo {
                size: self.data.len() as u64,
                dev: 262,
                ino: 3,
                regular: self.regular,
            })
        }
        fn close(&mut self, fd: i32) -> Result<(), i32> {
            self.closes.push(fd);
            Ok(())
        }
    }

    /// A plane over a scratch caller buffer.
    struct FakePlane {
        from_bytes: Vec<u8>,
        to_bytes: Vec<u8>,
        copies: Vec<(u64, usize)>,
    }

    impl FakePlane {
        fn new() -> Self {
            FakePlane {
                from_bytes: Vec::new(),
                to_bytes: Vec::new(),
                copies: Vec::new(),
            }
        }
    }

    impl GrantIpc for FakePlane {
        fn grant_direct(
            &mut self,
            _dest: minix_types::Endpoint,
            _address: u64,
            _len: usize,
            _write: bool,
        ) -> Result<i32, i32> {
            Err(-minix_types::ENOSYS)
        }
        fn grant_revoke(&mut self, _grant: i32) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
        fn send_rec(&mut self, _dst: minix_types::Endpoint, _msg: &mut Message) -> Result<(), i32> {
            Err(-minix_types::ENOSYS)
        }
    }

    impl TransferPlane for FakePlane {
        fn caller(&self) -> minix_types::Endpoint {
            minix_types::Endpoint(20)
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

    /// A configured face over a file of `len` bytes.
    fn configured(len: usize) -> (VndFace<FakeFile>, FakePlane) {
        let mut face = VndFace::new(0, FakeFile::new(len));
        // The caller holds the device open, as vnconfig does.
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        let request = VndIoctl {
            fildes: 3,
            flags: 0,
            geom: Default::default(),
            size: 0,
        };
        assert_eq!(face.configure(7, &request), Ok(len as u64));
        (face, FakePlane::new())
    }

    #[test]
    fn test_configure_steals_stats_and_sizes_the_device() {
        let mut face = VndFace::new(4, FakeFile::new(8192));
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert!(!face.configured());
        let request = VndIoctl {
            fildes: 3,
            flags: 0,
            geom: Default::default(),
            size: 0,
        };
        assert_eq!(face.configure(9, &request), Ok(8192));
        assert!(face.configured());
        assert_eq!(face.unit_info(), (4, 262, 3));
        // Geometry derived from 16 sectors: below one fallback cylinder,
        // so the single-head/single-sector layout applies (`vnd.c:336-342`).
        let geom = face.geometry(DeviceMinor(0)).unwrap();
        assert_eq!(geom.heads, 1);
        assert_eq!(geom.sectors, 1);
        assert_eq!(geom.cylinders, 16);
    }

    #[test]
    fn test_configure_refusals() {
        // A busy unit: the caller must hold exactly one open.
        let mut face = VndFace::new(0, FakeFile::new(512));
        let request = VndIoctl::default();
        assert_eq!(
            face.configure(7, &request),
            Err(EBUSY),
            "nobody has it open"
        );
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.configure(7, &request), Err(EBUSY), "two opens");

        // A descriptor that cannot be stolen, and a non-regular file: the
        // descriptor is released again.
        let mut file = FakeFile::new(512);
        file.copyfd_result = Err(-minix_types::EPERM);
        let mut face = VndFace::new(0, file);
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.configure(7, &request), Err(-minix_types::EPERM));

        let mut file = FakeFile::new(512);
        file.regular = false;
        let mut face = VndFace::new(0, file);
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.configure(7, &request), Err(EINVAL));
        assert_eq!(face.open_count(), 1);
    }

    #[test]
    fn test_clear_and_flush() {
        let (mut face, _plane) = configured(1024);
        assert_eq!(face.clear(0), Ok(()));
        assert!(!face.configured());
        // Clearing again, and flushing an unconfigured unit, answer ENXIO.
        assert_eq!(face.clear(0), Err(ENXIO));
        assert_eq!(face.flush(), Err(ENXIO));

        // Two opens keep the unit busy unless the force flag is set.
        let (mut face, _plane) = configured(1024);
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.clear(0), Err(EBUSY));
        assert_eq!(face.clear(VNDIOF_FORCE), Ok(()));
    }

    #[test]
    fn test_transfer_reads_and_writes_through_the_plane() {
        let (mut face, mut plane) = configured(4096);
        for (i, byte) in face.buf.iter_mut().enumerate() {
            *byte = i as u8;
        }
        // Fill the file with a known pattern through the backend.
        let pattern: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        face.backend.data[..].copy_from_slice(&pattern);

        // A read of 200 bytes at offset 100.
        let moved = BlockDriver::transfer(
            &mut face,
            &mut plane,
            DeviceMinor(0),
            false,
            100,
            200,
            0,
            RequestId(1),
        );
        assert_eq!(moved, 200);
        assert_eq!(plane.to_bytes.len(), 200);
        assert_eq!(plane.to_bytes[..8], pattern[100..108]);

        // A write of 64 bytes lands in the file.
        plane.from_bytes = vec![0xAB; 64];
        let moved = BlockDriver::transfer(
            &mut face,
            &mut plane,
            DeviceMinor(0),
            true,
            50,
            64,
            0,
            RequestId(2),
        );
        assert_eq!(moved, 64);
        assert!(face.backend.data[50..114].iter().all(|byte| *byte == 0xAB));
    }

    #[test]
    fn test_transfer_clips_and_refuses() {
        let (mut face, mut plane) = configured(1024);
        // Past the end answers zero.
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                &mut plane,
                DeviceMinor(0),
                false,
                1024,
                64,
                0,
                RequestId(1),
            ),
            0
        );
        // A read that runs past the end stops at the end of the file.
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                &mut plane,
                DeviceMinor(0),
                false,
                1000,
                64,
                0,
                RequestId(2),
            ),
            24
        );
        // An unserved minor is refused; the vectored pair is refused by the
        // device leg itself.
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                &mut plane,
                DeviceMinor(9),
                false,
                0,
                8,
                0,
                RequestId(3),
            ),
            -(ENXIO as i64)
        );
        assert_eq!(
            BlockDriver::transfer_vectored(
                &mut face,
                &mut plane,
                DeviceMinor(0),
                false,
                0,
                &[],
                0,
                RequestId(4),
            ),
            -(minix_types::ENOSYS as i64)
        );
        // A write with force-write flushes (the backend records it).
        plane.from_bytes = vec![1; 8];
        assert_eq!(
            BlockDriver::transfer(
                &mut face,
                &mut plane,
                DeviceMinor(0),
                true,
                0,
                8,
                BDEV_FORCEWRITE,
                RequestId(5),
            ),
            8
        );
        assert_eq!(face.backend.fsyncs, vec![7]);
    }

    #[test]
    fn test_read_only_gate_and_open_close_counters() {
        let mut face = VndFace::new(0, FakeFile::new(512));
        let request = VndIoctl {
            fildes: 3,
            flags: VNDIOF_READONLY,
            geom: Default::default(),
            size: 0,
        };
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.configure(7, &request), Ok(512));
        // A write open on a read-only unit is refused.
        assert_eq!(face.open(DeviceMinor(0), BDEV_W_BIT), EACCES);
        assert_eq!(face.close(DeviceMinor(0)), OK);
        assert_eq!(face.close(DeviceMinor(0)), EINVAL);
        assert_eq!(face.open_count(), 0);
        // Closing the last user and re-opening re-parses partitions.
        assert_eq!(face.open(DeviceMinor(0), 0), OK);
        assert_eq!(face.open_count(), 1);
    }

    #[test]
    fn test_a_file_without_a_table_serves_the_whole_device() {
        let (mut face, _plane) = configured(2048);
        let extent = face.partition(DeviceMinor(0)).unwrap();
        assert_eq!(extent.base, 0);
        assert_eq!(extent.size, 2048);
        // Partitions beyond the table are unset, and a subpartition minor
        // answers nothing.
        assert!(face.partition(DeviceMinor(1)).is_none());
        assert!(face.partition(DeviceMinor(MINOR_D0P0S0 as u32)).is_none());
        // The default hook keeps a hook-less transfer honest.
        let mut null = NullPlane;
        assert_eq!(
            VndFace::<FakeFile>::new(0, FakeFile::new(8)).transfer(
                &mut null,
                DeviceMinor(0),
                false,
                0,
                8,
                0,
                RequestId(6),
            ),
            -(ENXIO as i64)
        );
    }

    #[test]
    fn test_note_termination_waits_for_the_last_close() {
        let (mut face, _plane) = configured(512);
        assert!(!face.note_termination());
        assert_eq!(face.close(DeviceMinor(0)), OK);
        assert!(face.note_termination());
    }
}
